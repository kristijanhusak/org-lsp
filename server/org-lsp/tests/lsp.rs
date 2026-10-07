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
        // The tests' own options only, whatever configuration file the
        // machine has.
        let mut options = if options.is_null() { json!({}) } else { options };
        if let Some(fields) = options.as_object_mut() {
            fields.entry("config_file").or_insert(json!(false));
        }
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
            "org.table.recalculate"
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
    assert_eq!(changed, ["2. b\n3"]);
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
    assert_eq!(changed, ["1/2]\n- [X"]);
    // Nothing to toggle on text.
    client.open(&uri(), "* a\ntext\n");
    assert!(client.code_actions(&uri(), Position::new(1, 0), "org.checkbox.toggle").is_empty());
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
    let diagnostics = published["diagnostics"].as_array().unwrap().clone();
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
