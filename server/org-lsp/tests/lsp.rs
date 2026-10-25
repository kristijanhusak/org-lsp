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
    /// What the server asked while a request waited for its answer.
    asked: Vec<Request>,
}

impl Client {
    /// Start a server and initialise it with the given client capabilities
    /// and initialization options.
    fn start(capabilities: Value, options: Value) -> Client {
        private_state();
        // The tests' own options only, whatever configuration file the
        // machine has.
        let mut options = if options.is_null() { json!({}) } else { options };
        if let Some(fields) = options.as_object_mut() {
            fields.entry("config_file").or_insert(json!(false));
            // The IDs an Emacs of the machine left in its
            // `org-id-locations-file` are not read.
            fields.entry("org_id_locations_file").or_insert(json!(NO_ID_LOCATIONS));
        }
        // An editor with org-lsp's plugin, unless the test says otherwise.
        let mut capabilities = capabilities;
        if capabilities.pointer("/experimental/orgClient").is_none() {
            capabilities["experimental"]["orgClient"] = json!(true);
        }
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut client = Client {
            connection: client_side,
            server: Some(server),
            next_id: 0,
            initialize: InitializeResult::default(),
            asked: Vec::new(),
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
                Message::Request(request) => self.asked.push(request),
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

    /// The next notification `method` from the server.
    fn notification(&self, method: &str) -> Value {
        loop {
            match self.connection.receiver.recv_timeout(std::time::Duration::from_secs(5)).expect("a notification") {
                Message::Notification(notification) if notification.method == method => return notification.params,
                _ => {}
            }
        }
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
            "org.date.earlier",
            "org.clock.in",
            "org.clock.out",
            "org.clock.cancel",
            "org.checkbox.toggle",
            "org.table.recalculate",
            "org",
            "quickfix"
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
    let legend =
        serde_json::to_value(&client.initialize.capabilities.semantic_tokens_provider).unwrap()["legend"].clone();
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
    // Its value, `org-document-title`.
    let title = &full.data[1];
    assert_eq!((title.delta_line, title.delta_start, title.length), (0, 9, 4));
    assert_eq!(legend["tokenTypes"][title.token_type as usize], "documentTitle");
    // Stars of the first headline: next line, level-1 modifier.
    let stars = &full.data[2];
    assert_eq!((stars.delta_line, stars.delta_start, stars.length, stars.token_type), (1, 0, 1, 1));
    assert_eq!(stars.token_modifiers_bitset, 1);
    assert_eq!(full.data.len(), 12);

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
    assert_eq!(
        client.code_actions(&uri(), cursor, "org.todo").len()
            + client.code_actions(&uri(), cursor, "org.priority").len(),
        4
    );
    // And clocking in, with no clock running, and the three ways to
    // archive.
    assert_eq!(client.code_actions(&uri(), cursor, "org").len(), 8);
    assert_eq!(client.code_actions(&uri(), cursor, "org.clock")[0].title, "Clock in");
    let archive: Vec<String> =
        client.code_actions(&uri(), cursor, "org.archive").iter().map(|action| action.title.clone()).collect();
    assert_eq!(
        archive,
        ["Archive the subtree", "Archive the subtree under its Archive sibling", "Toggle the ARCHIVE tag"]
    );
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
    let options = json!({ "org_todo_keywords": ["TODO", "WAIT", "|", "DONE", "KILL"] });
    let mut client = Client::start(json!({}), options);
    client.open(&uri(), "* TODO a\n* WAIT b\n");
    let next = client.code_actions(&uri(), Position::new(0, 0), "org.todo.next");
    assert_eq!(edits(&next[0])[0].new_text, "WAIT");
    let previous = client.code_actions(&uri(), Position::new(1, 0), "org.todo.previous");
    assert_eq!(edits(&previous[0])[0].new_text, "TODO");
    client.shutdown();

    // Options the server does not understand are ignored.
    let mut client = Client::start(json!({}), json!({ "org_todo_keywords": 5, "unknown": true }));
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
    let response = client.raw_request("textDocument/signatureHelp", json!({}));
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
    private_state();
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

/// `org-indent-line` on a new line (on-type formatting after `RET`) and
/// `org-indent-region` on a range, under `adaptIndentation`.
#[test]
fn formats_tables_and_tags() {
    let mut client = Client::start(json!({}), json!({ "org_tags_column": -20 }));
    let capabilities = serde_json::to_value(&client.initialize.capabilities).unwrap();
    assert_eq!(capabilities["documentFormattingProvider"], true);
    client.open(&uri(), "* Ünï :t:\n|a|bb|\n|-\n|ccc|12|\nText\n");
    let edits = client
        .request::<lsp_types::request::Formatting>(lsp_types::DocumentFormattingParams {
            text_document: identifier(),
            options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let changed: Vec<(u32, u32, &str)> =
        edits.iter().map(|edit| (edit.range.start.line, edit.range.end.character, edit.new_text.as_str())).collect();
    assert_eq!(
        changed,
        [(0, 9, "* Ünï            :t:"), (1, 6, "| a   | bb |"), (2, 2, "|-----+----|"), (3, 8, "| ccc | 12 |"),]
    );
    assert!(edits.iter().all(|edit| edit.range.start.character == 0 && edit.range.end.line == edit.range.start.line));

    // A Windows file's lines the same, their `\r`s kept: `\r\n` ends a
    // line, so the edits stop before it.
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "* Ünï :t:\r\n|a|bb|\r\n|-\r\n|ccc|12|\r\nText\r\n".to_string(),
        }],
    });
    let edits = client
        .request::<lsp_types::request::Formatting>(lsp_types::DocumentFormattingParams {
            text_document: identifier(),
            options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let changed: Vec<(u32, u32, &str)> =
        edits.iter().map(|edit| (edit.range.start.line, edit.range.end.character, edit.new_text.as_str())).collect();
    assert_eq!(
        changed,
        [(0, 9, "* Ünï            :t:"), (1, 6, "| a   | bb |"), (2, 2, "|-----+----|"), (3, 8, "| ccc | 12 |"),]
    );
}

#[test]
fn indents_new_lines_and_ranges() {
    let mut client = Client::start(json!({}), json!({ "org_adapt_indentation": true }));
    let capabilities = serde_json::to_value(&client.initialize.capabilities).unwrap();
    assert_eq!(capabilities["documentOnTypeFormattingProvider"]["firstTriggerCharacter"], "\n");
    assert_eq!(capabilities["documentRangeFormattingProvider"], true);
    client.open(&uri(), "** Entry\n\n- item\n\nText\n");
    let on_type = |client: &mut Client, line: u32, character: u32| {
        client.request::<lsp_types::request::OnTypeFormatting>(lsp_types::DocumentOnTypeFormattingParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: identifier(),
                position: Position::new(line, character),
            },
            ch: "\n".to_string(),
            options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
        })
    };
    // The blank line below the headline: past the stars.
    let edits = on_type(&mut client, 1, 0).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, Range::new(Position::new(1, 0), Position::new(1, 0)));
    assert_eq!(edits[0].new_text, "   ");
    // A list item stays where it is.
    assert_eq!(on_type(&mut client, 2, 0), Some(vec![]));

    // Re-indented, and the property aligned.
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "** Entry\n:PROPERTIES:\n:ID: x\n:END:\n- item\n\nText\n".to_string(),
        }],
    });
    let edits = client
        .request::<lsp_types::request::RangeFormatting>(lsp_types::DocumentRangeFormattingParams {
            text_document: identifier(),
            range: Range::new(Position::new(0, 0), Position::new(7, 0)),
            options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let changed: Vec<(u32, &str)> = edits.iter().map(|edit| (edit.range.start.line, edit.new_text.as_str())).collect();
    assert_eq!(
        changed,
        [(1, "   :PROPERTIES:"), (2, "   :ID:       x"), (3, "   :END:"), (4, "   - item"), (6, "   Text")]
    );

    // A table with a line in the range is aligned whole, and the tags of
    // the headlines in it go to their column; the rest stays.
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version: 3 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "* A :x:\n|a|bb|\n|ccc|d|\n* B :y:\n".to_string(),
        }],
    });
    let edits = client
        .request::<lsp_types::request::RangeFormatting>(lsp_types::DocumentRangeFormattingParams {
            text_document: identifier(),
            range: Range::new(Position::new(2, 0), Position::new(3, 0)),
            options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let changed: Vec<(u32, &str)> = edits.iter().map(|edit| (edit.range.start.line, edit.new_text.as_str())).collect();
    assert_eq!(changed, [(1, "| a   | bb |"), (2, "| ccc | d  |")]);
    client.shutdown();
}

/// `org-indent-mode` for clients that show it: the prefix of each line,
/// the prefix of the lines it wraps into, and the hidden leading stars,
/// after every change while the file shows so, and once more when it
/// stops. Such a file is not indented for real (`adaptIndentation`).
#[test]
fn sends_virtual_indentation() {
    let capabilities = json!({ "experimental": { "orgVirtualIndent": true } });
    let mut client = Client::start(capabilities, json!({ "org_adapt_indentation": true }));
    client.open(&uri(), "#+STARTUP: indent\n* A\ntext\n** B\n- item\n");
    let sent = client.notification("org-lsp/virtualIndent");
    assert_eq!(sent["uri"], uri().as_str());
    assert_eq!(sent["version"], 1);
    assert_eq!(sent["lines"], json!([[1, 0, 2, 0], [2, 2, 2, 0], [3, 1, 4, 1], [4, 4, 6, 0]]));
    // No hard indentation in such a file.
    let edit = client.request::<lsp_types::request::OnTypeFormatting>(lsp_types::DocumentOnTypeFormattingParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: identifier(),
            position: Position::new(2, 0),
        },
        ch: "\n".to_string(),
        options: lsp_types::FormattingOptions { tab_size: 8, insert_spaces: true, ..Default::default() },
    });
    assert_eq!(edit, Some(vec![]));
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "* A\ntext\n".to_string(),
        }],
    });
    let sent = client.notification("org-lsp/virtualIndent");
    assert_eq!((sent["version"].clone(), sent["lines"].clone()), (json!(2), json!([])));
    client.shutdown();
}

/// `org_indent_indentation_per_level` widens the prefixes, and with
/// `org_indent_mode_turns_on_hiding_stars` off the stars stay.
#[test]
fn sends_virtual_indentation_of_other_widths() {
    let capabilities = json!({ "experimental": { "orgVirtualIndent": true } });
    let options = json!({ "org_indent_indentation_per_level": 4, "org_indent_mode_turns_on_hiding_stars": false });
    let client = Client::start(capabilities, options);
    client.open(&uri(), "#+STARTUP: indent\n* A\ntext\n** B\n- item\n");
    let sent = client.notification("org-lsp/virtualIndent");
    // As `org-indent-mode` shows it in Emacs with 4 per level.
    assert_eq!(sent["lines"], json!([[1, 0, 2, 0], [2, 2, 2, 0], [3, 3, 6, 0], [4, 6, 8, 0]]));
    client.shutdown();
}

/// `org_fontify_whole_heading_line`: the lines of the headlines, for a
/// client that highlights lines from edge to edge, each in the look of
/// its level as `org_n_level_faces` and `org_cycle_level_faces` say; again
/// when they move, and with no lines when there are none.
#[test]
fn sends_the_lines_of_headlines_with_whole_heading_lines() {
    let capabilities = json!({ "experimental": { "orgLineHighlights": true } });
    let options =
        json!({ "org_fontify_whole_heading_line": true, "org_n_level_faces": 2, "org_cycle_level_faces": false });
    let client = Client::start(capabilities, options);
    client.open(&uri(), "* A\ntext\n** B\n*** C\n");
    let sent = client.notification("org-lsp/lineHighlights");
    assert_eq!(sent["uri"], uri().as_str());
    let lines = json!([
        { "line": 0, "highlight": "headline.level1" },
        { "line": 2, "highlight": "headline.level2" },
        { "line": 3, "highlight": "headline.level2" },
    ]);
    assert_eq!(sent["lines"], lines);
    let change = |version: i32, text: &str| {
        client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        });
    };
    change(2, "text\n* A\n");
    let sent = client.notification("org-lsp/lineHighlights");
    assert_eq!(sent["lines"], json!([{ "line": 1, "highlight": "headline.level1" }]));
    change(3, "text\n");
    let sent = client.notification("org-lsp/lineHighlights");
    assert_eq!(sent["lines"], json!([]));
    client.shutdown();
}

/// `org-src-fontify-natively`: where the code of source, export and inline
/// source blocks is, for a client that colors it in its language, with
/// the editor's name of the language; not what an example block holds.
/// Again after each change, and once with no blocks when they are gone.
#[test]
fn sends_the_code_blocks() {
    let capabilities = json!({ "experimental": { "orgCodeBlocks": true } });
    let client = Client::start(capabilities, json!({ "org_src_lang_modes": { "ipython": "python" } }));
    let text = "* Code\n#+begin_src emacs-lisp :results none\n(defun f ()\n  1)\n#+end_src\n\
                #+BEGIN_EXPORT html\n<b>é</b>\n#+END_EXPORT\n\
                é src_sh[:exports code]{ls -l} and src_ipython{1}\n\
                #+begin_example\n#+begin_src python\nx = 1\n#+end_src\n#+end_example\n";
    client.open(&uri(), text);
    let sent = client.notification("org-lsp/codeBlocks");
    assert_eq!(sent["uri"], uri().as_str());
    assert_eq!(sent["version"], 1);
    let range = |a: u32, b: u32, c: u32, d: u32| json!({ "start": { "line": a, "character": b }, "end": { "line": c, "character": d } });
    assert_eq!(
        sent["blocks"],
        json!([
            { "range": range(2, 0, 3, 4), "language": "elisp", "name": "emacs-lisp", "kind": "src" },
            { "range": range(6, 0, 6, 8), "language": "html", "name": "html", "kind": "export" },
            { "range": range(8, 24, 8, 29), "language": "bash", "name": "sh", "kind": "inline" },
            { "range": range(8, 47, 8, 48), "language": "python", "name": "ipython", "kind": "inline" },
        ])
    );
    let change = |version: i32, text: &str| {
        client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        });
    };
    change(2, "Moved\n\n#+begin_src C++\nint x;\n#+end_src\n");
    let sent = client.notification("org-lsp/codeBlocks");
    assert_eq!(sent["version"], 2);
    assert_eq!(
        sent["blocks"],
        json!([{ "range": range(3, 0, 3, 6), "language": "cpp", "name": "C++", "kind": "src" }])
    );
    // The code changed where its range did not.
    change(3, "Moved\n\n#+begin_src C++\nint y;\n#+end_src\n");
    let sent = client.notification("org-lsp/codeBlocks");
    assert_eq!(sent["version"], 3);
    change(4, "No code\n");
    let sent = client.notification("org-lsp/codeBlocks");
    assert_eq!((sent["version"].clone(), sent["blocks"].clone()), (json!(4), json!([])));
    client.shutdown();
    // The option says the same for clients that cannot announce it.
    let client = Client::start(json!({}), json!({ "code_blocks": true }));
    client.open(&uri(), "#+begin_src sh\necho\n#+end_src\n");
    let sent = client.notification("org-lsp/codeBlocks");
    assert_eq!(sent["blocks"][0]["language"], "bash");
    client.shutdown();
}

/// A client that did not ask is not told where code is.
#[test]
fn no_code_blocks_unless_asked() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "#+begin_src sh\necho\n#+end_src\n");
    // An answer comes after what the server sent for the opening.
    client.next_id += 1;
    let id = RequestId::from(client.next_id);
    let params = json!({ "textDocument": { "uri": uri() } });
    let request = Request { id: id.clone(), method: "textDocument/foldingRange".to_string(), params };
    client.connection.sender.send(Message::Request(request)).unwrap();
    loop {
        match client.connection.receiver.recv_timeout(std::time::Duration::from_secs(5)).expect("an answer") {
            Message::Response(response) if response.id == id => break,
            Message::Notification(notification) => {
                assert_ne!(notification.method, "org-lsp/codeBlocks", "{notification:?}")
            }
            _ => {}
        }
    }
    client.shutdown();
}

/// A client that does not show `org-lsp/virtualIndent` (Zed, Helix) gets
/// `org-indent-mode`'s prefixes as inlay hints of blanks; one that does,
/// or says so with the `virtualIndent` option, gets none of those.
#[test]
fn virtual_indentation_as_inlay_hints() {
    let mut client = Client::start(json!({}), Value::Null);
    assert!(client.initialize.capabilities.inlay_hint_provider.is_some());
    client.open(&uri(), "#+STARTUP: indent\n* A\ntext\n** B\n- item\n");
    let hints = client
        .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
            text_document: identifier(),
            range: Range::new(Position::new(0, 0), Position::new(3, 0)),
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let shown: Vec<(u32, u32, String)> = hints
        .iter()
        .map(|hint| match &hint.label {
            lsp_types::InlayHintLabel::String(label) => (hint.position.line, hint.position.character, label.clone()),
            _ => panic!("a plain label"),
        })
        .collect();
    assert_eq!(shown, [(2, 0, "  ".to_string()), (3, 0, " ".to_string())]);
    client.shutdown();
    for (capabilities, options) in [
        (json!({ "experimental": { "orgVirtualIndent": true } }), Value::Null),
        (json!({}), json!({ "virtual_indent": true })),
    ] {
        // Hints are offered (`org-clock-display`'s times), but no
        // prefixes among them.
        let mut client = Client::start(capabilities, options);
        assert!(client.initialize.capabilities.inlay_hint_provider.is_some());
        client.open(&uri(), "#+STARTUP: indent\n* A\ntext\n** B\n- item\n");
        let hints = client
            .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
                text_document: identifier(),
                range: Range::new(Position::new(0, 0), Position::new(3, 0)),
                work_done_progress_params: Default::default(),
            })
            .unwrap();
        assert!(hints.is_empty(), "{hints:?}");
        client.shutdown();
    }
}

/// `org_pretty_entities`: an entity is hidden, and its character is an
/// inlay hint in its place; on a comment line it stays.
#[test]
fn shows_entities_as_their_characters() {
    let mut client = Client::start(json!({}), json!({ "org_pretty_entities": true }));
    client.open(&uri(), "An \\alpha{} and \\to.\n# \\beta\n");
    let hints = client
        .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
            text_document: identifier(),
            range: Range::new(Position::new(0, 0), Position::new(2, 0)),
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    let shown: Vec<(u32, u32, String)> = hints
        .iter()
        .map(|hint| match &hint.label {
            lsp_types::InlayHintLabel::String(label) => (hint.position.line, hint.position.character, label.clone()),
            _ => panic!("a plain label"),
        })
        .collect();
    assert_eq!(shown, [(0, 3, "α".to_string()), (0, 16, "→".to_string())]);
    let tokens = client
        .request::<lsp_types::request::SemanticTokensFullRequest>(lsp_types::SemanticTokensParams {
            text_document: identifier(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .unwrap();
    let lsp_types::SemanticTokensResult::Tokens(tokens) = tokens else { panic!("tokens") };
    let hidden = 1 << 23;
    let mut at = (0, 0);
    let mut hid = Vec::new();
    for token in &tokens.data {
        at = if token.delta_line == 0 {
            (at.0, at.1 + token.delta_start)
        } else {
            (at.0 + token.delta_line, token.delta_start)
        };
        if token.token_modifiers_bitset & hidden != 0 {
            hid.push((at.0, at.1, token.length));
        }
    }
    assert_eq!(hid, [(0, 3, 8), (0, 16, 3)]);
    client.shutdown();
}

/// A client that shows what stands in for hidden text itself
/// (`orgDisplay`, Neovim) gets it as `org-lsp/display` after every change
/// (entities, sub- and superscripts, shrunk fields), and once more when
/// there is none; neither inlay hints nor hidden tokens for it.
#[test]
fn sends_what_stands_in_for_hidden_text() {
    let capabilities = json!({ "experimental": { "orgDisplay": true } });
    let mut client = Client::start(capabilities, json!({ "org_pretty_entities": true }));
    client.open(&uri(), "An \\alpha{} and x^2.\n");
    let sent = client.notification("org-lsp/display");
    assert_eq!(sent["uri"], uri().as_str());
    assert_eq!(sent["version"], 1);
    assert_eq!(sent["items"], json!([[0, 3, 0, 11, "α"], [0, 17, 0, 19, "²"]]));
    let hints = client
        .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
            text_document: identifier(),
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            work_done_progress_params: Default::default(),
        })
        .unwrap();
    assert!(hints.is_empty(), "{hints:?}");
    let tokens = client
        .request::<lsp_types::request::SemanticTokensFullRequest>(lsp_types::SemanticTokensParams {
            text_document: identifier(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .unwrap();
    let lsp_types::SemanticTokensResult::Tokens(tokens) = tokens else { panic!("tokens") };
    assert!(tokens.data.iter().all(|token| token.token_modifiers_bitset & (1 << 23) == 0), "{tokens:?}");
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "An alpha.\n".to_string(),
        }],
    });
    let sent = client.notification("org-lsp/display");
    assert_eq!((sent["version"].clone(), sent["items"].clone()), (json!(2), json!([])));
    client.shutdown();
}

