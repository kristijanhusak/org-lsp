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
    DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult,
    Location, MessageType, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, ShowDocumentParams, ShowMessageParams,
    TextDocumentEdit, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Uri, WorkspaceEdit,
};
use org_core::agenda::{self, Config as AgendaConfig, Now, Span};
use org_core::capture;
use org_core::clock;
use org_core::date::{Date, DateTime, Day};
use org_core::edit::{self, Call, Command as EditCommand, Settings as EditSettings};
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

use crate::config::{self, CaptureTemplateOption, RefileFiles, RefileTarget};
use crate::document::Document;
use crate::line_index::{Encoding, LineIndex};
use crate::prompt::{self, DatePrompt, Target, TimeField};
use crate::views::{self, EntryRef, Query, View, ViewLine};
use crate::workspace::{Source, Workspace};
use crate::{actions, cli, commands, completion, custom, folding, highlight, manifest, symbols, uri};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// LSP 3.18 methods that `lsp-types` does not know yet.
const TEXT_DOCUMENT_CONTENT: &str = "workspace/textDocumentContent";
const TEXT_DOCUMENT_CONTENT_REFRESH: &str = "workspace/textDocumentContent/refresh";

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
    /// The client rereads semantic tokens when asked.
    tokens_refresh: bool,
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
        // No target: the end of `defaultNotesFile`, as `["file", ""]`.
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
    /// the day if it is shown there, else to today or the first day.
    Block { block: usize, day: Option<Day> },
}

impl Reveal {
    /// Where the cursor goes in `view`.
    fn position(&self, view: &View) -> Position {
        match self {
            Reveal::Home => Position::new(view.home_line(), 0),
            Reveal::At(position) => *position,
            Reveal::Block { block, day } => Position::new(view.block_line(*block, *day), 0),
        }
    }
}

/// A source of agenda entries as the server holds it while it works.
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

struct Server {
    client: Client,
    sender: crossbeam_channel::Sender<Message>,
    next_request: i32,
    pending: HashMap<RequestId, Pending>,
    /// What the command being run has to say, as Emacs says it in the echo
    /// area. It goes back with the command's result.
    message: Option<String>,
    /// The document the command being run was run in, whose edits are the
    /// user's to save.
    origin: Option<String>,
    /// `autoSave`: save the files a command changes elsewhere.
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
    /// A fixed clock, for tests.
    clock: Option<Now>,
    /// The clock that runs, as the files say.
    running: Option<RunningClock>,
    /// The entry clocked last: its file and headline, for
    /// `org.clock.goto` when no clock runs.
    last_clocked: Option<(Uri, String)>,
    /// `org-clock-into-drawer` and its relatives.
    clock_settings: org_core::clock::Settings,
    /// `org-clock-out-when-done`
    clock_out_when_done: bool,
    /// The clock's status waits for the edit that writes its line.
    clock_pending: bool,
    /// Log mode (`l`) and the clock report (`R`) in the agenda.
    agenda_log: bool,
    agenda_clockreport: bool,
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
            == Some(true),
        tokens_refresh: raw_params
            .pointer("/capabilities/workspace/semanticTokens/refreshSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false),
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
        capabilities: capabilities(client.encoding),
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
        next_request: 0,
        pending: HashMap::new(),
        message: None,
        origin: None,
        auto_save: options.auto_save.unwrap_or(true),
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
        capture_templates: options.capture_templates(),
        org_directory: options.org_directory(),
        default_notes_file: options.default_notes_file(),
        capture: None,
        note: None,
        captures: 0,
        capture_directory: capture_directory(),
        date_prompt: None,
        prompt_copy: None,
        separator_lines: options.separator_lines(),
        indent: options.indent_settings(),
        display: options.display(),
        virtually_indented: HashSet::new(),
        clock: options.clock.as_ref().and_then(|clock| clock.to_now()),
        running: None,
        clock_pending: false,
        last_clocked: None,
        clock_settings: options.clock_settings(),
        clock_out_when_done: options.clock_out_when_done.unwrap_or(true),
        agenda_log: options.agenda.start_with_log_mode.unwrap_or(false),
        agenda_clockreport: options.agenda.start_with_clockreport_mode.unwrap_or(false),
        documents: HashMap::new(),
        workspace: Workspace::new(options.agenda_files, folders),
        view: None,
        edited: HashSet::new(),
        reveal: None,
        follow: None,
        moving: None,
        last: None,
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
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = server.request(request);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => server.notification(notification),
            Message::Response(response) => server.response(response),
        }
    }
    server.remove_view_file();
    if server.captures > 0 {
        let _ = std::fs::remove_dir_all(&server.capture_directory);
    }
    Ok(())
}

