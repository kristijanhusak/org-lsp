//! The server loop: initialisation, document sync and request dispatch.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
use std::rc::Rc;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
    ShowMessage,
};
use lsp_types::request::{
    ApplyWorkspaceEdit, CodeActionRequest, DocumentSymbolRequest, ExecuteCommand, FoldingRangeRequest, GotoDefinition,
    Request as _, SemanticTokensFullRequest, SemanticTokensRangeRequest, ShowDocument, ShowMessageRequest,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    DocumentSymbolParams, DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult,
    Location, MessageActionItem, MessageType, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, ShowDocumentParams, ShowMessageParams,
    ShowMessageRequestParams, TextDocumentEdit, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, Uri, WorkspaceEdit,
};
use org_core::agenda::{self, Config as AgendaConfig, Now, Span};
use org_core::date::DateTime;
use org_core::outline::Outline;
use org_core::todo::{Settings as TodoSettings, Target};
use org_syntax::{Parse, TextSize, TodoKeywords};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Options;
use crate::document::Document;
use crate::line_index::{Encoding, LineIndex};
use crate::views::{self, EntryRef, Query, View};
use crate::workspace::{Source, Workspace};
use crate::{actions, cli, commands, folding, highlight, manifest, symbols, uri};

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
}

/// An outgoing request whose answer the server acts on.
enum Pending {
    /// An edit to a file; the agenda is rebuilt once it is applied.
    Edit,
    /// The agenda dispatcher's menu.
    Dispatch,
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
}

/// Where a command was invoked: a document and a place in it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    text_document: lsp_types::TextDocumentIdentifier,
    position: Position,
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
    todo: TodoKeywords,
    agenda: AgendaConfig,
    todo_settings: TodoSettings,
    /// A fixed clock, for tests.
    clock: Option<Now>,
    documents: HashMap<String, Document>,
    workspace: Workspace,
    view: Option<OpenView>,
    /// Files the server asked the client to edit and has not seen change yet.
    edited: HashSet<String>,
    /// The line to put the cursor on once the client shows the current view.
    reveal: Option<u32>,
}

/// Run the server until the client asks it to exit.
pub fn run(connection: Connection) -> Result<()> {
    let (id, raw_params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(raw_params.clone())?;

    // Unknown or malformed options must not keep the server from starting.
    let options: Options =
        params.initialization_options.and_then(|options| serde_json::from_value(options).ok()).unwrap_or_default();

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
    };

    let todo = options
        .todo_keywords
        .clone()
        .map(TodoKeywords::from_sequence)
        .filter(|keywords| !keywords.is_empty())
        .unwrap_or_default();

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
        todo,
        agenda: options.agenda.to_config(),
        todo_settings: options.todo_settings(),
        clock: options.clock.as_ref().and_then(|clock| clock.to_now()),
        documents: HashMap::new(),
        workspace: Workspace::new(options.agenda_files, folders),
        view: None,
        edited: HashSet::new(),
        reveal: None,
    };
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
    Ok(())
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

fn is_view_uri(uri: &Uri) -> bool {
    uri.as_str().strip_prefix(views::SCHEME).is_some_and(|rest| rest.starts_with(':'))
}

impl Server {
    fn document(&self, uri: &Uri) -> Option<&Document> {
        self.documents.get(uri.as_str())
    }

    /// The agenda, if `uri` names it.
    fn view_at(&self, uri: &Uri) -> Option<&View> {
        self.view.as_ref().filter(|open| open.uri == *uri).map(|open| &open.view)
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
        match self.pending.remove(&response.id) {
            Some(Pending::Edit) => self.refresh_view(),
            Some(Pending::Dispatch) => {
                let title = response
                    .response_result
                    .ok()
                    .and_then(|result| result.get("title").and_then(Value::as_str).map(str::to_string));
                let chosen = [commands::AGENDA_LIST, commands::AGENDA_TODOS]
                    .into_iter()
                    .find(|id| title.as_deref() == Some(command_title(id)));
                if let Some(id) = chosen {
                    if let Err(message) = self.execute(id, None) {
                        self.show_message(MessageType::ERROR, message);
                    }
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
                        Ok((id, params)) => match self.execute_command(params) {
                            Ok(()) => respond(id, Value::Null),
                            Err(message) => Response::new_err(id, ErrorCode::RequestFailed as i32, message),
                        },
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
                // The agenda is the server's own text; the client's copy is not read.
                if self.is_view(&document.uri) {
                    if let Some(open) = self.view.as_mut().filter(|open| open.uri == document.uri) {
                        open.client = Some(Document::new(document.text, document.version, &self.todo));
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
                // The client's copy of the agenda follows the server's.
                let (encoding, todo) = (self.client.encoding, &self.todo);
                let view = self.view.as_mut().filter(|open| open.uri == params.text_document.uri);
                if let Some(copy) = view.and_then(|open| open.client.as_mut()) {
                    copy.apply(params.content_changes, params.text_document.version, encoding, todo);
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
                if self.edited.remove(key) {
                    self.refresh_view();
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
                self.documents.remove(params.text_document.uri.as_str());
                // The agenda was closed: stop rebuilding it.
                if self.view.as_ref().is_some_and(|open| open.uri == params.text_document.uri) {
                    self.remove_view_file();
                    self.view = None;
                    self.reveal = None;
                }
            }
            _ => {}
        }
    }

    fn semantic_tokens(&self, uri: &Uri, range: Option<Range>) -> Option<SemanticTokens> {
        let encoding = self.client.encoding;
        if let Some(view) = self.view_at(uri) {
            let only = range.map(|range| view.index.text_range(&view.text, range, encoding));
            let data = highlight::encode(&view.tokens, &view.text, &view.index, encoding, only);
            return Some(SemanticTokens { result_id: None, data });
        }
        let document = self.document(uri)?;
        let tokens = highlight::tokens(&document.syntax(), document.parse.todo_keywords());
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
        Some(folding::folding_ranges(&document.syntax(), &document.text, &document.index))
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
        if self.client.document_changes {
            WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Edits(vec![TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri: uri.clone(), version },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                }])),
                ..Default::default()
            }
        } else {
            #[allow(clippy::mutable_key_type)]
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);
            WorkspaceEdit { changes: Some(changes), ..Default::default() }
        }
    }