/// `org_display_custom_times`: each timestamp's date and time shown in the
/// custom formats, a repeater left as it is; `#+STARTUP: customtime` turns
/// it on for a file, and `org.timestamp.overlays.toggle` off and on.
#[test]
fn shows_timestamps_in_custom_formats() {
    let capabilities = json!({ "experimental": { "orgDisplay": true } });
    let settings =
        json!({ "org_display_custom_times": true, "org_timestamp_custom_formats": ["%d.%m.%Y", "%d.%m.%Y %H:%M"] });
    let client = Client::start(capabilities.clone(), settings.clone());
    client.open(&uri(), "<2026-10-05 Mon 10:00 +1w> and [2026-10-06 Tue]\n");
    let sent = client.notification("org-lsp/display");
    assert_eq!(sent["items"], json!([[0, 1, 0, 21, "05.10.2026 10:00"], [0, 32, 0, 46, "06.10.2026"]]));
    client.shutdown();

    // For a client that does not show them itself, inlay hints; the
    // command turns them off and on again.
    let capabilities_hints = json!({ "textDocument": { "inlayHint": {} } });
    let mut client = Client::start(capabilities_hints, settings);
    client.open(&uri(), "<2026-10-05 Mon 10:00 +1w> and [2026-10-06 Tue]\n");
    let hints = |client: &mut Client| -> Vec<String> {
        let params = json!({ "textDocument": { "uri": uri().as_str() }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 1, "character": 0 } } });
        let result = client.raw_request("textDocument/inlayHint", params).response_result.unwrap();
        result.as_array().unwrap().iter().map(|hint| hint["label"].as_str().unwrap_or_default().to_string()).collect()
    };
    assert_eq!(hints(&mut client), ["05.10.2026 10:00", "06.10.2026"]);
    let toggle = |client: &mut Client| {
        let argument = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
        let result = client
            .raw_request(
                "workspace/executeCommand",
                json!({ "command": "org.timestamp.overlays.toggle", "arguments": [argument] }),
            )
            .response_result
            .unwrap();
        result["message"].clone()
    };
    assert_eq!(toggle(&mut client), "Time stamp overlays removed");
    assert!(hints(&mut client).is_empty());
    assert_eq!(toggle(&mut client), "Time stamps are overlaid with custom format");
    assert_eq!(hints(&mut client).len(), 2);
    client.shutdown();

    // Org's formats, for the file that asks for them.
    let client = Client::start(capabilities, Value::Null);
    client.open(&uri(), "#+STARTUP: customtime\n<2026-10-05 Mon>\n");
    let sent = client.notification("org-lsp/display");
    assert_eq!(sent["items"], json!([[1, 1, 1, 15, "10/05/26 Mon"]]));
    client.shutdown();
}

/// org-bullets (`org_bullets_mode`): each headline's last star shows as
/// the bullet of its level, the list repeating, and the stars before it are
/// hidden as `org-hide-leading-stars` hides them; the bullets of lists and
/// the marks of checkboxes as org-bullets.nvim shows them. Each takes the
/// look of the token it stands for.
#[test]
fn shows_headline_stars_as_bullets() {
    let capabilities = json!({ "experimental": { "orgDisplay": true, "orgVirtualIndent": true } });
    let settings = json!({
        "org_bullets_mode": true,
        "org_bullets_bullet_list": ["●", "○"],
        "org_bullets_checkboxes": { "half": "~" },
    });
    let client = Client::start(capabilities, settings);
    client.open(&uri(), "* One\n** Two\n*** Three\n- [X] a\n  + [-] b\n1. [ ] c\n");
    let indent = client.notification("org-lsp/virtualIndent");
    let hidden: Vec<(u64, u64)> = indent["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| (line[0].as_u64().unwrap(), line[3].as_u64().unwrap()))
        .collect();
    assert_eq!(hidden, [(1, 1), (2, 2)]);
    let sent = client.notification("org-lsp/display");
    assert_eq!(
        sent["items"],
        json!([
            [0, 0, 0, 1, "●", "stars.level1"],
            [1, 1, 1, 2, "○", "stars.level2"],
            [2, 2, 2, 3, "●", "stars.level3"],
            [3, 0, 3, 1, "•", "bullet"],
            [3, 3, 3, 4, "✓", "checkbox.checked"],
            [4, 2, 4, 3, "•", "bullet"],
            [4, 5, 4, 6, "~", "checkbox.partial"]
        ])
    );
    client.shutdown();
    // Off, lists and checkboxes are left as they are.
    let settings =
        json!({ "org_bullets_mode": true, "org_bullets_list_bullet": false, "org_bullets_checkboxes": false });
    let client = Client::start(json!({ "experimental": { "orgDisplay": true } }), settings);
    client.open(&uri(), "* One\n- [X] a\n");
    let sent = client.notification("org-lsp/display");
    assert_eq!(sent["items"], json!([[0, 0, 0, 1, "◉", "stars.level1"]]));
    client.shutdown();
}

/// `#+STARTUP:` decides for its file: `entitiespretty` shows entities as
/// their characters with the setting off, `literallinks` leaves links as
/// they are with `org_link_descriptive` on; `entitiesplain` and
/// `descriptivelinks` the other way.
#[test]
fn startup_decides_entities_and_links() {
    let hidden_and_hints = |settings: Value, text: &str| -> (usize, Vec<String>) {
        let mut client = Client::start(json!({}), settings);
        client.open(&uri(), text);
        let hints = client
            .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
                text_document: identifier(),
                range: Range::new(Position::new(0, 0), Position::new(3, 0)),
                work_done_progress_params: Default::default(),
            })
            .unwrap();
        let labels = hints
            .iter()
            .filter_map(|hint| match &hint.label {
                lsp_types::InlayHintLabel::String(label) => Some(label.clone()),
                _ => None,
            })
            .collect();
        let tokens = client
            .request::<lsp_types::request::SemanticTokensFullRequest>(lsp_types::SemanticTokensParams {
                text_document: identifier(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .unwrap();
        let lsp_types::SemanticTokensResult::Tokens(tokens) = tokens else { panic!("tokens") };
        let hidden = tokens.data.iter().filter(|token| token.token_modifiers_bitset & (1 << 23) != 0).count();
        client.shutdown();
        (hidden, labels)
    };
    let text = "#+STARTUP: entitiespretty literallinks\n\\alpha and [[https://orgmode.org][Org]]\n";
    // The entity alone hidden, shown as its character; the link whole.
    assert_eq!(hidden_and_hints(json!({}), text), (1, vec!["α".to_string()]));
    let plain = "#+STARTUP: entitiesplain descriptivelinks\n\\alpha and [[https://orgmode.org][Org]]\n";
    let (hidden, hints) =
        hidden_and_hints(json!({ "org_pretty_entities": true, "org_link_descriptive": false }), plain);
    assert!(hints.is_empty() && hidden > 0, "{hidden} {hints:?}");
}

/// `org_pretty_entities` with sub- and superscripts: those whose every
/// character has a raised or lowered form hidden, shown in Unicode as an
/// inlay hint; with `org_use_sub_superscripts = "{}"`, those in braces
/// only.
#[test]
fn shows_sub_and_superscripts() {
    let text = "x^2 and H_{2}O and z^* \\alpha_1\n";
    type Hints = Vec<(u32, u32, String)>;
    type Hidden = Vec<(u32, u32, u32)>;
    let shown = |settings: Value| -> (Hints, Hidden) {
        let mut client = Client::start(json!({}), settings);
        client.open(&uri(), text);
        let hints = client
            .request::<lsp_types::request::InlayHintRequest>(lsp_types::InlayHintParams {
                text_document: identifier(),
                range: Range::new(Position::new(0, 0), Position::new(1, 0)),
                work_done_progress_params: Default::default(),
            })
            .unwrap();
        let mut hints: Vec<(u32, u32, String)> = hints
            .iter()
            .map(|hint| match &hint.label {
                lsp_types::InlayHintLabel::String(label) => {
                    (hint.position.line, hint.position.character, label.clone())
                }
                _ => panic!("a plain label"),
            })
            .collect();
        hints.sort();
        let tokens = client
            .request::<lsp_types::request::SemanticTokensFullRequest>(lsp_types::SemanticTokensParams {
                text_document: identifier(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .unwrap();
        let lsp_types::SemanticTokensResult::Tokens(tokens) = tokens else { panic!("tokens") };
        let hidden = 1 << 23;
        let mut at = (0, 0);
        let mut hid = Vec::new();
        for token in &tokens.data {
            at = if token.delta_line == 0 {
                (at.0, at.1 + token.delta_start)
            } else {
                (at.0 + token.delta_line, token.delta_start)
            };
            if token.token_modifiers_bitset & hidden != 0 {
                hid.push((at.0, at.1, token.length));
            }
        }
        client.shutdown();
        (hints, hid)
    };
    let (hints, hid) = shown(json!({ "org_pretty_entities": true }));
    assert_eq!(
        hints,
        [(0, 1, "²".to_string()), (0, 9, "₂".to_string()), (0, 23, "α".to_string()), (0, 29, "₁".to_string())]
    );
    assert_eq!(hid, [(0, 1, 2), (0, 9, 4), (0, 23, 6), (0, 29, 2)]);
    let (hints, _) = shown(json!({ "org_pretty_entities": true, "org_use_sub_superscripts": "{}" }));
    assert_eq!(hints, [(0, 9, "₂".to_string()), (0, 23, "α".to_string())]);
    let (hints, _) =
        shown(json!({ "org_pretty_entities": true, "org_pretty_entities_include_sub_superscripts": false }));
    assert_eq!(hints, [(0, 23, "α".to_string())]);
}

/// `org-toggle-custom-properties-visibility`: the lines of the properties
/// of `org_custom_properties` for a client that hides lines, then none,
/// for it to show them again; a client that cannot is told so.
#[test]
fn hides_custom_properties_and_shows_them_again() {
    let toggle = |client: &mut Client| {
        let context = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
        client.raw_request(
            "workspace/executeCommand",
            json!({ "command": "org.custom.properties.toggle", "arguments": [context] }),
        )
    };
    let text = "* A\n:PROPERTIES:\n:ID: 1\n:CATEGORY: x\n:END:\n* B\n:PROPERTIES:\n:id: 2\n:END:\n";
    let options = json!({ "org_custom_properties": ["ID"] });
    let mut client = Client::start(json!({ "experimental": { "orgHiddenLines": true } }), options.clone());
    client.open(&uri(), text);
    let hidden = toggle(&mut client).response_result.expect("a result");
    assert_eq!(hidden, json!({ "hiddenLines": { "uri": uri().as_str(), "lines": [2, 7] } }));
    let shown = toggle(&mut client).response_result.expect("a result");
    assert_eq!(shown, json!({ "hiddenLines": { "uri": uri().as_str(), "lines": [] } }));
    client.shutdown();
    let mut client = Client::start(json!({}), options);
    client.open(&uri(), text);
    let refused = toggle(&mut client).response_result.expect_err("an error");
    assert_eq!(refused.message, "This editor cannot hide lines");
    client.shutdown();
}

/// RET (`org-return`): the result carries the edit and where the cursor
/// goes, for the editor to apply as the key is typed; in a source block's
/// code the command does not apply, and the editor's own RET does.
#[test]
fn returns_as_org_does() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "* TODO Write the report :work:\n#+begin_src sh\necho\n#+end_src\n");
    let run = |client: &mut Client, line: u32, character: u32| {
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        client.raw_request("workspace/executeCommand", json!({ "command": "org.return", "arguments": [context] }))
    };
    let result = run(&mut client, 0, 12).response_result.expect("a result");
    let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
    // The title split, the tags aligned again, the rest on a new line.
    let line = "* TODO Write the report :work:";
    let (start, end) = (edit.range.start.character as usize, edit.range.end.character as usize);
    assert_eq!((edit.range.start.line, edit.range.end.line), (0, 0));
    let changed = format!("{}{}{}", &line[..start], edit.new_text, &line[end..]);
    assert_eq!(changed, format!("* TODO Write{}:work:\nthe report", " ".repeat(59)));
    assert_eq!(result["cursor"], json!({ "line": 1, "character": 0 }));
    let refused = run(&mut client, 2, 2).response_result.expect_err("a refusal");
    assert_eq!(refused.data, Some(json!({ "notApplicable": true })));
    client.shutdown();
}

/// For an LSP plugin that shows documents on its own and passes on only an
/// error's message (coc.nvim): the documents to show come as
/// `org-lsp/showDocument`, answered with `org-lsp/showDocumentResult`, and
/// a command that does not apply says so in its result.
#[test]
fn shows_documents_and_refuses_with_notifications() {
    let directory = std::env::temp_dir().join(format!("org-lsp-show-notification-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (from, to) = (directory.join("from.org"), directory.join("to.org"));
    let text = "* See [[file:to.org]]\n#+begin_src sh\necho\n#+end_src\n";
    std::fs::write(&from, text).unwrap();
    std::fs::write(&to, "* There\n").unwrap();
    let uri: Uri = format!("file://{}", from.display()).parse().unwrap();
    let options = json!({ "show_document": true, "show_document_notification": true, "refusal_results": true });
    let mut client = Client::start(json!({}), options);
    client.open(&uri, text);
    let run = |client: &mut Client, command: &str, line: u32, character: u32| {
        let context =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": line, "character": character } });
        client.raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
    };
    // The answer and the notification, as they come.
    let context = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 10 } });
    let open = Request {
        id: RequestId::from(1000),
        method: "workspace/executeCommand".to_string(),
        params: json!({ "command": "org.open", "arguments": [context] }),
    };
    client.connection.sender.send(Message::Request(open)).unwrap();
    let (mut answered, mut show) = (false, None);
    while !answered || show.is_none() {
        match client.connection.receiver.recv_timeout(std::time::Duration::from_secs(30)).expect("the server says") {
            Message::Response(response) if response.id == RequestId::from(1000) => {
                response.response_result.expect("a result");
                answered = true;
            }
            Message::Notification(notification) if notification.method == "org-lsp/showDocument" => {
                show = Some(notification)
            }
            Message::Request(request) => {
                assert_ne!(request.method, "window/showDocument", "shown with window/showDocument")
            }
            _ => {}
        }
    }
    let show = show.unwrap();
    assert_eq!(show.params["params"]["uri"], json!(format!("file://{}", to.display())), "{:?}", show.params);
    let answer = json!({ "id": show.params["id"], "result": { "success": true } });
    client
        .connection
        .sender
        .send(Message::Notification(Notification::new("org-lsp/showDocumentResult".to_string(), answer)))
        .unwrap();
    // RET in a source block's code: the editor's own RET.
    let refused = run(&mut client, "org.return", 2, 2).response_result.expect("a result");
    assert_eq!(refused["notApplicable"], json!(true), "{refused}");
    assert!(refused["message"].is_string(), "{refused}");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// `org-open-line` (C-o): a new line after the cursor, which stays; in a
/// table a row above (`org-special-ctrl-o`), unless that is off.
#[test]
fn opens_lines_and_table_rows() {
    let text = "* A\nsome text\n| a | b |\n";
    let open_line = |options: Value, line: u32, character: u32, count: Option<i64>| -> (String, Value) {
        let mut client = Client::start(json!({}), options);
        client.open(&uri(), text);
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        if let Some(count) = count {
            context["count"] = json!(count);
        }
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": "org.open.line", "arguments": [context] }))
            .response_result
            .expect("a result");
        client.shutdown();
        let edits: Vec<TextEdit> = serde_json::from_value(result["edits"].clone()).unwrap();
        (apply_text_edits(text, &edits), result["cursor"].clone())
    };
    assert_eq!(
        open_line(Value::Null, 1, 4, None),
        ("* A\nsome\n text\n| a | b |\n".to_string(), json!({ "line": 1, "character": 4 }))
    );
    assert_eq!(open_line(Value::Null, 1, 4, Some(2)).0, "* A\nsome\n\n text\n| a | b |\n");
    let (row, _) = open_line(Value::Null, 2, 3, None);
    assert_eq!(row, "* A\nsome text\n|   |   |\n| a | b |\n");
    let (plain, _) = open_line(json!({ "org_special_ctrl_o": false }), 2, 3, None);
    assert_eq!(plain, "* A\nsome text\n| a\n | b |\n");
}

/// `org-display-outline-path`: the headlines above the entry at the
/// cursor, with a count the file's name first, as Emacs echoes them.
#[test]
fn shows_the_outline_path() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "* TODO [#A] A :t:\n** [[x][Link]] B  \n*** C\ntext\n");
    let mut path = |line: u32, count: Option<i64>| -> Value {
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": 0 } });
        if let Some(count) = count {
            context["count"] = json!(count);
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.outline.path", "arguments": [context] }))
            .response_result
            .expect("a result")
    };
    let name = "test.org";
    assert_eq!(path(3, None), json!({ "message": "A/Link B" }));
    assert_eq!(path(3, Some(4)), json!({ "message": format!("{name}/A/Link B") }));
    assert_eq!(path(0, None), Value::Null);
    assert_eq!(path(0, Some(4)), json!({ "message": name }));
    client.shutdown();
}

/// `org-convert-to-odd-levels` and `org-convert-to-oddeven-levels`: a
/// question first, then each headline below the top demoted or promoted,
/// its tags aligned and, with `org-adapt-indentation`, its text indented
/// with it, as Emacs does; the way back refuses even levels.
#[test]
fn converts_files_to_odd_levels_and_back() {
    let convert = |command: &str, text: &str, options: Value| -> std::result::Result<String, String> {
        let mut client = Client::start(json!({}), options);
        client.open(&uri(), text);
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
        let asked = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result;
        let asked = match asked {
            Ok(asked) => asked,
            Err(error) => {
                client.shutdown();
                return Err(error.message);
            }
        };
        assert_eq!(asked["input"]["kind"], "key");
        context["input"] = json!("y");
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .expect("a result");
        client.shutdown();
        let edits: Vec<TextEdit> = serde_json::from_value(result["edits"].clone()).unwrap();
        Ok(apply_text_edits(text, &edits))
    };
    let tags = " ".repeat(67);
    assert_eq!(
        convert("org.levels.odd", "* A\n** B   :tag:\ntext\n*** C\n**** D\n  deep\n* E\n** F\n", Value::Null),
        Ok(format!("* A\n*** B{tags}:tag:\ntext\n***** C\n******* D\n  deep\n* E\n*** F\n"))
    );
    assert_eq!(
        convert(
            "org.levels.odd",
            "* A\n** B   :tag:\n   text\n*** C\n**** D\n     deep\n",
            json!({ "org_adapt_indentation": true })
        ),
        // Emacs makes the 8 columns a tab; org-lsp indents with
        // `indent-tabs-mode` off.
        Ok(format!("* A\n*** B{tags}:tag:\n    text\n***** C\n******* D\n        deep\n"))
    );
    let tags = " ".repeat(68);
    assert_eq!(
        convert("org.levels.oddeven", "* A\n*** B :tag:\n***** C\n******* D\n* E\n", Value::Null),
        Ok(format!("* A\n** B{tags}:tag:\n*** C\n**** D\n* E\n"))
    );
    assert_eq!(
        convert("org.levels.oddeven", "* A\n*** B\n    text\n***** C\n", json!({ "org_adapt_indentation": true })),
        Ok("* A\n** B\n   text\n*** C\n".to_string())
    );
    assert_eq!(
        convert("org.levels.oddeven", "* A\n** B\n*** C\n", Value::Null),
        Err("Not all levels are odd in this file.  Conversion not possible".to_string())
    );
}

/// `returnUsesMetaReturn`: RET on the line of an item's bullet makes the
/// next item, numbered and the rest renumbered as M-RET does, in nested
/// lists too; elsewhere in the item RET stays Org's.
#[test]
fn return_uses_meta_return_on_items() {
    let mut client = Client::start(json!({}), json!({ "return_uses_meta_return": true }));
    let text = "* A\n1. one\n   1. a\n   2. b\n      more\n2. two\n";
    client.open(&uri(), text);
    let mut run = |line: u32, character: u32| -> String {
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": "org.return", "arguments": [context] }))
            .response_result
            .expect("a result");
        let mut edits: Vec<TextEdit> = serde_json::from_value(result["edits"].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        edits.sort_by_key(|edit| std::cmp::Reverse(offset(edit.range.start)));
        let mut changed = text.to_string();
        for edit in edits {
            changed.replace_range(offset(edit.range.start)..offset(edit.range.end), &edit.new_text);
        }
        changed
    };
    assert_eq!(run(2, 7), "* A\n1. one\n   1. a\n   2. \n   3. b\n      more\n2. two\n");
    assert_eq!(run(1, 6), "* A\n1. one\n2. \n   1. a\n   2. b\n      more\n3. two\n");
    // On a line of the item's text, a new line in the item.
    assert_eq!(run(4, 10), "* A\n1. one\n   1. a\n   2. b\n      more\n      \n2. two\n");
    client.shutdown();
}

/// `org.return.item` (`<S-CR>`) makes the next item without
/// `returnUsesMetaReturn`, while `org.return` stays Org's RET.
#[test]
fn return_item_makes_the_next_item() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "1. one\n2. two\n";
    client.open(&uri(), text);
    let mut run = |command: &str| -> String {
        let context = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 6 } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .expect("a result");
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        format!("{}{}{}", &text[..start], edit.new_text, &text[end..])
    };
    assert_eq!(run("org.return.item"), "1. one\n2. \n3. two\n");
    assert_eq!(run("org.return"), "1. one\n   \n2. two\n");
    client.shutdown();
}

/// `org.babel.tangle` writes the blocks' code to their files, as
/// `org-babel-tangle` does, and says how many blocks went; in a source
/// block it is a code action too.
#[test]
fn tangles_the_file() {
    let directory = std::env::temp_dir().join(format!("org-lsp-tangle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "* Code\n#+begin_src sh :tangle bin/run.sh :mkdirp yes :shebang #!/bin/sh\necho one\n#+end_src\n#+begin_src python :tangle yes\nx = 1\n#+end_src\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri, text);
    let context = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 0 } });
    let actions = client
        .raw_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri.as_str() },
                "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 0 } },
                "context": { "diagnostics": [] },
            }),
        )
        .response_result
        .unwrap();
    assert!(
        actions.as_array().unwrap().iter().any(|action| action["command"]["command"] == "org.babel.tangle"),
        "{actions}"
    );
    let result = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.babel.tangle", "arguments": [context] }))
        .response_result
        .unwrap();
    assert_eq!(result["message"], "Tangled 2 code blocks from notes.org");
    assert_eq!(std::fs::read_to_string(directory.join("bin/run.sh")).unwrap(), "#!/bin/sh\necho one\n");
    assert_eq!(std::fs::read_to_string(directory.join("notes.python")).unwrap(), "x = 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(directory.join("bin/run.sh")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// M-RET, M-S-RET, C-RET and C-S-RET: the result carries the edit and the
/// cursor, as with RET.
#[test]
fn inserts_headings_and_items_as_org_does() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "* TODO Write :work:\n- [ ] one\n** Sub\n* Next\n";
    client.open(&uri(), text);
    let mut run = |command: &str, line: u32, character: u32| -> (String, Value) {
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .expect("a result");
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        (format!("{}{}{}", &text[..start], edit.new_text, &text[end..]), result["cursor"].clone())
    };
    // In a list, an item; with a checkbox from M-S-RET.
    assert_eq!(
        run("org.meta.return", 1, 9),
        ("* TODO Write :work:\n- [ ] one\n- \n** Sub\n* Next\n".to_string(), json!({ "line": 2, "character": 2 }))
    );
    assert_eq!(run("org.insert.todo.heading", 1, 9).0, "* TODO Write :work:\n- [ ] one\n- [ ] \n** Sub\n* Next\n");
    // On the headline, a heading after it, with the keyword of M-S-RET.
    assert_eq!(
        run("org.insert.todo.heading", 0, 3),
        ("* TODO Write :work:\n* TODO \n- [ ] one\n** Sub\n* Next\n".to_string(), json!({ "line": 1, "character": 7 }))
    );
    // C-RET and C-S-RET: after the subtree.
    assert_eq!(run("org.insert.heading.after", 1, 0).0, "* TODO Write :work:\n- [ ] one\n** Sub\n* \n* Next\n");
    assert_eq!(
        run("org.insert.todo.heading.after", 0, 0).0,
        "* TODO Write :work:\n- [ ] one\n** Sub\n* TODO \n* Next\n"
    );
    client.shutdown();
}

