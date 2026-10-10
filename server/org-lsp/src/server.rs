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
use org_core::link;
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

use crate::config::{self, ArchivesMode, CaptureTemplateOption, LogOption, RefileFiles, RefileTarget};
use crate::document::Document;
use crate::line_index::{Encoding, LineIndex};
use crate::prompt::{self, DatePrompt, Target, TimeField};

mod bulk;
mod journal;
mod roam;
use crate::views::{self, EntryRef, Query, View, ViewLine};
use crate::workspace::{Source, Workspace};
use crate::{actions, cli, commands, completion, custom, faces, folding, highlight, manifest, port, symbols, uri};

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
    /// Shows what stands in for hidden text (entities, sub- and
    /// superscripts, shrunk fields), which the server sends
    /// (`org-lsp/display`), rather than as inlay hints; it says so with
    /// `experimental.orgDisplay`.
    display: bool,
    /// Highlights lines of the agenda from edge to edge as the server
    /// says (`org-lsp/lineHighlights`): `experimental.orgLineHighlights`,
    /// or the option `line_highlights`.
    line_highlights: bool,
    /// Reads the agenda's file again when told (`org-lsp/viewChanged`):
    /// the option `view_changed`.
    view_changed: bool,
    /// Shows images in place of text (`org.link.preview`,
    /// `org.latex.preview`): `experimental.orgImages`, or the option
    /// `images`.
    images: bool,
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
    /// A yes-or-no question of Emacs's, whose answer it waits for.
    EmacsQuestion(crossbeam_channel::Sender<bool>),
    /// The document org-journal leaves the cursor in is shown: its
    /// headlines are folded as org-journal folds them (`org-lsp/folds`).
    Folds { uri: Uri, folds: Value },
    /// An edit after which `uri` is shown at `position` and folded so.
    EditAndShow { uri: Uri, position: Position, folds: Value },
    /// A question of `command`, run with `argument`, asked with
    /// `window/showMessageRequest` (`message_prompts`); the answer runs it
    /// again.
    Answer { command: String, argument: Value, input: Value },
}

/// What a request to Emacs was for.
enum EmacsJob {
    /// An export, by what the menu calls it.
    Export(String),
    /// A command that changes the document `uri`, run on `text`, its
    /// `version`: the text Emacs leaves becomes an edit, unless the
    /// document changed meanwhile.
    Edit { uri: Uri, version: i32, text: String, point: usize },
    /// A command of column view on the document `uri`, run on `text`, its
    /// `version`: its edit, and what the view shows then.
    Columns { uri: Uri, version: i32, text: String },
    /// What column view shows of the document `uri`, at its `version`.
    ColumnsView { uri: Uri, version: i32 },
    /// Column view of the agenda `uri`, for the view's text.
    AgendaColumns { uri: Uri, text: String },
    /// A column view command in the agenda `uri`: the files sent.
    AgendaColumnsCommand { uri: Uri, files: SentFiles },
    /// The text of the file `file` (`content`) written back into the part
    /// of `uri` it edits (see [`SpecialEdit`]), run on `text`, its `version`.
    WriteBack { file: PathBuf, uri: Uri, version: i32, text: String, content: String },
    /// The images of the LaTeX fragments of `uri` at its `version`, for
    /// the preview `preview` (`org-lsp/images`) without its items.
    LatexPreview { uri: Uri, version: i32, text: String, preview: Value },
}

/// The files sent to Emacs for a command in the agenda: each its URI, its
/// document's version when open, and its text.
type SentFiles = Vec<(Uri, Option<i32>, String)>;

/// Column view (`org-columns`) on a document: where it was turned on (a
/// character, from 1), and whether on the whole file.
#[derive(Clone)]
struct ColumnsOn {
    start: usize,
    global: bool,
    /// What it shows, as Emacs last said.
    view: Value,
}

/// A part of an Org file edited in a file of its own (`org-edit-special`):
/// what the file says goes back each time the editor saves it, as Org
/// writes its edit buffer back (`org-edit-src-save`), and the Org file is
/// saved.
struct SpecialEdit {
    /// The file the part is edited in.
    file: PathBuf,
    /// When it was last changed, as the server last saw.
    modified: Option<std::time::SystemTime>,
    /// What it said when it was last written back, or made.
    content: String,
    /// The Org file.
    uri: Uri,
    /// Where `org-edit-special` edits the part again (a character, from 1).
    point: usize,
    /// The part (characters, from 1), and its text.
    begin: usize,
    end: usize,
    region: String,
    /// Emacs is writing it back.
    busy: bool,
    /// What is edited, and so how it is written back.
    kind: SpecialKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecialKind {
    /// What `org-edit-special` edits with `org-src`, written back by
    /// `org-edit-src-save`.
    Element,
    /// The formulas of a table (`org-table-edit-formulas`), installed
    /// (`org-table-fedit-finish`).
    Formulas,
    /// A field of a table (`org-table-edit-field`), put back
    /// (`org-table-finish-edit-field`).
    Field,
    /// A subtree or a block, narrowed to: put back as it is.
    Region,
}

/// A part of an Org file to edit in a file of its own (see
/// [`Server::open_special`]).
struct NewSpecial {
    uri: Uri,
    /// The Org file's.
    stem: String,
    /// The part (characters, from 1), its text, and the file's text.
    begin: usize,
    end: usize,
    region: String,
    content: String,
    extension: String,
    /// What Emacs calls the buffer it edits it in.
    name: String,
    kind: SpecialKind,
    point: usize,
}

/// Text that goes in place of `start..end`.
type Replacement = (usize, usize, String);

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

/// `org-agenda-dim-blocked-tasks`: what the agenda does with the tasks
/// their dependencies block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DimBlocked {
    No,
    Dim,
    /// `invisible`: leaves them out, unless checkboxes block them.
    Hide,
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
    /// The selection the command was run on, Emacs's active region: the
    /// cursor is at one of its ends, the mark at the other.
    range: Option<Range>,
    /// In column view: the column picked, from 0.
    column: Option<usize>,
    /// For the commands that read them (`"folds": true` in the manifest):
    /// the closed folds of the editor's window, as their first and last
    /// lines, from 0.
    folded: Option<Vec<[u32; 2]>>,
}

/// The Emacs command a command is on a selection, the active region, for
/// those whose work Org changes there and org-lsp leaves to Emacs.
fn region_command(id: &str) -> Option<&'static str> {
    Some(match id {
        commands::META_LEFT => "org-metaleft",
        commands::META_RIGHT => "org-metaright",
        commands::META_UP => "org-metaup",
        commands::META_DOWN => "org-metadown",
        commands::SHIFT_META_LEFT => "org-shiftmetaleft",
        commands::SHIFT_META_RIGHT => "org-shiftmetaright",
        commands::CHECKBOX_TOGGLE => "org-toggle-checkbox",
        commands::TABLE_CREATE => "org-table-create-or-convert-from-region",
        _ => return None,
    })
}

/// The target a refile command was given: `input`, or after "Create new
/// node …?" the target it asked about (in `answers`), a NUL and the
/// answer.
fn refile_answer(context: &Context) -> Option<String> {
    match (&context.answers, &context.input) {
        (Some(target), Some(yes)) => Some(format!("{target}\0{yes}")),
        _ => context.input.clone(),
    }
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
    /// What the editor would paste (`%c`, `%x`, `%^C`).
    clipboard: Option<String>,
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
    /// `:clock-in` and its relatives.
    clock: Option<CaptureClock>,
    /// `:jump-to-captured`: the target shows the entry when it is filed.
    jump: bool,
    /// `:unnarrowed`: the buffer shows the whole target file.
    unnarrowed: bool,
    /// A capture of org-roam's, and what it does once filed.
    roam: Option<roam::Captured>,
}

/// `:clock-in`: the captured entry is clocked from the start of the
/// capture; `:clock-keep` leaves the clock running when it is filed, and
/// `:clock-resume` starts the clock it stopped again.
#[derive(Debug, Clone)]
struct CaptureClock {
    start: DateTime,
    keep: bool,
    resume: bool,
    /// The entry whose clock the capture stopped.
    interrupted: Option<(Uri, String)>,
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
    /// What the note logs, besides a state change.
    log: Option<NoteLog>,
}

/// What refiling does with the entry: `org-refile`, `org-refile-copy`, or
/// `org-refile` with `C-u`, which goes to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefileMode {
    Move,
    Copy,
    Goto,
}

/// What a note the server asks for logs, besides a state change.
#[derive(Debug, Clone)]
enum NoteLog {
    /// A date changed or removed, and the old date.
    Planning(org_core::todo::PlanningChange, String),
    /// The entry refiled (`org-log-refile`).
    Refile,
}

/// The relative timer or the countdown of Org in Emacs, for the editor's
/// status line as Emacs's mode line shows it: `{ "seconds", "countdown",
/// "paused" }` when it was sent, or null when none runs.
pub const TIMER: &str = "org-lsp/timer";
/// The notification that asks the editor to save files, `{ "uris": [...] }`.
pub const SAVE: &str = "org-lsp/save";
/// The headlines' folds to open and close in a document org-journal shows.
pub const FOLDS: &str = "org-lsp/folds";
/// From the client: how many columns of text the window that shows the
/// agenda has, `{ "width": N }`, for `org_agenda_tags_column` `"auto"`.
pub const AGENDA_WIDTH: &str = "org-lsp/agendaWidth";
/// The entry of an agenda line with its highlighting, for a popup.
pub const PREVIEW: &str = "org-lsp/preview";
/// The agenda's file has been written anew, `{ "uri" }`: for a client
/// with the option `view_changed`, which reads it again.
pub const VIEW_CHANGED: &str = "org-lsp/viewChanged";
/// The faces of particular TODO keywords and priorities
/// (`org_todo_keyword_faces`, `org_priority_faces`), for the editor to
/// apply over the highlighting: `{ "todo": { KEYWORD: face }, "priority":
/// { LETTER: face } }`, each face `{ "foreground", "background", "bold",
/// "italic", "underline", "strikethrough" }` as set.
pub const FACES: &str = "org-lsp/faces";
/// The journal's directory, as `capabilities.experimental.orgJournal` has
/// it, for clients that cannot read the `initialize` result.
pub const JOURNAL: &str = "org-lsp/journal";
/// Images to show in place of text, or to take away: what
/// `org.latex.preview` made, once Emacs has made it. `{ "kind": "latex",
/// "uri", "scope", "range", "remove", "items": [{ "range", "path" }] }`, as
/// the preview commands answer (see [`Server::preview_command`]).
pub const PREVIEW_IMAGES: &str = "org-lsp/images";
/// What column view shows on a document, for the editor to draw over its
/// headlines: `{ "uri", "version", "header", "columns": [{ "key", "title",
/// "width" }], "lines": [[line, text, [value…]]] }`, each line one of a
/// headline with the text it shows and its values; `lines` null when
/// column view is off.
pub const COLUMNS_SHOWN: &str = "org-lsp/columns";
/// The folds of a file closed when it opens (`org-startup-folded`, its
/// `#+STARTUP:`, `VISIBILITY` properties), innermost first: `{ "folds":
/// [FoldingRange…] }`. Params: `{ "textDocument" }`.
pub const STARTUP_FOLDS: &str = "org-lsp/startupFolds";
/// From the client: the cursor moved to another line of the agenda, `{
/// "textDocument", "position" }`. In follow mode (`F`) the entry there
/// shows in another window.
pub const AGENDA_CURSOR: &str = "org-lsp/agendaCursor";
/// Moves the fixed clock of a server started with one (the `clock`
/// option), for tests: `{ "today": "2026-05-18", "now": "10:30" }`.
pub const SET_CLOCK: &str = "org-lsp/setClock";
/// From the client: the cursor moved to another line of an Org file, `{
/// "textDocument", "position" }`, while the org-roam buffer shows: it
/// follows the node there.
pub const CURSOR: &str = "org-lsp/cursor";

/// The name of a capture buffer starts with this, as Emacs names it.
pub const CAPTURE_PREFIX: &str = "CAPTURE-";

/// The name of a file of the org-roam buffer starts with this.
pub const ROAM_BUFFER_PREFIX: &str = roam::buffer::FILE_PREFIX;

/// Where a capture template goes, before the server finds it.
enum TargetSpec {
    /// In the target file.
    File(capture::Target),
    /// `(clock)`: under the entry clocked now.
    Clock,
    /// `(id …)`: under the entry with this ID.
    Id(String),
    /// `(file+olp+datetree …)`: the date tree under this outline path.
    DateTree(Vec<String>, Vec<capture::Group>),
    /// `(function …)` and `(file+function …)`: where the function, run by
    /// Emacs, leaves the point, in the file it leaves it in.
    Function { file: bool, name: String },
}

/// The kind and target of a capture template.
fn capture_spec(template: &CaptureTemplateOption) -> std::result::Result<(capture::Kind, TargetSpec), Refusal> {
    let kind = match template.kind.as_deref() {
        None | Some("entry") => capture::Kind::Entry,
        Some("plain") => capture::Kind::Plain,
        Some("item") => capture::Kind::Item,
        Some("checkitem") => capture::Kind::CheckItem,
        Some("table-line") => capture::Kind::TableLine,
        Some(other) => return Err(format!("Capture templates of type `{other}` are not supported").into()),
    };
    let rest = || template.target.get(2..).unwrap_or_default().to_vec();
    let target = match template.target.first().map(String::as_str) {
        Some("file") => TargetSpec::File(capture::Target::File),
        Some("file+headline") => {
            TargetSpec::File(capture::Target::Headline(template.target.get(2).cloned().unwrap_or_default()))
        }
        Some("file+olp") => TargetSpec::File(capture::Target::Path(rest())),
        Some("file+regexp") => {
            TargetSpec::File(capture::Target::Regexp(template.target.get(2).cloned().unwrap_or_default()))
        }
        Some("file+olp+datetree") => {
            let tree = template.tree_type.as_ref().map(|tree| tree.names()).unwrap_or_default();
            TargetSpec::DateTree(rest(), capture::Group::parse(&tree)?)
        }
        Some("function") => {
            TargetSpec::Function { file: false, name: template.target.get(1).cloned().unwrap_or_default() }
        }
        Some("file+function") => {
            TargetSpec::Function { file: true, name: template.target.get(2).cloned().unwrap_or_default() }
        }
        Some("clock") => TargetSpec::Clock,
        Some("id") => TargetSpec::Id(template.target.get(1).cloned().unwrap_or_default()),
        Some(other) => return Err(format!("Invalid capture target specification: {other}").into()),
        // No target: the end of `org_default_notes_file`, as `["file", ""]`.
        None => TargetSpec::File(capture::Target::File),
    };
    Ok((kind, target))
}

/// The templates and groups of templates one key further than `keys`, as
/// the choices of `org-mks`.
fn template_choices(templates: &[CaptureTemplateOption], keys: &str) -> Vec<Value> {
    templates
        .iter()
        .filter(|template| template.key.starts_with(keys) && template.key.chars().count() == keys.chars().count() + 1)
        .map(|template| {
            let key = template.key.chars().last().map(String::from).unwrap_or_default();
            let label = if !template.is_group() {
                template.description.clone()
            } else {
                format!("{}...", template.description)
            };
            json!({ "key": key, "label": label })
        })
        .collect()
}

/// The template of a capture template, or the default of its type when
/// it has none (`org-capture-set-plist`).
fn template_text(template: &CaptureTemplateOption) -> String {
    let text = template.template.as_ref().map(|text| text.text()).unwrap_or_default();
    if !text.trim().is_empty() {
        return text;
    }
    match template.kind.as_deref() {
        Some("item") => "- %?",
        Some("checkitem") => "- [ ] %?",
        Some("table-line") => "| %? |",
        Some("plain") => "",
        _ => "* %?\n  %a",
    }
    .to_string()
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
    /// Where the command port (`port`) sends what it is sent, and the
    /// port: open from the start with `command_actions`.
    port_sender: crossbeam_channel::Sender<port::Command>,
    port: Option<port::Port>,
    /// How the port's script starts a server when none answers: the Org
    /// file last opened, with the editor's command line.
    port_start: port::Start,
    /// Where the cursor was last seen in the agenda, for the port's
    /// commands that do not say: from the requests the client makes as
    /// the cursor moves, and from where the server put it.
    agenda_cursor: Option<Position>,
    /// Where the cursor was last seen in an Org file or a capture buffer,
    /// for the port's commands of Org files (`file:`, `key:`).
    file_cursor: Option<(Uri, Position)>,
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
    /// How many months the date prompt's calendar shows (`calendar_months`).
    calendar_months: usize,
    /// The day the calendar's weeks start on (`calendar_week_start_day`).
    calendar_week_start: usize,
    auto_save: bool,
    /// What the highlighting marks as hidden.
    conceal: highlight::Conceal,
    /// `org-pretty-entities`
    pretty_entities: bool,
    /// `org-startup-with-link-previews`.
    startup_link_previews: bool,
    /// `org-startup-with-latex-preview`
    startup_latex_preview: bool,
    /// `org-table-formula-debug`: recalculating goes through Emacs, which
    /// asks after each formula.
    formula_debug: bool,
    /// Sub- and superscripts shown where entities are
    /// (`org-pretty-entities-include-sub-superscripts`), and whether they
    /// need braces (`org-use-sub-superscripts` `{}`).
    pretty_scripts: Option<bool>,
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
    /// `org-capture-templates-contexts`
    capture_contexts: Vec<org_core::contexts::Context>,
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
    /// Emacs, for export (`emacs`); it starts when first asked.
    emacs: crate::emacs::Emacs,
    /// The export menu, the publishing projects and the structure
    /// templates, once Emacs gave them: its configuration does not change
    /// while it runs.
    export_menu: Option<Value>,
    structure_templates: Option<Value>,
    /// When the countdown Org runs in Emacs (`org-timer-set-timer`) ends:
    /// Emacs in batch runs no timers, so the server says it.
    countdown: Option<std::time::Instant>,
    /// The relative timer or the countdown, as the editor was last told
    /// (`org-lsp/timer`).
    timer: Option<Value>,
    /// The parts of Org files edited in files of their own, and where
    /// those files are.
    special_edits: Vec<SpecialEdit>,
    special_directory: PathBuf,
    /// Narrowed files widened and asked closed: removed again when the
    /// editor closes them, as some save a file before closing it.
    widened: Vec<PathBuf>,
    /// Twice a second, while there are some: whether their files were
    /// saved.
    special_tick: crossbeam_channel::Receiver<std::time::Instant>,
    /// When `appt.el` in Emacs checks the appointments next, while it has
    /// some: each minute, as its timer would.
    appt_check: Option<std::time::Instant>,
    /// A minute of those checks; shorter in the tests.
    appt_minute: std::time::Duration,
    /// When the clock passes the effort of its entry
    /// (`org-clock-notify-once-if-expired`), and what Org says then. Once
    /// for each clock.
    overrun: Option<(std::time::Instant, String)>,
    /// The clock (its start) whose overrun was said.
    overrun_said: Option<DateTime>,
    /// What the `%(sexp)` of the capture being filled evaluated to.
    capture_lisp: HashMap<String, Option<String>>,
    publish_projects: Option<Value>,
    /// What the exports made, the last last (`org-export-stack`): what
    /// the menu called each, and the file.
    /// An export to a buffer always shows in the editor.
    export_stack: Vec<(String, PathBuf, bool)>,
    /// What Emacs is doing, by tag.
    emacs_jobs: HashMap<u64, EmacsJob>,
    /// Where exports to a buffer are written to be shown.
    export_directory: PathBuf,
    next_export: u64,
    /// Where the agenda's file goes, for a client that cannot fetch it.
    view_directory: PathBuf,
    /// The date prompt's document: [`prompt::URI`], or the file it is
    /// written to for a client that cannot fetch it (`prompt_file`).
    prompt_uri: Uri,
    prompt_file: Option<PathBuf>,
    /// The date prompt that waits for an answer, in clients that show it.
    date_prompt: Option<DatePrompt>,
    /// The client's copy of the date prompt document, which it may keep
    /// after a prompt is done.
    prompt_copy: Option<Document>,
    /// `org-cycle-separator-lines`
    separator_lines: i64,
    /// How files show when they are opened.
    startup: folding::Startup,
    /// The documents column view shows on, by URI.
    columns: HashMap<String, ColumnsOn>,
    /// The shrunk columns of each table of a document (from 1), by URI,
    /// the tables in the order of the text.
    shrunk: HashMap<String, Vec<std::collections::BTreeSet<usize>>>,
    /// `org-startup-shrink-all-tables`
    startup_shrink: bool,
    /// Whether Emacs encrypts entries before a file is saved
    /// (`org-crypt-use-before-save-magic`), once it said.
    crypt_before_save: Option<bool>,
    /// `org-adapt-indentation` and its relatives.
    indent: org_core::indent::Settings,
    /// `org-startup-indented` and `org-hide-leading-stars`.
    display: org_core::indent::Display,
    /// The documents the client was last told to show with prefixes.
    virtually_indented: HashSet<String>,
    /// The documents last sent with something in `org-lsp/display`.
    displayed: HashSet<String>,
    /// `org-cite-global-bibliography`
    cite_global: Vec<String>,
    /// `org-link-abbrev-alist`.
    link_abbreviations: Vec<(String, String)>,
    /// `org-agenda-dim-blocked-tasks`.
    /// `org-agenda-dim-blocked-tasks`
    dim_blocked: DimBlocked,
    /// The entry the last refile put somewhere: its file, where its
    /// headline was, and the headline.
    last_refiled: Option<(Uri, usize, String)>,
    /// The bibliography files read, with when each changed and its size,
    /// to read one again when it changes.
    bibliographies: HashMap<PathBuf, (Option<std::time::SystemTime>, u64, Rc<BibFile>)>,
    /// The documents the server published citation diagnostics for.
    /// The documents with diagnostics published, which a change without
    /// any clears.
    diagnosed: HashSet<String>,
    /// The `ID`s of the files on disk that IDs are looked for in, as of
    /// their time of change (`org-id-locations`).
    id_files: HashMap<PathBuf, (std::time::SystemTime, Vec<String>)>,
    /// `org_lint`: Org's linter checks the documents.
    lint: bool,
    /// `org_lint_ignore`: the checkers it leaves out.
    lint_ignore: Vec<String>,
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
    /// `org-stored-links`: the links `org.link.store` keeps for
    /// `org.link.insert`, the last first.
    stored_links: org_core::link::Stored,
    /// The options of the link commands.
    link_settings: org_core::link::Settings,
    /// `org-clock-default-task`, marked by clocking in with `C-u C-u`.
    clock_default_task: Option<(Uri, String)>,
    /// `org-clock-interrupted-task`: the entry whose clock the last
    /// clock-in stopped.
    clock_interrupted: Option<(Uri, String)>,
    /// `org-clock-out-time`: when the last clock stopped.
    clock_out_time: Option<DateTime>,
    /// `org-clock-idle-time`: after how many minutes without a message
    /// from the editor a clock that runs is a question.
    clock_idle_time: Option<i64>,
    /// When the editor last sent something, in minutes.
    last_activity: Option<i64>,
    /// When the idle time asked about began.
    idle_since: Option<i64>,
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
    /// org-roam, when it is on.
    roam: Option<roam::Roam>,
    /// org-journal, when it is on.
    journal: Option<crate::config::JournalSettings>,
    /// What a journal command that asked for a date in the calendar goes
    /// on with: its id and its context so far.
    journal_state: Option<(String, Value)>,
    /// The directory of the journal's search results and schedule.
    journal_views: PathBuf,
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
    /// `org-refile-allow-creating-parent-nodes`
    refile_new_nodes: refile::NewNodes,
    /// `org_refile_target_verify_function`
    refile_verify: Option<String>,
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
    /// The sequence an entry was in when its keyword was cleared, by file
    /// and headline offset: where cycling starts again (the
    /// `org-todo-head` text property Emacs puts on the line).
    todo_heads: HashMap<String, HashMap<usize, String>>,
    /// See [`FACES`].
    faces: Value,
    /// The last link search found nothing (`org-link--search-failed`).
    link_search_failed: bool,
    /// `org_goto_max_level`
    goto_max_level: usize,
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
            == Some(true)
            || options.date_prompt == Some(true),
        virtual_indent: raw_params.pointer("/capabilities/experimental/orgVirtualIndent").and_then(Value::as_bool)
            == Some(true)
            || options.virtual_indent == Some(true),
        display: raw_params.pointer("/capabilities/experimental/orgDisplay").and_then(Value::as_bool) == Some(true),
        images: raw_params.pointer("/capabilities/experimental/orgImages").and_then(Value::as_bool) == Some(true)
            || options.images == Some(true),
        view_changed: options.view_changed == Some(true),
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
    // The prompt is a document the client fetches, or a file it reads
    // again when told.
    let shown = client.virtual_documents || client.view_changed;
    let client = Client { date_prompt: client.date_prompt && shown && client.show_document, ..client };

    let todo = options.todo_keywords();
    let faces = faces::faces(&options.todo_keyword_faces, &options.priority_faces);

    #[allow(deprecated)]
    let folders: Vec<PathBuf> = match &params.workspace_folders {
        Some(folders) => folders.iter().filter_map(|folder| uri::to_path(&folder.uri)).collect(),
        None => params.root_uri.as_ref().and_then(uri::to_path).into_iter().collect(),
    };

    let view_directory = agenda_directory(&client, &folders).unwrap_or_else(|| server_directory("view"));
    let prompt_file = (!client.virtual_documents).then(|| view_directory.join(prompt::FILE_NAME));
    let prompt_uri =
        prompt_file.as_deref().and_then(uri::from_path).unwrap_or_else(|| prompt::URI.parse().expect("valid URI"));

    let result = InitializeResult {
        capabilities: capabilities(client.encoding, true, client.command_actions, &faces, options.journal().as_ref()),
        server_info: Some(ServerInfo {
            name: "org-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    let mut result = serde_json::to_value(result)?;
    // The scheme of the agenda document, for clients that fetch it.
    result["capabilities"]["workspace"] = json!({ "textDocumentContent": { "schemes": [views::SCHEME] } });
    connection.initialize_finish(id, result)?;

    let emacs = crate::emacs::Emacs::new(crate::emacs::Settings {
        program: options.emacs.clone().unwrap_or_default(),
        config_file: options.emacs_config_file.clone().filter(|file| !file.is_empty()),
        config: options.emacs_config.clone().filter(|config| !config.trim().is_empty()),
    });
    let mut agenda = options.agenda_config();
    // The diary sexps the agenda cannot read, read by Emacs
    // (`org-diary-sexp-entry`).
    let caller = emacs.caller();
    agenda.diary = org_core::agenda::diary::Outside(Some(std::sync::Arc::new(move |sexp, entry, day| {
        let date = org_core::date::Date::from_day(day);
        let params = json!({ "sexp": sexp, "entry": entry, "date": [date.month, date.day, date.year] });
        let result = caller.call("diary-sexp", params, std::time::Duration::from_secs(30)).ok()?;
        let lines: Vec<String> =
            result.as_array()?.iter().filter_map(|line| line.as_str().map(str::to_string)).collect();
        (!lines.is_empty()).then_some(lines)
    })));
    let mut server = Server {
        client,
        sender: connection.sender.clone(),
        port_sender,
        port: None,
        port_start: port::Start::default(),
        agenda_cursor: None,
        file_cursor: None,
        next_request: 0,
        pending: HashMap::new(),
        message: None,
        kill: None,
        origin: None,
        auto_save: options.auto_save.unwrap_or(true),
        return_uses_meta_return: options.return_uses_meta_return.unwrap_or(false),
        calendar_months: options.calendar_months.unwrap_or(3),
        calendar_week_start: options.calendar_week_start_day.unwrap_or(0) % 7,
        all_agenda_tags: options.complete_tags_always_offer_all_agenda_tags.unwrap_or(false),
        pretty_entities: options.pretty_entities.unwrap_or(false),
        startup_link_previews: options.startup_with_link_previews.unwrap_or(false),
        startup_latex_preview: options.startup_with_latex_preview.unwrap_or(false),
        formula_debug: false,
        pretty_scripts: match (
            options.pretty_entities_include_sub_superscripts.unwrap_or(true),
            &options.use_sub_superscripts,
        ) {
            (false, _) | (_, Some(LogOption::Flag(false))) => None,
            (_, Some(LogOption::Name(name))) if name == "{}" => Some(true),
            _ => Some(false),
        },
        conceal: highlight::Conceal {
            links: options.link_descriptive.unwrap_or(true),
            emphasis: options.hide_emphasis_markers.unwrap_or(false),
        },
        saving: HashMap::new(),
        todo,
        agenda,
        agenda_options: options.agenda.clone(),
        edit_settings: options.edit_settings(),
        read_date: options.read_date_settings(),
        refile: (options.refile_targets(), options.refile_outline_path(), options.refile_settings()),
        archive: (options.archive_settings(), options.archive_default_command().to_string()),
        capture_templates: options.capture_templates(),
        capture_contexts: config::contexts(options.capture_templates_contexts.as_deref()),
        org_directory: options.org_directory(),
        default_notes_file: options.default_notes_file(),
        capture: None,
        note: None,
        captures: 0,
        capture_directory: server_directory("capture"),
        emacs,
        export_menu: None,
        structure_templates: None,
        countdown: None,
        timer: None,
        appt_check: None,
        special_edits: Vec::new(),
        widened: Vec::new(),
        special_directory: server_directory("edit"),
        special_tick: crossbeam_channel::tick(std::time::Duration::from_millis(500)),
        appt_minute: std::time::Duration::from_secs_f64(
            options.clock.as_ref().and_then(|clock| clock.minute).unwrap_or(60.0),
        ),
        overrun: None,
        overrun_said: None,
        capture_lisp: HashMap::new(),
        publish_projects: None,
        emacs_jobs: HashMap::new(),
        export_stack: Vec::new(),
        export_directory: server_directory("export"),
        next_export: 0,
        view_directory,
        prompt_uri,
        prompt_file,
        date_prompt: None,
        prompt_copy: None,
        separator_lines: options.separator_lines(),
        startup: options.startup(),
        columns: HashMap::new(),
        shrunk: HashMap::new(),
        startup_shrink: options.startup_shrink_all_tables.unwrap_or(false),
        crypt_before_save: None,
        indent: options.indent_settings(),
        display: options.display(),
        virtually_indented: HashSet::new(),
        displayed: HashSet::new(),
        cite_global: options.cite_global_bibliography.clone().unwrap_or_default(),
        dim_blocked: match &options.agenda_dim_blocked_tasks {
            Some(LogOption::Flag(false)) => DimBlocked::No,
            Some(LogOption::Name(name)) if name == "invisible" => DimBlocked::Hide,
            _ => DimBlocked::Dim,
        },
        last_refiled: None,
        link_abbreviations: options.link_abbrev_alist.clone().unwrap_or_default().into_iter().collect(),
        bibliographies: HashMap::new(),
        diagnosed: HashSet::new(),
        id_files: HashMap::new(),
        lint: options.lint.unwrap_or(true),
        lint_ignore: options.lint_ignore.clone().unwrap_or_default(),
        clock: options.clock.as_ref().and_then(|clock| clock.to_now()),
        running: None,
        clock_pending: false,
        last_clocked: None,
        clock_history: Vec::new(),
        clock_history_length: options.clock_history_length.unwrap_or(5).max(1),
        stored_links: Vec::new(),
        link_settings: options.link_settings(),
        clock_default_task: None,
        clock_interrupted: None,
        clock_out_time: None,
        clock_idle_time: options.clock_idle_time.filter(|minutes| *minutes > 0),
        last_activity: None,
        idle_since: None,
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
        roam: options.roam().map(roam::Roam::new),
        journal: options.journal(),
        journal_state: None,
        journal_views: server_directory("journal"),
        clock_display: HashMap::new(),
        clock_display_range: options.clock_display_default_range.clone().unwrap_or_else(|| "thisyear".to_string()),
        refile_current: None,
        refile_new_nodes: options.refile_new_nodes(),
        refile_verify: options.refile_target_verify_function.clone().filter(|name| !name.trim().is_empty()),
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
        todo_heads: HashMap::new(),
        faces,
        link_search_failed: false,
        goto_max_level: options.goto_max_level.unwrap_or(5),
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
    // The port answers from the start: its script opens the agenda
    // without one open (`org.agenda.list`).
    server.open_port();
    let emacs_events = server.emacs.events().clone();
    loop {
        let alarm = server.countdown.map_or_else(crossbeam_channel::never, crossbeam_channel::at);
        let appt = server.appt_check.map_or_else(crossbeam_channel::never, crossbeam_channel::at);
        let special =
            if server.special_edits.is_empty() { crossbeam_channel::never() } else { server.special_tick.clone() };
        let overrun =
            server.overrun.as_ref().map_or_else(crossbeam_channel::never, |(at, _)| crossbeam_channel::at(*at));
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
            recv(emacs_events) -> event => {
                if let Ok(event) = event {
                    server.emacs_event(event);
                }
            }
            recv(alarm) -> _ => server.countdown_done(),
            recv(overrun) -> _ => server.overrun_done(),
            recv(appt) -> _ => server.appt_check(),
            recv(special) -> _ => server.check_special_edits(),
        }
    }
    if let Some(port) = &server.port {
        port.close();
    }
    // Zed opens the agenda's tab again when it starts; the file stays for
    // it, and the next server takes it over.
    if !server.client.command_actions {
        server.remove_view_file();
    }
    if server.captures > 0 {
        let _ = std::fs::remove_dir_all(&server.capture_directory);
    }
    let _ = std::fs::remove_dir_all(&server.export_directory);
    if !server.special_edits.is_empty() {
        let _ = std::fs::remove_dir_all(&server.special_directory);
    }
    Ok(())
}

/// What `%[name]` puts in a capture: the file's text, or Org's note that
/// it could not read it. `~/` is the home directory; a relative name is
/// relative to `directory`.
fn capture_file_text(name: &str, directory: Option<&Path>) -> String {
    let path = match name.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
        None if Path::new(name).is_absolute() => PathBuf::from(name),
        None => directory.map(Path::to_path_buf).unwrap_or_default().join(name),
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
            "%![could not insert {0}: (file-missing Opening input file No such file or directory {0})]",
            path.display()
        ),
        Err(error) => format!("%![could not insert {}: (file-error {error})]", path.display()),
    }
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
        Some(key) if key.len() == 1 && key.as_bytes()[0] < 32 => {
            format!("{label} [C-{}]", char::from(key.as_bytes()[0] + 96))
        }
        Some(key) if key != label && key.chars().count() == 1 => format!("{label} [{key}]"),
        _ => label.to_string(),
    }
}

