//! End-to-end tests: a client talks to the server over the protocol, both
//! in memory and through the real binary's standard streams.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized};
use lsp_types::request::{
    CodeActionRequest, DocumentSymbolRequest, FoldingRangeRequest, Initialize, SemanticTokensFullRequest,
    SemanticTokensRangeRequest, Shutdown,
};
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentChanges, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRangeParams, InitializeResult, OneOf, Position, PositionEncodingKind, Range,
    SemanticTokensParams, SemanticTokensRangeParams, SemanticTokensResult, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextEdit, Uri, VersionedTextDocumentIdentifier,
};
use serde_json::{json, Value};

const DOCUMENT: &str = "\
#+TITLE: Test
* TODO First :tag:
  Body with *bold* text.
** Child
   - item one
     continued
* Second
";

struct Client {
    connection: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
    pub initialize: InitializeResult,
}

impl Client {
    /// Start a server and initialise it with the given client capabilities
    /// and initialization options.
    fn start(capabilities: Value, options: Value) -> Client {
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut client = Client {
            connection: client_side,
            server: Some(server),
            next_id: 0,
            initialize: InitializeResult::default(),
        };
        let result = client.raw_request(
            <Initialize as lsp_types::request::Request>::METHOD,
            json!({ "capabilities": capabilities, "initializationOptions": options }),
        );
        client.initialize = serde_json::from_value(result.response_result.expect("initialize result")).unwrap();
        client.notify::<Initialized>(lsp_types::InitializedParams {});
        client
    }

    fn raw_request(&mut self, method: &str, params: Value) -> Response {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        self.connection.sender.send(Message::Request(request)).unwrap();
        loop {
            match self.connection.receiver.recv().expect("server answers") {
                Message::Response(response) if response.id == id => return response,
                _ => {}
            }
        }
    }

    fn request<R: lsp_types::request::Request>(&mut self, params: R::Params) -> R::Result {
        let response = self.raw_request(R::METHOD, serde_json::to_value(params).unwrap());
        serde_json::from_value(response.response_result.expect("a result, not an error")).unwrap()
    }