/// From the keys of modal editors (`below`), M-RET puts the new item
/// after the item, even from its bullet; and lines added after a file's
/// last line go after its end, not after its final newline.
#[test]
fn inserts_below_the_line() {
    let mut client = Client::start(json!({}), Value::Null);
    let mut run = |text: &str, command: &str, context: Value| -> (String, TextEdit, Value) {
        client.open(&uri(), text);
        let mut context = context;
        context["textDocument"] = json!({ "uri": uri().as_str() });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .expect("a result");
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        (format!("{}{}{}", &text[..start], edit.new_text, &text[end..]), edit, result["cursor"].clone())
    };
    let (changed, _, cursor) = run(
        "1. one\n   more\n2. two\n",
        "org.meta.return",
        json!({ "position": { "line": 0, "character": 0 }, "below": true }),
    );
    assert_eq!(changed, "1. one\n   more\n2. \n3. two\n");
    assert_eq!(cursor, json!({ "line": 2, "character": 3 }));
    let (changed, edit, _) = run("| a | bb |\n", "org.return", json!({ "position": { "line": 0, "character": 3 } }));
    assert_eq!(changed, "| a | bb |\n|   |    |\n");
    assert_eq!(edit.range, Range::new(Position::new(0, 10), Position::new(0, 10)));
    assert_eq!(edit.new_text, "\n|   |    |");
    client.shutdown();
}

/// Org's meta keys on a headline: promote and demote it or its subtree,
/// move the subtree; the result carries the edit and the cursor. Off a
/// headline they do not apply, and a drawer is indented by `M-<right>`.
/// The same are code actions on a headline, where they change something.
#[test]
fn edits_the_structure() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "* A\n** B :tag:\ntext\n*** C\n** D\n";
    client.open(&uri(), text);
    let mut run = |command: &str, line: u32, character: u32| -> Result<(String, Value), Value> {
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .map_err(|error| error.data.unwrap_or(Value::Null))?;
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        Ok((format!("{}{}{}", &text[..start], edit.new_text, &text[end..]), result["cursor"].clone()))
    };
    let tags = |stars: usize| format!("{}{}:tag:", "*".repeat(stars), " ".repeat(77 - 5 - stars));
    assert_eq!(run("org.meta.left", 1, 0).unwrap().0, format!("* A\n{} B{}\ntext\n*** C\n** D\n", "*", &tags(1)[3..]));
    assert_eq!(
        run("org.shiftmeta.right", 1, 0).unwrap().0,
        format!("* A\n*** B{}\ntext\n**** C\n** D\n", &tags(3)[5..])
    );
    let (moved, cursor) = run("org.meta.down", 1, 2).unwrap();
    assert_eq!(moved, "* A\n** D\n** B :tag:\ntext\n*** C\n");
    assert_eq!(cursor, json!({ "line": 2, "character": 2 }));
    assert_eq!(run("org.meta.up", 1, 0).unwrap_err(), Value::Null, "no sibling above: an error");
    assert_eq!(run("org.meta.left", 2, 0).unwrap_err(), json!({ "notApplicable": true }), "in text");
    client.open(&uri(), "* A\n:LOGBOOK:\n- x\n:END:\n");
    let indented = client
        .raw_request(
            "workspace/executeCommand",
            json!({ "command": "org.meta.right", "arguments": [{ "textDocument": { "uri": uri().as_str() }, "position": { "line": 1, "character": 0 } }] }),
        )
        .response_result
        .unwrap();
    assert_eq!(indented["message"], "Drawer at point indented");
    client.open(&uri(), text);
    let kinds: Vec<String> = client
        .code_actions(&uri(), Position::new(4, 0), "org")
        .iter()
        .filter_map(|action| action.kind.as_ref().map(|kind| kind.as_str().to_string()))
        .filter(|kind| kind.starts_with("org.meta.") || kind.starts_with("org.shiftmeta."))
        .collect();
    assert_eq!(kinds, ["org.meta.left", "org.meta.right", "org.shiftmeta.left", "org.shiftmeta.right", "org.meta.up"]);
    client.shutdown();
}

/// Org's meta and shifted keys on a list item: indent, outdent and move
/// it, cycle the bullets, go to the next item; the result carries the
/// edit and the cursor. The meta keys are code actions there too, and
/// `C-c C-c` repairs the list.
#[test]
fn edits_list_items() {
    let mut client = Client::start(json!({}), Value::Null);
    let mut run = |text: &str, command: &str, line: u32, character: u32| -> Result<(String, Value), Value> {
        client.open(&uri(), text);
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .map_err(|error| error.data.unwrap_or(Value::Null))?;
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        // The text is ASCII: a character is a byte.
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        Ok((format!("{}{}{}", &text[..start], edit.new_text, &text[end..]), result["cursor"].clone()))
    };
    let text = "- a\n- b\n  - c\n- d\n";
    assert_eq!(run(text, "org.meta.right", 1, 2).unwrap().0, "- a\n  - b\n  - c\n- d\n");
    assert_eq!(run(text, "org.shiftmeta.right", 1, 2).unwrap().0, "- a\n  - b\n    - c\n- d\n");
    assert_eq!(run(text, "org.meta.left", 2, 4).unwrap().0, "- a\n- b\n- c\n- d\n");
    let (moved, cursor) = run(text, "org.meta.down", 1, 3).unwrap();
    assert_eq!(moved, "- a\n- d\n- b\n  - c\n");
    assert_eq!(cursor, json!({ "line": 2, "character": 3 }));
    assert_eq!(run(text, "org.shift.right", 0, 0).unwrap().0, "+ a\n+ b\n  - c\n+ d\n");
    assert_eq!(run(text, "org.shift.left", 0, 0).unwrap().0, "1) a\n2) b\n   - c\n3) d\n");
    let (same, cursor) = run(text, "org.shift.down", 1, 2).unwrap();
    assert_eq!((same.as_str(), cursor), (text, json!({ "line": 3, "character": 0 })));
    assert_eq!(run(text, "org.meta.up", 0, 0).unwrap_err(), Value::Null, "no item above: an error");
    // On a timestamp in an item the date moves, applied by the server.
    client.open(&uri(), "- <2026-05-18 Mon>\n");
    let context = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 4 } });
    let result = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.shift.right", "arguments": [context] }))
        .response_result
        .unwrap();
    assert_eq!(result, Value::Null);
    client.open(&uri(), text);
    let kinds: Vec<String> = client
        .code_actions(&uri(), Position::new(1, 0), "org")
        .iter()
        .filter_map(|action| action.kind.as_ref().map(|kind| kind.as_str().to_string()))
        .filter(|kind| kind.starts_with("org.meta.") || kind.starts_with("org.shiftmeta."))
        .collect();
    assert_eq!(kinds, ["org.meta.right", "org.shiftmeta.right", "org.meta.up", "org.meta.down"]);
    // `C-c ^` sorts the list, with its own sorting types.
    client.open(&uri(), "- b\n- c\n- a\n");
    let mut context = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 1, "character": 0 } });
    let asked = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.sort", "arguments": [context.clone()] }))
        .response_result
        .unwrap();
    assert_eq!(asked["input"]["prompt"], "Sort plain list");
    assert_eq!(asked["input"]["choices"][3]["label"], "checked");
    context["input"] = json!("a");
    let sorted = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.sort", "arguments": [context] }))
        .response_result
        .unwrap();
    assert_eq!(sorted["edits"][0]["newText"], "a\n- b\n- c");
    assert_eq!(sorted["cursor"], json!({ "line": 0, "character": 0 }));
    // `C-c C-c` on an item without a checkbox renumbers the list.
    client.open(&uri(), "1. a\n3. b\n7. c\n");
    let repair = client.code_actions(&uri(), Position::new(1, 1), "org.checkbox.toggle");
    let changed: Vec<String> = edits(&repair[0]).into_iter().map(|edit| edit.new_text).collect();
    // An edit for each line it changes.
    assert_eq!(changed, ["2", "3"]);
    client.shutdown();
}

/// Cutting, copying and pasting subtrees: what is cut or copied comes
/// back in `copy`, and pasting takes `clipboard`, or else the server's
/// last cut or copy.
#[test]
fn cuts_copies_and_pastes_subtrees() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "* A\n** B\ntext\n** C\n";
    client.open(&uri(), text);
    let mut run = |command: &str, line: u32, extra: Value| -> Value {
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": 0 } });
        for (key, value) in extra.as_object().unwrap() {
            context[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .unwrap()
    };
    let copied = run("org.copy.subtree", 2, json!({}));
    assert_eq!(copied["copy"], "** B\ntext\n");
    assert_eq!(copied["message"], "Copied: Subtree(s) with 10 characters");
    assert!(copied.get("edits").is_none());
    let cut = run("org.cut.subtree", 1, json!({ "count": 2 }));
    assert_eq!(cut["copy"], "** B\ntext\n** C\n");
    assert_eq!(cut["edits"][0]["newText"], "");
    assert_eq!(cut["message"], "Cut: Subtree(s) with 15 characters");
    // The text is ASCII: a character is a byte.
    let applied = |result: &Value| {
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        let (start, end) = (offset(edit.range.start), offset(edit.range.end));
        format!("{}{}{}", &text[..start], edit.new_text, &text[end..])
    };
    let pasted = run("org.paste.subtree", 0, json!({ "clipboard": "* X\n" }));
    assert_eq!(applied(&pasted), "* X\n* A\n** B\ntext\n** C\n");
    assert_eq!(pasted["message"], "Clipboard pasted as level 1 subtree");
    // Without a clipboard: the last cut, at the level of the headline after.
    let pasted = run("org.paste.subtree", 3, json!({}));
    assert_eq!(applied(&pasted), "* A\n** B\ntext\n** B\ntext\n** C\n** C\n");
    assert_eq!(pasted["cursor"], json!({ "line": 3, "character": 0 }));
    let error = client
        .raw_request(
            "workspace/executeCommand",
            json!({ "command": "org.paste.subtree", "arguments": [{ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 }, "clipboard": "text" }] }),
        )
        .response_result
        .unwrap_err();
    assert_eq!(error.message, "The clipboard is not a (set of) tree(s)");
    client.shutdown();
}

/// `org-sort`: the sorting type asked for as a key, the property for `r`,
/// and the children sorted, the cursor on their parent.
#[test]
fn sorts_entries() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "* P\n** b\n:PROPERTIES:\n:WHO: zed\n:END:\n** a\n** C\n:PROPERTIES:\n:WHO: amy\n:END:\n";
    client.open(&uri(), text);
    let run = |client: &mut Client, extra: Value| -> Result<Value, String> {
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 2 } });
        for (key, value) in extra.as_object().unwrap() {
            context[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.sort", "arguments": [context] }))
            .response_result
            .map_err(|error| error.message)
    };
    let asked = run(&mut client, json!({})).unwrap();
    assert_eq!(asked["input"]["kind"], "key");
    assert_eq!(asked["input"]["prompt"], "Sort children");
    assert_eq!(asked["input"]["choices"][0], json!({ "key": "a", "label": "alpha" }));
    let applied = |result: &Value| {
        let edit: TextEdit = serde_json::from_value(result["edits"][0].clone()).unwrap();
        let offset = |at: Position| {
            text.split_inclusive('\n').take(at.line as usize).map(str::len).sum::<usize>() + at.character as usize
        };
        format!("{}{}{}", &text[..offset(edit.range.start)], edit.new_text, &text[offset(edit.range.end)..])
    };
    let sorted = run(&mut client, json!({ "input": "a" })).unwrap();
    assert_eq!(
        applied(&sorted),
        "* P\n** a\n** b\n:PROPERTIES:\n:WHO: zed\n:END:\n** C\n:PROPERTIES:\n:WHO: amy\n:END:\n"
    );
    assert_eq!(sorted["cursor"], json!({ "line": 0, "character": 0 }));
    assert_eq!(sorted["message"], "Sorting entries...done");
    let property = run(&mut client, json!({ "input": "R" })).unwrap();
    assert_eq!(property["input"]["kind"], "choice");
    assert_eq!(property["input"]["choices"], json!([{ "key": "WHO", "label": "WHO" }]));
    assert_eq!(property["input"]["context"], json!({ "answers": "R" }));
    let by_who = run(&mut client, json!({ "input": "WHO", "answers": "R" })).unwrap();
    assert_eq!(
        applied(&by_who),
        "* P\n** b\n:PROPERTIES:\n:WHO: zed\n:END:\n** C\n:PROPERTIES:\n:WHO: amy\n:END:\n** a\n"
    );
    client.open(&uri(), "* P\n");
    assert_eq!(run(&mut client, json!({ "input": "a" })).unwrap_err(), "Nothing to sort");
    client.shutdown();
}

/// `org-reveal` (`C-c C-r`): the lines to show around the cursor, the
/// editor's other folds kept; with `C-u` each level's children, with
/// `C-u C-u` the parent's whole tree.
#[test]
fn reveals_the_context() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "* A\ntext of A\n** B\n:PROPERTIES:\n:ID: b\n:END:\ntext of B\n*** C\n** D\n*** E\n* F\n");
    let run = |client: &mut Client, command: &str, line: u32| -> Value {
        let context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": 0 } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .unwrap()
    };
    let shown = run(&mut client, "org.reveal", 6);
    assert_eq!(shown["sparse"], json!({ "lines": [0, 2, 3, 6, 7, 8], "keep": true, "unfold": [0, 2] }));
    assert_eq!(run(&mut client, "org.reveal.siblings", 6)["sparse"]["lines"], json!([0, 1, 2, 3, 6, 7, 8]));
    assert_eq!(run(&mut client, "org.reveal.tree", 9)["sparse"]["lines"], json!([8, 9]));
}

