//! The server loop: initialisation, document sync and request dispatch.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
    ShowMessage,
};
use lsp_types::request::{
    ApplyWorkspaceEdit, CodeActionRequest, DocumentSymbolRequest, ExecuteCommand, FoldingRangeRequest, GotoDefinition,
    Request as _, SemanticTokensFullRequest, SemanticTokensRangeRequest, ShowDocument,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    DocumentHighlight, DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams,
    InitializeResult, Location, MessageType, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, ShowDocumentParams, ShowMessageParams,
    TextDocumentEdit, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Uri, WorkspaceEdit,
};
use org_core::agenda::{self, Config as AgendaConfig, Now, Span};
use org_core::archive::{self, AllOutcome, Archived, Settings as ArchiveSettings};
use org_core::capture;
use org_core::clock;
use org_core::date::{Date, DateTime, Day};
use org_core::edit::{self, Call, Command as EditCommand, Settings as EditSettings};
use org_core::open::{self, Open};
use org_core::outline::Outline;
use org_core::planning::{self, Command as PlanningCommand, Prompt};
use org_core::priority;
use org_core::read_date::Settings as ReadDateSettings;
use org_core::refile::{self, OutlinePath, Settings as RefileSettings};
use org_core::todo::{self, Target as TodoTarget};
use org_core::Refusal;
use org_syntax::{Parse, TodoKeywords};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{self, ArchivesMode, CaptureTemplateOption, RefileFiles, RefileTarget};
use crate::document::Document;
use crate::line_index::{Encoding, LineIndex};
use crate::prompt::{self, DatePrompt, Target, TimeField};

mod bulk;
use crate::views::{self, EntryRef, Query, View, ViewLine};
use crate::workspace::{Source, Workspace};
use crate::{actions, cli, commands, completion, custom, folding, highlight, manifest, port, symbols, uri};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// LSP 3.18 methods that `lsp-types` does not know yet.
const TEXT_DOCUMENT_CONTENT: &str = "workspace/textDocumentContent";
const TEXT_DOCUMENT_CONTENT_REFRESH: &str = "workspace/textDocumentContent/refresh";
const LINE_HIGHLIGHTS: &str = "org-lsp/lineHighlights";

/// The commands that set or delete properties.
const PROPERTY_COMMANDS: [&str; 6] = [
    commands::SET_PROPERTY,
    commands::DELETE_PROPERTY,
    commands::DELETE_PROPERTY_GLOBALLY,
    commands::SET_EFFORT,
    commands::INCREASE_EFFORT,
    commands::TOGGLE_ORDERED,
];

/// What the server remembers about the client.
struct Client {
    encoding: Encoding,
    hierarchical_symbols: bool,
    document_changes: bool,
    /// The client fetches documents of the agenda scheme from the server.
    virtual_documents: bool,
    show_document: bool,
    /// The client shows the date prompt document and binds its keys; it
    /// says so with `experimental.orgDatePrompt`.
    date_prompt: bool,
    /// The client shows `org-indent-mode`'s prefixes, which the server
    /// sends (`org-lsp/virtualIndent`); it says so with
    /// `experimental.orgVirtualIndent`.
    virtual_indent: bool,
    /// Highlights lines of the agenda from edge to edge as the server
    /// says (`org-lsp/lineHighlights`): `experimental.orgLineHighlights`,
    /// or the option `line_highlights`.
    line_highlights: bool,
    /// The client rereads semantic tokens when asked.
    tokens_refresh: bool,
    /// The client asks for inlay hints again when told to.
    inlay_refresh: bool,
    /// Ask with `window/showMessageRequest` (`message_prompts`).
    message_prompts: bool,
    /// More commands as code actions, and the agenda's through the
    /// command port (`command_actions`).
    command_actions: bool,
    /// Where the command port's script goes (`command_script`).
    command_script: Option<PathBuf>,
}

/// An outgoing request whose answer the server acts on.
enum Pending {
    /// An edit to a file; the agenda is rebuilt once it is applied.
    Edit,
    /// An edit after which the cursor goes to `position` of `uri`, as
    /// Emacs leaves it after inserting a timestamp.
    EditAndMove { uri: Uri, position: Position },
    /// The cursor of the agenda is being moved to follow an entry.
    Reveal,
    /// The cursor of the date prompt is being moved to the selected day.
    PromptCursor,
    /// A question of `command`, run with `argument`, asked with
    /// `window/showMessageRequest` (`message_prompts`); the answer runs it
    /// again.
    Answer { command: String, argument: Value, input: Value },
}

/// The agenda the client is showing.
struct OpenView {
    uri: Uri,
    /// Where the view is written when the client cannot fetch it.
    file: Option<PathBuf>,
    view: View,
    /// The view as the client has it, while the client has it open. An
    /// editor with the view open fetches new text when told that the view
    /// changed, and reports the text it then shows like any other edit.
    client: Option<Document>,
    /// The view the client still shows, while that is an older one. The
    /// lines its requests name are lines of this view.
    shown: Option<View>,
    /// The client closed its file. It goes unless the next message opens
    /// it again, as coc.nvim does when it reads the file anew.
    closing: bool,
    /// The lines last highlighted from edge to edge in the client's copy
    /// (`org-lsp/lineHighlights`).
    highlighted: Option<Vec<u32>>,
}

impl OpenView {
    /// Make `view` the current one.
    fn replace(&mut self, view: View) {
        let old = std::mem::replace(&mut self.view, view);
        let copy = self.client.as_ref().map(|copy| copy.text.as_str());
        if copy == Some(old.text.as_str()) && old.text != self.view.text {
            self.shown = Some(old);
        }
        self.forget_shown();
    }

    /// Drop the older view once the client no longer shows it.
    fn forget_shown(&mut self) {
        let copy = self.client.as_ref().map(|copy| copy.text.as_str());
        if self.shown.as_ref().is_some_and(|shown| copy != Some(shown.text.as_str())) {
            self.shown = None;
        }
    }

    /// The view the client shows: the current one, unless the client has
    /// not fetched it yet.
    fn on_screen(&self) -> &View {
        self.shown.as_ref().unwrap_or(&self.view)
    }

    /// Whether the client shows the current view.
    fn is_current(&self) -> bool {
        self.client.as_ref().is_some_and(|copy| copy.text == self.view.text)
    }
}

/// Where a command was invoked: a document and a place in it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    text_document: lsp_types::TextDocumentIdentifier,
    position: Position,
    /// The count the user typed before the key, if any.
    count: Option<i64>,
    /// The answer to what the command asked for.
    input: Option<String>,
    /// For `org.date.preview`: the command whose prompt is being answered.
    command: Option<String>,
    /// The TODO state a change was asked for, when `input` is the text
    /// of the note that change wants (`""` for no keyword).
    state: Option<String>,
    /// The answers to the questions of `org.archive.done` so far, `y` or
    /// `n` each; `input` is then the next one.
    answers: Option<String>,
    /// For the property commands: the question `input` answers.
    step: Option<String>,
    /// The text of the buffer the command runs in, as the editor has it:
    /// a capture or note buffer whose last changes the server may not have
    /// heard of yet.
    text: Option<String>,
    /// For the commands whose result carries their edit (`org.return`):
    /// have the server apply it instead, and move the cursor. Code actions
    /// run them so.
    #[serde(default)]
    apply: bool,
    /// For M-RET and its neighbours: the new heading, item or row goes
    /// below the cursor's line, never above it and splitting nothing, as
    /// from the keys of modal editors (`<Leader><CR>`).
    #[serde(default)]
    below: bool,
    /// What the editor would paste, for the commands that read it
    /// (`"clipboard": true` in the manifest).
    clipboard: Option<String>,
}

/// What `org.capture` is run with. Capturing needs no document: from
/// where there is none (no file open), the answers come alone.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureContext {
    text_document: Option<lsp_types::TextDocumentIdentifier>,
    position: Option<Position>,
    input: Option<String>,
    /// The keys of the template picked so far.
    keys: Option<String>,
    /// The answers to the template's questions so far; `input` is then the
    /// next one.
    answers: Option<Vec<String>>,
}

/// The editing command that ran last, and the entry it ran on. Some
/// commands go on differently when they are repeated.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Last {
    command: String,
    file: String,
    /// Byte offset of the entry's headline.
    heading: Option<usize>,
}

/// Where the agenda's cursor goes once the view shows the effect of an
/// edit: to the line of the entry that was changed.
struct Follow {
    /// The view as it will read then.
    text: String,
    /// The line the cursor is on now.
    from: u32,
    position: Position,
}

/// A capture going on (`org-capture`): the buffer the editor shows, and
/// where its text goes.
struct Capturing {
    /// The capture buffer, a file of its own.
    uri: Uri,
    /// The target file, and its text when the capture began.
    target: Uri,
    snapshot: String,
    /// The template in the target, and the part the buffer shows.
    placed: capture::Placed,
    /// To place the text again when the target has changed meanwhile.
    spec: (capture::Target, capture::Kind, capture::Properties),
}

/// A clock that runs: the file of its `CLOCK:` line, and when it started.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunningClock {
    uri: Uri,
    start: DateTime,
    /// The headline of its entry, after the stars: two clocks may start
    /// in the same minute.
    head: String,
}

/// The clocks that run in `text`: where each line starts, when it
/// started, and the headline of its entry.
fn running_lines(text: &str) -> Vec<(usize, DateTime, String)> {
    clock::running(text)
        .into_iter()
        .filter_map(|(line, start)| {
            let heading = Server::heading_above(text, line)?;
            let head = text[heading..].lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
            Some((line, start, head))
        })
        .collect()
}

/// A note being written for a TODO state change (`org-add-note`), in a
/// buffer of its own like a capture's: the change it is for waits for it.
struct Noting {
    /// The note buffer, a file of its own.
    uri: Uri,
    /// The command that changes the state, where it was run.
    id: String,
    context: Context,
    /// The state the note is for; empty for none.
    state: String,
}

/// The notification that asks the editor to save files, `{ "uris": [...] }`.
pub const SAVE: &str = "org-lsp/save";
/// From the client: how many columns of text the window that shows the
/// agenda has, `{ "width": N }`, for `org_agenda_tags_column` `"auto"`.
pub const AGENDA_WIDTH: &str = "org-lsp/agendaWidth";
/// The entry of an agenda line with its highlighting, for a popup.
pub const PREVIEW: &str = "org-lsp/preview";
/// From the client: the cursor moved to another line of the agenda, `{
/// "textDocument", "position" }`. In follow mode (`F`) the entry there
/// shows in another window.
pub const AGENDA_CURSOR: &str = "org-lsp/agendaCursor";

/// The name of a capture buffer starts with this, as Emacs names it.
pub const CAPTURE_PREFIX: &str = "CAPTURE-";

/// The kind and target of a capture template.
fn capture_spec(template: &CaptureTemplateOption) -> std::result::Result<(capture::Kind, capture::Target), Refusal> {
    let kind = match template.kind.as_deref() {
        None | Some("entry") => capture::Kind::Entry,
        Some("plain") => capture::Kind::Plain,
        Some(other) => return Err(format!("Capture templates of type `{other}` are not supported yet").into()),
    };
    let target = match template.target.first().map(String::as_str) {
        Some("file") => capture::Target::File,
        Some("file+headline") => capture::Target::Headline(template.target.get(2).cloned().unwrap_or_default()),
        Some("file+olp") => capture::Target::Path(template.target.get(2..).unwrap_or_default().to_vec()),
        Some(other) => return Err(format!("Capture targets of type `{other}` are not supported yet").into()),
        // No target: the end of `org_default_notes_file`, as `["file", ""]`.
        None => capture::Target::File,
    };
    Ok((kind, target))
}

/// What an editing command was told besides its name.
#[derive(Default)]
struct Extra {
    /// The text of the note a TODO state change wants.
    note: Option<String>,
    /// The note was cancelled: change the state and log nothing.
    skip_note: bool,
    /// The key that selects the new TODO state.
    key: Option<String>,
    /// No note is asked for: a time is logged in its place.
    time_only: bool,
}

/// The file a command that asks for a date changes.
struct DateSource {
    file: Uri,
    text: String,
    /// Where the command acts.
    offset: usize,
    /// The version of the editor's copy, when the file is open there.
    version: Option<i32>,
    /// The headline of the entry, for a command run from the agenda.
    heading: Option<usize>,
}

/// Where the agenda's cursor goes once the client shows the current view.
enum Reveal {
    /// To today, in a view that was just opened.
    Home,
    /// To a line of the current view. When the view changes before the
    /// client has fetched it, the number means nothing any more.
    At(Position),
    /// Into a block that was moved to other days: to the first line of
    /// the day if it is shown there, else of the day at the same place
    /// among the days shown (`org-day-cnt`), else to today or the first
    /// day.
    Block { block: usize, day: Option<Day>, count: Option<usize> },
}

impl Reveal {
    /// Where the cursor goes in `view`.
    fn position(&self, view: &View) -> Position {
        match self {
            Reveal::Home => Position::new(view.home_line(), 0),
            Reveal::At(position) => *position,
            Reveal::Block { block, day, count } => Position::new(view.block_line(*block, *day, *count), 0),
        }
    }
}

/// A source of agenda entries as the server holds it while it works.
/// What a clock table reads beyond its own file.
#[derive(Default)]
struct ClocktableFiles {
    agenda: Vec<String>,
    /// By path: the open files, the agenda files.
    texts: HashMap<String, String>,
    /// By path: the files a file archives to that exist.
    archives: HashMap<String, Vec<String>>,
}

impl org_core::clocktable::Files for ClocktableFiles {
    fn agenda_files(&self) -> Vec<String> {
        self.agenda.clone()
    }

    fn archive_files(&self, path: &str, _: &str) -> Vec<String> {
        self.archives.get(path).cloned().unwrap_or_default()
    }

    fn read(&self, path: &str) -> Option<String> {
        self.texts.get(path).cloned().or_else(|| std::fs::read_to_string(path).ok())
    }
}

enum Held {
    Open(Box<Outline>),
    Disk(Rc<Source>),
}

impl Held {
    fn outline(&self) -> &Outline {
        match self {
            Held::Open(outline) => outline,
            Held::Disk(source) => &source.outline,
        }
    }
}

/// A bibliography file and its entries.
#[derive(Debug)]
struct BibFile {
    path: PathBuf,
    text: String,
    entries: Vec<org_core::cite::Entry>,
}

struct Server {
    client: Client,
    sender: crossbeam_channel::Sender<Message>,
    /// Where the command port (`port`) sends what it is sent, and its
    /// script once it is open: with `command_actions`, while the agenda is a
    /// file.
    port_sender: crossbeam_channel::Sender<port::Command>,
    port: Option<port::Port>,
    /// Where the cursor was last seen in the agenda, for the port's
    /// commands that do not say: from the requests the client makes as
    /// the cursor moves, and from where the server put it.
    agenda_cursor: Option<Position>,
    next_request: i32,
    pending: HashMap<RequestId, Pending>,
    /// What the command being run has to say, as Emacs says it in the echo
    /// area. It goes back with the command's result.
    message: Option<String>,
    /// The last subtree cut or copied, to paste where the editor does not
    /// say what its clipboard has.
    kill: Option<String>,
    /// The document the command being run was run in, whose edits are the
    /// user's to save.
    origin: Option<String>,
    /// `auto_save`: save the files a command changes elsewhere.
    /// `return_uses_meta_return`: RET on a list item makes the next item.
    return_uses_meta_return: bool,
    auto_save: bool,
    /// What the highlighting marks as hidden.
    conceal: highlight::Conceal,
    /// `org-complete-tags-always-offer-all-agenda-tags`
    all_agenda_tags: bool,
    /// The files to save once the edit request with this id is applied.
    saving: HashMap<RequestId, Vec<Uri>>,
    todo: TodoKeywords,
    agenda: AgendaConfig,
    /// The `agenda` options as given, for the custom commands' own.
    agenda_options: crate::config::AgendaOptions,
    edit_settings: EditSettings,
    read_date: ReadDateSettings,
    /// `org-refile-targets`, `org-refile-use-outline-path` and how a
    /// subtree is filed.
    refile: (Vec<RefileTarget>, OutlinePath, RefileSettings),
    /// How archiving goes, and `org-archive-default-command`.
    archive: (ArchiveSettings, String),
    /// `org-capture-templates`, `org-directory` and
    /// `org-default-notes-file`.
    capture_templates: Vec<CaptureTemplateOption>,
    org_directory: String,
    default_notes_file: String,
    /// The capture whose buffer the editor shows.
    capture: Option<Capturing>,
    /// The note being written for a TODO state change, if one is.
    note: Option<Noting>,
    /// How many captures there were.
    captures: u32,
    /// Where the capture and note buffers go, one directory each.
    capture_directory: PathBuf,
    /// Where the agenda's file goes, for a client that cannot fetch it.
    view_directory: PathBuf,
    /// The date prompt that waits for an answer, in clients that show it.
    date_prompt: Option<DatePrompt>,
    /// The client's copy of the date prompt document, which it may keep
    /// after a prompt is done.
    prompt_copy: Option<Document>,
    /// `org-cycle-separator-lines`
    separator_lines: i64,
    /// `org-adapt-indentation` and its relatives.
    indent: org_core::indent::Settings,
    /// `org-startup-indented` and `org-hide-leading-stars`.
    display: org_core::indent::Display,
    /// The documents the client was last told to show with prefixes.
    virtually_indented: HashSet<String>,
    /// `org-cite-global-bibliography`
    cite_global: Vec<String>,
    /// The bibliography files read, with when each changed and its size,
    /// to read one again when it changes.
    bibliographies: HashMap<PathBuf, (Option<std::time::SystemTime>, u64, Rc<BibFile>)>,
    /// The documents the server published citation diagnostics for.
    cite_diagnosed: HashSet<String>,
    /// A fixed clock, for tests.
    clock: Option<Now>,
    /// The clock that runs, as the files say.
    running: Option<RunningClock>,
    /// The entry clocked last: its file and headline, for
    /// `org.clock.goto` when no clock runs.
    last_clocked: Option<(Uri, String)>,
    /// `org-clock-history`: the entries clocked into, the last first, by
    /// file and title; at most `org-clock-history-length`.
    clock_history: Vec<(Uri, String)>,
    clock_history_length: usize,
    /// `org-clock-default-task`, marked by clocking in with `C-u C-u`.
    clock_default_task: Option<(Uri, String)>,
    /// `org-clock-interrupted-task`: the entry whose clock the last
    /// clock-in stopped.
    clock_interrupted: Option<(Uri, String)>,
    /// `org-clock-out-time`: when the last clock stopped.
    clock_out_time: Option<DateTime>,
    /// `org-clock-into-drawer` and its relatives.
    clock_settings: org_core::clock::Settings,
    /// `org-clock-out-when-done`
    clock_out_when_done: bool,
    /// `org-clock-in-switch-to-state`, `org-clock-out-switch-to-state`
    clock_in_switch: Option<String>,
    clock_out_switch: Option<String>,
    /// How clock tables are written: `org-clock-clocktable-default-properties`
    /// and the cell formats.
    clocktable: org_core::clocktable::Settings,
    /// The clock's status waits for the edit that writes its line.
    clock_pending: bool,
    /// Log mode (`l`) and the clock report (`R`) in the agenda.
    agenda_log: bool,
    agenda_clockreport: bool,
    /// Entry text mode (`E`): the most lines of each entry's text, when on.
    agenda_entry_text: Option<usize>,
    /// Follow mode (`F`): the entry of the cursor's line shows in another
    /// window as the cursor moves.
    agenda_follow: bool,
    /// The unit the last command moved an agenda timestamp by, when it was
    /// hours or minutes: the next `S-<right>` goes on by it (Emacs's
    /// `last-command`). `date_unit_before` is the one of the command
    /// before the current one.
    date_unit: Option<org_core::stamp::Unit>,
    date_unit_before: Option<org_core::stamp::Unit>,
    /// The lines marked for a bulk action (`org-agenda-bulk-marked-entries`),
    /// in the order they were marked.
    agenda_marks: Vec<views::EntryRef>,
    /// `org-agenda-persistent-marks`
    agenda_persistent_marks: bool,
    /// While a bulk action runs: the files as its commands left them.
    staging: Option<bulk::Staging>,
    /// `org-clock-display`: by document, the lines of headlines and the
    /// minutes clocked under them, shown as inlay hints until the
    /// document changes.
    clock_display: HashMap<String, Vec<(u32, i64)>>,
    /// `org-clock-display-default-range`
    clock_display_range: String,
    /// The file whose headlines are the refile targets of the current
    /// file (`org-refile-targets` `nil`), when it is not the entry's: a
    /// bulk refile reads them from its first entry's file.
    refile_current: Option<Uri>,
    /// What the agenda shows of archives (`v a`, `v A`).
    agenda_archives: ArchivesMode,
    /// The filters of the agenda (`/`, `\\`, `<`, `=`, `_`, `^`), which
    /// a new agenda command clears and rebuilding keeps.
    agenda_filters: org_core::agenda::filter::Filters,
    documents: HashMap<String, Document>,
    workspace: Workspace,
    view: Option<OpenView>,
    /// Files the server asked the client to edit and has not seen change yet.
    edited: HashSet<String>,
    /// Where to put the cursor once the client shows the current view.
    reveal: Option<Reveal>,
    follow: Option<Follow>,
    /// The cursor of the agenda is on its way from a line of the current
    /// view to another: the client was asked to move it and has not
    /// answered yet. A command from the old line is meant for the new one.
    moving: Option<(u32, u32)>,
    last: Option<Last>,
    /// `org-last-set-property`: the name `org-set-property` offers by
    /// default.
    last_property: Option<String>,
}

/// Run the server until the client asks it to exit.
pub fn run(connection: Connection) -> Result<()> {
    let (id, raw_params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(raw_params.clone())?;

    // The shared configuration file with the editor's options on top.
    // Unknown or malformed options must not keep the server from starting,
    // nor take the other options with them.
    let loaded = config::load(params.initialization_options.clone(), &|name| std::env::var(name).ok());
    for problem in &loaded.problems {
        eprintln!("org-lsp: {problem}");
    }
    let options = loaded.options;
    let (port_sender, port_receiver) = crossbeam_channel::unbounded();

    let general = params.capabilities.general.as_ref();
    let text_document = params.capabilities.text_document.as_ref();
    let window = params.capabilities.window.as_ref();
    let client = Client {
        encoding: Encoding::negotiate(general.and_then(|general| general.position_encodings.as_deref())),
        hierarchical_symbols: text_document
            .and_then(|capabilities| capabilities.document_symbol.as_ref())
            .and_then(|symbols| symbols.hierarchical_document_symbol_support)
            .unwrap_or(false),
        document_changes: params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.workspace_edit.as_ref())
            .and_then(|edit| edit.document_changes)
            .unwrap_or(false),
        virtual_documents: options.virtual_documents.unwrap_or_else(|| {
            raw_params.pointer("/capabilities/workspace/textDocumentContent").is_some_and(|value| !value.is_null())
        }),
        show_document: options.show_document.unwrap_or_else(|| {
            window.and_then(|window| window.show_document.as_ref()).is_some_and(|show| show.support)
        }),
        date_prompt: raw_params.pointer("/capabilities/experimental/orgDatePrompt").and_then(Value::as_bool)
            == Some(true),
        virtual_indent: raw_params.pointer("/capabilities/experimental/orgVirtualIndent").and_then(Value::as_bool)
            == Some(true)
            || options.virtual_indent == Some(true),
        line_highlights: raw_params.pointer("/capabilities/experimental/orgLineHighlights").and_then(Value::as_bool)
            == Some(true)
            || options.line_highlights == Some(true),
        tokens_refresh: raw_params
            .pointer("/capabilities/workspace/semanticTokens/refreshSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        inlay_refresh: raw_params
            .pointer("/capabilities/workspace/inlayHint/refreshSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        message_prompts: options.message_prompts.unwrap_or(false),
        command_actions: options.command_actions.unwrap_or(false),
        command_script: options
            .command_script
            .as_deref()
            .map(|path| config::expand_home(path, &|name| std::env::var(name).ok()))
            .or_else(port::script_path),
    };
    let client =
        Client { date_prompt: client.date_prompt && client.virtual_documents && client.show_document, ..client };

    let todo = options.todo_keywords();

    #[allow(deprecated)]
    let folders: Vec<PathBuf> = match &params.workspace_folders {
        Some(folders) => folders.iter().filter_map(|folder| uri::to_path(&folder.uri)).collect(),
        None => params.root_uri.as_ref().and_then(uri::to_path).into_iter().collect(),
    };

    let result = InitializeResult {
        capabilities: capabilities(client.encoding, true, client.command_actions),
        server_info: Some(ServerInfo {
            name: "org-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    let mut result = serde_json::to_value(result)?;
    // The scheme of the agenda document, for clients that fetch it.
    result["capabilities"]["workspace"] = json!({ "textDocumentContent": { "schemes": [views::SCHEME] } });
    connection.initialize_finish(id, result)?;

    let mut server = Server {
        client,
        sender: connection.sender.clone(),
        port_sender,
        port: None,
        agenda_cursor: None,
        next_request: 0,
        pending: HashMap::new(),
        message: None,
        kill: None,
        origin: None,
        auto_save: options.auto_save.unwrap_or(true),
        return_uses_meta_return: options.return_uses_meta_return.unwrap_or(false),
        all_agenda_tags: options.complete_tags_always_offer_all_agenda_tags.unwrap_or(false),
        conceal: highlight::Conceal {
            links: options.link_descriptive.unwrap_or(true),
            emphasis: options.hide_emphasis_markers.unwrap_or(false),
        },
        saving: HashMap::new(),
        todo,
        agenda: options.agenda_config(),
        agenda_options: options.agenda.clone(),
        edit_settings: options.edit_settings(),
        read_date: options.read_date_settings(),
        refile: (options.refile_targets(), options.refile_outline_path(), options.refile_settings()),
        archive: (options.archive_settings(), options.archive_default_command().to_string()),
        capture_templates: options.capture_templates(),
        org_directory: options.org_directory(),
        default_notes_file: options.default_notes_file(),
        capture: None,
        note: None,
        captures: 0,
        capture_directory: server_directory("capture"),
        view_directory: server_directory("view"),
        date_prompt: None,
        prompt_copy: None,
        separator_lines: options.separator_lines(),
        indent: options.indent_settings(),
        display: options.display(),
        virtually_indented: HashSet::new(),
        cite_global: options.cite_global_bibliography.clone().unwrap_or_default(),
        bibliographies: HashMap::new(),
        cite_diagnosed: HashSet::new(),
        clock: options.clock.as_ref().and_then(|clock| clock.to_now()),
        running: None,
        clock_pending: false,
        last_clocked: None,
        clock_history: Vec::new(),
        clock_history_length: options.clock_history_length.unwrap_or(5).max(1),
        clock_default_task: None,
        clock_interrupted: None,
        clock_out_time: None,
        clock_settings: options.clock_settings(),
        clock_out_when_done: options.clock_out_when_done.unwrap_or(true),
        clock_in_switch: options.clock_in_switch_to_state.clone(),
        clock_out_switch: options.clock_out_switch_to_state.clone(),
        clocktable: options.clocktable_settings(),
        agenda_log: options.agenda.start_with_log_mode.unwrap_or(false),
        agenda_clockreport: options.agenda.start_with_clockreport_mode.unwrap_or(false),
        agenda_archives: options.agenda.archives_mode(),
        agenda_entry_text: options.agenda_config().entry_text,
        agenda_follow: options.agenda.start_with_follow_mode.unwrap_or(false),
        date_unit: None,
        date_unit_before: None,
        agenda_marks: Vec::new(),
        agenda_persistent_marks: options.agenda.persistent_marks.unwrap_or(false),
        staging: None,
        clock_display: HashMap::new(),
        clock_display_range: options.clock_display_default_range.clone().unwrap_or_else(|| "thisyear".to_string()),
        refile_current: None,
        agenda_filters: Default::default(),
        documents: HashMap::new(),
        workspace: {
            let inheritance = options.inheritance();
            let mut workspace = Workspace::new(options.agenda_files, folders);
            workspace.inheritance = inheritance;
            workspace
        },
        view: None,
        edited: HashSet::new(),
        reveal: None,
        follow: None,
        moving: None,
        last: None,
        last_property: None,
    };
    // A problem with the options is worth a message where the user looks.
    if !loaded.problems.is_empty() {
        server.show_message(MessageType::WARNING, format!("org-lsp: {}", loaded.problems.join("; ")));
    }
    // A clock left running in the files runs on.
    server.running = server.find_running();
    if server.running.is_some() {
        server.publish_clock();
    }
    loop {
        crossbeam_channel::select! {
            recv(connection.receiver) -> message => match message {
                Ok(Message::Request(request)) => {
                    if connection.handle_shutdown(&request)? {
                        break;
                    }
                    let response = server.request(request);
                    connection.sender.send(Message::Response(response))?;
                }
                Ok(Message::Notification(notification)) => server.notification(notification),
                Ok(Message::Response(response)) => server.response(response),
                Err(_) => break,
            },
            recv(port_receiver) -> command => {
                if let Ok(command) = command {
                    server.run_from_port(command);
                }
            }
        }
    }
    server.remove_view_file();
    if server.captures > 0 {
        let _ = std::fs::remove_dir_all(&server.capture_directory);
    }
    Ok(())
}

/// The values property `name` has in the property drawers of `text`.
fn property_values(text: &str, name: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_drawer = false;
    for line in text.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case(":PROPERTIES:") {
            in_drawer = true;
        } else if line.eq_ignore_ascii_case(":END:") {
            in_drawer = false;
        } else if in_drawer {
            if let Some((key, value)) = line.strip_prefix(':').and_then(|rest| rest.split_once(':')) {
                if key.eq_ignore_ascii_case(name) && !value.trim().is_empty() {
                    values.push(value.trim().to_string());
                }
            }
        }
    }
    values
}

/// The names of the properties in the property drawers of `text`.
fn property_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_drawer = false;
    for line in text.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case(":PROPERTIES:") {
            in_drawer = true;
        } else if line.eq_ignore_ascii_case(":END:") {
            in_drawer = false;
        } else if in_drawer {
            if let Some((name, _)) = line.strip_prefix(':').and_then(|rest| rest.split_once(':')) {
                let name = name.trim_end_matches('+');
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// A command's result with what it has to say: `{ "message": … }` on top
/// of whatever else the result holds.
/// A choice of a question as a button: its label, and its key when it is
/// picked by one.
fn prompt_label(choice: &Value) -> String {
    let label = choice.get("label").and_then(Value::as_str).unwrap_or("");
    match choice.get("key").and_then(Value::as_str) {
        Some(" ") => format!("{label} [SPC]"),
        Some("\r") => format!("{label} [RET]"),
        Some("\t") => format!("{label} [TAB]"),
        Some(key) if key != label && key.chars().count() == 1 => format!("{label} [{key}]"),
        _ => label.to_string(),
    }
}

fn with_message(result: Value, message: Option<String>) -> Value {
    match (result, message) {
        (result, None) => result,
        (Value::Null, Some(message)) => json!({ "message": message }),
        (Value::Object(mut fields), Some(message)) => {
            fields.insert("message".to_string(), json!(message));
            Value::Object(fields)
        }
        (result, Some(_)) => result,
    }
}

/// What Emacs says about a change to the headline at `heading`: the
/// priority it now has, or, for a change made from the agenda where the
/// entry is out of sight, its TODO state. A state asked for (`requested`)
/// that the entry already had is worth a word as well; not one the entry
/// comes back to, as a repeating entry marked done does.
fn headline_message(
    old: &str,
    new: &str,
    heading: usize,
    keywords: &TodoKeywords,
    from_agenda: bool,
    requested: Option<&Option<String>>,
) -> Option<String> {
    let line = old[..heading.min(old.len())].matches('\n').count();
    let state = |text: &str| {
        let headline = text.lines().nth(line)?;
        let parse = org_syntax::parse(headline, keywords);
        let headline = org_syntax::ast::headline_at(&parse.syntax(), 0.into())?;
        let keyword = headline.todo_keyword().map(|token| token.text().to_string());
        let priority = headline.priority().map(|token| token.text().trim_matches(['[', '#', ']']).to_string());
        Some((keyword, priority))
    };
    let ((old_keyword, old_priority), (keyword, priority)) = (state(old)?, state(new)?);
    if priority != old_priority {
        return Some(match priority {
            Some(priority) => format!("Priority of current item set to {priority}"),
            None => "Priority removed".to_string(),
        });
    }
    if keyword != old_keyword && from_agenda {
        return Some(format!("TODO state changed to {}", keyword.unwrap_or_default()));
    }
    // A state picked by its key that the entry already has.
    if requested.is_some_and(|requested| *requested == old_keyword) {
        return Some(format!("TODO state was already {}", keyword.unwrap_or_default()));
    }
    None
}

/// A span as Emacs names it in its messages: `week`, or a number of days.
fn span_name(span: Span) -> String {
    match span {
        Span::Day => "day".to_string(),
        Span::Week => "week".to_string(),
        Span::Fortnight => "fortnight".to_string(),
        Span::Month => "month".to_string(),
        Span::Year => "year".to_string(),
        Span::Days(days) => days.to_string(),
    }
}

/// A directory for the capture buffers or the agenda file of a server:
/// one of its own, also when a process runs more than one (the tests).
fn server_directory(kind: &str) -> PathBuf {
    static SERVERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let server = SERVERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    temp_dir().join(format!("org-lsp-{kind}-{}-{server}", std::process::id()))
}

/// The directory for temporary files, with its links resolved. Editors
/// that resolve them name the file the server shows by its real path
/// (macOS's `/var/folders/…` is `/private/var/folders/…`), and the
/// server knows its files by the path it gave them.
fn temp_dir() -> PathBuf {
    let temp = std::env::temp_dir();
    // Not on Windows: there the real path is a `\\?\` one.
    #[cfg(unix)]
    if let Ok(real) = std::fs::canonicalize(&temp) {
        return real;
    }
    temp
}

/// Write the agenda's file and leave it read-only: the agenda is to read,
/// and editors that go by a file's permissions (Zed, Vim) then do not let
/// it be edited.
fn write_read_only(file: &Path, text: &str) -> std::io::Result<()> {
    // Whole, in one step: an editor that reads the file as it changes
    // never finds it half written, or missing.
    #[cfg(unix)]
    {
        let temporary = file.with_extension(format!("{}.new", std::process::id()));
        std::fs::write(&temporary, text)?;
        let mut permissions = std::fs::metadata(&temporary)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&temporary, permissions)?;
        std::fs::rename(&temporary, file)
    }
    // Windows does not replace a read-only file.
    #[cfg(not(unix))]
    {
        set_writable_if_present(file)?;
        std::fs::write(file, text)?;
        let mut permissions = std::fs::metadata(file)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(file, permissions)
    }
}

/// Let the owner of `file` write it again, if there is one.
#[cfg(not(unix))]
fn set_writable_if_present(file: &Path) -> std::io::Result<()> {
    if std::fs::metadata(file).is_ok_and(|metadata| metadata.permissions().readonly()) {
        set_writable(file)?;
    }
    Ok(())
}

/// Let the owner of `file` write it again.
fn set_writable(file: &Path) -> std::io::Result<()> {
    let mut permissions = std::fs::metadata(file)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(file, permissions)
}

/// What the server does. `inlay_hints`: show `org-indent-mode`'s prefixes
/// as inlay hints, for a client that does not show them otherwise.
/// `cursor`: ask for document highlights, which a client asks for as the
/// cursor moves, so that the server knows where it is in the agenda
/// (`command_actions`).
fn capabilities(encoding: Encoding, inlay_hints: bool, cursor: bool) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::INCREMENTAL),
            save: Some(TextDocumentSyncSaveOptions::Supported(true)),
            ..Default::default()
        })),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: highlight::legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(true),
                ..Default::default()
            },
        )),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        completion_provider: Some(lsp_types::CompletionOptions {
            trigger_characters: Some([":", "+", "[", "/", "*", "#", "\\"].iter().map(|c| c.to_string()).collect()),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(actions::KINDS.iter().map(|kind| CodeActionKind::new(kind)).collect()),
            ..Default::default()
        })),
        execute_command_provider: Some(ExecuteCommandOptions { commands: commands::ids(), ..Default::default() }),
        // `org-indent-line` on a new line, `org-indent-region` on a range.
        document_on_type_formatting_provider: Some(lsp_types::DocumentOnTypeFormattingOptions {
            first_trigger_character: "\n".to_string(),
            more_trigger_character: None,
        }),
        document_range_formatting_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        inlay_hint_provider: inlay_hints.then_some(OneOf::Left(true)),
        document_highlight_provider: cursor.then_some(OneOf::Left(true)),
        experimental: Some(json!({ "org": manifest::manifest() })),
        ..Default::default()
    }
}