    fn notify<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        let notification =
            Notification { method: N::METHOD.to_string(), params: serde_json::to_value(params).unwrap() };
        self.connection.sender.send(Message::Notification(notification)).unwrap();
    }

    fn open(&self, uri: &Uri, text: &str) {
        self.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "org".to_string(),
                version: 1,
                text: text.to_string(),
            },
        });
    }

    fn code_actions(&mut self, uri: &Uri, position: Position, only: &'static str) -> Vec<lsp_types::CodeAction> {
        let actions = self
            .request::<CodeActionRequest>(CodeActionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range: Range::new(position, position),
                context: CodeActionContext {
                    diagnostics: Vec::new(),
                    only: Some(vec![CodeActionKind::new(only)]),
                    trigger_kind: None,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .unwrap_or_default();
        actions
            .into_iter()
            .map(|action| match action {
                CodeActionOrCommand::CodeAction(action) => action,
                CodeActionOrCommand::Command(command) => panic!("unexpected command {command:?}"),
            })
            .collect()
    }

    fn shutdown(mut self) {
        self.request::<Shutdown>(());
        self.notify::<Exit>(());
        self.server.take().unwrap().join().expect("server exits cleanly");
    }
}

fn uri() -> Uri {
    "file:///tmp/test.org".parse().unwrap()
}

fn identifier() -> TextDocumentIdentifier {
    TextDocumentIdentifier { uri: uri() }
}

/// The edits of a code action, whichever form the server used.
fn edits(action: &lsp_types::CodeAction) -> Vec<TextEdit> {
    let edit = action.edit.as_ref().expect("the action carries an edit");
    if let Some(DocumentChanges::Edits(changes)) = &edit.document_changes {
        return changes
            .iter()
            .flat_map(|change| change.edits.iter())
            .map(|edit| match edit {
                OneOf::Left(edit) => edit.clone(),
                OneOf::Right(annotated) => annotated.text_edit.clone(),
            })
            .collect();
    }
    edit.changes.as_ref().expect("changes").values().flatten().cloned().collect()
}

#[test]
fn announces_capabilities() {
    let client = Client::start(json!({}), Value::Null);
    let capabilities = &client.initialize.capabilities;
    assert_eq!(capabilities.position_encoding, Some(PositionEncodingKind::UTF16));
    assert!(capabilities.folding_range_provider.is_some());
    assert!(capabilities.document_symbol_provider.is_some());
    let capabilities = serde_json::to_value(capabilities).unwrap();
    assert_eq!(capabilities["textDocumentSync"]["change"], 2);
    assert_eq!(
        capabilities["codeActionProvider"]["codeActionKinds"],
        json!([
            "org.todo.next",
            "org.todo.previous",
            "org.priority.up",
            "org.priority.down",
            "org.timestamp.up",
            "org.timestamp.down",
            "org.date.later",
            "org.date.earlier"
        ])
    );
    let legend = &capabilities["semanticTokensProvider"]["legend"];
    assert_eq!(legend["tokenTypes"][0], "headline");
    assert_eq!(legend["tokenModifiers"][0], "level1");
    assert_eq!(client.initialize.server_info.as_ref().unwrap().name, "org-lsp");
    client.shutdown();
}

#[test]
fn negotiates_position_encoding() {
    let client = Client::start(json!({ "general": { "positionEncodings": ["utf-16", "utf-8"] } }), Value::Null);
    assert_eq!(client.initialize.capabilities.position_encoding, Some(PositionEncodingKind::UTF8));
    client.shutdown();
    let client = Client::start(json!({ "general": { "positionEncodings": ["utf-32", "utf-16"] } }), Value::Null);
    assert_eq!(client.initialize.capabilities.position_encoding, Some(PositionEncodingKind::UTF32));
    client.shutdown();
}

#[test]
fn semantic_tokens_full_and_range() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), DOCUMENT);
    let full = client.request::<SemanticTokensFullRequest>(SemanticTokensParams {
        text_document: identifier(),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    let Some(SemanticTokensResult::Tokens(full)) = full else { panic!("expected tokens") };
    // `#+TITLE:` is the first token: line 0, column 0, eight characters, type `keyword`.
    let keyword = &full.data[0];
    assert_eq!((keyword.delta_line, keyword.delta_start, keyword.length, keyword.token_type), (0, 0, 8, 9));
    // Stars of the first headline: next line, level-1 modifier.
    let stars = &full.data[1];
    assert_eq!((stars.delta_line, stars.delta_start, stars.length, stars.token_type), (1, 0, 1, 1));
    assert_eq!(stars.token_modifiers_bitset, 1);
    assert_eq!(full.data.len(), 11);

    let range = client.request::<SemanticTokensRangeRequest>(SemanticTokensRangeParams {
        text_document: identifier(),
        range: Range::new(Position::new(6, 0), Position::new(7, 0)),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    let Some(lsp_types::SemanticTokensRangeResult::Tokens(range)) = range else { panic!("expected tokens") };
    // Only the last headline: stars and title.
    assert_eq!(range.data.len(), 2);
    assert_eq!(range.data[0].delta_line, 6);
    client.shutdown();
}

#[test]
fn folding_ranges() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), DOCUMENT);
    let ranges = client
        .request::<FoldingRangeRequest>(FoldingRangeParams {
            text_document: identifier(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .unwrap();
    let lines: Vec<(u32, u32)> = ranges.iter().map(|range| (range.start_line, range.end_line)).collect();
    assert_eq!(lines, [(1, 5), (3, 5), (4, 5)]);
    client.shutdown();
}

#[test]
fn document_symbols_nested_or_flat() {
    let params = || DocumentSymbolParams {
        text_document: identifier(),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let capabilities = json!({ "textDocument": { "documentSymbol": { "hierarchicalDocumentSymbolSupport": true } } });
    let mut client = Client::start(capabilities, Value::Null);
    client.open(&uri(), DOCUMENT);
    let Some(DocumentSymbolResponse::Nested(symbols)) = client.request::<DocumentSymbolRequest>(params()) else {
        panic!("expected nested symbols")
    };
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "First");
    assert_eq!(symbols[0].detail.as_deref(), Some("TODO :tag:"));
    assert_eq!(symbols[0].children.as_ref().unwrap()[0].name, "Child");
    assert_eq!(symbols[1].name, "Second");
    client.shutdown();

    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), DOCUMENT);
    let Some(DocumentSymbolResponse::Flat(symbols)) = client.request::<DocumentSymbolRequest>(params()) else {
        panic!("expected flat symbols")
    };
    let names: Vec<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["First", "Child", "Second"]);
    assert_eq!(symbols[1].container_name.as_deref(), Some("First"));
    client.shutdown();
}