/// `org-sparse-tree`: what to show asked for as a key, then what it
/// needs; the lines to show and the matches to highlight come back.
#[test]
fn makes_sparse_trees() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "* TODO a\nan apple\n* b :work:\n** c\nApple pie\n");
    let run = |client: &mut Client, extra: Value| -> Value {
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
        for (key, value) in extra.as_object().unwrap() {
            context[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.sparse.tree", "arguments": [context] }))
            .response_result
            .unwrap()
    };
    assert_eq!(run(&mut client, json!({}))["input"]["kind"], "key");
    let asked = run(&mut client, json!({ "input": "r" }));
    assert_eq!(asked["input"]["kind"], "text");
    assert_eq!(asked["input"]["context"], json!({ "answers": "r" }));
    let shown = run(&mut client, json!({ "input": "apple", "answers": "r" }));
    assert_eq!(shown["sparse"]["lines"], json!([1, 4]));
    assert_eq!(
        shown["sparse"]["highlights"],
        json!([
            { "start": { "line": 1, "character": 3 }, "end": { "line": 1, "character": 8 } },
            { "start": { "line": 4, "character": 0 }, "end": { "line": 4, "character": 5 } }
        ])
    );
    assert_eq!(shown["message"], "2 match(es) for regexp apple");
    let todo = run(&mut client, json!({ "input": "t" }));
    assert_eq!((todo["sparse"]["lines"].clone(), todo["message"].clone()), (json!([0]), json!("1 TODO entries found")));
    let tagged = run(&mut client, json!({ "input": "work", "answers": "m" }));
    assert_eq!(tagged["sparse"]["lines"], json!([2, 3]), "c inherits the tag");
    client.shutdown();
}

/// `org-highlight-sparse-tree-matches` off: the lines of the matches show,
/// with nothing highlighted.
#[test]
fn makes_sparse_trees_without_highlights() {
    let mut client = Client::start(json!({}), json!({ "org_highlight_sparse_tree_matches": false }));
    client.open(&uri(), "* TODO a\nan apple\n* b :work:\n** c\nApple pie\n");
    let context = json!({
        "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 },
        "input": "apple", "answers": "r",
    });
    let shown = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.sparse.tree", "arguments": [context] }))
        .response_result
        .unwrap();
    assert_eq!(shown["sparse"], json!({ "lines": [1, 4], "highlights": [] }));
    client.shutdown();
}

/// `org-remove-highlights-with-change`: the highlights of `org-occur` and
/// the times of `org-clock-display` go with the next change, and with
/// `false` stay, following the text, until `C-c C-c` takes them away. Those
/// of a sparse tree by tags always stay, as `org-scan-tags` leaves them in
/// Emacs.
#[test]
fn removes_highlights_with_change() {
    for remove in [true, false] {
        let capabilities = json!({ "textDocument": { "inlayHint": {} } });
        let options = json!({
            "org_remove_highlights_with_change": remove,
            "clock": { "today": "2026-03-02", "now": "12:00" },
        });
        let mut client = Client::start(capabilities, options);
        let text =
            "* TODO a :work:\n:LOGBOOK:\nCLOCK: [2026-03-01 Sun 10:00]--[2026-03-01 Sun 11:00] =>  1:00\n:END:\n";
        client.open(&uri(), text);
        let run = |client: &mut Client, command: &str, extra: Value| -> Option<Value> {
            let mut context =
                json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
            for (key, value) in extra.as_object().unwrap() {
                context[key] = value.clone();
            }
            client
                .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
                .response_result
                .ok()
        };
        let hints = |client: &mut Client| -> Vec<u32> {
            let range = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 10, "character": 0 } });
            let params = json!({ "textDocument": { "uri": uri().as_str() }, "range": range });
            let result = client.raw_request("textDocument/inlayHint", params).response_result.unwrap();
            result.as_array().unwrap().iter().map(|hint| hint["position"]["line"].as_u64().unwrap() as u32).collect()
        };
        let mut version = 1;
        let mut change = |client: &mut Client| {
            version += 1;
            client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri: uri(), version },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range::new(Position::new(0, 0), Position::new(0, 0))),
                    range_length: None,
                    text: "x\n".to_string(),
                }],
            });
        };
        let removed = json!("Temporary highlights/overlays removed from current buffer");
        let shown = run(&mut client, "org.sparse.tree", json!({ "input": "TODO", "answers": "r" })).unwrap();
        assert_eq!(shown["sparse"].get("keepHighlights").is_some(), !remove, "remove {remove}");
        run(&mut client, "org.clock.display", json!({})).unwrap();
        assert_eq!(hints(&mut client), [0]);
        change(&mut client);
        let cleared = run(&mut client, "org.checkbox.toggle", json!({ "position": { "line": 1, "character": 0 } }));
        if remove {
            assert_eq!(hints(&mut client), Vec::<u32>::new());
            assert_ne!(cleared.map(|result| result["message"].clone()), Some(removed.clone()));
        } else {
            let cleared = cleared.unwrap();
            assert_eq!(cleared["message"], removed);
            assert_eq!(cleared["sparse"], json!({ "keep": true, "unfold": [], "highlights": [] }));
            assert_eq!(hints(&mut client), Vec::<u32>::new());
        }
        // A sparse tree by tags: its highlights stay through changes.
        let tagged = run(&mut client, "org.sparse.tree", json!({ "input": "work", "answers": "m" })).unwrap();
        assert_eq!(tagged["sparse"]["keepHighlights"], true);
        change(&mut client);
        let cleared = run(&mut client, "org.checkbox.toggle", json!({ "position": { "line": 2, "character": 0 } }));
        assert_eq!(cleared.unwrap()["message"], removed);
        // With `false`, the times follow their headline.
        if !remove {
            run(&mut client, "org.clock.display", json!({ "position": { "line": 2, "character": 0 } })).unwrap();
            change(&mut client);
            assert_eq!(hints(&mut client), [3]);
        }
        client.shutdown();
    }
}

/// `org-num-mode`: `#+STARTUP: num` numbers the headlines as inlay hints
/// after their stars; `org.num.mode` turns it off and on, saying so as
/// Emacs does.
#[test]
fn numbers_headlines() {
    let capabilities = json!({ "textDocument": { "inlayHint": {} } });
    let mut client = Client::start(capabilities, json!({ "org_num_skip_tags": ["noexport"] }));
    client.open(&uri(), "#+STARTUP: num\n* A\n** B\n** C :noexport:\n*** D\n* E\n");
    let hints = |client: &mut Client| -> Vec<Value> {
        let range = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 6, "character": 0 } });
        let params = json!({ "textDocument": { "uri": uri().as_str() }, "range": range });
        let result = client.raw_request("textDocument/inlayHint", params).response_result.unwrap();
        result.as_array().unwrap().iter().map(|hint| json!([hint["position"], hint["label"]])).collect()
    };
    let at = |line: u32, character: u32, label: &str| json!([{ "line": line, "character": character }, label]);
    assert_eq!(hints(&mut client), [at(1, 2, "1 "), at(2, 3, "1.1 "), at(5, 2, "2 ")]);
    let toggle = |client: &mut Client| {
        let argument = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.num.mode", "arguments": [argument] }))
            .response_result
            .unwrap()["message"]
            .clone()
    };
    assert_eq!(toggle(&mut client), "Org-Num mode disabled in current buffer");
    assert_eq!(hints(&mut client), Vec::<Value>::new());
    assert_eq!(toggle(&mut client), "Org-Num mode enabled in current buffer");
    assert_eq!(hints(&mut client).len(), 3);
    client.shutdown();
}

/// Recalculating a table with formulas as a code action, in the table and
/// on its `#+TBLFM:` line; none where nothing would change.
#[test]
fn recalculates_tables_as_a_code_action() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "| 2 | 3 |   |\n#+TBLFM: $3=$1*$2\n");
    for position in [Position::new(0, 2), Position::new(1, 3)] {
        let actions = client.code_actions(&uri(), position, "org.table.recalculate");
        let kinds: Vec<&str> =
            actions.iter().filter_map(|action| action.kind.as_ref().map(|kind| kind.as_str())).collect();
        assert_eq!(kinds, ["org.table.recalculate"]);
        assert_eq!(actions[0].title, "Recalculate the table");
        let changed: Vec<String> = edits(&actions[0]).into_iter().map(|edit| edit.new_text).collect();
        assert_eq!(changed, ["6"]);
    }
    client.open(&uri(), "| 2 | 3 | 6 |\n#+TBLFM: $3=$1*$2\n");
    assert!(client.code_actions(&uri(), Position::new(0, 2), "org.table.recalculate").is_empty());
    client.shutdown();
}

/// `org-toggle-checkbox` as a code action: the box, and the cookie that
/// counts it.
#[test]
fn toggles_checkboxes() {
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), "* Groceries [0/2]\n- [ ] milk\n- [ ] eggs\n");
    let toggle = client.code_actions(&uri(), Position::new(1, 3), "org.checkbox.toggle");
    let changed: Vec<String> = edits(&toggle[0]).into_iter().map(|edit| edit.new_text).collect();
    assert_eq!(changed, ["1", "X"]);
    // Nothing to toggle on text.
    client.open(&uri(), "* a\ntext\n");
    assert!(client.code_actions(&uri(), Position::new(1, 0), "org.checkbox.toggle").is_empty());
    client.shutdown();
}

/// `org-lint` reports as diagnostics: warnings, information for checkers
/// of low trust, the checker as the code; `org_lint_ignore` leaves
/// checkers out, `org_lint` false all of them. An ID is looked for in the
/// open files and the agenda files.
#[test]
fn lints_documents() {
    let text = "* H\n[[*Missing]]\n#+CATEGORY: a\n#+CATEGORY: b\n#+begin_src\n#+end_src\n\
                * I\n:PROPERTIES:\n:ID: here\n:END:\n[[id:here]] [[id:nowhere]]\n#+begin_src nolang\n#+end_src\n";
    let diagnostics = |options: Value| {
        let client = Client::start(json!({}), options);
        client.open(&uri(), text);
        let published = client.notification("textDocument/publishDiagnostics");
        client.shutdown();
        published["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic["range"]["start"]["line"].as_u64().unwrap(),
                    diagnostic["severity"].as_u64().unwrap(),
                    diagnostic["code"].as_str().unwrap().to_string(),
                    diagnostic["message"].as_str().unwrap().to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    let found = diagnostics(json!({}));
    assert_eq!(
        found,
        vec![
            (1, 3, "invalid-fuzzy-link".to_string(), "Unknown fuzzy location \"Missing\"".to_string()),
            (
                3,
                3,
                "deprecated-category-setup".to_string(),
                "Spurious CATEGORY keyword.  Set :CATEGORY: property instead".to_string()
            ),
            (4, 3, "missing-language-in-src-block".to_string(), "Missing language in source block".to_string()),
            (10, 3, "invalid-id-link".to_string(), "Unknown ID \"nowhere\"".to_string()),
            (
                11,
                4,
                "suspicious-language-in-src-block".to_string(),
                "Unknown source block language: 'nolang'".to_string()
            ),
        ]
    );
    let found = diagnostics(json!({ "org_lint_ignore": ["invalid-fuzzy-link", "deprecated-category-setup"] }));
    assert_eq!(
        found.iter().map(|found| found.2.as_str()).collect::<Vec<_>>(),
        ["missing-language-in-src-block", "invalid-id-link", "suspicious-language-in-src-block"]
    );
    let mut client = Client::start(json!({}), json!({ "org_lint": false }));
    client.open(&uri(), text);
    // Nothing: the next message is the answer to a request.
    let symbols =
        client.raw_request("textDocument/documentSymbol", json!({ "textDocument": { "uri": uri().as_str() } }));
    assert!(symbols.response_result.is_ok());
    client.shutdown();
}

/// Citations (`oc-basic`): the bibliography of `#+BIBLIOGRAPHY:`, an
/// unknown key marked with the keys close to it and one to put instead,
/// a key's entry over it, where following it goes, and keys completed
/// after `@`.
#[test]
fn citations_follow_the_bibliography() {
    let directory = std::env::temp_dir().join(format!("org-lsp-cite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("refs.bib"),
        "@Book{knuth1984,\n  author = {Knuth, Donald E.},\n  title = {The {\\TeX}book},\n  publisher = {Addison-Wesley},\n  year = 1984\n}\n\n@article{doe2020,\n  author = {John Doe and Jane Roe},\n  title = {A Study},\n  journal = {Things},\n  date = {2020-05-01}\n}\n",
    )
    .unwrap();
    let file = directory.join("notes.org");
    let text = "#+bibliography: refs.bib\n\nAs [cite/t:see @knuth1984; @doe202 p. 3] says.\n";
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri, text);
    // The unknown key, with the one close to it.
    let published = client.notification("textDocument/publishDiagnostics");
    // The citation's, not the linter's (`org-lint` misses a
    // `#+PRINT_BIBLIOGRAPHY:`).
    let diagnostics: Vec<Value> = published["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["source"] == "org-lsp")
        .cloned()
        .collect();
    assert_eq!(diagnostics.len(), 1, "{published}");
    assert_eq!(diagnostics[0]["message"], "Unknown citation key: @doe202 (suggestions: doe2020)");
    assert_eq!(diagnostics[0]["range"]["start"], json!({ "line": 2, "character": 27 }));
    assert_eq!(diagnostics[0]["range"]["end"], json!({ "line": 2, "character": 34 }));
    let actions = client
        .raw_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri.as_str() },
                "range": diagnostics[0]["range"],
                "context": { "diagnostics": diagnostics },
            }),
        )
        .response_result
        .unwrap();
    let fix =
        actions.as_array().unwrap().iter().find(|action| action["title"] == "Replace with @doe2020").expect("a fix");
    assert_eq!(fix["kind"], "quickfix");
    // The entry over its key.
    let hover = |client: &mut Client, character: u32| {
        client
            .raw_request(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": character } }),
            )
            .response_result
            .unwrap()
    };
    assert_eq!(hover(&mut client, 18)["contents"]["value"], "Knuth. The {\\TeX}book, Addison-Wesley, 1984.");
    assert_eq!(hover(&mut client, 30)["contents"]["value"], "Suggestions: doe2020");
    assert_eq!(hover(&mut client, 2), Value::Null);
    // Following the key: its entry.
    let definition = client
        .raw_request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 20 } }),
        )
        .response_result
        .unwrap();
    assert!(definition["uri"].as_str().unwrap().ends_with("refs.bib"), "{definition}");
    assert_eq!(definition["range"]["start"], json!({ "line": 0, "character": 0 }));
    // `C-c C-o` on the style asks which of the keys.
    let asked = client
        .raw_request(
            "workspace/executeCommand",
            json!({ "command": "org.open", "arguments": [{
                "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 7 },
            }] }),
        )
        .response_result
        .unwrap();
    assert_eq!(asked["input"]["prompt"], "Select citation key");
    assert_eq!(
        asked["input"]["choices"],
        json!([{ "key": "knuth1984", "label": "knuth1984" }, { "key": "doe202", "label": "doe202" }])
    );
    // Keys after `@`, with their authors, year and title.
    client.notify::<lsp_types::notification::DidChangeTextDocument>(lsp_types::DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri.clone(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "#+bibliography: refs.bib\n\nSee [cite:@do".to_string(),
        }],
    });
    let completion = client
        .raw_request(
            "textDocument/completion",
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 15 } }),
        )
        .response_result
        .unwrap();
    let items = completion.as_array().unwrap();
    let doe = items.iter().find(|item| item["label"] == "doe2020").expect("doe2020 offered");
    assert_eq!(doe["detail"], "John Doe; Jane Roe         2020  A Study");
    assert_eq!(doe["textEdit"]["range"]["start"], json!({ "line": 2, "character": 11 }));
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// A bibliography named twice, by `#+BIBLIOGRAPHY:` and by
/// `org_cite_global_bibliography`: Emacs reads it twice and says its keys
/// twice, but completes each key once (its table is a hash).
#[test]
fn citation_keys_once_when_the_bibliography_is_named_twice() {
    let directory = std::env::temp_dir().join(format!("org-lsp-cite-twice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let bib = directory.join("refs.bib");
    std::fs::write(
        &bib,
        "@Book{knuth1984,\n  author = {Knuth, Donald E.},\n  title = {The {\\TeX}book},\n  year = 1984\n}\n",
    )
    .unwrap();
    let file = directory.join("notes.org");
    let text = "#+bibliography: refs.bib\n\nAs [cite:@knuht1984] says, [cite:@\n";
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let options = json!({ "org_cite_global_bibliography": [bib.display().to_string()] });
    let mut client = Client::start(json!({}), options);
    client.open(&uri, text);
    let published = client.notification("textDocument/publishDiagnostics");
    let diagnostics: Vec<Value> = published["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["source"] == "org-lsp")
        .cloned()
        .collect();
    assert_eq!(diagnostics[0]["message"], "Unknown citation key: @knuht1984 (suggestions: knuth1984 knuth1984)");
    let actions = client
        .raw_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri.as_str() },
                "range": diagnostics[0]["range"],
                "context": { "diagnostics": diagnostics },
            }),
        )
        .response_result
        .unwrap();
    let fixes =
        actions.as_array().unwrap().iter().filter(|action| action["title"] == "Replace with @knuth1984").count();
    assert_eq!(fixes, 1, "{actions}");
    let completion = client
        .raw_request(
            "textDocument/completion",
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 34 } }),
        )
        .response_result
        .unwrap();
    let labels: Vec<&str> = completion.as_array().unwrap().iter().map(|item| item["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["knuth1984"]);
    let asked = client
        .raw_request(
            "workspace/executeCommand",
            json!({ "command": "org.cite.insert", "arguments": [{
                "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 0 },
            }] }),
        )
        .response_result
        .unwrap();
    assert_eq!(asked["input"]["choices"].as_array().unwrap().len(), 1, "{asked}");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// The next message from the server that `wanted` picks, within a minute.
fn next_from_server(client: &Client, wanted: impl Fn(&Message) -> bool) -> Message {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let message = client.connection.receiver.recv_timeout(left).expect("the server says something");
        if wanted(&message) {
            return message;
        }
    }
}

/// What the server follows itself, Emacs exports too: an `id:` link to
/// an entry of another file, whose place it is told (`org-id-locations`,
/// which org-roam keeps in Emacs), and with org-roam a `roam:` link, a link
/// of its own type rather than a fuzzy link that cannot be resolved.
#[test]
fn exports_links_the_server_follows() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("exports_links_the_server_follows: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-export-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("node.org"), "* Node\n:PROPERTIES:\n:ID: node-1\n:END:\n").unwrap();
    // One open too, saved: Emacs reads it to link to it.
    let opened = directory.join("opened.org");
    let opened_text = "* Opened\n:PROPERTIES:\n:ID: opened-1\n:END:\n";
    std::fs::write(&opened, opened_text).unwrap();
    let file = directory.join("notes.org");
    let text =
        "* Notes\nSee [[id:node-1][the node]] and [[roam:Beta]], [[id:opened-1][opened]], [[id:node-1][again]].\n\
                * Own\n:PROPERTIES:\n:ID: own-1\n:END:\nAnd [[id:own-1][this one]].\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let options = json!({
        "emacs_config": config,
        "org_roam_enabled": true,
        "org_roam_directory": directory.display().to_string(),
    });
    let mut client = Client::start(json!({}), options);
    client.open(&format!("file://{}", opened.display()).parse().unwrap(), opened_text);
    client.open(&uri, text);
    let argument = json!({
        "textDocument": { "uri": uri.as_str() },
        "position": { "line": 0, "character": 0 },
        "input": "h", "answers": "", "step": "h",
    });
    client
        .raw_request("workspace/executeCommand", json!({ "command": "org.export.dispatch", "arguments": [argument] }))
        .response_result
        .expect("a result");
    let Message::Notification(done) = next_from_server(
        &client,
        |message| matches!(message, Message::Notification(notification) if notification.method == "window/showMessage"),
    ) else {
        unreachable!()
    };
    let html_file = directory.join("notes.html");
    assert_eq!(done.params["message"], format!("Exported to {}", html_file.display()));
    let written = std::fs::read_to_string(&html_file).unwrap();
    assert!(written.contains(r#"<a href="node.html#ID-node-1">the node</a>"#), "{written}");
    assert!(written.contains(r#"<a href="roam:Beta">roam:Beta</a>"#), "{written}");
    assert!(written.contains(r#"<a href="opened.html#ID-opened-1">opened</a>"#), "{written}");
    // Its own entry, by the anchor of its headline.
    assert!(written.contains(r##"And <a href="#org"##), "{written}");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn exports_through_emacs() {
    // Emacs is optional; CI's Rust job has none unless it says otherwise.
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("exports_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    // The editor's text, unsaved: what Emacs exports.
    let text = "#+TITLE: Notes\n* Héllo\nFirst.\n* Code\n#+begin_src emacs-lisp :exports results\n(+ 1 2)\n#+end_src\n";
    std::fs::write(&file, "* On disk\n").unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    // Another Org for Emacs, as `emacs_config` can load it.
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let context = |position: (u32, u32), extra: Value| {
        let mut context = json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": position.0, "character": position.1 },
        });
        for (key, value) in extra.as_object().unwrap() {
            context[key] = value.clone();
        }
        context
    };
    let run = |client: &mut Client, argument: Value| {
        client
            .raw_request(
                "workspace/executeCommand",
                json!({ "command": "org.export.dispatch", "arguments": [argument] }),
            )
            .response_result
            .expect("a result")
    };
    let keys = |result: &Value| -> Vec<String> {
        result["input"]["choices"].as_array().unwrap().iter().map(|choice| format!("{}", choice["key"])).collect()
    };

    // The menu is Emacs's: its backends, the options, publishing.
    let menu = run(&mut client, context((0, 0), json!({})));
    assert_eq!(menu["input"]["kind"], "key", "{menu}");
    assert_eq!(menu["input"]["prompt"], "Export command");
    let first = keys(&menu);
    for key in [r#""\u0002""#, r#""\u0013""#, r#""h""#, r#""l""#, r#""t""#, r#""P""#] {
        assert!(first.contains(&key.to_string()), "{key} in {first:?}");
    }
    assert_eq!(menu["input"]["context"], json!({ "answers": "", "step": "" }));

    // An option turned on is shown on.
    let scoped = run(&mut client, context((2, 0), json!({ "input": "\u{13}", "answers": "", "step": "" })));
    assert_eq!(scoped["input"]["context"]["answers"], "s");
    assert!(scoped["input"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|choice| choice["label"] == "Export scope: Subtree"));

    // A backend's entries.
    let html = run(&mut client, context((0, 0), json!({ "input": "h", "answers": "", "step": "" })));
    assert_eq!(html["input"]["prompt"], "Export to HTML");
    assert!(keys(&html).contains(&r#""o""#.to_string()), "{html}");

    // To a file: Emacs says where it went once it is done.
    let started = run(&mut client, context((0, 0), json!({ "input": "h", "answers": "", "step": "h" })));
    assert_eq!(started["message"], "As HTML file…");
    let Message::Request(question) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "window/showMessageRequest"),
    ) else {
        unreachable!()
    };
    // Babel asks before it evaluates, in the editor.
    assert!(
        question.params["message"].as_str().unwrap().contains("Evaluate this emacs-lisp code block"),
        "{:?}",
        question.params
    );
    client.connection.sender.send(Message::Response(Response::new_ok(question.id, json!({ "title": "Yes" })))).unwrap();
    let Message::Notification(done) = next_from_server(
        &client,
        |message| matches!(message, Message::Notification(notification) if notification.method == "window/showMessage"),
    ) else {
        unreachable!()
    };
    let html_file = directory.join("notes.html");
    assert_eq!(done.params["message"], format!("Exported to {}", html_file.display()));
    let written = std::fs::read_to_string(&html_file).unwrap();
    assert!(
        written.contains("Héllo") && written.contains("<title>Notes</title>") && written.contains("\n3\n</pre>"),
        "{written}"
    );

    // To a buffer, of the subtree at the cursor, the question answered no:
    // a file the editor is asked to show.
    run(&mut client, context((2, 0), json!({ "input": "U", "answers": "s", "step": "t" })));
    let Message::Request(show) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "window/showDocument"),
    ) else {
        unreachable!()
    };
    let shown: Uri = serde_json::from_value(show.params["uri"].clone()).unwrap();
    assert!(
        shown.as_str().ends_with("Org%20ASCII%20Export.txt") || shown.as_str().ends_with("Org ASCII Export.txt"),
        "{shown:?}"
    );
    let path = std::path::PathBuf::from(percent_decode(shown.path().as_str()));
    let exported = std::fs::read_to_string(&path).unwrap();
    assert!(exported.contains("HÉLLO") && exported.contains("First.") && !exported.contains("Code"), "{exported}");
    client.connection.sender.send(Message::Response(Response::new_ok(show.id, json!({ "success": true })))).unwrap();

    // `&`: the export stack, the last first; the one picked shows, a file
    // as it would open, an export to a buffer in the editor.
    let stack = run(&mut client, context((0, 0), json!({ "input": "&", "answers": "", "step": "" })));
    let labels: Vec<&str> =
        stack["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["label"].as_str().unwrap()).collect();
    assert_eq!(labels.len(), 2, "{stack}");
    assert!(labels[0].starts_with("As UTF-8 buffer: ") && labels[1].starts_with("As HTML file: "), "{labels:?}");
    client.asked.clear();
    run(&mut client, context((0, 0), json!({ "input": "0", "answers": "", "step": "&" })));
    let show = client.asked.iter().find(|request| request.method == "window/showDocument").expect("shown");
    assert_eq!(show.params["uri"], json!(format!("file://{}", html_file.display())), "{:?}", show.params);
    assert_eq!(show.params["external"], json!(true));

    // `C-v`, visible only: what the editor has folded is left out.
    let visible = run(&mut client, context((0, 0), json!({ "input": "\u{16}", "answers": "", "step": "" })));
    assert_eq!(visible["input"]["context"]["answers"], "v");
    run(&mut client, context((0, 0), json!({ "input": "U", "answers": "v", "step": "t", "folded": [[1, 2]] })));
    let Message::Request(question) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "window/showMessageRequest"),
    ) else {
        unreachable!()
    };
    client.connection.sender.send(Message::Response(Response::new_ok(question.id, json!({ "title": "No" })))).unwrap();
    let Message::Request(show) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "window/showDocument"),
    ) else {
        unreachable!()
    };
    let shown: Uri = serde_json::from_value(show.params["uri"].clone()).unwrap();
    let exported = std::fs::read_to_string(percent_decode(shown.path().as_str())).unwrap();
    assert!(exported.contains("Héllo") && !exported.contains("First.") && exported.contains("Code"), "{exported}");
    client.connection.sender.send(Message::Response(Response::new_ok(show.id, json!({ "success": true })))).unwrap();

    // An entry that is not there.
    let response = client.raw_request(
        "workspace/executeCommand",
        json!({ "command": "org.export.dispatch", "arguments": [context((0, 0), json!({ "input": "z", "answers": "", "step": "" }))] }),
    );
    assert!(response.response_result.unwrap_err().message.contains("No export command under z"));

    client.shutdown();
    assert!(!path.exists(), "the exports to a buffer go with the server");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn export_says_when_emacs_cannot_start() {
    let mut client = Client::start(json!({}), json!({ "emacs": "/nonexistent/emacs" }));
    client.open(&uri(), "* A\n");
    let response = client.raw_request(
        "workspace/executeCommand",
        json!({ "command": "org.export.dispatch", "arguments": [{ "textDocument": { "uri": uri().as_str() }, "position": { "line": 0, "character": 0 } }] }),
    );
    let error = response.response_result.expect_err("an error").message;
    assert!(error.contains("Cannot start Emacs (`/nonexistent/emacs`, the `emacs` option)"), "{error}");
    client.shutdown();
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn evaluates_code_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("evaluates_code_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-babel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("code.org");
    let text = "* Code\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\nInline src_emacs-lisp{(* 6 7)} here.\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let run = |client: &mut Client, command: &str, line: u32, character: u32| {
        let argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": line, "character": character } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    // Waits for the edit Emacs's text makes, answering yes to its question.
    let edited = |client: &Client| -> Vec<TextEdit> {
        loop {
            let Message::Request(request) = next_from_server(
                client,
                |message| matches!(message, Message::Request(request) if request.method == "window/showMessageRequest" || request.method == "workspace/applyEdit"),
            ) else {
                unreachable!()
            };
            if request.method == "window/showMessageRequest" {
                assert!(request.params["message"].as_str().unwrap().contains("Evaluate this emacs-lisp code block"));
                client
                    .connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, json!({ "title": "Yes" }))))
                    .unwrap();
                continue;
            }
            client
                .connection
                .sender
                .send(Message::Response(Response::new_ok(request.id, json!({ "applied": true }))))
                .unwrap();
            let changes = &request.params["edit"]["changes"][uri.as_str()];
            return serde_json::from_value(changes.clone()).unwrap();
        }
    };
    // C-c C-c in the block: the results under it.
    assert_eq!(run(&mut client, "org.checkbox.toggle", 2, 0), Value::Null);
    let edits = edited(&client);
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(edits[0].new_text, "#+RESULTS:\n: 3\n\n");
    let with_results = "* Code\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\n#+RESULTS:\n: 3\n\nInline src_emacs-lisp{(* 6 7)} here.\n";
    client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
        text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri.clone(), version: 2 },
        content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: with_results.to_string(),
        }],
    });
    // C-c C-c on inline code: its result after it.
    run(&mut client, "org.checkbox.toggle", 8, 12);
    let edits = edited(&client);
    assert_eq!(edits[0].new_text, "{{{results(=42=)}}} ", "{edits:?}");
    // C-c C-v k: the block's results go.
    run(&mut client, "org.babel.remove_result", 2, 0);
    let edits = edited(&client);
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(edits[0].new_text, "");
    assert_eq!(edits[0].range, Range::new(Position::new(5, 0), Position::new(8, 0)));
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// An Emacs that stops, started again: what it says each time it starts
/// (an option of `emacs_options` left out) is shown once, and how it
/// stopped each time.
#[cfg(unix)]
#[test]
fn says_once_what_each_emacs_says() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!("org-lsp-emacs-again-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    // Set up with a notice, then stop at the first command.
    let emacs = directory.join("emacs");
    let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"setup"'*)
      printf '\036org-lsp {"notice":"`emacs_options` leaves out `org-export-with-toc`: \\"bad\\" is not false, true or an integer"}\n'
      printf '\036org-lsp {"id":%s,"result":{}}\n' "$id" ;;
    *) exit 3 ;;
  esac