/// An edit of `org_core::indent` as the client counts positions.
fn indent_edit(text: &str, index: &LineIndex, encoding: Encoding, edit: org_core::indent::Edit) -> TextEdit {
    let range = org_syntax::TextRange::new((edit.start as u32).into(), (edit.end as u32).into());
    TextEdit { range: index.range(text, range, encoding), new_text: edit.text }
}

/// The edits that turn `text` into `new`, which has the same lines: each
/// line that differs, replaced.
fn line_edits(text: &str, index: &LineIndex, encoding: Encoding, new: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut start = 0;
    for (old, new) in text.split('\n').zip(new.split('\n')) {
        let end = start + old.len();
        if old != new {
            edits.push(indent_edit(
                text,
                index,
                encoding,
                org_core::indent::Edit { start, end, text: new.to_string() },
            ));
        }
        start = end + 1;
    }
    edits
}

/// Parse the parameters of `request`, or produce the error response.
fn parameters<P: DeserializeOwned>(request: Request) -> std::result::Result<(RequestId, P), Response> {
    let id = request.id.clone();
    match serde_json::from_value(request.params) {
        Ok(params) => Ok((id, params)),
        Err(error) => Err(Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string())),
    }
}

fn respond<T: serde::Serialize>(id: RequestId, result: T) -> Response {
    match serde_json::to_value(result) {
        Ok(value) => Response::new_ok(id, value),
        Err(error) => Response::new_err(id, ErrorCode::InternalError as i32, error.to_string()),
    }
}

/// The answer to a command that was not carried out. A command that has
/// no meaning at the cursor says so in the error data, so that an editor
/// can let the key do what it otherwise does.
fn refused(id: RequestId, refusal: Refusal) -> Response {
    let data = matches!(refusal, Refusal::NotHere(_)).then(|| json!({ "notApplicable": true }));
    let error = ResponseError { code: ErrorCode::RequestFailed as i32, message: refusal.message().to_string(), data };
    Response { id, response_result: Err(error) }
}

/// The command of `org-core` behind a command that asks for a date.
/// How a command that asks for a date reads and shows the answer. Going to
/// a date in the agenda asks with a plain `org-read-date`, which reads and
/// shows it as `org-timestamp` does.
fn date_command(id: &str) -> Option<PlanningCommand> {
    match id {
        commands::AGENDA_GOTO_DATE => Some(PlanningCommand::Timestamp { inactive: false }),
        id => planning_command(id),
    }
}

fn planning_command(id: &str) -> Option<PlanningCommand> {
    match id {
        commands::SCHEDULE => Some(PlanningCommand::Schedule),
        commands::DEADLINE => Some(PlanningCommand::Deadline),
        commands::TIMESTAMP_INSERT => Some(PlanningCommand::Timestamp { inactive: false }),
        commands::TIMESTAMP_INSERT_INACTIVE => Some(PlanningCommand::Timestamp { inactive: true }),
        // Active or inactive as the timestamp is: see `date_command_at`.
        commands::TIMESTAMP_CHANGE => Some(PlanningCommand::Timestamp { inactive: false }),
        _ => None,
    }
}