#[test]
fn todo_cycle_round_trip() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), DOCUMENT);

    // The cursor sits in the body of "First"; the action edits its headline.
    let cursor = Position::new(2, 10);
    let actions = client.code_actions(&uri(), cursor, "org.todo.next");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].title, "Cycle TODO state forward (→ DONE)");
    assert_eq!(actions[0].kind, Some(CodeActionKind::new("org.todo.next")));
    // The keyword changes and, as in Org, the tags move to their column.
    let edit = &edits(&actions[0])[0];
    assert_eq!(edit.range, Range::new(Position::new(1, 2), Position::new(1, 12)));
    assert_eq!(edit.new_text, format!("DONE First{}", " ".repeat(59)));
    // Without `documentChanges` support the plain `changes` form is used.
    assert!(actions[0].edit.as_ref().unwrap().document_changes.is_none());

    // Apply the edit the way an editor would, then ask again.
    client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(edit.range),
            range_length: None,
            text: edit.new_text.clone(),
        }],
    });
    let actions = client.code_actions(&uri(), cursor, "org.todo.next");
    assert_eq!(actions[0].title, "Cycle TODO state forward (→ none)");
    let edit = &edits(&actions[0])[0];
    assert_eq!(edit.range, Range::new(Position::new(1, 2), Position::new(1, 12)));
    assert_eq!(edit.new_text, format!("First{}", " ".repeat(5)));

    // Asking for every Org action returns both directions, of the TODO
    // state and of the priority.
    assert_eq!(client.code_actions(&uri(), cursor, "org.todo").len(), 2);
    assert_eq!(client.code_actions(&uri(), cursor, "org").len(), 4);
    // Nothing applies before the first headline or for unrelated kinds.
    assert!(client.code_actions(&uri(), Position::new(0, 3), "org").is_empty());
    assert!(client.code_actions(&uri(), cursor, "quickfix").is_empty());
    client.shutdown();
}

#[test]
fn versioned_edits_when_supported() {
    let capabilities = json!({ "workspace": { "workspaceEdit": { "documentChanges": true } } });
    let mut client = Client::start(capabilities, Value::Null);
    client.open(&uri(), DOCUMENT);
    let actions = client.code_actions(&uri(), Position::new(6, 0), "org.todo.next");
    let edit = actions[0].edit.as_ref().unwrap();
    let Some(DocumentChanges::Edits(changes)) = &edit.document_changes else { panic!("expected documentChanges") };
    assert_eq!(changes[0].text_document.version, Some(1));
    assert_eq!(edits(&actions[0])[0].new_text, "TODO ");
    client.shutdown();
}

#[test]
fn todo_keywords_from_initialization_options() {
    let options = json!({ "todoKeywords": ["TODO", "WAIT", "|", "DONE", "KILL"] });
    let mut client = Client::start(json!({}), options);
    client.open(&uri(), "* TODO a\n* WAIT b\n");
    let next = client.code_actions(&uri(), Position::new(0, 0), "org.todo.next");
    assert_eq!(edits(&next[0])[0].new_text, "WAIT");
    let previous = client.code_actions(&uri(), Position::new(1, 0), "org.todo.previous");
    assert_eq!(edits(&previous[0])[0].new_text, "TODO");
    client.shutdown();

    // Options the server does not understand are ignored.
    let mut client = Client::start(json!({}), json!({ "todoKeywords": 5, "unknown": true }));
    client.open(&uri(), "* a\n");
    let next = client.code_actions(&uri(), Position::new(0, 0), "org.todo.next");
    assert_eq!(edits(&next[0])[0].new_text, "TODO ");
    client.shutdown();
}

