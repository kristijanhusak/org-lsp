//! The agenda over the protocol: a client opens it, moves through it and
//! acts on entries, and the server answers the way an editor would see it.

use std::path::PathBuf;
use std::thread::JoinHandle;

use expect_test::expect;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use serde_json::{json, Value};

const VIEW: &str = "org-agenda:/Org%20Agenda";

const WORK: &str = "\
#+CATEGORY: work
* TODO [#A] Write report :office:
SCHEDULED: <2026-05-18 Mon 09:00>
* TODO Call [[https://example.com][the client]]
DEADLINE: <2026-05-20 Wed>
* Meeting <2026-05-19 Tue 14:00-15:00>
";

const HOME: &str = "\
* TODO Water plants
SCHEDULED: <2026-05-16 Sat +1w>
* DONE Buy milk
";

/// A client that answers the server's requests and remembers them.
struct Client {
    connection: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
    /// Requests the server sent, oldest first.
    received: Vec<Request>,
    /// What to answer to `window/showMessageRequest`.
    choice: Option<String>,
    initialize: Value,
}

impl Client {
    fn start(capabilities: Value, options: Value, root: Option<&str>) -> Client {
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut client = Client {
            connection: client_side,
            server: Some(server),
            next_id: 0,
            received: Vec::new(),
            choice: None,
            initialize: Value::Null,
        };
        let params = json!({ "capabilities": capabilities, "initializationOptions": options, "rootUri": root });
        client.initialize = client.request("initialize", params).expect("initialize result");
        client.notify("initialized", json!({}));
        client
    }

    /// An editor that fetches the agenda from the server.
    fn editor(options: Value, root: Option<&str>) -> Client {
        let capabilities = json!({
            "workspace": { "textDocumentContent": { "dynamicRegistration": true }, "workspaceEdit": { "documentChanges": true } },
            "window": { "showDocument": { "support": true } },
            "general": { "positionEncodings": ["utf-8"] }
        });
        Client::start(capabilities, options, root)
    }

    fn answer(&mut self, request: Request) {
        let result = match request.method.as_str() {
            "workspace/applyEdit" => json!({ "applied": true }),
            "window/showDocument" => json!({ "success": true }),
            "window/showMessageRequest" => match &self.choice {
                Some(title) => json!({ "title": title }),
                None => Value::Null,
            },
            _ => Value::Null,
        };
        self.connection.sender.send(Message::Response(Response::new_ok(request.id.clone(), result))).unwrap();
        self.received.push(request);
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        self.connection.sender.send(Message::Request(request)).unwrap();
        loop {
            match self.connection.receiver.recv().expect("server answers") {
                Message::Response(response) if response.id == id => {
                    return response.response_result.map_err(|error| error.message)
                }
                Message::Request(request) => self.answer(request),
                _ => {}
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification { method: method.to_string(), params };
        self.connection.sender.send(Message::Notification(notification)).unwrap();
    }

    /// Wait until the server has handled everything sent so far.
    fn settle(&mut self) {
        let _ = self.request("org-lsp/test-sync", Value::Null);
        let _ = self.request("org-lsp/test-sync", Value::Null);
    }

    /// Run a command with the cursor at `line` of `uri`, and return the
    /// requests the server made while it worked.
    fn command(&mut self, command: &str, uri: &str, line: u32) -> Vec<Request> {
        self.received.clear();
        let context = json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 0 } });
        self.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .unwrap_or_else(|error| panic!("{command} failed: {error}"));
        self.settle();
        std::mem::take(&mut self.received)
    }

    fn open(&self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "org", "version": 1, "text": text } }),
        );
    }

    fn view_text(&mut self, uri: &str) -> String {
        let result = self.request("workspace/textDocumentContent", json!({ "uri": uri })).unwrap();
        result["text"].as_str().expect("the view has text").to_string()
    }

    fn shutdown(mut self) {
        self.request("shutdown", Value::Null).unwrap();
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().expect("server exits cleanly");
    }
}

fn clock() -> Value {
    json!({ "clock": { "today": "2026-05-18", "now": "10:30" } })
}

fn methods(requests: &[Request]) -> Vec<&str> {
    requests.iter().map(|request| request.method.as_str()).collect()
}

/// The single text edit of a `workspace/applyEdit` request, with its file.
fn edit_of(request: &Request) -> (String, Value) {
    assert_eq!(request.method, "workspace/applyEdit");
    let change = &request.params["edit"]["documentChanges"][0];
    (change["textDocument"]["uri"].as_str().unwrap().to_string(), change["edits"][0].clone())
}

