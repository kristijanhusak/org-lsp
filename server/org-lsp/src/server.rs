//! The server loop: initialisation, document sync and request dispatch.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
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
use crate::prompt::{self, DatePrompt, Target};
use crate::views::{self, EntryRef, Query, View, ViewLine};
use crate::workspace::{Source, Workspace};
use crate::{actions, cli, commands, completion, folding, highlight, manifest, symbols, uri};

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
    /// For `org.capture`: the keys of the template picked so far.
    keys: Option<String>,
    /// For `org.capture`: the answers to the template's questions so far;
    /// `input` is then the next one.
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
    /// To the first line of a day, in a view that was just opened.
    Day(Day),
}

impl Reveal {
    /// Where the cursor goes in `view`.
    fn position(&self, view: &View) -> Position {
        match self {
            Reveal::Home => Position::new(view.home_line(), 0),
            Reveal::At(position) => *position,
            Reveal::Day(day) => Position::new(view.day_line(*day), 0),
        }
    }
}

/// A source of agenda entries as the server holds it while it works.
enum Held {
    Open(Outline),
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
    /// The files to save once the edit request with this id is applied.
    saving: HashMap<RequestId, Vec<Uri>>,
    todo: TodoKeywords,
    agenda: AgendaConfig,
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
    /// The date prompt that waits for an answer, in clients that show it.
    date_prompt: Option<DatePrompt>,
    /// The client's copy of the date prompt document, which it may keep
    /// after a prompt is done.
    prompt_copy: Option<Document>,
    /// `org-cycle-separator-lines`
    separator_lines: i64,
    /// A fixed clock, for tests.
    clock: Option<Now>,
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
        show_document: window.and_then(|window| window.show_document.as_ref()).is_some_and(|show| show.support),
        date_prompt: raw_params.pointer("/capabilities/experimental/orgDatePrompt").and_then(Value::as_bool)
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
        conceal: highlight::Conceal {
            links: options.link_descriptive.unwrap_or(true),
            emphasis: options.hide_emphasis_markers.unwrap_or(false),
        },
        saving: HashMap::new(),
        todo,
        agenda: options.agenda_config(),
        edit_settings: options.edit_settings(),
        read_date: options.read_date_settings(),
        refile: (options.refile_targets(), options.refile_outline_path(), options.refile_settings()),
        capture_templates: options.capture_templates(),
        org_directory: options.org_directory(),
        default_notes_file: options.default_notes_file(),
        capture: None,
        note: None,
        captures: 0,
        date_prompt: None,
        prompt_copy: None,
        separator_lines: options.separator_lines(),
        clock: options.clock.as_ref().and_then(|clock| clock.to_now()),
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
        let _ = std::fs::remove_dir_all(capture_directory());
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

/// Where the capture buffers of this server are.
fn capture_directory() -> PathBuf {
    std::env::temp_dir().join(format!("org-lsp-capture-{}", std::process::id()))
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
            trigger_characters: Some(vec![":".to_string()]),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(actions::KINDS.iter().map(|kind| CodeActionKind::new(kind)).collect()),
            ..Default::default()
        })),
        execute_command_provider: Some(ExecuteCommandOptions { commands: commands::ids(), ..Default::default() }),
        experimental: Some(json!({ "org": manifest::manifest() })),
        ..Default::default()
    }
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
                self.documents.insert(
                    document.uri.as_str().to_string(),
                    Document::new(document.text, document.version, &self.todo),
                );
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
            let context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
            let actions = [
                commands::TODO_NEXT,
                commands::TODO_PREVIOUS,
                commands::PRIORITY_UP,
                commands::PRIORITY_DOWN,
                commands::DATE_LATER,
                commands::DATE_EARLIER,
                commands::AGENDA_GOTO,
            ]
            .into_iter()
            .filter(|id| dated || !matches!(*id, commands::DATE_LATER | commands::DATE_EARLIER))
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
        let found =
            actions::actions(&document.text, &document.parse, offset, only, &self.edit_settings, self.date_time());
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
        Some(found.iter().map(to_lsp).collect())
    }

    /// TODO keywords and tags in a headline (`pcomplete`).
    fn completion(&mut self, params: lsp_types::CompletionParams) -> Option<lsp_types::CompletionResponse> {
        let position = params.text_document_position;
        let document = self.document(&position.text_document.uri)?;
        let encoding = self.client.encoding;
        let offset = document.index.offset(&document.text, position.position, encoding);
        let (start, items) = completion::complete(&document.text, offset, document.parse.todo_keywords())?;
        let range = Range::new(document.index.position(&document.text, start, encoding), position.position);
        let items = items
            .into_iter()
            .map(|(label, kind)| lsp_types::CompletionItem {
                kind: Some(match kind {
                    completion::Kind::Keyword => lsp_types::CompletionItemKind::KEYWORD,
                    completion::Kind::Tag => lsp_types::CompletionItemKind::ENUM_MEMBER,
                }),
                text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text: label.clone() })),
                label,
                ..Default::default()
            })
            .collect();
        Some(lsp_types::CompletionResponse::Array(items))
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
            Held::Open(Outline::new(&document.parse, &document.text, &uri::file_stem(uri)))
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
                file.1 = Held::Open(outline);
            }
        }
        let outlines: Vec<(Uri, &Outline)> = files.iter().map(|(uri, held)| (uri.clone(), held.outline())).collect();
        views::render(query, &outlines, self.now(), &self.agenda)
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
        let Some(query) = self.view.as_ref().map(|open| open.view.query) else { return };
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
        // The answer to a question, also from where there is no document.
        let input = argument.as_ref().and_then(|value| value.get("input")?.as_str()).map(str::to_string);
        let context: Option<Context> = argument.and_then(|value| serde_json::from_value(value).ok());
        match params.command.as_str() {
            commands::AGENDA_DISPATCH => self.dispatch(input.as_deref()),
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
            commands::CAPTURE => self.capture(context.as_ref()),
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
            if !matches!(open.view.query, Query::Agenda { .. }) {
                return Err("The TODO list has no dates to move through".into());
            }
            return Ok(Target::Agenda { view: uri.clone() });
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
        if let Target::Agenda { .. } = target {
            return self.goto_date(answer);
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
        let outcome = planning::run(command, text, source.offset, answer, repeated, now, &self.read_date)?;
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
    fn goto_date(&mut self, answer: &str) -> std::result::Result<(), Refusal> {
        let read = org_core::read_date::read_date(answer, None, self.date_time(), &self.read_date)?;
        let day = read.date.to_day();
        let open = self.view.as_ref().ok_or("No agenda is open")?;
        let Query::Agenda { span, .. } = open.view.query else {
            return Err("The TODO list has no dates to move through".into());
        };
        let span = Span::from_days(span.days(day));
        Ok(self.open_view_at(Query::Agenda { start: Some(day), span }, Reveal::Day(day))?)
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
        let open = self.date_prompt.as_mut().ok_or("No date prompt is open")?;
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
            commands::CALENDAR_SELECT => {
                // `org-calendar-select`: what was typed, and this day.
                let answer = open.answer_with(at);
                return self.finish_date_prompt(&answer);
            }
            commands::CALENDAR_TYPE => {
                let Some(typed) = context.and_then(|context| context.input.clone()) else {
                    // The minibuffer: ask, starting from what was typed.
                    return Ok(
                        json!({ "input": { "kind": "date", "prompt": open.label.trim_end().trim_end_matches(':'), "value": open.typed } }),
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
            commands::AGENDA_LIST => Ok(self.open_view(Query::Agenda { start: None, span: self.agenda.span })?),
            commands::AGENDA_TODOS => Ok(self.open_view(Query::Todo)?),
            commands::AGENDA_REDO => {
                self.view.as_ref().ok_or("No agenda is open")?;
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_LATER | commands::AGENDA_EARLIER | commands::AGENDA_TODAY => {
                let open = self.view.as_ref().ok_or("No agenda is open")?;
                let Query::Agenda { span, .. } = open.view.query else {
                    return Err("The TODO list has no dates to move through".into());
                };
                let steps = context.as_ref().and_then(|context| context.count).unwrap_or(1);
                let start = match command {
                    commands::AGENDA_LATER => Some(span.shift(open.view.start, steps)),
                    commands::AGENDA_EARLIER => Some(span.shift(open.view.start, -steps)),
                    _ => None,
                };
                Ok(self.open_view(Query::Agenda { start, span })?)
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
                // The new span starts from the day under the cursor.
                let day =
                    self.view.as_ref().filter(|open| matches!(open.view.query, Query::Agenda { .. })).map(|open| {
                        let line = context.as_ref().filter(|context| context.text_document.uri == open.uri);
                        line.and_then(|context| open.on_screen().line(context.position.line)?.day)
                            .unwrap_or(open.view.start)
                    });
                let start = day.map(|day| agenda::starting_day(day, span, &self.agenda));
                self.open_view(Query::Agenda { start, span })?;
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
    /// offers them, and the one picked.
    fn dispatch(&mut self, answer: Option<&str>) -> std::result::Result<Value, Refusal> {
        const VIEWS: [(&str, &str); 2] = [("a", commands::AGENDA_LIST), ("t", commands::AGENDA_TODOS)];
        let Some(answer) = answer else {
            let choices: Vec<Value> =
                VIEWS.iter().map(|(key, id)| json!({ "key": key, "label": commands::title(id) })).collect();
            return Ok(json!({ "input": {
                "kind": "key",
                "prompt": "Press key for agenda command",
                "value": "",
                "choices": choices,
            } }));
        };
        let (_, id) = VIEWS.iter().find(|(key, _)| *key == answer).ok_or("No agenda command for this key")?;
        self.execute(id, None)?;
        Ok(Value::Null)
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
    fn capture(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let templates = self.capture_templates.clone();
        let answering = context.and_then(|context| context.answers.clone());
        let mut keys = context.and_then(|context| context.keys.clone()).unwrap_or_default();
        if answering.is_none() {
            keys.push_str(context.and_then(|context| context.input.as_deref()).unwrap_or(""));
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
        let mut answers = answering.unwrap_or_default();
        if let (Some(_), Some(answer)) =
            (&context.and_then(|context| context.answers.as_ref()), context.and_then(|context| context.input.clone()))
        {
            answers.push(answer);
        }
        // Where the capture started, for `%f`, `%F` and `%a`.
        let from = context.map(|context| &context.text_document.uri).filter(|uri| !self.is_view(uri));
        let file = from.map(|uri| {
            (uri::file_name(uri), uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default())
        });
        let annotation = match (from, context) {
            (Some(uri), Some(context)) => self.link_to(uri, context.position),
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
        let tags_column = self.edit_settings.todo.tags_column;
        let placed = capture::place(&snapshot, &keywords, &target, kind, &filled, &properties, tags_column)?;
        if template.immediate_finish == Some(true) {
            let done = placed.finish(placed.shown());
            self.apply_capture(&target_uri, version, &snapshot, &done);
            return Ok(Value::Null);
        }
        // The capture buffer: a file named after the target, in a
        // directory of the server's own, one for each capture, so that an
        // editor that still has the last one open does not mix them up.
        self.captures += 1;
        let directory = capture_directory().join(self.captures.to_string());
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
        let directory = capture_directory().join(self.captures.to_string());
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
            let tags_column = self.edit_settings.todo.tags_column;
            let placed = capture::place(&now, &keywords, target, *kind, &edited, properties, tags_column)?;
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

    /// Where the entry at `heading` of `file` will stand in the agenda
    /// once `file` reads `text`, if that is on another line than the one
    /// the cursor is on.
    fn followed(&mut self, file: &Uri, text: &str, heading: usize, from: (Position, &ViewLine)) -> Option<Follow> {
        let query = self.view.as_ref()?.view.query;
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