#[test]
fn positions_use_the_negotiated_encoding() {
    let text = "* 😀é TODO x\n";
    for (encoding, title_start) in [("utf-8", 2), ("utf-16", 2), ("utf-32", 2)] {
        let mut client = Client::start(json!({ "general": { "positionEncodings": [encoding] } }), Value::Null);
        client.open(&uri(), text);
        let actions = client.code_actions(&uri(), Position::new(0, 0), "org.todo.next");
        let edit = &edits(&actions[0])[0];
        assert_eq!(edit.range.start, Position::new(0, title_start), "{encoding}");
        client.shutdown();
    }
    // A keyword after wide characters: the column depends on the encoding.
    let text = "* TODO 😀é\n";
    for (encoding, column) in [("utf-8", 13), ("utf-16", 10), ("utf-32", 9)] {
        let capabilities = json!({ "general": { "positionEncodings": [encoding] } });
        let mut client = Client::start(capabilities, Value::Null);
        client.open(&uri(), text);
        // Insert at the end of the headline using this encoding's column.
        client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, column), Position::new(0, column))),
                range_length: None,
                text: " :tag:".to_string(),
            }],
        });
        let symbols = client.request::<DocumentSymbolRequest>(DocumentSymbolParams {
            text_document: identifier(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
        let Some(DocumentSymbolResponse::Flat(symbols)) = symbols else { panic!("expected symbols") };
        assert_eq!(symbols[0].name, "😀é", "{encoding}");
        assert_eq!(symbols[0].location.range.end, Position::new(0, column + 6), "{encoding}");
        client.shutdown();
    }
}

#[test]
fn unknown_requests_and_documents() {
    let mut client = Client::start(json!({}), Value::Null);
    let response = client.raw_request("textDocument/hover", json!({}));
    assert_eq!(response.response_result.unwrap_err().code, lsp_server::ErrorCode::MethodNotFound as i32);

    let response = client.raw_request("textDocument/foldingRange", json!({ "wrong": true }));
    assert_eq!(response.response_result.unwrap_err().code, lsp_server::ErrorCode::InvalidParams as i32);

    // A document that was never opened yields `null`, not an error.
    let ranges = client.request::<FoldingRangeRequest>(FoldingRangeParams {
        text_document: identifier(),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    assert!(ranges.is_none());

    // Closing forgets the document.
    client.open(&uri(), DOCUMENT);
    client.notify::<DidCloseTextDocument>(DidCloseTextDocumentParams { text_document: identifier() });
    assert!(client.code_actions(&uri(), Position::new(1, 0), "org").is_empty());
    client.shutdown();
}

// ----- the real binary

fn frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn read_message(reader: &mut impl BufRead) -> Value {
    let mut length = 0;
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0, "server closed its output");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn binary_speaks_lsp_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_org-lsp"))
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut send = |message: Value| stdin.write_all(&frame(&message)).and_then(|()| stdin.flush()).unwrap();
    send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "capabilities": {} } }));
    let initialized = read_message(&mut stdout);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "org-lsp");

    send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
    send(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": { "uri": "file:///tmp/a.org", "languageId": "org", "version": 1, "text": "* TODO a\nb\n" }
    }}));
    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/foldingRange", "params": {
        "textDocument": { "uri": "file:///tmp/a.org" }
    }}));
    let folds = read_message(&mut stdout);
    assert_eq!(folds["result"], json!([{ "startLine": 0, "endLine": 1 }]));

    send(json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown" }));
    assert_eq!(read_message(&mut stdout)["id"], 3);
    send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    assert!(child.wait().unwrap().success());
}

#[test]
fn binary_prints_its_version_and_rejects_unknown_arguments() {
    let binary = env!("CARGO_BIN_EXE_org-lsp");
    let version = Command::new(binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), format!("org-lsp {}", env!("CARGO_PKG_VERSION")));

    let unknown = Command::new(binary).arg("--bogus").output().unwrap();
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8(unknown.stderr).unwrap().contains("unknown argument"));
}