done
"#;
    std::fs::write(&emacs, script).unwrap();
    std::fs::set_permissions(&emacs, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = directory.join("notes.org");
    std::fs::write(&file, "| a |\n").unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let mut client = Client::start(json!({}), json!({ "emacs": emacs.display().to_string() }));
    client.open(&uri, "| a |\n");
    let mut said = Vec::new();
    for _ in 0..2 {
        let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 2 } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.plot.gnuplot", "arguments": [argument] }))
            .response_result
            .expect("a result");
        loop {
            let Message::Notification(note) = next_from_server(
                &client,
                |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
            ) else {
                unreachable!()
            };
            let text = note.params["message"].as_str().unwrap().to_string();
            let stopped = text.starts_with("Emacs stopped");
            said.push(text);
            if stopped {
                break;
            }
        }
    }
    assert_eq!(
        said,
        [
            "org-lsp: `emacs_options` leaves out `org-export-with-toc`: \"bad\" is not false, true or an integer",
            "Emacs stopped (exit status 3)",
            "Emacs stopped (exit status 3)",
        ]
    );
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// The attachment dispatcher's `q`, a key it does not have, and `u`: a
/// download not considered safe is the editor's question; with
/// `org_resource_download_policy` true, none.
#[test]
fn attaches_from_a_url_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("attaches_from_a_url_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-attach-url-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.txt");
    std::fs::write(&source, "fetched\n").unwrap();
    let url = format!("file://{}", source.display());
    let file = directory.join("notes.org");
    let text = "* Task\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    for policy in [None, Some(true)] {
        let mut settings = json!({ "emacs_config": config });
        if let Some(policy) = policy {
            settings["org_resource_download_policy"] = json!(policy);
        }
        let mut client = Client::start(json!({}), settings);
        client.open(&uri, text);
        let attach = |client: &mut Client, extra: Value| {
            let mut argument =
                json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
            for (key, value) in extra.as_object().unwrap() {
                argument[key] = value.clone();
            }
            client.raw_request("workspace/executeCommand", json!({ "command": "org.attach", "arguments": [argument] }))
        };
        assert_eq!(
            attach(&mut client, json!({ "input": "q" })).response_result.unwrap(),
            json!({ "message": "Abort" })
        );
        let error = attach(&mut client, json!({ "input": "x" })).response_result.expect_err("an error");
        assert_eq!(error.message, "No such attachment command: x");
        attach(&mut client, json!({ "input": url, "step": "u" })).response_result.expect("a result");
        if policy.is_none() {
            let Message::Request(question) = next_from_server(
                &client,
                |message| matches!(message, Message::Request(request) if request.method == "window/showMessageRequest"),
            ) else {
                unreachable!()
            };
            let asked = question.params["message"].as_str().unwrap();
            assert!(asked.starts_with(&format!("An org-mode document would like to download {url}")), "{asked}");
            client
                .connection
                .sender
                .send(Message::Response(Response::new_ok(question.id, json!({ "title": "Yes" }))))
                .unwrap();
        }
        let Message::Request(edit) = next_from_server(&client, |message| {
            matches!(message, Message::Request(request) if request.method == "workspace/applyEdit")
                || matches!(message, Message::Notification(note) if note.method == "window/showMessage")
        }) else {
            panic!("Emacs did not attach the file")
        };
        let edits: Vec<TextEdit> =
            serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
        let attached = apply_text_edits(text, &edits);
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true }))))
            .unwrap();
        let id = attached.lines().find_map(|line| line.trim().strip_prefix(":ID:")).unwrap().trim().to_string();
        let fetched = directory.join("data").join(&id[..2]).join(&id[2..]).join("source.txt");
        assert_eq!(std::fs::read_to_string(&fetched).unwrap(), "fetched\n");
        // What Emacs says after the edit.
        let Message::Notification(said) = next_from_server(
            &client,
            |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
        ) else {
            unreachable!()
        };
        assert_eq!(said.params["message"], "File \"source.txt\" is now an attachment");
        client.shutdown();
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// A code block that fails: its results go in, then what Emacs says after
/// the edit, and what it shows in `*Org-Babel Error Output*`.
#[test]
fn shows_what_babel_shows_of_a_block_that_fails() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("shows_what_babel_shows_of_a_block_that_fails: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-babel-error-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("code.org");
    let text = "#+begin_src sh :results output\necho out; echo err >&2; exit 3\n#+end_src\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(
        json!({}),
        json!({
            "emacs_config": config,
            "emacs_options": { "org-babel-load-languages": { "shell": true }, "org-confirm-babel-evaluate": false },
        }),
    );
    client.open(&uri, text);
    let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 1, "character": 0 } });
    client
        .raw_request("workspace/executeCommand", json!({ "command": "org.checkbox.toggle", "arguments": [argument] }))
        .response_result
        .expect("a result");
    let Message::Request(edit) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
    ) else {
        unreachable!()
    };
    let edits: Vec<TextEdit> = serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
    assert_eq!(edits[0].new_text, "\n\n#+RESULTS:\n: out", "{edits:?}");
    client.connection.sender.send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true })))).unwrap();
    let mut said = Vec::new();
    while said.len() < 2 {
        let Message::Notification(note) = next_from_server(
            &client,
            |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
        ) else {
            unreachable!()
        };
        said.push((note.params["type"].as_i64().unwrap(), note.params["message"].as_str().unwrap().to_string()));
    }
    assert_eq!(
        said,
        [
            (3, "Babel evaluation exited with code 3".to_string()),
            (2, "err\n[ Babel evaluation exited with code 3 ]".to_string()),
        ]
    );
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// Emacs's yes-or-no question for a client that shows no
/// `window/showMessageRequest` (`message_requests` off): the Vim plugin
/// (org-lsp's editor code, `run_command`) is told to run
/// `org.emacs.answer`, which asks it as a command's question, one key, and
/// its messages come as `org-lsp/message` (`message_notifications`); a
/// client without org-lsp's editor code (Helix) finds Yes and No first in
/// the code actions.
#[test]
fn asks_emacs_questions_without_message_requests() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("asks_emacs_questions_without_message_requests: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-babel-ask-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("code.org");
    let text = "* Code\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 0 } });
    let execute = |client: &mut Client, command: &str, argument: Value| {
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    let results = |client: &Client| -> String {
        let Message::Request(request) = next_from_server(
            client,
            |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
        ) else {
            unreachable!()
        };
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(request.id, json!({ "applied": true }))))
            .unwrap();
        request.params["edit"].to_string()
    };

    // The Vim plugin.
    let options = json!({
        "emacs_config": config,
        "run_command": true,
        "message_requests": false,
        "message_notifications": true,
    });
    let mut client = Client::start(json!({}), options);
    client.open(&uri, text);
    assert_eq!(execute(&mut client, "org.babel.execute", argument.clone()), Value::Null);
    let Message::Notification(run) = next_from_server(
        &client,
        |message| matches!(message, Message::Notification(notification) if notification.method == "org-lsp/runCommand"),
    ) else {
        unreachable!()
    };
    assert_eq!(run.params["command"], "org.emacs.answer");
    let asked = execute(&mut client, "org.emacs.answer", run.params["argument"].clone());
    assert_eq!(asked["input"]["kind"], "key");
    assert_eq!(asked["input"]["prompt"], "Evaluate this emacs-lisp code block on your system? (y or n) ");
    assert_eq!(execute(&mut client, "org.emacs.answer", json!({ "input": "y" })), Value::Null);
    assert!(results(&client).contains(": 3"));
    // Its messages, as the plugin shows them.
    let response = client.raw_request(
        "workspace/executeCommand",
        json!({ "command": "org.emacs.answer", "arguments": [{ "input": "y" }] }),
    );
    assert_eq!(response.response_result.unwrap_err().message, "Emacs is not asking anything");
    client.shutdown();

    // Helix.
    let options = json!({ "emacs_config": config, "message_requests": false, "show_document": true });
    let mut client = Client::start(json!({ "experimental": { "orgClient": false } }), options);
    client.open(&uri, text);
    execute(&mut client, "org.babel.execute", argument.clone());
    let Message::Notification(said) = next_from_server(&client, |message| {
        matches!(message, Message::Notification(notification) if notification.method == "window/showMessage"
            && notification.params["message"].as_str().is_some_and(|text| text.contains("pick the answer")))
    }) else {
        unreachable!()
    };
    assert_eq!(
        said.params["message"],
        "Evaluate this emacs-lisp code block on your system? (y or n): pick the answer from the code actions"
    );
    let actions = client
        .raw_request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri.as_str() },
                "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 0 } },
                "context": { "diagnostics": [] }
            }),
        )
        .response_result
        .unwrap();
    let yes = &actions[0];
    assert_eq!((yes["title"].as_str(), yes["kind"].as_str()), (Some("Yes [y]"), Some("org.answer")), "{actions}");
    let command = yes["command"].clone();
    execute(&mut client, command["command"].as_str().unwrap(), command["arguments"][0].clone());
    assert!(results(&client).contains(": 3"));
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn moves_about_as_org_does() {
    let mut client = Client::start(json!({}), Value::Null);
    let text = "Intro [[x]]\n* A\n** B\ntext\n** C\n* D :tag:\n";
    client.open(&uri(), text);
    let mut run = |command: &str, line: u32, character: u32, extra: Value| {
        let mut argument =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        for (key, value) in extra.as_object().unwrap() {
            argument[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
    };
    let cursor = |result: &Result<Value, lsp_server::ResponseError>| {
        let value = result.as_ref().expect("a result");
        (value["cursor"]["line"].as_u64().unwrap(), value["cursor"]["character"].as_u64().unwrap())
    };
    assert_eq!(cursor(&run("org.heading.next", 0, 0, json!({}))), (1, 0));
    assert_eq!(cursor(&run("org.heading.next", 0, 0, json!({ "count": 3 }))), (4, 0));
    assert_eq!(cursor(&run("org.heading.previous", 3, 2, json!({}))), (2, 0));
    assert_eq!(cursor(&run("org.heading.forward", 2, 0, json!({}))), (4, 0));
    assert_eq!(cursor(&run("org.heading.backward", 4, 0, json!({}))), (2, 0));
    assert_eq!(cursor(&run("org.heading.up", 3, 1, json!({}))), (1, 0));
    let top = run("org.heading.up", 1, 0, json!({}));
    assert_eq!(top.unwrap_err().message, "Already at top level of the outline");
    // The link search: nothing after the last link, then from the start.
    let none = run("org.link.next", 1, 0, json!({}));
    assert_eq!(none.as_ref().unwrap()["message"], "No further link found");
    let wrapped = run("org.link.next", 1, 0, json!({}));
    assert_eq!(cursor(&wrapped), (0, 6));
    assert_eq!(wrapped.as_ref().unwrap()["message"], "Link search wrapped back to beginning of buffer");
    // `org-goto`: the headings by their outline path, then the one picked.
    let asked = run("org.goto", 0, 0, json!({}));
    let labels: Vec<String> = asked.as_ref().unwrap()["input"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|choice| choice["label"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(labels, ["A/", "A/B/", "A/C/", "D/"]);
    assert_eq!(cursor(&run("org.goto", 0, 0, json!({ "input": "A/C/" }))), (4, 0));
    client.shutdown();
}

#[test]
fn line_keys_as_org_special_ctrl_a_e_and_k_make_them() {
    let run = |client: &mut Client, command: &str, line: u32, character: u32| {
        let argument =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
    };
    let cursor = |result: &Result<Value, lsp_server::ResponseError>| {
        let value = result.as_ref().expect("a result");
        (value["cursor"]["line"].as_u64().unwrap(), value["cursor"]["character"].as_u64().unwrap())
    };
    let text = "* TODO Title :tag:\n- [ ] item\n";
    // Off: the keys keep the editor's meaning.
    let mut client = Client::start(json!({}), Value::Null);
    client.open(&uri(), text);
    for command in ["org.beginning.of.line", "org.end.of.line", "org.kill.line"] {
        let refused = run(&mut client, command, 0, 9).unwrap_err();
        assert_eq!(refused.data, Some(json!({ "notApplicable": true })), "{command}");
    }
    client.shutdown();

    let mut client =
        Client::start(json!({}), json!({ "org_special_ctrl_a_e": "reversed", "org_special_ctrl_k": true }));
    client.open(&uri(), text);
    // `reversed`: the line's start first, the title's on the key right after.
    assert_eq!(cursor(&run(&mut client, "org.beginning.of.line", 0, 9)), (0, 0));
    assert_eq!(cursor(&run(&mut client, "org.beginning.of.line", 0, 0)), (0, 7));
    assert_eq!(cursor(&run(&mut client, "org.end.of.line", 1, 0)), (1, 10));
    assert_eq!(cursor(&run(&mut client, "org.beginning.of.line", 1, 8)), (1, 0));
    assert_eq!(cursor(&run(&mut client, "org.beginning.of.line", 1, 0)), (1, 6));
    // C-k in the title: to the tags, the kill by characters.
    let killed = run(&mut client, "org.kill.line", 0, 9).unwrap();
    assert_eq!(killed["copy"], "tle");
    assert_eq!(killed["copyCharacters"], true);
    client.shutdown();

    let mut client = Client::start(json!({}), json!({ "org_special_ctrl_a_e": [true, false] }));
    client.open(&uri(), text);
    assert_eq!(cursor(&run(&mut client, "org.beginning.of.line", 0, 9)), (0, 7));
    assert!(run(&mut client, "org.end.of.line", 0, 9).is_err());
    client.shutdown();
}

#[test]
fn tab_before_folding_as_org_cycle_does() {
    let run = |client: &mut Client, line: u32, character: u32| {
        let argument =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.cycle", "arguments": [argument] }))
            .response_result
    };
    let text = "Intro\n* A\n** \n- a\n- \n";
    let mut client = Client::start(json!({}), json!({ "org_cycle_global_at_bob": true }));
    client.open(&uri(), text);
    // A new, empty headline: a sibling of its parent, the stars after it.
    let promoted = run(&mut client, 2, 3).unwrap();
    let edits: Vec<TextEdit> = serde_json::from_value(promoted["edits"].clone()).unwrap();
    assert_eq!(apply_text_edits(text, &edits), "Intro\n* A\n* \n- a\n- \n");
    assert_eq!(promoted["cursor"], json!({ "line": 2, "character": 2 }));
    // A new, empty item: in under the one before.
    let indented = run(&mut client, 4, 2).unwrap();
    let edits: Vec<TextEdit> = serde_json::from_value(indented["edits"].clone()).unwrap();
    assert_eq!(apply_text_edits(text, &edits), "Intro\n* A\n** \n- a\n  - \n");
    // At the start of the file, off a headline: global cycling.
    assert_eq!(run(&mut client, 0, 0).unwrap(), json!({ "global": true }));
    // Elsewhere the editor folds.
    assert_eq!(run(&mut client, 1, 0).unwrap(), json!({ "integrateLists": false }));
    client.shutdown();

    let mut client = Client::start(
        json!({}),
        json!({ "org_cycle_level_after_item_entry_creation": false, "org_cycle_include_plain_lists": "integrate" }),
    );
    client.open(&uri(), text);
    assert_eq!(run(&mut client, 2, 3).unwrap(), json!({ "integrateLists": true }));
    assert_eq!(run(&mut client, 0, 0).unwrap(), json!({ "integrateLists": true }));
    client.shutdown();
}

#[test]
fn folds_as_org_cycle_max_level_and_plain_lists_say() {
    let text = "* A\n** B\ntext\n*** C\nmore\n- item\n  more\n";
    let folds = |options: Value| {
        let mut client = Client::start(json!({}), options);
        client.open(&uri(), text);
        let result = client
            .raw_request("textDocument/foldingRange", json!({ "textDocument": { "uri": uri().as_str() } }))
            .response_result
            .unwrap();
        client.shutdown();
        let mut ranges: Vec<(u64, u64)> = result
            .as_array()
            .unwrap()
            .iter()
            .map(|range| (range["startLine"].as_u64().unwrap(), range["endLine"].as_u64().unwrap()))
            .collect();
        ranges.sort();
        ranges
    };
    assert_eq!(folds(Value::Null), [(0, 6), (1, 6), (3, 6), (5, 6)]);
    assert_eq!(folds(json!({ "org_cycle_max_level": 2 })), [(0, 6), (1, 6), (5, 6)]);
    assert_eq!(folds(json!({ "org_cycle_include_plain_lists": false })), [(0, 6), (1, 6), (3, 6)]);
}

/// `text` with `edits` applied (positions in characters; ASCII text).
fn apply_text_edits(text: &str, edits: &[TextEdit]) -> String {
    let offset = |position: Position| -> usize {
        let start: usize = text.split_inclusive('\n').take(position.line as usize).map(str::len).sum();
        start + position.character as usize
    };
    let mut sorted = edits.to_vec();
    sorted.sort_by_key(|edit| std::cmp::Reverse(offset(edit.range.start)));
    let mut out = text.to_string();
    for edit in sorted {
        out.replace_range(offset(edit.range.start)..offset(edit.range.end), &edit.new_text);
    }
    out
}

/// Shrunk table columns (`org-table-toggle-column-width`, `#+STARTUP:
/// shrink`): each field's text beyond the column's width cookie hidden in
/// the tokens, and `…` an inlay hint in its place, as Org's overlays show.
#[test]
fn shrinks_table_columns() {
    let mut client = Client::start(
        json!({ "textDocument": { "inlayHint": {} }, "general": { "positionEncodings": ["utf-8"] } }),
        json!({}),
    );
    let text = "#+STARTUP: shrink\n| <3>    | b |\n| abcdef | x |\n";
    client.open(&uri(), text);
    let hints = |client: &mut Client| -> Vec<(u32, u32, String)> {
        let params = json!({ "textDocument": { "uri": uri().as_str() }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 3, "character": 0 } } });
        let result = client.raw_request("textDocument/inlayHint", params).response_result.unwrap();
        result
            .as_array()
            .into_iter()
            .flatten()
            .map(|hint| {
                (
                    hint["position"]["line"].as_u64().unwrap() as u32,
                    hint["position"]["character"].as_u64().unwrap() as u32,
                    hint["label"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    // Shrunk when the file opens: `abc…`, as Emacs shows it.
    assert_eq!(hints(&mut client), [(1, 1, " ".into()), (1, 5, "…".into()), (2, 1, " ".into()), (2, 5, "…".into())]);
    // The same text sent again, as yegappan/lsp and vim-lsp do once the
    // file is open: the columns stay shrunk.
    client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier { uri: uri(), version: 2 },
        content_changes: vec![TextDocumentContentChangeEvent { range: None, range_length: None, text: text.into() }],
    });
    assert_eq!(hints(&mut client), [(1, 1, " ".into()), (1, 5, "…".into()), (2, 1, " ".into()), (2, 5, "…".into())]);
    let toggle = |client: &mut Client, line: u32, character: u32, extra: Value| {
        let mut context =
            json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": line, "character": character } });
        for (key, value) in extra.as_object().unwrap() {
            context[key] = value.clone();
        }
        client
            .raw_request(
                "workspace/executeCommand",
                json!({ "command": "org.table.toggle.column.width", "arguments": [context] }),
            )
            .response_result
            .unwrap()
    };
    // `C-c TAB` in the first column: shown whole.
    toggle(&mut client, 2, 3, json!({}));
    assert_eq!(hints(&mut client), []);
    // In the second, without a cookie: `…` alone.
    toggle(&mut client, 2, 11, json!({}));
    assert_eq!(hints(&mut client), [(1, 10, "…".into()), (2, 10, "…".into())]);
    // Before the first column, the columns asked for.
    let asked = toggle(&mut client, 2, 0, json!({}));
    assert_eq!(asked["input"]["prompt"], "Column ranges (e.g. 2-4 6-): ");
    toggle(&mut client, 2, 0, json!({ "input": "1-" }));
    assert_eq!(hints(&mut client), [(1, 1, " ".into()), (1, 5, "…".into()), (2, 1, " ".into()), (2, 5, "…".into())]);
    // `C-u C-u`: everything shown.
    toggle(&mut client, 2, 3, json!({ "count": 16 }));
    assert_eq!(hints(&mut client), []);
}

/// The relative timer and the countdown, for the editor's status line as
/// Emacs's mode line shows them (`org-lsp/timer`).
#[test]
fn tells_the_editor_how_the_timer_shows() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("tells_the_editor_how_the_timer_shows: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-timer-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "* Meeting\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let mut run = |command: &str, input: Option<&str>| -> Value {
        let mut argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
        if let Some(input) = input {
            argument["input"] = json!(input);
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result");
        let Message::Notification(note) = next_from_server(
            &client,
            |message| matches!(message, Message::Notification(note) if note.method == "org-lsp/timer"),
        ) else {
            unreachable!()
        };
        note.params
    };
    assert_eq!(run("org.timer.start", None), json!({ "seconds": 0, "countdown": false, "paused": false }));
    assert_eq!(run("org.timer.pause", None), json!({ "seconds": 0, "countdown": false, "paused": true }));
    assert_eq!(run("org.timer.stop", None), Value::Null);
    let countdown = run("org.timer.set", Some("10"));
    assert_eq!((countdown["countdown"].clone(), countdown["paused"].clone()), (json!(true), json!(false)));
    assert!(countdown["seconds"].as_i64().is_some_and(|seconds| (599..=600).contains(&seconds)), "{countdown}");
    assert_eq!(run("org.timer.stop", None), Value::Null);
}

#[test]
fn keeps_timers_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("keeps_timers_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-emacs-timers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "* Meeting\n\n* Report\n:PROPERTIES:\n:Effort:   0:30\n:END:\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let request = |client: &mut Client, command: &str, at: (u32, u32), input: Option<&str>| -> Value {
        let mut argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": at.0, "character": at.1 } });
        if let Some(input) = input {
            argument["input"] = json!(input);
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    let said = |client: &mut Client| -> String {
        let Message::Notification(note) = next_from_server(
            client,
            |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
        ) else {
            unreachable!()
        };
        note.params["message"].as_str().unwrap().to_string()
    };
    // The edit Emacs makes, applied as the editor would.
    let edited = |client: &mut Client| -> Vec<TextEdit> {
        let Message::Request(edit) = next_from_server(
            client,
            |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
        ) else {
            unreachable!()
        };
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(edit.id.clone(), json!({ "applied": true }))))
            .unwrap();
        serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap()
    };
    request(&mut client, "org.timer.start", (1, 0), None);
    assert!(said(&mut client).starts_with("Timer start time set to "));
    request(&mut client, "org.timer.insert", (1, 0), None);
    assert_eq!(
        apply_text_edits(text, &edited(&mut client)),
        "* Meeting\n0:00:00 \n* Report\n:PROPERTIES:\n:Effort:   0:30\n:END:\n"
    );
    request(&mut client, "org.timer.item", (1, 0), None);
    assert_eq!(
        apply_text_edits(text, &edited(&mut client)),
        "* Meeting\n- 0:00:00 :: \n* Report\n:PROPERTIES:\n:Effort:   0:30\n:END:\n"
    );
    request(&mut client, "org.timer.pause", (1, 0), None);
    assert_eq!(said(&mut client), "Timer paused at 0:00:00 ");
    request(&mut client, "org.timer.stop", (1, 0), None);
    assert_eq!(said(&mut client), "Timer stopped");
    // A countdown: asked for, then said when it ends, by the server.
    let asked = request(&mut client, "org.timer.set", (0, 0), None);
    assert_eq!(asked["input"]["kind"], "text", "{asked}");
    request(&mut client, "org.timer.set", (0, 0), Some("0:00:01"));
    assert_eq!(said(&mut client), "Meeting: time out");
    // Another while one runs: Emacs asks whether to replace it.
    request(&mut client, "org.timer.set", (0, 0), Some("10"));
    for (answer, then) in [("No", Some("No timer set")), ("Yes", None)] {
        request(&mut client, "org.timer.set", (0, 0), Some("20"));
        let Message::Request(question) = next_from_server(
            &client,
            |message| matches!(message, Message::Request(request) if request.method == "window/showMessageRequest"),
        ) else {
            unreachable!()
        };
        assert_eq!(question.params["message"], "Replace current timer?");
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(question.id, json!({ "title": answer }))))
            .unwrap();
        if let Some(then) = then {
            assert_eq!(said(&mut client), then);
        }
    }
    request(&mut client, "org.timer.pause", (0, 0), None);
    let paused = said(&mut client);
    assert!(paused.starts_with("Timer paused at 0:19:5") || paused == "Timer paused at 0:20:00 ", "{paused}");
    request(&mut client, "org.timer.stop", (0, 0), None);
    assert_eq!(said(&mut client), "Timer stopped");
    // With an effort, Org takes its minutes and asks nothing.
    let asked = request(&mut client, "org.timer.set", (2, 0), None);
    assert_eq!(asked, Value::Null);
    request(&mut client, "org.timer.pause", (2, 0), None);
    let paused = said(&mut client);
    assert!(paused == "Timer paused at 0:30:00 " || paused.starts_with("Timer paused at 0:29:5"), "{paused}");
    request(&mut client, "org.timer.stop", (2, 0), None);
    assert_eq!(said(&mut client), "Timer stopped");
    // `C-u`: started with an offset, the value at the cursor by default;
    // `C-u C-u` or a selection: the values in it shifted, the first to
    // zero by default.
    let items = "- 0:01:00 :: a\n- 0:02:30 :: b\n";
    let list: Uri = format!("file://{}", directory.join("items.org").display()).parse().unwrap();
    client.open(&list, items);
    let call = |client: &mut Client, extra: Value| -> Value {
        let mut argument =
            json!({ "textDocument": { "uri": list.as_str() }, "position": { "line": 0, "character": 4 } });
        for (key, value) in extra.as_object().unwrap() {
            argument[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.timer.start", "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    let asked = call(&mut client, json!({ "count": 4 }));
    assert_eq!(asked["input"]["prompt"], "Restart timer with offset [0:01:00]", "{asked}");
    call(&mut client, json!({ "count": 4, "input": "" }));
    let started = said(&mut client);
    assert!(started.ends_with("current value is 0:01:00"), "{started}");
    request(&mut client, "org.timer.stop", (2, 0), None);
    assert_eq!(said(&mut client), "Timer stopped");
    let range = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 2, "character": 0 } });
    // The cursor at the end of the selection.
    let end = json!({ "line": 2, "character": 0 });
    let asked = call(&mut client, json!({ "range": range, "position": end }));
    assert_eq!(asked["input"]["kind"], "text", "{asked}");
    call(&mut client, json!({ "range": range, "position": end, "input": "" }));
    let Message::Request(edit) = next_from_server(
        &client,
        |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
    ) else {
        unreachable!()
    };
    let edits: Vec<TextEdit> = serde_json::from_value(edit.params["edit"]["changes"][list.as_str()].clone()).unwrap();
    assert_eq!(apply_text_edits(items, &edits), "- 0:00:00 :: a\n- 0:01:30 :: b\n");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn reports_and_moves_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("reports_and_moves_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-emacs-moves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "| 1 | x |\n| 2 | y |\n\n#+name: one\n#+begin_src sh\necho 1\n#+end_src\n\n#+begin_quote\nq\n#+end_quote\n\n#+name: two\n#+begin_src sh\necho 2\n#+end_src\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let request = |client: &mut Client, command: &str, at: (u32, u32), input: Option<&str>| -> Value {
        let mut argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": at.0, "character": at.1 } });
        if let Some(input) = input {
            argument["input"] = json!(input);
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    // What Emacs says.
    let said = |client: &mut Client, command: &str, at: (u32, u32)| -> String {
        request(client, command, at, None);
        let Message::Notification(note) = next_from_server(
            client,
            |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
        ) else {
            unreachable!()
        };
        note.params["message"].as_str().unwrap().to_string()
    };
    // Where Emacs leaves the point.
    let moved = |client: &mut Client, command: &str, at: (u32, u32), input: Option<&str>| -> (u32, u32) {
        request(client, command, at, input);
        let Message::Request(show) = next_from_server(
            client,
            |message| matches!(message, Message::Request(request) if request.method == "window/showDocument"),
        ) else {
            unreachable!()
        };
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(show.id.clone(), json!({ "success": true }))))
            .unwrap();
        let start = &show.params["selection"]["start"];
        (start["line"].as_u64().unwrap() as u32, start["character"].as_u64().unwrap() as u32)
    };
    let sum = said(&mut client, "org.table.sum", (0, 2));
    assert!(sum.starts_with("Sum of 2 items: 3"), "{sum}");
    let info = said(&mut client, "org.table.field.info", (1, 6));
    assert!(info.starts_with("line @2, col $2, ref @2$2 or B2"), "{info}");
    assert_eq!(moved(&mut client, "org.babel.next", (0, 0), None), (4, 0));
    assert_eq!(moved(&mut client, "org.babel.next", (4, 0), None), (13, 0));
    assert_eq!(moved(&mut client, "org.babel.previous", (13, 0), None), (4, 0));
    assert_eq!(moved(&mut client, "org.block.next", (4, 0), None), (8, 0));
    assert_eq!(moved(&mut client, "org.block.previous", (8, 0), None), (4, 0));
    let asked = request(&mut client, "org.babel.goto", (0, 0), None);
    let names: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["key"].as_str().unwrap()).collect();
    assert_eq!(names, ["one", "two"]);
    assert_eq!(moved(&mut client, "org.babel.goto", (0, 0), Some("two")), (13, 0));
    // The named blocks into the Library of Babel.
    assert_eq!(said(&mut client, "org.babel.lob.ingest", (0, 0)), "2 source blocks added to Library of Babel");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// `org-babel-detangle` through Emacs: the code of the one file the
/// blocks tangle to put back into the block its link comments lead to.
#[test]
fn detangles_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("detangles_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-detangle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "* Code\n#+begin_src python :tangle out.py :comments link\nprint(\"one\")\n#+end_src\n";
    std::fs::write(&file, text).unwrap();
    std::fs::write(
        directory.join("out.py"),
        "# [[file:notes.org::*Code][Code:1]]\nprint(\"two\")\n# Code:1 ends here\n",
    )
    .unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
    let result = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.babel.detangle", "arguments": [argument] }))
        .response_result
        .expect("a result");
    assert_eq!(result, Value::Null);
    let message = next_from_server(&client, |message| {
        matches!(message, Message::Request(request) if request.method == "workspace/applyEdit")
            || matches!(message, Message::Notification(note) if note.method == "window/showMessage")
    });
    let Message::Request(edit) = message else { panic!("Emacs said {message:?}") };
    let edits: Vec<TextEdit> = serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
    let detangled = apply_text_edits(text, &edits);
    assert!(detangled.contains("print(\"two\")") && !detangled.contains("one"), "{detangled}");
    client.connection.sender.send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true })))).unwrap();
    client.shutdown();
    let _ = std::fs::remove_dir_all(&directory);
}