    fn code_actions(&mut self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let only = params.context.only.as_deref();
        if let Some(view) = self.view_at(uri) {
            // On an agenda line the actions are commands: the server
            // edits the entry's file and rebuilds the agenda.
            view.line(params.range.start.line)?.entry.as_ref()?;
            let context = json!({ "textDocument": { "uri": uri }, "position": params.range.start });
            let actions = [commands::TODO_NEXT, commands::TODO_PREVIOUS, commands::AGENDA_GOTO]
                .into_iter()
                .filter(|id| actions::requested(id, only))
                .map(|id| {
                    let title = command_title(id).to_string();
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
        let found = actions::actions(
            &document.syntax(),
            &document.text,
            document.parse.todo_keywords(),
            TextSize::new(offset as u32),
            only,
            &self.todo_settings,
            self.date_time(),
        );
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

    /// From an agenda line to the entry it shows.
    fn definition(&mut self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let position = params.text_document_position_params;
        let reference = self.view_at(&position.text_document.uri)?.line(position.position.line)?.entry.clone()?;
        self.locate(&reference).map(GotoDefinitionResponse::Scalar)
    }

    fn text_document_content(&mut self, params: &Value) -> Value {
        let uri = params.get("uri").and_then(Value::as_str);
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
        let files = self.agenda_files();
        let outlines: Vec<(Uri, &Outline)> = files.iter().map(|(uri, held)| (uri.clone(), held.outline())).collect();
        views::render(query, &outlines, self.now(), &self.agenda)
    }

    /// Build the view for `query` and make it the current one.
    fn set_view(&mut self, query: Query) {
        let view = self.render(query);
        match &mut self.view {
            Some(open) => open.view = view,
            None if self.client.virtual_documents => {
                if let Ok(uri) = views::VIRTUAL_URI.parse() {
                    self.view = Some(OpenView { uri, file: None, view, client: None });
                }
            }
            None => {
                // The client cannot fetch the view, so it gets a file.
                let directory = std::env::temp_dir().join(format!("org-lsp-{}", std::process::id()));
                let file = directory.join(views::FILE_NAME);
                if std::fs::create_dir_all(&directory).is_ok() {
                    if let Some(uri) = uri::from_path(&file) {
                        self.view = Some(OpenView { uri, file: Some(file), view, client: None });
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

    /// Ask the client to show the view with the cursor on `line`.
    fn show_view(&mut self, line: u32) {
        let Some(uri) = self.view.as_ref().map(|open| open.uri.clone()) else { return };
        let params = ShowDocumentParams {
            uri,
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(Position::new(line, 0), Position::new(line, 0))),
        };
        self.send_request::<ShowDocument>(params, None);
    }

    /// Move the cursor as `open_view` wanted, if the client by now shows
    /// the text that the line number belongs to.
    fn reveal_when_current(&mut self) {
        let current =
            self.view.as_ref().and_then(|open| Some(open.client.as_ref()?.text == open.view.text)).unwrap_or(false);
        if current {
            if let Some(line) = self.reveal.take() {
                self.show_view(line);
            }
        }
    }

    /// Show `query` in the client, with the cursor on today's date.
    fn open_view(&mut self, query: Query) -> std::result::Result<(), String> {
        if !self.client.show_document {
            return Err("This editor cannot open documents on the server's request".to_string());
        }
        self.set_view(query);
        let Some(open) = &self.view else { return Err("Cannot create the agenda".to_string()) };
        let line = open.view.home_line();
        match &open.client {
            // The client shows this very text already.
            Some(copy) if copy.text == open.view.text => self.show_view(line),
            // The client shows the old text. A cursor position sent now
            // would be applied to those lines, so it waits until the client
            // has replaced them; see `reveal_when_current`.
            Some(_) => {
                self.reveal = Some(line);
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

    /// Rebuild the view after its files changed; the cursor stays.
    fn refresh_view(&mut self) {
        let Some(query) = self.view.as_ref().map(|open| open.view.query) else { return };
        let view = self.render(query);
        let Some(open) = &mut self.view else { return };
        if view.text != open.view.text {
            open.view = view;
            self.publish_view();
        } else {
            // The same text may still stand for entries that moved.
            open.view = view;
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

    fn execute_command(&mut self, params: ExecuteCommandParams) -> std::result::Result<(), String> {
        let context = params.arguments.into_iter().next().and_then(|value| serde_json::from_value(value).ok());
        self.execute(&params.command, context)
    }

    fn execute(&mut self, command: &str, context: Option<Context>) -> std::result::Result<(), String> {
        match command {
            commands::AGENDA_DISPATCH => {
                let actions = [commands::AGENDA_LIST, commands::AGENDA_TODOS]
                    .map(|id| MessageActionItem { title: command_title(id).to_string(), properties: HashMap::new() });
                let params = ShowMessageRequestParams {
                    typ: MessageType::INFO,
                    message: "Org agenda".to_string(),
                    actions: Some(actions.to_vec()),
                };
                self.send_request::<ShowMessageRequest>(params, Some(Pending::Dispatch));
                Ok(())
            }
            commands::AGENDA_LIST => self.open_view(Query::Agenda { start: None, span: self.agenda.span }),
            commands::AGENDA_TODOS => self.open_view(Query::Todo),
            commands::AGENDA_REDO => {
                self.view.as_ref().ok_or("No agenda is open")?;
                self.refresh_view();
                Ok(())
            }
            commands::AGENDA_LATER | commands::AGENDA_EARLIER | commands::AGENDA_TODAY => {
                let open = self.view.as_ref().ok_or("No agenda is open")?;
                let Query::Agenda { span, .. } = open.view.query else {
                    return Err("The TODO list has no dates to move through".to_string());
                };
                let start = match command {
                    commands::AGENDA_LATER => Some(span.shift(open.view.start, 1)),
                    commands::AGENDA_EARLIER => Some(span.shift(open.view.start, -1)),
                    _ => None,
                };
                self.open_view(Query::Agenda { start, span })
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
                        line.and_then(|context| open.view.line(context.position.line)?.day).unwrap_or(open.view.start)
                    });
                let start = day.map(|day| agenda::starting_day(day, span, &self.agenda));
                self.open_view(Query::Agenda { start, span })
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
            commands::TODO_NEXT | commands::TODO_PREVIOUS => {
                let context = context.ok_or("This command needs the cursor position")?;
                self.cycle_todo(&context, command)
            }
            other => Err(format!("org-lsp has no command `{other}`")),
        }
    }

    /// The entry on the agenda line of `context`.
    fn entry_at(&self, context: Option<&Context>) -> Option<EntryRef> {
        let context = context?;
        self.view_at(&context.text_document.uri)?.line(context.position.line)?.entry.clone()
    }

    /// Change the TODO state of the entry at `context`, which is a place
    /// in an Org file or a line of the agenda.
    fn cycle_todo(&mut self, context: &Context, kind: &str) -> std::result::Result<(), String> {
        let encoding = self.client.encoding;
        let target = if kind == commands::TODO_NEXT { Target::Next } else { Target::Previous };
        let (settings, now) = (self.todo_settings.clone(), self.date_time());
        let uri = context.text_document.uri.clone();
        // The file to edit and the place of the entry in it.
        let (file, reference) = if self.is_view(&uri) {
            let reference = self.entry_at(Some(context)).ok_or("No entry on this line")?;
            (reference.uri.clone(), Some(reference))
        } else {
            (uri, None)
        };
        let position = context.position;
        let found = self.with_source(&file, |text, index, parse, outline, version| {
            let offset = match &reference {
                Some(reference) => {
                    let entry = views::resolve(reference, outline).ok_or("The entry is gone; rebuild the agenda")?;
                    outline.entries[entry].offset
                }
                None => index.offset(text, position, encoding),
            };
            let edits = actions::todo_edits(text, parse.todo_keywords(), offset, &target, &settings, now)?;
            let edits: Vec<TextEdit> = edits
                .into_iter()
                .map(|edit| TextEdit { range: index.range(text, edit.range, encoding), new_text: edit.new_text })
                .collect();
            Ok::<_, String>((edits, version))
        });
        let (edits, version) = found.ok_or("Cannot read the file of this entry")??;
        if edits.is_empty() {
            return Ok(());
        }
        let edit = self.workspace_edit(&file, version, edits);
        self.edited.insert(file.as_str().to_string());
        // No label: some editors print it every time an edit is applied.
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.send_request::<ApplyWorkspaceEdit>(params, Some(Pending::Edit));
        Ok(())
    }
}

fn command_title(id: &str) -> &'static str {
    commands::COMMANDS.iter().find(|command| command.id == id).map_or("", |command| command.title)
}