/// Write a tangled file as `org-babel-tangle` writes it: only when its text
/// changed, its directories made with `:mkdirp`, its mode set.
fn write_tangled(file: &org_core::tangle::File) -> std::io::Result<()> {
    if std::fs::read_to_string(&file.path).is_ok_and(|text| text == file.text) {
        return Ok(());
    }
    if file.make_directories {
        if let Some(parent) = file.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&file.path, &file.text)?;
    #[cfg(unix)]
    if let Some(mode) = file.mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file.path, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

/// Whether byte `offset` is in a source block.
/// Whether byte `offset` is on a line of an Org table (`org-at-table-p`):
/// one that starts with `|`, in a table rather than a block.
fn at_org_table(text: &str, root: &org_syntax::SyntaxNode, offset: usize) -> bool {
    if !org_core::table::at_table(text, offset) {
        return false;
    }
    let line = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let first = line + text[line..].len() - text[line..].trim_start_matches([' ', '\t']).len();
    let Some(token) = root.token_at_offset(org_syntax::TextSize::from(first as u32)).right_biased() else {
        return false;
    };
    token.parent_ancestors().any(|node| node.kind() == org_syntax::SyntaxKind::TABLE)
}

/// The table command a command is in a table.
fn table_command(id: &str) -> Option<org_core::table::Command> {
    use org_core::table::Command;
    Some(match id {
        commands::META_LEFT => Command::MoveColumn { left: true },
        commands::META_RIGHT => Command::MoveColumn { left: false },
        commands::META_UP => Command::MoveRow { up: true },
        commands::META_DOWN => Command::MoveRow { up: false },
        commands::SHIFT_META_LEFT => Command::DeleteColumn,
        commands::SHIFT_META_RIGHT => Command::InsertColumn,
        commands::SHIFT_META_UP => Command::KillRow,
        commands::SHIFT_META_DOWN => Command::InsertRow { below: false },
        commands::TABLE_INSERT_ROW_BELOW => Command::InsertRow { below: true },
        commands::TABLE_NEXT_FIELD => Command::NextField,
        commands::TABLE_PREVIOUS_FIELD => Command::PreviousField,
        commands::TABLE_HLINE => Command::InsertHline { above: false },
        commands::TABLE_HLINE_ABOVE => Command::InsertHline { above: true },
        commands::TABLE_HLINE_MOVE => Command::HlineAndMove,
        commands::TABLE_BLANK_FIELD => Command::BlankField,
        _ => return None,
    })
}

fn in_src_block(root: &org_syntax::SyntaxNode, offset: usize) -> bool {
    let Some(token) = root.token_at_offset(org_syntax::TextSize::from(offset as u32)).left_biased() else {
        return false;
    };
    token.parent_ancestors().filter(|node| node.kind() == org_syntax::SyntaxKind::BLOCK).any(|block| {
        block
            .children_with_tokens()
            .find(|child| child.kind() == org_syntax::SyntaxKind::BLOCK_BEGIN)
            .is_some_and(|begin| begin.to_string().trim_start().eq_ignore_ascii_case("#+begin_src"))
    })
}

/// Whether byte `offset` is on the first line of a list item, the line of
/// its bullet.
fn on_item_line(root: &org_syntax::SyntaxNode, text: &str, offset: usize) -> bool {
    let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let Some(token) = root.token_at_offset(org_syntax::TextSize::from(offset as u32)).left_biased() else {
        return false;
    };
    token
        .parent_ancestors()
        .find(|node| node.kind() == org_syntax::SyntaxKind::LIST_ITEM)
        .is_some_and(|item| usize::from(item.text_range().start()) >= line_start)
}

fn is_view_uri(uri: &Uri) -> bool {
    uri.as_str().strip_prefix(views::SCHEME).is_some_and(|rest| rest.starts_with(':'))
}

impl Server {
    fn document(&self, uri: &Uri) -> Option<&Document> {
        self.documents.get(uri.as_str())
    }

    /// The agenda as the client shows it, if `uri` names it.
    fn view_at(&self, uri: &Uri) -> Option<&View> {
        self.view.as_ref().filter(|open| open.uri == *uri).map(OpenView::on_screen)
    }

    /// The line of the agenda that a request at `line` of `uri` is about,
    /// with its number. That is the line the client shows there, or the
    /// line the cursor is being moved to from there.
    fn view_line(&self, uri: &Uri, line: u32) -> Option<(u32, &ViewLine)> {
        let open = self.view.as_ref().filter(|open| open.uri == *uri)?;
        let line = match self.moving {
            Some((from, to)) if from == line && open.is_current() => to,
            _ => line,
        };
        Some((line, open.on_screen().line(line)?))
    }

    fn is_view(&self, uri: &Uri) -> bool {
        is_view_uri(uri) || self.view.as_ref().is_some_and(|open| open.uri == *uri)
    }

    fn now(&self) -> Now {
        self.clock.unwrap_or_else(cli::now)
    }

    /// The current local time, for timestamps the server writes.
    fn date_time(&self) -> DateTime {
        let now = self.now();
        DateTime { day: now.today, minute: now.minute }
    }

    // ----- messages to the client

    fn send_request<R: lsp_types::request::Request>(&mut self, params: R::Params, pending: Option<Pending>) {
        self.send_raw_request(R::METHOD, serde_json::to_value(params).unwrap_or(Value::Null), pending);
    }

    /// Ask the editor to apply `params`. With `auto_save`, the files it
    /// changes other than the one the command was run in (an entry's file
    /// changed from the agenda, the target of a capture or a refile) are
    /// saved once the editor has applied the edit.
    fn apply_edit(&mut self, params: lsp_types::ApplyWorkspaceEditParams, pending: Pending) {
        if self.staging.is_some() {
            self.stage(&params.edit);
            return;
        }
        let mut files: Vec<Uri> = match (&params.edit.document_changes, &params.edit.changes) {
            (Some(lsp_types::DocumentChanges::Edits(edits)), _) => {
                edits.iter().map(|edit| edit.text_document.uri.clone()).collect()
            }
            (_, Some(changes)) => changes.keys().cloned().collect(),
            _ => Vec::new(),
        };
        files.retain(|uri| Some(uri.as_str()) != self.origin.as_deref() && !is_view_uri(uri));
        let request = self.new_request(
            ApplyWorkspaceEdit::METHOD,
            serde_json::to_value(params).unwrap_or(Value::Null),
            Some(pending),
        );
        if self.auto_save && !files.is_empty() {
            self.saving.insert(request.id.clone(), files);
        }
        let _ = self.sender.send(Message::Request(request));
    }

    fn send_raw_request(&mut self, method: &str, params: Value, pending: Option<Pending>) {
        let request = self.new_request(method, params, pending);
        // A closed channel means the client is gone; the loop ends on its own.
        let _ = self.sender.send(Message::Request(request));
    }

    fn new_request(&mut self, method: &str, params: Value, pending: Option<Pending>) -> Request {
        self.next_request += 1;
        let id = RequestId::from(format!("org-lsp-{}", self.next_request));
        if let Some(pending) = pending {
            self.pending.insert(id.clone(), pending);
        }
        Request { id, method: method.to_string(), params }
    }

    fn show_message(&self, typ: MessageType, message: impl Into<String>) {
        let params = ShowMessageParams { typ, message: message.into() };
        let notification = Notification::new(ShowMessage::METHOD.to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    fn response(&mut self, response: Response) {
        // An edit to save, now that the editor has applied it.
        if let Some(files) = self.saving.remove(&response.id) {
            let applied = response.response_result.as_ref().ok().and_then(|result| result.get("applied")?.as_bool());
            if applied == Some(true) {
                let uris: Vec<&str> = files.iter().map(|uri| uri.as_str()).collect();
                let notification = Notification::new(SAVE.to_string(), json!({ "uris": uris }));
                let _ = self.sender.send(Message::Notification(notification));
            }
        }
        match self.pending.remove(&response.id) {
            Some(Pending::Edit) => self.refresh_view(),
            Some(Pending::EditAndMove { uri, position }) => {
                self.refresh_view();
                let applied = response.response_result.ok().and_then(|result| result.get("applied")?.as_bool());
                if applied == Some(true) {
                    let selection = Some(Range::new(position, position));
                    let params = ShowDocumentParams { uri, external: None, take_focus: Some(true), selection };
                    self.send_request::<ShowDocument>(params, None);
                }
            }
            Some(Pending::Reveal) => self.moving = None,
            Some(Pending::PromptCursor) => {
                if let Some(open) = self.date_prompt.as_mut() {
                    open.moving_cursor = false;
                }
            }
            Some(Pending::Answer { command, argument, input }) => {
                let picked =
                    response.response_result.ok().and_then(|item| Some(item.get("title")?.as_str()?.to_string()));
                if let Some(picked) = picked {
                    self.answer_prompt(command, argument, &input, &picked);
                }
            }
            None => {}
        }
    }

    // ----- requests

    fn request(&mut self, request: Request) -> Response {
        self.drop_closed_view();
        macro_rules! dispatch {
            ($($request:ty => $handler:ident,)*) => {
                match request.method.as_str() {
                    $(<$request>::METHOD => match parameters(request) {
                        Ok((id, params)) => respond(id, self.$handler(params)),
                        Err(response) => response,
                    },)*
                    ExecuteCommand::METHOD => match parameters::<ExecuteCommandParams>(request) {
                        Ok((id, params)) => {
                            let (command, argument) = (params.command.clone(), params.arguments.first().cloned());
                            match self.run_command(params) {
                                Ok(value) => {
                                    if self.client.message_prompts {
                                        self.prompt_with_messages(command, argument.unwrap_or(Value::Null), &value);
                                    }
                                    respond(id, value)
                                }
                                Err(refusal) => refused(id, refusal),
                            }
                        }
                        Err(response) => response,
                    },
                    // A request too, for clients that send no notifications
                    // of their own (Vim's).
                    AGENDA_WIDTH => match parameters::<Value>(request) {
                        Ok((id, params)) => {
                            self.agenda_width(&params);
                            Response::new_ok(id, Value::Null)
                        }
                        Err(response) => response,
                    },
                    AGENDA_CURSOR => match parameters::<Value>(request) {
                        Ok((id, params)) => {
                            self.agenda_cursor(params);
                            Response::new_ok(id, Value::Null)
                        }
                        Err(response) => response,
                    },
                    PREVIEW => match parameters::<Value>(request) {
                        Ok((id, params)) => respond(id, self.preview(&params)),
                        Err(response) => response,
                    },
                    TEXT_DOCUMENT_CONTENT => match parameters::<Value>(request) {
                        Ok((id, params)) => respond(id, self.text_document_content(&params)),
                        Err(response) => response,
                    },
                    method => {
                        let message = format!("org-lsp does not handle `{method}`");
                        Response::new_err(request.id, ErrorCode::MethodNotFound as i32, message)
                    }
                }
            };
        }
        dispatch! {
            SemanticTokensFullRequest => semantic_tokens_full,
            SemanticTokensRangeRequest => semantic_tokens_range,
            FoldingRangeRequest => folding_ranges,
            DocumentSymbolRequest => document_symbols,
            CodeActionRequest => code_actions,
            GotoDefinition => definition,
            lsp_types::request::HoverRequest => hover,
            lsp_types::request::Completion => completion,
            lsp_types::request::OnTypeFormatting => on_type_formatting,
            lsp_types::request::RangeFormatting => range_formatting,
            lsp_types::request::Formatting => formatting,
            lsp_types::request::InlayHintRequest => inlay_hints,
            lsp_types::request::DocumentHighlightRequest => document_highlights,
        }
    }

    /// `org-lsp/agendaWidth`: the agenda's window is `width` columns wide;
    /// tags aligned to its edge (`org_agenda_tags_column` `"auto"`) move.
    fn agenda_width(&mut self, params: &Value) {
        let width = params.get("width").and_then(Value::as_u64).filter(|width| *width > 0).map(|width| width as usize);
        if width.is_some() && width != self.agenda.window_width {
            self.agenda.window_width = width;
            self.refresh_view();
        }
    }

    /// The cursor is on another line of the agenda: in follow mode, its
    /// entry shows in another window.
    fn agenda_cursor(&mut self, params: Value) {
        if self.agenda_follow {
            let context = serde_json::from_value::<Context>(params).ok();
            self.show_entry(context.as_ref());
        }
    }

    fn notification(&mut self, notification: Notification) {
        // A notification the server cannot read is dropped: there is no way
        // to answer one, and the next full sync repairs any lost change.
        fn parse<P: DeserializeOwned>(notification: Notification) -> Option<P> {
            serde_json::from_value(notification.params).ok()
        }
        let reopened = notification.method == DidOpenTextDocument::METHOD
            && self.view.as_ref().is_some_and(|open| {
                notification.params.pointer("/textDocument/uri").and_then(Value::as_str) == Some(open.uri.as_str())
            });
        if reopened {
            if let Some(open) = &mut self.view {
                open.closing = false;
            }
        } else {
            self.drop_closed_view();
        }
        match notification.method.as_str() {
            AGENDA_WIDTH => self.agenda_width(&notification.params),
            AGENDA_CURSOR => self.agenda_cursor(notification.params),
            DidOpenTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidOpenTextDocumentParams>(notification) else { return };
                let document = params.text_document;
                if document.uri.as_str() == prompt::URI {
                    self.prompt_copy = Some(Document::new(document.text, document.version, &self.todo));
                    self.reveal_prompt_when_current();
                    return;
                }
                // The agenda is the server's own text; the client's copy is not read.
                if self.is_view(&document.uri) {
                    if let Some(open) = self.view.as_mut().filter(|open| open.uri == document.uri) {
                        open.client = Some(Document::new(document.text, document.version, &self.todo));
                        open.highlighted = None;
                        open.forget_shown();
                    }
                    self.reveal_when_current();
                    return;
                }
                let key = document.uri.as_str().to_string();
                self.documents.insert(key.clone(), Document::new(document.text, document.version, &self.todo));
                self.sync_clock(&key);
                self.publish_virtual_indent(&key);
                self.publish_citation_diagnostics(&key);
            }
            DidChangeTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidChangeTextDocumentParams>(notification) else { return };
                if params.text_document.uri.as_str() == prompt::URI {
                    let (encoding, todo) = (self.client.encoding, &self.todo);
                    if let Some(copy) = self.prompt_copy.as_mut() {
                        copy.apply(params.content_changes, params.text_document.version, encoding, todo);
                    }
                    self.reveal_prompt_when_current();
                    return;
                }
                // The client's copy of the agenda follows the server's.
                let (encoding, todo) = (self.client.encoding, &self.todo);
                let view = self.view.as_mut().filter(|open| open.uri == params.text_document.uri);
                if let Some(open) = view.filter(|open| open.client.is_some()) {
                    if let Some(copy) = open.client.as_mut() {
                        copy.apply(params.content_changes, params.text_document.version, encoding, todo);
                    }
                    // New lines: highlighted anew, whatever was before.
                    open.highlighted = None;
                    open.forget_shown();
                    self.reveal_when_current();
                    return;
                }
                let key = params.text_document.uri.as_str();
                if let Some(document) = self.documents.get_mut(key) {
                    document.apply(
                        params.content_changes,
                        params.text_document.version,
                        self.client.encoding,
                        &self.todo,
                    );
                }
                self.sync_clock(key);
                self.publish_virtual_indent(key);
                self.publish_citation_diagnostics(key);
                // The clock's times go with the next change, as Emacs's
                // overlays do (`org-remove-highlights-with-change`).
                if self.clock_display.remove(key).is_some() {
                    self.refresh_inlay_hints();
                }
                // An edit the server asked for has arrived: show its effect.
                // Any other change means the user did something in between.
                if self.edited.remove(key) {
                    self.refresh_view();
                } else {
                    self.last = None;
                }
            }
            DidSaveTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidSaveTextDocumentParams>(notification) else { return };
                if !self.is_view(&params.text_document.uri) {
                    self.refresh_view();
                }
            }
            DidCloseTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidCloseTextDocumentParams>(notification) else { return };
                // The client may close its copy of the date prompt any time
                // after the prompt is done, even after another has opened,
                // so this cancels nothing: the next prompt replaces it.
                if params.text_document.uri.as_str() == prompt::URI {
                    self.prompt_copy = None;
                    return;
                }
                self.documents.remove(params.text_document.uri.as_str());
                self.virtually_indented.remove(params.text_document.uri.as_str());
                // The agenda was closed: stop rebuilding it, once it is clear
                // that the client does not open its file again at once.
                if let Some(open) = self.view.as_mut().filter(|open| open.uri == params.text_document.uri) {
                    open.client = None;
                    open.closing = true;
                    if open.file.is_none() {
                        self.drop_closed_view();
                    }
                }
            }
            _ => {}
        }
    }

    fn semantic_tokens(&self, uri: &Uri, range: Option<Range>) -> Option<SemanticTokens> {
        let encoding = self.client.encoding;
        if uri.as_str() == prompt::URI {
            let open = self.date_prompt.as_ref()?;
            let index = LineIndex::new(&open.text);
            let only = range.map(|range| index.text_range(&open.text, range, encoding));
            let data = highlight::encode(&open.tokens, &open.text, &index, encoding, only);
            return Some(SemanticTokens { result_id: None, data });
        }
        if let Some(view) = self.view_at(uri) {
            let only = range.map(|range| view.index.text_range(&view.text, range, encoding));
            let data = highlight::encode(&view.tokens, &view.text, &view.index, encoding, only);
            return Some(SemanticTokens { result_id: None, data });
        }
        let document = self.document(uri)?;
        let tokens = highlight::tokens(&document.syntax(), document.parse.todo_keywords(), self.conceal);
        let only = range.map(|range| document.index.text_range(&document.text, range, encoding));
        let data = highlight::encode(&tokens, &document.text, &document.index, encoding, only);
        Some(SemanticTokens { result_id: None, data })
    }

    fn semantic_tokens_full(&mut self, params: SemanticTokensParams) -> Option<SemanticTokens> {
        self.semantic_tokens(&params.text_document.uri, None)
    }

    fn semantic_tokens_range(&mut self, params: SemanticTokensRangeParams) -> Option<SemanticTokens> {
        self.semantic_tokens(&params.text_document.uri, Some(params.range))
    }

    fn folding_ranges(&mut self, params: FoldingRangeParams) -> Option<Vec<lsp_types::FoldingRange>> {
        let document = self.document(&params.text_document.uri)?;
        Some(folding::folding_ranges(&document.syntax(), &document.text, &document.index, self.separator_lines))
    }

    /// `org-indent-mode`'s prefixes as inlay hints of blanks at the start of
    /// lines, for a client that does not show `org-lsp/virtualIndent`.
    /// Hidden stars and the prefixes of wrapped lines cannot be shown so.
    fn inlay_hints(&mut self, params: lsp_types::InlayHintParams) -> Option<Vec<lsp_types::InlayHint>> {
        let document = self.document(&params.text_document.uri)?;
        let text = &document.text;
        let (first, last) = (params.range.start.line as usize, params.range.end.line as usize);
        let hint = |line: u32, character: u32, label: String| lsp_types::InlayHint {
            position: Position::new(line, character),
            label: lsp_types::InlayHintLabel::String(label),
            kind: None,
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let mut hints = Vec::new();
        // `org-indent-mode`'s prefixes, for a client that does not show
        // them otherwise.
        if !self.client.virtual_indent && self.display.indented(text) {
            let adapt = self.display.adapt(text, self.indent.adapt);
            hints.extend(
                org_core::indent::virtual_indentation(text, true, false, adapt)
                    .into_iter()
                    .filter(|line| line.prefix > 0 && (first..=last).contains(&line.line))
                    .map(|line| hint(line.line as u32, 0, " ".repeat(line.prefix))),
            );
        }
        // `org-clock-display`: the time after each headline, its line
        // filled with dots up to column 60, as Emacs's overlay shows it.
        for (line, minutes) in self.clock_display.get(params.text_document.uri.as_str()).into_iter().flatten() {
            if !(first..=last).contains(&(*line as usize)) {
                continue;
            }
            let shown = text.lines().nth(*line as usize).unwrap_or("");
            let width: usize = shown.chars().map(org_core::text::char_width).sum();
            let start: usize = text.split_inclusive('\n').take(*line as usize).map(str::len).sum();
            let end = document.index.position(text, start + shown.len(), self.client.encoding);
            let label = format!("{}{:>10} ", "·".repeat(60usize.saturating_sub(width)), clock::duration(*minutes));
            hints.push(hint(*line, end.character, label));
        }
        Some(hints)
    }

    fn document_symbols(&mut self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        let uri = &params.text_document.uri;
        let document = self.document(uri)?;
        let (root, text, index, encoding) = (document.syntax(), &document.text, &document.index, self.client.encoding);
        Some(if self.client.hierarchical_symbols {
            DocumentSymbolResponse::Nested(symbols::document_symbols(&root, text, index, encoding))
        } else {
            DocumentSymbolResponse::Flat(symbols::symbol_information(&root, text, index, encoding, uri))
        })
    }

    /// The edit as a workspace edit: versioned when the client supports
    /// that form, which lets it reject an edit to text that has changed.
    fn workspace_edit(&self, uri: &Uri, version: Option<i32>, edits: Vec<TextEdit>) -> WorkspaceEdit {
        self.workspace_edits(vec![(uri.clone(), version, edits)])
    }

    /// One edit over several documents, each with its version when the
    /// editor has it open.
    fn workspace_edits(&self, documents: Vec<(Uri, Option<i32>, Vec<TextEdit>)>) -> WorkspaceEdit {
        if self.client.document_changes {
            let edits = documents
                .into_iter()
                .map(|(uri, version, edits)| TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
                .collect();
            WorkspaceEdit { document_changes: Some(lsp_types::DocumentChanges::Edits(edits)), ..Default::default() }
        } else {
            #[allow(clippy::mutable_key_type)]
            let changes = documents.into_iter().map(|(uri, _, edits)| (uri, edits)).collect();
            WorkspaceEdit { changes: Some(changes), ..Default::default() }
        }
    }

    /// The text edits that turn `old` into `new`.
    fn text_edits(&self, old: &str, new: &str) -> Vec<TextEdit> {
        let index = LineIndex::new(old);
        let encoding = self.client.encoding;
        actions::edits_between(old, new)
            .into_iter()
            .map(|edit| TextEdit { range: index.range(old, edit.range, encoding), new_text: edit.new_text })
            .collect()
    }

    /// No highlights: the request says where the cursor is, which the
    /// agenda's keys want.
    fn document_highlights(&mut self, params: lsp_types::DocumentHighlightParams) -> Option<Vec<DocumentHighlight>> {
        let position = params.text_document_position_params;
        self.saw_cursor(&position.text_document.uri, position.position);
        None
    }

    /// The cursor is at `position` of `uri`: if that is the agenda, its
    /// commands from the port go there, and the port's script to this
    /// server.
    fn saw_cursor(&mut self, uri: &Uri, position: Position) {
        if !self.is_view(uri) {
            return;
        }
        self.agenda_cursor = Some(position);
        self.claim_script();
    }

    /// Make the port's script send to this server.
    fn claim_script(&mut self) {
        let (Some(port), Some(path)) = (&self.port, &self.client.command_script) else { return };
        if let Err(error) = port.claim(path) {
            self.show_message(MessageType::ERROR, format!("Cannot write {}: {error}", path.display()));
        }
    }

    fn code_actions(&mut self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        if self.client.command_actions {
            self.saw_cursor(uri, params.range.start);
        }
        let only = params.context.only.as_deref();
        let context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
        let as_action = |id: &'static str| {
            let title = commands::title(id).to_string();
            CodeActionOrCommand::CodeAction(CodeAction {
                title: title.clone(),
                kind: Some(CodeActionKind::new(id)),
                command: Some(lsp_types::Command {
                    title,
                    command: id.to_string(),
                    arguments: Some(vec![context.clone()]),
                }),
                ..Default::default()
            })
        };
        // With `command_actions`, what an editor runs from keys.
        let more = self.client.command_actions;
        if self.is_view(uri) && more {
            // The agenda's file is read-only, where Zed runs no code
            // action; its keys go through the command port instead.
            return Some(Vec::new());
        }
        if self.is_view(uri) {
            // On an agenda line the actions are commands: the server
            // edits the entry's file and rebuilds the agenda.
            let (_, line) = self.view_line(uri, params.range.start.line)?;
            let entry = line.entry.as_ref()?;
            let dated = entry.marker.is_some();
            let running = self.running.is_some();
            let ids = [
                commands::TODO_NEXT,
                commands::TODO_PREVIOUS,
                commands::PRIORITY_UP,
                commands::PRIORITY_DOWN,
                commands::DATE_LATER,
                commands::DATE_EARLIER,
                commands::CLOCK_IN,
                commands::CLOCK_OUT,
                commands::ARCHIVE_SUBTREE,
                commands::AGENDA_GOTO,
                commands::AGENDA_GOTO_OTHER,
            ];
            let actions = ids
                .into_iter()
                .filter(|id| dated || !matches!(*id, commands::DATE_LATER | commands::DATE_EARLIER))
                .filter(|id| running || *id != commands::CLOCK_OUT)
                .filter(|id| actions::requested(id, only))
                .map(as_action)
                .collect();
            return Some(actions);
        }
        // The capture buffer: file, give up or refile.
        if more && self.capture.as_ref().is_some_and(|capture| capture.uri == *uri) {
            let ids = [commands::CAPTURE_FINISH, commands::CAPTURE_ABORT, commands::CAPTURE_REFILE];
            return Some(ids.into_iter().filter(|id| actions::requested(id, only)).map(as_action).collect());
        }
        let document = self.document(uri)?;
        let encoding = self.client.encoding;
        let offset = document.index.offset(&document.text, params.range.start, encoding);
        let mut settings = self.edit_settings.clone();
        settings.todo.adapt = self.adapt_in(&document.text);
        let found = actions::actions(&document.text, &document.parse, offset, only, &settings, self.date_time());
        let to_lsp = |action: &actions::Action| {
            let edits = action
                .edits
                .iter()
                .map(|edit| TextEdit {
                    range: document.index.range(&document.text, edit.range, encoding),
                    new_text: edit.new_text.clone(),
                })
                .collect();
            let edit = self.workspace_edit(uri, Some(document.version), edits);
            CodeActionOrCommand::CodeAction(action.to_lsp(edit))
        };
        let mut result: Vec<CodeActionOrCommand> = found.iter().map(to_lsp).collect();
        // On an unknown citation key, the close keys to put instead, as a
        // click on it offers them in Emacs (`org-cite-basic--set-keymap`).
        for diagnostic in &params.context.diagnostics {
            let keys = diagnostic.data.as_ref().and_then(|data| data.get("citationKeys")).and_then(Value::as_array);
            for key in keys.into_iter().flatten().filter_map(Value::as_str) {
                let edits = vec![TextEdit { range: diagnostic.range, new_text: format!("@{key}") }];
                result.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Replace with @{key}"),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    edit: Some(self.workspace_edit(uri, Some(document.version), edits)),
                    ..Default::default()
                }));
            }
        }
        // On a headline: promoting, demoting and moving it, as edits, where
        // they would do something.
        if org_core::structure::at_heading(&document.text, offset) {
            use org_core::structure;
            let text = &document.text;
            let keywords = document.parse.todo_keywords();
            let structure_settings =
                structure::Settings { tags_column: self.refile.2.tags_column, indent: self.indent_in(text) };
            let tried = [
                (commands::META_LEFT, structure::change_levels(text, offset, -1, false, keywords, &structure_settings)),
                (commands::META_RIGHT, structure::change_levels(text, offset, 1, false, keywords, &structure_settings)),
                (
                    commands::SHIFT_META_LEFT,
                    structure::change_levels(text, offset, -1, true, keywords, &structure_settings),
                ),
                (
                    commands::SHIFT_META_RIGHT,
                    structure::change_levels(text, offset, 1, true, keywords, &structure_settings),
                ),
                (commands::META_UP, structure::move_subtree(text, offset, -1)),
                (commands::META_DOWN, structure::move_subtree(text, offset, 1)),
            ];
            for (id, outcome) in tried {
                let Ok(outcome) = outcome else { continue };
                if !actions::requested(id, only) || outcome.text == *text {
                    continue;
                }
                let edits = actions::edits_between(text, &outcome.text)
                    .into_iter()
                    .map(|edit| TextEdit {
                        range: document.index.range(text, edit.range, encoding),
                        new_text: edit.new_text,
                    })
                    .collect();
                result.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: commands::title(id).to_string(),
                    kind: Some(CodeActionKind::new(id)),
                    edit: Some(self.workspace_edit(uri, Some(document.version), edits)),
                    ..Default::default()
                }));
            }
        }
        // On a list item: outdenting, indenting and moving it.
        if !org_core::structure::at_heading(&document.text, offset) && org_core::list::at_item(&document.text, offset) {
            let text = &document.text;
            let keywords = document.parse.todo_keywords();
            let ids = [
                commands::META_LEFT,
                commands::META_RIGHT,
                commands::SHIFT_META_LEFT,
                commands::SHIFT_META_RIGHT,
                commands::META_UP,
                commands::META_DOWN,
            ];
            for id in ids {
                if !actions::requested(id, only) {
                    continue;
                }
                let Ok(edited) = item_structure(id, text, offset, keywords, self.refile.2.tags_column) else {
                    continue;
                };
                if edited.text == *text {
                    continue;
                }
                let edits = actions::edits_between(text, &edited.text)
                    .into_iter()
                    .map(|edit| TextEdit {
                        range: document.index.range(text, edit.range, encoding),
                        new_text: edit.new_text,
                    })
                    .collect();
                result.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: commands::title(id).to_string(),
                    kind: Some(CodeActionKind::new(id)),
                    edit: Some(self.workspace_edit(uri, Some(document.version), edits)),
                    ..Default::default()
                }));
            }
        }
        // In a table: moving, inserting and deleting its rows and columns,
        // rules, a blank field.
        if at_org_table(&document.text, &document.parse.syntax(), offset) {
            let text = &document.text;
            let ids = [
                commands::META_LEFT,
                commands::META_RIGHT,
                commands::META_UP,
                commands::META_DOWN,
                commands::SHIFT_META_LEFT,
                commands::SHIFT_META_RIGHT,
                commands::SHIFT_META_UP,
                commands::SHIFT_META_DOWN,
                commands::TABLE_INSERT_ROW_BELOW,
                commands::TABLE_HLINE,
                commands::TABLE_HLINE_ABOVE,
                commands::TABLE_BLANK_FIELD,
            ];
            for id in ids {
                let Some(command) = table_command(id).filter(|_| actions::requested(id, only)) else { continue };
                let Ok(done) = org_core::table::run(text, offset, command) else { continue };
                if done.text == *text {
                    continue;
                }
                let edits = actions::edits_between(text, &done.text)
                    .into_iter()
                    .map(|edit| TextEdit {
                        range: document.index.range(text, edit.range, encoding),
                        new_text: edit.new_text,
                    })
                    .collect();
                result.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: commands::title(id).to_string(),
                    kind: Some(CodeActionKind::new(id)),
                    edit: Some(self.workspace_edit(uri, Some(document.version), edits)),
                    ..Default::default()
                }));
            }
        }
        // Clocking and archiving, as commands: they may change another file
        // too.
        let in_entry = Self::heading_above(&document.text, offset).is_some();
        let running = self.running.is_some();
        let in_code = in_src_block(&document.parse.syntax(), offset);
        for (id, offered) in [
            (commands::TANGLE, in_code),
            (commands::CLOCK_IN, in_entry),
            (commands::CLOCK_OUT, running),
            (commands::CLOCK_CANCEL, running),
            (commands::ARCHIVE_SUBTREE, in_entry),
            (commands::ARCHIVE_SIBLING, in_entry),
            (commands::ARCHIVE_TAG, in_entry),
            (commands::TODO, more && in_entry),
            (commands::PRIORITY, more && in_entry),
            (commands::REFILE, more && in_entry),
            (commands::SORT, more),
            (commands::CLOCK_GOTO, more && running),
            (commands::CLOCK_IN_LAST, more && !self.clock_history.is_empty()),
            (commands::CLOCK_REPORT, more),
            (commands::DBLOCK_UPDATE, more && org_core::clocktable::in_block(&document.text, offset)),
            (commands::AGENDA_LIST, more),
            (commands::AGENDA_TODOS, more),
            (commands::AGENDA_DISPATCH, more),
            (commands::CAPTURE, more),
        ] {
            if offered && actions::requested(id, only) {
                result.push(as_action(id));
            }
        }
        // The keys that insert a heading or an item: the server applies
        // the edit and moves the cursor to it.
        if more {
            let mut applied = context.clone();
            applied["apply"] = json!(true);
            // And the subtree to and from the server's own clipboard.
            for (id, offered) in [
                (commands::META_RETURN, true),
                (commands::INSERT_TODO_HEADING, true),
                (commands::INSERT_HEADING_AFTER, true),
                (commands::INSERT_TODO_HEADING_AFTER, true),
                (commands::CUT_SUBTREE, in_entry),
                (commands::COPY_SUBTREE, in_entry),
                (commands::PASTE_SUBTREE, self.kill.is_some()),
            ] {
                if offered && actions::requested(id, only) {
                    let title = commands::title(id).to_string();
                    result.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: title.clone(),
                        kind: Some(CodeActionKind::new(id)),
                        command: Some(lsp_types::Command {
                            title,
                            command: id.to_string(),
                            arguments: Some(vec![applied.clone()]),
                        }),
                        ..Default::default()
                    }));
                }
            }
        }
        Some(result)
    }

    /// Completion as Org's `pcomplete` offers it, and file names in links.
    /// `org-indent-line` on the line the cursor is on once a key was typed:
    /// a new line, after `RET`.
    fn on_type_formatting(&mut self, params: lsp_types::DocumentOnTypeFormattingParams) -> Option<Vec<TextEdit>> {
        let position = params.text_document_position;
        let encoding = self.client.encoding;
        let document = self.document(&position.text_document.uri)?;
        let (text, index) = (&document.text, &document.index);
        let offset = index.offset(text, Position::new(position.position.line, 0), encoding);
        let edit = org_core::indent::indent_line(text, offset, &self.indent_in(text));
        Some(edit.into_iter().map(|edit| indent_edit(text, index, encoding, edit)).collect())
    }

    /// `org-indent-region` over the lines of the range, then the tables
    /// that have a line there aligned (`org-table-align`) and the tags of its
    /// headlines put in their column.
    fn range_formatting(&mut self, params: lsp_types::DocumentRangeFormattingParams) -> Option<Vec<TextEdit>> {
        let encoding = self.client.encoding;
        let settings = org_core::format::Settings { tags_column: self.refile.2.tags_column };
        let indent = self.indent_in(&self.document(&params.text_document.uri)?.text.clone());
        let document = self.document(&params.text_document.uri)?;
        let (text, index) = (&document.text, &document.index);
        let start = index.offset(text, params.range.start, encoding);
        let end = index.offset(text, params.range.end, encoding);
        let indented = org_core::indent::apply(text, &org_core::indent::indent_region(text, start, end, &indent));
        // A range that ends at the start of a line leaves that line out.
        let first = text[..start].matches('\n').count();
        let last = text[..end].matches('\n').count();
        let last = if end > start && text[..end].ends_with('\n') { last - 1 } else { last };
        let changes = org_core::format::format_lines(
            &indented,
            document.parse.todo_keywords(),
            &settings,
            first..=last.max(first),
        );
        let formatted = org_core::format::apply(&indented, &changes);
        Some(line_edits(text, index, encoding, &formatted))
    }

    /// Every table aligned (`org-table-align`) and every headline's tags
    /// in their column (`org-align-tags`).
    fn formatting(&mut self, params: lsp_types::DocumentFormattingParams) -> Option<Vec<TextEdit>> {
        let encoding = self.client.encoding;
        let settings = org_core::format::Settings { tags_column: self.refile.2.tags_column };
        let document = self.document(&params.text_document.uri)?;
        let (text, index) = (&document.text, &document.index);
        let changes = org_core::format::format(text, document.parse.todo_keywords(), &settings);
        let formatted = org_core::format::apply(text, &changes);
        Some(line_edits(text, index, encoding, &formatted))
    }

    fn completion(&mut self, params: lsp_types::CompletionParams) -> Option<lsp_types::CompletionResponse> {
        let position = params.text_document_position;
        let encoding = self.client.encoding;
        let uri = position.text_document.uri;
        // The tags of the agenda files, read only for a headline.
        let on_headline = self.document(&uri).is_some_and(|document| {
            let offset = document.index.offset(&document.text, position.position, encoding);
            let line_start = document.text[..offset].rfind('\n').map_or(0, |at| at + 1);
            document.text[line_start..].starts_with('*')
        });
        let agenda_tags = if self.all_agenda_tags && on_headline { self.agenda_tags() } else { Vec::new() };
        // The bibliography, read only after an `@` in a citation.
        let citation_keys: Vec<(String, String)> = {
            let document = self.document(&uri)?;
            let offset = document.index.offset(&document.text, position.position, encoding);
            let line_start = document.text[..offset].rfind('\n').map_or(0, |at| at + 1);
            if completion::citation_key_start(&document.text[line_start..offset]).is_some() {
                let text = document.text.clone();
                self.bibliography(&uri, &text)
                    .iter()
                    .flat_map(|file| file.entries.iter())
                    .map(|entry| (entry.key.clone(), entry.completion().unwrap_or_default()))
                    .collect()
            } else {
                Vec::new()
            }
        };
        let document = self.document(&uri)?;
        let offset = document.index.offset(&document.text, position.position, encoding);
        let directory = uri::to_path(&uri).and_then(|path| path.parent().map(Path::to_path_buf));
        let documents = &self.documents;
        let list_directory = |path: &Path| -> Vec<(String, bool)> {
            let Ok(entries) = std::fs::read_dir(path) else { return Vec::new() };
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let folder = entry.path().is_dir();
                    (!name.starts_with('.')).then_some((name, folder))
                })
                .take(1000)
                .collect()
        };
        let read_file = |path: &Path| -> Option<String> {
            let open =
                uri::from_path(path).and_then(|uri| documents.get(uri.as_str()).map(|document| document.text.clone()));
            open.or_else(|| std::fs::read_to_string(path).ok())
        };
        let context = completion::Context {
            keywords: document.parse.todo_keywords(),
            more_tags: &agenda_tags,
            directory: directory.as_deref(),
            list_directory: &list_directory,
            read_file: &read_file,
            citation_keys: &citation_keys,
        };
        let (start, items) = completion::complete(&document.text, offset, &context)?;
        let range = Range::new(document.index.position(&document.text, start, encoding), position.position);
        let items = items
            .into_iter()
            .enumerate()
            .map(|(rank, candidate)| lsp_types::CompletionItem {
                // In the order given: the file's tags before the others.
                sort_text: Some(format!("{rank:05}")),
                kind: Some(match candidate.kind {
                    completion::Kind::Keyword => lsp_types::CompletionItemKind::KEYWORD,
                    completion::Kind::Tag => lsp_types::CompletionItemKind::ENUM_MEMBER,
                    completion::Kind::Name => lsp_types::CompletionItemKind::PROPERTY,
                    completion::Kind::Value => lsp_types::CompletionItemKind::VALUE,
                    completion::Kind::LinkType => lsp_types::CompletionItemKind::MODULE,
                    completion::Kind::File => lsp_types::CompletionItemKind::FILE,
                    completion::Kind::Folder => lsp_types::CompletionItemKind::FOLDER,
                    completion::Kind::Heading => lsp_types::CompletionItemKind::REFERENCE,
                    completion::Kind::Entity => lsp_types::CompletionItemKind::CONSTANT,
                    completion::Kind::Citation => lsp_types::CompletionItemKind::REFERENCE,
                }),
                // Matched against what is typed too: `doe` finds a key by
                // its author.
                filter_text: candidate.detail.as_ref().map(|detail| format!("{} {detail}", candidate.label)),
                detail: candidate.detail,
                text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text: candidate.insert })),
                label: candidate.label,
                ..Default::default()
            })
            .collect();
        Some(lsp_types::CompletionResponse::Array(items))
    }

    /// The tags of every agenda file (`org-global-tags-completion-table`).
    fn agenda_tags(&mut self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .agenda_files()
            .iter()
            .flat_map(|(_, held)| held.outline().entries.iter().flat_map(|entry| entry.tags.iter()))
            .map(|tag| tag.name.clone())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// From an agenda line to the entry it shows.
    fn definition(&mut self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let position = params.text_document_position_params;
        let uri = &position.text_document.uri;
        if self.is_view(uri) {
            let reference = self.view_line(uri, position.position.line)?.1.entry.clone()?;
            return self.locate(&reference).map(GotoDefinitionResponse::Scalar);
        }
        // In an Org file: where a link (or a footnote) leads, when that is
        // a place in a file.
        let document = self.document(uri)?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, position.position, self.client.encoding);
        let opened = open::at_point(&text, &document.parse, offset, None).ok()?;
        // A citation: the entries of its keys.
        if let Open::Citation(keys) = &opened {
            let files = self.bibliography(uri, &text);
            let locations: Vec<Location> =
                keys.iter().filter_map(|key| Self::cite_location(&files, key, self.client.encoding).ok()).collect();
            return match locations.len() {
                0 => None,
                1 => locations.into_iter().next().map(GotoDefinitionResponse::Scalar),
                _ => Some(GotoDefinitionResponse::Array(locations)),
            };
        }
        self.open_location(uri, &text, &opened).ok().flatten().map(GotoDefinitionResponse::Scalar)
    }

    /// Over an agenda line, the entry it shows: its headline and its text,
    /// each drawer folded, as Org in a fenced block of Markdown.
    fn hover(&mut self, params: lsp_types::HoverParams) -> Option<lsp_types::Hover> {
        let position = params.text_document_position_params;
        if !self.is_view(&position.text_document.uri) {
            return self.citation_hover(&position.text_document.uri, position.position);
        }
        let text = self.entry_preview(&position.text_document.uri, position.position.line)?;
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: org_core::preview::markdown(&text),
            }),
            range: None,
        })
    }

    /// The entry of agenda line `line`, as a popup shows it: its headline
    /// and its text, each drawer folded.
    fn entry_preview(&mut self, uri: &Uri, line: u32) -> Option<String> {
        if !self.is_view(uri) {
            return None;
        }
        let reference = self.view_line(uri, line)?.1.entry.clone()?;
        self.with_source(&reference.uri, |text, _, _, outline, _| {
            views::resolve(&reference, outline)
                .map(|entry| org_core::preview::entry(text, outline.entries[entry].offset))
        })
        .flatten()
    }

    /// `org-lsp/preview`: the entry of an agenda line, as the hover shows
    /// it, with its semantic tokens, for an editor that shows it itself in
    /// a popup of its own (Vim): `{ "text", "data" }`, or null.
    fn preview(&mut self, params: &Value) -> Value {
        let Ok(position) = serde_json::from_value::<lsp_types::TextDocumentPositionParams>(params.clone()) else {
            return Value::Null;
        };
        let Some(text) = self.entry_preview(&position.text_document.uri, position.position.line) else {
            return Value::Null;
        };
        let parse = org_syntax::parse(&text, &self.todo);
        let tokens = highlight::tokens(&parse.syntax(), parse.todo_keywords(), self.conceal);
        let index = LineIndex::new(&text);
        let data = highlight::encode(&tokens, &text, &index, self.client.encoding, None);
        let tokens = serde_json::to_value(SemanticTokens { result_id: None, data }).unwrap_or_default();
        json!({ "text": text, "data": tokens["data"] })
    }

    /// `org-open-at-point` (`C-c C-o`) in an Org file, and
    /// `org-agenda-open-link` on an agenda line: the links of its entry.
    fn open_at_point(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let (uri, text, offset) = if self.is_view(uri) {
            let reference = self.entry_at(Some(context)).ok_or("No link to open here")?;
            let found = self
                .with_source(&reference.uri, |text, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| (text.to_string(), outline.entries[entry].offset))
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            (reference.uri.clone(), found.0, found.1)
        } else {
            let document = self.document(uri).ok_or("The document is not open")?;
            let offset = document.index.offset(&document.text, context.position, self.client.encoding);
            (uri.clone(), document.text.clone(), offset)
        };
        let parse = org_syntax::parse(&text, &self.todo);
        let answer = context.input.as_deref().and_then(|input| input.chars().next());
        let opened = open::at_point(&text, &parse, offset, answer)?;
        match &opened {
            Open::External(url) => {
                let target = url.parse::<Uri>().map_err(|_| format!("Cannot open {url}"))?;
                self.show_external(target);
            }
            Open::File { path, system: true, .. } => {
                let target =
                    self.link_path(&uri, path).and_then(|path| uri::from_path(&path)).ok_or("Cannot open the file")?;
                self.show_external(target);
            }
            Open::Agenda { date, span } => {
                let span = if *span == 1 { Span::Day } else { Span::Days(*span) };
                self.open_view_at(Query::Agenda { start: Some(date.to_day()), span }, Reveal::Home)?;
            }
            Open::Tags(tag) => {
                self.open_view_at(Query::Tags { matching: tag.clone(), todo_only: false }, Reveal::Home)?;
            }
            Open::Choose(choices) => {
                let choices: Vec<Value> = choices
                    .iter()
                    .map(|choice| json!({ "key": choice.key.to_string(), "label": choice.label }))
                    .collect();
                return Ok(json!({ "input": {
                    "kind": "key",
                    "prompt": "Select link to open",
                    "value": "",
                    "choices": choices,
                } }));
            }
            Open::Nothing(message) => self.message = Some(message.clone()),
            // `org-cite-basic-goto`: the entry of the key, picked among
            // the citation's when it has more than one.
            Open::Citation(keys) => {
                let key = match (keys.as_slice(), context.input.as_deref()) {
                    ([], _) => return Err("No citation key here".into()),
                    ([key], _) => key.clone(),
                    (_, Some(answer)) => answer.to_string(),
                    _ => {
                        let choices: Vec<Value> = keys.iter().map(|key| json!({ "key": key, "label": key })).collect();
                        return Ok(json!({ "input": {
                            "kind": "choice",
                            "prompt": "Select citation key",
                            "value": "",
                            "choices": choices,
                        } }));
                    }
                };
                let files = self.bibliography(&uri, &text);
                let location = Self::cite_location(&files, &key, self.client.encoding).map_err(Refusal::Failed)?;
                let params = ShowDocumentParams {
                    uri: location.uri,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(location.range),
                };
                self.send_request::<ShowDocument>(params, None);
            }
            Open::Here(_) | Open::File { .. } | Open::Id(_) => {
                let location = self.open_location(&uri, &text, &opened)?.ok_or("No link found")?;
                let params = ShowDocumentParams {
                    uri: location.uri,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(location.range),
                };
                self.send_request::<ShowDocument>(params, None);
            }
        }
        Ok(Value::Null)
    }

    /// Ask the client to have the system open `target` (a URL, a PDF).
    fn show_external(&mut self, target: Uri) {
        let params = ShowDocumentParams { uri: target, external: Some(true), take_focus: None, selection: None };
        self.send_request::<ShowDocument>(params, None);
    }

    /// A link's path as a file: relative to the file `from`, `~` for the
    /// home directory.
    fn link_path(&self, from: &Uri, path: &str) -> Option<PathBuf> {
        let path = match path.strip_prefix('~') {
            Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest.trim_start_matches('/')),
            None => PathBuf::from(path),
        };
        if path.is_absolute() {
            return Some(path);
        }
        Some(uri::to_path(from)?.parent()?.join(path))
    }

    /// The text of the file at `path`: as the editor has it when open
    /// there, else from disk.
    fn file_text(&self, path: &Path) -> Option<(Uri, String)> {
        let open = self
            .documents
            .iter()
            .find(|(key, _)| key.parse::<Uri>().ok().and_then(|key| uri::to_path(&key)).is_some_and(|key| key == path));
        if let Some((key, document)) = open {
            return Some((key.parse().ok()?, document.text.clone()));
        }
        let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned();
        Some((uri::from_path(path)?, text))
    }

    /// Where an opened link leads as a place in a file: in `text` itself
    /// (of `uri`), in another file at its search option, or the entry
    /// with an ID among the open files and the agenda files. None for what
    /// is no place (a URL, an agenda).
    fn open_location(&mut self, uri: &Uri, text: &str, opened: &Open) -> std::result::Result<Option<Location>, String> {
        let encoding = self.client.encoding;
        let at = |target: Uri, text: &str, offset: usize| {
            let position = LineIndex::new(text).position(text, offset, encoding);
            Location { uri: target, range: Range::new(position, position) }
        };
        match opened {
            Open::Here(offset) => Ok(Some(at(uri.clone(), text, *offset))),
            Open::File { path, search, system: false } => {
                let path = self.link_path(uri, path).ok_or("Cannot open the file")?;
                let Some(search) = search else {
                    let target = uri::from_path(&path).ok_or("Cannot open the file")?;
                    return Ok(Some(Location { uri: target, range: Range::default() }));
                };
                let (target, text) = self.file_text(&path).ok_or_else(|| format!("Cannot read {}", path.display()))?;
                let org = path.extension().is_some_and(|extension| extension == "org" || extension == "org_archive");
                let parse = org.then(|| org_syntax::parse(&text, &self.todo));
                let offset = open::search_in_file(&text, parse.as_ref(), search)?;
                Ok(Some(at(target, &text, offset)))
            }
            Open::Id(id) => {
                // `org-id-find`: the files open in the editor, then the
                // agenda files.
                let mut paths: Vec<PathBuf> =
                    self.documents.keys().filter_map(|key| uri::to_path(&key.parse().ok()?)).collect();
                paths.sort();
                for (file, _) in self.agenda_files() {
                    if let Some(path) = uri::to_path(&file).filter(|path| !paths.contains(path)) {
                        paths.push(path);
                    }
                }
                for path in paths {
                    let Some((target, text)) = self.file_text(&path) else { continue };
                    if let Some(offset) = open::find_id(&org_syntax::parse(&text, &self.todo), id) {
                        return Ok(Some(at(target, &text, offset)));
                    }
                }
                Err(format!("Cannot find entry with ID \"{id}\""))
            }
            _ => Ok(None),
        }
    }

    fn text_document_content(&mut self, params: &Value) -> Value {
        let uri = params.get("uri").and_then(Value::as_str);
        if uri == Some(prompt::URI) {
            return json!({ "text": self.date_prompt.as_ref().map(|open| open.text.clone()).unwrap_or_default() });
        }
        match &self.view {
            Some(open) if uri == Some(open.uri.as_str()) => json!({ "text": open.view.text }),
            // The client restored an agenda from an earlier session.
            _ if uri == Some(views::VIRTUAL_URI) => {
                let query = Query::Agenda { start: None, span: self.agenda.span };
                self.set_view(query);
                json!({ "text": self.view.as_ref().map(|open| open.view.text.clone()).unwrap_or_default() })
            }
            _ => Value::Null,
        }
    }

    // ----- agenda files

    /// The text, positions, tree and outline of the file at `uri`: from
    /// the editor when it is open there, from disk otherwise.
    fn with_source<T>(
        &mut self,
        uri: &Uri,
        action: impl FnOnce(&str, &LineIndex, &Parse, &Outline, Option<i32>) -> T,
    ) -> Option<T> {
        if let Some((document, version)) = self.staged_document(uri) {
            let outline = Outline::with_inheritance(
                &document.parse,
                &document.text,
                &uri::file_stem(uri),
                &self.workspace.inheritance,
            );
            return Some(action(&document.text, &document.index, &document.parse, &outline, version));
        }
        if let Some(document) = self.documents.get(uri.as_str()) {
            let outline = Outline::with_inheritance(
                &document.parse,
                &document.text,
                &uri::file_stem(uri),
                &self.workspace.inheritance,
            );
            return Some(action(&document.text, &document.index, &document.parse, &outline, Some(document.version)));
        }
        let source = self.workspace.read(&uri::to_path(uri)?, &self.todo)?;
        Some(action(&source.text, &source.index, &source.parse, &source.outline, None))
    }

    /// The agenda files with their outlines, in agenda order.
    fn agenda_files(&mut self) -> Vec<(Uri, Held)> {
        let inheritance = self.workspace.inheritance.clone();
        let open = |document: &Document, uri: &Uri| {
            Held::Open(Box::new(Outline::with_inheritance(
                &document.parse,
                &document.text,
                &uri::file_stem(uri),
                &inheritance,
            )))
        };
        if !self.workspace.is_configured() {
            // Nothing says where the agenda files are: use the files that
            // are open (not an entry shown in a popup, `org-preview:`).
            let mut keys: Vec<&String> = self.documents.keys().collect();
            keys.sort();
            return keys
                .into_iter()
                .filter_map(|key| {
                    let uri: Uri = key.parse().ok()?;
                    uri::to_path(&uri)?;
                    Some((uri.clone(), open(&self.documents[key], &uri)))
                })
                .collect();
        }
        // Open documents by path: the client may spell a URI differently.
        let by_path: HashMap<PathBuf, &String> =
            self.documents.keys().filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, key))).collect();
        let mut files = Vec::new();
        for path in self.workspace.files() {
            if let Some(key) = by_path.get(&path) {
                if let Ok(uri) = key.parse::<Uri>() {
                    files.push((uri.clone(), open(&self.documents[*key], &uri)));
                }
            } else if let (Some(uri), Some(source)) = (uri::from_path(&path), self.workspace.read(&path, &self.todo)) {
                files.push((uri, Held::Disk(source)));
            }
        }
        if self.agenda_archives == ArchivesMode::Files {
            let open: HashMap<PathBuf, String> = by_path.into_iter().map(|(path, key)| (path, key.clone())).collect();
            files = self.with_archive_files(files, &open);
        }
        files
    }

    /// `org-add-archive-files`: each agenda file followed by the files it
    /// archives to that exist, each file once.
    fn with_archive_files(&mut self, files: Vec<(Uri, Held)>, open: &HashMap<PathBuf, String>) -> Vec<(Uri, Held)> {
        let mut paths: Vec<PathBuf> = files.iter().filter_map(|(uri, _)| uri::to_path(uri)).collect();
        let mut more: Vec<(usize, PathBuf)> = Vec::new();
        for (index, (uri, _)) in files.iter().enumerate() {
            let Some(path) = uri::to_path(uri) else { continue };
            for file in self.archives_of(&path, open) {
                if !paths.contains(&file) {
                    paths.push(file.clone());
                    more.push((index, file));
                }
            }
        }
        let mut out = Vec::new();
        let mut more = more.into_iter().peekable();
        for (index, file) in files.into_iter().enumerate() {
            out.push(file);
            while let Some((_, path)) = more.next_if(|(after, _)| *after == index) {
                out.extend(self.held(&path, open));
            }
        }
        out
    }

    /// `org-all-archive-files`: the files the file at `path` archives to
    /// that exist.
    fn archives_of(&mut self, path: &Path, open: &HashMap<PathBuf, String>) -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let location = self.archive.0.location.clone();
        let text = match open.get(path) {
            Some(key) => self.documents[key].text.clone(),
            None => match self.workspace.read(path, &self.todo) {
                Some(source) => source.text.clone(),
                None => return Vec::new(),
            },
        };
        let context = archive::Context { path, home: home.as_deref(), now: self.date_time() };
        archive::archive_files(&text, &self.todo, &context, &location)
            .into_iter()
            .filter(|file| file.exists())
            .collect()
    }

    /// The file at `path` for a view: as the editor has it when open.
    fn held(&mut self, path: &Path, open: &HashMap<PathBuf, String>) -> Option<(Uri, Held)> {
        match open.get(path) {
            Some(key) => key.parse::<Uri>().ok().map(|uri| {
                let document = &self.documents[key];
                let outline = Outline::with_inheritance(
                    &document.parse,
                    &document.text,
                    &uri::file_stem(&uri),
                    &self.workspace.inheritance,
                );
                (uri, Held::Open(Box::new(outline)))
            }),
            None => match (uri::from_path(path), self.workspace.read(path, &self.todo)) {
                (Some(uri), Some(source)) => Some((uri, Held::Disk(source))),
                _ => None,
            },
        }
    }

    /// `org-agenda-text-search-extra-files`: the files a search view goes
    /// through besides the agenda files, added after `files`, and the
    /// order of the search; nothing when there are none.
    fn search_files(&mut self, files: &mut Vec<(Uri, Held)>) -> Option<views::SearchFiles> {
        let extra = self.agenda_options.text_search_extra_files.clone().unwrap_or_default();
        if extra.is_empty() {
            return None;
        }
        let open: HashMap<PathBuf, String> = self
            .documents
            .keys()
            .filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, key.clone())))
            .collect();
        let agenda = files.len();
        let mut paths: Vec<Option<PathBuf>> = files.iter().map(|(uri, _)| uri::to_path(uri)).collect();
        let mut index_of = |server: &mut Self, files: &mut Vec<(Uri, Held)>, path: PathBuf| -> Option<usize> {
            if let Some(index) = paths.iter().position(|known| known.as_ref() == Some(&path)) {
                return Some(index);
            }
            let held = server.held(&path, &open)?;
            files.push(held);
            paths.push(Some(path));
            Some(paths.len() - 1)
        };
        let with_archives = extra.first().is_some_and(|first| first == "agenda-archives");
        let mut archives: Vec<Vec<usize>> = vec![Vec::new(); agenda];
        if with_archives {
            for (index, found) in archives.iter_mut().enumerate() {
                let Some(path) = uri::to_path(&files[index].0) else { continue };
                for archive in self.archives_of(&path, &open) {
                    found.extend(index_of(self, files, archive));
                }
            }
        }
        let mut more = Vec::new();
        for pattern in extra.iter().skip(usize::from(with_archives)) {
            for path in self.workspace.org_files(pattern) {
                more.extend(index_of(self, files, path));
            }
        }
        let all: Vec<Option<PathBuf>> = files.iter().map(|(uri, _)| uri::to_path(uri)).collect();
        let archives_of = |index: usize| archives[index].clone();
        let archives_of: Option<&dyn Fn(usize) -> Vec<usize>> = with_archives.then_some(&archives_of as _);
        Some(views::SearchFiles { agenda, search: views::search_order(&all, agenda, archives_of, &more) })
    }

    /// Where the entry of an agenda line is now.
    fn locate(&mut self, reference: &EntryRef) -> Option<Location> {
        let encoding = self.client.encoding;
        self.with_source(&reference.uri, |text, index, _, outline, _| {
            let entry = &outline.entries[views::resolve(reference, outline)?];
            let start = index.position(text, entry.offset, encoding);
            Some(Location { uri: reference.uri.clone(), range: Range::new(start, start) })
        })?
    }

    // ----- the agenda view

    fn render(&mut self, query: Query) -> View {
        self.render_with(query, None)
    }

    /// Build the view with the outline of one file replaced: what the
    /// agenda will show once that file has changed.
    fn render_with(&mut self, query: Query, replaced: Option<(&Uri, Outline)>) -> View {
        let mut files = self.agenda_files();
        // A command that no longer reads shows nothing; opening it said why.
        let blocks = self.blocks(&query).unwrap_or_default();
        let search = if blocks.iter().any(|block| matches!(block.kind, views::BlockKind::Search { .. })) {
            self.search_files(&mut files)
        } else {
            None
        };
        if let Some((uri, outline)) = replaced {
            if let Some(file) = files.iter_mut().find(|(file, _)| file == uri) {
                file.1 = Held::Open(Box::new(outline));
            }
        }
        let outlines: Vec<(Uri, &Outline)> = files.iter().map(|(uri, held)| (uri.clone(), held.outline())).collect();
        // The entry the clock runs on, highlighted.
        let clocked = self.running.as_ref().and_then(|running| {
            let (uri, outline) = outlines.iter().find(|(uri, _)| *uri == running.uri)?;
            let line = outline.clocks.iter().find(|line| {
                line.clock.is_running()
                    && line.clock.start == Some(running.start)
                    && line.entry.is_some_and(|entry| outline.entries[entry].head == running.head)
            })?;
            Some((uri, outline.entries[line.entry?].offset))
        });
        views::render(
            query,
            &blocks,
            &outlines,
            search.as_ref(),
            self.now(),
            clocked,
            Some(&self.agenda_filters),
            &self.agenda_marks,
        )
    }

    /// The blocks `query` shows, in log mode and with the clock report as
    /// `l` and `R` have them.
    fn blocks(&self, query: &Query) -> std::result::Result<Vec<views::Block>, String> {
        let mut blocks = self.query_blocks(query)?;
        for block in &mut blocks {
            block.config.show_log = self.agenda_log;
            block.config.clockreport = self.agenda_clockreport;
            block.config.entry_text = self.agenda_entry_text;
            block.config.archives = self.agenda_archives != ArchivesMode::Off;
        }
        Ok(blocks)
    }

    fn query_blocks(&self, query: &Query) -> std::result::Result<Vec<views::Block>, String> {
        let one = |kind| vec![views::Block { kind, header: None, config: self.agenda.clone() }];
        Ok(match query {
            Query::Agenda { start, span } => one(views::BlockKind::Agenda { start: *start, span: *span }),
            Query::Todo => one(views::BlockKind::Todo { keywords: None }),
            Query::Tags { matching, todo_only } => {
                one(views::BlockKind::Tags { matcher: org_core::agenda::Matcher::new(matching), todo_only: *todo_only })
            }
            Query::Search { string, todo_only } => {
                one(views::BlockKind::Search { string: string.clone(), todo_only: *todo_only })
            }
            Query::Stuck => one(views::BlockKind::Stuck),
            Query::Custom { keys, moved } => {
                let commands = self.agenda_options.custom_commands.as_deref().unwrap_or_default();
                let command = custom::find(commands, keys).ok_or_else(|| format!("No agenda command `{keys}`"))?;
                custom::blocks(command, &self.agenda_options, &self.agenda, self.now(), &self.read_date, *moved)?
            }
        })
    }

    /// Build the view for `query` and make it the current one.
    fn set_view(&mut self, query: Query) {
        let view = self.render(query);
        match &mut self.view {
            Some(open) => open.replace(view),
            None if self.client.virtual_documents => {
                if let Ok(uri) = views::VIRTUAL_URI.parse() {
                    self.view = Some(OpenView {
                        uri,
                        file: None,
                        view,
                        client: None,
                        shown: None,
                        closing: false,
                        highlighted: None,
                    });
                }
            }
            None => {
                // The client cannot fetch the view, so it gets a file.
                let directory = self.view_directory.clone();
                let file = directory.join(views::FILE_NAME);
                if std::fs::create_dir_all(&directory).is_ok() {
                    self.open_port();
                    if let Some(uri) = uri::from_path(&file) {
                        self.view = Some(OpenView {
                            uri,
                            file: Some(file),
                            view,
                            client: None,
                            shown: None,
                            closing: false,
                            highlighted: None,
                        });
                    }
                }
            }
        }
    }

    /// Open the command port, for a client that cannot run a command
    /// from a key (`command_actions`), and make its script this server's.
    fn open_port(&mut self) {
        if !self.client.command_actions || self.port.is_some() {
            return;
        }
        match port::open(self.port_sender.clone()) {
            Ok(port) => {
                self.port = Some(port);
                self.claim_script();
            }
            Err(error) => self.show_message(MessageType::ERROR, format!("Cannot open the command port: {error}")),
        }
    }

    /// Run a command the port was sent, where it says in the agenda or
    /// else where the cursor was last seen there, as
    /// `workspace/executeCommand` runs it.
    /// Stop rebuilding the agenda the client closed, and remove its file.
    fn drop_closed_view(&mut self) {
        if self.view.as_ref().is_some_and(|open| open.closing) {
            self.remove_view_file();
            self.view = None;
            self.reveal = None;
            self.follow = None;
            self.moving = None;
        }
    }

    fn run_from_port(&mut self, sent: port::Command) {
        self.drop_closed_view();
        let Some(uri) = self.view.as_ref().map(|open| open.uri.clone()) else { return };
        let position = match sent.position {
            Some((line, character)) => Position::new(line, character),
            None => self.agenda_cursor.unwrap_or_default(),
        };
        let argument = json!({ "textDocument": { "uri": uri }, "position": position });
        let params = ExecuteCommandParams {
            command: sent.command.clone(),
            arguments: vec![argument.clone()],
            work_done_progress_params: Default::default(),
        };
        match self.run_command(params) {
            Ok(value) => {
                if self.client.message_prompts {
                    self.prompt_with_messages(sent.command, argument, &value);
                }
            }
            Err(refusal) => self.show_message(MessageType::INFO, refusal.message().to_string()),
        }
    }

    /// Tell the client that the view has new text.
    fn publish_view(&mut self) {
        let Some(open) = &self.view else { return };
        match &open.file {
            Some(file) => {
                if let Err(error) = write_read_only(file, &open.view.text) {
                    self.show_message(MessageType::ERROR, format!("Cannot write the agenda: {error}"));
                }
            }
            None => {
                let params = json!({ "uri": open.uri });
                self.send_raw_request(TEXT_DOCUMENT_CONTENT_REFRESH, params, None);
            }
        }
    }

    /// Ask the client to show the view with the cursor at `position`.
    fn show_view(&mut self, position: Position) {
        let Some(uri) = self.view.as_ref().map(|open| open.uri.clone()) else { return };
        self.agenda_cursor = Some(position);
        let params = ShowDocumentParams {
            uri,
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(position, position)),
        };
        self.send_request::<ShowDocument>(params, Some(Pending::Reveal));
    }

    /// Move the cursor as `open_view` wanted, if the client by now shows
    /// the text that the line number belongs to.
    fn reveal_when_current(&mut self) {
        self.publish_line_highlights();
        if self.view.as_ref().is_some_and(OpenView::is_current) {
            let reveal = self.reveal.take();
            let position = reveal.zip(self.view.as_ref()).map(|(reveal, open)| reveal.position(&open.view));
            if let Some(position) = position {
                self.show_view(position);
            }
        }
    }

    /// Tell the client which lines of the view it shows are highlighted
    /// from edge to edge, as an overlay of Emacs's covers the whole line
    /// (`org-agenda-clocking`): `org-lsp/lineHighlights`
    /// `{ uri, lines: [{ line, highlight }] }`, once the client shows the
    /// current text and when they change.
    fn publish_line_highlights(&mut self) {
        if !self.client.line_highlights {
            return;
        }
        let Some(open) = self.view.as_mut().filter(|open| open.is_current()) else { return };
        if open.highlighted.as_ref() == Some(&open.view.clocking) {
            return;
        }
        open.highlighted = Some(open.view.clocking.clone());
        let lines: Vec<Value> =
            open.view.clocking.iter().map(|line| json!({ "line": line, "highlight": "agendaClocking" })).collect();
        let params = json!({ "uri": open.uri, "lines": lines });
        let notification = lsp_server::Notification::new(LINE_HIGHLIGHTS.to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// Show `query` in the client, with the cursor on today's date.
    fn open_view(&mut self, query: Query) -> std::result::Result<(), String> {
        self.open_view_at(query, Reveal::Home)
    }

    /// Show `query` in the client, with the cursor where `reveal` says.
    fn open_view_at(&mut self, query: Query, reveal: Reveal) -> std::result::Result<(), String> {
        if !self.client.show_document {
            return Err("This editor cannot open documents on the server's request".to_string());
        }
        self.set_view(query);
        self.follow = None;
        self.moving = None;
        let Some(open) = &self.view else { return Err("Cannot create the agenda".to_string()) };
        let line = reveal.position(&open.view);
        match &open.client {
            // The client shows this very text already.
            Some(copy) if copy.text == open.view.text => self.show_view(line),
            // The client shows the old text. A cursor position sent now
            // would be applied to those lines, so it waits until the client
            // has replaced them; see `reveal_when_current`.
            Some(_) => {
                self.reveal = Some(reveal);
                self.publish_view();
            }
            // The client does not have the view open, as far as it has
            // said: showing it makes the client fetch it, or read the file.
            None => {
                self.publish_view();
                self.show_view(line);
            }
        }
        Ok(())
    }

    /// Rebuild the view after its files changed. The cursor stays, unless
    /// the change is the edit of a command that takes the cursor along.
    fn refresh_view(&mut self) {
        let Some(query) = self.view.as_ref().map(|open| open.view.query.clone()) else { return };
        let view = self.render(query);
        let Some(open) = &mut self.view else { return };
        let changed = view.text != open.view.text;
        // The same text highlighted anew: the entry the clock runs on.
        let recoloured = !changed && view.tokens != open.view.tokens;
        // The same text may still stand for entries that moved.
        open.replace(view);
        if let Some(follow) = self.follow.take_if(|follow| follow.text == open.view.text) {
            self.reveal = Some(Reveal::At(follow.position));
            self.moving = Some((follow.from, follow.position.line));
        } else if changed {
            // Line numbers of the old text: the cursor stays where it is.
            self.reveal = self.reveal.take().filter(|reveal| !matches!(reveal, Reveal::At(_)));
            self.moving = None;
        }
        if changed {
            self.publish_view();
        } else {
            if recoloured && self.client.tokens_refresh {
                self.send_raw_request("workspace/semanticTokens/refresh", Value::Null, None);
            }
            self.reveal_when_current();
        }
    }

    fn remove_view_file(&mut self) {
        if let Some(file) = self.view.as_ref().and_then(|open| open.file.as_ref()) {
            // Windows does not remove a read-only file.
            let _ = set_writable(file);
            let _ = std::fs::remove_file(file);
            if let (Some(port), Some(path)) = (&self.port, &self.client.command_script) {
                port.release(path);
            }
            if let Some(directory) = file.parent() {
                let _ = std::fs::remove_dir(directory);
            }
        }
    }

    // ----- commands

    /// Run a command as `workspace/executeCommand` asks: its result, with
    /// what it has to say.
    fn run_command(&mut self, params: ExecuteCommandParams) -> std::result::Result<Value, Refusal> {
        self.message = None;
        self.date_unit_before = self.date_unit.take();
        self.origin = params
            .arguments
            .first()
            .and_then(|argument| argument.pointer("/textDocument/uri")?.as_str())
            .map(str::to_string);
        let value = self.execute_command(params)?;
        Ok(with_message(value, self.message.take()))
    }

    /// For a client that does nothing with a command's result
    /// (`message_prompts`): say its message with `window/showMessage`, and
    /// ask what it asks for with `window/showMessageRequest`, one button a
    /// choice.
    fn prompt_with_messages(&mut self, command: String, argument: Value, result: &Value) {
        if let Some(message) = result.get("message").and_then(Value::as_str) {
            self.show_message(MessageType::INFO, message);
        }
        let Some(input) = result.get("input") else { return };
        let prompt = input.get("prompt").and_then(Value::as_str).unwrap_or("").to_string();
        let choices = input.get("choices").and_then(Value::as_array).cloned().unwrap_or_default();
        if choices.is_empty() {
            let message = format!("{prompt}: this editor cannot ask for text; use a date or text in the file instead");
            self.show_message(MessageType::WARNING, message);
            return;
        }
        let actions = choices
            .iter()
            .map(|choice| lsp_types::MessageActionItem { title: prompt_label(choice), properties: HashMap::new() })
            .collect();
        let params =
            lsp_types::ShowMessageRequestParams { typ: MessageType::INFO, message: prompt, actions: Some(actions) };
        let pending = Pending::Answer { command, argument, input: input.clone() };
        self.send_request::<lsp_types::request::ShowMessageRequest>(params, Some(pending));
    }

    /// The answer to a question asked by [`Server::prompt_with_messages`]:
    /// run the command again with it, as an editor does.
    fn answer_prompt(&mut self, command: String, argument: Value, input: &Value, picked: &str) {
        let choices = input.get("choices").and_then(Value::as_array).cloned().unwrap_or_default();
        let Some(key) =
            choices.iter().find(|choice| prompt_label(choice) == picked).and_then(|choice| choice.get("key"))
        else {
            return;
        };
        let mut argument = match argument {
            Value::Object(fields) => fields,
            _ => serde_json::Map::new(),
        };
        if let Some(Value::Object(context)) = input.get("context") {
            argument.extend(context.clone());
        }
        argument.insert("input".to_string(), key.clone());
        let argument = Value::Object(argument);
        let params = ExecuteCommandParams {
            command: command.clone(),
            arguments: vec![argument.clone()],
            work_done_progress_params: Default::default(),
        };
        match self.run_command(params) {
            Ok(value) => self.prompt_with_messages(command, argument, &value),
            Err(refusal) => self.show_message(MessageType::WARNING, refusal.message()),
        }
    }

    fn execute_command(&mut self, params: ExecuteCommandParams) -> std::result::Result<Value, Refusal> {
        let argument = params.arguments.into_iter().next();
        // Capturing and the agenda menu need no document.
        if params.command == commands::AGENDA_DISPATCH {
            return self.dispatch(argument.as_ref());
        }
        if params.command == commands::CAPTURE {
            let context = argument.and_then(|value| serde_json::from_value(value).ok()).unwrap_or_default();
            return self.capture(&context);
        }
        let context: Option<Context> = argument.and_then(|value| serde_json::from_value(value).ok());
        match params.command.as_str() {
            commands::AGENDA_QUERY_ADD
            | commands::AGENDA_QUERY_SUBTRACT
            | commands::AGENDA_QUERY_ADD_REGEXP
            | commands::AGENDA_QUERY_SUBTRACT_REGEXP => self.manipulate_query(&params.command, context.as_ref()),
            id if id.starts_with("org.calendar.") => self.calendar_command(id, context.as_ref()),
            // `org-save-all-org-buffers`: the server cannot save; the
            // editor saves the files it names that have changes.
            commands::AGENDA_SAVE => {
                let mut files: Vec<&String> = self.documents.keys().collect();
                files.sort();
                Ok(json!({ "save": files }))
            }
            commands::DATE_PREVIEW => self.preview_date(&context.ok_or("This command needs the cursor position")?),
            id if id.starts_with("org.agenda.filter") => self.agenda_filter(id, context.as_ref()),
            commands::AGENDA_BULK_ACTION => self.agenda_bulk_action(context.as_ref()),
            id if id.starts_with("org.agenda.bulk.") => self.agenda_bulk_mark(id, context.as_ref()),
            commands::PRIORITY => self.ask_priority(&context.ok_or("This command needs the cursor position")?),
            commands::REFILE => self.refile(&context.ok_or("This command needs the cursor position")?),
            commands::OPEN => self.open_at_point(&context.ok_or("This command needs the cursor position")?),
            commands::CITE_INSERT | commands::CITE_INSERT_PREFIX => self.cite_insert(
                params.command == commands::CITE_INSERT_PREFIX,
                &context.ok_or("This command needs the cursor position")?,
            ),
            commands::TANGLE => self.tangle(&context.ok_or("This command needs the cursor position")?),
            id if id.starts_with("org.archive.") => {
                self.archive(id, &context.ok_or("This command needs the cursor position")?)
            }
            commands::RETURN | commands::RETURN_ITEM => self.org_return(
                &context.ok_or("This command needs the cursor position")?,
                params.command == commands::RETURN_ITEM,
            ),
            commands::META_LEFT
            | commands::META_RIGHT
            | commands::SHIFT_META_LEFT
            | commands::SHIFT_META_RIGHT
            | commands::META_UP
            | commands::META_DOWN => {
                self.structure(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::SHIFT_META_UP
            | commands::SHIFT_META_DOWN
            | commands::TABLE_NEXT_FIELD
            | commands::TABLE_PREVIOUS_FIELD
            | commands::TABLE_INSERT_ROW_BELOW
            | commands::TABLE_HLINE
            | commands::TABLE_HLINE_ABOVE
            | commands::TABLE_HLINE_MOVE
            | commands::TABLE_BLANK_FIELD => {
                let context = context.ok_or("This command needs the cursor position")?;
                let command = table_command(&params.command).ok_or("Unknown command")?;
                self.table(command, &context)
            }
            commands::SORT => self.sort(&context.ok_or("This command needs the cursor position")?),
            commands::SPARSE_TREE => self.sparse_tree(&context.ok_or("This command needs the cursor position")?),
            commands::CUT_SUBTREE | commands::COPY_SUBTREE | commands::PASTE_SUBTREE => {
                self.subtree_clipboard(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::META_RETURN
            | commands::INSERT_TODO_HEADING
            | commands::INSERT_HEADING_AFTER
            | commands::INSERT_TODO_HEADING_AFTER => {
                self.meta_return(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            // In a note buffer, the keys of a capture store or cancel the
            // note.
            commands::CAPTURE_FINISH | commands::CAPTURE_ABORT
                if self.note.as_ref().is_some_and(|note| {
                    context.as_ref().is_none_or(|context| context.text_document.uri == note.uri)
                }) =>
            {
                let note = self.note.take().expect("a note");
                let text = context.and_then(|context| context.text);
                self.finish_note(note, params.command == commands::CAPTURE_ABORT, text)
            }
            commands::CAPTURE_FINISH => {
                let text = self.capture_buffer_text(context.as_ref());
                self.finish_capture(text)
            }
            commands::CAPTURE_ABORT => {
                let capture = self.capture.take().ok_or("No capture is going on")?;
                self.message = Some("Capture process aborted and target buffer cleaned up".to_string());
                Ok(json!({ "close": capture.uri }))
            }
            commands::CAPTURE_REFILE => {
                let text = self.capture_buffer_text(context.as_ref());
                self.refile_capture(context.and_then(|context| context.input).as_deref(), text)
            }
            commands::CLOCK_DISPLAY => self.clock_display(&context.ok_or("This command needs the cursor position")?),
            // `C-c C-c` takes the clock's times away first.
            commands::CHECKBOX_TOGGLE
                if context
                    .as_ref()
                    .is_some_and(|context| self.clock_display.contains_key(context.text_document.uri.as_str())) =>
            {
                let uri =
                    context.as_ref().map(|context| context.text_document.uri.as_str().to_string()).unwrap_or_default();
                self.clock_display.remove(&uri);
                self.refresh_inlay_hints();
                Ok(json!({ "message": "Temporary highlights/overlays removed from current buffer" }))
            }
            id @ (commands::CLOCK_REPORT | commands::DBLOCK_UPDATE) => {
                self.clocktable(id, &context.ok_or("This command needs the cursor position")?)
            }
            id if id.starts_with("org.clock.") => self.clock_command(id, context.as_ref()),
            id if id == commands::TODO
                || commands::AGENDA_DATE_COMMANDS.contains(&id)
                || id == commands::SET_TAGS
                || PROPERTY_COMMANDS.contains(&id)
                || actions::command(id).is_some() =>
            {
                self.run_edit(id, context)
            }
            id if commands::input(id).is_some() => {
                self.ask_date(id, &context.ok_or("This command needs the cursor position")?)
            }
            id => self.execute(id, context).map(|()| Value::Null),
        }
    }

    /// Where the command `id`, which asks for a date, acts when run at
    /// `context`: there in an Org file, or on the entry of an agenda line
    /// (`org-agenda-schedule`, `org-agenda-deadline`).
    fn date_target(&self, id: &str, context: &Context) -> std::result::Result<Target, Refusal> {
        let uri = &context.text_document.uri;
        if id == commands::AGENDA_GOTO_DATE {
            let open = self.view.as_ref().filter(|open| open.uri == *uri);
            let open = open.ok_or_else(|| Refusal::NotHere("This command works in the agenda".to_string()))?;
            let _ = open;
            self.agenda_block(Some(context.position.line))?;
            return Ok(Target::Agenda { view: uri.clone(), line: context.position.line });
        }
        if !self.is_view(uri) {
            return Ok(Target::Place { uri: uri.clone(), position: context.position });
        }
        if !matches!(id, commands::SCHEDULE | commands::DEADLINE) {
            return Err(Refusal::NotHere("This command works in Org files".to_string()));
        }
        let (line, row) = self.view_line(uri, context.position.line).ok_or("No entry on this line")?;
        row.entry.as_ref().ok_or("No entry on this line")?;
        Ok(Target::Entry { view: uri.clone(), line, character: context.position.character, row: row.clone() })
    }

    /// The file a command that asks for a date changes, as it reads now.
    fn date_source(&mut self, target: &Target) -> std::result::Result<DateSource, Refusal> {
        match target {
            Target::Place { uri, position } => {
                let document = self.document(uri).ok_or("The document is not open")?;
                Ok(DateSource {
                    file: uri.clone(),
                    text: document.text.clone(),
                    offset: document.index.offset(&document.text, *position, self.client.encoding),
                    version: Some(document.version),
                    heading: None,
                })
            }
            Target::Agenda { .. } => Err("The agenda has no file to change".into()),
            Target::Entry { row, .. } => {
                let reference = row.entry.as_ref().ok_or("No entry on this line")?;
                self.with_source(&reference.uri, |text, _, _, outline, version| {
                    let entry = views::resolve(reference, outline).ok_or("The entry is gone; rebuild the agenda")?;
                    let heading = outline.entries[entry].offset;
                    // Where the line's marker points: the timestamp it
                    // stands for, seen from the headline, or the headline.
                    let offset =
                        reference.marker.map_or(heading, |marker| heading + marker.saturating_sub(reference.offset));
                    Ok(DateSource {
                        file: reference.uri.clone(),
                        text: text.to_string(),
                        offset,
                        version,
                        heading: Some(heading),
                    })
                })
                .ok_or("Cannot read the file of this entry")?
            }
        }
    }

    /// What the command `id` that asks for a date does at `target`. Changing
    /// the timestamp at the cursor needs one there, and keeps it active or
    /// inactive.
    fn date_command_at(&mut self, id: &str, target: &Target) -> std::result::Result<PlanningCommand, Refusal> {
        if id != commands::TIMESTAMP_CHANGE {
            return Ok(date_command(id).ok_or("Unknown command")?);
        }
        let source = self.date_source(target)?;
        let inactive = planning::timestamp_kind(&source.text, source.offset)
            .ok_or_else(|| Refusal::NotHere("No timestamp at the cursor".to_string()))?;
        Ok(PlanningCommand::Timestamp { inactive })
    }

    /// What the date prompt of `command` at `target` starts from.
    fn date_prompt_start(&mut self, command: PlanningCommand, target: &Target) -> std::result::Result<Prompt, Refusal> {
        if let Target::Agenda { .. } = target {
            // `org-read-date` without a default: today, and nothing typed.
            return Ok(Prompt { default: None, value: String::new() });
        }
        let source = self.date_source(target)?;
        planning::prompt(command, &source.text, source.offset)
    }

    /// A command that asks for a date. Without an answer it says what to
    /// ask: `{ "input": { "kind", "prompt", "value" } }`, where `value` is
    /// the text the prompt starts with. With `input` it runs.
    fn ask_date(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let Some(answer) = &context.input else {
            let target = self.date_target(id, context)?;
            let command = self.date_command_at(id, &target)?;
            let prompt = self.date_prompt_start(command, &target)?;
            let input = commands::input(id).ok_or("Unknown command")?;
            // In a client that shows it, the calendar opens instead; the
            // answer is typed there once asked for (`org.calendar.type`).
            if self.client.date_prompt {
                let now = self.date_time();
                let open =
                    DatePrompt::new(id, command, target, prompt.default, prompt.value.clone(), now, &self.read_date);
                self.open_date_prompt(open);
                return Ok(Value::Null);
            }
            return Ok(json!({ "input": { "kind": input.kind, "prompt": input.prompt, "value": prompt.value } }));
        };
        // The answer typed while the calendar was open ends with the day
        // that was moved to there.
        let pending =
            self.date_prompt.take_if(|open| open.id == id && open.target.from() == &context.text_document.uri);
        let answer = match &pending {
            Some(open) if open.moved => format!("{answer} {}", Date::from_day(open.selected)),
            _ => answer.clone(),
        };
        // The prompt knows the entry even when the agenda changed since.
        let target = match &pending {
            Some(open) => open.target.clone(),
            None => self.date_target(id, context)?,
        };
        self.apply_date(id, &target, &answer)?;
        Ok(if pending.is_some() { json!({ "close": prompt::URI }) } else { Value::Null })
    }

    /// Run the command `id` that asked for a date, at `target`, with
    /// `answer`.
    fn apply_date(&mut self, id: &str, target: &Target, answer: &str) -> std::result::Result<(), Refusal> {
        if let Target::Agenda { line, .. } = target {
            return self.goto_date(answer, *line);
        }
        let command = self.date_command_at(id, target)?;
        let source = self.date_source(target)?;
        let key = source.file.as_str().to_string();
        // A timestamp right after another one makes a date range.
        let repeated = self.last.take().is_some_and(|last| {
            last.file == key
                && matches!(last.command.as_str(), commands::TIMESTAMP_INSERT | commands::TIMESTAMP_INSERT_INACTIVE)
        });
        self.last = Some(Last { command: id.to_string(), file: key.clone(), heading: None });
        let now = self.date_time();
        let text = &source.text;
        let outcome =
            planning::run(command, text, source.offset, answer, repeated, now, &self.read_date, self.adapt_in(text))?;
        let encoding = self.client.encoding;
        let index = LineIndex::new(text);
        let edits: Vec<TextEdit> = actions::edits_between(text, &outcome.text)
            .into_iter()
            .map(|edit| TextEdit { range: index.range(text, edit.range, encoding), new_text: edit.new_text })
            .collect();
        self.message = outcome.message.clone();
        if edits.is_empty() {
            return Ok(());
        }
        // `cid` leaves the cursor where it was in the timestamp it changed,
        // as nvim-orgmode does, not after it as `C-c .` does.
        let cursor = match org_core::stamp::Stamp::around(text, source.offset) {
            Some(old) if id == commands::TIMESTAMP_CHANGE => org_core::stamp::Stamp::at(&outcome.text, old.start)
                .map_or(outcome.cursor, |new| {
                    let mut at = source.offset.clamp(new.start, new.end - 1);
                    while !outcome.text.is_char_boundary(at) {
                        at -= 1;
                    }
                    Some(at)
                }),
            _ => outcome.cursor,
        };
        let pending = match (target, cursor) {
            // From the agenda, its cursor follows the entry, as after the
            // agenda's other commands.
            (Target::Entry { line, character, row, .. }, _) => {
                let from = (Position::new(*line, *character), row);
                self.follow =
                    source.heading.and_then(|heading| self.followed(&source.file, &outcome.text, heading, from));
                Pending::Edit
            }
            (_, Some(cursor)) => {
                let position = LineIndex::new(&outcome.text).position(&outcome.text, cursor, encoding);
                Pending::EditAndMove { uri: source.file.clone(), position }
            }
            (_, None) => Pending::Edit,
        };
        let edit = self.workspace_edit(&source.file, source.version, edits);
        self.edited.insert(key);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, pending);
        Ok(())
    }

    /// `org-agenda-goto-date`: the agenda from the day `answer` names,
    /// over as many days as its span has from there, with the cursor on
    /// that day. A week or a fortnight still starts on the first day of
    /// the week (`org-agenda-start-on-weekday`).
    fn goto_date(&mut self, answer: &str, line: u32) -> std::result::Result<(), Refusal> {
        let read = org_core::read_date::read_date(answer, None, self.date_time(), &self.read_date)?;
        let day = read.date.to_day();
        let (block, _, span) = self.agenda_block(Some(line))?;
        let span = Span::from_days(span.days(day));
        let query = self.moved(block, Some(day), span);
        Ok(self.open_view_at(query, Reveal::Block { block, day: Some(day), count: None })?)
    }

    // ----- the date prompt

    /// Show `open` as the date prompt, replacing one that was open.
    fn open_date_prompt(&mut self, mut open: DatePrompt) {
        let cursor = open.cursor();
        if self.prompt_copy.is_some() {
            // The client has the document: it shows it again and fetches
            // the new text, and the cursor follows once it has.
            self.date_prompt = Some(open);
            self.send_raw_request(TEXT_DOCUMENT_CONTENT_REFRESH, json!({ "uri": prompt::URI }), None);
            let params = ShowDocumentParams {
                uri: prompt::URI.parse().expect("valid URI"),
                external: None,
                take_focus: Some(true),
                selection: None,
            };
            self.send_request::<ShowDocument>(params, None);
            self.reveal_prompt_when_current();
        } else {
            open.reveal = false;
            open.moving_cursor = true;
            self.date_prompt = Some(open);
            let params = ShowDocumentParams {
                uri: prompt::URI.parse().expect("valid URI"),
                external: None,
                take_focus: Some(true),
                selection: Some(Range::new(cursor, cursor)),
            };
            self.send_request::<ShowDocument>(params, Some(Pending::PromptCursor));
        }
    }

    /// Whether the client shows the current text of the date prompt.
    fn prompt_is_current(&self) -> bool {
        let copy = self.prompt_copy.as_ref().map(|copy| copy.text.as_str());
        self.date_prompt.as_ref().is_some_and(|open| copy == Some(open.text.as_str()))
    }

    /// Tell the client that the date prompt has new text, or new colours.
    fn publish_date_prompt(&mut self) {
        if self.date_prompt.is_none() {
            return;
        }
        if self.prompt_is_current() {
            if self.client.tokens_refresh {
                self.send_raw_request("workspace/semanticTokens/refresh", Value::Null, None);
            }
            self.reveal_prompt_when_current();
        } else {
            let params = json!({ "uri": prompt::URI });
            self.send_raw_request(TEXT_DOCUMENT_CONTENT_REFRESH, params, None);
        }
    }

    /// Put the cursor on the selected day once the client shows the text
    /// it belongs to.
    fn reveal_prompt_when_current(&mut self) {
        if !self.prompt_is_current() {
            return;
        }
        let Some(open) = self.date_prompt.as_mut().filter(|open| open.reveal) else { return };
        open.reveal = false;
        open.moving_cursor = true;
        let cursor = open.cursor();
        let params = ShowDocumentParams {
            uri: prompt::URI.parse().expect("valid URI"),
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(cursor, cursor)),
        };
        self.send_request::<ShowDocument>(params, Some(Pending::PromptCursor));
    }

    /// A key of the date prompt. The day it starts from is the one under
    /// the cursor, as in Emacs's calendar, or else the selected one.
    fn calendar_command(&mut self, id: &str, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let (now, settings) = (self.date_time(), self.read_date);
        let step = self.edit_settings.timestamp_rounding_minutes.max(1);
        let open = self.date_prompt.as_mut().ok_or("No date prompt is open")?;
        // Picking the time: the keys that move by day choose the hours or
        // the minutes, and those that move by week change them.
        if open.time_field.is_some() {
            let timed = matches!(
                id,
                commands::CALENDAR_DAY_LATER
                    | commands::CALENDAR_DAY_EARLIER
                    | commands::CALENDAR_WEEK_EARLIER
                    | commands::CALENDAR_WEEK_LATER
            );
            match id {
                commands::CALENDAR_DAY_LATER => open.time_field_to(TimeField::Minute, now, &settings),
                commands::CALENDAR_DAY_EARLIER => open.time_field_to(TimeField::Hour, now, &settings),
                commands::CALENDAR_WEEK_EARLIER => open.change_time(true, step, now, &settings),
                commands::CALENDAR_WEEK_LATER => open.change_time(false, step, now, &settings),
                _ => {}
            }
            if timed {
                self.publish_date_prompt();
                return Ok(Value::Null);
            }
        }
        // Until the cursor is on the day selected last, the one it reports
        // is an old one: a key pressed again quickly goes on from the
        // selected day.
        let at = context
            .filter(|context| context.text_document.uri.as_str() == prompt::URI)
            .filter(|_| !open.reveal && !open.moving_cursor)
            .and_then(|context| open.day_at(context.position))
            .unwrap_or(open.selected);
        let date = Date::from_day(at);
        let by_months = |months: i64| {
            let target = Date::new(date.year, date.month + months, 1);
            let length = org_core::date::days_in_month(
                Date::from_day(target.to_day()).year,
                Date::from_day(target.to_day()).month,
            );
            Date::new(target.year, target.month, date.day.min(length)).to_day()
        };
        match id {
            commands::CALENDAR_DAY_LATER => open.select(at + 1, now, &settings),
            commands::CALENDAR_DAY_EARLIER => open.select(at - 1, now, &settings),
            commands::CALENDAR_WEEK_LATER => open.select(at + 7, now, &settings),
            commands::CALENDAR_WEEK_EARLIER => open.select(at - 7, now, &settings),
            commands::CALENDAR_MONTH_LATER => open.select(by_months(1), now, &settings),
            commands::CALENDAR_MONTH_EARLIER => open.select(by_months(-1), now, &settings),
            commands::CALENDAR_YEAR_LATER => open.select(by_months(12), now, &settings),
            commands::CALENDAR_YEAR_EARLIER => open.select(by_months(-12), now, &settings),
            commands::CALENDAR_SCROLL_LATER => open.scroll(1, now, &settings),
            commands::CALENDAR_SCROLL_EARLIER => open.scroll(-1, now, &settings),
            commands::CALENDAR_TODAY => open.select(now.day, now, &settings),
            commands::CALENDAR_TIME => open.toggle_time(step, now, &settings),
            commands::CALENDAR_SELECT => {
                // `org-calendar-select`: what was typed, and this day.
                let answer = open.answer_with(at);
                return self.finish_date_prompt(&answer);
            }
            commands::CALENDAR_TYPE => {
                let Some(typed) = context.and_then(|context| context.input.clone()) else {
                    // The minibuffer: ask, starting from what was typed.
                    return Ok(
                        json!({ "input": { "kind": "date", "prompt": open.label.trim_end().trim_end_matches(':'), "value": open.typed_with_time() } }),
                    );
                };
                // `RET` there: what was typed, and the day if it moved.
                open.typed = typed;
                let answer = open.answer();
                return self.finish_date_prompt(&answer);
            }
            commands::CALENDAR_CANCEL => {
                self.date_prompt = None;
                return Ok(json!({ "close": prompt::URI }));
            }
            other => return Err(format!("org-lsp has no command `{other}`").into()),
        }
        self.publish_date_prompt();
        Ok(Value::Null)
    }

    /// Run the command the date prompt answers, and close the prompt.
    fn finish_date_prompt(&mut self, answer: &str) -> std::result::Result<Value, Refusal> {
        let open = self.date_prompt.as_ref().ok_or("No date prompt is open")?;
        let (id, target) = (open.id.clone(), open.target.clone());
        self.apply_date(&id, &target, answer)?;
        self.date_prompt = None;
        Ok(json!({ "close": prompt::URI }))
    }

    /// `org.date.preview`: the timestamp an answer to the prompt of
    /// `command` makes, `{ "timestamp": "<2026-05-22 Fri>" }`.
    fn preview_date(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let id = context.command.as_deref().ok_or("Which command's prompt?")?;
        let answer = context.input.as_deref().unwrap_or("");
        let (now, settings) = (self.date_time(), self.read_date);
        // With the date prompt open, what is typed shows in its minibuffer
        // too, and the answer ends with the day moved to there.
        if let Some(open) = self.date_prompt.as_mut().filter(|open| open.id == id || id == commands::CALENDAR_TYPE) {
            open.set_typed(answer.to_string(), now, &settings);
            let read = org_core::read_date::read_date(&open.answer(), open.default, now, &settings)?;
            let inactive = matches!(open.command, PlanningCommand::Timestamp { inactive: true });
            let stamp = planning::timestamp_for(&read, inactive);
            self.publish_date_prompt();
            return Ok(json!({ "timestamp": stamp }));
        }
        let target = self.date_target(id, context)?;
        let command = self.date_command_at(id, &target)?;
        let start = self.date_prompt_start(command, &target)?;
        let read = org_core::read_date::read_date(answer, start.default, now, &settings)?;
        let stamp = planning::timestamp_for(&read, matches!(command, PlanningCommand::Timestamp { inactive: true }));
        Ok(json!({ "timestamp": stamp }))
    }

    fn execute(&mut self, command: &str, context: Option<Context>) -> std::result::Result<(), Refusal> {
        if actions::command(command).is_none() {
            // Whatever was being repeated has come to an end.
            self.last = None;
        }
        match command {
            commands::AGENDA_LIST => {
                self.agenda_filters = Default::default();
                self.agenda_marks.clear();
                Ok(self.open_view(Query::Agenda { start: self.start_day(), span: self.agenda.span })?)
            }
            commands::AGENDA_TODOS => {
                self.agenda_filters = Default::default();
                self.agenda_marks.clear();
                Ok(self.open_view(Query::Todo)?)
            }
            commands::AGENDA_REDO => {
                self.view.as_ref().ok_or("No agenda is open")?;
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_LOG | commands::AGENDA_CLOCKREPORT => {
                self.view.as_ref().ok_or("No agenda is open")?;
                let (flag, name) = if command == commands::AGENDA_LOG {
                    (&mut self.agenda_log, "Log mode")
                } else {
                    (&mut self.agenda_clockreport, "Clocktable mode")
                };
                *flag = !*flag;
                self.message = Some(format!("{name} is {}", if *flag { "on" } else { "off" }));
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_ENTRY_TEXT => {
                self.view.as_ref().ok_or("No agenda is open")?;
                // `org-agenda-entry-text-mode`: a count turns it on with
                // that many lines.
                let filters = &self.agenda_filters;
                if !filters.tag.is_empty()
                    || !filters.category.is_empty()
                    || !filters.regexp.is_empty()
                    || filters.top_headline.is_some()
                {
                    return Err("Can't show entry text in filtered views".into());
                }
                let count = context.as_ref().and_then(|context| context.count).filter(|count| *count > 0);
                let lines = count.map_or(self.agenda.entry_text_settings.max_lines, |count| count as usize);
                self.agenda_entry_text = (count.is_some() || self.agenda_entry_text.is_none()).then_some(lines);
                self.message = Some(match self.agenda_entry_text {
                    Some(lines) => format!("Entry text mode is on (maximum number of lines is {lines})"),
                    None => "Entry text mode is off".to_string(),
                });
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_ARCHIVES | commands::AGENDA_ARCHIVES_FILES => {
                self.view.as_ref().ok_or("No agenda is open")?;
                // `org-agenda-archives-mode`, with or without the files.
                self.agenda_archives = match (command, self.agenda_archives) {
                    (commands::AGENDA_ARCHIVES_FILES, ArchivesMode::Files) => ArchivesMode::Off,
                    (commands::AGENDA_ARCHIVES_FILES, _) => ArchivesMode::Files,
                    (_, ArchivesMode::Off) => ArchivesMode::Trees,
                    _ => ArchivesMode::Off,
                };
                self.message = Some(match self.agenda_archives {
                    ArchivesMode::Off => "No archives are included".to_string(),
                    ArchivesMode::Trees => "Trees with :ARCHIVE: tag are included".to_string(),
                    ArchivesMode::Files => {
                        "Trees with :ARCHIVE: tag and all active archive files are included".to_string()
                    }
                });
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_LATER | commands::AGENDA_EARLIER | commands::AGENDA_TODAY => {
                let line = self.view_cursor_line(context.as_ref());
                let (block, first, span) = self.agenda_block(line)?;
                let steps = context.as_ref().and_then(|context| context.count).unwrap_or(1);
                let start = match command {
                    commands::AGENDA_LATER => Some(span.shift(first, steps)),
                    commands::AGENDA_EARLIER => Some(span.shift(first, -steps)),
                    _ => None,
                };
                // `org-agenda-later`: to the day at the same place among
                // the days shown as the one the cursor was under.
                let count = match command {
                    commands::AGENDA_TODAY => None,
                    _ => line.and_then(|line| self.view.as_ref()?.on_screen().day_count(line)),
                };
                Ok(self.open_view_at(self.moved(block, start, span), Reveal::Block { block, day: None, count })?)
            }
            commands::AGENDA_DAY
            | commands::AGENDA_WEEK
            | commands::AGENDA_FORTNIGHT
            | commands::AGENDA_MONTH
            | commands::AGENDA_YEAR => {
                let span = match command {
                    commands::AGENDA_DAY => Span::Day,
                    commands::AGENDA_WEEK => Span::Week,
                    commands::AGENDA_FORTNIGHT => Span::Fortnight,
                    commands::AGENDA_MONTH => Span::Month,
                    _ => Span::Year,
                };
                // The new span starts from the day under the cursor; from
                // where there are no days, a new agenda.
                let line = self.view_cursor_line(context.as_ref());
                let (query, reveal) = match self.agenda_block(line) {
                    Ok((block, first, _)) => {
                        let open = self.view.as_ref().ok_or("No agenda is open")?;
                        let day = line.and_then(|line| open.on_screen().line(line)?.day);
                        let start = agenda::starting_day(day.unwrap_or(first), span, &self.agenda);
                        (self.moved(block, Some(start), span), Reveal::Block { block, day, count: None })
                    }
                    Err(_) => (Query::Agenda { start: None, span }, Reveal::Home),
                };
                self.open_view_at(query, reveal)?;
                self.message = Some(format!("Switched to {} view", span_name(span)));
                Ok(())
            }
            commands::AGENDA_FOLLOW => {
                self.view.as_ref().ok_or("No agenda is open")?;
                // `org-agenda-follow-mode`, which shows the entry at once.
                self.agenda_follow = !self.agenda_follow;
                if self.agenda_follow {
                    self.show_entry(context.as_ref());
                }
                self.message = Some(format!("Follow mode is {}", if self.agenda_follow { "on" } else { "off" }));
                Ok(())
            }
            commands::AGENDA_GOTO | commands::AGENDA_GOTO_OTHER => {
                let reference = self.entry_at(context.as_ref()).ok_or("No entry on this line")?;
                let location = self.locate(&reference).ok_or("The entry is gone; rebuild the agenda")?;
                let params = ShowDocumentParams {
                    uri: location.uri,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(location.range),
                };
                let mut params = serde_json::to_value(params).unwrap_or_default();
                // Not LSP's: the editors of this project show the file in a
                // window other than the agenda's.
                if command == commands::AGENDA_GOTO_OTHER {
                    params["orgOtherWindow"] = json!(true);
                }
                self.send_raw_request(ShowDocument::METHOD, params, None);
                Ok(())
            }

            other => Err(format!("org-lsp has no command `{other}`").into()),
        }
    }

    /// `org-agenda-show`: the entry of the agenda line `context` names in
    /// another window, the agenda keeping the focus. Nothing for a line
    /// without an entry.
    fn show_entry(&mut self, context: Option<&Context>) {
        let Some(location) = self.entry_at(context).and_then(|reference| self.locate(&reference)) else { return };
        let params = ShowDocumentParams {
            uri: location.uri,
            external: None,
            take_focus: Some(false),
            selection: Some(location.range),
        };
        let mut params = serde_json::to_value(params).unwrap_or_default();
        params["orgOtherWindow"] = json!(true);
        self.send_raw_request(ShowDocument::METHOD, params, None);
    }

    /// `org-agenda` (`C-c a`): the views by their keys, as Emacs's menu
    /// offers them, and the one picked. `keys` are those typed so far (a
    /// prefix of custom commands); `m` and `M` then ask for a match
    /// string (`ask: "match"`).
    fn dispatch(&mut self, argument: Option<&Value>) -> std::result::Result<Value, Refusal> {
        let get = |name: &str| argument.and_then(|value| value.get(name)?.as_str()).map(str::to_string);
        let prefix = get("keys").unwrap_or_default();
        let input = get("input");
        if get("ask").as_deref() == Some("match") {
            let matching = input.unwrap_or_default();
            self.agenda_filters = Default::default();
            self.agenda_marks.clear();
            self.open_view(Query::Tags { matching, todo_only: prefix == "M" })?;
            return Ok(Value::Null);
        }
        if get("ask").as_deref() == Some("search") {
            return self.search(input.unwrap_or_default(), prefix == "S");
        }
        let keys = format!("{prefix}{}", input.as_deref().unwrap_or(""));
        let commands = self.agenda_options.custom_commands.clone().unwrap_or_default();
        // The user's commands come first, as in Emacs.
        if custom::find(&commands, &keys).is_some() {
            let query = Query::Custom { keys, moved: None };
            self.blocks(&query)?;
            self.agenda_filters = Default::default();
            self.agenda_marks.clear();
            self.open_view(query)?;
            return Ok(Value::Null);
        }
        let built_in = [
            ("a", commands::title(commands::AGENDA_LIST)),
            ("t", commands::title(commands::AGENDA_TODOS)),
            ("m", "Match a TAGS/PROP/TODO query"),
            ("M", "Like m, but only TODO entries"),
            ("s", "Search for keywords"),
            ("S", "Like s, but only TODO entries"),
            ("#", "List stuck projects"),
        ];
        let next = custom::next_keys(&commands, &keys);
        if input.is_none() || !next.is_empty() {
            let mut choices: Vec<Value> = Vec::new();
            if keys.is_empty() {
                for (key, label) in built_in {
                    if !next.iter().any(|(own, _)| own == key) {
                        choices.push(json!({ "key": key, "label": label }));
                    }
                }
            }
            choices.extend(next.iter().map(|(key, label)| json!({ "key": key, "label": label })));
            return Ok(json!({ "input": {
                "kind": "key",
                "prompt": "Press key for agenda command",
                "value": "",
                "choices": choices,
                "context": { "keys": keys },
            } }));
        }
        match keys.as_str() {
            "a" => self.execute(commands::AGENDA_LIST, None)?,
            "t" => self.execute(commands::AGENDA_TODOS, None)?,
            "m" | "M" => {
                return Ok(json!({ "input": {
                    "kind": "text",
                    "prompt": "Match",
                    "value": "",
                    "context": { "keys": keys, "ask": "match" },
                } }))
            }
            "s" | "S" => return Ok(self.search_input(&keys, "")),
            "#" => {
                self.agenda_filters = Default::default();
                self.agenda_marks.clear();
                self.open_view(Query::Stuck)?
            }
            _ => return Err("No agenda command for this key".into()),
        }
        Ok(Value::Null)
    }

    /// The question of `org-search-view`: the search string, starting
    /// from `value`.
    fn search_input(&self, keys: &str, value: &str) -> Value {
        let prompt =
            if self.agenda.search.always_boolean { "[+-]Word/{Regexp} ..." } else { "Phrase or [+-]Word/{Regexp} ..." };
        json!({ "input": {
            "kind": "text",
            "prompt": prompt,
            "value": value,
            "context": { "keys": keys, "ask": "search" },
        } })
    }

    /// The agenda's filters (`org-agenda-filter` and its relatives): what
    /// each asks, then the filters set and the view shown again with the
    /// lines they keep. The result's `message` says what filters there
    /// are, as Emacs's mode line does.
    fn agenda_filter(&mut self, id: &str, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        use org_core::agenda::filter;
        self.view.as_ref().ok_or("No agenda is open")?;
        let answer = context.and_then(|context| context.input.clone());
        let step = context.and_then(|context| context.step.clone());
        let strip = context.and_then(|context| context.count).is_some_and(|count| count > 0);
        let mut filters = self.agenda_filters.clone();
        // What the cursor's line shows: its tags, category and top headline.
        let line = self.entry_at(context).and_then(|reference| {
            self.with_source(&reference.uri, |_, _, _, outline, _| {
                let index = views::resolve(&reference, outline)?;
                let entry = &outline.entries[index];
                let tags: Vec<String> = entry.tags.iter().map(|tag| tag.name.clone()).collect();
                Some((tags, entry.category.clone(), filter::top_headline(outline, index)))
            })
            .flatten()
        });
        let mut message: Option<String> = None;
        match id {
            commands::AGENDA_FILTER => {
                let represented = self.represented();
                let Some(answer) = answer else {
                    let mut completions: Vec<String> = represented.0.clone();
                    completions.extend(represented.1.iter().cloned());
                    let prompt = if strip { "Negative filter" } else { "Filter" };
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": format!("{prompt} [+cat-tag<0:10-/regexp/]"),
                        "value": filter::current_string(&filters),
                        "completions": completions,
                    } }));
                };
                let (parsed, ignored) = filter::parse(&answer, strip, &filters, &represented.0, &represented.1);
                filters = parsed;
                message = ignored.last().cloned();
            }
            commands::AGENDA_FILTER_TAG => {
                let exclude = step.as_deref() == Some("exclude") || (strip && step.is_none());
                match (step.as_deref(), answer.as_deref()) {
                    (Some("tag"), Some(tag)) => {
                        filters.tag = vec![format!(
                            "{}{tag}",
                            if context.and_then(|context| context.answers.as_deref()) == Some("-") { "-" } else { "+" }
                        )];
                        filters.expand_tags = true;
                    }
                    (Some("tag"), None) | (_, Some("q")) => return Ok(Value::Null),
                    (_, None) | (_, Some("-")) | (_, Some("+")) => {
                        let exclude = match answer.as_deref() {
                            Some("-") => true,
                            Some("+") => false,
                            _ => exclude,
                        };
                        return Ok(self.tag_filter_input(exclude));
                    }
                    (_, Some("\t")) => {
                        let tags = self.agenda_tags();
                        return Ok(json!({ "input": {
                            "kind": "text", "prompt": "Tag", "value": "", "completions": tags,
                            "context": { "step": "tag", "answers": if exclude { "-" } else { "+" } },
                        } }));
                    }
                    (_, Some("\\")) | (_, Some("\r")) => filters.tag.clear(),
                    (_, Some(".")) => {
                        filters.tag = line
                            .as_ref()
                            .map(|(tags, _, _)| tags.iter().map(|tag| format!("+{tag}")).collect())
                            .unwrap_or_default();
                        filters.expand_tags = true;
                    }
                    (_, Some(" ")) => {
                        filters.tag = vec![if exclude { "-" } else { "+" }.to_string()];
                        filters.expand_tags = true;
                    }
                    (_, Some(key)) => {
                        let tag = self
                            .agenda_tag_keys()
                            .into_iter()
                            .find(|(_, own)| own.to_string() == key)
                            .map(|(tag, _)| tag);
                        let Some(tag) = tag else {
                            return Err(Refusal::Failed(format!("Invalid tag selection character {key}")));
                        };
                        filters.tag = vec![format!("{}{tag}", if exclude { "-" } else { "+" })];
                        filters.expand_tags = true;
                    }
                }
            }
            commands::AGENDA_FILTER_CATEGORY => {
                let positive = filters.category.first().is_some_and(|first| first.starts_with('+'));
                if !filters.category.is_empty() && positive {
                    filters.category.clear();
                } else {
                    let category =
                        line.as_ref().map(|(_, category, _)| category.clone()).ok_or("No category at point")?;
                    if strip {
                        filters.category.insert(0, format!("-{category}"));
                    } else {
                        filters.category = vec![format!("+{category}")];
                    }
                }
            }
            commands::AGENDA_FILTER_REGEXP => {
                if !filters.regexp.is_empty() {
                    filters.regexp.clear();
                    message = Some("Regexp filter removed".to_string());
                } else {
                    let Some(answer) = answer else {
                        let prompt =
                            if strip { "Hide entries matching regexp" } else { "Narrow to entries matching regexp" };
                        return Ok(json!({ "input": { "kind": "text", "prompt": prompt, "value": "" } }));
                    };
                    filters.regexp.insert(0, format!("{}{answer}", if strip { "-" } else { "+" }));
                }
            }
            commands::AGENDA_FILTER_EFFORT => {
                let efforts = self.effort_values();
                match (step.as_deref(), answer.as_deref()) {
                    (None, None) => {
                        let choices: Vec<Value> = ["<", ">", "=", "_"]
                            .iter()
                            .map(|op| json!({ "key": op, "label": if *op == "_" { "remove the filter".to_string() } else { format!("effort {op}") } }))
                            .collect();
                        return Ok(json!({ "input": {
                            "kind": "key",
                            "prompt": "Effort operator? (> = or <)     or press `_' again to remove filter",
                            "value": "", "choices": choices, "context": { "step": "operator" },
                        } }));
                    }
                    (Some("operator"), Some("_")) => {
                        filters.effort.clear();
                        message = Some("Effort filter removed".to_string());
                    }
                    (Some("operator"), Some(op)) => {
                        let choices: Vec<Value> = efforts
                            .iter()
                            .take(10)
                            .enumerate()
                            .map(|(index, value)| json!({ "key": ((index + 1) % 10).to_string(), "label": value }))
                            .collect();
                        return Ok(json!({ "input": {
                            "kind": "key", "prompt": format!("Effort {op}"), "value": "", "choices": choices,
                            "context": { "step": "value", "answers": op },
                        } }));
                    }
                    (Some("value"), Some(key)) => {
                        let op = context.and_then(|context| context.answers.clone()).unwrap_or_else(|| "<".to_string());
                        let number: usize = key.parse().map_err(|_| Refusal::Failed(format!("No effort for {key}")))?;
                        let index = (number + 9) % 10;
                        let value = efforts.get(index).ok_or("No effort for this key")?;
                        filters.effort = vec![format!("{}{op}{value}", if strip { "-" } else { "+" })];
                    }
                    _ => return Ok(Value::Null),
                }
            }
            commands::AGENDA_FILTER_TOP_HEADLINE => {
                if filters.top_headline.is_some() {
                    filters.top_headline = None;
                } else {
                    let top = line.and_then(|(_, _, top)| top).ok_or("No top-level headline at point")?;
                    filters.top_headline = Some((top, strip));
                }
            }
            _ => {
                filters = Default::default();
                message = Some("All agenda filters removed".to_string());
            }
        }
        // Setting a filter turns entry text mode off
        // (`org-agenda-filter-apply`).
        let lists = |filters: &filter::Filters| {
            [filters.tag.clone(), filters.category.clone(), filters.regexp.clone(), filters.effort.clone()]
        };
        let (before, after) = (lists(&self.agenda_filters), lists(&filters));
        if before != after && after.iter().any(|list| !list.is_empty()) && self.agenda_entry_text.is_some() {
            self.agenda_entry_text = None;
            message.get_or_insert_with(|| "Entry text mode is off".to_string());
        }
        self.agenda_filters = filters;
        self.refresh_view();
        let message = message.or_else(|| {
            Some(if self.agenda_filters.is_empty() {
                "No agenda filters".to_string()
            } else {
                format!("Agenda filters: {}", self.agenda_filters.describe())
            })
        });
        Ok(json!({ "message": message }))
    }

    /// The question of `org-agenda-filter-by-tag` (`\\`): a tag by its key,
    /// or what the other keys do.
    fn tag_filter_input(&mut self, exclude: bool) -> Value {
        let mut choices: Vec<Value> = Vec::new();
        for (tag, key) in self.agenda_tag_keys() {
            choices.push(json!({ "key": key.to_string(), "label": tag }));
        }
        for (key, label) in [
            ("\t", "a tag, typed"),
            (" ", "any tag"),
            (".", "the tags of this line"),
            ("\\", "no tag filter"),
            (
                if exclude { "+" } else { "-" },
                if exclude { "filter rather than exclude" } else { "exclude rather than filter" },
            ),
            ("q", "quit"),
        ] {
            choices.push(json!({ "key": key, "label": label }));
        }
        let prompt = if exclude { "Exclude[+] by tag" } else { "Filter[-] by tag" };
        json!({ "input": {
            "kind": "key", "prompt": prompt, "value": "", "choices": choices,
            "context": { "step": if exclude { "exclude" } else { "filter" } },
        } })
    }

    /// The tags with a key in the agenda files' tables
    /// (`org-tag-alist-for-agenda`), each key once.
    fn agenda_tag_keys(&mut self) -> Vec<(String, char)> {
        let settings = self.edit_settings.tags.clone();
        let mut keys: Vec<(String, char)> = Vec::new();
        for (_, held) in self.agenda_files() {
            for entry in org_core::tags::current_alist(&held.outline().text, &settings) {
                if let org_core::tags::TagEntry::Tag(tag, Some(key)) = entry {
                    if !keys.iter().any(|(known, own)| *known == tag || *own == key) {
                        keys.push((tag, key));
                    }
                }
            }
        }
        keys
    }

    /// The efforts `_` offers: `Effort_ALL` of `org_global_properties`, or
    /// Org's list.
    fn effort_values(&self) -> Vec<String> {
        self.edit_settings
            .global_properties
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Effort_ALL"))
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| "0 0:10 0:30 1:00 2:00 3:00 4:00 5:00 6:00 7:00".to_string())
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    /// The tags and the categories of the open view's lines, for `/`.
    fn represented(&mut self) -> (Vec<String>, Vec<String>) {
        let Some(query) = self.view.as_ref().map(|open| open.view.query.clone()) else { return Default::default() };
        let mut files = self.agenda_files();
        let blocks = self.blocks(&query).unwrap_or_default();
        let search = if blocks.iter().any(|block| matches!(block.kind, views::BlockKind::Search { .. })) {
            self.search_files(&mut files)
        } else {
            None
        };
        let named: Vec<(&Outline, String, String)> = files
            .iter()
            .map(|(uri, held)| {
                let path = uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default();
                (held.outline(), path, uri::file_name(uri))
            })
            .collect();
        views::represented(&blocks, &named, search.as_ref(), self.now())
    }

    /// `org-agenda-manipulate-query` (`[`, `]`, `{`, `}`): in a search
    /// view, its search string asked for again with ` +`, ` -`, ` +{}` or
    /// ` -{}` after it (a space only when it was not Boolean); the answer
    /// (`input`) is searched for as the view was.
    fn manipulate_query(&mut self, command: &str, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let line = self.view_cursor_line(context);
        let open = self.view.as_ref().ok_or("No agenda is open")?;
        let block = line.and_then(|line| open.on_screen().line(line)).map_or(0, |line| line.block);
        let blocks = self.blocks(&open.view.query).map_err(Refusal::Failed)?;
        let (string, todo_only) = match blocks.get(block).map(|block| &block.kind) {
            Some(views::BlockKind::Search { string, todo_only }) => (string.clone(), *todo_only),
            Some(views::BlockKind::Agenda { .. }) => {
                return Err("Inactive timestamps in the agenda are not supported yet".into());
            }
            Some(views::BlockKind::Tags { .. }) => {
                return Err("Cannot manipulate query for tags-type agenda buffers".into())
            }
            _ => return Err("Cannot manipulate query for todo-type agenda buffers".into()),
        };
        if let Some(input) = context.and_then(|context| context.input.clone()) {
            return self.search(input, todo_only);
        }
        let added = match command {
            commands::AGENDA_QUERY_ADD => " +",
            commands::AGENDA_QUERY_SUBTRACT => " -",
            commands::AGENDA_QUERY_ADD_REGEXP => " +{}",
            _ => " -{}",
        };
        let string = org_core::agenda::search::manipulate(&string, added, &self.agenda.search);
        Ok(self.search_input(if todo_only { "S" } else { "s" }, &string))
    }

    /// `org-search-view` for `string`, which Org must be able to read.
    fn search(&mut self, string: String, todo_only: bool) -> std::result::Result<Value, Refusal> {
        if !string.chars().any(|c| !c.is_whitespace()) {
            return Ok(Value::Null);
        }
        org_core::agenda::search::parse(&string, todo_only, &self.agenda.search).map_err(Refusal::Failed)?;
        self.agenda_filters = Default::default();
        self.agenda_marks.clear();
        self.open_view(Query::Search { string, todo_only })?;
        Ok(Value::Null)
    }

    /// The line of the open view the cursor of `context` is on.
    fn view_cursor_line(&self, context: Option<&Context>) -> Option<u32> {
        let open = self.view.as_ref()?;
        context.filter(|context| context.text_document.uri == open.uri).map(|context| context.position.line)
    }

    /// The agenda block at `line` of the open view (the first one when
    /// no line is given): which block, its first day and its span.
    fn agenda_block(&self, line: Option<u32>) -> std::result::Result<(usize, Day, Span), Refusal> {
        let open = self.view.as_ref().ok_or("No agenda is open")?;
        let block = line
            .and_then(|line| open.on_screen().line(line))
            .map_or_else(|| open.view.blocks.iter().position(Option::is_some).unwrap_or(0), |line| line.block);
        match open.view.blocks.get(block).copied().flatten() {
            Some((start, span)) => Ok((block, start, span)),
            None => Err(match open.view.query {
                Query::Todo => "The TODO list has no dates to move through",
                Query::Tags { .. } => "A tags view has no dates to move through",
                Query::Search { .. } => "A search view has no dates to move through",
                Query::Stuck => "The stuck projects have no dates to move through",
                _ => "This block has no dates to move through",
            }
            .into()),
        }
    }

    /// The open view with its agenda block `block` on other days.
    fn moved(&self, block: usize, start: Option<Day>, span: Span) -> Query {
        match self.view.as_ref().map(|open| &open.view.query) {
            Some(Query::Custom { keys, .. }) => {
                Query::Custom { keys: keys.clone(), moved: Some(views::Moved { block, start, span }) }
            }
            _ => Query::Agenda { start, span },
        }
    }

    /// `org-agenda-start-day`: where a new agenda starts, if not today.
    fn start_day(&self) -> Option<Day> {
        let text = self.agenda_options.start_day.as_deref()?;
        let read = org_core::read_date::read_date(text, None, self.date_time(), &self.read_date).ok()?;
        Some(read.date.to_day())
    }

    /// The entry on the agenda line of `context`.
    fn entry_at(&self, context: Option<&Context>) -> Option<EntryRef> {
        let context = context?;
        self.view_line(&context.text_document.uri, context.position.line)?.1.entry.clone()
    }

    /// `org-refile` (`C-c C-w`), and `org-agenda-refile` on an agenda
    /// line: without an answer it offers the targets, `{ "input": {
    /// "kind": "choice", "prompt", "choices" } }`; with one it moves the
    /// subtree under the target it names, in its file or another.
    fn refile(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        self.last = None;
        let uri = &context.text_document.uri;
        let from_agenda = self.is_view(uri);
        // The entry: its file and a place in it.
        let (file, offset) = if from_agenda {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            let offset = self
                .with_source(&reference.uri, |_, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| outline.entries[entry].offset)
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            (reference.uri, offset)
        } else {
            let document = self.document(uri).ok_or("The document is not open")?;
            (uri.clone(), document.index.offset(&document.text, context.position, self.client.encoding))
        };
        self.refile_from(file, offset, None, from_agenda, context.input.as_deref())
    }

    /// `org-archive-subtree` (`C-c C-x C-s`) and its relatives, in an Org
    /// file or on an agenda line (`org-agenda-archive`). The archive file
    /// gets an edit when an editor has it open, and is written here when
    /// not, as Emacs saves it.
    fn archive(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        self.last = None;
        let yes_or_no = |prompt: &str, answers: Option<String>| {
            let mut input = json!({
                "kind": "key",
                "prompt": prompt,
                "value": "",
                "choices": [{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }],
            });
            if let Some(answers) = answers {
                input["context"] = json!({ "answers": answers });
            }
            json!({ "input": input })
        };
        let (defaults, default_command) = self.archive.clone();
        let id = match id {
            commands::ARCHIVE_CONFIRM => match context.input.as_deref() {
                None => return Ok(yes_or_no("Archive this subtree or entry?", None)),
                Some("y" | "Y" | " ") => commands::ARCHIVE_DEFAULT,
                Some(_) => return Err("Abort".into()),
            },
            other => other,
        };
        let id = match (id, default_command.as_str()) {
            (commands::ARCHIVE_DEFAULT, "sibling") => commands::ARCHIVE_SIBLING,
            (commands::ARCHIVE_DEFAULT, "tag") => commands::ARCHIVE_TAG,
            (commands::ARCHIVE_DEFAULT, _) => commands::ARCHIVE_SUBTREE,
            (other, _) => other,
        };
        // The entry: its file and a place in it.
        let uri = &context.text_document.uri;
        let (file, offset) = if self.is_view(uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            let offset = self
                .with_source(&reference.uri, |_, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| outline.entries[entry].offset)
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            (reference.uri, offset)
        } else {
            let document = self.document(uri).ok_or("The document is not open")?;
            (uri.clone(), document.index.offset(&document.text, context.position, self.client.encoding))
        };
        let (text, version, keywords) = self
            .with_source(&file, |text, _, parse, _, version| (text.to_string(), version, parse.todo_keywords().clone()))
            .ok_or("Cannot read the file of this entry")?;
        let adapt = self.adapt_in(&text);
        let settings = ArchiveSettings {
            levels: RefileSettings { adapt, ..defaults.levels },
            todo: todo::Settings { adapt, ..defaults.todo.clone() },
            ..defaults
        };
        let now = self.date_time();
        let path = uri::to_path(&file);
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let archived = match id {
            commands::ARCHIVE_SIBLING => {
                let (source, cursor) = archive::to_sibling(&text, offset, &keywords, now, &settings)?;
                Archived { source, files: Vec::new(), message: String::new(), cursor }
            }
            commands::ARCHIVE_TAG => {
                let (source, message) = archive::toggle_tag(&text, offset, &settings)?;
                Archived { source, files: Vec::new(), message, cursor: offset }
            }
            commands::ARCHIVE_SUBTREE | commands::ARCHIVE_DONE | commands::ARCHIVE_TAG_DONE => {
                let path = path.ok_or("No file associated to buffer")?;
                let where_ = archive::Context { path: &path, home: home.as_deref(), now };
                // Another file as an editor has it, or as it is on disk.
                let open: Vec<(PathBuf, String)> = self
                    .documents
                    .keys()
                    .filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, self.open_text(key)?)))
                    .collect();
                let mut read = |wanted: &Path| match open.iter().find(|(path, _)| path == wanted) {
                    Some((_, text)) => Some(text.clone()),
                    None => std::fs::read_to_string(wanted).ok(),
                };
                if id == commands::ARCHIVE_SUBTREE {
                    archive::archive_subtree(&text, offset, &keywords, &where_, &settings, &mut read)?
                } else {
                    let mut answered = context.answers.clone().unwrap_or_default();
                    if let Some(input) = context.input.as_deref() {
                        answered.push(if matches!(input, "y" | "Y" | " ") { 'y' } else { 'n' });
                    }
                    let answers: Vec<bool> = answered.chars().map(|answer| answer == 'y').collect();
                    let tag = id == commands::ARCHIVE_TAG_DONE;
                    match archive::archive_all(&text, offset, &keywords, &where_, &settings, tag, &answers, &mut read)?
                    {
                        AllOutcome::Ask(question) => return Ok(yes_or_no(&question, Some(answered))),
                        AllOutcome::Done(done) => done,
                    }
                }
            }
            _ => return Err(format!("Unknown command {id}").into()),
        };
        // The entry's file, and the archive files open in an editor, get
        // edits; the others are written.
        let mut documents = vec![(file.clone(), version, self.text_edits(&text, &archived.source))];
        for written in &archived.files {
            let open = uri::from_path(&written.path).and_then(|found| {
                let key = self
                    .documents
                    .keys()
                    .find(|key| {
                        key.parse::<Uri>().ok().and_then(|uri| uri::to_path(&uri)).as_ref() == Some(&written.path)
                    })
                    .cloned()?;
                Some((
                    key.parse::<Uri>().unwrap_or(found),
                    self.documents[&key].version,
                    self.open_text(&key).unwrap_or_default(),
                ))
            });
            match open {
                Some((uri, version, now)) => documents.push((uri, Some(version), self.text_edits(&now, &written.text))),
                None => {
                    if let Some(directory) = written.path.parent() {
                        let _ = std::fs::create_dir_all(directory);
                    }
                    std::fs::write(&written.path, &written.text)
                        .map_err(|error| format!("Cannot write {}: {error}", written.path.display()))?;
                }
            }
        }
        documents.retain(|(_, _, edits)| !edits.is_empty());
        if !archived.message.is_empty() {
            self.message = Some(archived.message);
        }
        if documents.is_empty() {
            self.refresh_view();
            return Ok(Value::Null);
        }
        for (uri, _, _) in &documents {
            self.edited.insert(uri.as_str().to_string());
        }
        let edit = self.workspace_edits(documents);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
        Ok(Value::Null)
    }

    /// Refile the entry at byte `offset` of `file`, which reads `text`
    /// when that is given rather than as it is now (an entry a capture is
    /// about to add); the edits still turn the file as it is now into the
    /// result.
    fn refile_from(
        &mut self,
        file: Uri,
        offset: usize,
        text: Option<String>,
        from_agenda: bool,
        answer: Option<&str>,
    ) -> std::result::Result<Value, Refusal> {
        // The files the targets come from, the entry's own first.
        let (targets, outline_path, settings) = self.refile.clone();
        let mut uris = vec![file];
        let add = |uris: &mut Vec<Uri>, found: Uri| {
            let same = |known: &Uri| {
                known == &found || (uri::to_path(known).is_some() && uri::to_path(known) == uri::to_path(&found))
            };
            match uris.iter().position(same) {
                Some(index) => index,
                None => {
                    uris.push(found);
                    uris.len() - 1
                }
            }
        };
        let mut selections = Vec::new();
        for target in &targets {
            let found: Vec<Uri> = match &target.files {
                RefileFiles::Current => vec![self.refile_current.clone().unwrap_or_else(|| uris[0].clone())],
                RefileFiles::Agenda => self.agenda_files().into_iter().map(|(uri, _)| uri).collect(),
                RefileFiles::Paths(patterns) => patterns
                    .iter()
                    .flat_map(|pattern| self.workspace.org_files(pattern))
                    .filter_map(|path| uri::from_path(&path))
                    .collect(),
            };
            let indexes = found.into_iter().map(|found| add(&mut uris, found)).collect();
            selections.push((target.select.clone(), indexes));
        }
        struct Loaded {
            text: String,
            keywords: TodoKeywords,
            title: Option<String>,
            version: Option<i32>,
            name: String,
            path: String,
        }
        let mut loaded = Vec::new();
        for (index, file) in uris.iter().enumerate() {
            let read = self.with_source(file, |text, _, parse, _, version| Loaded {
                text: text.to_string(),
                keywords: parse.todo_keywords().clone(),
                title: refile::file_title(parse),
                version,
                name: uri::file_name(file),
                path: uri::to_path(file).map_or_else(|| file.as_str().to_string(), |path| path.display().to_string()),
            });
            match read {
                Some(read) => loaded.push(Some(read)),
                // A capture's target file may not exist yet.
                None if index == 0 && text.is_some() => loaded.push(Some(Loaded {
                    text: String::new(),
                    keywords: self.todo.clone(),
                    title: None,
                    version: None,
                    name: uri::file_name(file),
                    path: uri::to_path(file)
                        .map_or_else(|| file.as_str().to_string(), |path| path.display().to_string()),
                })),
                None if index == 0 => return Err("Cannot read the file of this entry".into()),
                None => loaded.push(None),
            }
        }
        // The file as it is now, which the edits change.
        let as_now = loaded[0].as_ref().map(|file| file.text.clone()).unwrap_or_default();
        if let (Some(text), Some(Some(file))) = (text, loaded.first_mut()) {
            file.text = text;
        }
        let files: Vec<Option<refile::File>> = loaded
            .iter()
            .map(|file| {
                file.as_ref().map(|file| refile::File {
                    text: &file.text,
                    keywords: &file.keywords,
                    name: &file.name,
                    path: &file.path,
                    title: file.title.as_deref(),
                })
            })
            .collect();
        // Files that could not be read have no targets.
        let readable: Vec<refile::File> = files.iter().flatten().copied().collect();
        let position: Vec<Option<usize>> = {
            let mut next = 0;
            files
                .iter()
                .map(|file| {
                    file.as_ref().map(|_| {
                        next += 1;
                        next - 1
                    })
                })
                .collect()
        };
        let selections: Vec<(refile::Select, Vec<usize>)> = selections
            .into_iter()
            .map(|(select, indexes): (refile::Select, Vec<usize>)| {
                (select, indexes.into_iter().filter_map(|index| position[index]).collect())
            })
            .collect();
        // In the agenda no file is the current one: every target names its file.
        let current = (!from_agenda).then_some(0);
        let all = refile::all_targets(&selections, &readable, current, outline_path);
        if all.is_empty() {
            return Err("No refile targets".into());
        }
        let Some(answer) = answer else {
            let prompt = if from_agenda {
                "Refile to".to_string()
            } else {
                format!(
                    "Refile subtree \"{}\" to",
                    refile::heading_text(readable[0].text, offset, readable[0].keywords)
                )
            };
            let choices: Vec<Value> =
                refile::names(&all).into_iter().map(|name| json!({ "key": name, "label": name })).collect();
            return Ok(json!({ "input": { "kind": "choice", "prompt": prompt, "value": "", "choices": choices } }));
        };
        let (target_file, target) = refile::find(&all, answer).ok_or("Invalid target location")?;
        // Back from readable files to all of them.
        let target_file = position.iter().position(|at| *at == Some(*target_file)).expect("a readable file");
        let source = &readable[0].text;
        let other = (target_file != 0).then(|| loaded[target_file].as_ref().expect("read").text.as_str());
        // The target file's `org-adapt-indentation`, as Emacs pastes there.
        let settings = refile::Settings { adapt: self.adapt_in(other.unwrap_or(source)), ..settings };
        let refiled = refile::refile(source, offset, other, target.offset, &settings)?;
        let target_path = uri::to_path(&uris[target_file])
            .map_or_else(|| uris[target_file].as_str().to_string(), |path| path.display().to_string());
        let message = format!("Refile to \"{}\" in file {}: done", target.name, target_path);
        let mut documents = vec![(
            uris[0].clone(),
            loaded[0].as_ref().expect("read").version,
            self.text_edits(&as_now, &refiled.source),
        )];
        if let (Some(changed), Some(other)) = (&refiled.target, other) {
            let version = loaded[target_file].as_ref().expect("read").version;
            documents.push((uris[target_file].clone(), version, self.text_edits(other, changed)));
        }
        documents.retain(|(_, _, edits)| !edits.is_empty());
        if documents.is_empty() {
            return Ok(Value::Null);
        }
        for (uri, _, _) in &documents {
            self.edited.insert(uri.as_str().to_string());
        }
        let edit = self.workspace_edits(documents);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
        self.message = Some(message);
        Ok(Value::Null)
    }

    /// A path as Org names a capture target (`org-capture-expand-file`):
    /// empty for `org-default-notes-file`, relative to `org-directory`.
    fn capture_file(&self, path: &str) -> PathBuf {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let expand = |path: &str| match (path.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(path),
        };
        if path.is_empty() {
            return expand(&self.default_notes_file);
        }
        let path = expand(path);
        if path.is_absolute() {
            path
        } else {
            expand(&self.org_directory).join(path)
        }
    }

    /// `org-capture`: pick a template by its keys, answer its questions,
    /// and show the capture buffer, a file of its own with the template
    /// as it will stand in its target. Each step asks with an `input`
    /// whose `context` the editor sends back.
    fn capture(&mut self, context: &CaptureContext) -> std::result::Result<Value, Refusal> {
        let templates = self.capture_templates.clone();
        let answering = context.answers.clone();
        let mut keys = context.keys.clone().unwrap_or_default();
        if answering.is_none() {
            keys.push_str(context.input.as_deref().unwrap_or(""));
        }
        let chosen = templates.iter().find(|template| template.key == keys && template.template.is_some());
        let Some(template) = chosen.cloned() else {
            // The templates and groups one key further.
            let next: Vec<Value> = templates
                .iter()
                .filter(|template| {
                    template.key.starts_with(&keys) && template.key.chars().count() == keys.chars().count() + 1
                })
                .map(|template| {
                    let key = template.key.chars().last().map(String::from).unwrap_or_default();
                    let label = if template.template.is_some() {
                        template.description.clone()
                    } else {
                        format!("{}...", template.description)
                    };
                    json!({ "key": key, "label": label })
                })
                .collect();
            if next.is_empty() {
                return Err(
                    if keys.is_empty() { "No capture templates" } else { "No capture template for this key" }.into()
                );
            }
            return Ok(json!({ "input": {
                "kind": "key",
                "prompt": "Select a capture template",
                "value": "",
                "choices": next,
                "context": { "keys": keys },
            } }));
        };
        let mut answers = answering.clone().unwrap_or_default();
        if let (Some(_), Some(answer)) = (&answering, context.input.clone()) {
            answers.push(answer);
        }
        // Where the capture started, for `%f`, `%F` and `%a`.
        let from = context.text_document.as_ref().map(|document| &document.uri).filter(|uri| !self.is_view(uri));
        let file = from.map(|uri| {
            (uri::file_name(uri), uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default())
        });
        let annotation = match (from, context.position) {
            (Some(uri), Some(position)) => self.link_to(uri, position),
            _ => String::new(),
        };
        let fill_context = capture::Context {
            now: self.date_time(),
            file,
            initial: String::new(),
            annotation,
            user_name: String::new(),
        };
        let text = template.template.as_ref().map(|text| text.text()).unwrap_or_default();
        let filled = match capture::fill(&text, &fill_context, &answers, &self.read_date)? {
            capture::Filled::Text(filled) => filled,
            capture::Filled::Ask(question) => {
                let context = json!({ "keys": keys, "answers": answers });
                return Ok(match question {
                    capture::Question::Text { prompt, default, .. } => {
                        let prompt = if default.is_empty() { prompt } else { format!("{prompt} (default {default})") };
                        json!({ "input": { "kind": "text", "prompt": prompt, "value": "", "context": context } })
                    }
                    capture::Question::Date { prompt, .. } => {
                        let prompt = prompt.unwrap_or_else(|| "Date".to_string());
                        json!({ "input": { "kind": "date", "prompt": prompt, "value": "", "context": context } })
                    }
                });
            }
        };
        // The target.
        let (kind, target) = capture_spec(&template)?;
        let path = self.capture_file(template.target.get(1).map(String::as_str).unwrap_or(""));
        let target_uri = uri::from_path(&path).ok_or("Cannot name the target file")?;
        let (snapshot, version, keywords) = self
            .with_source(&target_uri, |text, _, parse, _, version| {
                (text.to_string(), version, parse.todo_keywords().clone())
            })
            .unwrap_or_else(|| (String::new(), None, self.todo.clone()));
        let properties = capture::Properties {
            prepend: template.prepend.unwrap_or(false),
            empty_lines: template.empty_lines,
            empty_lines_before: template.empty_lines_before,
            empty_lines_after: template.empty_lines_after,
        };
        let levels = refile::Settings { adapt: self.adapt_in(&snapshot), ..self.refile.2 };
        let placed = capture::place(&snapshot, &keywords, &target, kind, &filled, &properties, &levels)?;
        if template.immediate_finish == Some(true) {
            let done = placed.finish(placed.shown());
            self.apply_capture(&target_uri, version, &snapshot, &done);
            return Ok(Value::Null);
        }
        // The capture buffer: a file named after the target, in a
        // directory of the server's own, one for each capture, so that an
        // editor that still has the last one open does not mix them up.
        self.captures += 1;
        let directory = self.capture_directory.join(self.captures.to_string());
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        // It is an Org buffer whatever the target is called (Org's default
        // notes file is `~/.notes`), and editors tell that by the name.
        let mut name = format!("{}{}", CAPTURE_PREFIX, uri::file_name(&target_uri));
        if !name.ends_with(".org") {
            name.push_str(".org");
        }
        let buffer = directory.join(name);
        std::fs::write(&buffer, placed.shown()).map_err(|error| error.to_string())?;
        let buffer_uri = uri::from_path(&buffer).ok_or("Cannot name the capture buffer")?;
        let shown = placed.shown();
        let cursor = LineIndex::new(shown).position(shown, placed.cursor - placed.start, self.client.encoding);
        self.capture = Some(Capturing {
            uri: buffer_uri.clone(),
            target: target_uri,
            snapshot,
            placed,
            spec: (target, kind, properties),
        });
        let params = ShowDocumentParams {
            uri: buffer_uri,
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(cursor, cursor)),
        };
        self.send_request::<ShowDocument>(params, None);
        Ok(Value::Null)
    }

    /// `org-add-log-note`: a buffer of its own for the note a TODO state
    /// change wants, which `org.capture.finish` stores and
    /// `org.capture.abort` cancels. Its name starts as a capture buffer's,
    /// so editors treat it as one.
    fn open_note(
        &mut self,
        context: &Context,
        id: &str,
        wanted: org_core::todo::WantedNote,
    ) -> std::result::Result<Value, Refusal> {
        self.captures += 1;
        let directory = self.capture_directory.join(self.captures.to_string());
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let buffer = directory.join(format!("{CAPTURE_PREFIX}Note.org"));
        // The note goes on the third line, as in Emacs; an editor that takes
        // the last newline for the end of the file still has that line.
        let text = format!("# Insert note for {}.\n\n\n", wanted.purpose);
        std::fs::write(&buffer, &text).map_err(|error| error.to_string())?;
        let uri = uri::from_path(&buffer).ok_or("Cannot name the note buffer")?;
        self.note = Some(Noting {
            uri: uri.clone(),
            id: id.to_string(),
            context: context.clone(),
            state: wanted.state.unwrap_or_default(),
        });
        let end = Position::new(2, 0);
        let params =
            ShowDocumentParams { uri, external: None, take_focus: Some(true), selection: Some(Range::new(end, end)) };
        self.send_request::<ShowDocument>(params, None);
        Ok(Value::Null)
    }

    /// `org-store-log-note`: the state change with the note written in its
    /// buffer, or without any (cancelled), and the buffer closed.
    fn finish_note(
        &mut self,
        note: Noting,
        cancelled: bool,
        text: Option<String>,
    ) -> std::result::Result<Value, Refusal> {
        let written = match (text, self.document(&note.uri)) {
            (Some(text), _) => text,
            (None, Some(document)) => document.text.clone(),
            (None, None) => {
                uri::to_path(&note.uri).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default()
            }
        };
        // The comment lines at the top go, and blanks at the end.
        let mut text = written.as_str();
        while text.starts_with("# ") {
            text = text.split_once('\n').map_or("", |(_, rest)| rest);
            text = text.trim_start_matches([' ', '\t', '\n']);
        }
        let text = text.trim_end();
        let command = match note.id.as_str() {
            commands::ADD_NOTE => EditCommand::Note,
            _ => EditCommand::Todo(TodoTarget::State((!note.state.is_empty()).then(|| note.state.clone()))),
        };
        let extra = Extra { note: Some(text.to_string()), skip_note: cancelled, ..Extra::default() };
        // The change is the file's, as if made where it was asked for.
        self.origin = Some(note.context.text_document.uri.as_str().to_string());
        self.edit_with(&note.context, &note.id, Some(command), &extra)?;
        Ok(json!({ "close": note.uri }))
    }

    /// A link to the place `position` of `uri` (`org-store-link`): to its
    /// headline when it has one.
    fn link_to(&mut self, uri: &Uri, position: Position) -> String {
        let Some(path) = uri::to_path(uri) else { return String::new() };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let shown = match home.as_ref().and_then(|home| path.strip_prefix(home).ok()) {
            Some(rest) => format!("~/{}", rest.display()),
            None => path.display().to_string(),
        };
        let encoding = self.client.encoding;
        let heading = self.with_source(uri, |text, index, parse, _, _| {
            let offset = index.offset(text, position, encoding);
            refile::heading_text(text, offset, parse.todo_keywords())
        });
        match heading.filter(|heading| !heading.is_empty()) {
            Some(heading) => format!("[[file:{shown}::*{heading}][{heading}]]"),
            None => format!("[[file:{shown}]]"),
        }
    }

    /// The text a command came with, when it runs in the capture buffer.
    fn capture_buffer_text(&self, context: Option<&Context>) -> Option<String> {
        let context = context?;
        let capture = self.capture.as_ref()?;
        (context.text_document.uri == capture.uri).then(|| context.text.clone()).flatten()
    }

    /// The text of the capture buffer, as the editor has it: the text the
    /// command came with, or the server's copy.
    fn capture_text(&self, capture: &Capturing, given: Option<String>) -> String {
        match (given, self.document(&capture.uri)) {
            (Some(text), _) => text,
            (None, Some(document)) => document.text.clone(),
            (None, None) => {
                uri::to_path(&capture.uri).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default()
            }
        }
    }

    /// The target file once the capture buffer's text goes in: where the
    /// template was placed, or placed again when the file has changed.
    fn captured(
        &mut self,
        capture: &Capturing,
        given: Option<String>,
    ) -> std::result::Result<(String, Option<i32>, String), Refusal> {
        let edited = self.capture_text(capture, given);
        let (now, version, keywords) = self
            .with_source(&capture.target, |text, _, parse, _, version| {
                (text.to_string(), version, parse.todo_keywords().clone())
            })
            .unwrap_or_else(|| (String::new(), None, self.todo.clone()));
        let done = if now == capture.snapshot {
            capture.placed.finish(&edited)
        } else {
            let (target, kind, properties) = &capture.spec;
            let levels = refile::Settings { adapt: self.adapt_in(&now), ..self.refile.2 };
            let placed = capture::place(&now, &keywords, target, *kind, &edited, properties, &levels)?;
            placed.finish(placed.shown())
        };
        Ok((now, version, done))
    }

    /// Ask the editor to turn `now` into `done` in the target file. A file
    /// that does not exist yet is written here, as Emacs saves it: no
    /// editor has it open, and an edit cannot make it.
    fn apply_capture(&mut self, target: &Uri, version: Option<i32>, now: &str, done: &str) {
        let missing = uri::to_path(target).filter(|path| self.document(target).is_none() && !path.exists());
        if let Some(path) = missing {
            if let Some(directory) = path.parent() {
                let _ = std::fs::create_dir_all(directory);
            }
            if let Err(error) = std::fs::write(&path, done) {
                self.show_message(MessageType::ERROR, format!("Cannot write {}: {error}", path.display()));
            }
            self.refresh_view();
            return;
        }
        let edits = self.text_edits(now, done);
        if edits.is_empty() {
            return;
        }
        self.edited.insert(target.as_str().to_string());
        let edit = self.workspace_edit(target, version, edits);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
    }

    /// `org-capture-finalize` (`C-c C-c`): the text of the capture buffer
    /// goes to its target, and the buffer closes. The target is left for
    /// the user to save.
    fn finish_capture(&mut self, text: Option<String>) -> std::result::Result<Value, Refusal> {
        let capture = self.capture.take().ok_or("No capture is going on")?;
        let (now, version, done) = self.captured(&capture, text)?;
        self.apply_capture(&capture.target, version, &now, &done);
        Ok(json!({ "close": capture.uri }))
    }

    /// `org-capture-refile` (`C-c C-w`): the entry goes to its target, and
    /// from there under a refile target.
    fn refile_capture(&mut self, answer: Option<&str>, text: Option<String>) -> std::result::Result<Value, Refusal> {
        let capture = self.capture.take().ok_or("No capture is going on")?;
        if capture.spec.1 != capture::Kind::Entry {
            self.capture = Some(capture);
            return Err("Refiling from a capture buffer makes only sense for `entry'-type templates".into());
        }
        let (_, _, done) = self.captured(&capture, text)?;
        // The entry starts where the buffer's text does, after any blank lines.
        let start = capture.placed.start + done[capture.placed.start..].len()
            - done[capture.placed.start..].trim_start_matches(['\n', ' ', '\t']).len();
        let uri = capture.uri.clone();
        let target = capture.target.clone();
        let result = self.refile_from(target, start, Some(done), false, answer);
        match result {
            // Still asking where to: the capture stays.
            Ok(value) if value.get("input").is_some() => {
                self.capture = Some(capture);
                Ok(value)
            }
            Ok(_) => Ok(json!({ "close": uri })),
            Err(refusal) => {
                self.capture = Some(capture);
                Err(refusal)
            }
        }
    }

    /// `org-priority` (`C-c ,`), and `org-agenda-priority` on an agenda
    /// line: without an answer it says what to ask, `{ "input": { "kind":
    /// "priority", "prompt", "value" } }`; with `input` it sets the
    /// priority the answer names, or removes it for a space.
    fn ask_priority(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let settings = self.edit_settings.priorities;
        // The priorities of the file the entry is in.
        let priorities = if self.is_view(uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            self.with_source(&reference.uri, |_, _, parse, _, _| edit::document_priorities(parse))
                .ok_or("Cannot read the file of this entry")?
        } else {
            edit::document_priorities(&self.document(uri).ok_or("The document is not open")?.parse)
        }
        .unwrap_or(settings);
        let Some(answer) = &context.input else {
            let question = priority::question(&priorities);
            let mut input = json!({ "kind": "priority", "prompt": question, "value": "" });
            // A client that asks with buttons gets the priorities to pick.
            if self.client.message_prompts {
                let name = |value: i64| match u8::try_from(value) {
                    Ok(code) if !priorities.numeric() => char::from(code).to_string(),
                    _ => value.to_string(),
                };
                let mut choices: Vec<Value> = (priorities.highest..=priorities.lowest)
                    .map(|value| json!({ "key": name(value), "label": name(value) }))
                    .collect();
                choices.push(json!({ "key": " ", "label": "Remove" }));
                input["choices"] = json!(choices);
            }
            return Ok(json!({ "input": input }));
        };
        let action = priority::read_answer(answer, &priorities)?;
        self.edit(context, commands::PRIORITY, Some(EditCommand::Priority(action)))?;
        Ok(Value::Null)
    }

    /// Run the editing command `id` at `context`, which is a place in an
    /// Org file or a line of the agenda. The server works out the new
    /// text and asks the client to apply the difference. `given` is the
    /// change to make, for a command that asked what it is.
    fn edit(&mut self, context: &Context, id: &str, given: Option<EditCommand>) -> std::result::Result<(), Refusal> {
        self.edit_with(context, id, given, &Extra::default()).map(|_| ())
    }

    /// [`Server::edit`] for a change to the TODO state, which may want a
    /// note: say what to ask for it, `{ "input": { "kind": "note",
    /// "prompt", "value", "context": { "state" } } }`, unless `extra`
    /// gives its text. With `extra.key`, the state is the one that key
    /// selects (`org-fast-todo-selection`).
    fn edit_with(
        &mut self,
        context: &Context,
        id: &str,
        given: Option<EditCommand>,
        extra: &Extra,
    ) -> std::result::Result<Value, Refusal> {
        let encoding = self.client.encoding;
        let (settings, now) = (self.edit_settings.clone(), self.date_time());
        let (display, adapt) = (self.display, self.indent.adapt);
        let uri = context.text_document.uri.clone();
        let position = context.position;
        // On an agenda line the command goes to the entry in its file.
        let on_view = self.is_view(&uri);
        // The line the command is meant for, which is not always the one
        // the cursor is reported on.
        let line: Option<(u32, ViewLine)> =
            self.view_line(&uri, position.line).map(|(number, line)| (number, line.clone()));
        let dated_view = self.view_at(&uri).is_some_and(|view| matches!(view.query, Query::Agenda { .. }));
        let (file, reference, command) = if on_view {
            let reference = line.as_ref().and_then(|(_, line)| line.entry.clone()).ok_or("No entry on this line")?;
            let command = match id {
                id if commands::AGENDA_DATE_COMMANDS.contains(&id) && !dated_view => {
                    return Err("The TODO list shows no dates to change".into());
                }
                id if commands::AGENDA_DATE_COMMANDS.contains(&id) => {
                    use org_core::stamp::Unit;
                    let unit = match id {
                        commands::DATE_LATER_HOURS | commands::DATE_EARLIER_HOURS => Unit::Hour,
                        commands::DATE_LATER_MINUTES | commands::DATE_EARLIER_MINUTES => Unit::Minute,
                        _ => self.date_unit_before.unwrap_or(Unit::Day),
                    };
                    if unit != Unit::Day {
                        self.date_unit = Some(unit);
                    }
                    let forward =
                        matches!(id, commands::DATE_LATER | commands::DATE_LATER_HOURS | commands::DATE_LATER_MINUTES);
                    EditCommand::AgendaDate { forward, unit }
                }
                _ if given.is_some() => given.clone().expect("given"),
                commands::TODO_NEXT | commands::TODO_PREVIOUS | commands::PRIORITY_UP | commands::PRIORITY_DOWN => {
                    actions::command(id).ok_or("Unknown command")?
                }
                _ => return Err(Refusal::NotHere("This command works in Org files".to_string())),
            };
            (reference.uri.clone(), Some(reference), command)
        } else if given.is_none() && commands::AGENDA_DATE_COMMANDS[2..].contains(&id) {
            return Err(Refusal::NotHere("This command works in the agenda".to_string()));
        } else {
            (uri, None, given.map_or_else(|| actions::command(id).ok_or("Unknown command"), Ok)?)
        };

        let key = file.as_str().to_string();
        let previous = self.last.take();
        let count = context.count;
        let found = self.with_source(&file, |text, index, parse, outline, version| {
            // The place the command acts on, and the headline of its entry.
            let (offset, heading) = match &reference {
                Some(reference) => {
                    let entry = views::resolve(reference, outline).ok_or("The entry is gone; rebuild the agenda")?;
                    let heading = outline.entries[entry].offset;
                    let offset = match (&command, reference.marker) {
                        // The timestamp is where it was, seen from the headline.
                        (EditCommand::AgendaDate { .. }, Some(marker)) => {
                            heading + marker.saturating_sub(reference.offset)
                        }
                        (EditCommand::AgendaDate { .. }, None) => return Err("Cannot find time stamp".into()),
                        _ => heading,
                    };
                    (offset, Some(heading))
                }
                None => {
                    let offset = index.offset(text, position, encoding);
                    (offset, outline.entry_at(offset).map(|entry| outline.entries[entry].offset))
                }
            };
            let last = Last { command: id.to_string(), file: key.clone(), heading };
            // The state the key selects; a key that selects none quits.
            let command = match &extra.key {
                Some(answer) => {
                    match todo::fast_selection(text, parse.todo_keywords(), offset, answer).map_err(Refusal::NotHere)? {
                        Some(target) => EditCommand::Todo(target),
                        None => return Ok((last, None)),
                    }
                }
                None => command.clone(),
            };
            let call = Call {
                count,
                repeated: previous.as_ref() == Some(&last),
                now,
                note: extra.note.clone(),
                skip_note: extra.skip_note,
                time_only: extra.time_only,
            };
            // A state asked for by name: Emacs says so when the entry has it.
            let requested = match &command {
                EditCommand::Todo(TodoTarget::State(state)) => Some(state.clone()),
                _ => None,
            };
            let mut settings = settings.clone();
            settings.todo.adapt = display.adapt(text, adapt);
            let outcome = edit::run(&command, text, parse, offset, &call, &settings).map(|outcome| {
                let edits: Vec<TextEdit> = actions::edits_between(text, &outcome.text)
                    .into_iter()
                    .map(|edit| TextEdit { range: index.range(text, edit.range, encoding), new_text: edit.new_text })
                    .collect();
                let message = outcome.message.clone().or_else(|| {
                    heading.and_then(|heading| {
                        headline_message(
                            text,
                            &outcome.text,
                            heading,
                            parse.todo_keywords(),
                            reference.is_some(),
                            requested.as_ref(),
                        )
                    })
                });
                (edits, version, outcome.text, outcome.note, message)
            });
            Ok::<_, Refusal>((last, Some(outcome)))
        });
        let (last, outcome) = found.ok_or("Cannot read the file of this entry")??;
        // Remembered even when the command was refused, as in Emacs.
        let heading = last.heading;
        self.last = Some(last);
        let Some(outcome) = outcome else { return Ok(Value::Null) };
        let (edits, version, new_text, wanted, message) = outcome?;
        // The change wants a note: ask for its text, and make the change
        // once it comes, with the state it is about. An editor of this
        // project shows a note buffer, as Emacs does (`*Org Note*`).
        if let Some(wanted) = wanted {
            if self.client.date_prompt && self.client.show_document {
                return self.open_note(context, id, wanted);
            }
            return Ok(json!({ "input": {
                "kind": "note",
                "prompt": format!("Note for {}", wanted.purpose),
                "value": "",
                "context": { "state": wanted.state.unwrap_or_default() },
            } }));
        }
        self.message = message;
        let (edits, new_text) = match heading {
            Some(heading) => self.clock_out_if_done(&file, heading, edits, new_text),
            None => (edits, new_text),
        };
        if edits.is_empty() {
            return Ok(Value::Null);
        }
        // A change to priority or date can move the entry to another line
        // of the agenda. The cursor goes with it, so that the next press
        // of the key reaches the same entry.
        self.follow = match (&line, heading) {
            (Some((number, line)), Some(heading))
                if !matches!(id, commands::TODO | commands::TODO_NEXT | commands::TODO_PREVIOUS) =>
            {
                self.followed(&file, &new_text, heading, (Position::new(*number, position.character), line))
            }
            _ => None,
        };
        let edit = self.workspace_edit(&file, version, edits);
        self.edited.insert(key);
        // No label: some editors print it every time an edit is applied.
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
        Ok(Value::Null)
    }

    /// `org-set-tags-command` (`C-c C-q`; in the agenda for the line's
    /// entry). With fast tag selection it asks for one key at a time
    /// (input kind `key`), each turning a tag on or off, until RET; the
    /// tags so far travel in the input's `context` (`answers`: the mode,
    /// whether exclusive groups apply and whether the next change ends
    /// it, then the tags). TAB asks for a tag by name. Without fast
    /// selection the tags are typed (input kind `text`).
    /// Whether `C-c C-c` at `context` is `org-property-action`: in the
    /// property drawer of an entry of an Org file, or answering its
    /// questions.
    /// Whether the cursor of `context` is on a line of an Org table.
    fn at_table(&self, context: &Context) -> bool {
        let uri = &context.text_document.uri;
        let Some(document) = self.document(uri).filter(|_| !self.is_view(uri)) else { return false };
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        at_org_table(&document.text, &document.parse.syntax(), offset)
    }

    fn at_property_drawer(&self, context: &Context) -> bool {
        if context.step.is_some() {
            return true;
        }
        let uri = &context.text_document.uri;
        let Some(document) = self.document(uri).filter(|_| !self.is_view(uri)) else { return false };
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        org_core::properties::in_drawer(&document.text, offset)
    }

    /// The file and the byte offset a command at `context` acts on: the
    /// cursor's, or on an agenda line its entry's headline.
    fn command_place(&mut self, context: &Context) -> std::result::Result<(Uri, usize), Refusal> {
        let uri = &context.text_document.uri;
        if self.is_view(uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            let offset = self
                .with_source(&reference.uri, |_, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| outline.entries[entry].offset)
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            return Ok((reference.uri, offset));
        }
        let document = self.document(uri).ok_or("This file is not open")?;
        Ok((uri.clone(), document.index.offset(&document.text, context.position, self.client.encoding)))
    }

    /// The property commands: `org-set-property` (`C-c C-x p`),
    /// `org-delete-property`, `org-delete-property-globally`,
    /// `org-set-effort` (`C-c C-x e`), `org-inc-effort`,
    /// `org-toggle-ordered-property`, and `org-property-action` (`C-c C-c`
    /// in a property drawer), which asks which of them first. Each
    /// question comes back with `step` saying what `input` answers.
    fn property_command(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::properties::{self, Question};
        let (file, offset) = self.command_place(context)?;
        let settings = self.edit_settings.clone();
        let category = uri::file_stem(&file);
        let last = self.last_property.clone();
        // The questions are those of the file as it is now.
        let read = |server: &mut Self,
                    ask: &dyn Fn(
            &str,
            &org_syntax::Parse,
            &properties::Context,
        ) -> std::result::Result<Question, String>| {
            server
                .with_source(&file, |text, _, parse, _, _| {
                    let property_context = org_core::edit::property_context(parse, &settings, &category);
                    ask(text, parse, &property_context)
                })
                .ok_or_else(|| Refusal::Failed("Cannot read the file of this entry".to_string()))?
                .map_err(Refusal::Failed)
        };
        // A question, as an input: one of a list when only those will do,
        // or else text with the list to complete from.
        let input = |question: &Question, step: &str, answers: Option<&str>| {
            let prompt = question.prompt.trim_end_matches([':', ' ']);
            let mut context = json!({ "step": step });
            if let Some(answers) = answers {
                context["answers"] = json!(answers);
            }
            if question.require_match {
                let choices: Vec<Value> = question
                    .collection
                    .iter()
                    .map(|value| json!({ "key": value, "label": if value.is_empty() { "(none)" } else { value } }))
                    .collect();
                json!({ "input": { "kind": "choice", "prompt": prompt, "value": "", "choices": choices, "context": context } })
            } else {
                json!({ "input": {
                    "kind": "text", "prompt": prompt, "value": "",
                    "completions": question.collection, "context": context,
                } })
            }
        };
        let edit =
            |server: &mut Self, command: EditCommand| server.edit_with(context, id, Some(command), &Extra::default());
        let answer = context.input.clone();
        let name_question = |server: &mut Self| {
            read(server, &|text, _, property_context| {
                Ok(properties::name_question(text, offset, last.as_deref(), property_context))
            })
        };
        // What the step answers, or the command's first question.
        let step = match (context.step.as_deref(), &answer) {
            (Some(step), Some(_)) => step.to_string(),
            (Some(_), None) => return Ok(Value::Null),
            (None, _) => match id {
                commands::SET_PROPERTY => "set".to_string(),
                commands::DELETE_PROPERTY => "delete".to_string(),
                commands::DELETE_PROPERTY_GLOBALLY => "delete-globally".to_string(),
                commands::SET_EFFORT => "effort".to_string(),
                commands::INCREASE_EFFORT => return edit(self, EditCommand::Effort(None)),
                commands::TOGGLE_ORDERED => return edit(self, EditCommand::Ordered),
                _ => {
                    let choices = json!([
                        { "key": "s", "label": "set" },
                        { "key": "d", "label": "delete" },
                        { "key": "D", "label": "delete globally" },
                        { "key": "c", "label": "compute" },
                    ]);
                    return Ok(json!({ "input": {
                        "kind": "key", "prompt": "Property Action", "value": "", "choices": choices,
                        "context": { "step": "action" },
                    } }));
                }
            },
        };
        let answer = answer.unwrap_or_default();
        match step.as_str() {
            // `org-property-action`: the key picked.
            "action" => match answer.as_str() {
                "s" => Ok(input(&name_question(self)?, "name", None)),
                "d" => self.property_step_delete(context, &file, offset, &settings, &category),
                "D" => {
                    let question = read(self, &|text, _, property_context| {
                        Ok(properties::delete_globally_question(text, property_context))
                    })?;
                    Ok(input(&question, "delete-globally-name", None))
                }
                "c" => {
                    Err(Refusal::Failed("Computing a property needs column view, which org-lsp has not".to_string()))
                }
                key => Err(Refusal::Failed(format!("No such property action {key}"))),
            },
            "set" => Ok(input(&name_question(self)?, "name", None)),
            "name" => {
                let question = name_question(self)?;
                let name = question.answer(&answer);
                if !properties::valid_name(&name) {
                    return Err(Refusal::Failed(format!("Invalid property name: \"{name}\"")));
                }
                let asked = name.clone();
                let question = read(self, &|text, _, property_context| {
                    properties::value_question(text, offset, &asked, property_context)
                })?;
                Ok(input(&question, "value", Some(&name)))
            }
            "value" => {
                let name = context.answers.clone().unwrap_or_default();
                let asked = name.clone();
                let question = read(self, &|text, _, property_context| {
                    properties::value_question(text, offset, &asked, property_context)
                })?;
                let value = question.answer(&answer);
                self.last_property = Some(name.clone());
                edit(self, EditCommand::Property { name, value })
            }
            "delete" => self.property_step_delete(context, &file, offset, &settings, &category),
            "delete-name" => edit(self, EditCommand::DeleteProperty(answer)),
            "delete-globally" => {
                let question = read(self, &|text, _, property_context| {
                    Ok(properties::delete_globally_question(text, property_context))
                })?;
                Ok(input(&question, "delete-globally-name", None))
            }
            "delete-globally-name" => edit(self, EditCommand::DeletePropertyGlobally(answer)),
            "effort" => {
                let question = read(self, &|text, _, property_context| {
                    properties::effort_question(text, offset, property_context)
                })?;
                Ok(input(&question, "effort-value", None))
            }
            "effort-value" => edit(self, EditCommand::Effort(Some(answer))),
            other => Err(Refusal::Failed(format!("Unknown step {other}"))),
        }
    }

    /// `org-delete-property`: which property, unless the entry offers
    /// only one.
    fn property_step_delete(
        &mut self,
        context: &Context,
        file: &Uri,
        offset: usize,
        _settings: &EditSettings,
        _category: &str,
    ) -> std::result::Result<Value, Refusal> {
        let question = self
            .with_source(file, |text, _, _, _, _| org_core::properties::delete_question(text, offset))
            .ok_or("Cannot read the file of this entry")?;
        match question {
            Ok(question) => {
                let choices: Vec<Value> =
                    question.collection.iter().map(|name| json!({ "key": name, "label": name })).collect();
                Ok(json!({ "input": {
                    "kind": "choice", "prompt": "Property", "value": "", "choices": choices,
                    "context": { "step": "delete-name" },
                } }))
            }
            Err(name) => self.edit_with(
                context,
                commands::DELETE_PROPERTY,
                Some(EditCommand::DeleteProperty(name)),
                &Extra::default(),
            ),
        }
    }

    fn set_tags(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::tags::{self, Selection, State, Step};
        let uri = &context.text_document.uri;
        let (file, offset) = if self.is_view(uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            let offset = self
                .with_source(&reference.uri, |_, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| outline.entries[entry].offset)
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            (reference.uri, offset)
        } else {
            let document = self.document(uri).ok_or("This file is not open")?;
            (uri.clone(), document.index.offset(&document.text, context.position, self.client.encoding))
        };
        let others = if self.all_agenda_tags { self.agenda_tags() } else { Vec::new() };
        let settings = self.edit_settings.tags.clone();
        let found = self.with_source(&file, |text, _, _, outline, _| {
            let heading = tags::heading_at(text, offset)?;
            let line = text[heading..].lines().next().unwrap_or("");
            let inherited: Vec<String> = outline
                .entry_at(heading)
                .map(|entry| outline.entries[entry].tags.iter().filter(|tag| tag.inherited).map(|tag| tag.name.clone()))
                .into_iter()
                .flatten()
                .collect();
            Ok::<_, Refusal>((tags::table(text, &settings, &others), tags::local_tags(line), inherited))
        });
        let (table, own, inherited) = found.ok_or("Cannot read the file of this entry")??;
        let set = |server: &mut Self, chosen: Vec<String>| {
            server.edit_with(context, commands::SET_TAGS, Some(EditCommand::Tags(chosen)), &Extra::default())
        };
        if !tags::uses_fast_selection(&table, &settings) {
            return match context.input.as_deref() {
                Some(answer) => set(self, tags::parse_tags(answer)),
                None => Ok(json!({ "input": { "kind": "text", "prompt": "Tags", "value": tags::tag_string(&own) } })),
            };
        }
        let selection = Selection::new(&table);
        // Where the selection stands: `k` or `t` (a tag being typed), the
        // groups and the single-key flags, then the tags.
        let (mode, mut state) = match context.answers.as_deref().and_then(|answers| answers.split_once('\n')) {
            Some((flags, current)) => {
                let flag = |index: usize| flags.as_bytes().get(index) == Some(&b'1');
                let current = current.split(':').filter(|tag| !tag.is_empty()).map(str::to_string).collect();
                (flags.as_bytes().first().copied(), State { current, groups: flag(1), exit_after_next: flag(2) })
            }
            None => (None, State { current: own, groups: true, exit_after_next: settings.single_key }),
        };
        let step = match (mode, context.input.as_deref()) {
            (_, None) => Step::Again,
            (Some(b't'), Some(typed)) => tags::type_tag(&mut state, typed, &selection),
            (_, Some(key)) => match key.chars().next() {
                Some(key) => tags::press(&mut state, key, &selection),
                None => Step::Quit,
            },
        };
        let answers = |mode: char, state: &State| {
            let flag = |on: bool| if on { '1' } else { '0' };
            format!("{mode}{}{}\n{}", flag(state.groups), flag(state.exit_after_next), state.current.join(":"))
        };
        match step {
            Step::Accept => set(self, tags::parse_tags(&state.current.join(":"))),
            Step::Quit => Ok(Value::Null),
            Step::Type => Ok(json!({ "input": {
                "kind": "text", "prompt": "Tag", "value": "",
                "context": { "answers": answers('t', &state) },
            } })),
            Step::Again => {
                let mark = |tag: &str| {
                    if state.current.iter().any(|current| current == tag) {
                        format!("{tag} (on)")
                    } else if inherited.iter().any(|inherited| inherited == tag) {
                        format!("{tag} (inherited)")
                    } else {
                        tag.to_string()
                    }
                };
                let mut choices: Vec<Value> = selection
                    .keys
                    .iter()
                    .map(|(tag, key)| json!({ "key": key.to_string(), "label": mark(tag) }))
                    .collect();
                choices.push(json!({ "key": "\r", "label": "accept" }));
                choices.push(json!({ "key": " ", "label": "clear" }));
                choices.push(json!({ "key": "\t", "label": "type a tag" }));
                if !selection.groups.is_empty() {
                    let label = if state.groups { "exclusive groups off" } else { "exclusive groups on" };
                    choices.push(json!({ "key": "!", "label": label }));
                }
                let mut prompt = format!("Tags {}", tags::tag_string(&state.current));
                if !inherited.is_empty() {
                    prompt.push_str(&format!(" (inherited {})", tags::tag_string(&inherited)));
                }
                Ok(json!({ "input": {
                    "kind": "key", "prompt": prompt.trim_end(), "value": "", "choices": choices,
                    "context": { "answers": answers('k', &state) },
                } }))
            }
        }
    }

    /// An editing command, which may ask for something first. The answer
    /// to the note a TODO state change asked for comes with the state
    /// (`state` in the context); `org.todo` asks for the key of the new
    /// state when the keywords have keys (`org-todo` with fast selection),
    /// and cycles when they have none.
    fn run_edit(&mut self, id: &str, context: Option<Context>) -> std::result::Result<Value, Refusal> {
        let context = context.ok_or("This command needs the cursor position")?;
        // `org-add-note`: asks for the note, then writes the one given.
        if id == commands::ADD_NOTE {
            let extra = Extra { note: context.input.clone(), ..Extra::default() };
            return self.edit_with(&context, id, Some(EditCommand::Note), &extra);
        }
        if let Some(state) = &context.state {
            let target = TodoTarget::State((!state.is_empty()).then(|| state.clone()));
            let extra = Extra { note: Some(context.input.clone().unwrap_or_default()), ..Extra::default() };
            return self.edit_with(&context, id, Some(EditCommand::Todo(target)), &extra);
        }
        if matches!(id, commands::SHIFT_LEFT | commands::SHIFT_RIGHT | commands::SHIFT_UP | commands::SHIFT_DOWN) {
            if let Some(result) = self.shift_item(id, &context) {
                return result;
            }
            // `org-clocktable-try-shift`: the block of a clock table.
            if self.at_clocktable(&context, org_core::clocktable::at_clocktable_begin) {
                return self.clocktable(id, &context);
            }
        }
        // `C-c C-c` on the `#+BEGIN:` or `#+END:` line of a dynamic block
        // fills it in again.
        if id == commands::CHECKBOX_TOGGLE && self.at_clocktable(&context, org_core::clocktable::at_block_line) {
            return self.clocktable(id, &context);
        }
        if id == commands::SET_TAGS {
            return self.set_tags(&context);
        }
        // `C-c C-c` in a table aligns it (`org-table-align`).
        if id == commands::CHECKBOX_TOGGLE && self.at_table(&context) {
            return self.table(org_core::table::Command::Align, &context);
        }
        if PROPERTY_COMMANDS.contains(&id) || (id == commands::CHECKBOX_TOGGLE && self.at_property_drawer(&context)) {
            return self.property_command(id, &context);
        }
        if id != commands::TODO {
            return self.edit_with(&context, id, None, &Extra::default());
        }
        let keys = self.fast_keys(&context)?;
        if keys.is_empty() {
            return self.edit_with(&context, id, Some(EditCommand::Todo(TodoTarget::Next)), &Extra::default());
        }
        let Some(answer) = &context.input else {
            let choices: Vec<Value> = keys
                .iter()
                .map(|(name, key)| json!({ "key": key.to_string(), "label": name }))
                .chain([json!({ "key": " ", "label": "clear" })])
                .collect();
            return Ok(json!({ "input": {
                "kind": "key",
                "prompt": "Select a TODO state",
                "value": "",
                "choices": choices,
            } }));
        };
        let extra = Extra { key: Some(answer.clone()), ..Extra::default() };
        self.edit_with(&context, id, Some(EditCommand::Todo(TodoTarget::Next)), &extra)
    }

    /// `org-return` at `context`. The edit is not sent to be applied: the
    /// result carries it, `{ "edits": [TextEdit], "cursor": Position }`,
    /// for the editor to apply as the key is typed.
    fn org_return(&mut self, context: &Context, items: bool) -> std::result::Result<Value, Refusal> {
        let encoding = self.client.encoding;
        let uri = &context.text_document.uri;
        let text = self.document(uri).ok_or("This file is not open")?.text.clone();
        let settings =
            org_core::newline::Settings { indent: self.indent_in(&text), tags_column: self.refile.2.tags_column };
        let document = self.document(uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, encoding);
        // `return_uses_meta_return`, and `org.return.item` always: on the first
        // line of a list item, the next item, numbered as M-RET numbers it.
        if (items || self.return_uses_meta_return) && on_item_line(&document.parse.syntax(), &document.text, offset) {
            return self.meta_return(commands::META_RETURN, context);
        }
        let outcome = org_core::newline::org_return(&document.text, offset, document.parse.todo_keywords(), &settings)?;
        self.carried(context, outcome)
    }

    /// `org-babel-tangle`: write the code of the file's blocks to their
    /// files, and say how many blocks went.
    fn tangle(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let document = self.document(uri).ok_or("This file is not open")?;
        let path = uri::to_path(uri).ok_or("Only a file on disk can be tangled")?;
        let settings = org_core::tangle::Settings { preserve_indentation: self.refile.2.src_preserve_indentation };
        let outcome = org_core::tangle::tangle(&document.text, &document.parse, &path, &settings)?;
        for file in &outcome.files {
            write_tangled(file).map_err(|error| format!("{}: {error}", file.path.display()))?;
        }
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        self.message = Some(outcome.message(&name));
        Ok(Value::Null)
    }

    /// M-RET (`org-meta-return`), M-S-RET (`org-insert-todo-heading`),
    /// C-RET and C-S-RET (their `respect-content` forms) at `context`,
    /// carried in the result as `org.return` is.
    fn meta_return(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::meta_return;
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, encoding);
        let keywords = document.parse.todo_keywords();
        let settings = meta_return::Settings { tags_column: self.refile.2.tags_column, below: context.below };
        let text = &document.text;
        let outcome = match id {
            commands::META_RETURN => meta_return::meta_return(text, offset, keywords, &settings)?,
            commands::INSERT_TODO_HEADING => {
                meta_return::insert_todo_heading(text, offset, keywords, &settings, false)?
            }
            commands::INSERT_HEADING_AFTER => meta_return::insert_heading_after(text, offset, keywords, &settings),
            _ => meta_return::insert_todo_heading(text, offset, keywords, &settings, true)?,
        };
        self.carried(context, outcome)
    }

    /// Org's meta keys for structure editing (`org-metaleft`,
    /// `org-metaright`, `org-shiftmetaleft`, `org-shiftmetaright`,
    /// `org-metaup`, `org-metadown`): on a headline, promoting, demoting
    /// and moving subtrees; `M-<right>` on a drawer or block indents it;
    /// on a list item, outdenting, indenting and moving it, alone or with
    /// its children (`org-outdent-item`, `org-indent-item-tree`,
    /// `org-move-item-up`, …). Carried in the result as `org.return` is.
    fn structure(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::structure;
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        // In a table, the keys move, insert and delete its rows and columns.
        if at_org_table(&text, &document.parse.syntax(), offset) {
            return self.table(table_command(id).ok_or("Unknown command")?, context);
        }
        let keywords = document.parse.todo_keywords().clone();
        if id == commands::META_RIGHT {
            let settings = self.indent_in(&text);
            if let Some((outcome, message)) =
                structure::indent_element(&text, offset, &document.parse.syntax(), &settings)?
            {
                self.message = Some(message.to_string());
                return self.carried(context, outcome);
            }
        }
        let tags_column = self.refile.2.tags_column;
        if !structure::at_heading(&text, offset) && org_core::list::at_item(&text, offset) {
            let edited = item_structure(id, &text, offset, &keywords, tags_column)?;
            let outcome = org_core::newline::Outcome { text: edited.text, cursor: edited.cursor };
            return self.carried(context, outcome);
        }
        if !structure::at_heading(&text, offset) {
            return Err(Refusal::NotHere("Not at a headline".to_string()));
        }
        let settings = structure::Settings { tags_column, indent: self.indent_in(&text) };
        let count = context.count.filter(|count| *count > 0).unwrap_or(1) as isize;
        let outcome = match id {
            commands::META_LEFT => structure::change_levels(&text, offset, -1, false, &keywords, &settings)?,
            commands::META_RIGHT => structure::change_levels(&text, offset, 1, false, &keywords, &settings)?,
            commands::SHIFT_META_LEFT => structure::change_levels(&text, offset, -1, true, &keywords, &settings)?,
            commands::SHIFT_META_RIGHT => structure::change_levels(&text, offset, 1, true, &keywords, &settings)?,
            commands::META_UP => structure::move_subtree(&text, offset, -count)?,
            _ => structure::move_subtree(&text, offset, count)?,
        };
        self.carried(context, outcome)
    }

    /// A command of `org-table.el` on the table at the cursor, carried in
    /// the result as `org.return` is; the row `org-table-kill-row` takes
    /// is in `copy`, for the editor's register or clipboard.
    fn table(&mut self, command: org_core::table::Command, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::table;
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        if !at_org_table(&text, &document.parse.syntax(), offset) {
            return Err(Refusal::NotHere("Not at a table".to_string()));
        }
        let done = match table::run(&text, offset, command) {
            Ok(done) => done,
            // `C-c C-c` at the start of the table.
            Err(error) if error == table::RECALCULATE => {
                return self.edit_with(context, commands::TABLE_RECALCULATE, None, &Extra::default());
            }
            Err(error) => return Err(Refusal::Failed(error)),
        };
        if let Some(killed) = &done.killed {
            self.kill = Some(killed.clone());
        }
        let outcome = org_core::newline::Outcome { text: done.text, cursor: done.cursor };
        let mut result = self.carried(context, outcome)?;
        if let (Some(killed), Some(fields)) = (done.killed, result.as_object_mut()) {
            fields.insert("copy".to_string(), json!(killed));
        }
        Ok(result)
    }

    /// Org's shifted cursor keys on a list item in an Org file, carried in
    /// the result as `org.return` is: the bullets cycled
    /// (`org-cycle-list-bullet`), or the cursor to the next or previous
    /// item (`org-next-item`, `org-previous-item`). None elsewhere.
    fn shift_item(&mut self, id: &str, context: &Context) -> Option<std::result::Result<Value, Refusal>> {
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri)?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        if !org_core::edit::shifts_item(&text, offset) {
            return None;
        }
        let outcome = match id {
            commands::SHIFT_LEFT | commands::SHIFT_RIGHT => {
                org_core::list::cycle_bullet(&text, offset, id == commands::SHIFT_LEFT)
                    .map(|edited| org_core::newline::Outcome { text: edited.text, cursor: edited.cursor })
            }
            _ => org_core::list::next_item(&text, offset, id == commands::SHIFT_UP)
                .map(|cursor| org_core::newline::Outcome { text: text.clone(), cursor }),
        };
        Some(outcome.and_then(|outcome| self.carried(context, outcome)))
    }

    /// `org-sparse-tree` (`C-c /`): asks what to show, as a key, and then
    /// what that needs (a regexp, keywords, a match, a property and its
    /// value, dates); `answers` keeps the type and the answers so far, a
    /// line each. The result has `sparse`: `{ "lines", "highlights" }`,
    /// the lines to show with everything else folded, as `org-occur`
    /// shows its matches, and the ranges of the matches to highlight
    /// until the text changes.
    fn sparse_tree(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::sparse;
        let encoding = self.client.encoding;
        let uri = context.text_document.uri.clone();
        let document = self.document(&uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let parse = document.parse.clone();
        let keywords = parse.todo_keywords().clone();
        let mut answers: Vec<String> = context
            .answers
            .as_deref()
            .map(|answers| answers.split('\n').map(str::to_string).collect())
            .unwrap_or_default();
        let Some(input) = context.input.clone() else {
            let choices = json!([
                { "key": "r", "label": "regexp" },
                { "key": "t", "label": "todo" },
                { "key": "T", "label": "todo keyword" },
                { "key": "m", "label": "match" },
                { "key": "p", "label": "property" },
                { "key": "d", "label": "deadlines" },
                { "key": "b", "label": "before date" },
                { "key": "a", "label": "after date" },
                { "key": "D", "label": "dates range" },
            ]);
            return Ok(json!({ "input": { "kind": "key", "prompt": "Sparse tree", "value": "", "choices": choices } }));
        };
        answers.push(input);
        let kind = answers[0].clone();
        let args = &answers[1..];
        let ask = |kind: &str, prompt: &str, choices: Option<Vec<String>>, answers: &[String]| {
            let mut input =
                json!({ "kind": kind, "prompt": prompt, "value": "", "context": { "answers": answers.join("\n") } });
            if let Some(choices) = choices {
                input["choices"] = choices.iter().map(|choice| json!({ "key": choice, "label": choice })).collect();
            }
            Ok(json!({ "input": input }))
        };
        let read_date = |answer: &str| -> std::result::Result<(DateTime, String), Refusal> {
            let read = org_core::read_date::read_date(answer, None, self.date_time(), &self.read_date)?;
            let shown = match read.time {
                Some((hour, minute)) => format!("{} {hour:02}:{minute:02}", read.date),
                None => read.date.to_string(),
            };
            let (hour, minute) = read.time.unwrap_or((0, 0));
            Ok((DateTime::new(read.date, hour, minute), shown))
        };
        let (found, message) = match (kind.as_str(), args.len()) {
            ("r" | "R" | "/", 0) => return ask("text", "Regexp", None, &answers),
            ("r" | "R" | "/", _) => {
                if args[0].is_empty() {
                    return Err(Refusal::Failed("Regexp cannot be empty".to_string()));
                }
                let regex = org_core::agenda::matcher::emacs_buffer_regex(&args[0])
                    .ok_or_else(|| Refusal::Failed(format!("Invalid regexp: {}", args[0])))?;
                let found = sparse::occur(&text, &regex, |_| true);
                let message = format!("{} match(es) for regexp {}", found.count, args[0]);
                (found, Some(message))
            }
            ("t", _) => {
                let found = sparse::todo_tree(&text, &keywords, None);
                let message = format!("{} TODO entries found", found.count);
                (found, Some(message))
            }
            ("T", 0) => {
                let names: Vec<String> = keywords.names().map(str::to_string).collect();
                return ask("text", &format!("Keyword (or KWD1|KWD2|...) [{}]", names.join(" ")), None, &answers);
            }
            ("T", _) => {
                let wanted: Vec<String> = args[0].split('|').map(str::to_string).collect();
                let found = sparse::todo_tree(&text, &keywords, Some(&wanted));
                let message = format!("{} TODO entries found", found.count);
                (found, Some(message))
            }
            ("m", 0) => return ask("text", "Match", None, &answers),
            ("p" | "P", 0) => {
                let mut names: Vec<String> = Vec::new();
                for name in property_names(&text) {
                    if !names.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
                        names.push(name);
                    }
                }
                names.sort();
                return ask("choice", "Property", Some(names), &answers);
            }
            ("p" | "P", 1) => {
                let mut values: Vec<String> = property_values(&text, &args[0]);
                values.sort();
                values.dedup();
                return ask("choice", "Value", Some(values), &answers);
            }
            ("m" | "p" | "P", _) => {
                let matching = if kind == "m" {
                    args[0].clone()
                } else {
                    // A value in braces is a regexp; anything else a string.
                    let value = &args[1];
                    let value = if value.starts_with('{') && value.ends_with('}') {
                        value.clone()
                    } else {
                        format!("\"{value}\"")
                    };
                    format!("{}={value}", args[0].replace('-', "\\-"))
                };
                let outline = org_core::outline::Outline::new(&parse, &text, uri.as_str());
                let now = self.now();
                let clock = org_core::agenda::matcher::Clock { today: now.today, minute: now.minute };
                (sparse::tags_match(&text, &outline, &org_core::agenda::Matcher::new(&matching), clock), None)
            }
            ("d", _) => {
                let days = self.agenda.deadline_warning_days.abs();
                let found = sparse::deadlines(&text, &keywords, self.now().today, days);
                let message = format!("{} deadlines past-due or due within {days} days", found.count);
                (found, Some(message))
            }
            ("b" | "a", 0) => return ask("date", "Date", None, &answers),
            ("b", _) => {
                let (date, shown) = read_date(&args[0])?;
                let found = sparse::dates(&text, None, Some(date));
                let message = format!("{} entries before {shown}", found.count);
                (found, Some(message))
            }
            ("a", _) => {
                let (date, shown) = read_date(&args[0])?;
                let found = sparse::dates(&text, Some(date), None);
                let message = format!("{} entries after {shown}", found.count);
                (found, Some(message))
            }
            ("D", 0) => return ask("date", "Range starts", None, &answers),
            ("D", 1) => return ask("date", "Range end", None, &answers),
            ("D", _) => {
                let (start, start_shown) = read_date(&args[0])?;
                let (end, end_shown) = read_date(&args[1])?;
                let found = sparse::dates(&text, Some(start), Some(end));
                let message = format!("{} entries between {start_shown} and {end_shown}", found.count);
                (found, Some(message))
            }
            (other, _) => return Err(Refusal::Failed(format!("No such sparse tree command \"{other}\""))),
        };
        let document = self.document(&uri).ok_or("This file is not open")?;
        let index = &document.index;
        let lines: Vec<u32> = found.lines.iter().map(|at| index.position(&text, *at, encoding).line).collect();
        let highlights: Vec<Range> = found
            .highlights
            .iter()
            .map(|(start, end)| {
                Range::new(index.position(&text, *start, encoding), index.position(&text, *end, encoding))
            })
            .collect();
        self.message = message;
        Ok(json!({ "sparse": { "lines": lines, "highlights": highlights } }))
    }

    /// `org-sort` (`C-c ^`) on entries (`org-sort-entries`): asks for the
    /// sorting type, as a key, and for `r` and `R` the property, among
    /// those of the file; the cursor goes to the start of what was sorted.
    /// `org-table-sort-lines`: the column when the cursor is before the
    /// first one (step `column`), then the sorting type.
    fn sort_table(&mut self, context: &Context, text: &str, offset: usize) -> std::result::Result<Value, Refusal> {
        use org_core::table;
        let before_first = text[text[..offset].rfind('\n').map_or(0, |at| at + 1)..offset].trim().is_empty();
        let column: Option<usize> = match (context.step.as_deref(), context.answers.as_deref()) {
            (Some("column"), _) => {
                let Some(answer) = context.input.as_deref() else { return Ok(Value::Null) };
                Some(answer.trim().parse().map_err(|_| Refusal::Failed(format!("Not a number: {answer}")))?)
            }
            (_, Some(column)) => column.parse().ok(),
            _ => None,
        };
        if before_first && column.is_none() && context.input.is_none() {
            return Ok(json!({ "input": {
                "kind": "text", "prompt": table::ASK_COLUMN, "value": "", "context": { "step": "column" },
            } }));
        }
        let answer = match context.step.as_deref() {
            Some("column") => None,
            _ => context.input.as_deref(),
        };
        let Some(answer) = answer else {
            let types = table::SORT_TYPES;
            let mut choices: Vec<Value> =
                types.iter().map(|(key, label)| json!({ "key": key.to_string(), "label": label })).collect();
            choices.extend(types.iter().map(|(key, label)| {
                json!({ "key": key.to_ascii_uppercase().to_string(), "label": format!("{label}, reversed") })
            }));
            let mut input = json!({ "kind": "key", "prompt": "Sort Table", "value": "", "choices": choices });
            if let Some(column) = column {
                input["context"] = json!({ "step": "type", "answers": column.to_string() });
            }
            return Ok(json!({ "input": input }));
        };
        let kind = answer.chars().next().ok_or("Abort")?;
        self.table(table::Command::Sort { kind, column }, context)
    }

    fn sort(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::sort;
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        let keywords = document.parse.todo_keywords().clone();
        let priorities = edit::document_priorities(&document.parse).unwrap_or(self.edit_settings.priorities);
        // Org sorts the lines of a table and the items of a list there.
        if at_org_table(&text, &document.parse.syntax(), offset) {
            return self.sort_table(context, &text, offset);
        }
        // `org-sort-list` on an item.
        if org_core::list::at_item(&text, offset) {
            let Some(answer) = context.input.as_deref() else {
                let types = sort::LIST_TYPES;
                let mut choices: Vec<Value> =
                    types.iter().map(|(key, label)| json!({ "key": key.to_string(), "label": label })).collect();
                choices.extend(types.iter().map(|(key, label)| {
                    json!({ "key": key.to_ascii_uppercase().to_string(), "label": format!("{label}, reversed") })
                }));
                let input = json!({ "kind": "key", "prompt": "Sort plain list", "value": "", "choices": choices });
                return Ok(json!({ "input": input }));
            };
            let kind = answer.chars().next().ok_or("Abort")?;
            let outcome = sort::sort_list(&text, offset, kind, self.date_time())?;
            self.message = Some("Sorting items...done".to_string());
            return self.carried(context, outcome);
        }
        let Some(answer) = context.input.as_deref() else {
            let mut choices: Vec<Value> =
                sort::TYPES.iter().map(|(key, label)| json!({ "key": key.to_string(), "label": label })).collect();
            choices.extend(sort::TYPES.iter().map(|(key, label)| {
                json!({ "key": key.to_ascii_uppercase().to_string(), "label": format!("{label}, reversed") })
            }));
            let prompt = format!("Sort {}", sort::what(&text, offset));
            return Ok(json!({ "input": { "kind": "key", "prompt": prompt, "value": "", "choices": choices } }));
        };
        let (kind, property) = match context.answers.as_deref() {
            Some(kind) => (kind.chars().next().unwrap_or('a'), Some(answer)),
            None => (answer.chars().next().ok_or("Abort")?, None),
        };
        if kind.eq_ignore_ascii_case(&'r') && property.is_none() {
            let mut names: Vec<String> = Vec::new();
            for found in property_names(&text) {
                if !names.iter().any(|name| name.eq_ignore_ascii_case(&found)) {
                    names.push(found);
                }
            }
            names.sort();
            if names.is_empty() {
                return Err(Refusal::Failed("No properties to sort by".to_string()));
            }
            let choices: Vec<Value> = names.iter().map(|name| json!({ "key": name, "label": name })).collect();
            let input = json!({
                "kind": "choice", "prompt": "Property", "value": "", "choices": choices,
                "context": { "answers": kind.to_string() },
            });
            return Ok(json!({ "input": input }));
        }
        let settings = sort::Settings { keywords: &keywords, priorities, now: self.date_time() };
        let outcome = sort::sort_entries(&text, offset, kind, property, &settings)?;
        self.message = Some("Sorting entries...done".to_string());
        self.carried(context, outcome)
    }

    /// `org-cut-subtree`, `org-copy-subtree` and `org-paste-subtree`. What
    /// is cut or copied goes back in `copy`, for the editor's clipboard,
    /// and stays with the server; pasting takes the editor's clipboard, or
    /// else that.
    fn subtree_clipboard(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::structure;
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        if id == commands::PASTE_SUBTREE {
            let tree = context.clipboard.clone().or_else(|| self.kill.clone()).unwrap_or_default();
            let settings =
                structure::Settings { tags_column: self.refile.2.tags_column, indent: self.indent_in(&text) };
            let level = context.count.filter(|count| *count > 0).map(|count| count as usize);
            let (outcome, message) = structure::paste_subtree(&text, offset, &tree, level, &settings)?;
            self.message = Some(message);
            return self.carried(context, outcome);
        }
        let count = context.count.filter(|count| *count > 0).unwrap_or(1) as usize;
        let cut = id == commands::CUT_SUBTREE;
        let (outcome, copied, message) = structure::copy_subtree(&text, offset, count, cut)?;
        self.message = Some(message);
        self.kill = Some(copied.clone());
        let mut result = if cut { self.carried(context, outcome)? } else { json!({}) };
        if let Value::Object(fields) = &mut result {
            fields.insert("copy".to_string(), json!(copied));
        }
        Ok(result)
    }

    /// The result of a command that carries its edit and the cursor after
    /// it, `{ "edits": [TextEdit], "cursor": Position }`, for the editor to
    /// apply at once. With `apply` in the context the server applies the
    /// edit itself and moves the cursor there.
    fn carried(
        &mut self,
        context: &Context,
        outcome: org_core::newline::Outcome,
    ) -> std::result::Result<Value, Refusal> {
        let encoding = self.client.encoding;
        let uri = &context.text_document.uri;
        let document = self.document(uri).ok_or("This file is not open")?;
        let old = &document.text;
        let new = &outcome.text;
        // The edit: what lies between what both have at the start and at
        // the end.
        let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let limit = (old.len() - prefix).min(new.len() - prefix);
        let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(limit).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
            suffix -= 1;
        }
        let (mut start, mut end) = (prefix, old.len() - suffix);
        let mut text = new[prefix..new.len() - suffix].to_string();
        // Lines added after the last one go after its end, not after the
        // final newline: on a line past the buffer's last, Neovim 0.11.0
        // leaves an empty line behind them.
        if start == old.len() && suffix == 0 && old.ends_with('\n') && text.ends_with('\n') {
            (start, end) = (start - 1, start - 1);
            text.pop();
            text.insert(0, '\n');
        }
        let edit = indent_edit(old, &document.index, encoding, org_core::indent::Edit { start, end, text });
        let cursor = LineIndex::new(new).position(new, outcome.cursor, encoding);
        if context.apply {
            let version = document.version;
            let edit = self.workspace_edit(uri, Some(version), vec![edit]);
            let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
            self.apply_edit(params, Pending::EditAndMove { uri: uri.clone(), position: cursor });
            return Ok(Value::Null);
        }
        Ok(json!({ "edits": [edit], "cursor": cursor }))
    }

    /// The keys of fast TODO selection in the file of `context`: an Org
    /// file, or the file of the entry of an agenda line.
    fn fast_keys(&mut self, context: &Context) -> std::result::Result<Vec<(String, char)>, Refusal> {
        let uri = &context.text_document.uri;
        let file = if self.is_view(uri) {
            self.entry_at(Some(context)).ok_or("No entry on this line")?.uri
        } else {
            uri.clone()
        };
        let keys = self.with_source(&file, |_, _, parse, _, _| {
            parse.todo_keywords().fast_keys().into_iter().map(|(name, key)| (name.to_string(), key)).collect()
        });
        Ok(keys.ok_or("Cannot read the file of this entry")?)
    }

    // ----- clocking

    /// The clock that runs in the files the server knows, the latest one
    /// if there are several.
    fn find_running(&mut self) -> Option<RunningClock> {
        let mut found: Vec<RunningClock> = Vec::new();
        for (key, document) in &self.documents {
            let Ok(uri) = key.parse::<Uri>() else { continue };
            for (_, start, head) in running_lines(&document.text) {
                found.push(RunningClock { uri: uri.clone(), start, head });
            }
        }
        for (uri, held) in self.agenda_files() {
            let outline = held.outline();
            for line in outline.clocks.iter().filter(|line| line.clock.is_running()) {
                let (Some(start), Some(entry)) = (line.clock.start, line.entry) else { continue };
                let head = outline.entries[entry].head.clone();
                if !found.iter().any(|known| known.uri == uri && known.start == start && known.head == head) {
                    found.push(RunningClock { uri: uri.clone(), start, head });
                }
            }
        }
        found.into_iter().max_by_key(|running| clock::minutes_of(running.start))
    }

    /// Follow a document that changed: its running clock may be gone, or
    /// a new one there runs now.
    fn sync_clock(&mut self, key: &str) {
        let Some(document) = self.documents.get(key) else { return };
        let found = running_lines(&document.text);
        let changed = match &self.running {
            Some(running) if running.uri.as_str() == key => {
                if found.iter().any(|(_, start, head)| *start == running.start && *head == running.head) {
                    self.clock_pending
                } else if self.clock_pending {
                    // The edit that writes the line has not come yet.
                    false
                } else {
                    self.running = None;
                    true
                }
            }
            None => match found.into_iter().max_by_key(|(_, start, _)| clock::minutes_of(*start)) {
                Some((_, start, head)) => {
                    let Ok(uri) = key.parse::<Uri>() else { return };
                    self.running = Some(RunningClock { uri, start, head });
                    true
                }
                None => false,
            },
            Some(_) => false,
        };
        if changed {
            self.publish_clock();
        }
    }

    /// The text of the running clock's file, its version, and where its
    /// `CLOCK:` line starts.
    fn clock_line(&mut self, running: &RunningClock) -> Option<(String, Option<i32>, usize)> {
        self.with_source(&running.uri, |text, _, _, _, version| {
            let line = running_lines(text)
                .into_iter()
                .find(|(_, start, head)| *start == running.start && *head == running.head)?
                .0;
            Some((text.to_string(), version, line))
        })
        .flatten()
    }

    /// Tell the client what the clock does (`org-lsp/clock`): nothing
    /// (`null`), or the entry, the minutes clocked on it, its effort and
    /// the text of Emacs's mode line (`[0:25/1:00] (Write the report)`).
    fn publish_clock(&mut self) {
        let params = match self.running.clone() {
            None => Value::Null,
            Some(running) => match self.clock_status(&running) {
                Some(status) => status,
                // Its line is not in the text yet: once the edit is.
                None => {
                    self.clock_pending = true;
                    return;
                }
            },
        };
        self.clock_pending = false;
        let notification = lsp_server::Notification::new("org-lsp/clock".to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    // ----- citations

    /// The bibliography of the document `uri` with text `text`, as
    /// `org-cite-basic--parse-bibliography` lists it: the files of its
    /// `#+BIBLIOGRAPHY:` keywords (relative to it) and of
    /// `org_cite_global_bibliography`, the last first. A file Org cannot
    /// read is left out (Emacs then fails for the whole bibliography).
    fn bibliography(&mut self, uri: &Uri, text: &str) -> Vec<Rc<BibFile>> {
        let names = org_core::cite::bibliography_files(
            &org_core::cite::bibliography_keywords(text, &self.todo),
            &self.cite_global,
        );
        let mut files = Vec::new();
        for name in names {
            let Some(path) = self.link_path(uri, &name) else { continue };
            // `file-truename`
            let Ok(path) = std::fs::canonicalize(&path) else { continue };
            let Ok(metadata) = std::fs::metadata(&path) else { continue };
            let (modified, len) = (metadata.modified().ok(), metadata.len());
            let cached = self
                .bibliographies
                .get(&path)
                .filter(|(when, size, _)| *when == modified && *size == len && modified.is_some())
                .map(|(_, _, file)| file.clone());
            let file = match cached {
                Some(file) => file,
                None => {
                    let Ok(bytes) = std::fs::read(&path) else { continue };
                    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                    let entries = org_core::cite::parse_file(&name, &text).unwrap_or_default();
                    let file = Rc::new(BibFile { path: path.clone(), text, entries });
                    self.bibliographies.insert(path, (modified, len, file.clone()));
                    file
                }
            };
            files.insert(0, file);
        }
        files
    }

    /// The entry of `key` in a bibliography, and the file it is in.
    fn cite_entry<'a>(files: &'a [Rc<BibFile>], key: &str) -> Option<(&'a BibFile, &'a org_core::cite::Entry)> {
        files.iter().find_map(|file| file.entries.iter().find(|entry| entry.key == key).map(|entry| (&**file, entry)))
    }

    /// `org-cite-insert` (`C-c C-x @`), with the prefix argument `arg`
    /// (`C-u`), and the `basic` processor's questions: keys one at a time
    /// (step `keys`, those picked so far in `answers` separated by `;`)
    /// until `(done)`, then with `arg` a style (step `new-style`); one key
    /// (step `key`) to add to a citation or to replace the one at the
    /// cursor; a style (step `style`) on the style of a citation. With
    /// `arg` on a citation, it goes, or the reference at the cursor.
    fn cite_insert(&mut self, arg: bool, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::cite::{self, Insert};
        let uri = context.text_document.uri.clone();
        if self.is_view(&uri) {
            return Err(Refusal::NotHere("This command works in Org files".to_string()));
        }
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let (text, parse, version) = (document.text.clone(), document.parse.clone(), document.version);
        let offset = document.index.offset(&text, context.position, encoding);
        let step = context.step.clone();
        let answer = context.input.clone();
        if step.is_some() && answer.is_none() {
            return Ok(Value::Null);
        }
        let answer = answer.unwrap_or_default();
        let action = cite::insert(&text, &parse, offset, arg).map_err(Refusal::Failed)?;
        // The completion table: each key with its line.
        let table = |server: &mut Self| -> std::result::Result<Vec<Value>, Refusal> {
            let files = server.bibliography(&uri, &text);
            let mut choices = Vec::new();
            for file in &files {
                for entry in &file.entries {
                    let line = entry.completion().map_err(Refusal::Failed)?;
                    choices.push(json!({ "key": entry.key, "label": line.trim_end() }));
                }
            }
            if choices.is_empty() {
                return Err(Refusal::Failed("No bibliography set".to_string()));
            }
            Ok(choices)
        };
        let ask = |kind: &str, prompt: String, choices: Vec<Value>, step: &str, answers: Option<String>| {
            let mut context = json!({ "step": step });
            if let Some(answers) = answers {
                context["answers"] = json!(answers);
            }
            json!({ "input": { "kind": kind, "prompt": prompt, "value": "", "choices": choices, "context": context } })
        };
        let styles = || -> Vec<Value> {
            std::iter::once(json!({ "key": "", "label": "(default)" }))
                .chain(cite::STYLES.iter().map(|style| json!({ "key": style, "label": style })))
                .collect()
        };
        let style_prompt = "Style (\"\" for default)".to_string();
        let (changes, cursor) = match action {
            Insert::Delete(changes) => (changes, None),
            Insert::Style { start, end } => {
                if step.as_deref() != Some("style") {
                    return Ok(ask("choice", style_prompt, styles(), "style", None));
                }
                let style = if answer.trim().is_empty() { String::new() } else { format!("/{answer}") };
                (vec![cite::Change { start, end, text: style }], None)
            }
            Insert::Key { place, reference } => {
                if step.as_deref() != Some("key") {
                    return Ok(ask("choice", "Key".to_string(), table(self)?, "key", None));
                }
                if answer.is_empty() && reference {
                    return Err(Refusal::Failed("Aborted".to_string()));
                }
                (vec![place.change(&answer)], None)
            }
            Insert::New { at, style } => {
                let mut keys: Vec<String> = context
                    .answers
                    .as_deref()
                    .unwrap_or_default()
                    .split(';')
                    .filter(|key| !key.is_empty())
                    .map(str::to_string)
                    .collect();
                let style_answer = match step.as_deref() {
                    Some("keys") if !answer.trim().is_empty() => {
                        keys.push(answer.clone());
                        None
                    }
                    Some("keys") if keys.is_empty() => return Err(Refusal::Failed("Aborted".to_string())),
                    Some("keys") if style => {
                        return Ok(ask("choice", style_prompt, styles(), "new-style", Some(keys.join(";"))));
                    }
                    Some("keys") => Some(String::new()),
                    Some("new-style") => Some(answer.clone()),
                    _ => None,
                };
                match style_answer {
                    Some(style) => {
                        let citation = cite::new_citation(&keys, &style);
                        let cursor = at + citation.len();
                        (vec![cite::Change { start: at, end: at, text: citation }], Some(cursor))
                    }
                    None => {
                        let mut choices = table(self)?;
                        if !keys.is_empty() {
                            choices.push(json!({ "key": "", "label": "(done)" }));
                        }
                        let prompt = cite::keys_prompt(&keys).trim_end_matches([':', ' ']).to_string();
                        return Ok(ask("choice", prompt, choices, "keys", Some(keys.join(";"))));
                    }
                }
            }
        };
        let new = cite::apply(&text, &changes);
        let edits = self.text_edits(&text, &new);
        let edit = self.workspace_edit(&uri, Some(version), edits);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        match cursor {
            Some(cursor) => {
                let position = LineIndex::new(&new).position(&new, cursor, encoding);
                self.apply_edit(params, Pending::EditAndMove { uri, position });
            }
            None => self.apply_edit(params, Pending::Edit),
        }
        Ok(Value::Null)
    }

    /// Where following `key` goes (`org-cite-basic-goto`): its entry in
    /// its bibliography file, or the start of the file when Org finds no
    /// entry there.
    fn cite_location(files: &[Rc<BibFile>], key: &str, encoding: Encoding) -> std::result::Result<Location, String> {
        let (file, _) = Self::cite_entry(files, key).ok_or_else(|| format!("Cannot find citation key: \"{key}\""))?;
        let name = file.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let at = org_core::cite::entry_position(&name, &file.text, key).unwrap_or(0);
        let position = LineIndex::new(&file.text).position(&file.text, at, encoding);
        let uri = uri::from_path(&file.path).ok_or("Cannot open the bibliography")?;
        Ok(Location { uri, range: Range::new(position, position) })
    }

    /// Over a citation key, its entry as the help echo of `oc-basic` shows
    /// it (`Doe, Roe. Title, Publisher, 2020.`), or for a key the
    /// bibliography does not have, the keys close to it.
    fn citation_hover(&mut self, uri: &Uri, position: Position) -> Option<lsp_types::Hover> {
        let document = self.document(uri)?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, position, self.client.encoding);
        let (start, citation) = org_core::cite::citation_at(&document.parse, offset)?;
        let references = org_core::cite::references(&citation, start);
        let reference = org_core::cite::key_at(&references, offset)?.clone();
        let files = self.bibliography(uri, &text);
        let value = match Self::cite_entry(&files, &reference.key) {
            Some((_, entry)) => entry.plain().unwrap_or_else(|error| error),
            None => {
                let keys: Vec<String> =
                    files.iter().flat_map(|file| file.entries.iter().map(|entry| entry.key.clone())).collect();
                let close = org_core::cite::close_keys(&reference.key, &keys);
                if close.is_empty() {
                    format!("Unknown citation key: @{}", reference.key)
                } else {
                    format!("Suggestions: {}", close.iter().map(|key| key.as_str()).collect::<Vec<_>>().join(" "))
                }
            }
        };
        let document = self.document(uri)?;
        let encoding = self.client.encoding;
        let range = Range::new(
            document.index.position(&text, reference.start, encoding),
            document.index.position(&text, reference.end, encoding),
        );
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::PlainText,
                value,
            }),
            range: Some(range),
        })
    }

    /// The citation keys of the document `key` that its bibliography does
    /// not have, as diagnostics (the `error` face of `oc-basic`), with the
    /// keys close to each (`org-cite-basic--close-keys`) to substitute.
    fn publish_citation_diagnostics(&mut self, key: &str) {
        let Some(document) = self.documents.get(key) else { return };
        let Ok(uri) = key.parse::<Uri>() else { return };
        let (text, parse, version) = (document.text.clone(), document.parse.clone(), document.version);
        let mut diagnostics = Vec::new();
        if text.contains("[cite") {
            let files = self.bibliography(&uri, &text);
            let keys: Vec<String> =
                files.iter().flat_map(|file| file.entries.iter().map(|entry| entry.key.clone())).collect();
            let encoding = self.client.encoding;
            let Some(document) = self.documents.get(key) else { return };
            for node in parse.syntax().descendants().filter(|node| node.kind() == org_syntax::SyntaxKind::CITATION) {
                let start = usize::from(node.text_range().start());
                for reference in org_core::cite::references(&node.text().to_string(), start) {
                    if keys.contains(&reference.key) {
                        continue;
                    }
                    let close: Vec<String> =
                        org_core::cite::close_keys(&reference.key, &keys).into_iter().cloned().collect();
                    let mut message = format!("Unknown citation key: @{}", reference.key);
                    if !close.is_empty() {
                        message.push_str(&format!(" (suggestions: {})", close.join(" ")));
                    }
                    let range = Range::new(
                        document.index.position(&text, reference.start, encoding),
                        document.index.position(&text, reference.end, encoding),
                    );
                    diagnostics.push(lsp_types::Diagnostic {
                        range,
                        severity: Some(lsp_types::DiagnosticSeverity::ERROR),
                        source: Some("org-lsp".to_string()),
                        message,
                        data: Some(json!({ "citationKeys": close })),
                        ..Default::default()
                    });
                }
            }
        }
        if diagnostics.is_empty() && !self.cite_diagnosed.remove(key) {
            return;
        }
        if !diagnostics.is_empty() {
            self.cite_diagnosed.insert(key.to_string());
        }
        let params = lsp_types::PublishDiagnosticsParams { uri, diagnostics, version: Some(version) };
        let notification = lsp_server::Notification::new(
            <lsp_types::notification::PublishDiagnostics as lsp_types::notification::Notification>::METHOD.to_string(),
            params,
        );
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// `org-indent-mode` for the client: how far each line of the document
    /// `key` is shown indented, with its leading stars hidden, as
    /// `org-lsp/virtualIndent` `{ uri, version, lines }`, each line
    /// `[line, prefix, wrap, hidden stars]`. Sent after every change while
    /// the document shows so, and once more when it stops.
    fn publish_virtual_indent(&mut self, key: &str) {
        if !self.client.virtual_indent {
            return;
        }
        let Some(document) = self.documents.get(key) else { return };
        let text = &document.text;
        let (indent, hide_stars) = (self.display.indented(text), self.display.hides_stars(text));
        let lines: Vec<[usize; 4]> = if indent || hide_stars {
            let adapt = self.display.adapt(text, self.indent.adapt);
            org_core::indent::virtual_indentation(text, indent, hide_stars, adapt)
                .into_iter()
                .map(|line| [line.line, line.prefix, line.wrap, line.hidden_stars])
                .collect()
        } else {
            Vec::new()
        };
        if lines.is_empty() && !self.virtually_indented.remove(key) {
            return;
        }
        if !lines.is_empty() {
            self.virtually_indented.insert(key.to_string());
        }
        let params = json!({ "uri": key, "version": document.version, "lines": lines });
        let notification = lsp_server::Notification::new("org-lsp/virtualIndent".to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// `org-adapt-indentation` in a document: off where it shows with
    /// `org-indent-mode`, unless it is `headline-data`.
    fn adapt_in(&self, text: &str) -> org_core::indent::Adapt {
        self.display.adapt(text, self.indent.adapt)
    }

    /// The indentation settings for a document.
    fn indent_in(&self, text: &str) -> org_core::indent::Settings {
        org_core::indent::Settings { adapt: self.adapt_in(text), ..self.indent.clone() }
    }

    fn clock_status(&mut self, running: &RunningClock) -> Option<Value> {
        let now = clock::minutes_of(self.date_time());
        let started = clock::minutes_of(running.start);
        self.with_source(&running.uri, |text, index, _, outline, _| {
            let line = outline.clocks.iter().find(|line| {
                line.clock.is_running()
                    && line.clock.start == Some(running.start)
                    && line.entry.is_some_and(|entry| outline.entries[entry].head == running.head)
            })?;
            let entry = &outline.entries[line.entry?];
            let property = |name: &str| entry.properties.iter().find(|(key, _)| key == name).map(|(_, value)| value);
            // `org-clock-mode-line-total` `auto`: since the last repeat if
            // there was one, else the whole task.
            let since = property("LAST_REPEAT").and_then(|stamp| {
                let read = org_core::timestr::parse_time_string(stamp)?;
                let (hour, minute) = read.time.unwrap_or((0, 0));
                Some(read.date.to_day() * 1440 + hour * 60 + minute)
            });
            let minutes = clock::subtree_minutes(text, entry.offset, since) + (now - started).max(0);
            let effort = property("EFFORT")
                .and_then(|effort| clock::duration_to_minutes(effort))
                .map(|effort| effort.round() as i64);
            let heading = org_core::agenda::report::plain_heading(&entry.head, entry.todo.as_deref());
            let time = match effort {
                Some(effort) => format!("{}/{}", clock::duration(minutes), clock::duration(effort)),
                None => clock::duration(minutes),
            };
            let position = index.position(text, entry.offset, Encoding::Utf16);
            Some(json!({
                "heading": heading,
                "minutes": minutes,
                "effort": effort,
                "text": format!("[{time}] ({heading})"),
                "uri": running.uri.as_str(),
                "line": position.line,
            }))
        })
        .flatten()
    }

    /// The headline above byte `at` of `text`.
    fn heading_above(text: &str, at: usize) -> Option<usize> {
        let mut line = text[..at.min(text.len())].rfind('\n').map_or(0, |index| index + 1);
        loop {
            let end = text[line..].find('\n').map_or(text.len(), |end| line + end);
            if text[line..end].starts_with('*') && text[line..end].trim_start_matches('*').starts_with([' ', '\t']) {
                return Some(line);
            }
            if line == 0 {
                return None;
            }
            line = text[..line - 1].rfind('\n').map_or(0, |index| index + 1);
        }
    }

    /// Ask the client for inlay hints again, if it can be asked.
    fn refresh_inlay_hints(&mut self) {
        if self.client.inlay_refresh {
            self.send_raw_request("workspace/inlayHint/refresh", Value::Null, None);
        }
    }

    /// `org-clock-display` (`C-c C-x C-d`): the time clocked under each
    /// headline of the file, its subtree included, within
    /// `org_clock_display_default_range`, or with a count a range picked
    /// from a list; shown after the headlines until the file changes.
    fn clock_display(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        const RANGES: [&str; 8] =
            ["today", "yesterday", "thisweek", "lastweek", "thismonth", "lastmonth", "thisyear", "lastyear"];
        let uri = context.text_document.uri.clone();
        if self.is_view(&uri) {
            return Err(Refusal::NotHere("This command works in Org files".to_string()));
        }
        let custom = context.count.is_some_and(|count| count > 0);
        let range = if custom {
            let Some(answer) = context.input.clone() else {
                let choices: Vec<Value> = RANGES.iter().map(|range| json!({ "key": range, "label": range })).collect();
                return Ok(
                    json!({ "input": { "kind": "choice", "prompt": "Range", "value": "", "choices": choices } }),
                );
            };
            answer
        } else {
            self.clock_display_range.clone()
        };
        let now = self.date_time();
        let span = org_core::clock_range::special_range(&range, now, None, None, 0).map_err(Refusal::Failed)?;
        let document = self.document(&uri).ok_or("The document is not open")?;
        let (sums, total) =
            clock::sums(&document.text, span.start.map(clock::minutes_of), Some(clock::minutes_of(span.end)));
        let lines: Vec<(u32, i64)> = sums
            .into_iter()
            .map(|(offset, minutes)| {
                (document.index.position(&document.text, offset, self.client.encoding).line, minutes)
            })
            .collect();
        self.clock_display.insert(uri.as_str().to_string(), lines);
        self.refresh_inlay_hints();
        let label = if custom { "Total file time (custom)" } else { "Total file time" };
        Ok(
            json!({ "message": format!("{label}: {} ({} hours and {} minutes)", clock::duration(total), total / 60, total % 60) }),
        )
    }

    /// Whether `test` holds at the cursor of an Org file.
    fn at_clocktable(&self, context: &Context, test: fn(&str, usize) -> bool) -> bool {
        let Some(document) = self.document(&context.text_document.uri) else { return false };
        test(&document.text, document.index.offset(&document.text, context.position, self.client.encoding))
    }

    /// Clock tables: `org-clock-report` (`C-c C-x C-r`), `org-dblock-update`
    /// (`C-c C-x C-u`), `C-c C-c` on a dynamic block's `#+BEGIN:` or
    /// `#+END:` line, and `S-<arrows>` on a clock table's `#+BEGIN:` line
    /// (`org-clocktable-shift`).
    fn clocktable(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::clocktable;
        let uri = context.text_document.uri.clone();
        if self.is_view(&uri) {
            return Err(Refusal::NotHere("This command works in Org files".to_string()));
        }
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, encoding);
        let path = uri::to_path(&uri).map(|path| path.to_string_lossy().into_owned());
        let files = self.clocktable_files(path.as_deref());
        let settings = clocktable::Settings {
            now: self.date_time(),
            keywords: self.todo.clone(),
            inheritance: self.workspace.inheritance.clone(),
            ..self.clocktable.clone()
        };
        let table = clocktable::Context { path: path.as_deref(), settings: &settings, files: &files };
        let count = context.count.filter(|count| *count > 0);
        let done = match id {
            commands::CLOCK_REPORT => clocktable::report(&text, offset, count.is_some(), &table),
            commands::DBLOCK_UPDATE => clocktable::update(&text, offset, count.is_some(), &table),
            commands::CHECKBOX_TOGGLE => clocktable::update_here(&text, offset, &table),
            _ => clocktable::shift(
                &text,
                offset,
                matches!(id, commands::SHIFT_RIGHT | commands::SHIFT_UP),
                count,
                &table,
            ),
        }
        .map_err(Refusal::Failed)?;
        if let Some(message) = done.message {
            self.message = Some(message);
        }
        self.carried(context, org_core::newline::Outcome { text: done.text, cursor: done.cursor })
    }

    /// The files a clock table may read: the agenda files, the files
    /// they and the file at `path` archive to, and the open files; others
    /// are read from disk when a table names them.
    fn clocktable_files(&mut self, path: Option<&str>) -> ClocktableFiles {
        let mut files = ClocktableFiles::default();
        let open: HashMap<PathBuf, String> = self
            .documents
            .keys()
            .filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, key.clone())))
            .collect();
        for (file, key) in &open {
            files.texts.insert(file.to_string_lossy().into_owned(), self.documents[key].text.clone());
        }
        for (uri, held) in self.agenda_files() {
            let Some(file) = uri::to_path(&uri) else { continue };
            let name = file.to_string_lossy().into_owned();
            let text = match &held {
                Held::Open(outline) => outline.text.clone(),
                Held::Disk(source) => source.text.clone(),
            };
            files.texts.entry(name.clone()).or_insert(text);
            files.agenda.push(name);
        }
        let mut sources: Vec<String> = files.agenda.clone();
        sources.extend(path.map(str::to_string));
        for source in sources {
            let archives = self.archives_of(Path::new(&source), &open);
            files.archives.insert(source, archives.iter().map(|file| file.to_string_lossy().into_owned()).collect());
        }
        files
    }

    /// `text` with the entry at `heading` put in `state`
    /// (`org-clock-in-switch-to-state`, `org-clock-out-switch-to-state`).
    fn switch_state(&self, text: &str, heading: usize, state: Option<&str>) -> std::result::Result<String, String> {
        let Some(state) = state.filter(|state| !state.is_empty()) else { return Ok(text.to_string()) };
        let parse = org_syntax::parse(text, &self.todo);
        clock::switch_state(text, parse.todo_keywords(), heading, state, &self.edit_settings.todo, self.date_time())
    }

    /// `org-clock-in`, `org-clock-out`, `org-clock-cancel` and
    /// `org-clock-goto`, in an Org file or on an agenda line.
    fn clock_command(&mut self, id: &str, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        self.last = None;
        match id {
            commands::CLOCK_IN => self.clock_in(context.ok_or("This command needs the cursor position")?),
            commands::CLOCK_IN_LAST => self.clock_in_last(context.ok_or("This command needs the cursor position")?),
            commands::CLOCK_OUT | commands::CLOCK_CANCEL => {
                let running = self.running.clone().ok_or("No active clock")?;
                let Some((text, version, line)) = self.clock_line(&running) else {
                    self.running = None;
                    self.publish_clock();
                    return Err("Clock start time is gone".into());
                };
                let (new_text, message) = if id == commands::CLOCK_OUT {
                    let out = clock::clock_out(&text, line, self.date_time(), &self.clock_settings)?;
                    self.clock_out_time = Some(self.date_time());
                    let switched = match Self::heading_above(&text, line) {
                        Some(heading) => self.switch_state(&out.text, heading, self.clock_out_switch.as_deref())?,
                        None => out.text,
                    };
                    (switched, out.message)
                } else {
                    (clock::cancel(&text, line, &self.clock_settings)?, "Clock canceled".to_string())
                };
                if let Some(heading) = Self::heading_above(&text, line) {
                    let head = text[heading..].lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
                    self.last_clocked = Some((running.uri.clone(), head));
                }
                self.running = None;
                self.send_texts(vec![(running.uri, version, text, new_text)]);
                self.message = Some(message);
                self.publish_clock();
                Ok(Value::Null)
            }
            commands::CLOCK_GOTO => {
                let (uri, offset, recent) = match self.running.clone() {
                    Some(running) => {
                        let (text, _, line) = self.clock_line(&running).ok_or("Clock start time is gone")?;
                        (running.uri, Self::heading_above(&text, line).ok_or("Clock start time is gone")?, false)
                    }
                    None => {
                        let (uri, head) = self.last_clocked.clone().ok_or("No active or recent clock task")?;
                        let offset = self
                            .with_source(&uri, |_, _, _, outline, _| {
                                outline.entries.iter().find(|entry| entry.head == head).map(|entry| entry.offset)
                            })
                            .flatten()
                            .ok_or("No active or recent clock task")?;
                        (uri, offset, true)
                    }
                };
                let encoding = self.client.encoding;
                let position = self
                    .with_source(&uri, |text, index, _, _, _| index.position(text, offset, encoding))
                    .ok_or("Cannot read the file of the clocked entry")?;
                let params = ShowDocumentParams {
                    uri,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(Range::new(position, position)),
                };
                self.send_request::<ShowDocument>(params, None);
                if recent {
                    self.message = Some("No running clock, this is the most recently clocked task".to_string());
                }
                Ok(Value::Null)
            }
            other => Err(format!("org-lsp has no command `{other}`").into()),
        }
    }

    /// `org-clock-in` on the entry at the cursor, or on the entry of an
    /// agenda line. A clock that runs elsewhere stops first.
    fn clock_in(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let count = context.count.unwrap_or(0);
        // `C-u`: an entry of the history.
        if count == 4 {
            return self.clock_in_selected(context);
        }
        let (file, heading) = self.clock_target(context)?;
        // `C-u C-u`: the entry becomes the default task.
        if count == 16 {
            self.clock_default_task = self.entry_title(&file, heading).map(|title| (file.clone(), title));
        }
        // `C-u C-u C-u`: from when the last clock stopped.
        let now = self.date_time();
        let start = if count == 64 { self.clock_out_time.unwrap_or(now) } else { now };
        self.clock_in_at(file, heading, start)
    }

    /// The title of the entry at `heading` of `uri`, as the clock history
    /// knows it: the headline without keyword, priority and tags.
    fn entry_title(&mut self, uri: &Uri, heading: usize) -> Option<String> {
        self.with_source(uri, |_, _, _, outline, _| {
            let entry = &outline.entries[outline.entry_at(heading)?];
            Some(org_core::agenda::report::plain_heading(&entry.head, entry.todo.as_deref()))
        })
        .flatten()
    }

    /// Where the entry with `title` starts in `uri`.
    fn find_title(&mut self, uri: &Uri, title: &str) -> Option<usize> {
        self.with_source(uri, |_, _, _, outline, _| {
            outline
                .entries
                .iter()
                .find(|entry| org_core::agenda::report::plain_heading(&entry.head, entry.todo.as_deref()) == title)
                .map(|entry| entry.offset)
        })
        .flatten()
    }

    /// `org-clock-select-task`: the default task (`d`), the interrupted
    /// one (`i`), the clocked one (`c`) and the history (`1`, `2`…), to
    /// pick from; asked as a choice, then clocked into.
    fn clock_in_selected(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        // The history without repeats in a row.
        let mut recent: Vec<(Uri, String)> = Vec::new();
        for task in &self.clock_history {
            if recent.last() != Some(task) {
                recent.push(task.clone());
            }
        }
        if recent.is_empty() {
            return Err("No recent clock".into());
        }
        let mut tasks: Vec<(String, Uri, String)> = Vec::new();
        if let Some((uri, title)) = self.clock_default_task.clone() {
            tasks.push(("d".to_string(), uri, title));
        }
        if let Some((uri, title)) = self.clock_interrupted.clone() {
            tasks.push(("i".to_string(), uri, title));
        }
        if let Some(running) = self.running.clone() {
            if let Some(title) = self
                .clock_line(&running)
                .and_then(|(text, _, line)| Self::heading_above(&text, line))
                .and_then(|heading| self.entry_title(&running.uri, heading))
            {
                tasks.push(("c".to_string(), running.uri, title));
            }
        }
        let mut number = 0;
        for (uri, title) in recent {
            number += 1;
            let key = if number < 10 { char::from(b'0' + number as u8) } else { char::from(b'A' + number as u8 - 10) };
            tasks.push((key.to_string(), uri, title));
        }
        // What each line shows: the category and the headline.
        let mut found: Vec<(String, Uri, usize, String)> = Vec::new();
        for (key, uri, title) in tasks {
            let Some(heading) = self.find_title(&uri, &title) else { continue };
            let label = self
                .with_source(&uri, |_, _, _, outline, _| {
                    let entry = &outline.entries[outline.entry_at(heading)?];
                    let head = match org_core::text::tag_group(&entry.head) {
                        Some((blanks, _)) => entry.head[..blanks].trim_end(),
                        None => entry.head.as_str(),
                    };
                    Some(format!("{:<12}  {head}", entry.category))
                })
                .flatten()
                .unwrap_or_default();
            found.push((key, uri, heading, label));
        }
        let Some(answer) = context.input.clone() else {
            let choices: Vec<Value> =
                found.iter().map(|(key, _, _, label)| json!({ "key": key, "label": label })).collect();
            return Ok(json!({ "input": {
                "kind": "choice",
                "prompt": "Clock-in on task: ",
                "value": "",
                "choices": choices,
            } }));
        };
        if answer == "q" || answer == "x" {
            return Err("Abort".into());
        }
        let (_, uri, heading, _) = found
            .into_iter()
            .find(|(key, ..)| *key == answer)
            .ok_or_else(|| format!("Invalid task choice {answer}"))?;
        let now = self.date_time();
        self.clock_in_at(uri, heading, now)
    }

    /// `org-clock-in-last` (`C-c C-x C-x`): the clock again on the entry
    /// clocked last; with `C-u`, one picked from the history, and with
    /// `C-u C-u` from when the last clock stopped.
    fn clock_in_last(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let count = context.count.unwrap_or(0);
        if count == 4 {
            return self.clock_in_selected(context);
        }
        let Some((uri, title)) = self.clock_history.first().cloned() else {
            self.message = Some("No last clock".to_string());
            return Ok(Value::Null);
        };
        let heading = self.find_title(&uri, &title).ok_or("The entry clocked last is gone")?;
        let now = self.date_time();
        let start = if count == 16 { self.clock_out_time.unwrap_or(now) } else { now };
        let already = self.running.is_some();
        let result = self.clock_in_at(uri.clone(), heading, start)?;
        if !already {
            let file = uri::to_path(&uri)
                .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
                .unwrap_or_default();
            self.message = Some(format!("Clocking back: {title} (in {file})"));
        }
        Ok(result)
    }

    /// The file and the headline the clock goes to: the entry at the
    /// cursor, or that of an agenda line.
    fn clock_target(&mut self, context: &Context) -> std::result::Result<(Uri, usize), Refusal> {
        let encoding = self.client.encoding;
        let uri = context.text_document.uri.clone();
        let (file, heading) = if self.is_view(&uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            let heading = self
                .with_source(&reference.uri, |_, _, _, outline, _| {
                    views::resolve(&reference, outline).map(|entry| outline.entries[entry].offset)
                })
                .flatten()
                .ok_or("The entry is gone; rebuild the agenda")?;
            (reference.uri, heading)
        } else {
            let document = self.document(&uri).ok_or("The document is not open")?;
            let offset = document.index.offset(&document.text, context.position, encoding);
            let heading = Self::heading_above(&document.text, offset).ok_or("Before first headline")?;
            (uri, heading)
        };
        Ok((file, heading))
    }

    /// The clock started on the entry at `heading` of `file` at `start`;
    /// one that runs elsewhere stops now.
    fn clock_in_at(&mut self, file: Uri, heading: usize, start: DateTime) -> std::result::Result<Value, Refusal> {
        let now = self.date_time();
        let mut changes: Vec<(Uri, Option<i32>, String, String)> = Vec::new();
        let mut heading = heading;
        let title = self.entry_title(&file, heading);
        let mut interrupted = None;
        if let Some(running) = self.running.clone() {
            if let Some((text, version, line)) = self.clock_line(&running) {
                // Already on this entry: the clock continues.
                if running.uri == file && Self::heading_above(&text, line) == Some(heading) {
                    let title = self
                        .with_source(&file, |_, _, _, outline, _| {
                            let entry = &outline.entries[outline.entry_at(heading)?];
                            Some(org_core::agenda::report::plain_heading(&entry.head, entry.todo.as_deref()))
                        })
                        .flatten()
                        .unwrap_or_default();
                    self.message = Some(format!("Clock continues in \"{title}\""));
                    return Ok(Value::Null);
                }
                interrupted = Self::heading_above(&text, line)
                    .and_then(|running_heading| self.entry_title(&running.uri, running_heading))
                    .map(|title| (running.uri.clone(), title));
                let out = clock::clock_out(&text, line, now, &self.clock_settings)?;
                self.clock_out_time = Some(now);
                let out_text = match Self::heading_above(&text, line) {
                    Some(running_heading) => {
                        self.switch_state(&out.text, running_heading, self.clock_out_switch.as_deref())?
                    }
                    None => out.text,
                };
                if running.uri == file && line < heading {
                    heading = heading + out_text.len() - text.len();
                }
                changes.push((running.uri.clone(), version, text, out_text));
            }
        }
        let mut settings = self.clock_settings.clone();
        let (display, adapt) = (self.display, self.indent.adapt);
        let base = changes.iter().find(|(uri, ..)| *uri == file).map(|(_, _, _, new)| new.clone());
        let switch = self.clock_in_switch.clone().filter(|state| !state.is_empty());
        let (todo, now_time) = (self.edit_settings.todo.clone(), now);
        let found = self.with_source(&file, |text, _, parse, outline, version| {
            let base = base.clone().unwrap_or_else(|| text.to_string());
            let base = match &switch {
                Some(state) => clock::switch_state(&base, parse.todo_keywords(), heading, state, &todo, now_time)?,
                None => base,
            };
            settings.adapt = display.adapt(&base, adapt);
            let clocked = clock::clock_in(&base, parse.todo_keywords(), heading, &settings, start)?;
            let entry = &outline.entries[outline.entry_at(heading).ok_or("Before first headline")?];
            let repeat = entry.properties.iter().any(|(key, _)| key == "LAST_REPEAT");
            // The headline as the clock leaves it, its state switched.
            let head = base[heading..].lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
            Ok::<_, String>((text.to_string(), version, clocked.0, head, repeat))
        });
        let (old, version, new_text, head, repeat) = found.ok_or("Cannot read the file of this entry")??;
        match changes.iter_mut().find(|(uri, ..)| *uri == file) {
            Some(change) => change.3 = new_text,
            None => changes.push((file.clone(), version, old, new_text)),
        }
        self.send_texts(changes);
        self.running = Some(RunningClock { uri: file.clone(), start, head: head.clone() });
        self.clock_interrupted = interrupted;
        // `org-clock-history-push`
        if let Some(title) = title {
            let task = (file.clone(), title);
            self.clock_history.retain(|other| *other != task);
            self.clock_history.truncate(self.clock_history_length - 1);
            self.clock_history.insert(0, task);
        }
        self.last_clocked = Some((file, head));
        let shown = if repeat { "showing task time since last repeat." } else { "showing entire task time." };
        self.message =
            Some(format!("Clock starts at {} - {shown}", org_core::stamp::format_timestamp(start, true, true, "")));
        self.publish_clock();
        Ok(Value::Null)
    }

    /// `org-clock-out-when-done`: the entry at `heading` of `file`, which
    /// the clock runs on, became done in `new_text`; the clock stops there
    /// too, in the same edit.
    fn clock_out_if_done(
        &mut self,
        file: &Uri,
        heading: usize,
        edits: Vec<TextEdit>,
        new_text: String,
    ) -> (Vec<TextEdit>, String) {
        let unchanged = |edits, new_text| (edits, new_text);
        let Some(running) = self.running.clone().filter(|running| self.clock_out_when_done && running.uri == *file)
        else {
            return unchanged(edits, new_text);
        };
        let Some(old) = self.with_source(file, |text, _, _, _, _| text.to_string()) else {
            return unchanged(edits, new_text);
        };
        // The clock's line, among those that started in the same minute.
        let same_start = |text: &str| -> Vec<usize> {
            clock::running(text)
                .into_iter()
                .filter(|(_, start)| *start == running.start)
                .map(|(line, _)| line)
                .collect()
        };
        let before = same_start(&old);
        let Some(rank) = before.iter().position(|line| {
            Self::heading_above(&old, *line) == Some(heading)
                && old[heading..].lines().next().unwrap_or("").trim_start_matches('*').trim() == running.head
        }) else {
            return unchanged(edits, new_text);
        };
        let Some(&line) = same_start(&new_text).get(rank) else { return unchanged(edits, new_text) };
        let Some(headline) = Self::heading_above(&new_text, line) else { return unchanged(edits, new_text) };
        let parse = org_syntax::parse(&new_text, &self.todo);
        let first_word =
            new_text[headline..].lines().next().unwrap_or("").trim_start_matches('*').split_whitespace().next();
        if !first_word.is_some_and(|word| parse.todo_keywords().is_done(word)) {
            return unchanged(edits, new_text);
        }
        let Ok(out) = clock::clock_out(&new_text, line, self.date_time(), &self.clock_settings) else {
            return unchanged(edits, new_text);
        };
        let head = new_text[headline..].lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
        self.last_clocked = Some((file.clone(), head));
        self.clock_out_time = Some(self.date_time());
        self.running = None;
        self.publish_clock();
        let encoding = self.client.encoding;
        let index = LineIndex::new(&old);
        let edits = actions::edits_between(&old, &out.text)
            .into_iter()
            .map(|edit| TextEdit { range: index.range(&old, edit.range, encoding), new_text: edit.new_text })
            .collect();
        (edits, out.text)
    }

    /// Edit files from their old text to their new one, in one edit.
    fn send_texts(&mut self, changes: Vec<(Uri, Option<i32>, String, String)>) {
        let encoding = self.client.encoding;
        let mut documents = Vec::new();
        for (uri, version, old, new) in changes {
            let index = LineIndex::new(&old);
            let edits: Vec<TextEdit> = actions::edits_between(&old, &new)
                .into_iter()
                .map(|edit| TextEdit { range: index.range(&old, edit.range, encoding), new_text: edit.new_text })
                .collect();
            if !edits.is_empty() {
                self.edited.insert(uri.as_str().to_string());
                documents.push((uri, version, edits));
            }
        }
        if documents.is_empty() {
            return;
        }
        let edit = self.workspace_edits(documents);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
    }

    /// Where the entry at `heading` of `file` will stand in the agenda
    /// once `file` reads `text`, if that is on another line than the one
    /// the cursor is on.
    fn followed(&mut self, file: &Uri, text: &str, heading: usize, from: (Position, &ViewLine)) -> Option<Follow> {
        let query = self.view.as_ref()?.view.query.clone();
        let parse = org_syntax::parse(text, &self.todo);
        let outline = Outline::with_inheritance(&parse, text, &uri::file_stem(file), &self.workspace.inheritance);
        let view = self.render_with(query, Some((file, outline)));
        let (position, line) = from;
        let target = view.line_of(file, heading, (position.line, line))?;
        (target != position.line).then(|| Follow {
            text: view.text,
            from: position.line,
            position: Position::new(target, position.character),
        })
    }
}

/// Org's meta keys on a list item: `M-<left>` and `M-<right>` outdent and
/// indent it alone, with `S-` its children too; `M-<up>` and `M-<down>`
/// move it with its children.
fn item_structure(
    id: &str,
    text: &str,
    offset: usize,
    keywords: &org_syntax::TodoKeywords,
    tags_column: i64,
) -> std::result::Result<org_core::list::Edited, Refusal> {
    use org_core::list;
    match id {
        commands::META_LEFT => list::indent_item(text, offset, -1, false, keywords, tags_column),
        commands::META_RIGHT => list::indent_item(text, offset, 1, false, keywords, tags_column),
        commands::SHIFT_META_LEFT => list::indent_item(text, offset, -1, true, keywords, tags_column),
        commands::SHIFT_META_RIGHT => list::indent_item(text, offset, 1, true, keywords, tags_column),
        commands::META_UP => list::move_item(text, offset, false),
        _ => list::move_item(text, offset, true),
    }
}