/// A command's result with what it has to say: `{ "message": … }` on top
/// of whatever else the result holds.
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
/// entry is out of sight, its TODO state. A state set to the one it had is
/// worth a word as well.
fn headline_message(
    old: &str,
    new: &str,
    heading: usize,
    keywords: &TodoKeywords,
    from_agenda: bool,
    explicit: bool,
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
    if keyword == old_keyword && explicit {
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

/// A directory for the capture buffers of a server: one of its own, also
/// when a process runs more than one (the tests).
fn capture_directory() -> PathBuf {
    static SERVERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let server = SERVERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("org-lsp-capture-{}-{server}", std::process::id()))
}

fn capabilities(encoding: Encoding) -> ServerCapabilities {
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
        experimental: Some(json!({ "org": manifest::manifest() })),
        ..Default::default()
    }
}

/// An edit of `org_core::indent` as the client counts positions.
fn indent_edit(text: &str, index: &LineIndex, encoding: Encoding, edit: org_core::indent::Edit) -> TextEdit {
    let range = org_syntax::TextRange::new((edit.start as u32).into(), (edit.end as u32).into());
    TextEdit { range: index.range(text, range, encoding), new_text: edit.text }
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

    /// Ask the editor to apply `params`. With `autoSave`, the files it
    /// changes other than the one the command was run in (an entry's file
    /// changed from the agenda, the target of a capture or a refile) are
    /// saved once the editor has applied the edit.
    fn apply_edit(&mut self, params: lsp_types::ApplyWorkspaceEditParams, pending: Pending) {
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
            None => {}
        }
    }

    // ----- requests

    fn request(&mut self, request: Request) -> Response {
        macro_rules! dispatch {
            ($($request:ty => $handler:ident,)*) => {
                match request.method.as_str() {
                    $(<$request>::METHOD => match parameters(request) {
                        Ok((id, params)) => respond(id, self.$handler(params)),
                        Err(response) => response,
                    },)*
                    ExecuteCommand::METHOD => match parameters::<ExecuteCommandParams>(request) {
                        Ok((id, params)) => {
                            self.message = None;
                            self.origin = params
                                .arguments
                                .first()
                                .and_then(|argument| argument.pointer("/textDocument/uri")?.as_str())
                                .map(str::to_string);
                            match self.execute_command(params) {
                                Ok(value) => respond(id, with_message(value, self.message.take())),
                                Err(refusal) => refused(id, refusal),
                            }
                        }
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
            lsp_types::request::Completion => completion,
            lsp_types::request::OnTypeFormatting => on_type_formatting,
            lsp_types::request::RangeFormatting => range_formatting,
        }
    }

    fn notification(&mut self, notification: Notification) {
        // A notification the server cannot read is dropped: there is no way
        // to answer one, and the next full sync repairs any lost change.
        fn parse<P: DeserializeOwned>(notification: Notification) -> Option<P> {
            serde_json::from_value(notification.params).ok()
        }
        match notification.method.as_str() {
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
                        open.forget_shown();
                    }
                    self.reveal_when_current();
                    return;
                }
                let key = document.uri.as_str().to_string();
                self.documents.insert(key.clone(), Document::new(document.text, document.version, &self.todo));
                self.sync_clock(&key);
                self.publish_virtual_indent(&key);
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
                // The agenda was closed: stop rebuilding it.
                if self.view.as_ref().is_some_and(|open| open.uri == params.text_document.uri) {
                    self.remove_view_file();
                    self.view = None;
                    self.reveal = None;
                    self.follow = None;
                    self.moving = None;
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

    fn code_actions(&mut self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let only = params.context.only.as_deref();
        if self.is_view(uri) {
            // On an agenda line the actions are commands: the server
            // edits the entry's file and rebuilds the agenda.
            let (_, line) = self.view_line(uri, params.range.start.line)?;
            line.entry.as_ref()?;
            let dated = line.entry.as_ref().is_some_and(|entry| entry.marker.is_some());
            let running = self.running.is_some();
            let context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
            let actions = [
                commands::TODO_NEXT,
                commands::TODO_PREVIOUS,
                commands::PRIORITY_UP,
                commands::PRIORITY_DOWN,
                commands::DATE_LATER,
                commands::DATE_EARLIER,
                commands::CLOCK_IN,
                commands::CLOCK_OUT,
                commands::AGENDA_GOTO,
            ]
            .into_iter()
            .filter(|id| dated || !matches!(*id, commands::DATE_LATER | commands::DATE_EARLIER))
            .filter(|id| running || *id != commands::CLOCK_OUT)
            .filter(|id| actions::requested(id, only))
            .map(|id| {
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
            })
            .collect();
            return Some(actions);
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
        // Clocking, as commands: they may change another file too.
        let in_entry = Self::heading_above(&document.text, offset).is_some();
        let running = self.running.is_some();
        let context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
        for (id, offered) in
            [(commands::CLOCK_IN, in_entry), (commands::CLOCK_OUT, running), (commands::CLOCK_CANCEL, running)]
        {
            if !offered || !actions::requested(id, only) {
                continue;
            }
            let title = commands::title(id).to_string();
            result.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: title.clone(),
                kind: Some(CodeActionKind::new(id)),
                command: Some(lsp_types::Command {
                    title,
                    command: id.to_string(),
                    arguments: Some(vec![context.clone()]),
                }),
                ..Default::default()
            }));
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

    /// `org-indent-region` over the lines of the range.
    fn range_formatting(&mut self, params: lsp_types::DocumentRangeFormattingParams) -> Option<Vec<TextEdit>> {
        let encoding = self.client.encoding;
        let document = self.document(&params.text_document.uri)?;
        let (text, index) = (&document.text, &document.index);
        let start = index.offset(text, params.range.start, encoding);
        let end = index.offset(text, params.range.end, encoding);
        let edits = org_core::indent::indent_region(text, start, end, &self.indent_in(text));
        Some(edits.into_iter().map(|edit| indent_edit(text, index, encoding, edit)).collect())
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
                }),
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
        let reference = self.view_line(&position.text_document.uri, position.position.line)?.1.entry.clone()?;
        self.locate(&reference).map(GotoDefinitionResponse::Scalar)
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
        if let Some(document) = self.documents.get(uri.as_str()) {
            let outline = Outline::new(&document.parse, &document.text, &uri::file_stem(uri));
            return Some(action(&document.text, &document.index, &document.parse, &outline, Some(document.version)));
        }
        let source = self.workspace.read(&uri::to_path(uri)?, &self.todo)?;
        Some(action(&source.text, &source.index, &source.parse, &source.outline, None))
    }

    /// The agenda files with their outlines, in agenda order.
    fn agenda_files(&mut self) -> Vec<(Uri, Held)> {
        let open = |document: &Document, uri: &Uri| {
            Held::Open(Box::new(Outline::new(&document.parse, &document.text, &uri::file_stem(uri))))
        };
        if !self.workspace.is_configured() {
            // Nothing says where the agenda files are: use what is open.
            let mut keys: Vec<&String> = self.documents.keys().collect();
            keys.sort();
            return keys
                .into_iter()
                .filter_map(|key| {
                    let uri: Uri = key.parse().ok()?;
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
        files
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
        if let Some((uri, outline)) = replaced {
            if let Some(file) = files.iter_mut().find(|(file, _)| file == uri) {
                file.1 = Held::Open(Box::new(outline));
            }
        }
        let outlines: Vec<(Uri, &Outline)> = files.iter().map(|(uri, held)| (uri.clone(), held.outline())).collect();
        // A command that no longer reads shows nothing; opening it said why.
        let blocks = self.blocks(&query).unwrap_or_default();
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
        views::render(query, &blocks, &outlines, self.now(), clocked)
    }

    /// The blocks `query` shows, in log mode and with the clock report as
    /// `l` and `R` have them.
    fn blocks(&self, query: &Query) -> std::result::Result<Vec<views::Block>, String> {
        let mut blocks = self.query_blocks(query)?;
        for block in &mut blocks {
            block.config.show_log = self.agenda_log;
            block.config.clockreport = self.agenda_clockreport;
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
                    self.view = Some(OpenView { uri, file: None, view, client: None, shown: None });
                }
            }
            None => {
                // The client cannot fetch the view, so it gets a file.
                let directory = std::env::temp_dir().join(format!("org-lsp-{}", std::process::id()));
                let file = directory.join(views::FILE_NAME);
                if std::fs::create_dir_all(&directory).is_ok() {
                    if let Some(uri) = uri::from_path(&file) {
                        self.view = Some(OpenView { uri, file: Some(file), view, client: None, shown: None });
                    }
                }
            }
        }
    }

    /// Tell the client that the view has new text.
    fn publish_view(&mut self) {
        let Some(open) = &self.view else { return };
        match &open.file {
            Some(file) => {
                if let Err(error) = std::fs::write(file, &open.view.text) {
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
        if self.view.as_ref().is_some_and(OpenView::is_current) {
            let reveal = self.reveal.take();
            let position = reveal.zip(self.view.as_ref()).map(|(reveal, open)| reveal.position(&open.view));
            if let Some(position) = position {
                self.show_view(position);
            }
        }
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
            self.reveal_when_current();
        }
    }

    fn remove_view_file(&mut self) {
        if let Some(file) = self.view.as_ref().and_then(|open| open.file.as_ref()) {
            let _ = std::fs::remove_file(file);
            if let Some(directory) = file.parent() {
                let _ = std::fs::remove_dir(directory);
            }
        }
    }

    // ----- commands

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
            id if id.starts_with("org.calendar.") => self.calendar_command(id, context.as_ref()),
            // `org-save-all-org-buffers`: the server cannot save; the
            // editor saves the files it names that have changes.
            commands::AGENDA_SAVE => {
                let mut files: Vec<&String> = self.documents.keys().collect();
                files.sort();
                Ok(json!({ "save": files }))
            }
            commands::DATE_PREVIEW => self.preview_date(&context.ok_or("This command needs the cursor position")?),
            commands::PRIORITY => self.ask_priority(&context.ok_or("This command needs the cursor position")?),
            commands::REFILE => self.refile(&context.ok_or("This command needs the cursor position")?),
            // In a note buffer, the keys of a capture store or cancel the
            // note.
            commands::CAPTURE_FINISH | commands::CAPTURE_ABORT
                if self.note.as_ref().is_some_and(|note| {
                    context.as_ref().is_none_or(|context| context.text_document.uri == note.uri)
                }) =>
            {
                let note = self.note.take().expect("a note");
                self.finish_note(note, params.command == commands::CAPTURE_ABORT)
            }
            commands::CAPTURE_FINISH => self.finish_capture(),
            commands::CAPTURE_ABORT => {
                let capture = self.capture.take().ok_or("No capture is going on")?;
                self.message = Some("Capture process aborted and target buffer cleaned up".to_string());
                Ok(json!({ "close": capture.uri }))
            }
            commands::CAPTURE_REFILE => self.refile_capture(context.and_then(|context| context.input).as_deref()),
            id if id.starts_with("org.clock.") => self.clock_command(id, context.as_ref()),
            id if id == commands::TODO || actions::command(id).is_some() => self.run_edit(id, context),
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
        let pending = match (target, outcome.cursor) {
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
        Ok(self.open_view_at(query, Reveal::Block { block, day: Some(day) })?)
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
                Ok(self.open_view(Query::Agenda { start: self.start_day(), span: self.agenda.span })?)
            }
            commands::AGENDA_TODOS => Ok(self.open_view(Query::Todo)?),
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
            commands::AGENDA_LATER | commands::AGENDA_EARLIER | commands::AGENDA_TODAY => {
                let (block, first, span) = self.agenda_block(self.view_cursor_line(context.as_ref()))?;
                let steps = context.as_ref().and_then(|context| context.count).unwrap_or(1);
                let start = match command {
                    commands::AGENDA_LATER => Some(span.shift(first, steps)),
                    commands::AGENDA_EARLIER => Some(span.shift(first, -steps)),
                    _ => None,
                };
                Ok(self.open_view_at(self.moved(block, start, span), Reveal::Block { block, day: None })?)
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
                        (self.moved(block, Some(start), span), Reveal::Block { block, day })
                    }
                    Err(_) => (Query::Agenda { start: None, span }, Reveal::Home),
                };
                self.open_view_at(query, reveal)?;
                self.message = Some(format!("Switched to {} view", span_name(span)));
                Ok(())
            }
            commands::AGENDA_GOTO => {
                let reference = self.entry_at(context.as_ref()).ok_or("No entry on this line")?;
                let location = self.locate(&reference).ok_or("The entry is gone; rebuild the agenda")?;
                let params = ShowDocumentParams {
                    uri: location.uri,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(location.range),
                };
                self.send_request::<ShowDocument>(params, None);
                Ok(())
            }

            other => Err(format!("org-lsp has no command `{other}`").into()),
        }
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
            self.open_view(Query::Tags { matching, todo_only: prefix == "M" })?;
            return Ok(Value::Null);
        }
        let keys = format!("{prefix}{}", input.as_deref().unwrap_or(""));
        let commands = self.agenda_options.custom_commands.clone().unwrap_or_default();
        // The user's commands come first, as in Emacs.
        if custom::find(&commands, &keys).is_some() {
            let query = Query::Custom { keys, moved: None };
            self.blocks(&query)?;
            self.open_view(query)?;
            return Ok(Value::Null);
        }
        let built_in = [
            ("a", commands::title(commands::AGENDA_LIST)),
            ("t", commands::title(commands::AGENDA_TODOS)),
            ("m", "Match a TAGS/PROP/TODO query"),
            ("M", "Like m, but only TODO entries"),
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
            _ => return Err("No agenda command for this key".into()),
        }
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
                RefileFiles::Current => vec![uris[0].clone()],
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
    fn finish_note(&mut self, note: Noting, cancelled: bool) -> std::result::Result<Value, Refusal> {
        let written = match self.document(&note.uri) {
            Some(document) => document.text.clone(),
            None => uri::to_path(&note.uri).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default(),
        };
        // The comment lines at the top go, and blanks at the end.
        let mut text = written.as_str();
        while text.starts_with("# ") {
            text = text.split_once('\n').map_or("", |(_, rest)| rest);
            text = text.trim_start_matches([' ', '\t', '\n']);
        }
        let text = text.trim_end();
        let target = TodoTarget::State((!note.state.is_empty()).then(|| note.state.clone()));
        let extra = Extra { note: Some(text.to_string()), skip_note: cancelled, ..Extra::default() };
        // The change is the file's, as if made where it was asked for.
        self.origin = Some(note.context.text_document.uri.as_str().to_string());
        self.edit_with(&note.context, &note.id, Some(EditCommand::Todo(target)), &extra)?;
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

    /// The text of the capture buffer, as the editor has it.
    fn capture_text(&self, capture: &Capturing) -> String {
        match self.document(&capture.uri) {
            Some(document) => document.text.clone(),
            None => uri::to_path(&capture.uri).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default(),
        }
    }

    /// The target file once the capture buffer's text goes in: where the
    /// template was placed, or placed again when the file has changed.
    fn captured(&mut self, capture: &Capturing) -> std::result::Result<(String, Option<i32>, String), Refusal> {
        let edited = self.capture_text(capture);
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
    fn finish_capture(&mut self) -> std::result::Result<Value, Refusal> {
        let capture = self.capture.take().ok_or("No capture is going on")?;
        let (now, version, done) = self.captured(&capture)?;
        self.apply_capture(&capture.target, version, &now, &done);
        Ok(json!({ "close": capture.uri }))
    }

    /// `org-capture-refile` (`C-c C-w`): the entry goes to its target, and
    /// from there under a refile target.
    fn refile_capture(&mut self, answer: Option<&str>) -> std::result::Result<Value, Refusal> {
        let capture = self.capture.take().ok_or("No capture is going on")?;
        if capture.spec.1 != capture::Kind::Entry {
            self.capture = Some(capture);
            return Err("Refiling from a capture buffer makes only sense for `entry'-type templates".into());
        }
        let (_, _, done) = self.captured(&capture)?;
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
            return Ok(json!({ "input": { "kind": "priority", "prompt": question, "value": "" } }));
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
                commands::DATE_LATER | commands::DATE_EARLIER if !dated_view => {
                    return Err("The TODO list shows no dates to change".into());
                }
                commands::DATE_LATER | commands::DATE_EARLIER => {
                    EditCommand::AgendaDate { forward: id == commands::DATE_LATER }
                }
                _ if given.is_some() => given.clone().expect("given"),
                commands::TODO_NEXT | commands::TODO_PREVIOUS | commands::PRIORITY_UP | commands::PRIORITY_DOWN => {
                    actions::command(id).ok_or("Unknown command")?
                }
                _ => return Err(Refusal::NotHere("This command works in Org files".to_string())),
            };
            (reference.uri.clone(), Some(reference), command)
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
            };
            let explicit = matches!(command, EditCommand::Todo(TodoTarget::State(_)));
            let mut settings = settings.clone();
            settings.todo.adapt = display.adapt(text, adapt);
            let outcome = edit::run(&command, text, parse, offset, &call, &settings).map(|outcome| {
                let edits: Vec<TextEdit> = actions::edits_between(text, &outcome.text)
                    .into_iter()
                    .map(|edit| TextEdit { range: index.range(text, edit.range, encoding), new_text: edit.new_text })
                    .collect();
                let message = heading.and_then(|heading| {
                    headline_message(text, &outcome.text, heading, parse.todo_keywords(), reference.is_some(), explicit)
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

    /// An editing command, which may ask for something first. The answer
    /// to the note a TODO state change asked for comes with the state
    /// (`state` in the context); `org.todo` asks for the key of the new
    /// state when the keywords have keys (`org-todo` with fast selection),
    /// and cycles when they have none.
    fn run_edit(&mut self, id: &str, context: Option<Context>) -> std::result::Result<Value, Refusal> {
        let context = context.ok_or("This command needs the cursor position")?;
        if let Some(state) = &context.state {
            let target = TodoTarget::State((!state.is_empty()).then(|| state.clone()));
            let extra = Extra { note: Some(context.input.clone().unwrap_or_default()), ..Extra::default() };
            return self.edit_with(&context, id, Some(EditCommand::Todo(target)), &extra);
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

    /// `org-clock-in`, `org-clock-out`, `org-clock-cancel` and
    /// `org-clock-goto`, in an Org file or on an agenda line.
    fn clock_command(&mut self, id: &str, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        self.last = None;
        match id {
            commands::CLOCK_IN => self.clock_in(context.ok_or("This command needs the cursor position")?),
            commands::CLOCK_OUT | commands::CLOCK_CANCEL => {
                let running = self.running.clone().ok_or("No active clock")?;
                let Some((text, version, line)) = self.clock_line(&running) else {
                    self.running = None;
                    self.publish_clock();
                    return Err("Clock start time is gone".into());
                };
                let (new_text, message) = if id == commands::CLOCK_OUT {
                    let out = clock::clock_out(&text, line, self.date_time(), &self.clock_settings)?;
                    (out.text, out.message)
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
        let now = self.date_time();
        let mut changes: Vec<(Uri, Option<i32>, String, String)> = Vec::new();
        let mut heading = heading;
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
                let out = clock::clock_out(&text, line, now, &self.clock_settings)?;
                if running.uri == file && line < heading {
                    heading = heading + out.text.len() - text.len();
                }
                changes.push((running.uri.clone(), version, text, out.text));
            }
        }
        let mut settings = self.clock_settings.clone();
        let (display, adapt) = (self.display, self.indent.adapt);
        let base = changes.iter().find(|(uri, ..)| *uri == file).map(|(_, _, _, new)| new.clone());
        let found = self.with_source(&file, |text, _, parse, outline, version| {
            let base = base.clone().unwrap_or_else(|| text.to_string());
            settings.adapt = display.adapt(&base, adapt);
            let clocked = clock::clock_in(&base, parse.todo_keywords(), heading, &settings, now)?;
            let entry = &outline.entries[outline.entry_at(heading).ok_or("Before first headline")?];
            let repeat = entry.properties.iter().any(|(key, _)| key == "LAST_REPEAT");
            Ok::<_, String>((text.to_string(), version, clocked.0, entry.head.clone(), repeat))
        });
        let (old, version, new_text, head, repeat) = found.ok_or("Cannot read the file of this entry")??;
        match changes.iter_mut().find(|(uri, ..)| *uri == file) {
            Some(change) => change.3 = new_text,
            None => changes.push((file.clone(), version, old, new_text)),
        }
        self.send_texts(changes);
        self.running = Some(RunningClock { uri: file.clone(), start: now, head: head.clone() });
        self.last_clocked = Some((file, head));
        let shown = if repeat { "showing task time since last repeat." } else { "showing entire task time." };
        self.message =
            Some(format!("Clock starts at {} - {shown}", org_core::stamp::format_timestamp(now, true, true, "")));
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
        let outline = Outline::new(&parse, text, &uri::file_stem(file));
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