/// `org-plot/gnuplot` (`C-c " g`) and `orgtbl-ascii-plot` (`C-c " a`)
/// through Emacs: gnuplot.el, here a stand-in that keeps what it is sent,
/// gets the script org-plot writes from the `#+PLOT:` line; the bars are
/// drawn in a new column.
#[test]
fn plots_tables_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("plots_tables_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-plot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let script = directory.join("sent.gp");
    let file = directory.join("plot.org");
    let text = "#+PLOT: title:\"Sales\" ind:1 deps:(2) type:2d with:lines file:\"sales.png\"\n\
                | Month | Sales |\n|-------+-------|\n|     1 |    10 |\n|     2 |    30 |\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let load = std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let config = format!(
        "{load}\n(defun gnuplot-mode () nil)\n\
         (defun gnuplot-send-buffer-to-gnuplot () (write-region nil nil {:?}))\n(provide 'gnuplot)",
        script.display().to_string()
    );
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let at = |line: u32, character: u32| json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": line, "character": character } });
    let result = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.plot.gnuplot", "arguments": [at(3, 2)] }))
        .response_result
        .expect("a result");
    assert_eq!(result, Value::Null);
    // Emacs plots after the command is answered.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !script.exists() && std::time::Instant::now() < deadline {
        if let Ok(Message::Notification(note)) =
            client.connection.receiver.recv_timeout(std::time::Duration::from_millis(100))
        {
            assert_ne!(note.method, "window/showMessage", "Emacs said {}", note.params);
        }
    }
    let sent = std::fs::read_to_string(&script).expect("the script gnuplot gets");
    assert!(sent.contains("set title 'Sales'"), "{sent}");
    // The plot's file is in the Org file's directory.
    assert!(sent.contains(&format!("set output '{}'", directory.join("sales.png").display())), "{sent}");
    assert!(sent.contains("with lines"), "{sent}");
    // The bars of the column at the cursor, in a column of their own.
    let result = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.table.ascii.plot", "arguments": [at(3, 12)] }))
        .response_result
        .expect("a result");
    assert_eq!(result, Value::Null);
    let message = next_from_server(&client, |message| {
        matches!(message, Message::Request(request) if request.method == "workspace/applyEdit")
            || matches!(message, Message::Notification(note) if note.method == "window/showMessage")
    });
    let Message::Request(edit) = message else { panic!("Emacs said {message:?}") };
    let edits: Vec<TextEdit> = serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
    let plotted = apply_text_edits(text, &edits);
    assert!(plotted.contains("#+TBLFM: $3='(orgtbl-ascii-draw $2 10 30 12)"), "{plotted}");
    client.connection.sender.send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true })))).unwrap();
    client.shutdown();
    // Without gnuplot.el, Org names the command it runs (`this-command`).
    let mut client = Client::start(json!({}), json!({ "emacs_config": load }));
    client.open(&uri, text);
    client
        .raw_request("workspace/executeCommand", json!({ "command": "org.plot.gnuplot", "arguments": [at(3, 2)] }))
        .response_result
        .expect("a result");
    let Message::Notification(said) = next_from_server(
        &client,
        |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
    ) else {
        unreachable!()
    };
    // Quoted as the locale lets Emacs: ‘…’ or `…'.
    let message = said.params["message"].as_str().unwrap().replace(['‘', '’', '`', '\''], "");
    assert_eq!(message, "org-plot/gnuplot failed to load required package \"gnuplot\"");
    client.shutdown();
    let _ = std::fs::remove_dir_all(&directory);
}

