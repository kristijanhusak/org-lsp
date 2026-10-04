//! The server loop: initialisation, document sync and request dispatch.

use std::collections::HashMap;
use std::error::Error;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{
    CodeActionRequest, DocumentSymbolRequest, FoldingRangeRequest, Request as _, SemanticTokensFullRequest,
    SemanticTokensRangeRequest,
};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    DocumentSymbolParams, DocumentSymbolResponse, FoldingRangeParams, FoldingRangeProviderCapability, InitializeParams,
    InitializeResult, OneOf, OptionalVersionedTextDocumentIdentifier, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensRangeParams, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, TextDocumentEdit, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, Uri, WorkspaceEdit,
};
use org_syntax::{TextSize, TodoKeywords};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::document::Document;
use crate::line_index::Encoding;
use crate::{actions, folding, highlight, symbols};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Settings a client may pass as `initializationOptions`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Options {
    /// The default TODO sequence, written like Org's `#+TODO:` line:
    /// `["TODO", "NEXT", "|", "DONE"]`. Documents can override it.
    todo_keywords: Option<Vec<String>>,
}

/// What the server remembers about the client.
struct Client {
    encoding: Encoding,
    hierarchical_symbols: bool,
    document_changes: bool,
}

struct Server {
    client: Client,
    todo: TodoKeywords,
    documents: HashMap<String, Document>,
}

/// Run the server until the client asks it to exit.
pub fn run(connection: Connection) -> Result<()> {
    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;

    let general = params.capabilities.general.as_ref();
    let text_document = params.capabilities.text_document.as_ref();
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
    };

    // Unknown or malformed options must not keep the server from starting.
    let options: Options =
        params.initialization_options.and_then(|options| serde_json::from_value(options).ok()).unwrap_or_default();
    let todo = options
        .todo_keywords
        .map(TodoKeywords::from_sequence)
        .filter(|keywords| !keywords.is_empty())
        .unwrap_or_default();

    let result = InitializeResult {
        capabilities: capabilities(client.encoding),
        server_info: Some(ServerInfo {
            name: "org-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;

    let mut server = Server { client, todo, documents: HashMap::new() };
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
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn capabilities(encoding: Encoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::INCREMENTAL),
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
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(actions::KINDS.iter().map(|kind| CodeActionKind::new(kind)).collect()),
            ..Default::default()
        })),
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

impl Server {
    fn document(&self, uri: &Uri) -> Option<&Document> {
        self.documents.get(uri.as_str())
    }

    fn request(&self, request: Request) -> Response {
        macro_rules! dispatch {
            ($($request:ty => $handler:ident,)*) => {
                match request.method.as_str() {
                    $(<$request>::METHOD => match parameters(request) {
                        Ok((id, params)) => respond(id, self.$handler(params)),
                        Err(response) => response,
                    },)*
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
                self.documents.insert(
                    document.uri.as_str().to_string(),
                    Document::new(document.text, document.version, &self.todo),
                );
            }
            DidChangeTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidChangeTextDocumentParams>(notification) else { return };
                if let Some(document) = self.documents.get_mut(params.text_document.uri.as_str()) {
                    document.apply(
                        params.content_changes,
                        params.text_document.version,
                        self.client.encoding,
                        &self.todo,
                    );
                }
            }
            DidCloseTextDocument::METHOD => {
                let Some(params) = parse::<lsp_types::DidCloseTextDocumentParams>(notification) else { return };
                self.documents.remove(params.text_document.uri.as_str());
            }
            _ => {}
        }
    }

    fn semantic_tokens(&self, uri: &Uri, range: Option<lsp_types::Range>) -> Option<SemanticTokens> {
        let document = self.document(uri)?;
        let tokens = highlight::tokens(&document.syntax(), document.parse.todo_keywords());
        let encoding = self.client.encoding;
        let only = range.map(|range| document.index.text_range(&document.text, range, encoding));
        let data = highlight::encode(&tokens, &document.text, &document.index, encoding, only);
        Some(SemanticTokens { result_id: None, data })
    }

    fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Option<SemanticTokens> {
        self.semantic_tokens(&params.text_document.uri, None)
    }

    fn semantic_tokens_range(&self, params: SemanticTokensRangeParams) -> Option<SemanticTokens> {
        self.semantic_tokens(&params.text_document.uri, Some(params.range))
    }

    fn folding_ranges(&self, params: FoldingRangeParams) -> Option<Vec<lsp_types::FoldingRange>> {
        let document = self.document(&params.text_document.uri)?;
        Some(folding::folding_ranges(&document.syntax(), &document.text, &document.index))
    }

    fn document_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        let uri = &params.text_document.uri;
        let document = self.document(uri)?;
        let (root, text, index, encoding) = (document.syntax(), &document.text, &document.index, self.client.encoding);
        Some(if self.client.hierarchical_symbols {
            DocumentSymbolResponse::Nested(symbols::document_symbols(&root, text, index, encoding))
        } else {
            DocumentSymbolResponse::Flat(symbols::symbol_information(&root, text, index, encoding, uri))
        })
    }

    fn code_actions(&self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let document = self.document(uri)?;
        let encoding = self.client.encoding;
        let offset = document.index.offset(&document.text, params.range.start, encoding);
        let found = actions::actions(
            &document.syntax(),
            document.parse.todo_keywords(),
            TextSize::new(offset as u32),
            params.context.only.as_deref(),
        );
        let to_lsp = |action: &actions::Action| {
            let range = document.index.range(&document.text, action.edit.range, encoding);
            let edit = action.edit.to_lsp(range);
            // A versioned edit lets the client reject it if the text has
            // changed since; not every client supports that form.
            let workspace_edit = if self.client.document_changes {
                WorkspaceEdit {
                    document_changes: Some(lsp_types::DocumentChanges::Edits(vec![TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            uri: uri.clone(),
                            version: Some(document.version),
                        },
                        edits: vec![OneOf::Left(edit)],
                    }])),
                    ..Default::default()
                }
            } else {
                #[allow(clippy::mutable_key_type)]
                let mut changes = HashMap::new();
                changes.insert(uri.clone(), vec![edit]);
                WorkspaceEdit { changes: Some(changes), ..Default::default() }
            };
            CodeActionOrCommand::CodeAction(action.to_lsp(workspace_edit))
        };
        Some(found.iter().map(to_lsp).collect())
    }
}