/// The words of a file's `#+STARTUP:` lines, lowercased, in order.
fn startup_words(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches([' ', '\t']);
            line.get(..10).filter(|key| key.eq_ignore_ascii_case("#+startup:")).map(|_| &line[10..])
        })
        .flat_map(|words| words.split_whitespace().map(str::to_ascii_lowercase))
        .collect()
}

/// What a column view command asks, Org's question with its default, as
/// one line of text: for `id` on the `column`th column of `line` in what
/// the view shows.
fn columns_question(id: &str, view: &Value, line: usize, column: usize) -> Option<(String, String)> {
    // The column's key, and its value on the line.
    let key = view["columns"][column]["key"].as_str().unwrap_or_default().to_string();
    let value = view["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row[0].as_u64() == Some(line as u64))
        .and_then(|row| row[2][column].as_str())
        .unwrap_or_default()
        .to_string();
    match id {
        commands::COLUMNS_EDIT => match key.as_str() {
            "ITEM" => Some(("Edit: ".to_string(), value)),
            "PRIORITY" => Some(("Priority A-C, SPC to remove: ".to_string(), String::new())),
            "TAGS" => Some(("Tags: ".to_string(), value)),
            "DEADLINE" | "SCHEDULED" => Some(("Date+time: ".to_string(), String::new())),
            "TODO" | "CLOCKSUM" | "CLOCKSUM_T" => None,
            _ => {
                let current = if value.is_empty() { String::new() } else { format!(" [{value}]") };
                Some((format!("{key} value{current}: "), value))
            }
        },
        commands::COLUMNS_ALLOWED => Some(("Allowed: ".to_string(), String::new())),
        _ => None,
    }
}

/// The question of `org.export.dispatch` at `step` (none for the first
/// key, a backend's key, or `P`), as Emacs's dispatcher shows it: the
/// options with their state, then the backends or the entries of one.
fn export_question(menu: &Value, step: Option<&str>, options: &str) -> Value {
    let on = |letter: char| options.contains(letter);
    let mut choices = vec![
        json!({ "key": "\u{2}", "label": format!("Body only: {}", if on('b') { "On" } else { "Off" }) }),
        json!({ "key": "\u{13}", "label": format!("Export scope: {}", if on('s') { "Subtree" } else { "Buffer" }) }),
        json!({ "key": "\u{6}", "label": format!("Force publishing: {}", if on('f') { "On" } else { "Off" }) }),
        json!({ "key": "\u{16}", "label": format!("Visible only: {}", if on('v') { "On" } else { "Off" }) }),
    ];
    let tops = menu.as_array().cloned().unwrap_or_default();
    match step {
        None => {
            for top in &tops {
                choices.push(json!({ "key": top.get("key"), "label": top.get("label") }));
            }
            choices.push(json!({ "key": "#", "label": "Insert template" }));
            choices.push(json!({ "key": "&", "label": "Export stack" }));
            choices.push(json!({ "key": "P", "label": "Publish" }));
            choices.push(json!({ "key": "q", "label": "Exit" }));
        }
        Some("P") => {
            for (key, label) in
                [("f", "Current file"), ("p", "Current project"), ("x", "Choose project"), ("a", "All projects")]
            {
                choices.push(json!({ "key": key, "label": label }));
            }
            choices.push(json!({ "key": "q", "label": "Main menu" }));
        }
        Some(first) => {
            let top = tops.iter().find(|top| top.get("key").and_then(Value::as_str) == Some(first));
            for entry in top.and_then(|top| top.get("entries")?.as_array()).into_iter().flatten() {
                choices.push(json!({ "key": entry.get("key"), "label": entry.get("label") }));
            }
            choices.push(json!({ "key": "q", "label": "Main menu" }));
        }
    }
    let prompt = match step {
        None => "Export command".to_string(),
        Some("P") => "Publish".to_string(),
        Some(first) => tops
            .iter()
            .find(|top| top.get("key").and_then(Value::as_str) == Some(first))
            .and_then(|top| top.get("label")?.as_str())
            .unwrap_or("Export command")
            .to_string(),
    };
    // The main menu's step is empty, so that it replaces the one before.
    let context = json!({ "answers": options, "step": step.unwrap_or("") });
    json!({ "input": { "kind": "key", "prompt": prompt, "value": "", "choices": choices, "context": context } })
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
/// The current section of the cursor at byte `offset`, as Org takes it
/// for previews: from its headline (or the start, before the first) to
/// the next headline of any level (`org-entry-end-position`).
fn section_bounds(text: &str, offset: usize) -> (usize, usize) {
    let is_heading = |line: &str| {
        let stars = line.bytes().take_while(|byte| *byte == b'*').count();
        stars > 0 && line[stars..].starts_with([' ', '\t'])
    };
    let mut start = 0;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if at > offset {
            break;
        }
        if is_heading(line) {
            start = at;
        }
        at += line.len();
    }
    let mut end = text.len();
    let mut at = start;
    for (index, line) in text[start..].split_inclusive('\n').enumerate() {
        if index > 0 && is_heading(line) {
            end = at;
            break;
        }
        at += line.len();
    }
    (start, end)
}

/// The TODO keyword of the headline at byte `heading` of `text`.
fn headline_keyword(text: &str, heading: usize, keywords: &TodoKeywords) -> Option<String> {
    let line = text.get(heading..)?.lines().next()?;
    let word = line.trim_start_matches('*').split_whitespace().next()?;
    (line.starts_with('*') && keywords.contains(word)).then(|| word.to_string())
}

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
/// Where the agenda's file goes for a client that keeps its tabs across
/// restarts (`command_actions`, Zed): the same place for the same folders
/// each time, beside the port's script, so that the tab it opens again
/// finds the file, and the next server takes it over.
fn agenda_directory(client: &Client, folders: &[PathBuf]) -> Option<PathBuf> {
    if !client.command_actions {
        return None;
    }
    // FNV-1a: a name that stays the same from one run to the next.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in folders.iter().flat_map(|folder| folder.to_string_lossy().into_owned().into_bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    Some(client.command_script.as_ref()?.parent()?.join("agenda").join(format!("{hash:016x}")))
}

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

/// The values of the `:ID:` properties of `text`, as
/// `org-id-update-id-locations` finds them.
fn property_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches([' ', '\t']);
            let rest = line.get(..4).filter(|key| key.eq_ignore_ascii_case(":ID:")).map(|_| &line[4..])?;
            rest.starts_with([' ', '\t']).then(|| rest.split_whitespace().next()).flatten().map(str::to_string)
        })
        .collect()
}

/// Write the agenda's file and leave it read-only: the agenda is to read,
/// and editors that go by a file's permissions (Zed, Vim) then do not let
/// it be edited.
fn write_read_only(file: &Path, text: &str) -> std::io::Result<()> {
    // In place, the same file each time: Zed follows a file outside the
    // project by the file itself, and loses one that another is renamed
    // over after the first time. The text goes in one write.
    set_writable_if_present(file)?;
    std::fs::write(file, text)?;
    let mut permissions = std::fs::metadata(file)?.permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(file, permissions)
}

/// Let the owner of `file` write it again, if there is one.
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
/// `{ "directory" }` of org-journal, when it is on.
fn journal_directory(journal: Option<&crate::config::JournalSettings>) -> Value {
    match journal {
        Some(journal) => {
            let directory = roam::expand(&journal.directory);
            json!({ "directory": format!("{}/", directory.display()).replace("//", "/") })
        }
        None => Value::Null,
    }
}