/// The formula debugger (`C-c {`): while it is on, recalculating goes
/// through Emacs, which asks after each formula whether to go on, its
/// substitution history with the question.
#[test]
fn debugs_formulas_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("debugs_formulas_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-debugger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let text = "| 1 | 2 |   |\n| 3 | 4 |   |\n#+TBLFM: $3=$1+$2\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let mut client = Client::start(json!({}), json!({ "emacs_config": config }));
    client.open(&uri, text);
    let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 2 } });
    let run = |client: &mut Client, command: &str| -> Value {
        client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument.clone()] }))
            .response_result
            .expect("a result")
    };
    assert_eq!(run(&mut client, "org.table.formula.debugger")["message"], "Formula debugging has been turned on");
    assert_eq!(run(&mut client, "org.table.recalculate"), Value::Null);
    let mut histories = Vec::new();
    let edit = loop {
        let message = next_from_server(&client, |message| {
            matches!(message, Message::Request(request)
                if request.method == "window/showMessageRequest" || request.method == "workspace/applyEdit")
                || matches!(message, Message::Notification(note) if note.method == "window/showMessage")
        });
        match message {
            Message::Request(request) if request.method == "window/showMessageRequest" => {
                histories.push(request.params["message"].as_str().unwrap().to_string());
                client
                    .connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, json!({ "title": "Yes" }))))
                    .unwrap();
            }
            Message::Request(request) => break request,
            other => panic!("Emacs said {other:?}"),
        }
    };
    assert_eq!(histories.len(), 2, "{histories:?}");
    assert!(
        histories[0].contains("$1->    (1)+(2)") && histories[0].ends_with("Debugging Formula.  Continue to next?")
    );
    assert!(histories[1].contains("Final:  7"), "{}", histories[1]);
    let edits: Vec<TextEdit> = serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
    assert_eq!(apply_text_edits(text, &edits), "| 1 | 2 | 3 |\n| 3 | 4 | 7 |\n#+TBLFM: $3=$1+$2\n");
    client.connection.sender.send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true })))).unwrap();
    client.shutdown();
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn edits_through_emacs() {
    let found = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
        eprintln!("edits_through_emacs: no Emacs, skipped");
        return;
    }
    let directory = std::env::temp_dir().join(format!("org-lsp-emacs-edits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    std::fs::write(&file, "").unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let config =
        std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
    let options = json!({
        "emacs_config": config,
        "emacs_options": { "org-attach-method": "mv" },
        "org_table_formula_constants": { "k": "10" }
    });
    let mut client = Client::start(json!({}), options);
    let mut version = 1;
    let mut text = "* Head\nSome text here.\nline\n".to_string();
    client.open(&uri, &text);
    // Run `command` at `line`:`character` with `extra` in the argument, then
    // take the edit Emacs makes and the cursor it leaves.
    let run = |client: &mut Client,
               text: &mut String,
               version: &mut i32,
               command: &str,
               at: (u32, u32),
               extra: Value|
     -> (Value, Option<(u32, u32)>) {
        let mut argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": at.0, "character": at.1 } });
        for (key, value) in extra.as_object().unwrap() {
            argument[key] = value.clone();
        }
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
            .response_result
            .expect("a result");
        if result.get("input").is_some() {
            return (result, None);
        }
        let Message::Request(edit) = next_from_server(
            client,
            |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
        ) else {
            unreachable!()
        };
        let edits: Vec<TextEdit> =
            serde_json::from_value(edit.params["edit"]["changes"][uri.as_str()].clone()).unwrap();
        *text = apply_text_edits(text, &edits);
        *version += 1;
        client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri.clone(), version: *version },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.clone(),
            }],
        });
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(edit.id, json!({ "applied": true }))))
            .unwrap();
        let Message::Request(show) = next_from_server(
            client,
            |message| matches!(message, Message::Request(request) if request.method == "window/showDocument"),
        ) else {
            unreachable!()
        };
        let start = &show.params["selection"]["start"];
        let cursor = (start["line"].as_u64().unwrap() as u32, start["character"].as_u64().unwrap() as u32);
        client
            .connection
            .sender
            .send(Message::Response(Response::new_ok(show.id, json!({ "success": true }))))
            .unwrap();
        (result, Some(cursor))
    };

    // A new footnote: the reference at the cursor, the definition in the
    // Footnotes section, the cursor after its label.
    let (_, cursor) = run(&mut client, &mut text, &mut version, "org.footnote.action", (1, 5), json!({}));
    assert_eq!(text, "* Head\nSome [fn:1]text here.\nline\n\n* Footnotes\n\n[fn:1] \n");
    assert_eq!(cursor, Some((6, 6)));
    // `C-c -` off a table: an item; `C-c *`: a heading.
    run(&mut client, &mut text, &mut version, "org.table.hline", (2, 0), json!({}));
    assert!(text.contains("\n- line\n"), "{text}");
    run(&mut client, &mut text, &mut version, "org.table.recalculate.line", (2, 0), json!({}));
    assert!(text.contains("\n** line\n"), "{text}");
    // `C-c ;`
    run(&mut client, &mut text, &mut version, "org.toggle.comment", (0, 0), json!({}));
    assert!(text.starts_with("* COMMENT Head\n"), "{text}");
    // Emphasis: asked for a marker, then the markers around the cursor.
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.emphasize", (1, 0), json!({}));
    assert!(asked["input"]["choices"].as_array().unwrap().iter().any(|choice| choice["key"] == "*"), "{asked}");
    let (_, cursor) = run(&mut client, &mut text, &mut version, "org.emphasize", (1, 18), json!({ "input": "*" }));
    let (line, character) = cursor.expect("the cursor");
    let marked = text.lines().nth(line as usize).unwrap();
    assert_eq!(&marked[character as usize - 1..=character as usize], "**", "{text}");
    // A block from the structure templates Emacs has.
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.structure.template", (3, 0), json!({}));
    assert!(
        asked["input"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|choice| choice["key"] == "s" && choice["label"] == "src"),
        "{asked}"
    );
    run(&mut client, &mut text, &mut version, "org.structure.template", (3, 0), json!({ "input": "q" }));
    assert!(text.contains("#+begin_quote\n"), "{text}");
    // A drawer.
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.drawer.insert", (0, 0), json!({}));
    assert_eq!(asked["input"]["kind"], "text");
    run(&mut client, &mut text, &mut version, "org.drawer.insert", (2, 0), json!({ "input": "NOTES" }));
    assert!(text.contains(":NOTES:\n"), "{text}");
    // The footnote menu, as `C-u C-c C-x f` offers it.
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.footnote.menu", (0, 0), json!({}));
    let keys: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["s", "r", "S", "n", "d"]);
    // Cloning: how many, then the shift.
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.subtree.clone", (0, 0), json!({ "input": "2" }));
    assert_eq!(asked["input"]["context"]["step"], "shift");
    run(
        &mut client,
        &mut text,
        &mut version,
        "org.subtree.clone",
        (0, 0),
        json!({ "input": "", "step": "shift", "answers": "2" }),
    );
    assert_eq!(text.matches("* COMMENT Head").count(), 3, "{text}");
    // Formulas org-lsp leaves to Emacs: Emacs Lisp, computed there by
    // `C-u C-c *`, and by `C-c C-c` on the `#+TBLFM:` line.
    let replace = |client: &mut Client, text: &mut String, version: &mut i32, new: String| {
        *text = new;
        *version += 1;
        client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier { uri: uri.clone(), version: *version },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.clone(),
            }],
        });
    };
    let table = "* Sums\n| 1 | 2 |   |\n| 3 | 4 |   |\n#+TBLFM: $3='(+ $1 $2);N\n".to_string();
    replace(&mut client, &mut text, &mut version, table);
    run(&mut client, &mut text, &mut version, "org.table.recalculate", (1, 2), json!({}));
    assert_eq!(text, "* Sums\n| 1 | 2 | 3 |\n| 3 | 4 | 7 |\n#+TBLFM: $3='(+ $1 $2);N\n");
    let changed = text.replace("(+ $1 $2)", "(* $1 $2)");
    replace(&mut client, &mut text, &mut version, changed);
    run(&mut client, &mut text, &mut version, "org.checkbox.toggle", (3, 0), json!({}));
    assert_eq!(text, "* Sums\n| 1 | 2 |  2 |\n| 3 | 4 | 12 |\n#+TBLFM: $3='(* $1 $2);N\n");
    // Emacs has the constants of the settings (`org-table-formula-constants`).
    let changed = text.replace("(* $1 $2)", "(* $1 $k)");
    replace(&mut client, &mut text, &mut version, changed);
    run(&mut client, &mut text, &mut version, "org.table.recalculate", (1, 2), json!({}));
    assert_eq!(text, "* Sums\n| 1 | 2 | 10 |\n| 3 | 4 | 30 |\n#+TBLFM: $3='(* $1 $k);N\n");
    // Tangling to a file named by Emacs Lisp: Emacs tangles.
    let source = "#+begin_src emacs-lisp :tangle (concat \"out\" \".el\")\n(message \"hi\")\n#+end_src\n";
    replace(&mut client, &mut text, &mut version, source.to_string());
    let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
    client.raw_request("workspace/executeCommand", json!({ "command": "org.babel.tangle", "arguments": [argument] }));
    let said = next_from_server(
        &client,
        |message| matches!(message, Message::Notification(note) if note.method == "window/showMessage"),
    );
    let Message::Notification(said) = said else { unreachable!() };
    assert_eq!(said.params["message"], "Tangled 1 code block from notes.org", "{said:?}");
    assert_eq!(std::fs::read_to_string(directory.join("out.el")).unwrap(), "(message \"hi\")\n");
    // Tables: copying down (`S-RET`), an empty table (`C-c |`).
    replace(&mut client, &mut text, &mut version, "| a | 1 |\n|   |   |\n|   |   |\n".to_string());
    let (_, cursor) = run(&mut client, &mut text, &mut version, "org.table.copy.down", (0, 6), json!({}));
    assert_eq!(text, "| a | 1 |\n|   | 2 |\n|   |   |\n");
    assert_eq!(cursor, Some((1, 6)));
    replace(&mut client, &mut text, &mut version, "Text\n\n".to_string());
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.table.create", (1, 0), json!({}));
    assert_eq!(asked["input"]["kind"], "text");
    run(&mut client, &mut text, &mut version, "org.table.create", (1, 0), json!({ "input": "2x2" }));
    assert_eq!(text, "Text\n|   |   |\n|---+---|\n|   |   |\n\n");
    // On a selection, Emacs's region: `C-c |` makes the lines a table,
    // `M-<right>` demotes the headlines in it, `C-c -` makes items.
    let region = |from: (u32, u32), to: (u32, u32)| json!({ "range": { "start": { "line": from.0, "character": from.1 }, "end": { "line": to.0, "character": to.1 } } });
    replace(&mut client, &mut text, &mut version, "a,b\nc,d\n".to_string());
    run(&mut client, &mut text, &mut version, "org.table.create", (0, 0), region((0, 0), (2, 0)));
    assert_eq!(text, "| a | b |\n| c | d |\n");
    replace(&mut client, &mut text, &mut version, "* A\n* B\none\ntwo\n".to_string());
    run(&mut client, &mut text, &mut version, "org.meta.right", (0, 0), region((0, 0), (2, 0)));
    assert_eq!(text, "** A\n** B\none\ntwo\n");
    run(&mut client, &mut text, &mut version, "org.table.hline", (2, 0), region((2, 0), (4, 0)));
    assert_eq!(text, "** A\n** B\n- one\n- two\n");
    // In a table, cutting the cells of the region and pasting them as a
    // rectangle (`org-table-cut-region`, `org-table-paste-rectangle`).
    replace(&mut client, &mut text, &mut version, "| a | b |\n| c | d |\n".to_string());
    run(&mut client, &mut text, &mut version, "org.cut.subtree", (0, 2), region((0, 2), (1, 3)));
    assert_eq!(text, "|   | b |\n|   | d |\n");
    run(&mut client, &mut text, &mut version, "org.paste.subtree", (0, 6), json!({}));
    assert_eq!(text, "|   | a |\n|   | c |\n");
    // A file of comma-separated values imported as a table, and the
    // table exported to another, asked for with its format.
    std::fs::write(directory.join("data.csv"), "a,b\nc,d\n").unwrap();
    replace(&mut client, &mut text, &mut version, "\n".to_string());
    run(&mut client, &mut text, &mut version, "org.table.import", (0, 0), json!({ "input": "data.csv" }));
    assert_eq!(text, "| a | b |\n| c | d |\n\n");
    let export = |client: &mut Client, extra: Value| -> Value {
        let mut argument =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 2 } });
        for (key, value) in extra.as_object().unwrap() {
            argument[key] = value.clone();
        }
        client
            .raw_request("workspace/executeCommand", json!({ "command": "org.table.export", "arguments": [argument] }))
            .response_result
            .expect("a result")
    };
    let asked = export(&mut client, json!({}));
    assert_eq!(asked["input"]["prompt"], "Export table to", "{asked}");
    let asked = export(&mut client, json!({ "input": "out.csv", "step": "file", "answers": "[]" }));
    assert_eq!(asked["input"]["prompt"], "Format", "{asked}");
    assert_eq!(asked["input"]["choices"][0]["key"], "orgtbl-to-csv", "{asked}");
    let context = asked["input"]["context"].clone();
    export(&mut client, json!({ "input": "orgtbl-to-csv", "step": "format", "answers": context["answers"] }));
    let written = directory.join("out.csv");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !written.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(std::fs::read_to_string(&written).unwrap(), "a,b\nc,d\n");
    // `C-c C-a c`: a file attached by copying it, the entry tagged and
    // given an ID.
    std::fs::write(directory.join("doc.txt"), "attached\n").unwrap();
    replace(&mut client, &mut text, &mut version, "* Task\n".to_string());
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.attach", (0, 0), json!({ "input": "c" }));
    assert_eq!(asked["input"]["prompt"], "File to keep as an attachment", "{asked}");
    run(&mut client, &mut text, &mut version, "org.attach", (0, 0), json!({ "input": "doc.txt", "step": "c" }));
    assert!(text.starts_with("* Task") && text.contains(":ATTACH:") && text.contains(":ID:"), "{text}");
    let id = text.lines().find_map(|line| line.trim().strip_prefix(":ID:")).unwrap().trim().to_string();
    let copied = directory.join("data").join(&id[..2]).join(&id[2..]).join("doc.txt");
    assert_eq!(std::fs::read_to_string(&copied).unwrap(), "attached\n");
    // `C-c C-a a`: by `org-attach-method`, which `emacs_options` makes `mv`.
    std::fs::write(directory.join("moved.txt"), "moved\n").unwrap();
    replace(&mut client, &mut text, &mut version, "* Other\n".to_string());
    run(&mut client, &mut text, &mut version, "org.attach", (0, 0), json!({ "input": "moved.txt", "step": "a" }));
    let id = text.lines().find_map(|line| line.trim().strip_prefix(":ID:")).unwrap().trim().to_string();
    let moved = directory.join("data").join(&id[..2]).join(&id[2..]).join("moved.txt");
    assert_eq!(std::fs::read_to_string(&moved).unwrap(), "moved\n");
    assert!(!directory.join("moved.txt").exists(), "the file is moved, not copied");
    // A column view inserted (`C-c C-x x`), then filled in again by
    // `C-c C-c` on its `#+BEGIN:` line, both by Emacs.
    replace(&mut client, &mut text, &mut version, "* TODO A\n* B\n\n".to_string());
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.dblock.insert", (2, 0), json!({}));
    assert_eq!(asked["input"]["choices"][1]["key"], "columnview", "{asked}");
    let (asked, _) =
        run(&mut client, &mut text, &mut version, "org.dblock.insert", (2, 0), json!({ "input": "columnview" }));
    assert_eq!(asked["input"]["context"]["step"], "columnview", "{asked}");
    run(
        &mut client,
        &mut text,
        &mut version,
        "org.dblock.insert",
        (2, 0),
        json!({ "input": "global", "step": "columnview" }),
    );
    assert!(text.contains("#+BEGIN: columnview :hlines 1 :id global\n"), "{text}");
    assert!(text.contains("| A    | TODO |"), "{text}");
    let begin = text.lines().position(|line| line.starts_with("#+BEGIN:")).unwrap() as u32;
    let changed = text.replacen("* TODO A", "* DONE A", 1);
    replace(&mut client, &mut text, &mut version, changed);
    run(&mut client, &mut text, &mut version, "org.checkbox.toggle", (begin, 0), json!({}));
    assert!(text.contains("| A    | DONE |"), "{text}");
    // `#` in the export menu: the options template of a backend.
    replace(&mut client, &mut text, &mut version, "* Head\n".to_string());
    let (asked, _) = run(&mut client, &mut text, &mut version, "org.export.dispatch", (0, 0), json!({ "input": "#" }));
    let backends: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["key"].as_str().unwrap()).collect();
    assert_eq!(backends.first(), Some(&"default"), "{asked}");
    assert!(backends.contains(&"html"), "{backends:?}");
    run(
        &mut client,
        &mut text,
        &mut version,
        "org.export.dispatch",
        (0, 0),
        json!({ "input": "default", "step": "#", "answers": "" }),
    );
    assert!(text.contains("#+title: ") && text.contains("#+options: ") && text.ends_with("* Head\n"), "{text}");
    // `S-<down>` and `S-<right>` in a table move the field.
    replace(&mut client, &mut text, &mut version, "| a | b |\n| c | d |\n".to_string());
    let (_, cursor) = run(&mut client, &mut text, &mut version, "org.shift.down", (0, 2), json!({}));
    assert_eq!(text, "| c | b |\n| a | d |\n");
    assert_eq!(cursor, Some((1, 2)));
    run(&mut client, &mut text, &mut version, "org.shift.right", (1, 2), json!({}));
    assert_eq!(text, "| c | b |\n| d | a |\n");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn takes_back_an_edit_the_editor_does_not_make() {
    let directory = std::env::temp_dir().join(format!("org-lsp-refused-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("tasks.org");
    let text = "* Task\n";
    std::fs::write(&file, text).unwrap();
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let mut client = Client::start(json!({}), json!({}));
    client.open(&uri, text);
    let run = |client: &mut Client, command: &str| -> Response {
        let argument = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
        client.raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [argument] }))
    };
    run(&mut client, "org.clock.in");
    // The editor does not make the clock's edit.
    let edit = match client.asked.iter().position(|request| request.method == "workspace/applyEdit") {
        Some(at) => client.asked.remove(at),
        None => {
            let Message::Request(edit) = next_from_server(
                &client,
                |message| matches!(message, Message::Request(request) if request.method == "workspace/applyEdit"),
            ) else {
                unreachable!()
            };
            edit
        }
    };
    client
        .connection
        .sender
        .send(Message::Response(Response::new_ok(edit.id, json!({ "applied": false, "failureReason": "read-only" }))))
        .unwrap();
    let Message::Notification(said) = next_from_server(&client, |message| {
        matches!(message, Message::Notification(note) if note.method == "window/showMessage"
            && note.params["message"].as_str().is_some_and(|text| text.starts_with("The editor did not")))
    }) else {
        unreachable!()
    };
    assert_eq!(said.params["message"], "The editor did not make the change: read-only");
    // No clock runs: the file has none.
    let out = run(&mut client, "org.clock.out");
    assert_eq!(out.response_result.unwrap_err().message, "No active clock");
    // Nor was it ever clocked, for clocking in again on the last task.
    let last = run(&mut client, "org.clock.in.last");
    assert_eq!(last.response_result.unwrap()["message"], "No last clock");
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// What was said of a file goes when it is closed: an empty list of
/// diagnostics for it.
#[test]
fn clears_the_diagnostics_of_a_closed_file() {
    let client = Client::start(json!({}), json!({}));
    client.open(&uri(), "* Note\nSee [[*Missing]].\n");
    let published = client.notification("textDocument/publishDiagnostics");
    assert!(!published["diagnostics"].as_array().unwrap().is_empty(), "{published}");
    client.notify::<lsp_types::notification::DidCloseTextDocument>(lsp_types::DidCloseTextDocumentParams {
        text_document: identifier(),
    });
    let published = client.notification("textDocument/publishDiagnostics");
    assert_eq!(published["uri"], uri().as_str());
    assert_eq!(published["diagnostics"], json!([]));
}