/// Apply an edit that stays on one line to `text`.
fn apply(text: &str, edit: &Value) -> String {
    let (start, end) = (&edit["range"]["start"], &edit["range"]["end"]);
    assert_eq!(start["line"], end["line"]);
    let line = start["line"].as_u64().unwrap() as usize;
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let (from, to) = (start["character"].as_u64().unwrap() as usize, end["character"].as_u64().unwrap() as usize);
    lines[line].replace_range(from..to, edit["newText"].as_str().unwrap());
    lines.join("\n")
}

#[test]
fn announces_the_agenda() {
    let client = Client::editor(json!({}), None);
    let capabilities = &client.initialize["capabilities"];
    assert_eq!(capabilities["workspace"]["textDocumentContent"]["schemes"], json!(["org-agenda"]));
    assert_eq!(capabilities["definitionProvider"], json!(true));
    let commands: Vec<&str> = capabilities["executeCommandProvider"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert!(commands.contains(&"org.agenda.list") && commands.contains(&"org.todo.next"));
    // The command table, for editors to build their key bindings from.
    let manifest = capabilities["experimental"]["org"]["commands"].as_array().unwrap();
    assert_eq!(manifest.len(), commands.len());
    let later = manifest.iter().find(|command| command["command"] == "org.agenda.later").unwrap();
    assert_eq!(later["keys"], json!([{ "key": "f", "in": "agenda" }]));
    client.shutdown();
}

#[test]
fn opens_the_agenda_from_open_documents() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.open("file:///org/home.org", HOME);

    let requests = client.command("org.agenda.list", "file:///org/work.org", 0);
    assert_eq!(methods(&requests), ["workspace/textDocumentContent/refresh", "window/showDocument"]);
    let show = &requests[1].params;
    assert_eq!(show["uri"], VIEW);
    assert_eq!(show["takeFocus"], true);
    // The cursor goes to today's date.
    assert_eq!(show["selection"]["start"], json!({ "line": 1, "character": 0 }));

    expect![[r#"
        Week-agenda (W21):
        Monday     18 May 2026 W21
                       8:00...... ----------------
          work:        9:00...... Scheduled:  TODO [#A] Write report            :office:
                      10:00...... ----------------
                      10:30...... now - - - - - - - - - - - - - - - - - - - - - - - - -
                      12:00...... ----------------
                      14:00...... ----------------
                      16:00...... ----------------
                      18:00...... ----------------
                      20:00...... ----------------
          home:       Sched. 2x:  TODO Water plants
          work:       In   2 d.:  TODO Call the client
        Tuesday    19 May 2026
          work:       14:00-15:00 Meeting 
        Wednesday  20 May 2026
          work:       Deadline:   TODO Call the client
        Thursday   21 May 2026
        Friday     22 May 2026
        Saturday   23 May 2026
          home:       Scheduled:  TODO Water plants
        Sunday     24 May 2026
    "#]]
    .assert_eq(&client.view_text(VIEW));

    // Highlighting: the header, then the date with `today`.
    let tokens =
        client.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": VIEW } })).unwrap();
    let data: Vec<u64> = tokens["data"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).collect();
    let legend = &client.initialize["capabilities"]["semanticTokensProvider"]["legend"];
    let type_name = |index: u64| legend["tokenTypes"][index as usize].as_str().unwrap().to_string();
    let modifier_names = |bits: u64| -> Vec<String> {
        let names = legend["tokenModifiers"].as_array().unwrap();
        (0..names.len())
            .filter(|bit| bits & (1 << bit) != 0)
            .map(|bit| names[bit].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!((data[0], data[1], data[2], type_name(data[3])), (0, 0, 18, "agendaStructure".to_string()));
    assert_eq!(
        (data[5], data[6], type_name(data[8]), modifier_names(data[9])),
        (1, 0, "agendaDate".to_string(), vec!["today".to_string()])
    );

    // From a line to its entry.
    let definition = client
        .request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": VIEW }, "position": { "line": 12, "character": 20 } }),
        )
        .unwrap();
    assert_eq!(definition["uri"], "file:///org/work.org");
    assert_eq!(definition["range"]["start"], json!({ "line": 3, "character": 0 }));
    let nothing = client
        .request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } }),
        )
        .unwrap();
    assert_eq!(nothing, Value::Null);

    // The actions on an entry line are commands with the line as context.
    let actions = client
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": VIEW },
                "range": { "start": { "line": 12, "character": 0 }, "end": { "line": 12, "character": 0 } },
                "context": { "diagnostics": [] }
            }),
        )
        .unwrap();
    let kinds: Vec<&str> = actions.as_array().unwrap().iter().map(|action| action["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["org.todo.next", "org.todo.previous", "org.agenda.goto"]);
    assert_eq!(actions[0]["command"]["arguments"][0]["position"]["line"], 12);
    client.shutdown();
}

#[test]
fn moves_through_time_and_changes_span() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);

    let requests = client.command("org.agenda.later", VIEW, 0);
    assert_eq!(methods(&requests), ["workspace/textDocumentContent/refresh", "window/showDocument"]);
    assert!(client.view_text(VIEW).starts_with("Week-agenda (W22):\nMonday     25 May 2026 W22\n"));
    client.command("org.agenda.earlier", VIEW, 0);
    client.command("org.agenda.earlier", VIEW, 0);
    assert!(client.view_text(VIEW).starts_with("Week-agenda (W20):\nMonday     11 May 2026 W20\n"));
    client.command("org.agenda.today", VIEW, 0);
    assert!(client.view_text(VIEW).starts_with("Week-agenda (W21):\n"));

    // The day view starts from the day under the cursor: line 15 is Wednesday.
    let lines: Vec<String> = client.view_text(VIEW).lines().map(str::to_string).collect();
    let wednesday = lines.iter().position(|line| line.starts_with("Wednesday")).unwrap() as u32;
    client.command("org.agenda.day", VIEW, wednesday + 1);
    assert_eq!(
        client.view_text(VIEW),
        "Day-agenda (W21):\nWednesday  20 May 2026\n  work:       Deadline:   TODO Call the client\n"
    );
    client.command("org.agenda.month", VIEW, 1);
    assert!(client.view_text(VIEW).starts_with("Month-agenda (W18-W22):\nFriday      1 May 2026\n"));
    client.command("org.agenda.later", VIEW, 0);
    assert!(client.view_text(VIEW).starts_with("Month-agenda (W23-W27):\nMonday      1 June 2026 W23\n"));

    client.command("org.agenda.todos", VIEW, 0);
    expect![[r#"
        Global list of TODO items of type: ALL
          work:       TODO [#A] Write report                                    :office:
          work:       TODO Call the client
    "#]]
    .assert_eq(&client.view_text(VIEW));
    let error = client.request("workspace/executeCommand", json!({ "command": "org.agenda.later" })).unwrap_err();
    assert_eq!(error, "The TODO list has no dates to move through");
    client.shutdown();
}

#[test]
fn changes_an_entry_from_the_agenda() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.todos", VIEW, 0);

    // Line 2 is "TODO Call the client".
    let requests = client.command("org.todo.next", VIEW, 2);
    assert_eq!(methods(&requests), ["workspace/applyEdit"]);
    let (uri, edit) = edit_of(&requests[0]);
    assert_eq!(uri, "file:///org/work.org");
    assert_eq!(requests[0].params["edit"]["documentChanges"][0]["textDocument"]["version"], 1);
    assert_eq!(
        edit,
        json!({ "range": { "start": { "line": 3, "character": 2 }, "end": { "line": 3, "character": 6 } }, "newText": "DONE" })
    );

    // The editor applies the edit; the agenda follows.
    client.received.clear();
    let changed = apply(WORK, &edit);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": changed }] }),
    );
    client.settle();
    assert_eq!(methods(&client.received), ["workspace/textDocumentContent/refresh"]);
    expect![[r#"
        Global list of TODO items of type: ALL
          work:       TODO [#A] Write report                                    :office:
    "#]]
    .assert_eq(&client.view_text(VIEW));

    // The same command in the Org file itself.
    let requests = client.command("org.todo.previous", "file:///org/work.org", 1);
    let (_, edit) = edit_of(&requests[0]);
    assert_eq!(edit["newText"], "");
    assert_eq!(edit["range"]["start"], json!({ "line": 1, "character": 2 }));

    // Go to the entry of a line.
    let requests = client.command("org.agenda.goto", VIEW, 1);
    assert_eq!(methods(&requests), ["window/showDocument"]);
    assert_eq!(requests[0].params["uri"], "file:///org/work.org");
    assert_eq!(requests[0].params["selection"]["start"], json!({ "line": 1, "character": 0 }));

    let error = client
        .request(
            "workspace/executeCommand",
            json!({ "command": "org.agenda.goto", "arguments": [{ "textDocument": { "uri": VIEW }, "position": { "line": 0, "character": 0 } }] }),
        )
        .unwrap_err();
    assert_eq!(error, "No entry on this line");
    client.shutdown();
}

#[test]
fn the_dispatcher_asks_which_view() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.choice = Some("List of all TODO entries".to_string());
    let requests = client.command("org.agenda.dispatch", "file:///org/work.org", 0);
    assert_eq!(
        methods(&requests),
        ["window/showMessageRequest", "workspace/textDocumentContent/refresh", "window/showDocument"]
    );
    let titles: Vec<&str> = requests[0].params["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Agenda for the current week or day", "List of all TODO entries"]);
    assert!(client.view_text(VIEW).starts_with("Global list of TODO items"));

    // Dismissing the menu does nothing.
    client.choice = None;
    let requests = client.command("org.agenda.dispatch", "file:///org/work.org", 0);
    assert_eq!(methods(&requests), ["window/showMessageRequest"]);
    client.shutdown();
}

fn workspace(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("org-lsp-agenda-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("work.org"), WORK).unwrap();
    std::fs::write(root.join("sub/home.org"), HOME).unwrap();
    std::fs::write(root.join("notes.txt"), "* TODO not an Org file\n").unwrap();
    root
}

#[test]
fn reads_agenda_files_from_the_workspace() {
    let root = workspace("disk");
    let root_uri = format!("file://{}", root.display());
    let mut client = Client::editor(clock(), Some(&root_uri));
    client.command("org.agenda.todos", VIEW, 0);
    let listed = client.view_text(VIEW);
    expect![[r#"
        Global list of TODO items of type: ALL
          work:       TODO [#A] Write report                                    :office:
          home:       TODO Water plants
          work:       TODO Call the client
    "#]]
    .assert_eq(&listed);

    // An edit to a file that is not open carries no version.
    let requests = client.command("org.todo.next", VIEW, 2);
    let (uri, edit) = edit_of(&requests[0]);
    assert_eq!(uri, format!("{root_uri}/sub/home.org"));
    assert_eq!(requests[0].params["edit"]["documentChanges"][0]["textDocument"]["version"], Value::Null);

    // The editor opens the file to apply it. What it holds now counts,
    // not what is on disk.
    client.open(&uri, &apply(HOME, &edit));
    client.command("org.agenda.redo", VIEW, 0);
    assert!(!client.view_text(VIEW).contains("Water plants"));
    client.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
    client.command("org.agenda.redo", VIEW, 0);
    assert_eq!(client.view_text(VIEW), listed);

    // A change on disk shows after the next rebuild.
    std::fs::write(root.join("work.org"), "* TODO Only this\n").unwrap();
    client.command("org.agenda.redo", VIEW, 0);
    assert_eq!(
        client.view_text(VIEW),
        "Global list of TODO items of type: ALL\n  home:       TODO Water plants\n  work:       TODO Only this\n"
    );
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn agenda_files_option_selects_files() {
    let root = workspace("option");
    let options = json!({ "clock": { "today": "2026-05-18" }, "agendaFiles": [format!("{}/sub", root.display())], "agenda": { "span": "day" } });
    let mut client = Client::editor(options, None);
    client.command("org.agenda.list", VIEW, 0);
    assert_eq!(
        client.view_text(VIEW),
        "Day-agenda (W21):\nMonday     18 May 2026 W21\n  home:       Sched. 2x:  TODO Water plants\n"
    );
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn writes_a_file_for_clients_without_virtual_documents() {
    let capabilities = json!({ "window": { "showDocument": { "support": true } } });
    let mut client = Client::start(capabilities, clock(), None);
    client.open("file:///org/work.org", WORK);
    let requests = client.command("org.agenda.todos", "file:///org/work.org", 0);
    assert_eq!(methods(&requests), ["window/showDocument"]);
    let uri = requests[0].params["uri"].as_str().unwrap().to_string();
    assert!(uri.starts_with("file://") && uri.ends_with("/Agenda.org-agenda"), "{uri}");
    let path = PathBuf::from(uri.strip_prefix("file://").unwrap());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("Global list of TODO items"));

    // The editor opens that file; requests on it reach the agenda.
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "org-agenda", "version": 1, "text": written } }),
    );
    let definition = client
        .request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 0 } }),
        )
        .unwrap();
    assert_eq!(definition["uri"], "file:///org/work.org");
    let tokens = client.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": uri } })).unwrap();
    assert!(!tokens["data"].as_array().unwrap().is_empty());

    // A rebuild rewrites the file.
    client.command("org.agenda.list", &uri, 0);
    assert!(std::fs::read_to_string(&path).unwrap().starts_with("Week-agenda (W21):"));
    client.shutdown();
    assert!(!path.exists(), "the file is removed when the server stops");
}

#[test]
fn needs_a_client_that_can_show_documents() {
    let mut client = Client::start(json!({}), clock(), None);
    let error = client.request("workspace/executeCommand", json!({ "command": "org.agenda.list" })).unwrap_err();
    assert_eq!(error, "This editor cannot open documents on the server's request");
    let error = client.request("workspace/executeCommand", json!({ "command": "org.unknown" })).unwrap_err();
    assert_eq!(error, "org-lsp has no command `org.unknown`");
    client.shutdown();
}