fn capabilities(
    encoding: Encoding,
    inlay_hints: bool,
    cursor: bool,
    faces: &Value,
    journal: Option<&crate::config::JournalSettings>,
) -> ServerCapabilities {
    // `orgJournal`: the journal's directory, whose files are Org files
    // without the name saying so (`org-journal-is-journal`).
    let journal = journal_directory(journal);
    ServerCapabilities {
        position_encoding: Some(encoding.kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::INCREMENTAL),
            save: Some(TextDocumentSyncSaveOptions::Supported(true)),
            // `org-crypt-use-before-save-magic`.
            will_save_wait_until: Some(true),
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
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        document_link_provider: Some(lsp_types::DocumentLinkOptions {
            resolve_provider: Some(true),
            work_done_progress_options: Default::default(),
        }),
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
        // `orgFaces`: the faces of particular keywords and priorities
        // (see `org-lsp/faces`).
        experimental: Some(json!({ "org": manifest::manifest(), "orgFaces": faces, "orgJournal": journal })),
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
fn parameters<P: DeserializeOwned>(request: Request) -> std::result::Result<(RequestId, P), Box<Response>> {
    let id = request.id.clone();
    match serde_json::from_value(request.params) {
        Ok(params) => Ok((id, params)),
        Err(error) => Err(Box::new(Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string()))),
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
        commands::AGENDA_GOTO_DATE
        | commands::JOURNAL_NEW_DATE_ENTRY
        | commands::JOURNAL_NEW_SCHEDULED_ENTRY
        | commands::JOURNAL_RESCHEDULE
        | commands::JOURNAL_SEARCH => Some(PlanningCommand::Timestamp { inactive: false }),
        id => planning_command(id),
    }
}

fn planning_command(id: &str) -> Option<PlanningCommand> {
    match id {
        commands::SCHEDULE => Some(PlanningCommand::Schedule),
        commands::DEADLINE => Some(PlanningCommand::Deadline),
        commands::SCHEDULE_DELAY => Some(PlanningCommand::Delay { deadline: false }),
        commands::DEADLINE_WARNING => Some(PlanningCommand::Delay { deadline: true }),
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
            Some(Pending::Folds { uri, folds }) => {
                let mut params = folds;
                params["uri"] = json!(uri.as_str());
                let notification = Notification::new(FOLDS.to_string(), params);
                let _ = self.sender.send(Message::Notification(notification));
            }
            Some(Pending::EditAndShow { uri, position, folds }) => {
                self.refresh_view();
                let applied = response.response_result.ok().and_then(|result| result.get("applied")?.as_bool());
                if applied == Some(true) {
                    let selection = Some(Range::new(position, position));
                    let params =
                        ShowDocumentParams { uri: uri.clone(), external: None, take_focus: Some(true), selection };
                    self.send_request::<ShowDocument>(params, Some(Pending::Folds { uri, folds }));
                }
            }
            Some(Pending::Reveal) => self.moving = None,
            Some(Pending::PromptCursor) => {
                if let Some(open) = self.date_prompt.as_mut() {
                    open.moving_cursor = false;
                }
            }
            Some(Pending::EmacsQuestion(answer)) => {
                let picked =
                    response.response_result.ok().and_then(|item| Some(item.get("title")?.as_str()?.to_string()));
                let _ = answer.send(picked.as_deref() == Some("Yes"));
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
        self.notice_activity();
        macro_rules! dispatch {
            ($($request:ty => $handler:ident,)*) => {
                match request.method.as_str() {
                    $(<$request>::METHOD => match parameters(request) {
                        Ok((id, params)) => respond(id, self.$handler(params)),
                        Err(response) => *response,
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
                        Err(response) => *response,
                    },
                    // A request too, for clients that send no notifications
                    // of their own (Vim's).
                    AGENDA_WIDTH => match parameters::<Value>(request) {
                        Ok((id, params)) => {
                            self.agenda_width(&params);
                            Response::new_ok(id, Value::Null)
                        }
                        Err(response) => *response,
                    },
                    CURSOR => match parameters::<Value>(request) {
                        Ok((id, params)) => {
                            self.roam_cursor(params);
                            Response::new_ok(id, Value::Null)
                        }
                        Err(response) => *response,
                    },
                    AGENDA_CURSOR => match parameters::<Value>(request) {
                        Ok((id, params)) => {
                            self.agenda_cursor(params);
                            Response::new_ok(id, Value::Null)
                        }
                        Err(response) => *response,
                    },
                    STARTUP_FOLDS => match parameters::<FoldingRangeParams>(request) {
                        Ok((id, params)) => respond(id, self.startup_folds(&params.text_document)),
                        Err(response) => *response,
                    },
                    FACES => Response::new_ok(request.id, self.faces.clone()),
                    JOURNAL => Response::new_ok(request.id, journal_directory(self.journal.as_ref())),
                    PREVIEW => match parameters::<Value>(request) {
                        Ok((id, params)) => respond(id, self.preview(&params)),
                        Err(response) => *response,
                    },
                    TEXT_DOCUMENT_CONTENT => match parameters::<Value>(request) {
                        Ok((id, params)) => respond(id, self.text_document_content(&params)),
                        Err(response) => *response,
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
            lsp_types::request::WillSaveWaitUntil => will_save,
            lsp_types::request::DocumentHighlightRequest => document_highlights,
            lsp_types::request::DocumentLinkRequest => document_links,
            lsp_types::request::DocumentLinkResolve => document_link_resolve,
            lsp_types::request::References => references,
            lsp_types::request::PrepareRenameRequest => prepare_rename,
            lsp_types::request::Rename => rename,
            lsp_types::request::WorkspaceSymbolRequest => workspace_symbols,
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
        // A fixed clock moved on, for tests.
        if notification.method == SET_CLOCK && self.clock.is_some() {
            let clock: Option<crate::config::ClockOption> = parse(notification);
            if let Some(now) = clock.and_then(|clock| clock.to_now()) {
                self.clock = Some(now);
            }
            return;
        }
        self.notice_activity();
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
            CURSOR => self.roam_cursor(notification.params),
            DidOpenTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidOpenTextDocumentParams>(notification) else { return };
                let document = params.text_document;
                if document.uri == self.prompt_uri {
                    self.prompt_copy = Some(Document::new(document.text, document.version, &self.todo));
                    self.reveal_prompt_when_current();
                    return;
                }
                // The agenda's file of an earlier server, which the client
                // opens again as it starts: the agenda once more.
                if self.view.is_none()
                    && uri::to_path(&document.uri) == Some(self.view_directory.join(views::FILE_NAME))
                {
                    self.set_view(Query::Agenda { start: self.start_day(), span: self.agenda.span });
                    if let Some(open) = self.view.as_mut().filter(|open| open.uri == document.uri) {
                        open.client = Some(Document::new(document.text, document.version, &self.todo));
                    }
                    self.publish_view();
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
                self.remember_start_file(&document.uri);
                self.shrink_at_startup(&key, &document.text);
                self.documents.insert(key.clone(), Document::new(document.text, document.version, &self.todo));
                self.sync_clock(&key);
                self.publish_virtual_indent(&key);
                self.publish_display(&key);
                self.publish_diagnostics(&key);
            }
            DidChangeTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidChangeTextDocumentParams>(notification) else { return };
                if params.text_document.uri == self.prompt_uri {
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
                let shrunk_before = self
                    .shrunk
                    .contains_key(key)
                    .then(|| self.documents.get(key).map(|document| document.text.clone()))
                    .flatten();
                let changed_lines = changed_lines(&params.content_changes);
                if let Some(document) = self.documents.get_mut(key) {
                    document.apply(
                        params.content_changes,
                        params.text_document.version,
                        self.client.encoding,
                        &self.todo,
                    );
                }
                if let Some(old) = shrunk_before {
                    self.shrunk_after_change(key, &old, changed_lines);
                }
                self.sync_clock(key);
                // Column view follows the text.
                if self.columns.contains_key(key) {
                    if let Ok(uri) = key.parse::<Uri>() {
                        self.columns_view(&uri);
                    }
                }
                self.publish_virtual_indent(key);
                self.publish_display(key);
                self.publish_diagnostics(key);
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
                if params.text_document.uri == self.prompt_uri {
                    self.prompt_copy = None;
                    return;
                }
                self.documents.remove(params.text_document.uri.as_str());
                self.virtually_indented.remove(params.text_document.uri.as_str());
                self.displayed.remove(params.text_document.uri.as_str());
                if let Some(path) = uri::to_path(&params.text_document.uri) {
                    if let Some(index) = self.widened.iter().position(|widened| *widened == path) {
                        let _ = std::fs::remove_file(self.widened.remove(index));
                    }
                }
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
        if *uri == self.prompt_uri {
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
        let (conceal, _, _) = self.display_of(&document.text);
        let tokens = highlight::tokens(&document.syntax(), document.parse.todo_keywords(), conceal);
        let radio: Vec<(usize, usize)> =
            org_core::radio::links(&document.text, &document.parse).iter().map(|link| (link.start, link.end)).collect();
        let tokens = highlight::with_radio_links(tokens, &radio);
        // `org-pretty-entities`: an entity hidden, its character an inlay
        // hint in its place; so are the shrunk fields of tables. A client
        // that shows them itself (`org-lsp/display`) hides them itself.
        let mut hidden: Vec<(usize, usize)> = if self.client.display {
            Vec::new()
        } else {
            self.stand_ins(uri.as_str(), &document.text, &document.syntax())
                .into_iter()
                .map(|(start, end, _)| (start, end))
                .collect()
        };
        hidden.sort_unstable();
        let tokens = if hidden.is_empty() { tokens } else { highlight::with_hidden(tokens, &hidden) };
        let only = range.map(|range| document.index.text_range(&document.text, range, encoding));
        let data = highlight::encode(&tokens, &document.text, &document.index, encoding, only);
        Some(SemanticTokens { result_id: None, data })
    }

    /// What stands in for hidden text of the document `key`, as
    /// `(start, end, shown)`, in order: each entity's character
    /// (`org-pretty-entities`), sub- and superscripts in Unicode, and what
    /// shrunk table fields show.
    fn stand_ins(&self, key: &str, text: &str, root: &org_syntax::SyntaxNode) -> Vec<(usize, usize, String)> {
        let (_, pretty, scripts) = self.display_of(text);
        let mut out: Vec<(usize, usize, String)> = Vec::new();
        if pretty {
            out.extend(
                org_core::pretty::entities(text).into_iter().map(|(start, end, utf8)| (start, end, utf8.to_string())),
            );
        }
        if let Some(braces) = scripts {
            out.extend(org_core::pretty::scripts(root, text, braces));
        }
        out.extend(
            self.shrunk_overlays(key, text).into_iter().map(|overlay| (overlay.start, overlay.end, overlay.display)),
        );
        out.sort_by_key(|(start, end, _)| (*start, *end));
        out
    }

    /// What stands in for hidden text of the document `key` (see
    /// [`Self::stand_ins`]), for a client that shows it itself, as
    /// `org-lsp/display` `{ uri, version, items }`, each item `[line,
    /// character, end line, end character, shown]`. Sent after every
    /// change while there is some, and once more when there is none.
    fn publish_display(&mut self, key: &str) {
        if !self.client.display {
            return;
        }
        let Some(document) = self.documents.get(key) else { return };
        let text = &document.text;
        let encoding = self.client.encoding;
        let items: Vec<Value> = self
            .stand_ins(key, text, &document.syntax())
            .into_iter()
            .map(|(start, end, shown)| {
                let (from, to) =
                    (document.index.position(text, start, encoding), document.index.position(text, end, encoding));
                json!([from.line, from.character, to.line, to.character, shown])
            })
            .collect();
        if items.is_empty() && !self.displayed.remove(key) {
            return;
        }
        if !items.is_empty() {
            self.displayed.insert(key.to_string());
        }
        let params = json!({ "uri": key, "version": document.version, "items": items });
        let notification = lsp_server::Notification::new("org-lsp/display".to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// How a file shows: what is concealed, whether entities show as their
    /// characters, and the sub- and superscripts with them (whether they
    /// need braces). As the settings say, unless the file's `#+STARTUP:`
    /// says otherwise: `descriptivelinks` or `literallinks`
    /// (`org-link-descriptive`), `entitiespretty` or `entitiesplain`
    /// (`org-pretty-entities`); the last word wins.
    fn display_of(&self, text: &str) -> (highlight::Conceal, bool, Option<bool>) {
        let mut conceal = self.conceal;
        let mut pretty = self.pretty_entities;
        for word in startup_words(text) {
            match word.as_str() {
                "descriptivelinks" => conceal.links = true,
                "literallinks" => conceal.links = false,
                "entitiespretty" => pretty = true,
                "entitiesplain" => pretty = false,
                _ => {}
            }
        }
        (conceal, pretty, self.pretty_scripts.filter(|_| pretty))
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

    /// Before a file is saved: with `org-crypt-use-before-save-magic` in
    /// Emacs's configuration, its entries tagged `crypt` encrypted
    /// (`org-encrypt-entries`). Emacs is asked only for a file with a
    /// `:crypt:` tag, until it says it has the magic.
    /// Before that, in one of org-roam's files, `roam:` links become `id:`
    /// links (`org-roam-link-replace-all`).
    fn will_save(&mut self, params: lsp_types::WillSaveTextDocumentParams) -> Option<Vec<TextEdit>> {
        let uri = params.text_document.uri;
        let old = self.document(&uri)?.text.clone();
        let replaced = self.roam_before_save(&uri, &old);
        let text = replaced.clone().unwrap_or_else(|| old.clone());
        let encrypted = self.encrypt_before_save(&uri, &text);
        let new = encrypted.or(replaced)?;
        Some(self.text_edits(&old, &new))
    }

    /// The text of the file at `uri` with its `crypt` entries encrypted,
    /// when Emacs encrypts before saving and that changes it.
    fn encrypt_before_save(&mut self, uri: &Uri, old: &str) -> Option<String> {
        if self.crypt_before_save == Some(false) {
            return None;
        }
        let path = uri::to_path(uri)?;
        if self.crypt_before_save.is_none() && !old.contains(":crypt:") {
            return None;
        }
        let params = json!({ "file": path.to_string_lossy(), "text": old, "point": 1 });
        let result = match self.emacs.call("crypt-before-save", params, std::time::Duration::from_secs(10)) {
            Ok(result) => result,
            Err(error) => {
                self.show_message(MessageType::ERROR, error);
                return None;
            }
        };
        let magic = result.get("magic").and_then(Value::as_bool).unwrap_or(false);
        self.crypt_before_save = Some(magic);
        result.get("text").and_then(Value::as_str).filter(|new| *new != old).map(str::to_string)
    }

    /// The folds closed when a file opens (see [`STARTUP_FOLDS`]).
    fn startup_folds(&mut self, params: &lsp_types::TextDocumentIdentifier) -> Option<Value> {
        let document = self.document(&params.uri)?;
        let folds = folding::startup_folds(
            &document.syntax(),
            &document.text,
            &document.index,
            self.separator_lines,
            &self.startup,
        );
        Some(json!({ "folds": folds }))
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
        // `org-pretty-entities`: the character of each entity, whose text
        // the tokens hide; so with sub- and superscripts, in Unicode where
        // they can be, and shrunk table fields, what Org's overlays show.
        if !self.client.display {
            for (start, _, shown) in self.stand_ins(params.text_document.uri.as_str(), text, &document.syntax()) {
                let at = document.index.position(text, start, self.client.encoding);
                if (first..=last).contains(&(at.line as usize)) && !shown.is_empty() {
                    hints.push(hint(at.line, at.character, shown));
                }
            }
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

    /// The cursor is at `position` of `uri`, the agenda or an Org file:
    /// the port's commands for it go there, and the port's script to this
    /// server.
    fn saw_cursor(&mut self, uri: &Uri, position: Position) {
        if self.is_view(uri) {
            self.agenda_cursor = Some(position);
        } else if self.documents.contains_key(uri.as_str()) {
            self.file_cursor = Some((uri.clone(), position));
        } else {
            return;
        }
        // The keys go to the server whose file the cursor is in.
        self.claim_script();
    }

    /// The Org file `uri` was opened: the port's script opens it, in its
    /// workspace folder, to start a server when none answers. Not the
    /// server's own files (capture buffers, narrowed files, an agenda),
    /// which go away or are no project's.
    fn remember_start_file(&mut self, uri: &Uri) {
        let Some(path) = uri::to_path(uri) else { return };
        let script_directory = self.client.command_script.as_ref().and_then(|script| script.parent());
        if path.starts_with(temp_dir())
            || script_directory.is_some_and(|directory| path.starts_with(directory))
            || path.extension().is_some_and(|extension| extension == "org-agenda")
            || self.port_start.file.as_ref() == Some(&path)
        {
            return;
        }
        let folder = self.workspace.folders().iter().find(|folder| path.starts_with(folder)).cloned();
        self.port_start.folder = folder.or_else(|| path.parent().map(Path::to_path_buf));
        self.port_start.file = Some(path);
        // Only into the script this server answers to.
        if let (Some(port), Some(script)) = (&self.port, &self.client.command_script) {
            if port.owns(script) {
                self.claim_script();
            }
        }
    }

    /// Make the port's script send to this server.
    fn claim_script(&mut self) {
        let (Some(port), Some(path)) = (&self.port, &self.client.command_script) else { return };
        if let Err(error) = port.claim(path, &self.port_start) {
            self.show_message(MessageType::ERROR, format!("Cannot write {}: {error}", path.display()));
        }
    }

    fn code_actions(&mut self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        if self.client.command_actions {
            self.saw_cursor(uri, params.range.start);
        }
        let only = params.context.only.as_deref();
        let mut context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
        // Over a selection: the region, the point at its start. Not over
        // one character, which is the cursor in Helix.
        let (start, end) = (params.range.start, params.range.end);
        if start.line != end.line || end.character > start.character + 1 {
            context["range"] = json!(params.range);
        }
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
                let Ok(edited) = item_structure(
                    id,
                    text,
                    offset,
                    keywords,
                    self.refile.2.tags_column,
                    self.edit_settings.list_circular_motion,
                ) else {
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
        let in_table = at_org_table(&document.text, &document.parse.syntax(), offset);
        for (id, offered) in [
            (commands::TANGLE, in_code),
            (commands::TABLE_TOGGLE_COLUMN_WIDTH, in_table),
            (commands::BABEL_EXECUTE, in_code),
            (commands::EDIT_SPECIAL, in_code),
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
        let found = completion::complete(&document.text, offset, &context);
        // org-roam's titles in a link or, with
        // `org_roam_completion_everywhere`, for a word, first.
        let roam = self.roam_completion(&uri, offset);
        let document = self.document(&uri)?;
        let Some((start, items)) = found else {
            return (!roam.is_empty()).then_some(lsp_types::CompletionResponse::Array(roam));
        };
        let range = Range::new(document.index.position(&document.text, start, encoding), position.position);
        let items: Vec<lsp_types::CompletionItem> = items
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
        Some(lsp_types::CompletionResponse::Array(roam.into_iter().chain(items).collect()))
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
        let call = |link: &str, list: &[(String, String)]| self.expand_abbreviation(link, list);
        let opened =
            open::at_point_calling(&text, &document.parse, offset, None, &self.link_abbreviations, Some(&call)).ok()?;
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
            return self
                .link_hover(&position.text_document.uri, position.position)
                .or_else(|| self.citation_hover(&position.text_document.uri, position.position));
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

    /// The links of an Org file (`textDocument/documentLink`): a URL or a
    /// file at once; a place in a file (`*Heading`, `id:`, `file:…::search`)
    /// once the editor asks for it (`documentLink/resolve`), as the file
    /// with `#L` and the line. The tooltip is what Emacs shows over a link.
    fn document_links(&mut self, params: lsp_types::DocumentLinkParams) -> Option<Vec<lsp_types::DocumentLink>> {
        let uri = params.text_document.uri;
        if self.is_view(&uri) {
            return None;
        }
        let encoding = self.client.encoding;
        let document = self.document(&uri)?;
        let (text, parse, index) = (document.text.clone(), document.parse.clone(), document.index.clone());
        let mut out = Vec::new();
        for link in open::links(&text, &parse, &self.link_abbreviations) {
            let (target, data) = match &link.open {
                Ok(Open::External(url)) => (url.parse::<Uri>().ok(), None),
                Ok(Open::File { path, search: None, .. }) => {
                    (self.link_path(&uri, path).and_then(|path| uri::from_path(&path)), None)
                }
                Ok(Open::File { .. } | Open::Here(_) | Open::Id(_)) => {
                    (None, Some(json!({ "uri": uri.as_str(), "offset": link.start })))
                }
                _ => continue,
            };
            if target.is_none() && data.is_none() {
                continue;
            }
            let range =
                Range::new(index.position(&text, link.start, encoding), index.position(&text, link.end, encoding));
            out.push(lsp_types::DocumentLink { range, target, tooltip: Some(format!("LINK: {}", link.raw)), data });
        }
        Some(out)
    }

    /// Where a link `document_links` left for later leads.
    fn document_link_resolve(&mut self, mut link: lsp_types::DocumentLink) -> lsp_types::DocumentLink {
        let Some(data) = link.data.take() else { return link };
        let uri = data.get("uri").and_then(Value::as_str).and_then(|uri| uri.parse::<Uri>().ok());
        let offset = data.get("offset").and_then(Value::as_u64).map(|offset| offset as usize);
        let (Some(uri), Some(offset)) = (uri, offset) else { return link };
        let Some(document) = self.document(&uri) else { return link };
        let (text, parse) = (document.text.clone(), document.parse.clone());
        let Some(Ok(opened)) = open::link_at(&text, &parse, offset, &self.link_abbreviations).map(|found| found.open)
        else {
            return link;
        };
        if let Ok(Some(location)) = self.open_location(&uri, &text, &opened) {
            let start = location.range.start;
            link.target = format!("{}#L{},{}", location.uri.as_str(), start.line + 1, start.character + 1).parse().ok();
        }
        link
    }

    /// Over a link in an Org file, the link as Emacs shows it (`LINK:
    /// …`), and the entry it leads to, as the agenda's hover shows one.
    fn link_hover(&mut self, uri: &Uri, position: Position) -> Option<lsp_types::Hover> {
        let encoding = self.client.encoding;
        let document = self.document(uri)?;
        let (text, parse, index) = (document.text.clone(), document.parse.clone(), document.index.clone());
        let offset = index.offset(&text, position, encoding);
        let link = open::link_at(&text, &parse, offset, &self.link_abbreviations)?;
        let entry = link.open.as_ref().ok().and_then(|opened| self.link_entry(uri, &text, opened));
        let line = format!("LINK: {}", link.raw);
        let value = match entry {
            Some(entry) => format!("`{}`\n\n{}", line.replace('`', "'"), org_core::preview::markdown(&entry)),
            None => format!("`{}`", line.replace('`', "'")),
        };
        let range = Range::new(index.position(&text, link.start, encoding), index.position(&text, link.end, encoding));
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value,
            }),
            range: Some(range),
        })
    }

    /// The Org files the server knows: the agenda files and the files open
    /// in the editor, each with its text.
    fn known_files(&mut self) -> Vec<(Uri, String)> {
        let mut files: Vec<(Uri, String)> = self
            .agenda_files()
            .into_iter()
            .map(|(uri, held)| {
                let text = match &held {
                    Held::Open(outline) => outline.text.clone(),
                    Held::Disk(source) => source.text.clone(),
                };
                (uri, text)
            })
            .collect();
        let mut paths: Vec<PathBuf> = files.iter().filter_map(|(uri, _)| uri::to_path(uri)).collect();
        let mut keys: Vec<&String> = self.documents.keys().collect();
        keys.sort();
        for key in keys {
            let Ok(uri) = key.parse::<Uri>() else { continue };
            let Some(path) = uri::to_path(&uri) else { continue };
            if !paths.contains(&path) && !self.is_view(&uri) {
                paths.push(path);
                files.push((uri, self.documents[key].text.clone()));
            }
        }
        files
    }

    /// Backlinks (`textDocument/references`): the links in the Org files
    /// the server knows that lead to the entry at the cursor, or to the
    /// entry the link at the cursor leads to: by its ID, a search for its
    /// headline or its `CUSTOM_ID`, from another file or the same one.
    fn references(&mut self, params: lsp_types::ReferenceParams) -> Option<Vec<Location>> {
        let encoding = self.client.encoding;
        let place = params.text_document_position;
        let uri = place.text_document.uri;
        if self.is_view(&uri) {
            return None;
        }
        let document = self.document(&uri)?;
        let (text, parse) = (document.text.clone(), document.parse.clone());
        let offset = document.index.offset(&text, place.position, encoding);
        let (target, target_text, heading) = match open::link_at(&text, &parse, offset, &self.link_abbreviations) {
            Some(link) => {
                let opened = link.open.ok()?;
                let location = self.open_location(&uri, &text, &opened).ok()??;
                let (_, target_text) = self.file_text(&uri::to_path(&location.uri)?)?;
                let at = LineIndex::new(&target_text).offset(&target_text, location.range.start, encoding);
                let heading = org_core::properties::heading_at(&target_text, at)?;
                (location.uri, target_text, heading)
            }
            None => {
                let heading = org_core::properties::heading_at(&text, offset)?;
                (uri.clone(), text.clone(), heading)
            }
        };
        let mut out = Vec::new();
        if params.context.include_declaration {
            let position = LineIndex::new(&target_text).position(&target_text, heading, encoding);
            out.push(Location { uri: target.clone(), range: Range::new(position, position) });
        }
        for (file, file_text, link) in self.backlinks(&target, &target_text, heading) {
            let index = LineIndex::new(&file_text);
            let range = Range::new(
                index.position(&file_text, link.start, encoding),
                index.position(&file_text, link.end, encoding),
            );
            out.push(Location { uri: file, range });
        }
        Some(out)
    }

    /// The links in the Org files the server knows that lead to the entry
    /// at `heading` of `target` (whose text is `target_text`): by its ID,
    /// or a search for its headline or its `CUSTOM_ID`, from another file
    /// or the same one. Each with the text of its file.
    fn backlinks(&mut self, target: &Uri, target_text: &str, heading: usize) -> Vec<(Uri, String, open::Link)> {
        let Some(target_path) = uri::to_path(target) else { return Vec::new() };
        let canonical = std::fs::canonicalize(&target_path).ok();
        let same =
            |path: &Path| path == target_path || (canonical.is_some() && std::fs::canonicalize(path).ok() == canonical);
        let target_parse = org_syntax::parse(target_text, &self.todo);
        let id = {
            let outline = Outline::new(&target_parse, target_text, &uri::file_stem(target));
            outline.entry_at(heading).and_then(|entry| outline.entry_property(Some(entry), "ID", false))
        };
        let mut out = Vec::new();
        for (file, file_text) in self.known_files() {
            let Some(file_path) = uri::to_path(&file) else { continue };
            let file_parse = org_syntax::parse(&file_text, &self.todo);
            for link in open::links(&file_text, &file_parse, &self.link_abbreviations) {
                let leads = match &link.open {
                    Ok(Open::Here(at)) => same(&file_path) && *at == heading,
                    Ok(Open::Id(found)) => id.as_deref() == Some(found.as_str()),
                    Ok(Open::File { path, search: Some(search), system: false }) => {
                        self.link_path(&file, path).is_some_and(|path| same(&path))
                            && open::search_in_file(target_text, Some(&target_parse), search).ok() == Some(heading)
                    }
                    _ => false,
                };
                if leads {
                    out.push((file.clone(), file_text.clone(), link));
                }
            }
        }
        out
    }

    /// `textDocument/prepareRename`: a tag, or a headline's title.
    fn prepare_rename(
        &mut self,
        params: lsp_types::TextDocumentPositionParams,
    ) -> Option<lsp_types::PrepareRenameResponse> {
        let encoding = self.client.encoding;
        let document = self.document(&params.text_document.uri)?;
        let offset = document.index.offset(&document.text, params.position, encoding);
        let (start, end, name) = match org_core::rename::target(&document.text, offset, document.parse.todo_keywords())?
        {
            org_core::rename::Target::Tag { start, end, name } => (start, end, name),
            org_core::rename::Target::Heading { start, end, title, .. } => (start, end, title),
        };
        let range = Range::new(
            document.index.position(&document.text, start, encoding),
            document.index.position(&document.text, end, encoding),
        );
        Some(lsp_types::PrepareRenameResponse::RangeWithPlaceholder { range, placeholder: name })
    }

    /// `textDocument/rename`: a tag in every Org file the server knows, in
    /// its headlines (aligned again) and `#+FILETAGS:` and `#+TAGS:` lines;
    /// or a headline's title, and the links that search for it.
    fn rename(&mut self, params: lsp_types::RenameParams) -> Option<WorkspaceEdit> {
        use org_core::rename::{self, Target};
        let encoding = self.client.encoding;
        let place = params.text_document_position;
        let uri = place.text_document.uri;
        let new = params.new_name.trim().to_string();
        let document = self.document(&uri)?;
        let (text, offset) = (document.text.clone(), document.index.offset(&document.text, place.position, encoding));
        let target = rename::target(&text, offset, document.parse.todo_keywords())?;
        let mut changed: Vec<(Uri, String, String)> = Vec::new();
        match target {
            Target::Tag { name, .. } => {
                if !rename::valid_tag(&new) {
                    self.show_message(MessageType::ERROR, format!("Invalid tag: \"{new}\""));
                    return None;
                }
                let settings = self.edit_settings.tags.clone();
                for (file, old) in self.known_files() {
                    let renamed = rename::rename_tag(&old, &name, &new, &settings);
                    if renamed != old {
                        changed.push((file, old, renamed));
                    }
                }
            }
            Target::Heading { heading, start, end, .. } => {
                if new.is_empty() || new.contains('\n') {
                    return None;
                }
                // The links' paths, by file, last first.
                let mut by_file: Vec<(Uri, String, Vec<Replacement>)> = Vec::new();
                for (file, file_text, link) in self.backlinks(&uri, &text, heading) {
                    if !matches!(link.open, Ok(Open::Here(_) | Open::File { .. })) {
                        continue;
                    }
                    let Some(raw) = rename::retarget(&link.raw, &new) else { continue };
                    let written = if link.bracket {
                        org_core::link::escape(&raw)
                    } else if raw.contains(char::is_whitespace) {
                        continue;
                    } else {
                        raw
                    };
                    match by_file.iter_mut().find(|(other, _, _)| *other == file) {
                        Some((_, _, edits)) => edits.push((link.path.0, link.path.1, written)),
                        None => by_file.push((file, file_text, vec![(link.path.0, link.path.1, written)])),
                    }
                }
                if !by_file.iter().any(|(file, _, _)| *file == uri) {
                    by_file.push((uri.clone(), text.clone(), Vec::new()));
                }
                for (file, old, mut edits) in by_file {
                    if file == uri {
                        edits.push((start, end, new.clone()));
                    }
                    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
                    let mut renamed = old.clone();
                    for (from, to, written) in edits {
                        renamed.replace_range(from..to, &written);
                    }
                    changed.push((file, old, renamed));
                }
            }
        }
        let documents: Vec<(Uri, Option<i32>, Vec<TextEdit>)> = changed
            .into_iter()
            .map(|(file, old, new)| {
                let version = self.document(&file).map(|document| document.version);
                let edits = self.text_edits(&old, &new);
                (file, version, edits)
            })
            .collect();
        Some(self.workspace_edits(documents))
    }

    /// `workspace/symbol`: the headlines of the Org files the server knows
    /// whose title has the letters of the query in order, without regard
    /// to case, each with its file and its parent.
    fn workspace_symbols(
        &mut self,
        params: lsp_types::WorkspaceSymbolParams,
    ) -> Option<lsp_types::WorkspaceSymbolResponse> {
        let encoding = self.client.encoding;
        let query: Vec<char> = params.query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
        let matches = |name: &str| {
            let mut wanted = query.iter().peekable();
            for c in name.to_lowercase().chars() {
                if wanted.peek() == Some(&&c) {
                    wanted.next();
                }
            }
            wanted.peek().is_none()
        };
        let mut out = Vec::new();
        for (file, text) in self.known_files() {
            let parse = org_syntax::parse(&text, &self.todo);
            let index = LineIndex::new(&text);
            let name = uri::file_name(&file);
            for mut symbol in symbols::symbol_information(&parse.syntax(), &text, &index, encoding, &file) {
                if !matches(&symbol.name) {
                    continue;
                }
                symbol.container_name = Some(match symbol.container_name.take() {
                    Some(parent) => format!("{name} › {parent}"),
                    None => name.clone(),
                });
                out.push(symbol);
                if out.len() >= 1000 {
                    return Some(lsp_types::WorkspaceSymbolResponse::Flat(out));
                }
            }
        }
        Some(lsp_types::WorkspaceSymbolResponse::Flat(out))
    }

    /// The entry a link leads to, when it leads to an entry's headline in
    /// an Org file: its headline and text, each drawer folded.
    fn link_entry(&mut self, uri: &Uri, text: &str, opened: &Open) -> Option<String> {
        let (target, offset) = match opened {
            Open::Here(offset) => (text.to_string(), *offset),
            Open::Id(id) => {
                let (file, offset) = self.find_id(id)?;
                (self.file_text(&uri::to_path(&file)?)?.1, offset)
            }
            Open::File { path, search: Some(search), system: false } => {
                let path = self.link_path(uri, path)?;
                if !path.extension().is_some_and(|extension| extension == "org" || extension == "org_archive") {
                    return None;
                }
                let (_, target) = self.file_text(&path)?;
                let parse = org_syntax::parse(&target, &self.todo);
                let offset = open::search_in_file(&target, Some(&parse), search).ok()?;
                (target, offset)
            }
            _ => return None,
        };
        let heading = org_core::properties::heading_at(&target, offset)?;
        (heading == offset).then(|| org_core::preview::entry(&target, heading))
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
        let radio: Vec<(usize, usize)> =
            org_core::radio::links(&text, &parse).iter().map(|link| (link.start, link.end)).collect();
        let tokens = highlight::with_radio_links(tokens, &radio);
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
        let call = |link: &str, list: &[(String, String)]| self.expand_abbreviation(link, list);
        let mut opened = open::at_point_calling(&text, &parse, offset, answer, &self.link_abbreviations, Some(&call))?;
        // `org-attach-follow`: the file in the entry's attachment
        // directory, as Emacs finds it.
        if let Open::Attachment(link) = &opened {
            let (name, search) = match link.rfind("::") {
                Some(at) => (&link[..at], Some(&link[at..])),
                None => (link.as_str(), None),
            };
            let path = uri::to_path(&uri).ok_or("Only a file on disk has attachments")?;
            let point = text[..offset].chars().count() + 1;
            let params = json!({ "file": path.to_string_lossy(), "text": text, "point": point, "name": name });
            let found = self
                .emacs
                .call("attach-expand", params, std::time::Duration::from_secs(60))
                .map_err(Refusal::Failed)?;
            let full = found.as_str().ok_or("No attachment directory for this entry")?;
            opened = open::file_link(&format!("{full}{}", search.unwrap_or("")));
        }
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
            Open::Roam(title) => return self.roam_follow(&uri, offset, title),
            Open::Attachment(_) => return Err("No attachment directory for this entry".into()),
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
                let (target, offset) = self.find_id(id).ok_or_else(|| format!("Cannot find entry with ID \"{id}\""))?;
                let (_, text) = self
                    .file_text(&uri::to_path(&target).ok_or("Cannot read the file")?)
                    .ok_or("Cannot read the file")?;
                Ok(Some(at(target, &text, offset)))
            }
            _ => Ok(None),
        }
    }

    /// `org-link-expand-abbrev` of a link whose abbreviation calls a
    /// function (`%(…)`), by Emacs, which knows the function.
    fn expand_abbreviation(
        &self,
        link: &str,
        abbreviations: &[(String, String)],
    ) -> std::result::Result<String, String> {
        let params = json!({ "link": link, "abbreviations": abbreviations });
        let expanded = self.emacs.call("expand-abbrev", params, std::time::Duration::from_secs(30))?;
        expanded.as_str().map(str::to_string).ok_or_else(|| format!("Cannot expand the link {link}"))
    }

    /// The files whose IDs Org knows: those open in the editor, then the
    /// agenda files.
    fn id_paths(&mut self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> =
            self.documents.keys().filter_map(|key| uri::to_path(&key.parse().ok()?)).collect();
        paths.sort();
        for (file, _) in self.agenda_files() {
            if let Some(path) = uri::to_path(&file).filter(|path| !paths.contains(path)) {
                paths.push(path);
            }
        }
        paths
    }

    /// Every `ID` of the files IDs are looked for in: the open files as
    /// the editor has them, the others as they are on disk, read again
    /// once they change.
    fn known_ids(&mut self) -> HashSet<String> {
        let mut ids = HashSet::new();
        let mut on_disk = HashMap::new();
        for path in self.id_paths() {
            let open = self.documents.iter().find(|(key, _)| {
                key.parse::<Uri>().ok().and_then(|key| uri::to_path(&key)).is_some_and(|key| key == path)
            });
            if let Some((_, document)) = open {
                ids.extend(property_ids(&document.text));
                continue;
            }
            let Some(changed) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()).ok() else {
                continue;
            };
            let found = match self.id_files.remove(&path) {
                Some((time, found)) if time == changed => found,
                _ => {
                    std::fs::read(&path).map(|bytes| property_ids(&String::from_utf8_lossy(&bytes))).unwrap_or_default()
                }
            };
            ids.extend(found.iter().cloned());
            on_disk.insert(path, (changed, found));
        }
        self.id_files = on_disk;
        ids
    }

    /// `org-id-find`: the file and the headline of the entry with this
    /// `ID`, in the files open in the editor, then in the agenda files.
    fn find_id(&mut self, id: &str) -> Option<(Uri, usize)> {
        for path in self.id_paths() {
            let Some((target, text)) = self.file_text(&path) else { continue };
            if let Some(offset) = open::find_id(&org_syntax::parse(&text, &self.todo), id) {
                return Some((target, offset));
            }
        }
        None
    }

    fn text_document_content(&mut self, params: &Value) -> Value {
        let uri = params.get("uri").and_then(Value::as_str);
        if uri == Some(self.prompt_uri.as_str()) {
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
            let files = keys
                .into_iter()
                .filter_map(|key| {
                    let uri: Uri = key.parse().ok()?;
                    uri::to_path(&uri)?;
                    Some((uri.clone(), open(&self.documents[key], &uri)))
                })
                .collect();
            return self.with_journal_files(files);
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
        let open: HashMap<PathBuf, String> = by_path.into_iter().map(|(path, key)| (path, key.clone())).collect();
        files = self.with_journal_files(files);
        if self.agenda_archives == ArchivesMode::Files {
            files = self.with_archive_files(files, &open);
        }
        files
    }

    /// `org-journal-enable-agenda-integration`: the journal's files of
    /// today and the days to come, after the other agenda files, as
    /// `org-journal--update-org-agenda-files` leaves them.
    fn with_journal_files(&mut self, files: Vec<(Uri, Held)>) -> Vec<(Uri, Held)> {
        let Some(journal) = self.journal.clone().filter(|journal| journal.agenda_integration) else { return files };
        let directory = roam::expand(&journal.directory);
        let mut names = Vec::new();
        roam::list_files(&directory, "", 0, &mut names);
        let Ok(agenda) = org_core::journal::agenda_files(&names, self.date_time(), &journal.core) else {
            return files;
        };
        let is_journal = |uri: &Uri| {
            uri::to_path(uri).is_some_and(|path| {
                path.strip_prefix(&directory).is_ok_and(|relative| {
                    org_core::journal::is_journal_file(&relative.to_string_lossy().replace('\\', "/"), &journal.core)
                })
            })
        };
        let mut out: Vec<(Uri, Held)> = files.into_iter().filter(|(uri, _)| !is_journal(uri)).collect();
        let open: HashMap<PathBuf, String> = self
            .documents
            .keys()
            .filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, key.clone())))
            .collect();
        for name in agenda {
            out.extend(self.held(&directory.join(name), &open));
        }
        out
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
        let mut view = views::render(
            query,
            &blocks,
            &outlines,
            search.as_ref(),
            self.now(),
            clocked,
            Some(&self.agenda_filters),
            &self.agenda_marks,
        );
        drop(outlines);
        drop(files);
        self.dim_blocked(&mut view);
        view
    }

    /// `org-agenda-dim-blocked-tasks`: the lines of tasks their
    /// dependencies block (`org-entry-blocked-p`) in one dimmed token each.
    fn dim_blocked(&mut self, view: &mut View) {
        let todo = self.edit_settings.todo.clone();
        if self.dim_blocked == DimBlocked::No
            || (!todo.enforce_todo_dependencies && !todo.enforce_todo_checkbox_dependencies)
        {
            return;
        }
        // For each entry: whether it is blocked, and by checkboxes.
        let mut known: HashMap<(String, usize), Option<bool>> = HashMap::new();
        let mut dimmed: Vec<(usize, usize)> = Vec::new();
        let mut hidden: Vec<usize> = Vec::new();
        let mut start = 0;
        for (number, line_text) in view.text.split('\n').enumerate() {
            let end = start + line_text.len();
            if let Some(entry) = view.lines.get(number).and_then(|line| line.entry.as_ref()) {
                let key = (entry.uri.as_str().to_string(), entry.offset);
                let blocked = match known.get(&key) {
                    Some(blocked) => *blocked,
                    None => {
                        let blocked = self
                            .with_source(&entry.uri, |text, _, parse, _, _| {
                                org_core::todo::entry_blocked(text, parse.todo_keywords(), entry.offset, &todo)
                            })
                            .flatten();
                        known.insert(key, blocked);
                        blocked
                    }
                };
                // `invisible`: out of sight, unless checkboxes block it,
                // which the agenda does not show.
                match blocked {
                    Some(false) if self.dim_blocked == DimBlocked::Hide => hidden.push(number),
                    Some(_) if end > start => dimmed.push((start, end)),
                    _ => {}
                }
            }
            start = end + 1;
        }
        if !dimmed.is_empty() {
            Self::dim_lines(view, dimmed);
        }
        view.hide_lines(&hidden);
    }

    /// The text from each start to each end in `dimmed` shown as a blocked
    /// task (`org-agenda-dimmed-todo-face`).
    fn dim_lines(view: &mut View, dimmed: Vec<(usize, usize)>) {
        let inside = |token: &crate::highlight::Token| {
            dimmed
                .iter()
                .any(|(start, end)| usize::from(token.range.start()) < *end && usize::from(token.range.end()) > *start)
        };
        view.tokens.retain(|token| !inside(token));
        for (start, end) in dimmed {
            let range = org_syntax::TextRange::new((start as u32).into(), (end as u32).into());
            view.tokens.push(crate::highlight::Token {
                range,
                ty: crate::highlight::TokenType::AgendaDimmed,
                modifiers: 0,
            });
        }
        view.tokens.sort_by_key(|token| token.range.start());
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
                    // This server's agenda: its keys come here.
                    self.open_port();
                    self.claim_script();
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
    /// from a key (`command_actions`), and make its script this server's,
    /// unless another server answers there: that one keeps it until this
    /// one's agenda is used.
    fn open_port(&mut self) {
        if !self.client.command_actions || self.port.is_some() {
            return;
        }
        match port::open(self.port_sender.clone()) {
            Ok(port) => {
                let taken = self.client.command_script.as_ref().is_some_and(|script| port.answered_elsewhere(script));
                self.port = Some(port);
                self.port_start.editor = port::editor_command();
                if taken {
                    return;
                }
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
            if let Some(open) = &self.view {
                self.columns.remove(open.uri.as_str());
            }
            self.remove_view_file();
            self.view = None;
            self.reveal = None;
            self.follow = None;
            self.moving = None;
        }
    }

    fn run_from_port(&mut self, sent: port::Command) {
        self.drop_closed_view();
        let view = self.view.as_ref().map(|open| open.uri.clone());
        let at = sent.position.map(|(line, character)| Position::new(line, character));
        // `file:COMMAND` and `key:KEY` act in the Org file or capture buffer
        // the cursor was last seen in; a file given acts there.
        let in_file = sent.command.starts_with("file:") || sent.command.starts_with("key:");
        let file = match sent.file.as_deref().and_then(uri::from_path).filter(|uri| Some(uri) != view.as_ref()) {
            Some(uri) => Some((uri, at.unwrap_or_default())),
            None if in_file => match self.file_cursor.clone() {
                Some(seen) => Some(seen),
                None => return self.show_message(MessageType::INFO, "Put the cursor in an Org file first"),
            },
            None => None,
        };
        let (argument, scopes) = match (file, view) {
            // An Org file or a capture buffer: the server applies what the
            // command does there, and moves the cursor, as an editor would.
            (Some((uri, position)), _) => {
                let capture = self.capture.as_ref().is_some_and(|capture| capture.uri == uri)
                    || self.note.as_ref().is_some_and(|note| note.uri == uri);
                let scopes: &[commands::Scope] = if capture {
                    &[commands::Scope::Capture, commands::Scope::Org, commands::Scope::Global]
                } else {
                    &[commands::Scope::Org, commands::Scope::Global]
                };
                (json!({ "textDocument": { "uri": uri }, "position": position, "apply": true }), scopes)
            }
            (None, Some(uri)) => {
                let position = at.or(self.agenda_cursor).unwrap_or_default();
                (
                    json!({ "textDocument": { "uri": uri }, "position": position }),
                    &[commands::Scope::Agenda, commands::Scope::Global][..],
                )
            }
            // With no agenda open, the commands that need none: the agenda,
            // the TODO list, the agenda menu, capture.
            (None, None) => (json!({}), &[commands::Scope::Global][..]),
        };
        // `key:C-c C-c`: what that key of Org's runs where it is pressed.
        let command = match sent.command.strip_prefix("key:") {
            Some(key) => match commands::by_key(key, scopes) {
                Some(id) => id.to_string(),
                None => return self.show_message(MessageType::INFO, format!("{key} is undefined here")),
            },
            None => sent.command.strip_prefix("file:").unwrap_or(&sent.command).to_string(),
        };
        let params = ExecuteCommandParams {
            command: command.clone(),
            arguments: vec![argument.clone()],
            work_done_progress_params: Default::default(),
        };
        match self.run_command(params) {
            Ok(value) => {
                self.apply_result(&argument, &value);
                if self.client.message_prompts {
                    self.prompt_with_messages(command, argument, &value);
                }
            }
            Err(refusal) => self.show_message(MessageType::INFO, refusal.message().to_string()),
        }
    }

    /// A command's result that the editor would carry out, for a client
    /// that does nothing with one: the edits it carries applied, and the
    /// cursor moved where it says.
    fn apply_result(&mut self, argument: &Value, value: &Value) {
        let Some(uri) =
            argument.pointer("/textDocument/uri").and_then(Value::as_str).and_then(|uri| uri.parse::<Uri>().ok())
        else {
            return;
        };
        let Some(cursor) =
            value.get("cursor").and_then(|cursor| serde_json::from_value::<Position>(cursor.clone()).ok())
        else {
            return;
        };
        let edits: Vec<TextEdit> =
            value.get("edits").and_then(|edits| serde_json::from_value(edits.clone()).ok()).unwrap_or_default();
        if edits.is_empty() {
            let selection = Some(Range::new(cursor, cursor));
            let params = ShowDocumentParams { uri, external: None, take_focus: Some(true), selection };
            self.send_request::<ShowDocument>(params, None);
            return;
        }
        let version = self.document(&uri).map(|document| document.version);
        let edit = self.workspace_edit(&uri, version, edits);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::EditAndMove { uri, position: cursor });
    }

    /// Tell the client that the view has new text.
    fn publish_view(&mut self) {
        let Some(open) = &self.view else { return };
        match &open.file {
            Some(file) => {
                let uri = open.uri.clone();
                match write_read_only(file, &open.view.text) {
                    Err(error) => self.show_message(MessageType::ERROR, format!("Cannot write the agenda: {error}")),
                    // The client reads it again.
                    Ok(()) if self.client.view_changed => {
                        let params = json!({ "uri": uri });
                        let notification = lsp_server::Notification::new(VIEW_CHANGED.to_string(), params);
                        let _ = self.sender.send(Message::Notification(notification));
                    }
                    Ok(()) => {}
                }
            }
            None => {
                let params = json!({ "uri": open.uri });
                self.send_raw_request(TEXT_DOCUMENT_CONTENT_REFRESH, params, None);
            }
        }
        // Column view stays on, as `org-agenda-redo` keeps it.
        self.agenda_columns_view();
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
            Ok(value) => {
                // A command run from the command port: what it does is
                // applied here too (`org-goto`'s heading picked).
                if argument.get("apply").and_then(Value::as_bool) == Some(true) {
                    self.apply_result(&argument, &value);
                }
                self.prompt_with_messages(command, argument, &value)
            }
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
        if params.command.starts_with("org.journal.") {
            return self.journal_command(&params.command, &argument.unwrap_or_else(|| json!({})));
        }
        // A capture for a `roam:` link to a new node goes on with its
        // questions, which come back to `org.open`.
        let roam_follow = params.command == commands::OPEN
            && argument.as_ref().is_some_and(|value| value.get("title").is_some() && value.get("step").is_some());
        if params.command.starts_with("org.roam.") || roam_follow {
            let command = if roam_follow { commands::ROAM_NODE_FIND } else { params.command.as_str() };
            let context = argument.and_then(|value| serde_json::from_value(value).ok()).unwrap_or_default();
            return self.roam_command(command, &context);
        }
        if params.command == commands::LINK_PREVIEW || params.command == commands::LATEX_PREVIEW {
            let field = |name: &str| argument.as_ref().and_then(|value| value.get(name)).cloned();
            let startup = field("startup").and_then(|value| value.as_bool()) == Some(true);
            let color = |name: &str| field(name).and_then(|value| value.as_str().map(str::to_string));
            let colors = (color("foreground"), color("background"));
            let context: Context = argument
                .and_then(|value| serde_json::from_value(value).ok())
                .ok_or("This command needs the cursor position")?;
            return self.preview_command(&params.command, &context, startup, colors);
        }
        if matches!(
            params.command.as_str(),
            commands::NARROW_SUBTREE | commands::NARROW_BLOCK | commands::TREE_INDIRECT
        ) {
            let context: Context = argument
                .and_then(|value| serde_json::from_value(value).ok())
                .ok_or("This command needs the cursor position")?;
            return self.narrow(&params.command, &context);
        }
        // In a narrowed file, `C-c '` leaves it, as `C-x n w` does.
        let narrowed = argument
            .as_ref()
            .and_then(|value| value.pointer("/textDocument/uri")?.as_str()?.parse::<Uri>().ok())
            .and_then(|uri| uri::to_path(&uri))
            .is_some_and(|path| {
                self.special_edits.iter().any(|edit| edit.file == path && edit.kind == SpecialKind::Region)
            });
        if params.command == commands::NARROW_WIDEN || (params.command == commands::EDIT_SPECIAL && narrowed) {
            let context: Context = argument
                .and_then(|value| serde_json::from_value(value).ok())
                .ok_or("This command needs the cursor position")?;
            return self.widen(&context);
        }
        if params.command == commands::EDIT_SPECIAL || params.command == commands::TABLE_EDIT_FIELD {
            return self.edit_special(argument, params.command == commands::TABLE_EDIT_FIELD);
        }
        let context: Option<Context> = argument.and_then(|value| serde_json::from_value(value).ok());
        // Off a table, `C-c -` and `C-c *` turn lines into items and
        // headings (`org-ctrl-c-minus`, `org-ctrl-c-star`), in Emacs.
        if let Some(found) = context.as_ref().filter(|found| !self.is_view(&found.text_document.uri)) {
            let table_key = matches!(
                params.command.as_str(),
                commands::TABLE_HLINE | commands::TABLE_RECALCULATE | commands::TABLE_RECALCULATE_LINE
            );
            if table_key && !self.at_table(found) {
                let command =
                    if params.command == commands::TABLE_HLINE { "org-toggle-item" } else { "org-toggle-heading" };
                return self.emacs_run(command, found);
            }
        }
        // On a selection, the commands Org gives a meaning there run in
        // Emacs, with the selection as the active region.
        if let Some(found) = context.as_ref().filter(|found| Self::has_region(found)) {
            if let Some(command) = region_command(&params.command) {
                return self.emacs_run(command, found);
            }
        }
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
            commands::AGENDA_WRITE => self.agenda_write(context.as_ref()),
            commands::AGENDA_APPT => self.agenda_to_appt(context.as_ref()),
            commands::TABLE_TOGGLE_COLUMN_WIDTH => {
                self.toggle_column_width(&context.ok_or("This command needs the cursor position")?)
            }
            commands::COLUMNS => self.columns(&context.ok_or("This command needs the cursor position")?),
            id if id.starts_with("org.columns.") => {
                self.columns_command(id, &context.ok_or("This command needs the cursor position")?)
            }
            commands::AGENDA_BULK_ACTION => self.agenda_bulk_action(context.as_ref()),
            id if id.starts_with("org.agenda.bulk.") => self.agenda_bulk_mark(id, context.as_ref()),
            commands::PRIORITY => self.ask_priority(&context.ok_or("This command needs the cursor position")?),
            commands::REFILE => self.refile(&context.ok_or("This command needs the cursor position")?),
            commands::REFILE_COPY => {
                self.refile_as(&context.ok_or("This command needs the cursor position")?, RefileMode::Copy)
            }
            commands::REFILE_GOTO => {
                self.refile_as(&context.ok_or("This command needs the cursor position")?, RefileMode::Goto)
            }
            commands::REFILE_GOTO_LAST => self.refile_goto_last(),
            commands::OPEN => self.open_at_point(&context.ok_or("This command needs the cursor position")?),
            commands::LINK_STORE => self.link_store(&context.ok_or("This command needs the cursor position")?),
            commands::LINK_INSERT | commands::LINK_INSERT_LAST | commands::LINK_INSERT_ALL => {
                self.link_insert(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::ID_GET_CREATE => self.id_get_create(&context.ok_or("This command needs the cursor position")?),
            commands::CITE_INSERT | commands::CITE_INSERT_PREFIX => self.cite_insert(
                params.command == commands::CITE_INSERT_PREFIX,
                &context.ok_or("This command needs the cursor position")?,
            ),
            commands::TANGLE => self.tangle(&context.ok_or("This command needs the cursor position")?),
            commands::TABLE_FORMULA_DEBUGGER => {
                self.formula_debug = !self.formula_debug;
                let state = if self.formula_debug { "on" } else { "off" };
                self.message = Some(format!("Formula debugging has been turned {state}"));
                Ok(Value::Null)
            }
            commands::DETANGLE => self.detangle(&context.ok_or("This command needs the cursor position")?),
            commands::EXPORT => self.export(&context.ok_or("This command needs the cursor position")?),
            commands::HEADING_NEXT
            | commands::HEADING_PREVIOUS
            | commands::HEADING_FORWARD
            | commands::HEADING_BACKWARD
            | commands::HEADING_UP
            | commands::LINK_NEXT
            | commands::LINK_PREVIOUS => {
                self.motion(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::GOTO => self.goto(&context.ok_or("This command needs the cursor position")?),
            commands::FOOTNOTE_ACTION
            | commands::FOOTNOTE_MENU
            | commands::TOGGLE_ITEM
            | commands::TOGGLE_HEADING
            | commands::TOGGLE_COMMENT
            | commands::EMPHASIZE
            | commands::STRUCTURE_TEMPLATE
            | commands::DRAWER_INSERT
            | commands::SUBTREE_CLONE
            | commands::TABLE_COPY_DOWN
            | commands::TABLE_SUM
            | commands::TABLE_FIELD_INFO
            | commands::TABLE_CREATE
            | commands::TABLE_IMPORT
            | commands::TABLE_EXPORT
            | commands::BABEL_NEXT
            | commands::BABEL_PREVIOUS
            | commands::BABEL_GOTO
            | commands::BLOCK_NEXT
            | commands::BLOCK_PREVIOUS
            | commands::BABEL_LOB_INGEST
            | commands::TIMER_START
            | commands::TIMER_INSERT
            | commands::TIMER_ITEM
            | commands::TIMER_PAUSE
            | commands::TIMER_STOP
            | commands::TIMER_SET => {
                self.emacs_command(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::BABEL_EXECUTE => {
                self.emacs_run("org-babel-execute-maybe", &context.ok_or("This command needs the cursor position")?)
            }
            commands::BABEL_EXECUTE_BUFFER => {
                self.emacs_run("org-babel-execute-buffer", &context.ok_or("This command needs the cursor position")?)
            }
            commands::BABEL_EXECUTE_SUBTREE => {
                self.emacs_run("org-babel-execute-subtree", &context.ok_or("This command needs the cursor position")?)
            }
            commands::CRYPT_ENCRYPT
            | commands::CRYPT_DECRYPT
            | commands::CRYPT_ENCRYPT_ALL
            | commands::CRYPT_DECRYPT_ALL => {
                let command = match params.command.as_str() {
                    commands::CRYPT_ENCRYPT => "org-encrypt-entry",
                    commands::CRYPT_DECRYPT => "org-decrypt-entry",
                    commands::CRYPT_ENCRYPT_ALL => "org-encrypt-entries",
                    _ => "org-decrypt-entries",
                };
                self.emacs_run(command, &context.ok_or("This command needs the cursor position")?)
            }
            commands::BABEL_REMOVE_RESULT => self.emacs_run(
                "org-babel-remove-result-one-or-many",
                &context.ok_or("This command needs the cursor position")?,
            ),
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
            commands::REVEAL | commands::REVEAL_SIBLINGS | commands::REVEAL_TREE => {
                self.reveal(&params.command, &context.ok_or("This command needs the cursor position")?)
            }
            // In a table, `org-cut-special` and its neighbours cut, copy and
            // paste the cells of the region, or of the field (Emacs's own
            // clipboard, `org-table-clip`).
            commands::CUT_SUBTREE | commands::COPY_SUBTREE | commands::PASTE_SUBTREE
                if context.as_ref().is_some_and(|context| self.at_table(context)) =>
            {
                let command = match params.command.as_str() {
                    commands::CUT_SUBTREE => "org-table-cut-region",
                    commands::COPY_SUBTREE => "org-table-copy-region",
                    _ => "org-table-paste-rectangle",
                };
                self.emacs_run(command, &context.expect("a context"))
            }
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
                self.refile_capture(context.as_ref().and_then(refile_answer).as_deref(), text)
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
            commands::DBLOCK_INSERT => self.dblock_insert(&context.ok_or("This command needs the cursor position")?),
            commands::ATTACH => self.attach(&context.ok_or("This command needs the cursor position")?),
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
            commands::SCHEDULE_REMOVE | commands::DEADLINE_REMOVE => {
                self.remove_date(&params.command, &context.ok_or("This command needs the cursor position")?)
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
        if matches!(
            id,
            commands::JOURNAL_NEW_DATE_ENTRY
                | commands::JOURNAL_NEW_SCHEDULED_ENTRY
                | commands::JOURNAL_RESCHEDULE
                | commands::JOURNAL_SEARCH
        ) {
            return Ok(Target::Journal { from: uri.clone() });
        }
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
        if !matches!(
            id,
            commands::SCHEDULE
                | commands::DEADLINE
                | commands::SCHEDULE_REMOVE
                | commands::DEADLINE_REMOVE
                | commands::SCHEDULE_DELAY
                | commands::DEADLINE_WARNING
        ) {
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
            Target::Journal { .. } => Err("The journal's commands change their own files".into()),
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
        if let Target::Agenda { .. } | Target::Journal { .. } = target {
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
                let (value, layout) = (prompt.value.clone(), (self.calendar_months, self.calendar_week_start));
                let open = DatePrompt::new(id, command, target, prompt.default, value, now, &self.read_date, layout);
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
        // A command that asks for another date (a journal search's last
        // day) has the calendar open again.
        let asked_again = self.date_prompt.is_some();
        Ok(if pending.is_some() && !asked_again { json!({ "close": self.prompt_uri.as_str() }) } else { Value::Null })
    }

    /// Run the command `id` that asked for a date, at `target`, with
    /// `answer`.
    fn apply_date(&mut self, id: &str, target: &Target, answer: &str) -> std::result::Result<(), Refusal> {
        if let Target::Agenda { line, .. } = target {
            return self.goto_date(answer, *line);
        }
        if let Target::Journal { from } = target {
            return self.journal_date(id, from, answer);
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
        self.apply_planning(id, target, &source, outcome)
    }

    /// `org-schedule` and `org-deadline` with `C-u`: the entry's date of
    /// that kind taken out, and logged as `org_log_reschedule` and
    /// `org_log_redeadline` say.
    fn remove_date(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let target = self.date_target(id, context)?;
        let source = self.date_source(&target)?;
        let command =
            if id == commands::DEADLINE_REMOVE { PlanningCommand::Deadline } else { PlanningCommand::Schedule };
        let outcome = planning::remove(command, &source.text, source.offset)?;
        self.apply_planning(id, &target, &source, outcome)?;
        Ok(Value::Null)
    }

    /// The change a command that asks for a date made, with what it logs:
    /// the time at once, or a note asked for once the change is made.
    fn apply_planning(
        &mut self,
        id: &str,
        target: &Target,
        source: &DateSource,
        mut outcome: planning::Outcome,
    ) -> std::result::Result<(), Refusal> {
        let key = source.file.as_str().to_string();
        let text = &source.text;
        let wanted = self.log_planning(&mut outcome);
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
        // The note, in its buffer, for the entry where it now is.
        if let Some(changed) = wanted {
            let encoding = self.client.encoding;
            let position = LineIndex::new(&outcome.text).position(&outcome.text, changed.heading, encoding);
            let context: Context = serde_json::from_value(json!({
                "textDocument": { "uri": source.file.as_str() },
                "position": position,
            }))
            .map_err(|error| error.to_string())?;
            let wanted = org_core::todo::WantedNote { purpose: changed.change.purpose().to_string(), state: None };
            self.open_note(&context, id, wanted)?;
            if let Some(note) = &mut self.note {
                note.log = Some(NoteLog::Planning(changed.change, changed.old));
            }
        }
        Ok(())
    }

    /// `org-log-reschedule` and `org-log-redeadline` for the date
    /// `outcome` changed: the time logged in its text at once, or the
    /// change returned when it wants a note, which an editor without a
    /// note buffer logs without one.
    fn log_planning(&self, outcome: &mut planning::Outcome) -> Option<planning::Changed> {
        use org_core::todo;
        use org_syntax::Logging;
        let changed = outcome.changed.clone()?;
        let settings = &self.edit_settings.todo;
        let logging = todo::planning_logging(settings, &outcome.text, changed.change)?;
        if logging == Logging::Note && self.client.date_prompt && self.client.show_document {
            return Some(changed);
        }
        let keywords = org_syntax::parse(&outcome.text, &self.todo).todo_keywords().clone();
        let now = self.date_time();
        if let Ok(text) = todo::log_planning_change(
            &outcome.text,
            &keywords,
            changed.heading,
            settings,
            now,
            changed.change,
            &changed.old,
            "",
        ) {
            outcome.text = text;
        }
        None
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
        let journal = self.journal_days();
        if !journal.is_empty() {
            open.mark_journal(journal, self.date_time(), &self.read_date);
        }
        let cursor = open.cursor();
        if self.prompt_copy.is_some() {
            // The client has the document: it shows it again and fetches
            // the new text, and the cursor follows once it has.
            self.date_prompt = Some(open);
            self.refresh_prompt_document();
            let params = ShowDocumentParams {
                uri: self.prompt_uri.clone(),
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
            // A file is there before the client opens it.
            if self.prompt_file.is_some() {
                self.refresh_prompt_document();
            }
            let params = ShowDocumentParams {
                uri: self.prompt_uri.clone(),
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
            self.refresh_prompt_document();
        }
    }

    /// Have the client fetch the date prompt's new text, or write it to
    /// its file and tell the client to read that again.
    fn refresh_prompt_document(&mut self) {
        let Some(file) = &self.prompt_file else {
            let params = json!({ "uri": prompt::URI });
            self.send_raw_request(TEXT_DOCUMENT_CONTENT_REFRESH, params, None);
            return;
        };
        let Some(text) = self.date_prompt.as_ref().map(|open| open.text.clone()) else { return };
        let written = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| write_read_only(file, &text));
        match written {
            Err(error) => self.show_message(MessageType::ERROR, format!("Cannot write the date prompt: {error}")),
            Ok(()) => {
                let params = json!({ "uri": self.prompt_uri.as_str() });
                let notification = lsp_server::Notification::new(VIEW_CHANGED.to_string(), params);
                let _ = self.sender.send(Message::Notification(notification));
            }
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
            uri: self.prompt_uri.clone(),
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
            .filter(|context| context.text_document.uri == self.prompt_uri)
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
                return Ok(json!({ "close": self.prompt_uri.as_str() }));
            }
            other => return Err(format!("org-lsp has no command `{other}`").into()),
        }
        self.publish_date_prompt();
        Ok(Value::Null)
    }

    /// Run the command the date prompt answers, and close the prompt.
    fn finish_date_prompt(&mut self, answer: &str) -> std::result::Result<Value, Refusal> {
        let open = self.date_prompt.take().ok_or("No date prompt is open")?;
        let (id, target) = (open.id.clone(), open.target.clone());
        // The command was run in the file, not in the calendar: the file
        // stays unsaved, as Emacs leaves it (`auto_save` is for the others).
        if let Target::Place { uri, .. } = &target {
            self.origin = Some(uri.as_str().to_string());
        }
        if let Err(refusal) = self.apply_date(&id, &target, answer) {
            self.date_prompt.get_or_insert(open);
            return Err(refusal);
        }
        // A command that asks for another date (a journal search's last
        // day) has the calendar open again.
        if self.date_prompt.is_some() {
            return Ok(Value::Null);
        }
        Ok(json!({ "close": self.prompt_uri.as_str() }))
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
            commands::AGENDA_HABITS => {
                self.view.as_ref().ok_or("No agenda is open")?;
                let count = context.and_then(|context| context.count);
                let habits =
                    self.agenda.habits.as_mut().ok_or("Habits are off: `org-habit` is not in `org_modules`")?;
                if count.is_some_and(|count| count > 1) {
                    habits.show_all_today = !habits.show_all_today;
                } else {
                    habits.show_habits = !habits.show_habits;
                    self.message = Some(format!("Habits turned {}", if habits.show_habits { "on" } else { "off" }));
                }
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_COLUMNS => {
                let uri = self.view.as_ref().map(|open| open.uri.clone()).ok_or("No agenda is open")?;
                // The format is the one of the cursor's entry, as Org takes it.
                let start = context.as_ref().map_or(0, |context| context.position.line as usize);
                self.columns.insert(uri.as_str().to_string(), ColumnsOn { start, global: false, view: Value::Null });
                self.agenda_columns_view();
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

    /// Where a command runs, as the contexts of capture templates and
    /// agenda commands see it: the file, the major mode and the buffer's
    /// name. The agenda is `org-agenda-mode` and has no file; an Org file
    /// the server has is `org-mode`.
    fn here(&self, uri: Option<&Uri>) -> (Option<String>, &'static str, String) {
        match uri {
            Some(uri) if self.is_view(uri) => (None, "org-agenda-mode", "*Org Agenda*".to_string()),
            Some(uri) => {
                let file = uri::to_path(uri).map(|path| path.display().to_string());
                let mode = if self.document(uri).is_some() { "org-mode" } else { "fundamental-mode" };
                (file, mode, uri::file_name(uri))
            }
            None => (None, "fundamental-mode", "*scratch*".to_string()),
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
            self.agenda_filters = Default::default();
            self.agenda_marks.clear();
            self.open_view(Query::Tags { matching, todo_only: prefix == "M" })?;
            return Ok(Value::Null);
        }
        if get("ask").as_deref() == Some("search") {
            return self.search(input.unwrap_or_default(), prefix == "S");
        }
        let keys = format!("{prefix}{}", input.as_deref().unwrap_or(""));
        // The commands offered where the menu was asked for, each with the
        // key of the command it stands for.
        let uri = argument
            .and_then(|value| value.get("textDocument")?.get("uri")?.as_str())
            .and_then(|uri| uri.parse::<Uri>().ok());
        let (file, mode, buffer) = self.here(uri.as_ref());
        let here = org_core::contexts::Here { file: file.as_deref(), mode, buffer: &buffer };
        let all = self.agenda_options.custom_commands.clone().unwrap_or_default();
        let keyed: Vec<(String, String)> =
            all.iter().map(|command| (command.key.clone(), command.key.clone())).collect();
        let contexts = config::contexts(self.agenda_options.custom_commands_contexts.as_deref());
        let keyed = org_core::contexts::contextualize(
            &keyed,
            |(key, _)| key,
            |(_, used), key| (key.to_string(), used.clone()),
            &contexts,
            &here,
        )?;
        let commands: Vec<config::CustomCommandOption> = keyed
            .iter()
            .filter_map(|(key, used)| {
                let command = all.iter().find(|command| command.key == *used)?;
                Some(config::CustomCommandOption { key: key.clone(), ..command.clone() })
            })
            .collect();
        // The user's commands come first, as in Emacs.
        if custom::find(&commands, &keys).is_some() {
            let used = keyed.iter().find(|(key, _)| *key == keys).map_or(keys.clone(), |(_, used)| used.clone());
            let query = Query::Custom { keys: used, moved: None };
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
    /// `org-agenda-write` (`C-x C-w`): the agenda as plain text, or with
    /// an `.org` name its entries (each without its children, at the top
    /// level). Asks for the file, and whether to overwrite it.
    fn agenda_write(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let view = self.view.as_ref().ok_or("No agenda is open")?;
        let query = view.view.query.clone();
        let entries: Vec<views::EntryRef> = {
            let mut seen = Vec::new();
            for line in &view.view.lines {
                if let Some(entry) = &line.entry {
                    if !seen
                        .iter()
                        .any(|known: &views::EntryRef| known.uri == entry.uri && known.offset == entry.offset)
                    {
                        seen.push(entry.clone());
                    }
                }
            }
            seen
        };
        let Some(input) = context.and_then(|context| context.input.clone()) else {
            return Ok(json!({ "input": { "kind": "text", "prompt": "Write agenda to file", "value": "" } }));
        };
        let (name, confirmed) = match context.and_then(|context| context.answers.clone()) {
            Some(name) => (name, Some(matches!(input.as_str(), "y" | "Y" | " "))),
            None => (input, None),
        };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = match (name.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(&name),
        };
        let path = if path.is_absolute() { path } else { self.capture_file(&self.org_directory.clone()).join(path) };
        if path.exists() {
            match confirmed {
                None => {
                    return Ok(json!({ "input": {
                        "kind": "key",
                        "prompt": format!("Overwrite existing file {}? ", path.display()),
                        "value": "",
                        "choices": [{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }],
                        "context": { "answers": name },
                    } }));
                }
                Some(false) => return Err(format!("Cannot write agenda to file {}", path.display()).into()),
                Some(true) => {}
            }
        }
        let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or("").to_lowercase();
        let (text, done) = match extension.as_str() {
            "org" => {
                let settings = self.refile.2;
                let mut parts: Vec<String> = Vec::new();
                for entry in &entries {
                    let found = self
                        .with_source(&entry.uri, |text, _, _, outline, _| {
                            let found = views::resolve(entry, outline)?;
                            Some((text.to_string(), outline.entries[found].offset))
                        })
                        .flatten();
                    let part = found.map(|(text, offset)| {
                        let settings = refile::Settings { adapt: self.adapt_in(&text), ..settings };
                        refile::entry_at_top(&text, offset, &settings)
                    });
                    if let Some(part) = part.filter(|part| !parts.contains(part)) {
                        parts.push(part);
                    }
                }
                (parts.concat(), "Org file written to")
            }
            "html" | "htm" | "ps" | "pdf" | "ics" => {
                return Err(format!(
                "Writing the agenda as .{extension} is Emacs's (htmlize, ps-print, ox-icalendar); write .txt or .org"
            )
                .into())
            }
            _ => (self.agenda_text(query), "Plain text written to"),
        };
        if let Some(directory) = path.parent() {
            let _ = std::fs::create_dir_all(directory);
        }
        std::fs::write(&path, text)
            .map_err(|error| format!("Cannot write agenda to file {}: {error}", path.display()))?;
        self.show_message(MessageType::INFO, format!("{done} {}", path.display()));
        Ok(Value::Null)
    }

    /// The agenda of `query` as Org prints it (`org-batch-agenda`), links as
    /// written.
    fn agenda_text(&mut self, query: Query) -> String {
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
        views::org_text(&blocks, &named, search.as_ref(), self.now(), Some(&self.agenda_filters))
    }

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
        self.refile_as(context, RefileMode::Move)
    }

    /// [`Server::refile`] as `mode` says: moving, copying or going there.
    fn refile_as(&mut self, context: &Context, mode: RefileMode) -> std::result::Result<Value, Refusal> {
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
        // A selection: the region refiled, whole subtrees or none.
        let region = match self.region_of(context).filter(|_| !from_agenda && mode != RefileMode::Goto) {
            Some((start, end)) => {
                let document = self.document(uri).ok_or("The document is not open")?;
                Some(refile::region(&document.text, start, end)?)
            }
            None => None,
        };
        self.refile_from(file, offset, region, None, from_agenda, refile_answer(context).as_deref(), mode)
    }

    /// `org-refile-goto-last-stored` (`C-u C-u C-c C-w`): where the last
    /// refile put its entry.
    fn refile_goto_last(&mut self) -> std::result::Result<Value, Refusal> {
        let (uri, at, head) = self.last_refiled.clone().ok_or("No refile has put an entry anywhere yet")?;
        let encoding = self.client.encoding;
        let position = self
            .with_source(&uri, |text, index, _, outline, _| {
                // The headline, nearest where it was.
                let entry = outline
                    .entries
                    .iter()
                    .filter(|entry| entry.head == head)
                    .min_by_key(|entry| entry.offset.abs_diff(at))?;
                Some(index.position(text, entry.offset, encoding))
            })
            .flatten()
            .ok_or("The entry refiled last is gone")?;
        let selection = Some(Range::new(position, position));
        self.send_request::<ShowDocument>(
            ShowDocumentParams { uri, external: None, take_focus: Some(true), selection },
            None,
        );
        Ok(Value::Null)
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
    #[allow(clippy::too_many_arguments)]
    fn refile_from(
        &mut self,
        file: Uri,
        offset: usize,
        region: Option<(usize, usize)>,
        text: Option<String>,
        from_agenda: bool,
        answer: Option<&str>,
        mode: RefileMode,
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
        let mut all = refile::all_targets(&selections, &readable, current, outline_path);
        // `org-refile-target-verify-function`, in Emacs, at each headline.
        if let Some(function) = self.refile_verify.clone() {
            let mut kept = vec![true; all.len()];
            for (file, readable_file) in readable.iter().enumerate() {
                let mut offsets: Vec<usize> = all
                    .iter()
                    .filter(|(at, target)| *at == file && target.offset.is_some())
                    .filter_map(|(_, target)| target.offset)
                    .collect();
                offsets.sort_unstable();
                offsets.dedup();
                if offsets.is_empty() {
                    continue;
                }
                let text = readable_file.text;
                let points: Vec<usize> = offsets.iter().map(|at| text[..*at].chars().count() + 1).collect();
                let params =
                    json!({ "file": readable_file.path, "text": text, "function": function, "points": points });
                let said = self
                    .emacs
                    .call("refile-verify", params, std::time::Duration::from_secs(60))
                    .map_err(Refusal::Failed)?;
                let verdicts: Vec<bool> = said
                    .as_array()
                    .map(|all| all.iter().map(|one| one.as_bool() == Some(true)).collect())
                    .unwrap_or_default();
                for (index, (at, target)) in all.iter().enumerate() {
                    if *at != file {
                        continue;
                    }
                    if let Some(offset) = target.offset {
                        let place = offsets.iter().position(|known| *known == offset).expect("an offset");
                        kept[index] = verdicts.get(place).copied().unwrap_or(false);
                    }
                }
            }
            let mut verdicts = kept.into_iter();
            all.retain(|_| verdicts.next().unwrap_or(false));
        }
        if all.is_empty() {
            return Err("No refile targets".into());
        }
        let action = if mode == RefileMode::Copy { "Copy" } else { "Refile" };
        let Some(answer) = answer else {
            let prompt = if mode == RefileMode::Goto {
                "Goto".to_string()
            } else if from_agenda {
                format!("{action} to")
            } else if region.is_some() {
                format!("{action} region to")
            } else {
                format!(
                    "{action} subtree \"{}\" to",
                    refile::heading_text(readable[0].text, offset, readable[0].keywords)
                )
            };
            let choices: Vec<Value> =
                refile::names(&all).into_iter().map(|name| json!({ "key": name, "label": name })).collect();
            return Ok(json!({ "input": { "kind": "choice", "prompt": prompt, "value": "", "choices": choices } }));
        };
        // After a NUL, the answer to "Create new node …?".
        let (answer, confirmed) = match answer.split_once('\0') {
            Some((answer, yes)) => (answer, Some(matches!(yes, "y" | "Y" | " "))),
            None => (answer, None),
        };
        // `org-refile-allow-creating-parent-nodes`: a target named after an
        // existing one is made, in the text it goes in. One named so that
        // is not made refiles nothing, as in Org.
        let mut made: Option<(usize, String)> = None;
        let (target_file, target_name, target_offset) = match refile::find(&all, answer) {
            Some((file, target)) => (*file, target.name.clone(), target.offset),
            None => {
                let new_nodes = if mode == RefileMode::Goto { refile::NewNodes::No } else { self.refile_new_nodes };
                match refile::new_node(&all, answer).filter(|_| new_nodes != refile::NewNodes::No) {
                    Some(((file, parent), child)) => {
                        if new_nodes == refile::NewNodes::Confirm && confirmed.is_none() {
                            return Ok(json!({ "input": {
                                "kind": "key",
                                "prompt": format!("Create new node \"{child}\"? "),
                                "value": "",
                                "choices": [{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }],
                                "context": { "answers": answer },
                            } }));
                        }
                        if confirmed == Some(false) {
                            return Ok(Value::Null);
                        }
                        let (text, heading) = refile::new_child(readable[*file].text, parent.offset, &child);
                        made = Some((*file, text));
                        (*file, format!("{}/{child}", parent.name), Some(heading))
                    }
                    None if answer.rsplit_once('/').is_some_and(|(_, child)| !child.is_empty()) => {
                        return Ok(Value::Null);
                    }
                    None => return Err("Invalid target location".into()),
                }
            }
        };
        let made_in = made.as_ref().map(|(file, _)| *file);
        // The entry, after a new target made above it in its own file.
        let moved = |at: usize| match (&made, target_offset) {
            (Some((0, text)), Some(heading)) if heading - 1 <= at => at + text.len() - readable[0].text.len(),
            _ => at,
        };
        let offset = moved(offset);
        let region = region.map(|(start, end)| (moved(start), moved(end)));
        // Back from readable files to all of them.
        let target_file = position.iter().position(|at| *at == Some(target_file)).expect("a readable file");
        let made_text = made.map(|(_, text)| text);
        let source = if made_in == Some(0) { made_text.as_deref().expect("made") } else { readable[0].text };
        // The target file as the editor has it, and as the entry goes in.
        let other_now = (target_file != 0).then(|| loaded[target_file].as_ref().expect("read").text.as_str());
        let other = other_now.map(|now| {
            if made_in.is_some_and(|file| file != 0) {
                made_text.as_deref().expect("made")
            } else {
                now
            }
        });
        // `C-u`: to the target, nothing moved; the top of a file is its
        // start or end, as notes go there.
        if mode == RefileMode::Goto {
            let text = other.unwrap_or(source);
            let at = target_offset.unwrap_or(if settings.reverse_note_order { 0 } else { text.len() });
            let position = LineIndex::new(text).position(text, at, self.client.encoding);
            let selection = Some(Range::new(position, position));
            let uri = uris[target_file].clone();
            self.send_request::<ShowDocument>(
                ShowDocumentParams { uri, external: None, take_focus: Some(true), selection },
                None,
            );
            return Ok(Value::Null);
        }
        // The target file's `org-adapt-indentation`, as Emacs pastes there.
        let settings = refile::Settings { adapt: self.adapt_in(other.unwrap_or(source)), ..settings };
        let mut refiled =
            refile::refile(source, offset, region, other, target_offset, &settings, mode == RefileMode::Copy)?;
        // `org-log-refile`: the time logged in the refiled entry at once, or
        // a note asked for once it is there.
        let into = if refiled.target.is_some() { target_file } else { 0 };
        let mut wanted_note = None;
        // A region: the time, in the last headline Org set a note up for.
        let logged_at = match (refiled.pasted, region) {
            (Some(at), Some((start, end))) => {
                let text = refiled.target.as_ref().unwrap_or(&refiled.source);
                refile::region_log_heading(text, at, end - start)
            }
            (pasted, _) => pasted,
        };
        if let Some(at) = logged_at {
            use org_syntax::Logging;
            let text = refiled.target.as_mut().unwrap_or(&mut refiled.source);
            let todo_settings = &self.edit_settings.todo;
            match org_core::todo::refile_logging(todo_settings, text) {
                Some(Logging::Note) if region.is_none() && self.client.date_prompt && self.client.show_document => {
                    wanted_note =
                        Some((uris[into].clone(), LineIndex::new(text).position(text, at, self.client.encoding)));
                }
                Some(_) => {
                    let keywords = org_syntax::parse(text, &self.todo).todo_keywords().clone();
                    if let Ok(logged) =
                        org_core::todo::log_refile(text, &keywords, at, todo_settings, self.date_time(), "")
                    {
                        *text = logged;
                    }
                }
                None => {}
            }
        }
        let target_path = uri::to_path(&uris[target_file])
            .map_or_else(|| uris[target_file].as_str().to_string(), |path| path.display().to_string());
        let message = format!("{action} to \"{}\" in file {}: done", target_name, target_path);
        // For `org-refile-goto-last-stored`: the entry, by its headline.
        if let Some(at) = refiled.pasted {
            let text = refiled.target.as_ref().unwrap_or(&refiled.source);
            let line = &text[at..text[at..].find('\n').map_or(text.len(), |end| at + end)];
            let head = line.trim_start_matches('*').trim().to_string();
            self.last_refiled = Some((uris[into].clone(), at, head));
        }
        let mut documents = vec![(
            uris[0].clone(),
            loaded[0].as_ref().expect("read").version,
            self.text_edits(&as_now, &refiled.source),
        )];
        if let (Some(changed), Some(now)) = (&refiled.target, other_now) {
            let version = loaded[target_file].as_ref().expect("read").version;
            documents.push((uris[target_file].clone(), version, self.text_edits(now, changed)));
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
        if let Some((uri, position)) = wanted_note {
            let context: Context =
                serde_json::from_value(json!({ "textDocument": { "uri": uri.as_str() }, "position": position }))
                    .map_err(|error| error.to_string())?;
            let wanted = org_core::todo::WantedNote { purpose: "refiling".to_string(), state: None };
            self.open_note(&context, commands::REFILE, wanted)?;
            if let Some(note) = &mut self.note {
                note.log = Some(NoteLog::Refile);
            }
        }
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
        // The templates offered where the capture starts.
        let (file, mode, buffer) = self.here(context.text_document.as_ref().map(|document| &document.uri));
        let here = org_core::contexts::Here { file: file.as_deref(), mode, buffer: &buffer };
        let templates = org_core::contexts::contextualize(
            &self.capture_templates,
            |template| &template.key,
            |template, key| CaptureTemplateOption { key: key.to_string(), ..template.clone() },
            &self.capture_contexts,
            &here,
        )?;
        let answering = context.answers.clone();
        let mut keys = context.keys.clone().unwrap_or_default();
        if answering.is_none() {
            keys.push_str(context.input.as_deref().unwrap_or(""));
        }
        let chosen = templates.iter().find(|template| template.key == keys && !template.is_group());
        let Some(template) = chosen.cloned() else {
            let next = template_choices(&templates, &keys);
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
        // `:time-prompt`: the date of the tree entry is asked first, and
        // stands for the time of the capture.
        let now = self.date_time();
        let mut default_time = now;
        let mut tree_date = now.date();
        let mut asked = answers.as_slice();
        if template.time_prompt == Some(true) {
            let Some((answer, rest)) = answers.split_first() else {
                let context = json!({ "keys": keys, "answers": answers });
                return Ok(json!({ "input": {
                    "kind": "date",
                    "prompt": "Date for tree entry",
                    "value": "",
                    "context": context,
                } }));
            };
            let read = org_core::read_date::read_date(answer, None, now, &self.read_date)?;
            tree_date = read.date;
            let (hour, minute) = match read.time {
                Some(time) => time,
                None if read.date == now.date() => (now.minute / 60, now.minute % 60),
                None => (0, 0),
            };
            default_time = DateTime::new(read.date, hour, minute);
            asked = rest;
        }
        let document = context.text_document.as_ref().map(|document| document.uri.clone());
        let clipboard = context.clipboard.clone().unwrap_or_default();
        let fill_context = self.capture_fill_context(document.as_ref(), context.position, default_time, clipboard);
        let text = template_text(&template);
        // `%[file]`: relative to the directory of the file captured from.
        let directory = from.and_then(uri::to_path).and_then(|path| path.parent().map(Path::to_path_buf));
        let insert_file = |name: &str| capture_file_text(name, directory.as_deref());
        // `%(sexp)`: evaluated by Emacs once a capture, as the template's
        // questions come one at a time.
        if answers.is_empty() {
            self.capture_lisp.clear();
        }
        let mut evaluated = std::mem::take(&mut self.capture_lisp);
        let emacs = &self.emacs;
        let mut eval = |sexp: &str| -> std::result::Result<Option<String>, String> {
            if let Some(value) = evaluated.get(sexp) {
                return Ok(value.clone());
            }
            let params = json!({ "sexp": sexp, "directory": directory.as_ref().map(|path| path.to_string_lossy()) });
            let result = emacs.call("eval", params, std::time::Duration::from_secs(30))?;
            let value = result.as_str().map(str::to_string);
            evaluated.insert(sexp.to_string(), value.clone());
            Ok(value)
        };
        let outside = capture::Outside { insert_file: &insert_file, eval: &mut eval };
        let filled = capture::fill_with(&text, &fill_context, asked, &self.read_date, Some(outside));
        self.capture_lisp = evaluated;
        let filled = match filled? {
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
                    // The tags of the target file, or of the agenda files.
                    capture::Question::Tags { prompt, all_files } => {
                        let tags = if all_files {
                            self.agenda_tags()
                        } else {
                            let path = self.capture_file(template.target.get(1).map(String::as_str).unwrap_or(""));
                            let mut tags: Vec<String> = uri::from_path(&path)
                                .and_then(|uri| {
                                    self.with_source(&uri, |_, _, _, outline, _| {
                                        outline
                                            .entries
                                            .iter()
                                            .flat_map(|entry| entry.tags.iter().map(|tag| tag.name.clone()))
                                            .chain(outline.file_tags.iter().cloned())
                                            .collect::<Vec<String>>()
                                    })
                                })
                                .unwrap_or_default();
                            tags.sort();
                            tags.dedup();
                            tags
                        };
                        let prompt = prompt.unwrap_or_else(|| "Tags".to_string());
                        json!({ "input": {
                            "kind": "text", "prompt": prompt, "value": "", "completions": tags, "context": context,
                        } })
                    }
                    // `%^C`, `%^L`: the initial content or the clipboard.
                    capture::Question::Clipboard { default, choices, .. } => json!({ "input": {
                        "kind": "text",
                        "prompt": "Clipboard/kill value",
                        "value": default,
                        "completions": choices,
                        "context": context,
                    } }),
                    capture::Question::Property { name, default } => {
                        let prompt = if default.is_empty() {
                            format!("{name} value")
                        } else {
                            format!("{name} value (default {default})")
                        };
                        json!({ "input": { "kind": "text", "prompt": prompt, "value": "", "context": context } })
                    }
                });
            }
        };
        // The target.
        let (kind, spec) = capture_spec(&template)?;
        let (target_uri, target) = match spec {
            TargetSpec::File(target) => {
                let path = self.capture_file(template.target.get(1).map(String::as_str).unwrap_or(""));
                (uri::from_path(&path).ok_or("Cannot name the target file")?, target)
            }
            TargetSpec::DateTree(path, groups) => {
                let file = self.capture_file(template.target.get(1).map(String::as_str).unwrap_or(""));
                let uri = uri::from_path(&file).ok_or("Cannot name the target file")?;
                (uri, capture::Target::DateTree { path, date: tree_date, groups })
            }
            TargetSpec::Clock => {
                let running = self.running.clone().ok_or("No running clock that could be used as capture target")?;
                let (text, _, line) =
                    self.clock_line(&running).ok_or("No running clock that could be used as capture target")?;
                let heading =
                    Self::heading_above(&text, line).ok_or("No running clock that could be used as capture target")?;
                (running.uri, capture::Target::Heading(heading))
            }
            TargetSpec::Id(id) => {
                let (uri, heading) = self.find_id(&id).ok_or_else(|| format!("Cannot find target ID \"{id}\""))?;
                (uri, capture::Target::Heading(heading))
            }
            TargetSpec::Function { file, name } => {
                // `file+function`: run in the file as the editor has it.
                let mut params = json!({ "function": name });
                if file {
                    let path = self.capture_file(template.target.get(1).map(String::as_str).unwrap_or(""));
                    let uri = uri::from_path(&path).ok_or("Cannot name the target file")?;
                    let text = self.with_source(&uri, |text, _, _, _, _| text.to_string()).unwrap_or_default();
                    params["file"] = json!(path.to_string_lossy());
                    params["text"] = json!(text);
                }
                let found = self
                    .emacs
                    .call("capture-target", params, std::time::Duration::from_secs(60))
                    .map_err(Refusal::Failed)?;
                let path = found.get("file").and_then(Value::as_str).ok_or("The target function visited no file")?;
                let uri = uri::from_path(Path::new(path)).ok_or("Cannot name the target file")?;
                let point = found.get("point").and_then(Value::as_u64).unwrap_or(1) as usize;
                let text = self.with_source(&uri, |text, _, _, _, _| text.to_string()).unwrap_or_default();
                let at = text.char_indices().nth(point.saturating_sub(1)).map_or(text.len(), |(at, _)| at);
                (uri, capture::Target::Exact(at))
            }
        };
        let (mut snapshot, version, keywords) = self
            .with_source(&target_uri, |text, _, parse, _, version| {
                (text.to_string(), version, parse.todo_keywords().clone())
            })
            .unwrap_or_else(|| (String::new(), None, self.todo.clone()));
        // `:clock-in`: the clock that runs stops now; in the target file,
        // the capture goes in after that.
        let mut before = snapshot.clone();
        let clock = if template.clock_in == Some(true) && kind == capture::Kind::Entry {
            let mut interrupted = None;
            if let Some((change, entry)) = self.stop_clock_for_capture() {
                interrupted = Some(entry);
                if change.0 == target_uri {
                    snapshot = change.3.clone();
                    if template.immediate_finish != Some(true) {
                        self.send_texts(vec![change]);
                        before = snapshot.clone();
                    }
                } else {
                    self.send_texts(vec![change]);
                }
            }
            Some(CaptureClock {
                start: self.date_time(),
                keep: template.clock_keep == Some(true),
                resume: template.clock_resume == Some(true),
                interrupted,
            })
        } else {
            None
        };
        let properties = capture::Properties {
            prepend: template.prepend.unwrap_or(false),
            empty_lines: template.empty_lines,
            empty_lines_before: template.empty_lines_before,
            empty_lines_after: template.empty_lines_after,
            table_line_pos: template.table_line_pos.clone(),
        };
        let levels = refile::Settings { adapt: self.adapt_in(&snapshot), ..self.refile.2 };
        let mut placed = capture::place(&snapshot, &keywords, &target, kind, &filled, &properties, &levels)?;
        let jump = template.jump_to_captured == Some(true);
        if template.immediate_finish == Some(true) {
            let done = placed.finalize(placed.shown(), &keywords, &self.edit_settings.todo, self.date_time());
            let start = placed.begin;
            return self.file_capture(&target_uri, version, &before, done, start, &keywords, clock.as_ref(), jump);
        }
        let unnarrowed = template.unnarrowed == Some(true);
        if unnarrowed {
            placed.unnarrow();
        }
        let uri = self.show_capture(&target_uri, &placed)?;
        self.capture = Some(Capturing {
            uri,
            target: target_uri,
            snapshot,
            placed,
            spec: (target, kind, properties),
            clock,
            jump,
            unnarrowed,
            roam: None,
        });
        Ok(Value::Null)
    }

    /// The capture buffer: a file named after the target, in a directory
    /// of the server's own, one for each capture, so that an editor that
    /// still has the last one open does not mix them up. It shows what
    /// the capture places in its target, with the cursor where `%?` was.
    fn show_capture(&mut self, target_uri: &Uri, placed: &capture::Placed) -> std::result::Result<Uri, Refusal> {
        self.captures += 1;
        let directory = self.capture_directory.join(self.captures.to_string());
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        // It is an Org buffer whatever the target is called (Org's default
        // notes file is `~/.notes`), and editors tell that by the name.
        let mut name = format!("{}{}", CAPTURE_PREFIX, uri::file_name(target_uri));
        if !name.ends_with(".org") {
            name.push_str(".org");
        }
        let buffer = directory.join(name);
        std::fs::write(&buffer, placed.shown()).map_err(|error| error.to_string())?;
        let buffer_uri = uri::from_path(&buffer).ok_or("Cannot name the capture buffer")?;
        let shown = placed.shown();
        let cursor = LineIndex::new(shown).position(shown, placed.cursor - placed.start, self.client.encoding);
        let params = ShowDocumentParams {
            uri: buffer_uri.clone(),
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(cursor, cursor)),
        };
        self.send_request::<ShowDocument>(params, None);
        Ok(buffer_uri)
    }

    /// What `org-capture-fill-template` fills in for a capture started
    /// at `position` of `document` (a file, or an agenda line): `%f`, `%F`,
    /// `%a` and the link's properties, `%k` and `%K`.
    fn capture_fill_context(
        &mut self,
        document: Option<&Uri>,
        position: Option<Position>,
        now: DateTime,
        clipboard: String,
    ) -> capture::Context {
        let from = document.filter(|uri| !self.is_view(uri));
        let file = from.map(|uri| {
            (uri::file_name(uri), uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default())
        });
        let (annotation, link_props) = match (from, position) {
            (Some(uri), Some(position)) => self.link_with_props(uri, position),
            // From an agenda line: a link to its entry.
            (None, Some(position)) => {
                let entry = document
                    .and_then(|uri| self.view_line(uri, position.line))
                    .and_then(|(_, line)| line.entry.clone());
                let encoding = self.client.encoding;
                let found = entry.and_then(|reference| {
                    let at = self
                        .with_source(&reference.uri, |text, index, _, outline, _| {
                            views::resolve(&reference, outline)
                                .map(|entry| index.position(text, outline.entries[entry].offset, encoding))
                        })
                        .flatten()?;
                    Some((reference.uri, at))
                });
                match found {
                    Some((uri, at)) => self.link_with_props(&uri, at),
                    None => (String::new(), Vec::new()),
                }
            }
            _ => (String::new(), Vec::new()),
        };
        // `%k` and `%K`: the entry clocked now.
        let (mut clock_heading, mut clock_link) = (String::new(), String::new());
        if let Some(running) = self.running.clone() {
            if let Some(heading) =
                self.clock_line(&running).and_then(|(text, _, line)| Self::heading_above(&text, line))
            {
                clock_heading = self.entry_title(&running.uri, heading).unwrap_or_default();
                let encoding = self.client.encoding;
                let position =
                    self.with_source(&running.uri, |text, index, _, _, _| index.position(text, heading, encoding));
                if let Some(position) = position {
                    clock_link = self.link_to(&running.uri, position);
                }
            }
        }
        capture::Context {
            now,
            file,
            initial: String::new(),
            annotation,
            link_props,
            user_name: String::new(),
            clock_heading,
            clock_link,
            tags_column: self.edit_settings.todo.tags_column,
            clipboard,
        }
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
            log: None,
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
        let command = match (note.id.as_str(), &note.log) {
            (_, Some(NoteLog::Planning(change, old))) => {
                EditCommand::PlanningNote { change: *change, old: old.clone() }
            }
            (_, Some(NoteLog::Refile)) => EditCommand::RefileNote,
            (commands::ADD_NOTE, _) => EditCommand::Note,
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
        self.link_with_props(uri, position).0
    }

    /// [`Self::link_to`], with the link's properties (`%:keyword`).
    fn link_with_props(&mut self, uri: &Uri, position: Position) -> (String, Vec<(String, String)>) {
        let encoding = self.client.encoding;
        let Some((path, home)) = Self::link_file(uri) else { return (String::new(), Vec::new()) };
        let (settings, edit_settings, category) =
            (self.link_settings.clone(), self.edit_settings.clone(), uri::file_stem(uri));
        let made = self.with_source(uri, |text, index, parse, _, version| {
            let offset = index.offset(text, position, encoding);
            let property_context = edit::property_context(parse, &edit_settings, &category);
            let file = link::File { path: &path, home: home.as_deref() };
            link::store(text, offset, None, &file, &settings, false, false, &property_context, &mut link::new_uuid)
                .map(|made| (made, text.to_string(), version))
        });
        let Some(Ok((made, text, version))) = made else { return (String::new(), Vec::new()) };
        // An ID made for the link (`org_id_link_to_org_use_id` true).
        if let Some((edited, _)) = &made.edited {
            self.send_texts(vec![(uri.clone(), version, text, edited.clone())]);
        }
        (link::annotation(&made), made.props)
    }

    /// The path of the file a link command runs in, and the home
    /// directory, which `~` stands for in links.
    fn link_file(uri: &Uri) -> Option<(String, Option<String>)> {
        let path = uri::to_path(uri)?.display().to_string();
        Some((path, std::env::var("HOME").ok().filter(|home| !home.is_empty())))
    }

    /// `org-store-link` (`C-c l`): a link to the place at the cursor, or
    /// to the entry of an agenda line, first in the stored links. With
    /// `C-u` `org_link_context_for_files` is turned around.
    fn link_store(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let (uri, offset) = self.command_place(context)?;
        let (path, home) = Self::link_file(&uri).ok_or("This file has no name")?;
        let negate = context.count == Some(4);
        let region = self.region_of(context).filter(|_| !self.is_view(&context.text_document.uri));
        let (settings, edit_settings, category) =
            (self.link_settings.clone(), self.edit_settings.clone(), uri::file_stem(&uri));
        let (made, text, version) = self
            .with_source(&uri, |text, _, parse, _, version| {
                let property_context = edit::property_context(parse, &edit_settings, &category);
                let file = link::File { path: &path, home: home.as_deref() };
                link::store(
                    text,
                    offset,
                    region,
                    &file,
                    &settings,
                    true,
                    negate,
                    &property_context,
                    &mut link::new_uuid,
                )
                .map(|made| (made, text.to_string(), version))
            })
            .ok_or("Cannot read the file")?
            .map_err(Refusal::Failed)?;
        if let Some((edited, _)) = &made.edited {
            self.send_texts(vec![(uri.clone(), version, text, edited.clone())]);
        }
        let message = link::remember(&mut self.stored_links, &made);
        self.show_message(MessageType::INFO, message);
        Ok(Value::Null)
    }

    /// `org-insert-link` (`C-c C-l`), `org-insert-last-stored-link` (`C-c
    /// M-l`) and `org-insert-all-links` (`C-c C-M-l`). The questions of
    /// `org-insert-link` come back with `step` "link", the answers so far
    /// in `answers`.
    fn link_insert(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        if self.is_view(&uri) {
            return Err(Refusal::NotHere("This command works in Org files".to_string()));
        }
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let (text, version) = (document.text.clone(), document.version);
        let offset = document.index.offset(&text, context.position, encoding);
        let region = self.region_of(context);
        let (path, home) = Self::link_file(&uri).ok_or("This file has no name")?;
        let file = link::File { path: &path, home: home.as_deref() };
        // The other file an `id:` link's entry got its ID in.
        let mut elsewhere: Option<(Uri, Option<i32>, String, String)> = None;
        let inserted = if id == commands::LINK_INSERT {
            let prefix = match context.count {
                Some(4) => 1,
                Some(16) => 2,
                Some(64) => 3,
                _ => 0,
            };
            let mut answers: Vec<String> =
                context.answers.as_deref().and_then(|answers| serde_json::from_str(answers).ok()).unwrap_or_default();
            if context.step.as_deref() == Some("link") {
                match &context.input {
                    Some(input) => answers.push(input.clone()),
                    None => return Ok(Value::Null),
                }
            }
            let abbreviations = open::abbreviations(&text, &self.link_abbreviations);
            // What `id:` links complete to and are described by: this file,
            // and the files with IDs (`org-id-files`), read only when an
            // `id:` link is being inserted.
            let wants_ids = answers.iter().any(|answer| answer == "id" || answer.starts_with("id:"))
                || self.stored_links.first().is_some_and(|(link, _)| link.starts_with("id:"));
            let mut others: Vec<(Uri, Option<i32>, String, org_syntax::Parse)> = Vec::new();
            if wants_ids {
                let own = uri::to_path(&uri);
                for path in self.id_paths() {
                    if Some(&path) == own.as_ref() {
                        continue;
                    }
                    let Some((other, text)) = self.file_text(&path) else { continue };
                    if link::has_ids(&text) {
                        let version = self.document(&other).map(|document| document.version);
                        let parse = org_syntax::parse(&text, &self.todo);
                        others.push((other, version, text, parse));
                    }
                }
            }
            let parse = org_syntax::parse(&text, &self.todo);
            let keywords = parse.todo_keywords().clone();
            let names: Vec<String> =
                std::iter::once(&uri).chain(others.iter().map(|(other, ..)| other)).map(uri::file_name).collect();
            let paths: Vec<String> = std::iter::once(&uri)
                .chain(others.iter().map(|(other, ..)| other))
                .map(|file| {
                    uri::to_path(file).map_or_else(|| file.as_str().to_string(), |path| path.display().to_string())
                })
                .collect();
            let other_keywords: Vec<_> = others.iter().map(|(.., parse)| parse.todo_keywords().clone()).collect();
            let refile_files: Vec<refile::File> = (0..names.len())
                .map(|index| refile::File {
                    text: if index == 0 { &text } else { &others[index - 1].2 },
                    keywords: if index == 0 { &keywords } else { &other_keywords[index - 1] },
                    name: &names[index],
                    path: &paths[index],
                    title: None,
                })
                .collect();
            let targets = if wants_ids { link::id_targets(&refile_files) } else { Vec::new() };
            let entries = refile::names(&targets);
            let (edit_settings, category) = (self.edit_settings.clone(), uri::file_stem(&uri));
            let context = edit::property_context(&parse, &edit_settings, &category);
            let mut made_there = None;
            let mut id_entry = |answer: &str| {
                let (index, target) = refile::find(&targets, answer)?;
                let offset = target.offset?;
                if *index == 0 {
                    return Some(link::IdEntry::Here(offset));
                }
                let (other, version, other_text, other_parse) = &others[index - 1];
                let other_context = edit::property_context(other_parse, &edit_settings, &uri::file_stem(other));
                let (edited, id) = link::id_get_create(other_text, offset, &other_context, &mut link::new_uuid).ok()?;
                made_there = edited.map(|(edited, _)| (other.clone(), *version, other_text.clone(), edited));
                let heading = link::entry_heading(other_text, offset, &other_keywords[index - 1]);
                Some(link::IdEntry::There { id, heading })
            };
            // `org-id-find`, in this file and the files with IDs.
            let mut id_heading = |id: &str| {
                std::iter::once((&parse, text.as_str(), &keywords))
                    .chain(
                        others
                            .iter()
                            .zip(&other_keywords)
                            .map(|((_, _, text, parse), keywords)| (parse, text.as_str(), keywords)),
                    )
                    .find_map(|(parse, text, keywords)| {
                        open::find_id(parse, id).map(|offset| link::entry_heading(text, offset, keywords))
                    })
            };
            let mut outside = link::Outside {
                abbreviations: &abbreviations,
                id_entries: &entries,
                id_entry: &mut id_entry,
                id_heading: &mut id_heading,
                context: &context,
                new_id: &mut link::new_uuid,
            };
            let step = link::insert(
                &text,
                offset,
                region,
                &file,
                &self.stored_links,
                prefix,
                &answers,
                &self.link_settings,
                &mut outside,
            )
            .map_err(Refusal::Failed)?;
            elsewhere = made_there;
            match step {
                link::Step::Ask(question) => {
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": question.prompt,
                        "value": question.initial,
                        "completions": question.completions,
                        "context": { "step": "link", "answers": serde_json::to_string(&answers).unwrap_or_default() },
                    } }));
                }
                link::Step::Done(inserted) => inserted,
            }
        } else {
            let (count, keep, pre) = if id == commands::LINK_INSERT_LAST {
                (Some(context.count.unwrap_or(1).max(1) as usize), false, "")
            } else {
                (None, context.count == Some(4), "- ")
            };
            let inserted =
                link::insert_all(&text, offset, &file, &self.stored_links, count, keep, pre, "\n", &self.link_settings)
                    .map_err(Refusal::Failed)?;
            match inserted {
                Some(inserted) => inserted,
                None => {
                    self.show_message(MessageType::INFO, "No link to insert");
                    return Ok(Value::Null);
                }
            }
        };
        self.stored_links = inserted.stored;
        let mut documents = vec![(uri.clone(), Some(version), self.text_edits(&text, &inserted.text))];
        if let Some((other, other_version, before, after)) = elsewhere {
            self.edited.insert(other.as_str().to_string());
            documents.push((other, other_version, self.text_edits(&before, &after)));
        }
        let edit = self.workspace_edits(documents);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        let position = LineIndex::new(&inserted.text).position(&inserted.text, inserted.cursor, encoding);
        self.apply_edit(params, Pending::EditAndMove { uri, position });
        Ok(Value::Null)
    }

    /// `org-id-get-create`: the entry at the cursor gets an ID unless it
    /// has one.
    fn id_get_create(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let (uri, offset) = self.command_place(context)?;
        let (edit_settings, category) = (self.edit_settings.clone(), uri::file_stem(&uri));
        let (made, text, version) = self
            .with_source(&uri, |text, _, parse, _, version| {
                let property_context = edit::property_context(parse, &edit_settings, &category);
                link::id_get_create(text, offset, &property_context, &mut link::new_uuid)
                    .map(|made| (made, text.to_string(), version))
            })
            .ok_or("Cannot read the file")?
            .map_err(Refusal::Failed)?;
        let (edited, id) = made;
        if let Some((edited, _)) = edited {
            self.send_texts(vec![(uri, version, text, edited)]);
        }
        Ok(json!(id))
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
        let (todo, time) = (&self.edit_settings.todo, self.date_time());
        // A buffer that shows the whole file stands for all of it.
        let done = if now == capture.snapshot || (capture.unnarrowed && capture.roam.is_none()) {
            capture.placed.finalize(&edited, &keywords, todo, time)
        } else if capture.roam.is_some() {
            // org-roam's target has the node's `ID` and head besides.
            return Err("The target file has changed since the capture began".into());
        } else {
            let (target, kind, properties) = &capture.spec;
            let levels = refile::Settings { adapt: self.adapt_in(&now), ..self.refile.2 };
            let placed = capture::place(&now, &keywords, target, *kind, &edited, properties, &levels)?;
            placed.finalize(placed.shown(), &keywords, todo, time)
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
        let (now, version, done) = match self.captured(&capture, text) {
            Ok(captured) => captured,
            Err(refusal) => {
                self.capture = Some(capture);
                return Err(refusal);
            }
        };
        let keywords = org_syntax::parse(&done, &self.todo).todo_keywords().clone();
        let start = capture.placed.begin;
        let filed = capture.roam.is_some().then(|| done.clone());
        self.file_capture(
            &capture.target,
            version,
            &now,
            done,
            start,
            &keywords,
            capture.clock.as_ref(),
            capture.jump,
        )?;
        if let (Some(roam), Some(done)) = (&capture.roam, filed) {
            self.roam_finalize(roam, &capture.target, &done);
        }
        Ok(json!({ "close": capture.uri }))
    }

    /// The clock that runs stopped now, for a capture with `:clock-in`:
    /// the change to its file, for the caller to send, and its entry, to
    /// start it again with `:clock-resume`.
    #[allow(clippy::type_complexity)]
    fn stop_clock_for_capture(&mut self) -> Option<((Uri, Option<i32>, String, String), (Uri, String))> {
        let running = self.running.clone()?;
        let (text, version, line) = self.clock_line(&running)?;
        let heading = Self::heading_above(&text, line)?;
        let title = self.entry_title(&running.uri, heading)?;
        let now = self.date_time();
        let out = clock::clock_out(&text, line, now, &self.clock_settings).ok()?;
        self.clock_out_time = Some(now);
        self.running = None;
        self.publish_clock();
        Some(((running.uri.clone(), version, text, out.text), (running.uri, title)))
    }

    /// The captured text goes to its target (`org-capture-finalize`): with
    /// `:clock-in`, the entry that starts after `start` gets the clock of
    /// the capture, stopped now unless `:clock-keep`, and with
    /// `:clock-resume` the clock the capture stopped starts again; with
    /// `:jump-to-captured`, the target shows the entry.
    #[allow(clippy::too_many_arguments)]
    fn file_capture(
        &mut self,
        target: &Uri,
        version: Option<i32>,
        now: &str,
        done: String,
        start: usize,
        keywords: &TodoKeywords,
        clock: Option<&CaptureClock>,
        jump: bool,
    ) -> std::result::Result<Value, Refusal> {
        let start = start.min(done.len());
        let heading = start + done[start..].len() - done[start..].trim_start_matches(['\n', ' ', '\t']).len();
        let mut done = done;
        let mut resume = None;
        if let Some(clock) = clock {
            let time = self.date_time();
            let (clocked, _) = clock::clock_in(&done, keywords, heading, &self.clock_settings, clock.start)?;
            done = clocked;
            if clock.keep {
                let head = done[heading..].lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
                self.running = Some(RunningClock { uri: target.clone(), start: clock.start, head: head.clone() });
                self.last_clocked = Some((target.clone(), head));
                self.clock_pending = true;
            } else {
                let line = running_lines(&done)
                    .into_iter()
                    .find(|(line, start, _)| {
                        *start == clock.start && Self::heading_above(&done, *line) == Some(heading)
                    })
                    .map(|(line, ..)| line);
                if let Some(line) = line {
                    done = clock::clock_out(&done, line, time, &self.clock_settings)?.text;
                }
                if clock.resume {
                    resume = clock.interrupted.clone();
                }
            }
        }
        let position = LineIndex::new(&done).position(&done, heading, self.client.encoding);
        match resume {
            Some((uri, title)) => {
                // The clock the capture stopped starts again, in the same
                // edit as the capture when it is in the same file.
                let found = if uri == *target {
                    org_core::outline::Outline::new(&org_syntax::parse(&done, &self.todo), &done, "")
                        .entries
                        .iter()
                        .find(|entry| {
                            org_core::agenda::report::plain_heading(&entry.head, entry.todo.as_deref()) == title
                        })
                        .map(|entry| entry.offset)
                } else {
                    self.find_title(&uri, &title)
                };
                let change = (target.clone(), version, now.to_string(), done.clone());
                match found {
                    Some(at) => {
                        let time = self.date_time();
                        self.clock_in_after(uri, at, time, vec![change])?;
                    }
                    None => self.send_texts(vec![change]),
                }
            }
            None => self.apply_capture(target, version, now, &done),
        }
        if clock.is_some_and(|clock| clock.keep) {
            self.publish_clock();
        }
        if jump {
            let params = ShowDocumentParams {
                uri: target.clone(),
                external: None,
                take_focus: Some(true),
                selection: Some(Range::new(position, position)),
            };
            self.send_request::<ShowDocument>(params, None);
        }
        Ok(Value::Null)
    }

    /// `org-capture-refile` (`C-c C-w`): the entry goes to its target, and
    /// from there under a refile target.
    fn refile_capture(&mut self, answer: Option<&str>, text: Option<String>) -> std::result::Result<Value, Refusal> {
        let capture = self.capture.take().ok_or("No capture is going on")?;
        if capture.spec.1 != capture::Kind::Entry {
            self.capture = Some(capture);
            return Err("Refiling from a capture buffer makes only sense for `entry'-type templates".into());
        }
        let (_, _, done) = match self.captured(&capture, text) {
            Ok(captured) => captured,
            Err(refusal) => {
                self.capture = Some(capture);
                return Err(refusal);
            }
        };
        // The entry starts where the template went in, after any blank lines.
        let begin = capture.placed.begin.min(done.len());
        let start = begin + done[begin..].len() - done[begin..].trim_start_matches(['\n', ' ', '\t']).len();
        let uri = capture.uri.clone();
        let target = capture.target.clone();
        let result = self.refile_from(target, start, None, Some(done), false, answer, RefileMode::Move);
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
                commands::TODO_NEXT
                | commands::TODO_PREVIOUS
                | commands::TODO_NEXT_SET
                | commands::TODO_PREVIOUS_SET
                | commands::PRIORITY_UP
                | commands::PRIORITY_DOWN => actions::command(id).ok_or("Unknown command")?,
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
        let heads = self.todo_heads.get(&key).cloned().unwrap_or_default();
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
            let repeated = previous.as_ref() == Some(&last);
            // Cycling and switching sets start, on an entry without a
            // keyword, from the sequence it was last in.
            let keywords = parse.todo_keywords();
            let old_keyword = heading.and_then(|heading| headline_keyword(text, heading, keywords));
            let remembered = heading.and_then(|heading| heads.get(&heading).cloned());
            let command = match command {
                EditCommand::Todo(TodoTarget::Cycle { .. }) => {
                    EditCommand::Todo(TodoTarget::Cycle { head: remembered.clone(), repeated })
                }
                EditCommand::Todo(TodoTarget::NextSet { .. }) => {
                    EditCommand::Todo(TodoTarget::NextSet { head: remembered.clone() })
                }
                EditCommand::Todo(TodoTarget::PreviousSet { .. }) => {
                    EditCommand::Todo(TodoTarget::PreviousSet { head: remembered.clone() })
                }
                command => command,
            };
            let switching_sets =
                matches!(command, EditCommand::Todo(TodoTarget::NextSet { .. } | TodoTarget::PreviousSet { .. }));
            let call = Call {
                count,
                repeated,
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
                // The sequence to come back to when the keyword is gone.
                let forget = heading.map(|heading| {
                    let keyword = headline_keyword(&outcome.text, heading, keywords);
                    let head = old_keyword.as_deref().and_then(|old| keywords.head(old)).map(str::to_string);
                    (heading, keyword.is_none().then(|| head.or(remembered.clone())).flatten())
                });
                // `Keyword-Set 2/3: REPORT BUG | FIXED`, as Emacs says it.
                let set_message = switching_sets
                    .then(|| heading.and_then(|heading| headline_keyword(&outcome.text, heading, keywords)))
                    .flatten()
                    .and_then(|head| {
                        let (number, total) = keywords.set_number(&head)?;
                        Some(format!("Keyword-Set {number}/{total}: {}", keywords.sequence(&head).join(" ")))
                    });
                let message = set_message.or_else(|| outcome.message.clone()).or_else(|| {
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
                (edits, version, outcome.text, outcome.note, message, forget)
            });
            Ok::<_, Refusal>((last, Some(outcome)))
        });
        let (last, outcome) = found.ok_or("Cannot read the file of this entry")??;
        // Remembered even when the command was refused, as in Emacs.
        let heading = last.heading;
        self.last = Some(last);
        let Some(outcome) = outcome else { return Ok(Value::Null) };
        let (edits, version, new_text, wanted, message, forget) = outcome?;
        if let Some((heading, head)) = forget {
            let heads = self.todo_heads.entry(file.as_str().to_string()).or_default();
            match head {
                Some(head) => heads.insert(heading, head),
                None => heads.remove(&heading),
            };
        }
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
                if !matches!(
                    id,
                    commands::TODO
                        | commands::TODO_NEXT
                        | commands::TODO_PREVIOUS
                        | commands::TODO_NEXT_SET
                        | commands::TODO_PREVIOUS_SET
                ) =>
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
    /// Whether the cursor of `context` is on a timestamp
    /// (`org-at-timestamp-p` with `lax`).
    fn at_timestamp(&self, context: &Context) -> bool {
        self.document(&context.text_document.uri).is_some_and(|document| {
            let offset = document.index.offset(&document.text, context.position, self.client.encoding);
            org_core::stamp::Stamp::around(&document.text, offset).is_some()
        })
    }

    /// Whether the cursor of `context` is on a line of an Org table.
    fn at_table(&self, context: &Context) -> bool {
        let uri = &context.text_document.uri;
        let Some(document) = self.document(uri).filter(|_| !self.is_view(uri)) else { return false };
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        at_org_table(&document.text, &document.parse.syntax(), offset)
    }

    /// Whether `C-c C-c` at `context` is Babel's: on a source block or a
    /// `#+CALL:` line (`org-element-at-point`), or an inline source block
    /// or call (`org-element-context`).
    fn at_babel(&self, context: &Context) -> bool {
        let uri = &context.text_document.uri;
        let Some(document) = self.document(uri).filter(|_| !self.is_view(uri)) else { return false };
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        let element = org_core::indent::Elements::new(&document.text).at(offset);
        if element.is_some_and(|element| matches!(element.kind, "src-block" | "babel-call")) {
            return true;
        }
        let root = document.parse.syntax();
        let at = org_syntax::TextSize::from(offset as u32);
        let inline = |token: Option<org_syntax::SyntaxToken>| {
            token.is_some_and(|token| {
                std::iter::once(token.kind()).chain(token.parent_ancestors().map(|node| node.kind())).any(|kind| {
                    matches!(kind, org_syntax::SyntaxKind::INLINE_SRC_BLOCK | org_syntax::SyntaxKind::INLINE_BABEL_CALL)
                })
            })
        };
        let tokens = root.token_at_offset(at);
        inline(tokens.clone().right_biased()) || inline(tokens.left_biased())
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
            // Typed, each tag completed from the table (`org-set-tags-command`'s
            // `completing-read-multiple`): what follows the last `:`.
            let mut completions: Vec<String> = Vec::new();
            for entry in &table {
                if let tags::TagEntry::Tag(tag, _) = entry {
                    if !completions.contains(tag) {
                        completions.push(tag.clone());
                    }
                }
            }
            return match context.input.as_deref() {
                Some(answer) => set(self, tags::parse_tags(answer)),
                None => Ok(json!({ "input": {
                    "kind": "text", "prompt": "Tags", "value": tags::tag_string(&own),
                    "completions": completions, "separator": ":",
                } })),
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
            // In a table, off a timestamp: `org-table-move-cell-up` and
            // the rest, in Emacs.
            if self.at_table(&context) && !self.at_timestamp(&context) {
                let command = match id {
                    commands::SHIFT_UP => "org-table-move-cell-up",
                    commands::SHIFT_DOWN => "org-table-move-cell-down",
                    commands::SHIFT_LEFT => "org-table-move-cell-left",
                    _ => "org-table-move-cell-right",
                };
                return self.emacs_run(command, &context);
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
        // `C-c C-c` on a source block or a call evaluates it, in Emacs.
        if id == commands::CHECKBOX_TOGGLE && self.at_babel(&context) {
            return self.emacs_run("org-ctrl-c-ctrl-c", &context);
        }
        // `C-c C-c` in a table aligns it (`org-table-align`).
        if id == commands::CHECKBOX_TOGGLE && self.at_table(&context) {
            return self.table(org_core::table::Command::Align, &context);
        }
        if PROPERTY_COMMANDS.contains(&id) || (id == commands::CHECKBOX_TOGGLE && self.at_property_drawer(&context)) {
            return self.property_command(id, &context);
        }
        if id != commands::TODO {
            return self.edit_or_recalculate(&context, id);
        }
        let keys = self.fast_keys(&context)?;
        if keys.is_empty() {
            let cycle = TodoTarget::Cycle { head: None, repeated: false };
            return self.edit_with(&context, id, Some(EditCommand::Todo(cycle)), &Extra::default());
        }
        let Some(answer) = &context.input else {
            // Each keyword in its face, as `org-fast-todo-selection` shows
            // it (`org-get-todo-face`): its own face from
            // `org_todo_keyword_faces`, or else that of a TODO or a done
            // keyword, as the highlight of the token.
            let choices: Vec<Value> = keys
                .iter()
                .map(|(name, key, done)| {
                    let mut choice = json!({
                        "key": key.to_string(),
                        "label": name,
                        "highlight": if *done { "todoKeyword.done" } else { "todoKeyword" },
                    });
                    if let Some(face) = self.faces["todo"].get(name) {
                        choice["face"] = face.clone();
                    }
                    choice
                })
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
        let outcome = match org_core::tangle::tangle(&document.text, &document.parse, &path, &settings) {
            Ok(outcome) => outcome,
            // Emacs Lisp to evaluate, or a block to run: Emacs tangles.
            Err(message) if org_core::tangle::needs_emacs(&message) => {
                return self.emacs_run("org-babel-tangle", context);
            }
            Err(message) => return Err(message.into()),
        };
        for file in &outcome.files {
            write_tangled(file).map_err(|error| format!("{}: {error}", file.path.display()))?;
        }
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        self.message = Some(outcome.message(&name));
        Ok(Value::Null)
    }

    /// `org-babel-detangle`, by Emacs: asks which file to detangle, among
    /// those the file's blocks tangle to that are there (none asked when
    /// there is one, its name typed when there are none), then Emacs runs
    /// it on that file with this one as the editor has it; what it changes
    /// here comes back as an edit. Blocks of other files are left alone.
    fn detangle(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let document = self.document(uri).ok_or("This file is not open")?;
        let path = uri::to_path(uri).ok_or("Only a file on disk can be detangled")?;
        let chosen = match context.input.as_deref().filter(|input| !input.trim().is_empty()) {
            Some(input) => Some(path.parent().map_or_else(|| PathBuf::from(input), |dir| dir.join(input.trim()))),
            None => {
                let settings =
                    org_core::tangle::Settings { preserve_indentation: self.refile.2.src_preserve_indentation };
                let mut files: Vec<PathBuf> =
                    org_core::tangle::tangle(&document.text, &document.parse, &path, &settings)
                        .map(|outcome| {
                            outcome.files.into_iter().map(|file| file.path).filter(|file| file.exists()).collect()
                        })
                        .unwrap_or_default();
                files.dedup();
                match files.as_slice() {
                    [only] => Some(only.clone()),
                    [] => {
                        return Ok(json!({ "input": { "kind": "text", "prompt": "Detangle file: ", "value": "" } }));
                    }
                    _ => {
                        let dir = path.parent().unwrap_or(Path::new(""));
                        let choices: Vec<Value> = files
                            .iter()
                            .map(|file| {
                                let shown = file.strip_prefix(dir).unwrap_or(file).to_string_lossy().into_owned();
                                json!({ "key": shown, "label": shown })
                            })
                            .collect();
                        return Ok(json!({ "input": {
                            "kind": "choice", "prompt": "Detangle", "value": "", "choices": choices,
                        } }));
                    }
                }
            }
        };
        let file = chosen.ok_or("No file to detangle")?;
        if !file.exists() {
            return Err(format!("No file {}", file.display()).into());
        }
        self.emacs_run_with("org-lsp-bridge-detangle", vec![json!(file.to_string_lossy())], context)
    }

    /// `org-export-dispatch` (`C-c C-e`): Org's export menu, run by Emacs.
    /// Without an answer it asks for the first key: a backend from the
    /// menu Emacs makes of the backends it has, `P` to publish, or one of
    /// the options to turn on or off (`C-v` visible only, the editor's
    /// closed folds left out, `C-b` body only, `C-s` the subtree at
    /// the cursor, `C-f` force publishing). A backend's key asks for one of
    /// its entries (`step` is then the backend's key, `answers` the options
    /// on, as letters). The entry runs in Emacs on the document as the
    /// editor has it; what it makes comes later (see
    /// [`Server::emacs_event`]).
    fn export(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let path = uri::to_path(uri).ok_or("Only a file on disk can be exported")?;
        if self.document(uri).is_none() {
            return Err("This file is not open".into());
        }
        let menu = self.export_menu()?;
        let mut options = context.answers.clone().unwrap_or_default();
        let step = context.step.clone().filter(|step| !step.is_empty());
        let Some(key) = context.input.as_deref() else {
            return Ok(export_question(&menu, step.as_deref(), &options));
        };
        let option = match key {
            "\u{2}" => Some('b'),
            "\u{13}" => Some('s'),
            "\u{6}" => Some('f'),
            "\u{16}" => Some('v'),
            _ => None,
        };
        if let Some(option) = option {
            if options.contains(option) {
                options.retain(|letter| letter != option);
            } else {
                options.push(option);
            }
            return Ok(export_question(&menu, step.as_deref(), &options));
        }
        match step.as_deref() {
            None if key == "q" => Ok(Value::Null),
            None if key == "P" => Ok(export_question(&menu, Some("P"), &options)),
            // `#`: `org-export-insert-default-template`, for a backend.
            None if key == "#" => {
                let backends = self
                    .emacs
                    .call("export-backends", json!({}), std::time::Duration::from_secs(60))
                    .map_err(Refusal::Failed)?;
                let choices: Vec<Value> = backends
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|name| json!({ "key": name, "label": name }))
                    .collect();
                Ok(json!({ "input": {
                    "kind": "choice", "prompt": "Options category", "value": "", "choices": choices,
                    "context": { "step": "#", "answers": options },
                } }))
            }
            // `&`: `org-export-stack`, what the exports made, the last
            // first; the one picked shows (`org-export-stack-view`).
            None if key == "&" => {
                if self.export_stack.is_empty() {
                    return Err("The export stack is empty".into());
                }
                let choices: Vec<Value> = self
                    .export_stack
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, (label, file, _))| {
                        json!({ "key": index.to_string(), "label": format!("{label}: {}", file.display()) })
                    })
                    .collect();
                Ok(json!({ "input": {
                    "kind": "choice", "prompt": "Export stack", "value": "", "choices": choices,
                    "context": { "step": "&", "answers": options },
                } }))
            }
            Some("&") => {
                let (_, file, buffer) = key
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| self.export_stack.get(index))
                    .ok_or("No such export in the stack")?
                    .clone();
                self.show_exported(&file, buffer);
                Ok(Value::Null)
            }
            Some("#") => {
                let subtree = options.contains('s');
                self.emacs_run_with("org-lsp-bridge-export-template", vec![json!(key), json!(subtree)], context)
            }
            None => {
                let top = menu
                    .as_array()
                    .and_then(|tops| tops.iter().find(|top| top.get("key").and_then(Value::as_str) == Some(key)))
                    .ok_or_else(|| format!("No export command under {key}"))?;
                if top.get("entries").is_some_and(|entries| entries.as_array().is_some()) {
                    Ok(export_question(&menu, Some(key), &options))
                } else {
                    let label = top.get("label").and_then(Value::as_str).unwrap_or(key).to_string();
                    self.start_export(context, &path, key, None, None, &options, label)
                }
            }
            Some(_) if key == "q" => Ok(export_question(&menu, None, &options)),
            Some("P") => {
                let label = match key {
                    "f" => "Publishing the current file",
                    "p" => "Publishing the current project",
                    "a" => "Publishing all projects",
                    "x" => {
                        let projects = match &self.publish_projects {
                            Some(projects) => projects.clone(),
                            None => {
                                let projects = self
                                    .emacs
                                    .call("publish-projects", json!({}), std::time::Duration::from_secs(60))
                                    .map_err(Refusal::Failed)?;
                                self.publish_projects = Some(projects.clone());
                                projects
                            }
                        };
                        let choices: Vec<Value> = projects
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(|name| json!({ "key": name, "label": name }))
                            .collect();
                        if choices.is_empty() {
                            return Err("No projects in org-publish-project-alist".into());
                        }
                        return Ok(json!({ "input": {
                            "kind": "choice", "prompt": "Publish project", "value": "", "choices": choices,
                            "context": { "step": "P:x", "answers": options },
                        } }));
                    }
                    _ => return Err(format!("No publishing command under {key}").into()),
                };
                self.start_export(context, &path, "P", Some(key), None, &options, label.to_string())
            }
            Some("P:x") => {
                let label = format!("Publishing {key}");
                self.start_export(context, &path, "P", Some("x"), Some(key), &options, label)
            }
            Some(first) => {
                let entry = menu
                    .as_array()
                    .and_then(|tops| tops.iter().find(|top| top.get("key").and_then(Value::as_str) == Some(first)))
                    .and_then(|top| top.get("entries")?.as_array())
                    .and_then(|entries| {
                        entries.iter().find(|entry| entry.get("key").and_then(Value::as_str) == Some(key))
                    })
                    .ok_or_else(|| format!("No export command under {first} {key}"))?;
                let label = entry.get("label").and_then(Value::as_str).unwrap_or(key).to_string();
                let first = first.to_string();
                self.start_export(context, &path, &first, Some(key), None, &options, label)
            }
        }
    }

    /// The export menu, from Emacs the first time.
    fn export_menu(&mut self) -> std::result::Result<Value, Refusal> {
        if let Some(menu) = &self.export_menu {
            return Ok(menu.clone());
        }
        let menu =
            self.emacs.call("export-menu", json!({}), std::time::Duration::from_secs(120)).map_err(Refusal::Failed)?;
        self.export_menu = Some(menu.clone());
        Ok(menu)
    }

    /// Have Emacs run the menu entry `first` `second` on the document of
    /// `context`, at its cursor.
    #[allow(clippy::too_many_arguments)]
    fn start_export(
        &mut self,
        context: &Context,
        path: &Path,
        first: &str,
        second: Option<&str>,
        project: Option<&str>,
        options: &str,
        label: String,
    ) -> std::result::Result<Value, Refusal> {
        let encoding = self.client.encoding;
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, encoding);
        let point = document.text[..offset].chars().count() + 1;
        let names: Vec<&str> = options
            .chars()
            .filter_map(|letter| match letter {
                'b' => Some("body"),
                's' => Some("subtree"),
                'f' => Some("force"),
                'v' => Some("visible"),
                _ => None,
            })
            .collect();
        let agenda_files: Vec<String> =
            self.workspace.files().iter().map(|file| file.to_string_lossy().into_owned()).collect();
        let params = json!({
            "file": path.to_string_lossy(),
            "text": document.text,
            "point": point,
            "first": first,
            "second": second,
            "project": project,
            "options": names,
            "agenda_files": agenda_files,
            // What `C-v` (visible only) leaves out: the editor's closed folds.
            "folded": context.folded.clone().unwrap_or_default(),
        });
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, EmacsJob::Export(label.clone()));
        self.emacs.start(tag, "export", params);
        self.message = Some(format!("{label}…"));
        Ok(Value::Null)
    }

    /// What Emacs has for the server: a question to ask in the editor, or
    /// what an export made, to show.
    fn emacs_event(&mut self, event: crate::emacs::Event) {
        match event {
            crate::emacs::Event::Ask { prompt, answer } => {
                let actions = ["Yes", "No"]
                    .into_iter()
                    .map(|title| lsp_types::MessageActionItem { title: title.to_string(), properties: HashMap::new() })
                    .collect();
                let params = lsp_types::ShowMessageRequestParams {
                    typ: MessageType::INFO,
                    message: prompt,
                    actions: Some(actions),
                };
                self.send_request::<lsp_types::request::ShowMessageRequest>(
                    params,
                    Some(Pending::EmacsQuestion(answer)),
                );
            }
            crate::emacs::Event::Done { tag, result } => match (self.emacs_jobs.remove(&tag), result) {
                (Some(EmacsJob::Export(label)), Err(error)) => {
                    self.show_message(MessageType::ERROR, format!("{label}: {error}"));
                }
                (Some(EmacsJob::Export(label)), Ok(result)) => self.exported(&label, &result),
                (Some(EmacsJob::Edit { .. }), Err(error)) => self.show_message(MessageType::ERROR, error),
                (Some(EmacsJob::LatexPreview { .. }), Err(error)) => self.show_message(MessageType::ERROR, error),
                (Some(EmacsJob::LatexPreview { uri, version, text, preview }), Ok(result)) => {
                    self.latex_previewed(&uri, version, &text, preview, &result);
                }
                (Some(EmacsJob::Edit { uri, version, text, point }), Ok(result)) => {
                    // Every command says how long the countdown has left,
                    // and how the timer shows.
                    self.countdown = result
                        .get("countdown")
                        .and_then(Value::as_f64)
                        .map(|left| std::time::Instant::now() + std::time::Duration::from_secs_f64(left));
                    self.timer_said(&result);
                    self.emacs_edited(&uri, version, &text, point, &result);
                    // What the command opened (an attachment, its
                    // directory), the system opens; what it visited (a new
                    // attachment), the editor.
                    let opened = |key: &str| {
                        result.get(key).and_then(Value::as_str).and_then(|path| uri::from_path(Path::new(path)))
                    };
                    if let Some(target) = opened("opened") {
                        self.show_external(target);
                    }
                    if let Some(target) = opened("visited") {
                        let params =
                            ShowDocumentParams { uri: target, external: None, take_focus: Some(true), selection: None };
                        self.send_request::<ShowDocument>(params, None);
                    }
                }
                (Some(EmacsJob::ColumnsView { uri, version }), result) => {
                    if self.document(&uri).map(|document| document.version) == Some(version) {
                        match result {
                            Ok(view) => self.columns_shown(&uri, version, Some(view)),
                            Err(error) => {
                                self.columns_shown(&uri, version, None);
                                self.show_message(MessageType::ERROR, error);
                            }
                        }
                    }
                }
                (Some(EmacsJob::AgendaColumns { uri, text }), result) => {
                    let current = self.view.as_ref().is_some_and(|open| open.uri == uri && open.view.text == text);
                    if current && self.columns.contains_key(uri.as_str()) {
                        match result {
                            Ok(view) => self.columns_shown(&uri, 0, Some(view)),
                            Err(error) => {
                                self.columns_shown(&uri, 0, None);
                                self.show_message(MessageType::ERROR, error);
                            }
                        }
                    }
                }
                (Some(EmacsJob::AgendaColumnsCommand { .. }), Err(error)) => {
                    self.show_message(MessageType::ERROR, error)
                }
                (Some(EmacsJob::AgendaColumnsCommand { uri, files }), Ok(result)) => {
                    self.agenda_columns_done(&uri, files, &result);
                }
                (Some(EmacsJob::Columns { .. }), Err(error)) => self.show_message(MessageType::ERROR, error),
                (Some(EmacsJob::Columns { uri, version, text }), Ok(mut result)) => {
                    // The cursor stays where it is; the view says the rest.
                    if let Some(fields) = result.as_object_mut() {
                        fields.remove("point");
                    }
                    let view = result.get("extra").cloned().filter(|view| !view.is_null());
                    self.columns_shown(&uri, version + 1, view);
                    self.emacs_edited(&uri, version, &text, 0, &result);
                }
                (Some(EmacsJob::WriteBack { file, uri, version, text, content }), result) => {
                    self.written_back(&file, &uri, version, &text, content, result);
                }
                (None, _) => {}
            },
        }
    }

    /// `org-clock-notify-once-if-expired`: when the clock that runs, as
    /// `status` describes it, reaches the effort of its entry, Org says so,
    /// once for each clock.
    fn arm_overrun(&mut self, status: &Value) {
        self.overrun = None;
        let Some(running) = &self.running else {
            self.overrun_said = None;
            return;
        };
        if self.overrun_said == Some(running.start) {
            return;
        }
        let (Some(minutes), Some(effort)) =
            (status.get("minutes").and_then(Value::as_i64), status.get("effort").and_then(Value::as_i64))
        else {
            return;
        };
        if effort <= 0 {
            return;
        }
        let heading = status.get("heading").and_then(Value::as_str).unwrap_or_default();
        let written = status.get("effortText").and_then(Value::as_str).unwrap_or_default();
        let message = format!("Task \u{2018}{heading}\u{2019} should be finished by now. ({written})");
        let left = std::time::Duration::from_secs(60 * (effort - minutes).max(0) as u64);
        self.overrun = Some((std::time::Instant::now() + left, message));
    }

    /// The clock passed the effort: what Org says (see
    /// [`Server::arm_overrun`]).
    fn overrun_done(&mut self) {
        if let Some((_, message)) = self.overrun.take() {
            self.overrun_said = self.running.as_ref().map(|running| running.start);
            self.show_message(MessageType::INFO, message);
        }
    }

    /// `org-startup-shrink-all-tables` (`#+STARTUP: shrink`): the columns
    /// with a width cookie shrunk when a file opens.
    fn shrink_at_startup(&mut self, key: &str, text: &str) {
        let mut shrink = self.startup_shrink;
        for line in text.lines() {
            let line = line.trim_start();
            if !line.get(..10).is_some_and(|start| start.eq_ignore_ascii_case("#+startup:")) {
                continue;
            }
            for word in line[10..].split_whitespace() {
                match word.to_ascii_lowercase().as_str() {
                    "shrink" => shrink = true,
                    "noshrink" => shrink = false,
                    _ => {}
                }
            }
        }
        if !shrink {
            return;
        }
        let tables = org_core::shrink::tables(text);
        let sets = tables.iter().map(|table| org_core::shrink::cookie_columns(text, table)).collect();
        self.shrunk.insert(key.to_string(), sets);
    }

    /// What the shrunk columns of `key`'s tables show (see
    /// [`org_core::shrink`]).
    fn shrunk_overlays(&self, key: &str, text: &str) -> Vec<org_core::shrink::Overlay> {
        let Some(sets) = self.shrunk.get(key) else { return Vec::new() };
        let tables = org_core::shrink::tables(text);
        let mut out: Vec<org_core::shrink::Overlay> = tables
            .iter()
            .zip(sets)
            .flat_map(|(table, columns)| org_core::shrink::overlays(text, table, columns))
            .collect();
        out.sort_by_key(|overlay| overlay.start);
        out
    }

    /// After a change of `key` (whose text was `old`, the lines changed
    /// those of `changed`): a table changed shows its columns again, as
    /// Emacs takes its overlays away when the text under them changes; the
    /// others keep theirs.
    fn shrunk_after_change(&mut self, key: &str, old: &str, changed: Option<(usize, usize)>) {
        let Some(sets) = self.shrunk.remove(key) else { return };
        let Some(text) = self.documents.get(key).map(|document| document.text.clone()) else { return };
        let Some((first, last)) = changed else { return };
        let before = org_core::shrink::tables(old);
        let after = org_core::shrink::tables(&text);
        let mut kept: Vec<std::collections::BTreeSet<usize>> = vec![Default::default(); after.len()];
        let shift = after.len() as isize - before.len() as isize;
        for (index, table) in before.iter().enumerate() {
            let (start, end) = table.line_range(old);
            let set = sets.get(index).cloned().unwrap_or_default();
            if end < first {
                if let Some(slot) = kept.get_mut(index) {
                    *slot = set;
                }
            } else if start > last {
                let moved = index as isize + shift;
                if let Some(slot) = usize::try_from(moved).ok().and_then(|moved| kept.get_mut(moved)) {
                    *slot = set;
                }
            }
        }
        if kept.iter().any(|set| !set.is_empty()) {
            self.shrunk.insert(key.to_string(), kept);
        }
    }

    /// `org-table-toggle-column-width` (`C-c TAB`): see the command.
    fn toggle_column_width(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let key = context.text_document.uri.as_str().to_string();
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = document.text.clone();
        let offset = document.index.offset(&text, context.position, self.client.encoding);
        let tables = org_core::shrink::tables(&text);
        let index = tables
            .iter()
            .position(|table| table.lines.iter().any(|(start, end)| (*start..=*end).contains(&offset)))
            .ok_or("Not in a table")?;
        let table = &tables[index];
        let mut sets = self.shrunk.remove(&key).unwrap_or_default();
        sets.resize(tables.len(), Default::default());
        let shrunk = sets[index].clone();
        let line =
            table.lines.iter().find(|(start, end)| (*start..=*end).contains(&offset)).copied().unwrap_or_default();
        let before_cursor = &text[line.0..offset];
        let columns = before_cursor.matches('|').count() + before_cursor.matches('+').count();
        let fields = text[line.0..line.1].matches('|').count().max(1) - 1;
        let new = match context.count {
            Some(16) => Default::default(),
            Some(count) if count > 1 => org_core::shrink::cookie_columns(&text, table),
            _ if columns == 0 || columns > fields => {
                // Before the first column or past the last: which ones.
                let Some(input) = context.input.clone() else {
                    self.shrunk.insert(key, sets);
                    return Ok(
                        json!({ "input": { "kind": "text", "prompt": "Column ranges (e.g. 2-4 6-): ", "value": "" } }),
                    );
                };
                let asked = column_ranges(&input, fields);
                shrunk.symmetric_difference(&asked).copied().collect()
            }
            _ => shrunk.symmetric_difference(&[columns].into_iter().collect()).copied().collect(),
        };
        sets[index] = new;
        if sets.iter().any(|set| !set.is_empty()) {
            self.shrunk.insert(key, sets);
        }
        if self.client.tokens_refresh {
            self.send_raw_request("workspace/semanticTokens/refresh", Value::Null, None);
        }
        self.refresh_inlay_hints();
        self.publish_display(context.text_document.uri.as_str());
        Ok(Value::Null)
    }

    /// `org-columns` (`C-c C-x C-c`), through Emacs: column view, on the
    /// whole file with `C-u` or before the first headline, else on the
    /// subtree Org takes (the first one above with `COLUMNS`, or the top
    /// headline). The editor draws what it shows (see [`COLUMNS_SHOWN`]).
    fn columns(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        uri::to_path(&uri).ok_or("Only a file on disk can show column view through Emacs")?;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        let start = document.text[..offset].chars().count() + 1;
        let global = context.count.is_some_and(|count| count > 1);
        self.columns.insert(uri.as_str().to_string(), ColumnsOn { start, global, view: Value::Null });
        self.columns_view(&uri);
        Ok(Value::Null)
    }

    /// Asks Emacs what column view shows on `uri` now.
    fn columns_view(&mut self, uri: &Uri) {
        let Some(on) = self.columns.get(uri.as_str()) else { return };
        let (Some(path), Some(document)) = (uri::to_path(uri), self.document(uri)) else { return };
        let params = json!({
            "file": path.to_string_lossy(),
            "text": document.text,
            "point": on.start,
            "global": on.global,
        });
        let job = EmacsJob::ColumnsView { uri: uri.clone(), version: document.version };
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, job);
        self.emacs.start(tag, "columns", params);
    }

    /// What Emacs needs for column view of the agenda: its lines, each
    /// with the file and the headline of its entry, or marked as a date
    /// line or a block's header; the line it was turned on at; and the
    /// files, as the editor has them.
    /// Also the agenda's URI and each file's URI, version when open and
    /// text, to apply what a command changes.
    fn agenda_columns_params(&self) -> Option<(Uri, Value, SentFiles)> {
        let open = self.view.as_ref()?;
        let view = &open.view;
        let mut files: SentFiles = Vec::new();
        let mut lines = Vec::new();
        for (number, text) in view.text.lines().enumerate() {
            let line = view.lines.get(number);
            let entry = line.and_then(|line| line.entry.as_ref());
            let mut item = json!({ "text": text });
            if let Some(entry) = entry {
                let path = uri::to_path(&entry.uri)?;
                if !files.iter().any(|(uri, _, _)| *uri == entry.uri) {
                    let (uri, file_text) = self.file_text(&path)?;
                    let version = self.document(&uri).map(|document| document.version);
                    files.push((uri, version, file_text));
                }
                let file_text = &files.iter().find(|(uri, _, _)| *uri == entry.uri)?.2;
                let offset = entry.offset.min(file_text.len());
                if file_text.is_char_boundary(offset) {
                    item["file"] = json!(path.to_string_lossy());
                    item["point"] = json!(file_text[..offset].chars().count() + 1);
                }
            } else if !text.is_empty() && !text.starts_with(' ') {
                if line.is_some_and(|line| line.day.is_some()) {
                    item["date"] = json!(true);
                } else {
                    item["structure"] = json!(true);
                }
            }
            lines.push(item);
        }
        let sent: Vec<Value> = files
            .iter()
            .filter_map(|(uri, _, text)| Some(json!({ "file": uri::to_path(uri)?.to_string_lossy(), "text": text })))
            .collect();
        let start = self.columns.get(open.uri.as_str()).map_or(0, |on| on.start);
        Some((open.uri.clone(), json!({ "lines": lines, "files": sent, "start": start }), files))
    }

    /// Asks Emacs what column view shows in the agenda now, when it is on.
    fn agenda_columns_view(&mut self) {
        let Some(uri) = self.view.as_ref().map(|open| open.uri.clone()) else { return };
        if !self.columns.contains_key(uri.as_str()) {
            return;
        }
        let Some((uri, params, _)) = self.agenda_columns_params() else { return };
        let text = self.view.as_ref().map(|open| open.view.text.clone()).unwrap_or_default();
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, EmacsJob::AgendaColumns { uri, text });
        self.emacs.start(tag, "agenda-columns", params);
    }

    /// A column view command in the agenda (`org-columns-map` there): the
    /// value of the entry of the cursor's line changed in its file, or
    /// shown. Moving, narrowing and deleting columns are left out: they
    /// change the format the agenda's view alone has.
    fn agenda_columns_command(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        let on = self.columns.get(uri.as_str()).cloned().ok_or("Column view is off")?;
        match id {
            commands::COLUMNS_QUIT => {
                self.columns_shown(&uri, 0, None);
                return Ok(Value::Null);
            }
            commands::COLUMNS_REDO => {
                self.agenda_columns_view();
                return Ok(Value::Null);
            }
            _ => {}
        }
        let command = match id {
            commands::COLUMNS_EDIT => "org-columns-edit-value",
            commands::COLUMNS_NEXT => "org-columns-next-allowed-value",
            commands::COLUMNS_PREVIOUS => "org-columns-previous-allowed-value",
            commands::COLUMNS_SHOW => "org-columns-show-value",
            commands::COLUMNS_ALLOWED => "org-columns-edit-allowed",
            commands::COLUMNS_TODO => "org-columns-todo",
            _ => return Err(Refusal::NotHere("Not in the agenda's column view".to_string())),
        };
        let column = context.column.unwrap_or(0);
        let line = context.position.line as usize;
        let answer = match columns_question(id, &on.view, line, column) {
            Some((prompt, value)) => match context.input.clone() {
                Some(answer) => Some(answer),
                None => return Ok(json!({ "input": { "kind": "text", "prompt": prompt, "value": value } })),
            },
            None => None,
        };
        let (uri, mut params, files) = self.agenda_columns_params().ok_or("No agenda is open")?;
        params["line"] = json!(line);
        params["column"] = json!(column);
        params["command"] = json!(command);
        if let Some(answer) = answer {
            params["answer"] = json!(answer);
        }
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, EmacsJob::AgendaColumnsCommand { uri, files });
        self.emacs.start(tag, "agenda-columns-command", params);
        Ok(Value::Null)
    }

    /// What a column view command in the agenda did: what it said, the
    /// view, and what it changed in the files, applied there unless they
    /// changed meanwhile. The agenda is redone then, and its column view
    /// with it.
    fn agenda_columns_done(&mut self, uri: &Uri, files: SentFiles, result: &Value) {
        if let Some(message) = result.get("message").and_then(Value::as_str).filter(|said| !said.is_empty()) {
            self.show_message(MessageType::INFO, message);
        }
        if let Some(view) = result.get("view").filter(|view| !view.is_null()) {
            self.columns_shown(uri, 0, Some(view.clone()));
        }
        let mut documents = Vec::new();
        for changed in result.get("files").and_then(Value::as_array).into_iter().flatten() {
            let (Some(file), Some(text)) = (changed.get("file").and_then(Value::as_str), changed["text"].as_str())
            else {
                continue;
            };
            let Some((file_uri, version, old)) = files
                .iter()
                .find(|(file_uri, _, _)| uri::to_path(file_uri).is_some_and(|path| path == Path::new(file)))
            else {
                continue;
            };
            let now = self.file_text(Path::new(file)).map(|(_, text)| text);
            if now.as_deref() != Some(old.as_str())
                || self.document(file_uri).map(|document| document.version) != *version
            {
                self.show_message(
                    MessageType::WARNING,
                    format!("{file} changed while Emacs ran the command; nothing was changed. Run it again."),
                );
                continue;
            }
            match version {
                Some(version) => documents.push((file_uri.clone(), Some(*version), self.text_edits(old, text))),
                None => {
                    if let Err(error) = std::fs::write(file, text) {
                        self.show_message(MessageType::ERROR, format!("Cannot write {file}: {error}"));
                    }
                }
            }
        }
        documents.retain(|(_, _, edits)| !edits.is_empty());
        if documents.is_empty() {
            self.refresh_view();
            return;
        }
        for (document, _, _) in &documents {
            self.edited.insert(document.as_str().to_string());
        }
        let edit = self.workspace_edits(documents);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
    }

    /// Tells the editor what column view shows on `uri`, or with no `view`
    /// that it is off.
    fn columns_shown(&mut self, uri: &Uri, version: i32, view: Option<Value>) {
        let params = match view {
            Some(mut view) => {
                if let Some(on) = self.columns.get_mut(uri.as_str()) {
                    on.view = view.clone();
                } else {
                    return;
                }
                view["uri"] = json!(uri.as_str());
                view["version"] = json!(version);
                view
            }
            None => {
                self.columns.remove(uri.as_str());
                json!({ "uri": uri.as_str(), "version": version, "lines": null })
            }
        };
        let notification = Notification::new(COLUMNS_SHOWN.to_string(), params);
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// The keys of column view (`org-columns-map`), on the column the
    /// editor says is picked: Org's commands, in Emacs, with what they ask
    /// asked first (`e`, `a`).
    fn columns_command(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        if self.is_view(&uri) {
            return self.agenda_columns_command(id, context);
        }
        let on = self.columns.get(uri.as_str()).cloned().ok_or("Column view is off")?;
        if id == commands::COLUMNS_QUIT {
            let version = self.document(&uri).map_or(0, |document| document.version);
            self.columns_shown(&uri, version, None);
            return Ok(Value::Null);
        }
        if id == commands::COLUMNS_REDO {
            self.columns_view(&uri);
            return Ok(Value::Null);
        }
        let command = match id {
            commands::COLUMNS_EDIT => "org-columns-edit-value",
            commands::COLUMNS_NEXT => "org-columns-next-allowed-value",
            commands::COLUMNS_PREVIOUS => "org-columns-previous-allowed-value",
            commands::COLUMNS_SHOW => "org-columns-show-value",
            commands::COLUMNS_ALLOWED => "org-columns-edit-allowed",
            commands::COLUMNS_TODO => "org-columns-todo",
            commands::COLUMNS_TOGGLE => "org-columns-toggle-or-columns-quit",
            commands::COLUMNS_MOVE_LEFT => "org-columns-move-left",
            commands::COLUMNS_MOVE_RIGHT => "org-columns-move-right",
            commands::COLUMNS_DELETE => "org-columns-delete",
            commands::COLUMNS_NARROW => "org-columns-narrow",
            commands::COLUMNS_WIDEN => "org-columns-widen",
            commands::COLUMNS_ROW_UP => "org-columns-move-row-up",
            commands::COLUMNS_ROW_DOWN => "org-columns-move-row-down",
            _ => return Err(format!("No such command: {id}").into()),
        };
        let column = context.column.unwrap_or(0);
        let path = uri::to_path(&uri).ok_or("Only a file on disk can show column view through Emacs")?;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let line = context.position.line as usize;
        let answer = match columns_question(id, &on.view, line, column) {
            Some((prompt, value)) => match context.input.clone() {
                Some(answer) => Some(answer),
                None => return Ok(json!({ "input": { "kind": "text", "prompt": prompt, "value": value } })),
            },
            None => None,
        };
        // The start of the cursor's line: Emacs's columns are its first
        // characters, padded as long as the line is too short.
        let line_start = document.index.offset(&document.text, Position::new(line as u32, 0), self.client.encoding);
        let point = document.text[..line_start].chars().count() + 1;
        let mut args = vec![json!(on.start), json!(on.global), json!(column), json!(command)];
        if let Some(answer) = answer {
            args.push(json!(answer));
        }
        let params = json!({
            "file": path.to_string_lossy(),
            "text": document.text,
            "point": point,
            "command": "org-lsp-bridge-columns-command",
            "args": args,
        });
        let job = EmacsJob::Columns { uri: uri.clone(), version: document.version, text: document.text.clone() };
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, job);
        self.emacs.start(tag, "run", params);
        Ok(Value::Null)
    }

    /// `org-edit-special` (`C-c '`), through Emacs. What Org edits in a
    /// buffer of its own goes in a file of its own, shown, its text as Org
    /// puts it there (indentation and commas taken away; a table's
    /// formulas one by line, `org-table-edit-formulas`); at `#+INCLUDE:`
    /// and the like, the file is shown; at a date, a planning line or a
    /// link, the command Org runs there runs.
    /// With `table_field`, `org-table-edit-field` (C-c and a backquote): the field at the
    /// cursor in a file of its own, put back when it is saved.
    fn edit_special(&mut self, argument: Option<Value>, table_field: bool) -> std::result::Result<Value, Refusal> {
        let context: Context = argument
            .clone()
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or("This command needs the cursor position")?;
        let uri = context.text_document.uri.clone();
        let path = uri::to_path(&uri).ok_or("Only a file on disk can be edited through Emacs")?;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        let point = document.text[..offset].chars().count() + 1;
        let params = json!({ "file": path.to_string_lossy(), "text": document.text, "point": point });
        let method = if table_field { "edit-field" } else { "edit-special" };
        let result = self.emacs.call(method, params, std::time::Duration::from_secs(60)).map_err(Refusal::Failed)?;
        if let Some(file) = result.get("visit").and_then(Value::as_str) {
            let target = uri::from_path(Path::new(file)).ok_or("This file cannot be shown")?;
            let params = ShowDocumentParams { uri: target, external: None, take_focus: Some(true), selection: None };
            self.send_request::<ShowDocument>(params, None);
            return Ok(Value::Null);
        }
        if let Some(other) = result.get("other").and_then(Value::as_str) {
            let command = match other {
                "deadline" => commands::DEADLINE,
                "schedule" => commands::SCHEDULE,
                "timestamp-inactive" => commands::TIMESTAMP_INSERT_INACTIVE,
                "link" => commands::OPEN,
                _ => commands::TIMESTAMP_INSERT,
            };
            let params = ExecuteCommandParams {
                command: command.to_string(),
                arguments: argument.into_iter().collect(),
                work_done_progress_params: Default::default(),
            };
            return self.execute_command(params);
        }
        let edit = result.get("edit").ok_or("No special environment to edit here")?;
        let field = |key: &str| edit.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let number = |key: &str| edit.get(key).and_then(Value::as_u64).unwrap_or(1) as usize;
        let (begin, end, region) = (number("begin"), number("end"), field("region"));
        let kind = match field("kind").as_str() {
            "formulas" => SpecialKind::Formulas,
            "field" => SpecialKind::Field,
            _ => SpecialKind::Element,
        };
        let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        self.open_special(NewSpecial {
            uri,
            stem,
            begin,
            end,
            region,
            content: field("text"),
            extension: field("extension"),
            name: field("name"),
            kind,
            point,
        })
    }

    /// Shows a part of an Org file in a file of its own, written back each
    /// time it is saved (see [`SpecialEdit`]); the same part again, its file
    /// again (a field, the same field).
    fn open_special(&mut self, new: NewSpecial) -> std::result::Result<Value, Refusal> {
        let same = |open: &&SpecialEdit| {
            open.uri == new.uri
                && open.begin == new.begin
                && open.region == new.region
                && open.kind == new.kind
                && open.file.exists()
                && (new.kind != SpecialKind::Field || open.point == new.point)
        };
        if let Some(open) = self.special_edits.iter().find(same) {
            let target = uri::from_path(&open.file).ok_or("This file cannot be shown")?;
            let params = ShowDocumentParams { uri: target, external: None, take_focus: Some(true), selection: None };
            self.send_request::<ShowDocument>(params, None);
            return Ok(Value::Null);
        }
        let directory = self.special_directory.join(self.special_edits.len().to_string());
        let suffix = if new.kind == SpecialKind::Region { "narrowed" } else { "edit" };
        let file = directory.join(format!("{}-{suffix}.{}", new.stem, new.extension));
        std::fs::create_dir_all(&directory)
            .and_then(|()| std::fs::write(&file, &new.content))
            .map_err(|error| Refusal::Failed(format!("Cannot write {}: {error}", file.display())))?;
        let target = uri::from_path(&file).ok_or("This file cannot be shown")?;
        let (begin, end, kind, point) = (new.begin, new.end, new.kind, new.point);
        self.special_edits.push(SpecialEdit {
            modified: std::fs::metadata(&file).and_then(|metadata| metadata.modified()).ok(),
            file,
            content: new.content,
            uri: new.uri,
            // In the part, Org edits it again from its start; at a footnote
            // reference, from there; a field, from in it.
            point: if kind != SpecialKind::Field && (begin..=end).contains(&point) { begin } else { point },
            begin,
            end,
            region: new.region,
            busy: false,
            kind,
        });
        let params = ShowDocumentParams { uri: target, external: None, take_focus: Some(true), selection: None };
        self.send_request::<ShowDocument>(params, None);
        self.message = Some(format!("{}: saving the file writes it back", new.name));
        Ok(Value::Null)
    }

    /// `widen` (`C-x n w`) in a narrowed file: what it says now, saved or
    /// not, goes back into the Org file, and it closes (the result has
    /// `close`), its file gone.
    fn widen(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = &context.text_document.uri;
        let path = uri::to_path(uri).ok_or("Not in a narrowed file")?;
        let index = self
            .special_edits
            .iter()
            .position(|edit| edit.file == path && edit.kind == SpecialKind::Region)
            .ok_or(Refusal::NotHere("Not in a narrowed file".to_string()))?;
        let content = match self.document(uri) {
            Some(document) => document.text.clone(),
            None => std::fs::read_to_string(&path).map_err(|_| "The narrowed file is gone")?,
        };
        if content != self.special_edits[index].content {
            // A narrowed part goes back at once, without Emacs.
            self.write_back(index, content);
        }
        let edit = self.special_edits.remove(index);
        let _ = std::fs::remove_file(&edit.file);
        self.widened.push(edit.file);
        Ok(json!({ "close": uri.as_str() }))
    }

    /// `org-narrow-to-subtree` (`C-x n s`), `org-narrow-to-block` (`C-x n
    /// b`) and `org-tree-to-indirect-buffer` (`C-c C-x b`): the subtree or
    /// the block at the cursor in a file of its own, all there is to see
    /// there, written back as it is each time it is saved.
    fn narrow(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        let path = uri::to_path(&uri).ok_or("Only a file on disk can be narrowed")?;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let text = &document.text;
        let offset = document.index.offset(text, context.position, self.client.encoding);
        let root = document.syntax();
        let range = if id == commands::NARROW_BLOCK {
            let token = root
                .token_at_offset(org_syntax::TextSize::from(offset as u32))
                .right_biased()
                .ok_or("Not in a block")?;
            token
                .parent_ancestors()
                .find(|node| {
                    matches!(node.kind(), org_syntax::SyntaxKind::BLOCK | org_syntax::SyntaxKind::DYNAMIC_BLOCK)
                })
                .ok_or("Not in a block")?
                .text_range()
        } else {
            org_syntax::ast::headline_at(&root, org_syntax::TextSize::from(offset as u32))
                .ok_or("Before first headline")?
                .syntax()
                .text_range()
        };
        let (start, finish) = (usize::from(range.start()), usize::from(range.end()));
        let region = text[start..finish].to_string();
        let begin = text[..start].chars().count() + 1;
        let end = begin + region.chars().count();
        let point = text[..offset].chars().count() + 1;
        let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let name = if id == commands::TREE_INDIRECT { format!("{stem}.org-1") } else { format!("{stem}.org") };
        self.open_special(NewSpecial {
            uri,
            stem,
            begin,
            end,
            content: region.clone(),
            region,
            extension: "org".to_string(),
            name,
            kind: SpecialKind::Region,
            point,
        })
    }

    /// Whether the files of [`Server::edit_special`] were saved: each that
    /// says something new is written back.
    fn check_special_edits(&mut self) {
        for index in 0..self.special_edits.len() {
            let edit = &mut self.special_edits[index];
            if edit.busy {
                continue;
            }
            let Ok(modified) = std::fs::metadata(&edit.file).and_then(|metadata| metadata.modified()) else {
                continue;
            };
            if edit.modified == Some(modified) {
                continue;
            }
            edit.modified = Some(modified);
            match std::fs::read_to_string(&edit.file) {
                Ok(content) if content != edit.content => self.write_back(index, content),
                _ => {}
            }
        }
    }

    /// `org-edit-src-save`: Emacs writes `content` back into the part
    /// `index` edits, unless that part changed in the Org file since.
    fn write_back(&mut self, index: usize, content: String) {
        let edit = &self.special_edits[index];
        let name = edit.file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(path) = uri::to_path(&edit.uri) else { return };
        let Some(document) = self.document(&edit.uri) else {
            let message = format!("{name}: {} is not open, so nothing was written back", path.display());
            self.show_message(MessageType::ERROR, message);
            return;
        };
        let text = document.text.clone();
        let version = document.version;
        let byte = |at: usize| text.char_indices().nth(at.saturating_sub(1)).map_or(text.len(), |(byte, _)| byte);
        let mut point = edit.point;
        if text.get(byte(edit.begin)..byte(edit.end)) != Some(edit.region.as_str()) {
            // Moved, by changes before it: found again when it is the only
            // such text, and edited from the same place in it.
            let found: Vec<usize> = text.match_indices(&edit.region).map(|(at, _)| at).collect();
            match found[..] {
                [at] if (edit.begin..=edit.end).contains(&edit.point) && !edit.region.is_empty() => {
                    point = text[..at].chars().count() + 1 + (edit.point - edit.begin);
                }
                _ => {
                    let message = format!(
                        "{name}: what it edits in {} changed; nothing was written back. Edit it again with C-c '.",
                        path.display()
                    );
                    self.show_message(MessageType::ERROR, message);
                    return;
                }
            }
        }
        // A part narrowed to goes back as it is.
        if edit.kind == SpecialKind::Region {
            let (file, uri) = (edit.file.clone(), edit.uri.clone());
            let begin = if point == edit.point { edit.begin } else { point };
            let start = byte(begin);
            let finish =
                text[start..].char_indices().nth(edit.region.chars().count()).map_or(text.len(), |(at, _)| start + at);
            let new = format!("{}{}{}", &text[..start], content, &text[finish..]);
            let result = json!({ "text": new, "extra": { "begin": begin, "end": begin + content.chars().count() } });
            self.written_back(&file, &uri, version, &text, content, Ok(result));
            return;
        }
        let params = json!({
            "file": path.to_string_lossy(),
            "text": text,
            "point": point,
            "command": match edit.kind {
                SpecialKind::Element => "org-lsp-bridge-edit-special-save",
                SpecialKind::Formulas => "org-lsp-bridge-formulas-save",
                SpecialKind::Field => "org-lsp-bridge-field-save",
                SpecialKind::Region => unreachable!("a region goes back without Emacs"),
            },
            "args": [content],
        });
        let edit = &mut self.special_edits[index];
        edit.busy = true;
        let job = EmacsJob::WriteBack { file: edit.file.clone(), uri: edit.uri.clone(), version, text, content };
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, job);
        self.emacs.start(tag, "run", params);
    }

    /// What Emacs wrote back (see [`Server::write_back`]) becomes an edit
    /// of the Org file, which the editor then saves.
    fn written_back(
        &mut self,
        file: &Path,
        uri: &Uri,
        version: i32,
        old: &str,
        content: String,
        result: std::result::Result<Value, String>,
    ) {
        let Some(index) = self.special_edits.iter().position(|edit| edit.file == file) else { return };
        self.special_edits[index].busy = false;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.show_message(MessageType::ERROR, error);
                return;
            }
        };
        if self.document(uri).map(|document| document.version) != Some(version) {
            self.show_message(
                MessageType::WARNING,
                "The Org file changed while Emacs wrote the edit back; nothing was changed. Save again.",
            );
            return;
        }
        let new = result.get("text").and_then(Value::as_str).unwrap_or(old).to_string();
        let number = |key: &str| result.get("extra").and_then(|extra| extra.get(key)).and_then(Value::as_u64);
        let edit = &mut self.special_edits[index];
        if let (Some(begin), Some(end)) = (number("begin"), number("end")) {
            let (begin, end) = (begin as usize, end as usize);
            let chars: Vec<char> = new.chars().collect();
            let (old_begin, old_end) = (edit.begin, edit.end);
            let moved = result.get("point").and_then(Value::as_u64).map(|at| at as usize);
            edit.point = if let (SpecialKind::Field, Some(at)) = (edit.kind, moved) {
                at
            } else if edit.point == old_begin {
                begin
            } else if edit.point > old_end {
                (edit.point + end).saturating_sub(old_end)
            } else {
                edit.point
            };
            edit.region =
                chars.get(begin.saturating_sub(1)..end.saturating_sub(1)).unwrap_or_default().iter().collect();
            edit.begin = begin;
            edit.end = end;
        }
        edit.content = content;
        if new == old {
            return;
        }
        // Org saves the file it writes back to.
        let edits = self.text_edits(old, &new);
        let edit = self.workspace_edit(uri, Some(version), edits);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        let request = self.new_request(
            ApplyWorkspaceEdit::METHOD,
            serde_json::to_value(params).unwrap_or(Value::Null),
            Some(Pending::Edit),
        );
        self.saving.insert(request.id.clone(), vec![uri.clone()]);
        let _ = self.sender.send(Message::Request(request));
    }

    /// `org-agenda-to-appt`: Emacs's `appt.el` adds today's entries with
    /// a time from the agenda files, those open as the editor has them;
    /// with `C-u`, the appointments it had are dropped first.
    fn agenda_to_appt(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let refresh = context.and_then(|context| context.count).is_some_and(|count| count > 1);
        let mut files = Vec::new();
        let mut texts = Vec::new();
        for (uri, held) in self.agenda_files() {
            let Some(path) = uri::to_path(&uri) else { continue };
            let path = path.to_string_lossy().into_owned();
            if let Held::Open(_) = held {
                if let Some(document) = self.document(&uri) {
                    texts.push(json!([path, document.text]));
                }
            }
            files.push(path);
        }
        let params = json!({ "files": files, "texts": texts, "refresh": refresh });
        let result = self.emacs.call("appt", params, std::time::Duration::from_secs(60)).map_err(Refusal::Failed)?;
        self.message = result.get("message").and_then(Value::as_str).map(str::to_string);
        self.appt_said(&result);
        Ok(Value::Null)
    }

    /// Each minute while there are appointments, `appt-check`: what its
    /// window shows, or with `appt-display-format` `echo` what it says.
    fn appt_check(&mut self) {
        match self.emacs.call("appt-check", json!({}), std::time::Duration::from_secs(10)) {
            Ok(result) => {
                if let Some(message) = result.get("message").and_then(Value::as_str) {
                    self.show_message(MessageType::INFO, message);
                }
                self.appt_said(&result);
            }
            Err(error) => {
                self.appt_check = None;
                self.show_message(MessageType::ERROR, error);
            }
        }
    }

    /// What `appt.el` showed in its window, and the next check while it
    /// has appointments.
    fn appt_said(&mut self, result: &Value) {
        for shown in result.get("shown").and_then(Value::as_array).into_iter().flatten() {
            if let Some(shown) = shown.as_str() {
                self.show_message(MessageType::INFO, shown);
            }
        }
        let left = result.get("appointments").and_then(Value::as_u64).unwrap_or(0);
        let minute = self.appt_minute;
        self.appt_check = (left > 0).then(|| match self.appt_check {
            // Each minute from the first, as Emacs's timer runs.
            Some(at) if at > std::time::Instant::now() => at,
            Some(at) => at + minute,
            None => std::time::Instant::now() + minute,
        });
    }

    /// The countdown is over: Emacs ends it as its timer would, and what
    /// Org says then (`Write the report: time out`) is shown.
    fn countdown_done(&mut self) {
        self.countdown = None;
        match self.emacs.call("timer-fire", json!({}), std::time::Duration::from_secs(10)) {
            Ok(result) => {
                if let Some(message) = result.get("message").and_then(Value::as_str) {
                    self.show_message(MessageType::INFO, message);
                }
                self.timer_said(&result);
            }
            Err(error) => self.show_message(MessageType::ERROR, error),
        }
    }

    /// Tells the editor how the timer shows now (see [`TIMER`]), when one
    /// runs or ran.
    fn timer_said(&mut self, result: &Value) {
        let timer = result.get("timer").filter(|timer| !timer.is_null()).cloned();
        if timer.is_none() && self.timer.is_none() {
            return;
        }
        self.timer = timer.clone();
        let notification = Notification::new(TIMER.to_string(), timer.unwrap_or(Value::Null));
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// Org's motion commands (`org-next-visible-heading` and the rest):
    /// the result carries the cursor, as `org.return`'s does, and says what
    /// Org says. A link search that found nothing makes the same command
    /// right after start from the other end.
    fn motion(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::motion::{self, Motion};
        let uri = &context.text_document.uri;
        let encoding = self.client.encoding;
        let document = self.document(uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, encoding);
        let which = match id {
            commands::HEADING_NEXT => Motion::NextHeading,
            commands::HEADING_PREVIOUS => Motion::PreviousHeading,
            commands::HEADING_FORWARD => Motion::ForwardSameLevel,
            commands::HEADING_BACKWARD => Motion::BackwardSameLevel,
            commands::HEADING_UP => Motion::Up,
            commands::LINK_NEXT => Motion::NextLink,
            _ => Motion::PreviousLink,
        };
        let key = uri.as_str().to_string();
        let repeated = self.last.as_ref().is_some_and(|last| last.command == id && last.file == key);
        let failed_before = repeated && self.link_search_failed;
        let count = context.count.filter(|count| *count != 0).unwrap_or(1);
        let moved = motion::run(&document.text, &document.parse, offset, which, count, failed_before)
            .map_err(Refusal::Failed)?;
        let cursor = document.index.position(&document.text, moved.offset, encoding);
        self.link_search_failed = moved.failed;
        self.last = Some(Last { command: id.to_string(), file: key, heading: None });
        self.message = moved.message;
        Ok(json!({ "edits": [], "cursor": cursor }))
    }

    /// `org-goto` (`C-c C-j`), as with `org-goto-interface` set to
    /// `outline-path-completion`: the file's headings to
    /// `org_goto_max_level`, named by their outline path; the answer moves
    /// the cursor to the heading picked.
    fn goto(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::refile::{self, OutlinePath, Select};
        let uri = &context.text_document.uri;
        let encoding = self.client.encoding;
        let document = self.document(uri).ok_or("This file is not open")?;
        let path = uri::to_path(uri).map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
        let name = path.rsplit(['/', '\\']).next().unwrap_or_default().to_string();
        let title = refile::file_title(&document.parse);
        let keywords = document.parse.todo_keywords();
        let file = refile::File { text: &document.text, keywords, name: &name, path: &path, title: title.as_deref() };
        let targets = refile::targets(&file, &Select::MaxLevel(self.goto_max_level), OutlinePath::Yes, true);
        let Some(answer) = &context.input else {
            if targets.is_empty() {
                return Err("No headings to go to".into());
            }
            let choices: Vec<Value> =
                targets.iter().map(|target| json!({ "key": target.name, "label": target.name })).collect();
            return Ok(json!({ "input": { "kind": "choice", "prompt": "Goto", "value": "", "choices": choices } }));
        };
        let offset = targets
            .iter()
            .find(|target| &target.name == answer)
            .and_then(|target| target.offset)
            .ok_or_else(|| format!("No heading {answer}"))?;
        let cursor = document.index.position(&document.text, offset, encoding);
        Ok(json!({ "edits": [], "cursor": cursor }))
    }

    /// The commands Emacs runs that ask for something first: what they
    /// ask, as Org asks it, then Emacs runs the command with the answers.
    fn emacs_command(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let answer = context.input.clone();
        match id {
            commands::FOOTNOTE_ACTION => self.emacs_run("org-footnote-action", context),
            commands::TOGGLE_ITEM => self.emacs_run("org-toggle-item", context),
            commands::TOGGLE_HEADING => self.emacs_run("org-toggle-heading", context),
            commands::TOGGLE_COMMENT => self.emacs_run("org-toggle-comment", context),
            // Off a table `S-RET` keeps its own meaning.
            commands::TABLE_COPY_DOWN if !self.at_table(context) => {
                Err(Refusal::NotHere("Not in table data field".to_string()))
            }
            commands::TABLE_COPY_DOWN => {
                self.emacs_run_with("org-table-copy-down", vec![json!(context.count.unwrap_or(1))], context)
            }
            commands::BABEL_LOB_INGEST => {
                let path = uri::to_path(&context.text_document.uri).ok_or("Only a file on disk can be ingested")?;
                self.emacs_run_with("org-babel-lob-ingest", vec![json!(path.to_string_lossy())], context)
            }
            // `C-u C-u`, or a selection: the timer values of the region
            // shifted (`org-timer-change-times-in-region`).
            commands::TIMER_START if context.count == Some(16) || Self::has_region(context) => {
                if !Self::has_region(context) {
                    return Err("The mark is not set now, so there is no region".into());
                }
                let Some(answer) = answer else {
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": "Enter time difference like \"-1:08:26\".  Default is first time to zero",
                        "value": "",
                    } }));
                };
                self.emacs_run_with("org-lsp-bridge-timer-shift", vec![json!(answer)], context)
            }
            // `C-u`: started with an offset, the timer value at the cursor
            // by default.
            commands::TIMER_START if context.count == Some(4) => {
                let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
                let offset = document.index.offset(&document.text, context.position, self.client.encoding);
                let default =
                    org_core::timestr::timer_value_at(&document.text, offset).unwrap_or_else(|| "0:00:00".to_string());
                let Some(answer) = answer else {
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": format!("Restart timer with offset [{default}]"),
                        "value": "",
                    } }));
                };
                self.emacs_run_with("org-lsp-bridge-timer-offset", vec![json!(answer), json!(default)], context)
            }
            commands::TIMER_START => self.emacs_run("org-timer-start", context),
            commands::TIMER_INSERT => self.emacs_run("org-timer", context),
            commands::TIMER_ITEM => self.emacs_run("org-timer-item", context),
            commands::TIMER_PAUSE => self.emacs_run("org-timer-pause-or-continue", context),
            commands::TIMER_STOP => self.emacs_run("org-timer-stop", context),
            commands::TIMER_SET => {
                // With an effort, Org takes its minutes and asks nothing.
                let Some(answer) = answer else {
                    let uri = &context.text_document.uri;
                    let path = uri::to_path(uri).ok_or("Only a file on disk can run code through Emacs")?;
                    let document = self.document(uri).ok_or("This file is not open")?;
                    let offset = document.index.offset(&document.text, context.position, self.client.encoding);
                    let point = document.text[..offset].chars().count() + 1;
                    let params = json!({ "file": path.to_string_lossy(), "text": document.text, "point": point });
                    let default = self
                        .emacs
                        .call("timer-default", params, std::time::Duration::from_secs(60))
                        .map_err(Refusal::Failed)?;
                    if default.get("effort").and_then(Value::as_bool) == Some(true) {
                        return self.emacs_run("org-timer-set-timer", context);
                    }
                    let value = default.get("default").and_then(Value::as_str).unwrap_or_default();
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": "How much time left? (minutes or h:mm:ss)",
                        "value": value,
                    } }));
                };
                self.emacs_run_with("org-lsp-bridge-timer-set", vec![json!(answer.trim())], context)
            }
            commands::TABLE_SUM => self.emacs_run("org-lsp-bridge-table-sum", context),
            commands::TABLE_FIELD_INFO => self.emacs_run("org-table-field-info", context),
            commands::TABLE_CREATE => {
                let Some(answer) = answer else {
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": "Table size Columns x Rows (empty for Org's default)",
                        "value": "",
                    } }));
                };
                self.emacs_run_with("org-lsp-bridge-table-create", vec![json!(answer.trim())], context)
            }
            commands::BABEL_NEXT | commands::BABEL_PREVIOUS | commands::BLOCK_NEXT | commands::BLOCK_PREVIOUS => {
                let command = match id {
                    commands::BABEL_NEXT => "org-babel-next-src-block",
                    commands::BABEL_PREVIOUS => "org-babel-previous-src-block",
                    commands::BLOCK_NEXT => "org-next-block",
                    _ => "org-previous-block",
                };
                self.emacs_run_with(command, vec![json!(context.count.unwrap_or(1))], context)
            }
            commands::BABEL_GOTO => {
                let Some(answer) = answer.filter(|answer| !answer.is_empty()) else {
                    let uri = &context.text_document.uri;
                    let path = uri::to_path(uri).ok_or("Only a file on disk can run code through Emacs")?;
                    let document = self.document(uri).ok_or("This file is not open")?;
                    let params = json!({ "file": path.to_string_lossy(), "text": document.text });
                    let names = self
                        .emacs
                        .call("src-block-names", params, std::time::Duration::from_secs(60))
                        .map_err(Refusal::Failed)?;
                    let choices: Vec<Value> = names
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|name| json!({ "key": name, "label": name }))
                        .collect();
                    if choices.is_empty() {
                        return Err(Refusal::Failed("No named code blocks".to_string()));
                    }
                    return Ok(json!({ "input": {
                        "kind": "choice",
                        "prompt": "source-block name",
                        "value": "",
                        "choices": choices,
                    } }));
                };
                self.emacs_run_with("org-babel-goto-named-src-block", vec![json!(answer)], context)
            }
            commands::FOOTNOTE_MENU => {
                let Some(answer) = answer else {
                    let choices = [
                        ("s", "sort"),
                        ("r", "renumber fn:N"),
                        ("S", "renumber and sort"),
                        ("n", "normalize"),
                        ("d", "delete"),
                    ]
                    .map(|(key, label)| json!({ "key": key, "label": label }));
                    return Ok(
                        json!({ "input": { "kind": "key", "prompt": "Footnotes", "value": "", "choices": choices } }),
                    );
                };
                let command = match answer.as_str() {
                    "s" => "org-footnote-sort",
                    "r" => "org-footnote-renumber-fn:N",
                    "S" => "org-lsp-bridge-footnote-renumber-and-sort",
                    "n" => "org-footnote-normalize",
                    "d" => "org-footnote-delete",
                    other => return Err(format!("No such footnote command {other}").into()),
                };
                self.emacs_run(command, context)
            }
            commands::EMPHASIZE => {
                let Some(answer) = answer else {
                    let choices = [
                        ("*", "bold"),
                        ("/", "italic"),
                        ("_", "underline"),
                        ("=", "verbatim"),
                        ("~", "code"),
                        ("+", "strike-through"),
                        (" ", "remove"),
                    ]
                    .map(|(key, label)| json!({ "key": key, "label": label }));
                    return Ok(
                        json!({ "input": { "kind": "key", "prompt": "Emphasis marker or tag", "value": "", "choices": choices } }),
                    );
                };
                let marker = answer.chars().next().ok_or("No marker")?;
                self.emacs_run_with("org-emphasize", vec![json!(u32::from(marker))], context)
            }
            commands::STRUCTURE_TEMPLATE => {
                let templates = match &self.structure_templates {
                    Some(templates) => templates.clone(),
                    None => {
                        let found = self
                            .emacs
                            .call("structure-templates", json!({}), std::time::Duration::from_secs(120))
                            .map_err(Refusal::Failed)?;
                        self.structure_templates = Some(found.clone());
                        found
                    }
                };
                let pairs: Vec<(String, String)> = templates
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|pair| Some((pair.get(0)?.as_str()?.to_string(), pair.get(1)?.as_str()?.to_string())))
                    .collect();
                let Some(answer) = answer else {
                    let choices: Vec<Value> =
                        pairs.iter().map(|(key, kind)| json!({ "key": key, "label": kind })).collect();
                    return Ok(
                        json!({ "input": { "kind": "key", "prompt": "Select a key", "value": "", "choices": choices } }),
                    );
                };
                let kind = pairs
                    .iter()
                    .find(|(key, _)| *key == answer)
                    .map(|(_, kind)| kind.clone())
                    .ok_or_else(|| format!("No structure template under {answer}"))?;
                self.emacs_run_with("org-insert-structure-template", vec![json!(kind)], context)
            }
            commands::DRAWER_INSERT => {
                let Some(answer) = answer.filter(|answer| !answer.trim().is_empty()) else {
                    let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
                    let mut names: Vec<String> = drawer_names(&document.text);
                    names.sort();
                    names.dedup();
                    return Ok(
                        json!({ "input": { "kind": "text", "prompt": "Drawer", "value": "", "completions": names } }),
                    );
                };
                self.emacs_run_with("org-insert-drawer", vec![Value::Null, json!(answer.trim())], context)
            }
            commands::TABLE_IMPORT => {
                let Some(answer) = answer else {
                    return Ok(json!({ "input": { "kind": "text", "prompt": "Import file", "value": "" } }));
                };
                // The separator found as Org finds it: tabs, commas, spaces.
                self.emacs_run_with("org-table-import", vec![json!(answer.trim()), Value::Null], context)
            }
            commands::TABLE_EXPORT => self.table_export(context),
            _ => {
                // `org-clone-subtree-with-time-shift`: how many, then the
                // shift (`+1w`, empty to copy unchanged).
                let Some(answer) = answer else {
                    return Ok(
                        json!({ "input": { "kind": "text", "prompt": "Number of clones to produce", "value": "" } }),
                    );
                };
                if context.step.as_deref() != Some("shift") {
                    let clones: i64 = answer.trim().parse().map_err(|_| format!("Not a number: {answer}"))?;
                    if clones < 1 {
                        return Err("Invalid number of replications".into());
                    }
                    return Ok(json!({ "input": {
                        "kind": "text",
                        "prompt": "Date shift per clone (e.g. +1w, empty to copy unchanged)",
                        "value": "",
                        "context": { "step": "shift", "answers": clones.to_string() },
                    } }));
                }
                let clones: i64 = context.answers.as_deref().unwrap_or("1").parse().unwrap_or(1);
                self.emacs_run_with(
                    "org-clone-subtree-with-time-shift",
                    vec![json!(clones), json!(answer.trim())],
                    context,
                )
            }
        }
    }

    /// `org-table-export`: the file (`TABLE_EXPORT_FILE`, else asked for,
    /// an existing one only once overwriting it is confirmed), then the
    /// format (`TABLE_EXPORT_FORMAT`, else asked for among Org's, the one
    /// for the file's extension by default); Emacs writes it.
    fn table_export(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        const FORMATS: [&str; 8] = [
            "orgtbl-to-tsv",
            "orgtbl-to-csv",
            "orgtbl-to-latex",
            "orgtbl-to-html",
            "orgtbl-to-generic",
            "orgtbl-to-texinfo",
            "orgtbl-to-orgtbl",
            "orgtbl-to-unicode",
        ];
        let uri = context.text_document.uri.clone();
        let encoding = self.client.encoding;
        let (edit_settings, category) = (self.edit_settings.clone(), uri::file_stem(&uri));
        let (property_file, property_format) = self
            .with_source(&uri, |text, index, parse, _, _| {
                let offset = index.offset(text, context.position, encoding);
                let properties = edit::property_context(parse, &edit_settings, &category);
                (
                    org_core::properties::inherited(text, offset, "TABLE_EXPORT_FILE", &properties),
                    org_core::properties::inherited(text, offset, "TABLE_EXPORT_FORMAT", &properties),
                )
            })
            .ok_or("This file is not open")?;
        // What was answered so far: the file, then the format.
        let mut answers: Vec<String> =
            context.answers.as_deref().and_then(|answers| serde_json::from_str(answers).ok()).unwrap_or_default();
        let step = context.step.as_deref().unwrap_or("");
        if let Some(input) = &context.input {
            match step {
                "overwrite" if !matches!(input.as_str(), "y" | "Y" | " ") => {
                    return Err("File not written".into());
                }
                "overwrite" => {}
                _ => answers.push(input.trim().to_string()),
            }
        }
        let ask = |kind: &str, prompt: String, step: &str, answers: &[String], choices: Option<Value>| {
            let mut input = json!({ "kind": kind, "prompt": prompt, "value": "",
                "context": { "step": step, "answers": serde_json::to_string(answers).unwrap_or_default() } });
            if let Some(choices) = choices {
                input["choices"] = choices;
            }
            json!({ "input": input })
        };
        let file = match property_file {
            Some(file) => file,
            None => {
                let Some(file) = answers.first().cloned() else {
                    return Ok(ask("text", "Export table to".to_string(), "file", &answers, None));
                };
                let directory = uri::to_path(&uri).and_then(|path| path.parent().map(Path::to_path_buf));
                let full = directory.map_or_else(|| PathBuf::from(&file), |directory| directory.join(&file));
                if full.exists() && step == "file" {
                    let choices = json!([{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }]);
                    let prompt = format!("Overwrite file {file}? ");
                    return Ok(ask("key", prompt, "overwrite", &answers, Some(choices)));
                }
                file
            }
        };
        let format = match property_format {
            Some(format) => format,
            // The format, just answered.
            None if step == "format" => answers.last().cloned().unwrap_or_default(),
            None => {
                let extension = Path::new(&file).extension().map(|ext| ext.to_string_lossy().to_string());
                let default = extension
                    .and_then(|ext| FORMATS.iter().find(|format| format.ends_with(&ext)))
                    .unwrap_or(&"orgtbl-to-tsv");
                let choices: Vec<Value> = std::iter::once(*default)
                    .chain(FORMATS.iter().copied().filter(|format| format != default))
                    .map(|format| json!({ "key": format, "label": format }))
                    .collect();
                return Ok(ask("choice", "Format".to_string(), "format", &answers, Some(json!(choices))));
            }
        };
        self.emacs_run_with("org-table-export", vec![json!(file), json!(format)], context)
    }

    /// Run the Emacs command `command` (Babel's) on the document of
    /// `context`, at its cursor; the text it leaves comes back as an edit
    /// (see [`Server::emacs_edited`]).
    /// `org-link-preview` (`C-c C-x C-v`) and `org-latex-preview` (`C-c C-x
    /// C-l`) at `context`, for a client that shows images: the part of the
    /// file and whether its previews show or go, as Emacs decides from the
    /// prefix (`count`: 4 clears, 16 or 11 the whole file, 64 clears it, 1
    /// and 11 take links with a description too), the region and the
    /// cursor (on a link or a fragment it toggles that one). The images of
    /// links come with the answer, `{ "preview": … }`; those of LaTeX from
    /// Emacs, in `org-lsp/images`. `colors` are the
    /// foreground and the background LaTeX is drawn in. With `startup`, the whole file's links
    /// when it opens with previews (`org_startup_with_link_previews`,
    /// `#+STARTUP: inlineimages`), else nothing.
    fn preview_command(
        &mut self,
        id: &str,
        context: &Context,
        startup: bool,
        colors: (Option<String>, Option<String>),
    ) -> std::result::Result<Value, Refusal> {
        if !self.client.images {
            return Ok(json!({ "message": "This editor cannot show images" }));
        }
        let latex = id == commands::LATEX_PREVIEW;
        let uri = context.text_document.uri.clone();
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let (text, version, index) = (document.text.clone(), document.version, document.index.clone());
        let parse = document.parse.clone();
        if startup {
            let mut on = if latex { self.startup_latex_preview } else { self.startup_link_previews };
            for word in startup_words(&text) {
                match (latex, word.as_str()) {
                    (false, "inlineimages" | "linkpreviews") | (true, "latexpreview") => on = true,
                    (false, "noinlineimages" | "nolinkpreviews") | (true, "nolatexpreview") => on = false,
                    _ => {}
                }
            }
            // Emacs is not started for a file without fragments.
            let fragments = || {
                parse.syntax().descendants().any(|node| {
                    matches!(
                        node.kind(),
                        org_syntax::SyntaxKind::LATEX_FRAGMENT | org_syntax::SyntaxKind::LATEX_ENVIRONMENT
                    )
                })
            };
            if !on || (latex && !fragments()) {
                return Ok(Value::Null);
            }
        }
        let offset = index.offset(&text, context.position, encoding);
        let region = context.range.filter(|range| range.start != range.end).map(|range| {
            let (a, b) = (index.offset(&text, range.start, encoding), index.offset(&text, range.end, encoding));
            (a.min(b), a.max(b))
        });
        // The link or the fragment the cursor is on.
        let at = if latex {
            parse
                .syntax()
                .token_at_offset(org_syntax::TextSize::from(offset as u32))
                .right_biased()
                .and_then(|token| {
                    token.parent_ancestors().find(|node| {
                        matches!(
                            node.kind(),
                            org_syntax::SyntaxKind::LATEX_FRAGMENT | org_syntax::SyntaxKind::LATEX_ENVIRONMENT
                        )
                    })
                })
                .map(|node| (usize::from(node.text_range().start()), usize::from(node.text_range().end())))
        } else {
            open::link_at(&text, &parse, offset, &self.link_abbreviations).map(|link| (link.start, link.end))
        };
        let whole = (0, text.len());
        let section = section_bounds(&text, offset);
        let point = if latex { "fragment at point" } else { "image at point" };
        let (scope, (from, to), remove) = match context.count {
            _ if startup => ("buffer", whole, Some(false)),
            Some(64) => ("buffer", whole, Some(true)),
            Some(16 | 11) => ("buffer", whole, Some(false)),
            Some(4) => match (region, at) {
                (Some(range), _) => ("region", range, Some(true)),
                (None, Some(range)) => ("preview at point", range, Some(true)),
                (None, None) => ("current section", section, Some(true)),
            },
            _ => match (region, at) {
                (Some(range), _) => ("region", range, Some(false)),
                (None, Some(range)) => (point, range, None),
                (None, None) => ("current section", section, Some(false)),
            },
        };
        let include_linked = !latex && matches!(context.count, Some(1 | 11));
        let range = |start: usize, end: usize| {
            index.range(&text, org_syntax::TextRange::new((start as u32).into(), (end as u32).into()), encoding)
        };
        let mut preview = json!({
            "kind": if latex { "latex" } else { "link" },
            "uri": uri,
            "scope": scope,
            "range": range(from, to),
            "remove": remove,
            "includeLinked": include_linked,
            "items": [],
        });
        if startup {
            preview["startup"] = json!(true);
        }
        if remove == Some(true) {
            return Ok(json!({ "preview": preview }));
        }
        if latex {
            // Emacs makes the images; they come in `org-lsp/preview`.
            let path = uri::to_path(&uri).ok_or("Only a file on disk has LaTeX previews")?;
            let chars = |at: usize| text[..at].chars().count() + 1;
            let params = json!({
                "file": path.to_string_lossy(),
                "text": text,
                "point": chars(offset),
                "from": chars(from),
                "to": chars(to),
                "foreground": colors.0,
                "background": colors.1,
            });
            self.next_export += 1;
            let tag = self.next_export;
            self.emacs_jobs.insert(tag, EmacsJob::LatexPreview { uri, version, text, preview });
            self.emacs.start(tag, "latex-preview", params);
            return Ok(json!({ "message": "Creating LaTeX previews..." }));
        }
        let items: Vec<Value> =
            org_core::link_preview::previews(&text, &parse, &self.link_abbreviations, include_linked, from, to)
                .into_iter()
                .filter_map(|found| {
                    let path = if found.attachment {
                        self.attachment_path(&uri, &text, found.start, &found.path)?
                    } else {
                        self.link_path(&uri, &found.path)?
                    };
                    // `a/./b.png` as `a/b.png`.
                    let path: PathBuf = path.components().collect();
                    let path = Some(path).filter(|path| path.is_file())?;
                    Some(json!({ "range": range(found.start, found.end), "path": path.to_string_lossy() }))
                })
                .collect();
        preview["items"] = json!(items);
        Ok(json!({ "preview": preview }))
    }

    /// The file of the `attachment:` link `name` at `offset` of `text`:
    /// `org-attach-expand` in Emacs, in the attachment directory of the
    /// entry there. None without Emacs, or without a directory.
    fn attachment_path(&mut self, uri: &Uri, text: &str, offset: usize, name: &str) -> Option<PathBuf> {
        let name = name.find("::").map_or(name, |at| &name[..at]);
        let file = uri::to_path(uri)?;
        let point = text[..offset].chars().count() + 1;
        let params = json!({ "file": file.to_string_lossy(), "text": text, "point": point, "name": name });
        let found = self.emacs.call("attach-expand", params, std::time::Duration::from_secs(60)).ok()?;
        found.as_str().map(PathBuf::from)
    }

    /// The images Emacs made for the LaTeX fragments of `uri`, sent in
    /// `org-lsp/images` while the document is as it was.
    fn latex_previewed(&mut self, uri: &Uri, version: i32, text: &str, mut preview: Value, result: &Value) {
        if self.document(uri).map(|document| document.version) != Some(version) {
            return;
        }
        let encoding = self.client.encoding;
        let index = LineIndex::new(text);
        // Characters from 1, as Emacs counts them, as bytes.
        let starts: Vec<usize> = text.char_indices().map(|(at, _)| at).chain([text.len()]).collect();
        let byte = |at: u64| starts.get((at as usize).saturating_sub(1)).copied().unwrap_or(text.len());
        let items: Vec<Value> = result
            .get("previews")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|found| {
                let (start, end) = (byte(found.get("start")?.as_u64()?), byte(found.get("end")?.as_u64()?));
                let range = org_syntax::TextRange::new((start as u32).into(), (end as u32).into());
                Some(json!({ "range": index.range(text, range, encoding), "path": found.get("file")?.as_str()? }))
            })
            .collect();
        let said = if items.is_empty() { "No LaTeX fragments to preview" } else { "Creating LaTeX previews... done." };
        let startup = preview.get("startup") == Some(&json!(true));
        let quiet = startup && items.is_empty();
        preview["items"] = json!(items);
        let notification = lsp_server::Notification::new(PREVIEW_IMAGES.to_string(), preview);
        let _ = self.sender.send(Message::Notification(notification));
        if !quiet {
            self.show_message(MessageType::INFO, said);
        }
    }

    fn emacs_run(&mut self, command: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        self.emacs_run_with(command, Vec::new(), context)
    }

    /// The edit `id` at `context`, by org-lsp; but a table whose formulas
    /// only Emacs computes is recalculated there.
    fn edit_or_recalculate(&mut self, context: &Context, id: &str) -> std::result::Result<Value, Refusal> {
        // With the formula debugger on, Emacs recalculates.
        if self.formula_debug && self.recalculates(context, id) {
            return self.emacs_recalculate(id, context);
        }
        match self.edit_with(context, id, None, &Extra::default()) {
            Err(Refusal::Failed(message)) if org_core::tblfm::needs_emacs(&message) => {
                self.emacs_recalculate(id, context)
            }
            other => other,
        }
    }

    /// Whether `id` at `context` recalculates a table: `C-c *` and its
    /// forms, or `C-c C-c` on a `#+TBLFM:` line.
    fn recalculates(&self, context: &Context, id: &str) -> bool {
        if matches!(id, commands::TABLE_RECALCULATE | commands::TABLE_RECALCULATE_LINE | commands::TABLE_ITERATE) {
            return true;
        }
        let Some(document) = self.document(&context.text_document.uri) else { return false };
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        id == commands::CHECKBOX_TOGGLE && org_core::tblfm::on_formula_line(&document.text, offset)
    }

    /// `org-table-recalculate` (`C-c *`, `C-u C-c *`), `org-table-iterate`
    /// or, on a `#+TBLFM:` line, `org-table-calc-current-TBLFM`, in Emacs:
    /// for the formulas org-lsp leaves to it, and for all of them while the
    /// formula debugger is on.
    fn emacs_recalculate(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, self.client.encoding);
        let (command, args) = if org_core::tblfm::on_formula_line(&document.text, offset) {
            ("org-table-calc-current-TBLFM", vec![])
        } else {
            match id {
                commands::TABLE_RECALCULATE => ("org-table-recalculate", vec![json!(true)]),
                commands::TABLE_ITERATE => ("org-table-iterate", vec![]),
                _ => ("org-table-recalculate", vec![]),
            }
        };
        if self.formula_debug {
            let mut wrapped = vec![json!(command)];
            wrapped.extend(args);
            return self.emacs_run_with("org-lsp-bridge-debug-formulas", wrapped, context);
        }
        if args.is_empty() {
            self.emacs_run(command, context)
        } else {
            self.emacs_run_with(command, args, context)
        }
    }

    /// Whether the command was run on a selection (`org-region-active-p`).
    fn has_region(context: &Context) -> bool {
        context.range.is_some_and(|range| range.start != range.end)
    }

    /// The selection a command was run on in an Org file, as byte offsets:
    /// the active region, from its beginning to its end.
    fn region_of(&self, context: &Context) -> Option<(usize, usize)> {
        let range = context.range.filter(|_| Self::has_region(context))?;
        let document = self.document(&context.text_document.uri)?;
        let encoding = self.client.encoding;
        let start = document.index.offset(&document.text, range.start, encoding);
        let end = document.index.offset(&document.text, range.end, encoding);
        Some((start.min(end), start.max(end)))
    }

    /// [`Server::emacs_run`] with the command called with `args`.
    fn emacs_run_with(
        &mut self,
        command: &str,
        args: Vec<Value>,
        context: &Context,
    ) -> std::result::Result<Value, Refusal> {
        let uri = context.text_document.uri.clone();
        let path = uri::to_path(&uri).ok_or("Only a file on disk can run code through Emacs")?;
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let offset = document.index.offset(&document.text, context.position, encoding);
        let point = document.text[..offset].chars().count() + 1;
        let mut params = json!({
            "file": path.to_string_lossy(),
            "text": document.text,
            "point": point,
            "command": command,
            "args": args,
        });
        // The region: the mark at the end of the selection the cursor is
        // not at.
        if let Some(range) = context.range.filter(|range| range.start != range.end) {
            let other = if range.start == context.position { range.end } else { range.start };
            let at = document.index.offset(&document.text, other, encoding);
            params["mark"] = json!(document.text[..at].chars().count() + 1);
        }
        let job = EmacsJob::Edit { uri: uri.clone(), version: document.version, text: document.text.clone(), point };
        self.next_export += 1;
        let tag = self.next_export;
        self.emacs_jobs.insert(tag, job);
        self.emacs.start(tag, "run", params);
        Ok(Value::Null)
    }

    /// What an Emacs command left of the document `uri`: the difference as
    /// an edit, unless the document changed since it was sent; else what
    /// Emacs said.
    fn emacs_edited(&mut self, uri: &Uri, version: i32, old: &str, point: usize, result: &Value) {
        let new = result.get("text").and_then(Value::as_str).unwrap_or(old);
        let said = result.get("message").and_then(Value::as_str).filter(|message| !message.is_empty());
        // Where Emacs leaves the point (a character, from 1), in the text
        // it leaves.
        let moved = result
            .get("point")
            .and_then(Value::as_u64)
            .map(|at| new.char_indices().nth(at.saturating_sub(1) as usize).map_or(new.len(), |(byte, _)| byte));
        let position = moved.map(|at| LineIndex::new(new).position(new, at, self.client.encoding));
        if new == old {
            if let Some(message) = said {
                self.show_message(MessageType::INFO, message);
            }
            let jumped = result.get("point").and_then(Value::as_u64).is_some_and(|at| at as usize != point);
            if let (Some(position), true) = (position, jumped) {
                if self.document(uri).map(|document| document.version) == Some(version) {
                    let selection = Some(Range::new(position, position));
                    let params =
                        ShowDocumentParams { uri: uri.clone(), external: None, take_focus: Some(true), selection };
                    self.send_request::<ShowDocument>(params, None);
                }
            }
            return;
        }
        if self.document(uri).map(|document| document.version) != Some(version) {
            self.show_message(
                MessageType::WARNING,
                "The file changed while Emacs ran the code; nothing was changed. Run it again.",
            );
            return;
        }
        let edits = self.text_edits(old, new);
        let edit = self.workspace_edit(uri, Some(version), edits);
        // The file stays unsaved, as Emacs leaves it.
        self.origin = Some(uri.as_str().to_string());
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        match position {
            Some(position) => self.apply_edit(params, Pending::EditAndMove { uri: uri.clone(), position }),
            None => self.apply_edit(params, Pending::Edit),
        }
    }

    /// Show what an export made: the text of an export to a buffer as a
    /// file of its own; a file to be opened in the editor, or by the
    /// system when it is no text (HTML, PDF, ODT); else where it went.
    fn exported(&mut self, label: &str, result: &Value) {
        if let Some(text) = result.get("text").and_then(Value::as_str) {
            let name = result.get("name").and_then(Value::as_str).unwrap_or("Org Export").trim_matches('*');
            let extension = result.get("extension").and_then(Value::as_str).unwrap_or("txt");
            let file = self.export_directory.join(format!("{name}.{extension}"));
            let written = std::fs::create_dir_all(&self.export_directory).and_then(|()| std::fs::write(&file, text));
            if let Err(error) = written {
                self.show_message(MessageType::ERROR, format!("Cannot write {}: {error}", file.display()));
                return;
            }
            if let Some(uri) = uri::from_path(&file) {
                let params = ShowDocumentParams { uri, external: None, take_focus: Some(true), selection: None };
                self.send_request::<ShowDocument>(params, None);
            }
            self.export_stack.push((label.to_string(), file, true));
            return;
        }
        if result.get("published").is_some() {
            self.show_message(MessageType::INFO, "Publishing done");
            return;
        }
        let Some(file) = result.get("file").and_then(Value::as_str) else { return };
        let path = PathBuf::from(file);
        self.export_stack.push((label.to_string(), path.clone(), false));
        if result.get("open").and_then(Value::as_bool) == Some(true) && self.show_exported(&path, false) {
            return;
        }
        self.show_message(MessageType::INFO, format!("Exported to {file}"));
    }

    /// Shows what an export made: an export to a buffer or a text file in
    /// the editor, any other (a PDF, a page) with the system's program for
    /// it. Whether it could.
    fn show_exported(&mut self, path: &Path, buffer: bool) -> bool {
        let extension = path.extension().map(|extension| extension.to_string_lossy().to_lowercase());
        let external = !buffer
            && !matches!(
                extension.as_deref(),
                Some("org" | "txt" | "md" | "tex" | "texi" | "ics" | "el" | "csv" | "json" | "xml")
            );
        let Some(uri) = uri::from_path(path) else { return false };
        let params = ShowDocumentParams { uri, external: Some(external), take_focus: Some(!external), selection: None };
        self.send_request::<ShowDocument>(params, None);
        true
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
            let edited =
                item_structure(id, &text, offset, &keywords, tags_column, self.edit_settings.list_circular_motion)?;
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
                return self.edit_or_recalculate(context, commands::TABLE_RECALCULATE);
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
            _ => org_core::list::next_item(
                &text,
                offset,
                id == commands::SHIFT_UP,
                self.edit_settings.list_circular_motion,
            )
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

    /// `org-reveal` (`C-c C-r`): the lines to show around the cursor, as
    /// `org-fold-show-set-visibility` shows them; the result has `sparse`
    /// with `keep`, the editor's other folds left as they are, and
    /// `unfold`, the first lines of the folds to open, outer ones first.
    fn reveal(&mut self, id: &str, context: &Context) -> std::result::Result<Value, Refusal> {
        use org_core::reveal::{reveal, Detail};
        let document = self.document(&context.text_document.uri).ok_or("This file is not open")?;
        let text = &document.text;
        let offset = document.index.offset(text, context.position, self.client.encoding);
        let detail = match id {
            commands::REVEAL_SIBLINGS => Detail::Canonical,
            commands::REVEAL_TREE => Detail::Tree,
            _ => Detail::Lineage,
        };
        let lines: Vec<u32> = reveal(text, offset, detail)
            .into_iter()
            .map(|at| document.index.position(text, at, self.client.encoding).line)
            .collect();
        // The folds to open: those around a line to show, not those that
        // start on it (a headline shows, its subtree stays folded).
        let mut unfold: Vec<u32> =
            folding::folding_ranges(&document.syntax(), text, &document.index, self.separator_lines)
                .into_iter()
                .filter(|range| lines.iter().any(|line| range.start_line < *line && *line <= range.end_line))
                .map(|range| range.start_line)
                .collect();
        unfold.sort_unstable();
        unfold.dedup();
        Ok(json!({ "sparse": { "lines": lines, "highlights": [], "keep": true, "unfold": unfold } }))
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
    /// file, or the file of the entry of an agenda line. Each keyword with
    /// its key and whether it is a done state.
    fn fast_keys(&mut self, context: &Context) -> std::result::Result<Vec<(String, char, bool)>, Refusal> {
        let uri = &context.text_document.uri;
        let file = if self.is_view(uri) {
            self.entry_at(Some(context)).ok_or("No entry on this line")?.uri
        } else {
            uri.clone()
        };
        let keys = self.with_source(&file, |_, _, parse, _, _| {
            let keywords = parse.todo_keywords();
            keywords
                .fast_keys()
                .into_iter()
                .map(|(name, key)| (name.to_string(), key, keywords.is_done(name)))
                .collect()
        });
        Ok(keys.ok_or("Cannot read the file of this entry")?)
    }

    // ----- clocking

    /// The clock that runs in the files the server knows, the latest one
    /// if there are several.
    fn find_running(&mut self) -> Option<RunningClock> {
        self.open_clocks().into_iter().max_by_key(|running| clock::minutes_of(running.start))
    }

    /// The clocks that run in the open files and the agenda files.
    fn open_clocks(&mut self) -> Vec<RunningClock> {
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
        found
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
        self.arm_overrun(&params);
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

    /// The diagnostics of the document `key`: what Org's linter finds
    /// (`org-lint`), and the citation keys its bibliography does not have
    /// (the `error` face of `oc-basic`), with the keys close to each
    /// (`org-cite-basic--close-keys`) to substitute.
    fn publish_diagnostics(&mut self, key: &str) {
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
                    // An Org file is text: nothing in it is an error or a
                    // warning, only information.
                    diagnostics.push(lsp_types::Diagnostic {
                        range,
                        severity: Some(lsp_types::DiagnosticSeverity::INFORMATION),
                        source: Some("org-lsp".to_string()),
                        message,
                        data: Some(json!({ "citationKeys": close })),
                        ..Default::default()
                    });
                }
            }
        }
        // `invalid-id-link`: the IDs of the open files and the agenda files.
        let ids = (self.lint && text.contains("id:")).then(|| self.known_ids());
        diagnostics.extend(self.lint_diagnostics(&uri, &text, &parse, ids.as_ref()));
        diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start.line, diagnostic.range.start.character));
        if diagnostics.is_empty() && !self.diagnosed.remove(key) {
            return;
        }
        if !diagnostics.is_empty() {
            self.diagnosed.insert(key.to_string());
        }
        let params = lsp_types::PublishDiagnosticsParams { uri, diagnostics, version: Some(version) };
        let notification = lsp_server::Notification::new(
            <lsp_types::notification::PublishDiagnostics as lsp_types::notification::Notification>::METHOD.to_string(),
            params,
        );
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// What `org-lint` reports in a document, as diagnostics from the
    /// report's place to the end of its line: a warning, or information
    /// for a checker of low trust. The checker's name is the code.
    fn lint_diagnostics(
        &self,
        uri: &Uri,
        text: &str,
        parse: &org_syntax::Parse,
        ids: Option<&HashSet<String>>,
    ) -> Vec<lsp_types::Diagnostic> {
        if !self.lint || uri::file_name(uri).starts_with(CAPTURE_PREFIX) || self.is_roam_buffer(uri) {
            return Vec::new();
        }
        let Some(document) = self.documents.get(uri.as_str()) else { return Vec::new() };
        let directory = uri::to_path(uri).and_then(|path| path.parent().map(Path::to_path_buf));
        let exists = |name: &str| {
            let path = match name.strip_prefix('~') {
                Some(rest) => match std::env::var_os("HOME") {
                    Some(home) => PathBuf::from(home).join(rest.trim_start_matches('/')),
                    None => return false,
                },
                None => PathBuf::from(name),
            };
            let path = match (&directory, path.is_absolute()) {
                (Some(directory), false) => directory.join(path),
                _ => path,
            };
            path.exists()
        };
        let resolve = |name: &str| {
            let path = match name.strip_prefix('~') {
                Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest.trim_start_matches('/')),
                None => PathBuf::from(name),
            };
            Some(match (&directory, path.is_absolute()) {
                (Some(directory), false) => directory.join(path),
                _ => path,
            })
        };
        let read = |name: &str| resolve(name).and_then(|path| std::fs::read_to_string(path).ok());
        let file_name = || uri::to_path(uri).and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()));
        let id_known = |id: &str| ids.map(|ids| ids.contains(id));
        let context = org_core::lint::Context {
            file_exists: &exists,
            priorities: self.edit_settings.priorities,
            read_file: &read,
            file_name: &file_name,
            id_known: &id_known,
        };
        let encoding = self.client.encoding;
        org_core::lint::lint(text, parse, &context)
            .into_iter()
            .filter(|report| !self.lint_ignore.iter().any(|ignored| ignored == report.checker))
            .map(|report| {
                let start = report.offset.min(text.len());
                let end = text[start..].find('\n').map_or(text.len(), |found| start + found);
                let range = Range::new(
                    document.index.position(text, start, encoding),
                    document.index.position(text, end, encoding),
                );
                lsp_types::Diagnostic {
                    range,
                    // An Org file is text: what org-lint finds is
                    // information, a hint where it trusts it less.
                    severity: Some(if report.trust == "low" {
                        lsp_types::DiagnosticSeverity::HINT
                    } else {
                        lsp_types::DiagnosticSeverity::INFORMATION
                    }),
                    code: Some(lsp_types::NumberOrString::String(report.checker.to_string())),
                    source: Some("org-lint".to_string()),
                    message: report.message,
                    ..Default::default()
                }
            })
            .collect()
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
                "effortText": property("EFFORT"),
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
        // Dynamic blocks other than clock tables (`columnview`, those of
        // the user's) are Emacs's to fill in.
        let others = match id {
            commands::DBLOCK_UPDATE if count.is_some() => clocktable::has_other_blocks(&text),
            commands::DBLOCK_UPDATE | commands::CHECKBOX_TOGGLE => {
                clocktable::block_name(&text, offset).is_some_and(|name| !name.eq_ignore_ascii_case("clocktable"))
            }
            _ => false,
        };
        if others {
            return match count {
                Some(_) if id == commands::DBLOCK_UPDATE => self.emacs_run("org-update-all-dblocks", context),
                _ => self.emacs_run("org-dblock-update", context),
            };
        }
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

    /// `org-attach` (`C-c C-a`): a command of Org's dispatcher, by key, then
    /// what it asks for, and Emacs runs it: the attachment directory made
    /// or changed, the entry's `ID` or `DIR` and `ATTACH` tag set; what it
    /// opens, the editor or the system opens.
    fn attach(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        const COMMANDS: [(&str, &str); 14] = [
            ("a", "Attach a file, as org_attach_method says"),
            ("c", "Attach a file by copying it"),
            ("m", "Attach a file by moving it"),
            ("l", "Attach a file by a hard link"),
            ("y", "Attach a file by a symbolic link"),
            ("u", "Attach a file from a URL"),
            ("n", "Create a new attachment"),
            ("z", "Synchronize with the attachment directory"),
            ("o", "Open an attachment"),
            ("f", "Open the attachment directory"),
            ("d", "Delete an attachment"),
            ("D", "Delete all attachments"),
            ("s", "Set the attachment directory (DIR)"),
            ("S", "Unset the attachment directory"),
        ];
        let key = context.step.clone().or_else(|| context.input.clone());
        let Some(key) = key else {
            let choices: Vec<Value> =
                COMMANDS.iter().map(|(key, label)| json!({ "key": key, "label": label })).collect();
            return Ok(json!({ "input": { "kind": "key", "prompt": "Attachments", "value": "", "choices": choices } }));
        };
        if !COMMANDS.iter().any(|(known, _)| *known == key) {
            return Err(format!("No attachment command under {key}").into());
        }
        // What the command reads, answered: the input once the key is the
        // step.
        let answer = context.step.as_ref().and(context.input.clone());
        let ask = |prompt: &str| json!({ "input": { "kind": "text", "prompt": prompt, "value": "", "context": { "step": key } } });
        let answer = match (key.as_str(), answer) {
            ("a" | "c" | "m" | "l" | "y", None) => return Ok(ask("File to keep as an attachment")),
            ("u", None) => return Ok(ask("URL of the file to attach")),
            ("n", None) => return Ok(ask("Create attachment named")),
            ("s", None) => return Ok(ask("Attachment directory")),
            ("o" | "d", None) => {
                // The attachments to pick from (`org-attach-file-list`).
                let uri = &context.text_document.uri;
                let path = uri::to_path(uri).ok_or("Only a file on disk has attachments")?;
                let document = self.document(uri).ok_or("This file is not open")?;
                let offset = document.index.offset(&document.text, context.position, self.client.encoding);
                let point = document.text[..offset].chars().count() + 1;
                let params = json!({ "file": path.to_string_lossy(), "text": document.text, "point": point });
                let files = self
                    .emacs
                    .call("attach-files", params, std::time::Duration::from_secs(60))
                    .map_err(Refusal::Failed)?;
                let files: Vec<String> = files
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|file| file.as_str().map(str::to_string))
                    .collect();
                match files.as_slice() {
                    [] if key == "d" => return Err("No attachments".into()),
                    // One attachment opens at once (`org-attach-open`).
                    [] | [_] => String::new(),
                    _ => {
                        let prompt = if key == "o" { "Open attachment" } else { "Delete attachment" };
                        let choices: Vec<Value> =
                            files.iter().map(|file| json!({ "key": file, "label": file })).collect();
                        return Ok(json!({ "input": {
                            "kind": "choice",
                            "prompt": prompt,
                            "value": "",
                            "choices": choices,
                            "context": { "step": key },
                        } }));
                    }
                }
            }
            (_, answer) => answer.unwrap_or_default(),
        };
        self.emacs_run_with("org-lsp-bridge-attach", vec![json!(key), json!(answer)], context)
    }

    /// `org-dynamic-block-insert-dblock` (`C-c C-x x`): which block, then a
    /// clock table as `org-clock-report` makes it, or a column view of the
    /// scope asked for (`org-columns-insert-dblock`), by Emacs.
    fn dblock_insert(&mut self, context: &Context) -> std::result::Result<Value, Refusal> {
        match (context.step.as_deref(), context.input.as_deref()) {
            (None, None) => Ok(json!({ "input": {
                "kind": "choice",
                "prompt": "Dynamic block",
                "value": "",
                "choices": [
                    { "key": "clocktable", "label": "clocktable" },
                    { "key": "columnview", "label": "columnview" },
                ],
            } })),
            (None, Some("clocktable")) => self.clocktable(commands::CLOCK_REPORT, context),
            (None, Some("columnview")) => {
                // The IDs of the file's entries, as `org-property-values`.
                let ids: Vec<String> = self
                    .document(&context.text_document.uri)
                    .map(|document| {
                        document
                            .text
                            .lines()
                            .filter_map(|line| line.trim_start().strip_prefix(":ID:"))
                            .map(|id| id.trim().to_string())
                            .filter(|id| !id.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let completions: Vec<String> =
                    ["global".to_string(), "local".to_string()].into_iter().chain(ids).collect();
                Ok(json!({ "input": {
                    "kind": "text",
                    "prompt": "Capture columns (local, global, entry with :ID: property) [local]",
                    "value": "",
                    "completions": completions,
                    "context": { "step": "columnview" },
                } }))
            }
            (Some("columnview"), Some(id)) => {
                self.emacs_run_with("org-lsp-bridge-columns-insert-dblock", vec![json!(id.trim())], context)
            }
            (_, Some(other)) => Err(format!("No such dynamic block: {other}").into()),
            (Some(_), None) => Ok(Value::Null),
        }
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
            commands::CLOCK_RESOLVE => self.resolve_dangling(context),
            commands::CLOCK_RESOLVE_IDLE => self.resolve_idle(context),
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

    /// Note a message from the editor: one after `org-clock-idle-time`
    /// without any while a clock runs asks what to do with the idle time
    /// (`org-resolve-clocks-if-idle`).
    fn notice_activity(&mut self) {
        let now = clock::minutes_of(self.date_time());
        if let (Some(idle), Some(last)) = (self.clock_idle_time, self.last_activity) {
            if self.running.is_some() && self.idle_since.is_none() && now - last > idle {
                self.idle_since = Some(last);
                let question = self.idle_question(last, now);
                let uri = self.running.as_ref().map(|running| running.uri.as_str().to_string()).unwrap_or_default();
                let argument = json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 } });
                self.prompt_with_messages(commands::CLOCK_RESOLVE_IDLE.to_string(), argument, &question);
            }
        }
        self.last_activity = Some(now);
    }

    /// What `org-clock-resolve` asks after idle time.
    fn idle_question(&self, since: i64, now: i64) -> Value {
        let choices = [
            ("k", "Keep the idle time"),
            ("s", "Subtract the idle time"),
            ("S", "Subtract the idle time and clock out"),
            ("C", "Cancel the clock"),
            ("i", "Ignore"),
        ];
        let choices: Vec<Value> = choices.iter().map(|(key, label)| json!({ "key": key, "label": label })).collect();
        json!({ "input": {
            "kind": "choice",
            "prompt": format!("Clocked in & idle for {:.1} mins", (now - since) as f64),
            "value": "",
            "choices": choices,
        } })
    }

    /// The answer to the idle question: the time kept (`k`), the clock
    /// stopped when the idle time began and started again now (`s`) or
    /// not (`S`), the clock canceled (`C`). A clock that ran less than a
    /// minute before is canceled instead of stopped.
    fn resolve_idle(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let since = self.idle_since.take().ok_or("No idle time to resolve")?;
        let answer = context.and_then(|context| context.input.clone()).unwrap_or_default();
        let running = self.running.clone().ok_or("No active clock")?;
        let start = clock::minutes_of(running.start);
        let barely_started = since - start < 1;
        let stop = DateTime { day: since.div_euclid(1440), minute: since.rem_euclid(1440) };
        match answer.as_str() {
            "s" | "S" | "C" => {
                let (text, version, line) = self.clock_line(&running).ok_or("Clock start time is gone")?;
                let heading = Self::heading_above(&text, line);
                let new_text = if answer == "C" || barely_started {
                    clock::cancel(&text, line, &self.clock_settings)?
                } else {
                    self.clock_out_time = Some(stop);
                    clock::clock_out(&text, line, stop, &self.clock_settings)?.text
                };
                self.running = None;
                let change = (running.uri.clone(), version, text, new_text);
                if let (true, Some(heading)) = (answer == "s", heading) {
                    // The clock line is under its headline, which stays.
                    let now = self.date_time();
                    return self.clock_in_after(running.uri, heading, now, vec![change]);
                }
                self.send_texts(vec![change]);
                self.publish_clock();
                Ok(Value::Null)
            }
            _ => Ok(Value::Null),
        }
    }

    /// `org-resolve-clocks`: the first clock that runs in the files beside
    /// the clocked one (a dangling clock), with what to do with it: run on
    /// as the clock (`k`), stopped now (`K`), started again now (`s`),
    /// canceled (`S`, `C`), left as it is (`i`).
    fn resolve_dangling(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let current = self.running.clone();
        let dangling: Vec<RunningClock> = self
            .open_clocks()
            .into_iter()
            .filter(|clock| {
                current.as_ref().is_none_or(|running| {
                    !(running.uri == clock.uri && running.start == clock.start && running.head == clock.head)
                })
            })
            .collect();
        let Some(clock) = dangling.first().cloned() else {
            return Ok(json!({ "message": "No dangling clocks" }));
        };
        let now = self.date_time();
        let Some(answer) = context.and_then(|context| context.input.clone()) else {
            let minutes = clock::minutes_of(now) - clock::minutes_of(clock.start);
            let choices = [
                ("k", "Keep it: the clock runs on it"),
                ("K", "Keep it and clock out now"),
                ("s", "Subtract it: the clock starts again now"),
                ("S", "Subtract it and clock out"),
                ("C", "Cancel it"),
                ("i", "Ignore"),
            ];
            let choices: Vec<Value> =
                choices.iter().map(|(key, label)| json!({ "key": key, "label": label })).collect();
            return Ok(json!({ "input": {
                "kind": "choice",
                "prompt": format!("Dangling clock started {minutes} mins ago"),
                "value": "",
                "choices": choices,
            } }));
        };
        let (text, version, line) = self.clock_line(&clock).ok_or("Clock start time is gone")?;
        let heading = Self::heading_above(&text, line).ok_or("Clock start time is gone")?;
        match answer.as_str() {
            // The dangling clock becomes the clock; the one that ran stops.
            "k" => {
                if let Some(running) = current {
                    if let Some((other, other_version, other_line)) = self.clock_line(&running) {
                        let out = clock::clock_out(&other, other_line, now, &self.clock_settings)?;
                        self.clock_out_time = Some(now);
                        self.send_texts(vec![(running.uri, other_version, other, out.text)]);
                    }
                }
                self.running = Some(clock);
                self.publish_clock();
            }
            "K" => {
                let out = clock::clock_out(&text, line, now, &self.clock_settings)?;
                self.clock_out_time = Some(now);
                self.send_texts(vec![(clock.uri, version, text, out.text)]);
            }
            "s" | "S" | "C" => {
                let canceled = clock::cancel(&text, line, &self.clock_settings)?;
                let change = (clock.uri.clone(), version, text, canceled);
                if answer == "s" {
                    return self.clock_in_after(clock.uri, heading, now, vec![change]);
                }
                self.send_texts(vec![change]);
            }
            _ => {}
        }
        Ok(Value::Null)
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
        self.clock_in_after(file, heading, start, Vec::new())
    }

    /// [`Server::clock_in_at`] after `changes` to the files, made in the
    /// same edit.
    fn clock_in_after(
        &mut self,
        file: Uri,
        heading: usize,
        start: DateTime,
        changes: Vec<(Uri, Option<i32>, String, String)>,
    ) -> std::result::Result<Value, Refusal> {
        let now = self.date_time();
        let mut changes = changes;
        let mut heading = heading;
        let title = self.entry_title(&file, heading);
        let mut interrupted = None;
        if let Some(running) = self.running.clone() {
            // Its line, in the text a change before made of its file.
            let prior = changes.iter().position(|(uri, ..)| *uri == running.uri);
            let found = match prior {
                Some(index) => {
                    let base = changes[index].3.clone();
                    running_lines(&base)
                        .into_iter()
                        .find(|(_, start, head)| *start == running.start && *head == running.head)
                        .map(|(line, ..)| (base, changes[index].1, line))
                }
                None => self.clock_line(&running),
            };
            if let Some((text, version, line)) = found {
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
                match prior {
                    Some(index) => changes[index].3 = out_text,
                    None => changes.push((running.uri.clone(), version, text, out_text)),
                }
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
    circular: bool,
) -> std::result::Result<org_core::list::Edited, Refusal> {
    use org_core::list;
    match id {
        commands::META_LEFT => list::indent_item(text, offset, -1, false, keywords, tags_column),
        commands::META_RIGHT => list::indent_item(text, offset, 1, false, keywords, tags_column),
        commands::SHIFT_META_LEFT => list::indent_item(text, offset, -1, true, keywords, tags_column),
        commands::SHIFT_META_RIGHT => list::indent_item(text, offset, 1, true, keywords, tags_column),
        commands::META_UP => list::move_item(text, offset, false, circular),
        _ => list::move_item(text, offset, true, circular),
    }
}

/// The names of the drawers in `text` (`:NAME:` alone on a line), for
/// `org-insert-drawer`'s completion.
fn drawer_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix(':')?.strip_suffix(':')?;
            (!name.is_empty()
                && !name.eq_ignore_ascii_case("END")
                && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
            .then(|| name.to_string())
        })
        .collect()
}

/// The lines a change touches, in the text before it (the first and the
/// last, from 0); none when the whole text is replaced.
fn changed_lines(changes: &[lsp_types::TextDocumentContentChangeEvent]) -> Option<(usize, usize)> {
    let mut lines: Option<(usize, usize)> = None;
    for change in changes {
        let range = change.range?;
        let (first, last) = (range.start.line as usize, range.end.line as usize);
        lines = Some(lines.map_or((first, last), |(a, b)| (a.min(first), b.max(last))));
    }
    lines
}

/// `org-table--read-column-selection`: the columns of `2-4 6-` (from 1),
/// in a table of `count` columns.
fn column_ranges(input: &str, count: usize) -> std::collections::BTreeSet<usize> {
    let mut out = std::collections::BTreeSet::new();
    for part in input.split_whitespace() {
        let (from, to) = match part.split_once('-') {
            Some((from, to)) => (from.parse().unwrap_or(1), to.parse().unwrap_or(count)),
            None => match part.parse() {
                Ok(column) => (column, column),
                Err(_) => continue,
            },
        };
        out.extend(from.max(1)..=to.min(count));
    }
    out
}