/// A client that sends something more often than the diagnostics wait
/// for (a cursor that moves, tokens of the range scrolled to) still gets
/// them, a second after the change at most.
#[test]
fn diagnostics_come_while_other_messages_do() {
    let client = Client::start(json!({}), json!({}));
    let started = std::time::Instant::now();
    client.open(&uri(), "* Note\nSee [[*Missing]].\n");
    let mut published = None;
    while published.is_none() && started.elapsed() < std::time::Duration::from_secs(5) {
        let notification = Notification { method: "$/cursorMoved".to_string(), params: json!({}) };
        client.connection.sender.send(Message::Notification(notification)).unwrap();
        if let Ok(Message::Notification(notification)) =
            client.connection.receiver.recv_timeout(std::time::Duration::from_millis(50))
        {
            if notification.method == "textDocument/publishDiagnostics" {
                published = Some(started.elapsed());
            }
        }
    }
    let published = published.expect("diagnostics within 5 s");
    assert!(published < std::time::Duration::from_secs(3), "{published:?}");
}

/// After `shutdown`, a request is refused (`InvalidRequest`) and a
/// notification dropped, until `exit` ends the server as it should.
#[test]
fn refuses_requests_after_shutdown() {
    let mut client = Client::start(json!({}), json!({}));
    client.open(&uri(), "* Note\n");
    client.request::<lsp_types::request::Shutdown>(());
    let refused = client.raw_request("textDocument/foldingRange", json!({ "textDocument": { "uri": uri().as_str() } }));
    assert_eq!(
        refused.response_result.err().map(|error| error.code),
        Some(lsp_server::ErrorCode::InvalidRequest as i32)
    );
    client.notify::<lsp_types::notification::DidCloseTextDocument>(lsp_types::DidCloseTextDocumentParams {
        text_document: identifier(),
    });
    client.notify::<lsp_types::notification::Exit>(());
    client.server.take().unwrap().join().expect("the server ends cleanly");
}

/// `org-ellipsis`: what closed folds show after their first line, in the
/// initialize result and on request; the standard three dots unless it is
/// a string that is not empty.
#[test]
fn announces_the_ellipsis() {
    for (setting, shown) in [(json!(null), "..."), (json!(""), "..."), (json!(" ▾"), " ▾")] {
        let mut client = Client::start(json!({}), json!({ "org_ellipsis": setting }));
        let experimental = client.initialize.capabilities.experimental.clone().unwrap();
        assert_eq!(experimental["orgEllipsis"], json!(shown));
        let response = client.raw_request("org-lsp/ellipsis", Value::Null);
        assert_eq!(response.response_result.ok(), Some(json!(shown)));
        client.shutdown();
    }
}

/// `org-tag-faces`, with the faces of keywords and priorities: announced
/// in the initialize result and on request, a color as the foreground.
#[test]
fn announces_the_faces_of_tags() {
    let settings = json!({ "org_tag_faces": { "work": "orange", "home": ":weight bold" } });
    let mut client = Client::start(json!({}), settings);
    let faces = client.initialize.capabilities.experimental.clone().unwrap()["orgFaces"].clone();
    let expected = json!({ "work": { "foreground": "orange" }, "home": { "bold": true } });
    assert_eq!(faces["tag"], expected);
    let response = client.raw_request("org-lsp/faces", Value::Null);
    assert_eq!(response.response_result.ok().map(|faces| faces["tag"].clone()), Some(expected));
    client.shutdown();
}

/// `org-inlinetask-insert-task` (`C-c C-x t`): an inline task of
/// `org_inlinetask_min_level` stars with its `END` line, in
/// `org_inlinetask_default_state` unless the command says without, as
/// Emacs writes it (`tests/inlinetask`). The minimum level is the
/// default: it holds for the process, and the other tests run beside.
#[test]
fn inserts_inline_tasks() {
    let insert = |command: &str, options: Value| -> String {
        let mut client = Client::start(json!({}), options);
        let text = "* A\nbody\n";
        client.open(&uri(), text);
        let context = json!({ "textDocument": { "uri": uri().as_str() }, "position": { "line": 1, "character": 0 } });
        let result = client
            .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .response_result
            .expect("a result");
        client.shutdown();
        let edits: Vec<TextEdit> = serde_json::from_value(result["edits"].clone()).unwrap();
        apply_text_edits(text, &edits)
    };
    let stars = "*".repeat(15);
    assert_eq!(insert("org.inlinetask.insert", Value::Null), format!("* A\n{stars} \n{stars} END\nbody\n"));
    let todo = json!({ "org_inlinetask_default_state": "TODO" });
    assert_eq!(insert("org.inlinetask.insert", todo.clone()), format!("* A\n{stars} TODO \n{stars} END\nbody\n"));
    assert_eq!(insert("org.inlinetask.insert.nostate", todo), format!("* A\n{stars} \n{stars} END\nbody\n"));
}

/// The BibTeX commands of `ol-bibtex.el`: a new entry with its questions
/// asked one at a time (the answers so far coming back in `replies`),
/// the entry at the cursor copied as BibTeX, the file's entries exported
/// to a `.bib` file, a BibTeX entry pasted and a file of them imported
/// as headings.
#[test]
fn bibtex_entries_as_headlines() {
    let directory = std::env::temp_dir().join(format!("org-lsp-bibtex-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes.org");
    let uri: Uri = format!("file://{}", file.display()).parse().unwrap();
    let mut client = Client::start(json!({}), Value::Null);
    // Each question answered in turn: the prompts asked, and the result.
    let run = |client: &mut Client, command: &str, text: &str, line: u32, answers: &[&str], extra: Value| {
        client.open(&uri, text);
        let mut context =
            json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": line, "character": 0 } });
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        let mut asked = Vec::new();
        let mut answers = answers.iter();
        loop {
            let result = client
                .raw_request("workspace/executeCommand", json!({ "command": command, "arguments": [context.clone()] }))
                .response_result
                .map_err(|error| error.message);
            let Ok(result) = result else { return (asked, Err(result.unwrap_err())) };
            let Some(input) = result.get("input").filter(|input| !input.is_null()) else {
                return (asked, Ok(result));
            };
            asked.push(input["prompt"].as_str().unwrap().to_string());
            for (key, value) in input["context"].as_object().into_iter().flatten() {
                context[key] = value.clone();
            }
            context["input"] = json!(answers.next().expect("an answer"));
        }
    };
    let text = "* a\n";
    let (asked, result) = run(&mut client, "org.bibtex.create", text, 1, &["misc", "Paper", "k1"], Value::Null);
    assert_eq!(asked, ["Type", "title", "id"]);
    let edits: Vec<TextEdit> = serde_json::from_value(result.unwrap()["edits"].clone()).unwrap();
    let created = apply_text_edits(text, &edits);
    assert_eq!(created, "* a\n* Paper\n:PROPERTIES:\n:TITLE:    Paper\n:BTYPE:    misc\n:CUSTOM_ID: k1\n:END:\n");
    // The type is completed from Org's.
    let context = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 0, "character": 0 } });
    let first = client
        .raw_request("workspace/executeCommand", json!({ "command": "org.bibtex.create", "arguments": [context] }))
        .response_result
        .unwrap();
    assert_eq!(first["input"]["completions"][0], "article");
    assert_eq!(first["input"]["select"], true);
    let (_, unknown) = run(&mut client, "org.bibtex.create", text, 1, &["paper"], Value::Null);
    assert_eq!(unknown.unwrap_err(), "Type::paper is not known");

    let (_, copied) = run(&mut client, "org.bibtex.copy", &created, 1, &[], Value::Null);
    assert_eq!(copied.unwrap()["copy"], "@misc{k1,\n  title={Paper}\n}\n");
    let (asked, exported) = run(&mut client, "org.bibtex.export", &created, 0, &["refs.bib"], Value::Null);
    assert_eq!(asked, ["BibTeX file"]);
    let target = directory.join("refs.bib");
    assert_eq!(exported.unwrap()["message"], format!("Successfully exported 1 BibTeX entries to {}", target.display()));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "@misc{k1,\n  title={Paper}\n}\n");

    let clipboard = "@article{doe,\n  author = {Jane Doe},\n  title = {On {T}hings},\n  year = 2001\n}\n";
    let (_, yanked) = run(&mut client, "org.bibtex.yank", text, 1, &[], json!({ "clipboard": clipboard }));
    let edits: Vec<TextEdit> = serde_json::from_value(yanked.unwrap()["edits"].clone()).unwrap();
    assert_eq!(
        apply_text_edits(text, &edits),
        "* a\n* On {T}hings\n:PROPERTIES:\n:TITLE:    On {T}hings\n:BTYPE:    article\n:CUSTOM_ID: doe\n\
         :AUTHOR:   Jane Doe\n:YEAR:     2001\n:END:\n"
    );
    let (asked, imported) = run(&mut client, "org.bibtex.import", text, 1, &["refs.bib"], Value::Null);
    assert_eq!(asked, ["File"]);
    let imported = imported.unwrap();
    assert_eq!(imported["message"], "Parsed 1 entries");
    let edits: Vec<TextEdit> = serde_json::from_value(imported["edits"].clone()).unwrap();
    assert_eq!(
        apply_text_edits(text, &edits),
        "* a\n* Paper\n:PROPERTIES:\n:TITLE:    Paper\n:BTYPE:    misc\n:CUSTOM_ID: k1\n:END:\n\n"
    );
    client.shutdown();
    std::fs::remove_dir_all(directory).unwrap();
}

/// What the server says as it starts (here a problem with the options)
/// waits for the client's `initialized`: nothing but the answer to
/// `initialize` comes before it.
#[test]
fn says_nothing_before_initialized() {
    private_state();
    let (server_side, connection) = Connection::memory();
    let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
    let params = json!({ "capabilities": {}, "initializationOptions": { "config_file": 5 } });
    let request = Request { id: RequestId::from(1), method: "initialize".to_string(), params };
    connection.sender.send(Message::Request(request)).unwrap();
    match connection.receiver.recv().unwrap() {
        Message::Response(response) => assert_eq!(response.id, RequestId::from(1)),
        other => panic!("not the answer to initialize: {other:?}"),
    }
    let early = connection.receiver.recv_timeout(std::time::Duration::from_millis(300));
    assert!(early.is_err(), "before initialized: {early:?}");
    let client = Client {
        connection,
        server: Some(server),
        next_id: 1,
        initialize: InitializeResult::default(),
        asked: Vec::new(),
    };
    client.notify::<Initialized>(lsp_types::InitializedParams {});
    let said = client.connection.receiver.recv_timeout(std::time::Duration::from_secs(10)).expect("a message");
    let Message::Notification(said) = said else { panic!("not a message: {said:?}") };
    assert_eq!(said.method, "window/showMessage");
    assert!(said.params["message"].as_str().unwrap().contains("`config_file` is malformed"), "{said:?}");
    client.shutdown();
}

/// A command that waits for Emacs holds up no other answer: a hover is
/// answered while Emacs works on `org-edit-special`, which is answered
/// once Emacs is done. `$/cancelRequest` gives up a command whose Emacs is
/// stuck: Emacs is stopped, and the next command starts another.
#[cfg(unix)]
#[test]
fn answers_other_requests_while_emacs_works() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!("org-lsp-working-emacs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    // An Emacs that takes a second over `edit-special` and answers with its
    // process id, and never answers `export-menu` (once it has it, there
    // is a file `stuck`).
    let emacs = directory.join("emacs");
    let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"export-menu"'*) touch "$(dirname "$0")/stuck" ;;
    *'"method":"edit-special"'*) sleep 1; printf '\036org-lsp {"id":%s,"error":"Emacs %s"}\n' "$id" "$$" ;;
    *) printf '\036org-lsp {"id":%s,"result":{}}\n' "$id" ;;
  esac
done
"#;
    std::fs::write(&emacs, script).unwrap();
    std::fs::set_permissions(&emacs, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut client = Client::start(json!({}), json!({ "emacs": emacs.display().to_string() }));
    let path = directory.join("notes.org");
    let uri: Uri = format!("file://{}", path.display()).parse().unwrap();
    client.open(&uri, "* Heading\n#+begin_src sh\necho hi\n#+end_src\n");
    let at = json!({ "textDocument": { "uri": uri.as_str() }, "position": { "line": 2, "character": 0 } });
    let send = |client: &mut Client, method: &str, params: Value| {
        client.next_id += 1;
        let id = RequestId::from(client.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        client.connection.sender.send(Message::Request(request)).unwrap();
        id
    };
    let command = |name: &str| json!({ "command": name, "arguments": [at.clone()] });
    // The answers in the order they come, and how long each took.
    let answers = |client: &mut Client, count: usize| {
        let began = std::time::Instant::now();
        let mut answered = Vec::new();
        while answered.len() < count {
            let message = client.connection.receiver.recv_timeout(std::time::Duration::from_secs(10));
            if let Message::Response(response) = message.expect("the server answers") {
                answered.push((response, began.elapsed()));
            }
        }
        answered
    };

    let special = send(&mut client, "workspace/executeCommand", command("org.edit.special"));
    let hover = send(&mut client, "textDocument/hover", at.clone());
    let answered = answers(&mut client, 2);
    assert_eq!(answered[0].0.id, hover);
    assert_eq!(answered[1].0.id, special);
    let first = answered[1].0.response_result.as_ref().expect_err("Emacs's error").message.clone();
    assert!(first.starts_with("Emacs "), "{first}");

    // Stuck: canceled, the command is answered so, and Emacs stopped.
    let export = send(&mut client, "workspace/executeCommand", command("org.export.dispatch"));
    let hover = send(&mut client, "textDocument/hover", at.clone());
    let answered = answers(&mut client, 1);
    assert_eq!(answered[0].0.id, hover);
    let began = std::time::Instant::now();
    while !directory.join("stuck").exists() {
        assert!(began.elapsed() < std::time::Duration::from_secs(10), "Emacs has the job");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let params = json!({ "id": export.clone() });
    client
        .connection
        .sender
        .send(Message::Notification(Notification { method: "$/cancelRequest".into(), params }))
        .unwrap();
    let answered = answers(&mut client, 1);
    assert_eq!(answered[0].0.id, export);
    assert_eq!(answered[0].0.response_result.as_ref().expect_err("an error").code, -32800);
    let special = send(&mut client, "workspace/executeCommand", command("org.edit.special"));
    let answered = answers(&mut client, 1);
    assert_eq!(answered[0].0.id, special);
    let second = answered[0].0.response_result.as_ref().expect_err("Emacs's error").message.clone();
    assert!(second.starts_with("Emacs ") && second != first, "another Emacs answers: {first}, {second}");
    client.shutdown();
    let _ = std::fs::remove_dir_all(directory);
}

/// An `org-id-locations-file` that is not there.
const NO_ID_LOCATIONS: &str = "/nonexistent/org-lsp-test/.org-id-locations";

/// The servers of the tests keep what they leave for others (where they
/// listen for org-protocol URLs) in a state directory of their own, not
/// the user's.
fn private_state() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let state = std::env::temp_dir().join(format!("org-lsp-test-state-{}", std::process::id()));
        std::env::set_var("XDG_STATE_HOME", state);
    });
}
