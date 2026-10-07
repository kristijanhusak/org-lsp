//! The agenda over the protocol: a client opens it, moves through it and
//! acts on entries, and the server answers the way an editor would see it.

use std::path::PathBuf;
use std::thread::JoinHandle;

use expect_test::expect;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response, ResponseError};
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
    /// The version of the agenda document, as the editor counts it.
    view_version: i32,
    /// The version of the Org file that `edit` changes.
    file_version: i32,
    /// Leave requests to show a document unanswered, like an editor that
    /// has not got round to them yet. They wait in `held`.
    hold_show: bool,
    held: Vec<Request>,
    initialize: Value,
    /// Notifications the server sent, oldest first.
    notifications: Vec<Notification>,
}

impl Client {
    fn start(capabilities: Value, options: Value, root: Option<&str>) -> Client {
        // The tests' own options only, whatever configuration file the
        // machine has.
        let mut options = if options.is_null() { json!({}) } else { options };
        if let Some(fields) = options.as_object_mut() {
            fields.entry("configFile").or_insert(json!(false));
        }
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut client = Client {
            connection: client_side,
            server: Some(server),
            next_id: 0,
            received: Vec::new(),
            choice: None,
            view_version: 0,
            file_version: 1,
            hold_show: false,
            held: Vec::new(),
            initialize: Value::Null,
            notifications: Vec::new(),
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

    /// An editor that also shows Org's date prompt.
    fn with_date_prompt(options: Value) -> Client {
        let capabilities = json!({
            "workspace": {
                "textDocumentContent": { "dynamicRegistration": true },
                "workspaceEdit": { "documentChanges": true },
                "semanticTokens": { "refreshSupport": true }
            },
            "window": { "showDocument": { "support": true } },
            "general": { "positionEncodings": ["utf-8"] },
            "experimental": { "orgDatePrompt": true }
        });
        Client::start(capabilities, options, None)
    }

    fn answer(&mut self, request: Request) {
        if self.hold_show && request.method == "window/showDocument" {
            self.held.push(request);
            return;
        }
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
        self.request_raw(method, params).map_err(|error| error.message)
    }

    /// Like `request`, with the whole error: its message and its data.
    fn request_raw(&mut self, method: &str, params: Value) -> Result<Value, ResponseError> {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        self.connection.sender.send(Message::Request(request)).unwrap();
        loop {
            match self.connection.receiver.recv().expect("server answers") {
                Message::Response(response) if response.id == id => return response.response_result,
                Message::Request(request) => self.answer(request),
                Message::Notification(notification) => self.notifications.push(notification),
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

    /// Run an editing command at `position` (line and character) of `uri`
    /// and apply the edit it asks for to `text`, as an editor would: the
    /// document changes, and the server hears about it.
    fn edit(
        &mut self,
        command: &str,
        uri: &str,
        position: (u32, u32),
        extra: Value,
        text: &mut String,
    ) -> Vec<Request> {
        self.received.clear();
        let mut context =
            json!({ "textDocument": { "uri": uri }, "position": { "line": position.0, "character": position.1 } });
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        self.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
            .unwrap_or_else(|error| panic!("{command} failed: {error}"));
        self.settle();
        let mut requests = std::mem::take(&mut self.received);
        if let Some(request) = requests.iter().find(|request| request.method == "workspace/applyEdit") {
            let (file, edits) = edits_of(request);
            *text = apply(text, &edits);
            self.file_version += 1;
            self.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": file, "version": self.file_version }, "contentChanges": [{ "text": text }] }),
            );
            self.settle();
            requests.append(&mut self.received);
        }
        requests
    }

    fn open(&self, uri: &str, text: &str) {
        self.open_as(uri, "org", text);
    }

    fn open_as(&self, uri: &str, language: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": language, "version": 1, "text": text } }),
        );
    }

    /// Open the agenda the way an editor does that was asked to show it:
    /// fetch its text and say that the document is open.
    fn show_view(&mut self) -> String {
        let text = self.view_text(VIEW);
        self.view_version = 1;
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": VIEW, "languageId": "org-agenda", "version": 1, "text": text } }),
        );
        text
    }

    /// Fetch the agenda again and report the new text, as an editor does
    /// when it is told that the view changed.
    fn refetch_view(&mut self) -> String {
        let text = self.view_text(VIEW);
        self.view_version += 1;
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": VIEW, "version": self.view_version }, "contentChanges": [{ "text": text }] }),
        );
        text
    }

    /// Run a command on the open agenda and let the editor follow. Returns
    /// the requests the server made, before and after the editor fetched
    /// the new text, and that text.
    fn view_command(&mut self, command: &str, line: u32) -> (Vec<Request>, String) {
        let mut requests = self.command(command, VIEW, line);
        let text = self.refetch_view();
        self.settle();
        requests.append(&mut self.received);
        (requests, text)
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

/// The file and the text edits of a `workspace/applyEdit` request.
fn edits_of(request: &Request) -> (String, Vec<Value>) {
    assert_eq!(request.method, "workspace/applyEdit");
    let change = &request.params["edit"]["documentChanges"][0];
    (change["textDocument"]["uri"].as_str().unwrap().to_string(), change["edits"].as_array().unwrap().clone())
}

/// Apply edits to `text`. Positions count bytes, as the test client asks.
fn apply(text: &str, edits: &[Value]) -> String {
    let offset = |position: &Value| {
        let line = position["line"].as_u64().unwrap() as usize;
        let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        start + position["character"].as_u64().unwrap() as usize
    };
    let mut out = text.to_string();
    for edit in edits.iter().rev() {
        let (start, end) = (offset(&edit["range"]["start"]), offset(&edit["range"]["end"]));
        out.replace_range(start..end, edit["newText"].as_str().unwrap());
    }
    out
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
    // The manifest: what editors build their keys and highlighting from.
    let manifest = &capabilities["experimental"]["org"];
    let table = manifest["commands"].as_array().unwrap();
    // Every command but the one editors call on their own while a date is
    // typed.
    assert_eq!(table.len() + 1, commands.len());
    assert!(commands.contains(&"org.date.preview"));
    let find = |id: &str| table.iter().find(|command| command["command"] == id).unwrap();
    assert_eq!(
        find("org.agenda.goto")["keys"],
        json!([
            { "key": "RET", "in": "agenda", "preset": "emacs" },
            { "key": "<CR>", "in": "agenda", "preset": "vim" }
        ])
    );
    assert_eq!(
        find("org.agenda.goto.other")["keys"],
        json!([
            { "key": "TAB", "in": "agenda", "preset": "emacs" },
            { "key": "<Tab>", "in": "agenda", "preset": "vim" }
        ])
    );
    assert_eq!(find("org.todo.next")["edits"], true);
    assert_eq!(find("org.agenda.later")["edits"], false);
    assert_eq!(find("org.schedule")["input"], json!({ "kind": "date", "prompt": "Schedule" }));
    assert!(find("org.todo.next").get("input").is_none());
    assert_eq!(
        manifest["agenda"],
        json!({ "scheme": "org-agenda", "uri": VIEW, "fileName": "Agenda.org-agenda", "languageId": "org-agenda" })
    );
    // Every token of the legend is described, and the looks refer to it.
    let legend = &capabilities["semanticTokensProvider"]["legend"];
    let ids = |list: &Value| -> Vec<Value> { list.as_array().unwrap().iter().map(|item| item["id"].clone()).collect() };
    assert_eq!(Value::Array(ids(&manifest["tokenTypes"])), legend["tokenTypes"]);
    assert_eq!(Value::Array(ids(&manifest["tokenModifiers"])), legend["tokenModifiers"]);
    let today = manifest["highlights"].as_array().unwrap().iter().find(|look| look["token"] == "agendaDate.today");
    assert_eq!(today.unwrap()["vim"], json!(["@org.agenda.today", "@markup.strong"]));
    client.shutdown();
}

#[test]
fn closing_the_agenda_stops_rebuilding_it() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/home.org", HOME);
    client.command("org.agenda.list", "file:///org/home.org", 0);
    client.notify("textDocument/didClose", json!({ "textDocument": { "uri": VIEW } }));
    client.settle();
    // A change to a file no longer makes the server publish the view.
    client.received.clear();
    client.notify("textDocument/didSave", json!({ "textDocument": { "uri": "file:///org/home.org" } }));
    client.settle();
    assert_eq!(methods(&client.received), [] as [&str; 0]);
    let error = client
        .request("workspace/executeCommand", json!({ "command": "org.agenda.later", "arguments": [] }))
        .unwrap_err();
    assert_eq!(error, "No agenda is open");
    // Fetching it again builds a fresh one.
    assert!(client.view_text(VIEW).starts_with("Week-agenda (W21):"));
    client.shutdown();
}

#[test]
fn opens_the_agenda_from_open_documents() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.open("file:///org/home.org", HOME);

    // The editor does not have the agenda yet: showing it makes it fetch the text.
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
    assert_eq!(
        kinds,
        [
            "org.todo.next",
            "org.todo.previous",
            "org.priority.up",
            "org.priority.down",
            "org.date.later",
            "org.date.earlier",
            "org.clock.in",
            "org.archive.subtree",
            "org.agenda.goto",
            "org.agenda.goto.other"
        ]
    );
    assert_eq!(actions[0]["command"]["arguments"][0]["position"]["line"], 12);
    client.shutdown();
}

#[test]
fn moves_through_time_and_changes_span() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    client.show_view();

    // The editor has the agenda open now. It is told that the text changed.
    // The cursor is moved only once the editor reports the new text: the
    // line is one of that text, not of the old one.
    let requests = client.command("org.agenda.later", VIEW, 0);
    assert_eq!(methods(&requests), ["workspace/textDocumentContent/refresh"]);
    client.received.clear();
    // Fetching the text is not yet showing it.
    assert!(client.view_text(VIEW).starts_with("Week-agenda (W22):\nMonday     25 May 2026 W22\n"));
    client.settle();
    assert_eq!(methods(&client.received), [] as [&str; 0]);
    client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), ["window/showDocument"]);
    assert_eq!(client.received[0].params["selection"]["start"], json!({ "line": 1, "character": 0 }));
    // Further changes move nothing.
    client.received.clear();
    client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), [] as [&str; 0]);

    client.view_command("org.agenda.earlier", 0);
    let (_, text) = client.view_command("org.agenda.earlier", 0);
    assert!(text.starts_with("Week-agenda (W20):\nMonday     11 May 2026 W20\n"));
    let (requests, text) = client.view_command("org.agenda.today", 0);
    assert!(text.starts_with("Week-agenda (W21):\n"));
    // Today is the second line again.
    assert_eq!(methods(&requests), ["workspace/textDocumentContent/refresh", "window/showDocument"]);
    assert_eq!(requests[1].params["selection"]["start"]["line"], 1);
    // Asked again, the text stays as it is: there is nothing to fetch, and
    // the cursor goes to today at once.
    let requests = client.command("org.agenda.today", VIEW, 5);
    assert_eq!(methods(&requests), ["window/showDocument"]);

    // `org-agenda-later` and `-earlier` keep the cursor on the same day
    // among the days shown (`org-day-cnt`): from Wednesday, or a line under
    // it, to the Wednesday of the next week and back; from the title, where
    // there is no day, to today or else the first day.
    let at_cursor = |requests: &[Request], text: &str| {
        let shown = requests.iter().find(|request| request.method == "window/showDocument").unwrap();
        text.lines().nth(shown.params["selection"]["start"]["line"].as_u64().unwrap() as usize).unwrap().to_string()
    };
    let wednesday = text.lines().position(|line| line.starts_with("Wednesday")).unwrap() as u32;
    assert!(text.lines().nth(wednesday as usize + 1).unwrap().starts_with("  work:"));
    let (requests, later) = client.view_command("org.agenda.later", wednesday + 1);
    assert_eq!(at_cursor(&requests, &later), "Wednesday  27 May 2026");
    let wednesday = later.lines().position(|line| line.starts_with("Wednesday")).unwrap() as u32;
    let (requests, earlier) = client.view_command("org.agenda.earlier", wednesday);
    assert_eq!(at_cursor(&requests, &earlier), "Wednesday  20 May 2026");
    let (requests, later) = client.view_command("org.agenda.later", 0);
    assert_eq!(at_cursor(&requests, &later), "Monday     25 May 2026 W22");
    let (requests, earlier) = client.view_command("org.agenda.earlier", 0);
    assert_eq!(at_cursor(&requests, &earlier), "Monday     18 May 2026 W21");

    // The day view starts from the day under the cursor.
    let lines: Vec<&str> = text.lines().collect();
    let wednesday = lines.iter().position(|line| line.starts_with("Wednesday")).unwrap() as u32;
    let (_, text) = client.view_command("org.agenda.day", wednesday + 1);
    assert_eq!(text, "Day-agenda (W21):\nWednesday  20 May 2026\n  work:       Deadline:   TODO Call the client\n");
    let (requests, text) = client.view_command("org.agenda.month", 1);
    assert!(text.starts_with("Month-agenda (W18-W22):\nFriday      1 May 2026\n"));
    // The cursor stays on the day it was on, far below the one line the
    // day view had.
    assert_eq!(
        text.lines().nth(requests[1].params["selection"]["start"]["line"].as_u64().unwrap() as usize).unwrap(),
        "Wednesday  20 May 2026"
    );
    let (_, text) = client.view_command("org.agenda.later", 0);
    assert!(text.starts_with("Month-agenda (W23-W27):\nMonday      1 June 2026 W23\n"));

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
    let (uri, edits) = edits_of(&requests[0]);
    assert_eq!(uri, "file:///org/work.org");
    assert_eq!(requests[0].params["edit"]["documentChanges"][0]["textDocument"]["version"], 1);
    // Only the keyword changes, so the cursor in the file stays put.
    assert_eq!(
        edits,
        [
            json!({ "range": { "start": { "line": 3, "character": 2 }, "end": { "line": 3, "character": 6 } }, "newText": "DONE" })
        ]
    );

    // The editor applies the edit; the agenda follows.
    client.received.clear();
    let changed = apply(WORK, &edits);
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
    // Org also puts the tags back in their column.
    let requests = client.command("org.todo.previous", "file:///org/work.org", 1);
    let (_, edits) = edits_of(&requests[0]);
    let headline = apply(&changed, &edits).lines().nth(1).unwrap().to_string();
    assert_eq!(headline, format!("* [#A] Write report{}:office:", " ".repeat(50)));

    // Go to the entry of a line.
    let requests = client.command("org.agenda.goto", VIEW, 1);
    assert_eq!(methods(&requests), ["window/showDocument"]);
    assert_eq!(requests[0].params["uri"], "file:///org/work.org");
    assert_eq!(requests[0].params["selection"]["start"], json!({ "line": 1, "character": 0 }));
    assert!(requests[0].params.get("orgOtherWindow").is_none());
    // TAB: in another window, which the editors of this project read.
    let requests = client.command("org.agenda.goto.other", VIEW, 1);
    assert_eq!(requests[0].params["uri"], "file:///org/work.org");
    assert_eq!(requests[0].params["orgOtherWindow"], true);

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
    // Emacs's menu: a view by its key.
    let at = json!({ "textDocument": { "uri": "file:///org/work.org" }, "position": { "line": 0, "character": 0 } });
    let run = |client: &mut Client, extra: Value| {
        let mut context = at.clone();
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        client.request("workspace/executeCommand", json!({ "command": "org.agenda.dispatch", "arguments": [context] }))
    };
    let asked = run(&mut client, json!({})).unwrap();
    assert_eq!(asked["input"]["kind"], "key");
    assert_eq!(asked["input"]["prompt"], "Press key for agenda command");
    assert_eq!(
        asked["input"]["choices"],
        json!([
            { "key": "a", "label": "Agenda for the current week or day" },
            { "key": "t", "label": "List of all TODO entries" },
            { "key": "m", "label": "Match a TAGS/PROP/TODO query" },
            { "key": "M", "label": "Like m, but only TODO entries" }
        ])
    );
    client.received.clear();
    assert_eq!(run(&mut client, json!({ "input": "t" })), Ok(Value::Null));
    client.settle();
    assert!(client.view_text(VIEW).starts_with("Global list of TODO items"));
    assert_eq!(run(&mut client, json!({ "input": "x" })), Err("No agenda command for this key".to_string()));
    // Asked with no document, as from a key with no file open.
    let answer = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.agenda.dispatch", "arguments": [{ "input": "a" }] }),
    );
    assert_eq!(answer, Ok(Value::Null));
    client.shutdown();
}

/// `org-agenda-custom-commands`: a group of commands under a prefix key,
/// a block agenda whose agenda block moves on its own (`f`), and the
/// built-in tags search (`m`).
#[test]
fn runs_custom_agenda_commands() {
    let mut options = clock();
    options["agenda"] = json!({ "customCommands": [
        { "key": "x", "description": "Mine" },
        { "key": "xb", "description": "Day and office", "blocks": [
            { "type": "agenda", "settings": { "span": "day" } },
            { "type": "tags", "match": "office", "header": "At the office" }
        ] }
    ] });
    let mut client = Client::editor(options, None);
    client.open("file:///org/work.org", WORK);
    let run = |client: &mut Client, command: &str, extra: Value| {
        let mut context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 0, "character": 0 } });
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
        client.settle();
        result
    };
    let asked = run(&mut client, "org.agenda.dispatch", json!({})).unwrap();
    let keys: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["a", "t", "m", "M", "x"]);
    assert_eq!(asked["input"]["choices"][4]["label"], "+Mine");
    let asked = run(&mut client, "org.agenda.dispatch", json!({ "input": "x" })).unwrap();
    assert_eq!(asked["input"]["choices"], json!([{ "key": "b", "label": "Day and office" }]));
    assert_eq!(asked["input"]["context"], json!({ "keys": "x" }));
    assert_eq!(run(&mut client, "org.agenda.dispatch", json!({ "keys": "x", "input": "b" })), Ok(Value::Null));
    let text = client.view_text(VIEW);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Day-agenda (W21):");
    assert_eq!(lines[1], "Monday     18 May 2026 W21");
    let separator = lines.iter().position(|line| line.starts_with("====")).unwrap();
    // No blank line above the separator, unlike Emacs.
    assert_ne!(lines[separator - 1], "");
    assert_eq!(lines[separator + 1], "At the office");
    assert!(lines[separator + 2].contains("Write report"), "{text}");

    // `f` on the agenda block moves it a day; the other block stays.
    run(&mut client, "org.agenda.later", json!({ "position": { "line": 1, "character": 0 } })).unwrap();
    let text = client.view_text(VIEW);
    assert_eq!(text.lines().nth(1), Some("Tuesday    19 May 2026"));
    assert!(text.contains("At the office"));
    let on_tags = text.lines().position(|line| line == "At the office").unwrap() as u32;
    let refused = run(&mut client, "org.agenda.later", json!({ "position": { "line": on_tags, "character": 0 } }));
    assert_eq!(refused, Err("This block has no dates to move through".to_string()));
    // Rebuilding keeps the moved block where it is.
    run(&mut client, "org.agenda.redo", json!({})).unwrap();
    assert_eq!(client.view_text(VIEW).lines().nth(1), Some("Tuesday    19 May 2026"));

    // `m`: a match string, then the tags view.
    let asked = run(&mut client, "org.agenda.dispatch", json!({ "input": "m" })).unwrap();
    assert_eq!((asked["input"]["kind"].as_str(), asked["input"]["prompt"].as_str()), (Some("text"), Some("Match")));
    let context = asked["input"]["context"].clone();
    assert_eq!(context, json!({ "keys": "m", "ask": "match" }));
    run(&mut client, "org.agenda.dispatch", json!({ "keys": "m", "ask": "match", "input": "office" })).unwrap();
    let text = client.view_text(VIEW);
    assert!(text.starts_with("Headlines with TAGS match: office\n"), "{text}");
    assert!(text.contains("Write report"));
    client.shutdown();
}

/// Moving an agenda block below others keeps the cursor in that block,
/// and `hideEmptyBlocks` leaves out the blocks with nothing to show.
#[test]
fn moves_a_lower_block_and_hides_empty_ones() {
    let mut options = clock();
    options["agenda"] = json!({ "hideEmptyBlocks": true, "customCommands": [
        { "key": "o", "description": "Office, then the day", "blocks": [
            { "type": "tags", "match": "nowhere", "header": "Nothing here" },
            { "type": "tags", "match": "office", "header": "At the office" },
            { "type": "agenda", "settings": { "span": "day" } }
        ] }
    ] });
    let mut client = Client::editor(options, None);
    client.open("file:///org/work.org", WORK);
    let result = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.agenda.dispatch", "arguments": [{ "input": "o" }] }),
    );
    assert_eq!(result, Ok(Value::Null));
    client.settle();
    let text = client.refetch_view();
    client.settle();
    assert!(!text.contains("Nothing here"), "{text}");
    assert!(text.starts_with("At the office\n"), "{text}");
    let lines: Vec<&str> = text.lines().collect();
    let monday = lines.iter().position(|line| line.starts_with("Monday")).unwrap() as u32;

    let cursor = |requests: &[Request], text: &str| {
        let shown = requests.iter().find(|request| request.method == "window/showDocument").unwrap();
        let line = shown.params["selection"]["start"]["line"].as_u64().unwrap() as usize;
        text.lines().nth(line).unwrap().to_string()
    };
    let (requests, text) = client.view_command("org.agenda.later", monday);
    assert_eq!(cursor(&requests, &text), "Tuesday    19 May 2026");
    let tuesday = text.lines().position(|line| line.starts_with("Tuesday")).unwrap() as u32;
    let (requests, text) = client.view_command("org.agenda.week", tuesday);
    assert_eq!(cursor(&requests, &text), "Tuesday    19 May 2026");
    let (requests, text) = client.view_command("org.agenda.today", tuesday);
    assert_eq!(cursor(&requests, &text), "Monday     18 May 2026 W21");
    client.shutdown();
}

/// `org-clock-in` and its relatives: the `CLOCK:` line in its drawer, the
/// clock's status for the editor (`org-lsp/clock`), switching to another
/// entry, clocking out, cancelling, and a done state stopping the clock.
#[test]
fn clocks_in_and_out() {
    const URI: &str = "file:///org/clock.org";
    let mut text = "* TODO Write\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n* Other\n".to_string();
    let mut client = Client::editor(clock(), None);
    client.open(URI, &text);
    let mut version = 1;
    // Run a command; apply its edit as the editor would, and return what
    // it said and the clock's last status.
    let mut run = |client: &mut Client, text: &mut String, command: &str, line: u32| {
        client.received.clear();
        client.notifications.clear();
        let at = json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": 0 } });
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [at] }));
        client.settle();
        if let Some(edit) = client.received.iter().find(|request| request.method == "workspace/applyEdit") {
            let (_, edits) = edits_of(edit);
            *text = apply(text, &edits);
            version += 1;
            client.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": URI, "version": version }, "contentChanges": [{ "text": text }] }),
            );
            client.settle();
        }
        let status = client
            .notifications
            .iter()
            .rev()
            .find(|notification| notification.method == "org-lsp/clock")
            .map(|notification| notification.params.clone());
        (result, status)
    };

    let (result, status) = run(&mut client, &mut text, "org.clock.in", 0);
    assert_eq!(result.unwrap()["message"], "Clock starts at [2026-05-18 Mon 10:30] - showing entire task time.");
    assert_eq!(
        text,
        "* TODO Write\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 10:30]\n:END:\n* Other\n"
    );
    let status = status.unwrap();
    assert_eq!((status["text"].as_str(), status["minutes"].as_i64()), (Some("[0:00/1:00] (Write)"), Some(0)));
    // Again on the same entry: it goes on.
    let (result, _) = run(&mut client, &mut text, "org.clock.in", 1);
    assert_eq!(result.unwrap()["message"], "Clock continues in \"Write\"");

    // Another entry: the first clock stops, in the same edit.
    let (_, status) = run(&mut client, &mut text, "org.clock.in", 7);
    assert_eq!(
        text,
        "* TODO Write\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n:LOGBOOK:\n\
         CLOCK: [2026-05-18 Mon 10:30]--[2026-05-18 Mon 10:30] =>  0:00\n:END:\n\
         * Other\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 10:30]\n:END:\n"
    );
    assert_eq!(status.unwrap()["text"], "[0:00] (Other)");
    let (result, status) = run(&mut client, &mut text, "org.clock.out", 0);
    assert_eq!(result.unwrap()["message"], "Clock stopped at [2026-05-18 Mon 10:30] after 0:00");
    assert_eq!(status, Some(Value::Null));
    assert!(
        text.ends_with("* Other\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 10:30]--[2026-05-18 Mon 10:30] =>  0:00\n:END:\n")
    );
    let (result, _) = run(&mut client, &mut text, "org.clock.out", 0);
    assert_eq!(result, Err("No active clock".to_string()));

    // Cancelled: the line goes.
    let before = text.clone();
    run(&mut client, &mut text, "org.clock.in", 0).0.unwrap();
    let (result, _) = run(&mut client, &mut text, "org.clock.cancel", 0);
    assert_eq!(result.unwrap()["message"], "Clock canceled");
    assert_eq!(text, before);

    // Going to the clocked entry; and with none, to the last one.
    run(&mut client, &mut text, "org.clock.in", 0).0.unwrap();
    run(&mut client, &mut text, "org.clock.goto", 8).0.unwrap();
    let shown = client.received.iter().find(|request| request.method == "window/showDocument").unwrap();
    assert_eq!(shown.params["selection"]["start"], json!({ "line": 0, "character": 0 }));

    // Done: the clock stops with it.
    let (_, status) = run(&mut client, &mut text, "org.todo.next", 0);
    assert_eq!(status, Some(Value::Null));
    assert!(text.starts_with("* DONE Write\n"), "{text}");
    assert!(!text.contains("CLOCK: [2026-05-18 Mon 10:30]\n"), "{text}");
    let (result, _) = run(&mut client, &mut text, "org.clock.goto", 3);
    assert_eq!(result.unwrap()["message"], "No running clock, this is the most recently clocked task");
    client.shutdown();
}

/// The agenda: the clocked entry's line highlighted; `l` and `R`.
#[test]
fn shows_the_clock_in_the_agenda() {
    const URI: &str = "file:///org/work.org";
    let text = "* TODO Write\nSCHEDULED: <2026-05-18 Mon>\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]--[2026-05-18 Mon 09:30] =>  0:30\nCLOCK: [2026-05-18 Mon 10:00]\n:END:\n";
    let mut client = Client::editor(clock(), None);
    client.open(URI, text);
    let run = |client: &mut Client, command: &str| {
        let at = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } });
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [at] }));
        client.settle();
        result
    };
    run(&mut client, "org.agenda.list").unwrap();
    let tokens =
        client.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": VIEW } })).unwrap();
    let legend = &client.initialize["capabilities"]["semanticTokensProvider"]["legend"];
    let bit = legend["tokenModifiers"].as_array().unwrap().iter().position(|name| name == "clocking").unwrap();
    let data = tokens["data"].as_array().unwrap();
    assert!(data.chunks(5).any(|token| token[4].as_u64().unwrap() & (1 << bit) != 0), "a token is marked clocking");

    assert_eq!(run(&mut client, "org.agenda.log").unwrap()["message"], "Log mode is on");
    let view = client.view_text(VIEW);
    assert!(view.contains("Clocked:   (0:30) TODO Write"), "{view}");
    assert!(view.contains("Clocked:   (-) TODO Write"), "{view}");
    assert_eq!(run(&mut client, "org.agenda.clockreport").unwrap()["message"], "Clocktable mode is on");
    let view = client.view_text(VIEW);
    assert!(view.contains("| work.org | *File time*      | *0:30* |"), "{view}");
    assert!(view.contains("| Write            | 0:30   |"), "the links show as their descriptions: {view}");
    client.shutdown();
}

/// Clocking in and out from the agenda moves its highlight there: the
/// client is asked for the tokens again, and told the lines to highlight
/// from edge to edge (`org-lsp/lineHighlights`) once it shows the text.
#[test]
fn moves_the_clock_highlight_in_the_agenda() {
    const URI: &str = "file:///org/work.org";
    let mut text = "* TODO Write\nSCHEDULED: <2026-05-18 Mon>\n* TODO Read\nSCHEDULED: <2026-05-18 Mon>\n".to_string();
    let capabilities = json!({
        "workspace": {
            "textDocumentContent": { "dynamicRegistration": true },
            "workspaceEdit": { "documentChanges": true },
            "semanticTokens": { "refreshSupport": true }
        },
        "window": { "showDocument": { "support": true } },
        "general": { "positionEncodings": ["utf-8"] },
        "experimental": { "orgLineHighlights": true }
    });
    let mut client = Client::start(capabilities, clock(), None);
    client.open(URI, &text);
    client.command("org.agenda.list", URI, 0);
    let view = client.show_view();
    client.settle();
    let highlights = |client: &mut Client| -> Vec<Value> {
        std::mem::take(&mut client.notifications)
            .into_iter()
            .filter(|notification| notification.method == "org-lsp/lineHighlights")
            .map(|notification| notification.params)
            .collect()
    };
    assert_eq!(highlights(&mut client), [json!({ "uri": VIEW, "lines": [] })]);
    let line = view.lines().position(|line| line.contains("TODO Read")).unwrap() as u32;
    let requests = client.edit("org.clock.in", VIEW, (line, 0), json!({}), &mut text);
    assert!(text.contains("CLOCK: [2026-05-18 Mon 10:30]"), "{text}");
    assert!(methods(&requests).contains(&"workspace/semanticTokens/refresh"), "{:?}", methods(&requests));
    assert_eq!(
        highlights(&mut client),
        [json!({ "uri": VIEW, "lines": [{ "line": line, "highlight": "agendaClocking" }] })]
    );
    client.edit("org.clock.out", VIEW, (line, 0), json!({}), &mut text);
    assert_eq!(highlights(&mut client), [json!({ "uri": VIEW, "lines": [] })]);
    client.shutdown();
}

/// An agenda written to a file, for clients without virtual documents, is
/// highlighted with the agenda's tokens once the client opens it.
#[test]
fn highlights_the_agenda_file() {
    let mut options = clock();
    options["virtualDocuments"] = json!(false);
    let capabilities = json!({ "window": { "showDocument": { "support": true } } });
    let mut client = Client::start(capabilities, options, None);
    const WORK_URI: &str = "file:///org/work.org";
    client.open(WORK_URI, WORK);
    let context = json!({ "textDocument": { "uri": WORK_URI }, "position": { "line": 1, "character": 0 } });
    client.received.clear();
    client
        .request("workspace/executeCommand", json!({ "command": "org.agenda.list", "arguments": [context] }))
        .unwrap();
    client.settle();
    let shown = client.received.iter().find(|request| request.method == "window/showDocument").expect("shown");
    let agenda = shown.params["uri"].as_str().unwrap().to_string();
    let text = std::fs::read_to_string(agenda.trim_start_matches("file://")).unwrap();
    client.open_as(&agenda, "org-agenda", &text);
    let tokens =
        client.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": agenda } })).unwrap();
    let legend = &client.initialize["capabilities"]["semanticTokensProvider"]["legend"];
    let structure = legend["tokenTypes"].as_array().unwrap().iter().position(|name| name == "agendaStructure").unwrap();
    let data = tokens["data"].as_array().unwrap();
    assert!(data.chunks(5).any(|token| token[3].as_u64() == Some(structure as u64)), "agenda tokens: {data:?}");
    // Read-only, so that editors do not let it be edited; the server
    // writes it again all the same.
    let path = agenda.trim_start_matches("file://");
    assert!(std::fs::metadata(path).unwrap().permissions().readonly());
    let at = json!({ "textDocument": { "uri": agenda }, "position": { "line": 0, "character": 0 } });
    client.received.clear();
    client.request("workspace/executeCommand", json!({ "command": "org.agenda.day", "arguments": [at] })).unwrap();
    client.settle();
    let errors = client.received.iter().filter(|message| message.method == "window/showMessage").collect::<Vec<_>>();
    assert!(errors.is_empty(), "messages: {errors:?}");
    let day = std::fs::read_to_string(path).unwrap();
    assert_ne!(day, text);
    assert!(std::fs::metadata(path).unwrap().permissions().readonly());
    client.shutdown();
    assert!(!std::path::Path::new(path).exists());
}

/// `v a` and `v A` (`org-agenda-archives-mode`): entries in archived trees,
/// then those and the files the agenda files archive to.
#[test]
fn includes_archives_when_asked() {
    let root = std::env::temp_dir().join(format!("org-lsp-agenda-archives-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("work.org"), "* TODO Write\n* Old :ARCHIVE:\n** TODO Leftover\n").unwrap();
    std::fs::write(root.join("work.org_archive"), "* TODO Archived long ago\n").unwrap();
    let root_uri = format!("file://{}", root.display());
    let mut client = Client::editor(clock(), Some(&root_uri));
    client.command("org.agenda.todos", VIEW, 0);
    // Without the tags column.
    let todos = |client: &mut Client| {
        let view = client.view_text(VIEW);
        view.lines().skip(1).map(|line| line.split("  :").next().unwrap().trim().to_string()).collect::<Vec<String>>()
    };
    assert_eq!(todos(&mut client), ["work:       TODO Write"]);
    let run = |client: &mut Client, command: &str| {
        let at = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 0, "character": 0 } });
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [at] }));
        client.settle();
        result.unwrap()["message"].clone()
    };
    assert_eq!(run(&mut client, "org.agenda.archives"), "Trees with :ARCHIVE: tag are included");
    assert_eq!(todos(&mut client), ["work:       TODO Write", "work:       TODO Leftover"]);
    assert_eq!(
        run(&mut client, "org.agenda.archives.files"),
        "Trees with :ARCHIVE: tag and all active archive files are included"
    );
    assert_eq!(
        todos(&mut client),
        ["work:       TODO Write", "work:       TODO Leftover", "work:       TODO Archived long ago"]
    );
    assert_eq!(run(&mut client, "org.agenda.archives.files"), "No archives are included");
    assert_eq!(todos(&mut client), ["work:       TODO Write"]);
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// Run `command` at `line` and `character` of `uri`, and return its result
/// with the files the server then asked the editor to save.
fn run_saving(client: &mut Client, command: &str, uri: &str, at: (u32, u32), extra: Value) -> (Value, Vec<Value>) {
    client.notifications.clear();
    client.received.clear();
    let mut context = json!({ "textDocument": { "uri": uri }, "position": { "line": at.0, "character": at.1 } });
    for (key, value) in extra.as_object().into_iter().flatten() {
        context[key] = value.clone();
    }
    let result = client
        .request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
        .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    client.settle();
    let saved = client
        .notifications
        .iter()
        .filter(|notification| notification.method == "org-lsp/save")
        .map(|notification| notification.params["uris"].clone())
        .collect();
    (result, saved)
}

/// Commands say what they did, as Emacs says it in the echo area, and the
/// files they change from elsewhere are saved.
#[test]
fn says_what_it_did_and_saves_what_it_changed_elsewhere() {
    let mut options = clock();
    options["todoKeywords"] = json!(["TODO(t)", "|", "DONE(d)"]);
    let mut client = Client::editor(options, None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    let report = text.lines().position(|line| line.contains("Write report")).unwrap() as u32;

    // From the agenda the entry is out of sight: Emacs says what it became,
    // and its file is saved.
    let (result, saved) = run_saving(&mut client, "org.todo", VIEW, (report, 0), json!({ "input": "d" }));
    assert_eq!(result, json!({ "message": "TODO state changed to DONE" }));
    assert_eq!(saved, [json!(["file:///org/work.org"])]);
    let (result, _) = run_saving(&mut client, "org.agenda.day", VIEW, (report, 0), json!({}));
    assert_eq!(result, json!({ "message": "Switched to day view" }));

    // In the file itself the edit is the user's to save.
    let (result, saved) = run_saving(&mut client, "org.priority.down", "file:///org/work.org", (1, 0), json!({}));
    assert_eq!(result, json!({ "message": "Priority of current item set to B" }));
    assert!(saved.is_empty());
    let (result, _) = run_saving(&mut client, "org.todo", "file:///org/work.org", (1, 0), json!({ "input": "t" }));
    assert_eq!(result, json!({ "message": "TODO state was already TODO" }));
    // A repeating entry marked done is open again, its date moved on: Org
    // says where to, not that the state was already TODO.
    client.open("file:///org/repeat.org", "* TODO Water\nSCHEDULED: <2026-05-18 Mon +1w>\n");
    let (result, _) = run_saving(&mut client, "org.todo", "file:///org/repeat.org", (0, 0), json!({ "input": "d" }));
    assert_eq!(result, json!({ "message": "Entry repeats: SCHEDULED: <2026-05-25 Mon +1w>" }));

    // `cid`: the timestamp at the cursor changes, inactive as it was; off a
    // timestamp there is nothing to change.
    let mut dates = "* a\n<2026-05-20 Wed> [2026-05-21 Thu]\n".to_string();
    client.open("file:///org/dates.org", &dates);
    let requests =
        client.edit("org.timestamp.change", "file:///org/dates.org", (1, 20), json!({ "input": "fri" }), &mut dates);
    assert_eq!(methods(&requests)[0], "workspace/applyEdit");
    assert_eq!(dates, "* a\n<2026-05-20 Wed> [2026-05-22 Fri]\n");
    let context =
        json!({ "textDocument": { "uri": "file:///org/dates.org" }, "position": { "line": 0, "character": 0 } });
    let refused = client
        .request("workspace/executeCommand", json!({ "command": "org.timestamp.change", "arguments": [context] }));
    assert_eq!(refused, Err("No timestamp at the cursor".to_string()));
    client.shutdown();

    // `autoSave` off: nothing is saved.
    let mut options = clock();
    options["autoSave"] = json!(false);
    let mut client = Client::editor(options, None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    client.show_view();
    let (_, saved) = run_saving(&mut client, "org.todo.next", VIEW, (report, 0), json!({}));
    assert!(saved.is_empty());
    client.shutdown();
}

/// `t` in the calendar picks the time with the keys that move by day and
/// week; the answer takes it in place of the typed one.
#[test]
fn picks_a_time_in_the_calendar() {
    const URI: &str = "file:///org/notes.org";
    const PROMPT: &str = "org-agenda:/Org%20Date";
    let mut client = Client::with_date_prompt(clock());
    let notes = "* TODO Call them\nSCHEDULED: <2026-05-20 Wed 10:00>\n";
    client.open(URI, notes);
    let on_prompt = json!({ "textDocument": { "uri": PROMPT }, "position": { "line": 0, "character": 0 } });
    let run = |client: &mut Client, command: &str, context: Value| {
        client.received.clear();
        let result =
            client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] })).unwrap();
        client.settle();
        result
    };
    run(
        &mut client,
        "org.schedule",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 0 } }),
    );
    for command in
        ["org.calendar.time", "org.calendar.week.earlier", "org.calendar.day.later", "org.calendar.week.later"]
    {
        run(&mut client, command, on_prompt.clone());
    }
    let text = client.view_text(PROMPT);
    assert!(text.contains("\nTime: 10:55\n"), "{text}");
    assert!(text.trim_end().ends_with("=> <2026-05-20 Wed 10:55>"), "{text}");
    // `t` again: the keys move the day, the time stays.
    run(&mut client, "org.calendar.time", on_prompt.clone());
    run(&mut client, "org.calendar.day.later", on_prompt.clone());
    let text = client.view_text(PROMPT);
    assert!(text.trim_end().ends_with("=> <2026-05-21 Thu 10:55>"), "{text}");
    let done = run(&mut client, "org.calendar.select", on_prompt);
    assert_eq!(done["message"], "Scheduled to <2026-05-21 Thu 10:55>");
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("the change");
    assert_eq!(apply(notes, &edits_of(edit).1), "* TODO Call them\nSCHEDULED: <2026-05-21 Thu 10:55>\n");
    client.shutdown();
}

/// Keys of the calendar pressed quickly one after another: the second one
/// comes with the cursor where it was before the first moved it, and goes
/// on from the day the first selected.
#[test]
fn calendar_keys_go_on_from_the_selected_day_until_the_cursor_follows() {
    const URI: &str = "file:///org/notes.org";
    const PROMPT: &str = "org-agenda:/Org%20Date";
    let mut client = Client::with_date_prompt(clock());
    client.open(URI, "* TODO Call them\nSCHEDULED: <2026-05-20 Wed>\n");
    let run = |client: &mut Client, command: &str, context: Value| {
        client.received.clear();
        client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] })).unwrap();
        client.settle();
    };
    run(
        &mut client,
        "org.schedule",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 0 } }),
    );
    let cursor = client.received[0].params["selection"]["start"].clone();
    let text = client.view_text(PROMPT);
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": PROMPT, "languageId": "org-date", "version": 1, "text": text } }),
    );
    client.settle();
    // The cursor stays on the 20th: the editor has not got round to moving
    // it when the next key comes.
    let stale = json!({ "textDocument": { "uri": PROMPT }, "position": cursor });
    client.hold_show = true;
    run(&mut client, "org.calendar.week.later", stale.clone());
    let text = client.view_text(PROMPT);
    assert!(text.trim_end().ends_with("=> <2026-05-27 Wed>"), "{text}");
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": PROMPT, "version": 2 }, "contentChanges": [{ "text": text }] }),
    );
    client.settle();
    run(&mut client, "org.calendar.day.later", stale.clone());
    let text = client.view_text(PROMPT);
    assert!(text.trim_end().ends_with("=> <2026-05-28 Thu>"), "from the selected day: {text}");
    client.shutdown();
}

/// The note a TODO state wants is written in a buffer of its own, as
/// Emacs's `*Org Note*`: finished, it is logged with the change;
/// cancelled, the state changes and nothing is logged.
#[test]
fn writes_a_state_note_in_a_buffer_of_its_own() {
    let mut options = clock();
    options["todoKeywords"] = json!(["TODO(t)", "WAIT(w@)", "|", "DONE(d)"]);
    let mut client = Client::with_date_prompt(options);
    let note_for = |client: &mut Client, file: &str| {
        client.open(file, "* TODO Call\n");
        client.received.clear();
        let context =
            json!({ "textDocument": { "uri": file }, "position": { "line": 0, "character": 0 }, "input": "w" });
        let result = client
            .request("workspace/executeCommand", json!({ "command": "org.todo", "arguments": [context] }))
            .unwrap();
        assert_eq!(result, Value::Null);
        client.settle();
        assert!(client.received.iter().all(|request| request.method != "workspace/applyEdit"), "nothing changes yet");
        let show =
            client.received.iter().find(|request| request.method == "window/showDocument").expect("the note buffer");
        assert_eq!(show.params["selection"]["start"], json!({ "line": 2, "character": 0 }));
        let buffer = show.params["uri"].as_str().unwrap().to_string();
        assert!(buffer.ends_with("/CAPTURE-Note.org"), "{buffer}");
        let text = std::fs::read_to_string(buffer.strip_prefix("file://").unwrap()).unwrap();
        assert_eq!(text, "# Insert note for state change from \"TODO\" to \"WAIT\".\n\n\n");
        buffer
    };
    let finish = |client: &mut Client, command: &str, buffer: &str| {
        client.received.clear();
        let context = json!({ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } });
        let result =
            client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] })).unwrap();
        assert_eq!(result["close"], buffer);
        client.settle();
        let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("the change");
        apply("* TODO Call\n", &edits_of(edit).1)
    };

    let buffer = note_for(&mut client, "file:///org/a.org");
    client.open(&buffer, "# Insert note for state change from \"TODO\" to \"WAIT\".\n\nWaiting for Bob\n\n");
    assert_eq!(
        finish(&mut client, "org.capture.finish", &buffer),
        "* WAIT Call\n- State \"WAIT\"       from \"TODO\"       [2026-05-18 Mon 10:30] \\\\\n  Waiting for Bob"
    );

    let buffer = note_for(&mut client, "file:///org/b.org");
    assert_eq!(finish(&mut client, "org.capture.abort", &buffer), "* WAIT Call\n");
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

    // "Water plants" repeats every week. Marking it done moves it to the
    // next week and leaves it open, as `org-todo` does. The file is not
    // open in the editor, so the edit carries no version.
    let requests = client.command("org.todo.next", VIEW, 2);
    let (uri, edits) = edits_of(&requests[0]);
    assert_eq!(uri, format!("{root_uri}/sub/home.org"));
    assert_eq!(requests[0].params["edit"]["documentChanges"][0]["textDocument"]["version"], Value::Null);
    let repeated = apply(HOME, &edits);
    expect![[r#"
        * TODO Water plants
        SCHEDULED: <2026-05-23 Sat +1w>
        :PROPERTIES:
        :LAST_REPEAT: [2026-05-18 Mon 10:30]
        :END:
        - State "DONE"       from "TODO"       [2026-05-18 Mon 10:30]
        * DONE Buy milk
    "#]]
    .assert_eq(&repeated);

    // The editor opens the file to apply the edit. What it holds now
    // counts, not what is on disk.
    client.open(&uri, &repeated);
    client.command("org.agenda.list", VIEW, 0);
    let agenda = client.view_text(VIEW);
    assert!(!agenda.contains("Sched. 2x"), "{agenda}");
    assert!(agenda.contains("Saturday   23 May 2026\n  home:       Scheduled:  TODO Water plants\n"), "{agenda}");
    client.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
    client.command("org.agenda.todos", VIEW, 0);
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

/// The agenda lines from `from` up to the next line that starts with `until`.
fn lines_between(text: &str, from: &str, until: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with(until) && inside {
            break;
        }
        inside |= line.starts_with(from);
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[test]
fn changes_priority_from_the_agenda_and_the_cursor_follows() {
    let mut client = Client::editor(clock(), None);
    let mut work = WORK.to_string();
    client.open("file:///org/work.org", WORK);
    client.open("file:///org/home.org", HOME);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    let line_of = |text: &str, needle: &str| text.lines().position(|line| line.contains(needle)).unwrap() as u32;
    // "Call the client" is due in two days and stands below today's
    // scheduled entry.
    let call = line_of(&text, "In   2 d.:");
    assert_eq!(text.lines().nth(call as usize - 1).unwrap(), "  home:       Sched. 2x:  TODO Water plants");

    // Up from no priority: the default, B. Nothing moves in the agenda.
    let requests = client.edit("org.priority.up", VIEW, (call, 14), json!({}), &mut work);
    assert_eq!(methods(&requests), ["workspace/applyEdit", "workspace/textDocumentContent/refresh"]);
    let (uri, edits) = edits_of(&requests[0]);
    assert_eq!(uri, "file:///org/work.org");
    assert_eq!(
        edits,
        [
            json!({ "range": { "start": { "line": 3, "character": 7 }, "end": { "line": 3, "character": 7 } }, "newText": "[#B] " })
        ]
    );
    client.received.clear();
    let text = client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), [] as [&str; 0]);
    assert_eq!(text.lines().nth(call as usize).unwrap(), "  work:       In   2 d.:  TODO [#B] Call the client");

    // Up again: A, which sorts the entry above the other one. The cursor
    // goes with it, in its column, once the editor shows the new text.
    let requests = client.edit("org.priority.up", VIEW, (call, 14), json!({}), &mut work);
    assert_eq!(methods(&requests), ["workspace/applyEdit", "workspace/textDocumentContent/refresh"]);
    client.received.clear();
    let text = client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), ["window/showDocument"]);
    assert_eq!(client.received[0].params["selection"]["start"], json!({ "line": call - 1, "character": 14 }));
    assert_eq!(
        lines_between(&text, "  work:       In", "Tuesday"),
        "  work:       In   2 d.:  TODO [#A] Call the client\n  home:       Sched. 2x:  TODO Water plants\n"
    );
    let call = call - 1;

    // Past the highest priority comes none, and from there the same key
    // goes on at the lowest.
    client.edit("org.priority.up", VIEW, (call, 0), json!({}), &mut work);
    assert_eq!(work.lines().nth(3).unwrap(), "* TODO Call [[https://example.com][the client]]");
    let text = client.refetch_view();
    let call = line_of(&text, "In   2 d.:");
    client.edit("org.priority.up", VIEW, (call, 0), json!({}), &mut work);
    assert_eq!(work.lines().nth(3).unwrap(), "* TODO [#C] Call [[https://example.com][the client]]");
    // The other direction, from the file this time.
    client.edit("org.priority.down", "file:///org/work.org", (4, 3), json!({}), &mut work);
    assert_eq!(work.lines().nth(3).unwrap(), "* TODO Call [[https://example.com][the client]]");
    // Something else happened in between: the cycle starts over.
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": "file:///org/work.org", "version": 50 }, "contentChanges": [{ "text": work }] }),
    );
    client.edit("org.priority.down", "file:///org/work.org", (4, 3), json!({}), &mut work);
    assert_eq!(work.lines().nth(3).unwrap(), "* TODO [#B] Call [[https://example.com][the client]]");
    client.shutdown();
}

#[test]
fn changes_dates_from_the_agenda() {
    const TRIP: &str =
        "* Trip <2026-05-21 Thu>--<2026-05-22 Fri>\n* TODO Water plants\nSCHEDULED: <2026-05-16 Sat +1w>\n";
    let mut client = Client::editor(clock(), None);
    let mut home = TRIP.to_string();
    client.open("file:///org/home.org", TRIP);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    let line_of = |text: &str, needle: &str| text.lines().position(|line| line.contains(needle)).unwrap() as u32;

    // A date that has passed goes to today with the first step.
    let plants = line_of(&text, "Sched. 2x:");
    let requests = client.edit("org.date.later", VIEW, (plants, 0), json!({}), &mut home);
    assert_eq!(home.lines().nth(2).unwrap(), "SCHEDULED: <2026-05-18 Mon +1w>");
    let (_, edits) = edits_of(&requests[0]);
    // Only what differs is replaced.
    assert_eq!(
        edits,
        [
            json!({ "range": { "start": { "line": 2, "character": 21 }, "end": { "line": 2, "character": 26 } }, "newText": "8 Mon" })
        ]
    );
    let text = client.refetch_view();
    assert_eq!(text.lines().nth(plants as usize).unwrap(), "  home:       Scheduled:  TODO Water plants");

    // From there, day by day. The entry leaves today's list, and the
    // cursor goes with it.
    client.received.clear();
    client.edit("org.date.later", VIEW, (plants, 0), json!({}), &mut home);
    assert_eq!(home.lines().nth(2).unwrap(), "SCHEDULED: <2026-05-19 Tue +1w>");
    client.received.clear();
    let text = client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), ["window/showDocument"]);
    let plants = line_of(&text, "Water plants");
    assert_eq!(client.received[0].params["selection"]["start"]["line"], plants);
    assert_eq!(text.lines().nth(plants as usize - 1).unwrap(), "Tuesday    19 May 2026");

    // A count is a number of days.
    client.edit("org.date.earlier", VIEW, (plants, 0), json!({ "count": 3 }), &mut home);
    assert_eq!(home.lines().nth(2).unwrap(), "SCHEDULED: <2026-05-16 Sat +1w>");

    // A date range moves as a whole, from either of its days.
    let text = client.refetch_view();
    let trip = line_of(&text, "(2/2):  Trip");
    client.edit("org.date.later", VIEW, (trip, 0), json!({}), &mut home);
    assert_eq!(home.lines().next().unwrap(), "* Trip <2026-05-22 Fri>--<2026-05-23 Sat>");

    // The TODO list has entries, but no dates.
    client.command("org.agenda.todos", VIEW, 0);
    client.refetch_view();
    let context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } });
    let error = client
        .request("workspace/executeCommand", json!({ "command": "org.date.later", "arguments": [context] }))
        .unwrap_err();
    assert_eq!(error, "The TODO list shows no dates to change");
    // Its lines offer no date actions either.
    let actions = client
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": VIEW },
                "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 0 } },
                "context": { "diagnostics": [] }
            }),
        )
        .unwrap();
    let kinds: Vec<&str> = actions.as_array().unwrap().iter().map(|action| action["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "org.todo.next",
            "org.todo.previous",
            "org.priority.up",
            "org.priority.down",
            "org.clock.in",
            "org.archive.subtree",
            "org.agenda.goto",
            "org.agenda.goto.other"
        ]
    );
    client.shutdown();
}

#[test]
fn changes_timestamps_and_priorities_in_a_file() {
    const NOTES: &str = "* TODO Call them :work:\nSCHEDULED: <2026-05-18 Mon 10:58-11:30 +1w>\nSome text.\n";
    const URI: &str = "file:///org/notes.org";
    let mut client = Client::editor(clock(), None);
    let mut notes = NOTES.to_string();
    client.open(URI, NOTES);
    let line = |text: &str, number: usize| text.lines().nth(number).unwrap().to_string();

    // The part under the cursor changes: the day, …
    client.edit("org.timestamp.up", URI, (1, 21), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: <2026-05-19 Tue 10:58-11:30 +1w>");
    // … the minutes, in steps of five and with the end of the range, …
    client.edit("org.timestamp.up", URI, (1, 30), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: <2026-05-19 Tue 11:00-11:35 +1w>");
    // … by exactly the count that was given, …
    client.edit("org.timestamp.down", URI, (1, 30), json!({ "count": 2 }), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: <2026-05-19 Tue 10:58-11:33 +1w>");
    // … and the repeater.
    client.edit("org.timestamp.up", URI, (1, 40), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: <2026-05-19 Tue 10:58-11:33 +2w>");
    // On a bracket the timestamp becomes inactive.
    client.edit("org.timestamp.up", URI, (1, 11), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: [2026-05-19 Tue 10:58-11:33 +2w]");
    // A day later, wherever in the timestamp the cursor is.
    client.edit("org.date.later", URI, (1, 14), json!({ "count": 7 }), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: [2026-05-26 Tue 10:58-11:33 +2w]");

    // The shifted cursor keys act on what the cursor is on.
    client.edit("org.shift.down", URI, (1, 21), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: [2026-05-25 Mon 10:58-11:33 +2w]");
    client.edit("org.shift.left", URI, (1, 30), json!({}), &mut notes);
    assert_eq!(line(&notes, 1), "SCHEDULED: [2026-05-24 Sun 10:58-11:33 +2w]");
    client.edit("org.shift.up", URI, (0, 3), json!({}), &mut notes);
    assert_eq!(line(&notes, 0), format!("* TODO [#B] Call them{}:work:", " ".repeat(50)));
    client.edit("org.shift.right", URI, (0, 3), json!({}), &mut notes);
    assert_eq!(line(&notes, 0), format!("* DONE [#B] Call them{}:work:", " ".repeat(50)));
    // The priority commands work from anywhere in the entry.
    client.edit("org.priority.up", URI, (2, 4), json!({}), &mut notes);
    assert_eq!(line(&notes, 0), format!("* DONE [#A] Call them{}:work:", " ".repeat(50)));

    // Where a command means nothing, the server says so in a way that an
    // editor can tell from a failure: the key may then do something else.
    let run = |client: &mut Client, command: &str, uri: &str, position: (u32, u32)| {
        let context =
            json!({ "textDocument": { "uri": uri }, "position": { "line": position.0, "character": position.1 } });
        client.request_raw("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
    };
    for (command, position) in [("org.timestamp.up", (2, 4)), ("org.shift.up", (2, 4)), ("org.date.later", (0, 3))] {
        let error = run(&mut client, command, URI, position).unwrap_err();
        assert_eq!(error.data, Some(json!({ "notApplicable": true })), "{command}: {}", error.message);
    }
    let error = run(&mut client, "org.timestamp.up", URI, (2, 4)).unwrap_err();
    assert_eq!(error.message, "Not at a timestamp");
    // A command that applies but cannot be done is a plain failure.
    client.open("file:///org/empty.org", "* a\n");
    client.command("org.agenda.todos", VIEW, 0);
    let error = run(&mut client, "org.timestamp.up", VIEW, (0, 0)).unwrap_err();
    assert_eq!((error.message.as_str(), error.data), ("No entry on this line", None));

    // The actions at a timestamp.
    let actions = client
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": URI },
                "range": { "start": { "line": 1, "character": 21 }, "end": { "line": 1, "character": 21 } },
                "context": { "diagnostics": [], "only": ["org.timestamp", "org.date"] }
            }),
        )
        .unwrap();
    let titles: Vec<&str> =
        actions.as_array().unwrap().iter().map(|action| action["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        [
            "Increase the part of the timestamp at the cursor",
            "Decrease the part of the timestamp at the cursor",
            "Move the date one day later",
            "Move the date one day earlier"
        ]
    );
    client.shutdown();
}

/// Conformance against Emacs: `tests/agenda-actions/cases.json` holds agenda
/// files, a line of the agenda, a command and what the files read after
/// Emacs ran its own agenda command there (see
/// `scripts/agenda_action_reference.py`).
#[test]
fn agenda_actions_match_emacs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/agenda-actions/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = String::new();
    let mut changed = 0;
    for (index, case) in cases.iter().enumerate() {
        let (today, now) = case["clock"].as_str().unwrap().split_once(' ').unwrap();
        let mut options = case.get("options").cloned().unwrap_or_else(|| json!({}));
        options["clock"] = json!({ "today": today, "now": now });
        options["agenda"] = json!({ "span": case.get("span").cloned().unwrap_or_else(|| json!("week")) });
        let mut client = Client::editor(options, None);
        let files = case["files"].as_object().unwrap();
        for (name, text) in files {
            client.open(&format!("file:///org/{name}"), text.as_str().unwrap());
        }
        let todo_list = case.get("view").and_then(Value::as_str) == Some("todo");
        client.command(if todo_list { "org.agenda.todos" } else { "org.agenda.list" }, VIEW, 0);
        let view = client.view_text(VIEW);
        let needle = case["line"].as_str().unwrap();
        let line = view.lines().position(|line| line.contains(needle)).unwrap_or_else(|| panic!("no `{needle}`"));
        let mut context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": line, "character": 0 } });
        if let Some(count) = case.get("count").filter(|count| !count.is_null()) {
            context["count"] = count.clone();
        }
        let command = format!("org.{}", case["command"].as_str().unwrap().replace('-', "."));
        // A command that asks for a date first says what the prompt
        // starts with, and then runs with the answer.
        if let Some(answer) = case.get("answer") {
            let asked =
                client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
            if let (Ok(asked), Value::Object(_)) = (&asked, &case["expected"]) {
                if asked["input"]["value"] != case["prompt"] {
                    failures.push_str(&format!(
                        "\ncase {index}: {command} on `{needle}` offers {} where Emacs offers {}\n",
                        asked["input"]["value"], case["prompt"]
                    ));
                }
            }
            context["input"] = answer.clone();
        }
        client.received.clear();
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
        client.settle();
        // The files as they read once the editor has applied the edit.
        let mut after: serde_json::Map<String, Value> = files.clone();
        for request in client.received.iter().filter(|request| request.method == "workspace/applyEdit") {
            let (uri, edits) = edits_of(request);
            let name = uri.rsplit('/').next().unwrap().to_string();
            let changed = apply(after[&name].as_str().unwrap(), &edits);
            after.insert(name, Value::String(changed));
        }
        changed += usize::from(after != *files);
        // A command that changes the view: the new text, and where the
        // cursor goes in it.
        if let (Some(agenda), Ok(_)) = (case.get("agenda"), &result) {
            let shown = client.view_text(VIEW);
            let reveal = client.received.iter().rev().find(|request| request.method == "window/showDocument");
            let cursor = reveal.map(|request| request.params["selection"]["start"]["line"].clone());
            if shown != agenda.as_str().unwrap() || cursor.as_ref() != Some(&case["cursor"]) {
                failures.push_str(&format!(
                    "\ncase {index}: {command} `{}`\n--- emacs (cursor {})\n{}\n--- org-lsp (cursor {cursor:?})\n{shown}\n",
                    case["answer"],
                    case["cursor"],
                    agenda.as_str().unwrap()
                ));
            }
        }
        let same = match (&case["expected"], &result) {
            // Where Emacs refuses, so must the server.
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok(_)) => *expected == after,
            _ => false,
        };
        if !same {
            failures.push_str(&format!(
                "\ncase {index}: {command} on `{needle}` (count {})\n--- emacs\n{:#}\n--- org-lsp ({result:?})\n{:#}\n",
                case["count"],
                case["expected"],
                Value::Object(after)
            ));
        }
        client.shutdown();
    }
    assert!(failures.is_empty(), "agenda commands differ from Emacs:{failures}");
    // The comparison means something only if most commands changed a file.
    assert!(changed > cases.len() / 2, "{changed} of {} cases changed a file", cases.len());
}

/// Keys can come faster than the agenda is redrawn. A line number then
/// names a line of the text the editor still shows, or the line the
/// cursor is about to leave. The command must reach the entry the user
/// sees under the cursor, never the one that has since taken its place.
#[test]
fn a_command_from_a_stale_agenda_reaches_its_entry() {
    const NOTES: &str = "* TODO First\nSCHEDULED: <2026-05-18 Mon>\n* TODO Second\nSCHEDULED: <2026-05-18 Mon>\n";
    const URI: &str = "file:///org/notes.org";
    let mut client = Client::editor(clock(), None);
    let mut notes = NOTES.to_string();
    client.open(URI, NOTES);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    let line_of = |text: &str, needle: &str| text.lines().position(|line| line.contains(needle)).unwrap() as u32;
    let first = line_of(&text, "TODO First");
    assert_eq!(line_of(&text, "TODO Second"), first + 1);

    // "First" moves to Tuesday. Its line now shows "Second", but the
    // editor has not fetched the new text.
    client.edit("org.date.later", VIEW, (first, 0), json!({}), &mut notes);
    assert_eq!(notes, "* TODO First\nSCHEDULED: <2026-05-19 Tue>\n* TODO Second\nSCHEDULED: <2026-05-18 Mon>\n");
    assert_eq!(client.view_text(VIEW).lines().nth(first as usize).unwrap(), "  notes:      Scheduled:  TODO Second");
    // The same key again, from the same line of the old text.
    client.edit("org.date.later", VIEW, (first, 0), json!({}), &mut notes);
    assert_eq!(notes, "* TODO First\nSCHEDULED: <2026-05-20 Wed>\n* TODO Second\nSCHEDULED: <2026-05-18 Mon>\n");
    // What the editor asks about its old text is answered from that text.
    let definition = client
        .request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": VIEW }, "position": { "line": first, "character": 0 } }),
        )
        .unwrap();
    assert_eq!(definition["range"]["start"]["line"], 0);

    // Now the editor shows the new text. The server asks it to move the
    // cursor to the entry; until the editor has done that, the cursor is
    // still reported on the old line, and still means "First".
    client.hold_show = true;
    client.received.clear();
    let text = client.refetch_view();
    client.settle();
    assert_eq!(client.held.len(), 1);
    let target = client.held[0].params["selection"]["start"]["line"].as_u64().unwrap() as u32;
    assert_eq!(target, line_of(&text, "TODO First"));
    assert_eq!(text.lines().nth(first as usize).unwrap(), "  notes:      Scheduled:  TODO Second");
    client.edit("org.date.later", VIEW, (first, 0), json!({}), &mut notes);
    assert_eq!(notes, "* TODO First\nSCHEDULED: <2026-05-21 Thu>\n* TODO Second\nSCHEDULED: <2026-05-18 Mon>\n");

    // Once the cursor has moved, a line means what it shows.
    client.hold_show = false;
    for request in std::mem::take(&mut client.held) {
        client.answer(request);
    }
    client.refetch_view();
    client.settle();
    client.edit("org.date.later", VIEW, (first, 0), json!({}), &mut notes);
    assert_eq!(notes, "* TODO First\nSCHEDULED: <2026-05-21 Thu>\n* TODO Second\nSCHEDULED: <2026-05-19 Tue>\n");
    client.shutdown();
}

#[test]
fn a_cursor_move_for_an_older_view_is_dropped() {
    const HOME: &str = "* TODO Water plants\nSCHEDULED: <2026-05-24 Sun +1w>\n* Meeting <2026-05-19 Tue 14:00>\n";
    const URI: &str = "file:///org/home.org";
    let mut client = Client::editor(clock(), None);
    let mut home = HOME.to_string();
    client.open(URI, HOME);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    let plants = text.lines().position(|line| line.contains("Water plants")).unwrap() as u32;

    // The entry moves to Tuesday, and the cursor is to go with it once the
    // editor shows the new text. Before it does, the same line moves the
    // entry back to Saturday, where its repeat stands on that very line.
    client.edit("org.date.earlier", VIEW, (plants, 0), json!({ "count": 5 }), &mut home);
    assert_eq!(home.lines().nth(1).unwrap(), "SCHEDULED: <2026-05-19 Tue +1w>");
    client.edit("org.date.earlier", VIEW, (plants, 0), json!({ "count": 3 }), &mut home);
    assert_eq!(home.lines().nth(1).unwrap(), "SCHEDULED: <2026-05-16 Sat +1w>");

    // The cursor stays: the line it would have gone to belongs to a view
    // the editor never showed, and now shows the meeting.
    client.received.clear();
    let text = client.refetch_view();
    client.settle();
    assert_eq!(methods(&client.received), Vec::<&str>::new());
    assert_eq!(text.lines().nth(plants as usize).unwrap(), "  home:       Scheduled:  TODO Water plants");
    assert_eq!(text.lines().nth(plants as usize - 1).unwrap(), "Saturday   23 May 2026");
    client.shutdown();
}

#[test]
fn asks_for_a_date_and_uses_the_answer() {
    const NOTES: &str = "* TODO Call them\nSCHEDULED: <2026-05-20 Wed 10:00 +1w>\n* Notes\nSee \n";
    const URI: &str = "file:///org/notes.org";
    let mut client = Client::editor(clock(), None);
    let mut notes = NOTES.to_string();
    client.open(URI, NOTES);
    let at = |line: u32, character: u32, extra: Value| {
        let mut context =
            json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": character } });
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        context
    };

    // Without an answer the command says what to ask, starting from the
    // time of the date the entry has.
    let asked = client
        .request("workspace/executeCommand", json!({ "command": "org.schedule", "arguments": [at(0, 0, json!({}))] }))
        .unwrap();
    assert_eq!(asked, json!({ "input": { "kind": "date", "prompt": "Schedule", "value": "10:00" } }));
    // What an answer means, while it is typed.
    let preview = |client: &mut Client, input: &str| {
        let context = at(0, 0, json!({ "command": "org.schedule", "input": input }));
        client.request("workspace/executeCommand", json!({ "command": "org.date.preview", "arguments": [context] }))
    };
    assert_eq!(preview(&mut client, "fri 9:00").unwrap(), json!({ "timestamp": "<2026-05-22 Fri 09:00>" }));
    assert_eq!(preview(&mut client, "++1").unwrap(), json!({ "timestamp": "<2026-05-21 Thu>" }));

    // With one it runs; the repeater stays.
    client.edit("org.schedule", URI, (0, 0), json!({ "input": "fri" }), &mut notes);
    assert_eq!(notes.lines().nth(1).unwrap(), "SCHEDULED: <2026-05-22 Fri +1w>");
    client.edit("org.deadline", URI, (0, 0), json!({ "input": "+1w" }), &mut notes);
    // As in Emacs, a new keyword starts the planning line.
    assert_eq!(notes.lines().nth(1).unwrap(), "DEADLINE: <2026-05-25 Mon> SCHEDULED: <2026-05-22 Fri +1w>");

    // A timestamp goes in at the cursor, which then stands after it.
    let requests = client.edit("org.timestamp.insert.inactive", URI, (3, 4), json!({ "input": "3pm" }), &mut notes);
    assert_eq!(notes.lines().nth(3).unwrap(), "See [2026-05-18 Mon 15:00]");
    assert_eq!(methods(&requests), ["workspace/applyEdit", "window/showDocument"]);
    assert_eq!(requests[1].params["uri"], URI);
    assert_eq!(requests[1].params["selection"]["start"], json!({ "line": 3, "character": 26 }));
    // Right after it, the same command adds the end of a range.
    client.edit("org.timestamp.insert.inactive", URI, (3, 26), json!({ "input": "tue" }), &mut notes);
    assert_eq!(notes.lines().nth(3).unwrap(), "See [2026-05-18 Mon 15:00]--[2026-05-19 Tue]");

    // Nothing to schedule before the first headline, and nothing from the
    // agenda yet.
    client.open("file:///org/intro.org", "Intro\n* a\n");
    let context =
        json!({ "textDocument": { "uri": "file:///org/intro.org" }, "position": { "line": 0, "character": 0 } });
    let error = client
        .request_raw("workspace/executeCommand", json!({ "command": "org.schedule", "arguments": [context] }))
        .unwrap_err();
    assert_eq!(error.data, Some(json!({ "notApplicable": true })));
    client.shutdown();
}

#[test]
fn shows_a_calendar_with_the_date_prompt() {
    const NOTES: &str = "* TODO Call them\nSCHEDULED: <2026-05-20 Wed 10:00 +1w>\n* Notes\n";
    const URI: &str = "file:///org/notes.org";
    const PROMPT: &str = "org-agenda:/Org%20Date";
    let mut client = Client::with_date_prompt(clock());
    let mut notes = NOTES.to_string();
    client.open(URI, NOTES);
    let manifest = &client.initialize["capabilities"]["experimental"]["org"];
    assert_eq!(manifest["datePrompt"]["uri"], PROMPT);
    let at = |line: u32, character: u32| json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": character } });
    let on_prompt = |line: u32, character: u32| json!({ "textDocument": { "uri": PROMPT }, "position": { "line": line, "character": character } });
    let run = |client: &mut Client, command: &str, context: Value| {
        client.received.clear();
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
        client.settle();
        result
    };

    // The calendar opens on the old date with the cursor on its day; the
    // answer is asked for only from there.
    let asked = run(&mut client, "org.schedule", at(0, 0)).unwrap();
    assert_eq!(asked, Value::Null);
    assert_eq!(methods(&client.received), ["window/showDocument"]);
    assert_eq!(client.received[0].params["uri"], PROMPT);
    let text = client.view_text(PROMPT);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "          April 2026                May 2026                June 2026");
    assert_eq!(lines[lines.len() - 1], "SCHEDULED Date+time [2026-05-20]: 10:00    => <2026-05-20 Wed 10:00>");
    let cursor = &client.received[0].params["selection"]["start"];
    let (line, character) = (cursor["line"].as_u64().unwrap() as usize, cursor["character"].as_u64().unwrap() as usize);
    assert_eq!(&lines[line][character - 1..=character], "20");
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": PROMPT, "languageId": "org-date", "version": 1, "text": text } }),
    );
    client.settle();

    // A day later, from the cursor: new text, then the cursor on Thursday.
    run(&mut client, "org.calendar.day.later", on_prompt(line as u32, character as u32)).unwrap();
    assert_eq!(methods(&client.received), ["workspace/textDocumentContent/refresh"]);
    let text = client.view_text(PROMPT);
    assert!(text.ends_with("10:00    => <2026-05-21 Thu 10:00>\n"), "{text}");
    client.received.clear();
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": PROMPT, "version": 2 }, "contentChanges": [{ "text": text }] }),
    );
    client.settle();
    assert_eq!(methods(&client.received), ["window/showDocument"]);
    assert_eq!(client.received[0].params["selection"]["start"]["character"], character as u64 + 3);
    // Tokens mark the selected day.
    let tokens =
        client.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": PROMPT } })).unwrap();
    assert!(!tokens["data"].as_array().unwrap().is_empty());

    // Typing: the minibuffer starts from what was typed, and its answer
    // ends with the day that was moved to.
    let asked = run(&mut client, "org.calendar.type", on_prompt(0, 0)).unwrap();
    assert_eq!(asked["input"]["prompt"], "SCHEDULED Date+time [2026-05-20]");
    assert_eq!(asked["input"]["value"], "10:00");
    let mut typed = on_prompt(0, 0);
    typed["input"] = json!("15:00");
    let done = run(&mut client, "org.calendar.type", typed).unwrap();
    // Emacs says what it did, with the repeater the entry keeps.
    assert_eq!(done, json!({ "close": PROMPT, "message": "Scheduled to <2026-05-21 Thu 15:00 +1w>" }));
    let (_, edits) = edits_of(&client.received[0]);
    notes = apply(&notes, &edits);
    assert_eq!(notes.lines().nth(1).unwrap(), "SCHEDULED: <2026-05-21 Thu 15:00 +1w>");
    client.file_version += 1;
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": URI, "version": client.file_version }, "contentChanges": [{ "text": notes }] }),
    );

    // A day picked with RET; months and years move; cancelling closes.
    run(&mut client, "org.deadline", at(0, 0)).unwrap();
    run(&mut client, "org.calendar.month.later", on_prompt(0, 0)).unwrap();
    assert!(client.view_text(PROMPT).contains("=> <2026-06-18 Thu>"));
    run(&mut client, "org.calendar.year.earlier", on_prompt(0, 0)).unwrap();
    assert!(client.view_text(PROMPT).contains("=> <2025-06-18 Wed>"));
    run(&mut client, "org.calendar.today", on_prompt(0, 0)).unwrap();
    let done = run(&mut client, "org.calendar.select", on_prompt(0, 0)).unwrap();
    assert_eq!(done, json!({ "close": PROMPT, "message": "Deadline on <2026-05-18 Mon>" }));
    let (_, edits) = edits_of(&client.received[0]);
    notes = apply(&notes, &edits);
    assert_eq!(notes.lines().nth(1).unwrap(), "DEADLINE: <2026-05-18 Mon> SCHEDULED: <2026-05-21 Thu 15:00 +1w>");
    run(&mut client, "org.timestamp.insert", at(2, 0)).unwrap();
    assert_eq!(run(&mut client, "org.calendar.cancel", on_prompt(0, 0)).unwrap(), json!({ "close": PROMPT }));
    assert_eq!(run(&mut client, "org.calendar.today", on_prompt(0, 0)).unwrap_err(), "No date prompt is open");
    client.shutdown();
}

#[test]
fn names_the_files_to_save() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/b.org", "* b\n");
    client.open("file:///org/a.org", "* a\n");
    let result =
        client.request("workspace/executeCommand", json!({ "command": "org.agenda.save", "arguments": [] })).unwrap();
    assert_eq!(result, json!({ "save": ["file:///org/a.org", "file:///org/b.org"] }));
    client.shutdown();
}

#[test]
fn completes_keywords_and_tags_in_a_headline() {
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/c.org", "* TODO a :work:home:\n* D\n* b :w\n");
    let complete = |client: &mut Client, line: u32, character: u32| {
        let params = json!({ "textDocument": { "uri": "file:///org/c.org" }, "position": { "line": line, "character": character } });
        let items = client.request("textDocument/completion", params).unwrap();
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["label"].clone(), item["textEdit"]["range"]["start"]["character"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(complete(&mut client, 1, 3), [(json!("DONE"), json!(2))]);
    assert_eq!(complete(&mut client, 2, 6), [(json!("work"), json!(5))]);
    assert_eq!(
        client.initialize["capabilities"]["completionProvider"]["triggerCharacters"],
        json!([":", "+", "[", "/", "*", "#", "\\"])
    );
    client.shutdown();
}

/// File names in a link, from the directory of the file, and the
/// headlines of the file named.
#[test]
fn completes_file_names_in_links() {
    let root = workspace("complete-files");
    let uri = format!("file://{}/c.org", root.display());
    let mut client = Client::editor(clock(), None);
    client.open(&uri, "[[file:\n[[file:sub/\n[[file:work.org::*\n");
    let complete = |client: &mut Client, line: u32, character: u32| {
        let params = json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } });
        let items = client.request("textDocument/completion", params).unwrap();
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["textEdit"]["newText"].as_str().unwrap().to_string(), item["kind"].as_u64().unwrap()))
            .collect::<Vec<_>>()
    };
    const FILE: u64 = 17;
    const FOLDER: u64 = 19;
    assert_eq!(
        complete(&mut client, 0, 7),
        [("notes.txt".to_string(), FILE), ("sub/".to_string(), FOLDER), ("work.org".to_string(), FILE)]
    );
    assert_eq!(complete(&mut client, 1, 11), [("home.org".to_string(), FILE)]);
    let headlines: Vec<String> = complete(&mut client, 2, 19).into_iter().map(|(text, _)| text).collect();
    assert_eq!(
        headlines,
        ["Write report", "Call [[https://example.com][the client]]", "Meeting <2026-05-19 Tue 14:00-15:00>"]
    );
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// With `completeTagsAlwaysOfferAllAgendaTags`, a tag is completed from
/// the agenda files too, after the file's own.
#[test]
fn completes_tags_of_the_agenda_files() {
    let root = workspace("tags");
    std::fs::write(root.join("sub/tagged.org"), "#+FILETAGS: :project:\n* a :errand:work:\n").unwrap();
    let mut options = clock();
    options["agendaFiles"] = json!([root.join("sub").display().to_string()]);
    let file = format!("file://{}/c.org", root.display());
    let complete = |client: &mut Client| {
        client.open(&file, "* a :zoo:\n* b :\n");
        let params = json!({ "textDocument": { "uri": file }, "position": { "line": 1, "character": 5 } });
        let items = client.request("textDocument/completion", params).unwrap();
        let mut items = items.as_array().unwrap().clone();
        items.sort_by_key(|item| item["sortText"].as_str().unwrap_or("").to_string());
        items.iter().map(|item| item["label"].as_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    let mut client = Client::editor(options.clone(), None);
    assert_eq!(complete(&mut client), ["zoo"], "the file's own, as Org does by default");
    client.shutdown();
    options["completeTagsAlwaysOfferAllAgendaTags"] = json!(true);
    let mut client = Client::editor(options, None);
    assert_eq!(complete(&mut client), ["zoo", "errand", "project", "work"]);
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `C-c C-s` on an agenda line (`org-agenda-schedule`): the calendar
/// opens on the entry's date, and the day picked there goes to the
/// entry's file, which need not be open in the editor. The agenda's
/// cursor then follows the entry.
#[test]
fn schedules_from_the_agenda() {
    const PROMPT: &str = "org-agenda:/Org%20Date";
    let root = workspace("schedule");
    let mut options = clock();
    options["agendaFiles"] = json!([format!("{}/sub", root.display())]);
    let mut client = Client::with_date_prompt(options);
    client.command("org.agenda.list", VIEW, 0);
    let view = client.show_view();
    let line = view.lines().position(|line| line.contains("Water plants")).unwrap() as u32;

    // Commands that insert a timestamp have no place in the agenda.
    let context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": line, "character": 4 } });
    let refused = client
        .request_raw("workspace/executeCommand", json!({ "command": "org.timestamp.insert", "arguments": [context] }));
    assert!(refused.is_err());
    let header = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } });
    let refused =
        client.request("workspace/executeCommand", json!({ "command": "org.schedule", "arguments": [header] }));
    assert_eq!(refused, Err("No entry on this line".to_string()));

    // What an answer means, for an input box, as in a file.
    let mut preview = context.clone();
    preview["command"] = json!("org.schedule");
    preview["input"] = json!("+2d");
    let shown =
        client.request("workspace/executeCommand", json!({ "command": "org.date.preview", "arguments": [preview] }));
    assert_eq!(shown, Ok(json!({ "timestamp": "<2026-05-20 Wed>" })));

    let requests = client.command("org.schedule", VIEW, line);
    assert_eq!(methods(&requests), ["window/showDocument"]);
    assert_eq!(requests[0].params["uri"], PROMPT);
    let text = client.view_text(PROMPT);
    assert!(text.ends_with("SCHEDULED Date+time [2026-05-16]:    => <2026-05-16 Sat>\n"), "{text}");

    // A week later, and pick it.
    client.command("org.calendar.week.later", PROMPT, 0);
    let requests = client.command("org.calendar.select", PROMPT, 0);
    let edit = requests.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (uri, edits) = edits_of(edit);
    assert_eq!(uri, format!("file://{}/sub/home.org", root.display()));
    assert_eq!(edit.params["edit"]["documentChanges"][0]["textDocument"]["version"], Value::Null);
    let scheduled = apply(HOME, &edits);
    assert_eq!(scheduled, "* TODO Water plants\nSCHEDULED: <2026-05-23 Sat +1w>\n* DONE Buy milk\n");

    // The editor opens the file to apply the edit, and then changes it;
    // the agenda shows the entry on Saturday, and its cursor goes there.
    client.open(&uri, HOME);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": scheduled }] }),
    );
    client.settle();
    client.received.clear();
    let view = client.refetch_view();
    client.settle();
    let saturday = view.lines().position(|line| line.contains("Scheduled:  TODO Water plants")).unwrap();
    assert!(view.lines().nth(saturday - 1).unwrap().starts_with("Saturday   23 May 2026"), "{view}");
    let reveal = client.received.iter().find(|request| request.method == "window/showDocument").expect("a reveal");
    assert_eq!(reveal.params["selection"]["start"]["line"], saturday as u64);
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `j` in the agenda (`org-agenda-goto-date`): the calendar opens on
/// today, and the agenda then shows the week of the day picked there, with
/// the cursor on that day.
#[test]
fn goes_to_a_date_in_the_agenda() {
    const PROMPT: &str = "org-agenda:/Org%20Date";
    let mut client = Client::with_date_prompt(clock());
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    client.show_view();

    let requests = client.command("org.agenda.goto.date", VIEW, 3);
    assert_eq!(methods(&requests), ["window/showDocument"]);
    let text = client.view_text(PROMPT);
    assert!(text.ends_with("\nDate+time [2026-05-18]:    => <2026-05-18 Mon>\n"), "{text}");

    // Ten days on: Thursday of the next week.
    client.command("org.calendar.week.later", PROMPT, 0);
    for _ in 0..3 {
        client.command("org.calendar.day.later", PROMPT, 0);
    }
    client.received.clear();
    let context = json!({ "textDocument": { "uri": PROMPT }, "position": { "line": 0, "character": 0 } });
    let closed =
        client.request("workspace/executeCommand", json!({ "command": "org.calendar.select", "arguments": [context] }));
    assert_eq!(closed, Ok(json!({ "close": PROMPT })));
    client.settle();
    let view = client.refetch_view();
    client.settle();
    assert!(view.starts_with("Week-agenda (W22):\nMonday     25 May 2026 W22\n"), "{view}");
    let thursday = view.lines().position(|line| line.starts_with("Thursday   28 May 2026")).unwrap();
    let reveal =
        client.received.iter().rev().find(|request| request.method == "window/showDocument").expect("a reveal");
    assert_eq!(reveal.params["selection"]["start"]["line"], thursday as u64);

    // Not in the TODO list, and only in the agenda.
    client.command("org.agenda.todos", VIEW, 0);
    let context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } });
    let refused = client
        .request("workspace/executeCommand", json!({ "command": "org.agenda.goto.date", "arguments": [context] }));
    assert_eq!(refused, Err("The TODO list has no dates to move through".to_string()));
    let context =
        json!({ "textDocument": { "uri": "file:///org/work.org" }, "position": { "line": 1, "character": 0 } });
    let refused = client
        .request_raw("workspace/executeCommand", json!({ "command": "org.agenda.goto.date", "arguments": [context] }));
    assert!(refused.is_err());
    client.shutdown();
}

/// `C-c C-t` (`org-todo`) with keys on the keywords: the server asks for
/// a key, and a state that records a note (`@`) asks for its text before
/// it is set. Without keys, the same command cycles.
#[test]
fn selects_todo_states_by_key_and_asks_for_notes() {
    const URI: &str = "file:///org/keys.org";
    const KEYS: &str = "#+TODO: TODO(t) WAIT(w@/!) | DONE(d!)\n* TODO Call them\n";
    let mut client = Client::editor(clock(), None);
    let mut text = KEYS.to_string();
    client.open(URI, KEYS);
    let at = json!({ "textDocument": { "uri": URI }, "position": { "line": 1, "character": 3 } });
    let run = |client: &mut Client, extra: Value| {
        let mut context = at.clone();
        for (key, value) in extra.as_object().into_iter().flatten() {
            context[key] = value.clone();
        }
        client.request("workspace/executeCommand", json!({ "command": "org.todo", "arguments": [context] })).unwrap()
    };

    let asked = run(&mut client, json!({}));
    assert_eq!(asked["input"]["kind"], "key");
    assert_eq!(asked["input"]["prompt"], "Select a TODO state");
    assert_eq!(asked["input"]["choices"][1], json!({ "key": "w", "label": "WAIT" }));

    // WAIT wants a note: nothing changes until its text comes.
    client.received.clear();
    let asked = run(&mut client, json!({ "input": "w" }));
    client.settle();
    assert_eq!(asked["input"]["kind"], "note");
    assert_eq!(asked["input"]["prompt"], "Note for state change from \"TODO\" to \"WAIT\"");
    assert_eq!(asked["input"]["context"], json!({ "state": "WAIT" }));
    assert!(client.received.is_empty(), "no edit yet");
    client.edit("org.todo", URI, (1, 3), json!({ "input": "Waiting for Bob", "state": "WAIT" }), &mut text);
    assert_eq!(
        text,
        "#+TODO: TODO(t) WAIT(w@/!) | DONE(d!)\n* WAIT Call them\n- State \"WAIT\"       from \"TODO\"       [2026-05-18 Mon 10:30] \\\\\n  Waiting for Bob"
    );
    // A key that selects nothing changes nothing.
    client.received.clear();
    assert_eq!(run(&mut client, json!({ "input": "x" })), Value::Null);
    client.settle();
    assert!(client.received.is_empty());

    // Without keys, `org.todo` cycles.
    const PLAIN: &str = "file:///org/plain.org";
    let mut plain = "* TODO Plain\n".to_string();
    client.open(PLAIN, &plain.clone());
    client.edit("org.todo", PLAIN, (0, 0), json!({}), &mut plain);
    assert_eq!(plain, "* DONE Plain\n");
    // A repeating entry records the time of the repeat, and asks nothing.
    const REPEAT: &str = "file:///org/repeat.org";
    let mut repeat = "* TODO Water\nSCHEDULED: <2026-05-16 Sat +1w>\n".to_string();
    client.open(REPEAT, &repeat.clone());
    let requests = client.edit("org.todo", REPEAT, (0, 0), json!({}), &mut repeat);
    assert!(methods(&requests).contains(&"workspace/applyEdit"), "changed at once");
    assert!(repeat.contains("SCHEDULED: <2026-05-23 Sat +1w>"), "{repeat}");
    client.shutdown();
}

/// `C-c C-w` (`org-refile`): the targets come from `refileTargets`; the
/// subtree moves under the one picked, also into a file the editor does
/// not have open. From the agenda every target names its file.
#[test]
fn refiles_entries_between_files() {
    let root = workspace("refile");
    let mut options = clock();
    options["agendaFiles"] = json!([root.display().to_string(), root.join("sub").display().to_string()]);
    options["refileTargets"] = json!([{ "files": "agenda", "level": 1 }]);
    let mut client = Client::editor(options, None);
    let work = format!("file://{}/work.org", root.display());
    client.open(&work, WORK);
    let call = json!({ "textDocument": { "uri": work }, "position": { "line": 3, "character": 4 } });

    let asked =
        client.request("workspace/executeCommand", json!({ "command": "org.refile", "arguments": [call] })).unwrap();
    assert_eq!(asked["input"]["kind"], "choice");
    assert_eq!(asked["input"]["prompt"], "Refile subtree \"Call the client\" to");
    let names: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["label"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "Write report",
            "Call the client",
            "Meeting <2026-05-19 Tue 14:00-15:00>",
            "Water plants (home.org)",
            "Buy milk (home.org)"
        ]
    );

    // home.org is not open in the editor.
    let home = format!("file://{}/sub/home.org", root.display());
    let mut answer = call.clone();
    answer["input"] = json!("Water plants (home.org)");
    client.received.clear();
    client.request("workspace/executeCommand", json!({ "command": "org.refile", "arguments": [answer] })).unwrap();
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let changes = edit.params["edit"]["documentChanges"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(
        (changes[0]["textDocument"]["uri"].as_str(), changes[0]["textDocument"]["version"].as_i64()),
        (Some(work.as_str()), Some(1))
    );
    assert_eq!(
        (changes[1]["textDocument"]["uri"].as_str(), changes[1]["textDocument"]["version"].as_i64()),
        (Some(home.as_str()), None)
    );
    assert_eq!(
        apply(WORK, changes[0]["edits"].as_array().unwrap()),
        "#+CATEGORY: work\n* TODO [#A] Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n* Meeting <2026-05-19 Tue 14:00-15:00>\n"
    );
    assert_eq!(
        apply(HOME, changes[1]["edits"].as_array().unwrap()),
        "* TODO Water plants\nSCHEDULED: <2026-05-16 Sat +1w>\n** TODO Call [[https://example.com][the client]]\nDEADLINE: <2026-05-20 Wed>\n* DONE Buy milk\n"
    );

    // From the TODO list: no file is the current one.
    client.command("org.agenda.todos", VIEW, 0);
    let view = client.view_text(VIEW);
    let line = view.lines().position(|line| line.contains("Water plants")).unwrap();
    let at = json!({ "textDocument": { "uri": VIEW }, "position": { "line": line, "character": 0 } });
    let asked =
        client.request("workspace/executeCommand", json!({ "command": "org.refile", "arguments": [at] })).unwrap();
    assert_eq!(asked["input"]["prompt"], "Refile to");
    assert_eq!(asked["input"]["choices"][0]["label"], "Write report (work.org)");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// Run a command at `at` of `uri`, apply the edit it asks for to `text`
/// as an editor would, and return its result.
fn run_edit(client: &mut Client, command: &str, uri: &str, at: (u32, u32), extra: Value, text: &mut String) -> Value {
    client.received.clear();
    let mut context = json!({ "textDocument": { "uri": uri }, "position": { "line": at.0, "character": at.1 } });
    for (key, value) in extra.as_object().into_iter().flatten() {
        context[key] = value.clone();
    }
    let result = client
        .request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
        .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").cloned();
    if let Some(request) = edit {
        let (file, edits) = edits_of(&request);
        *text = apply(text, &edits);
        client.file_version += 1;
        client.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": file, "version": client.file_version }, "contentChanges": [{ "text": text }] }),
        );
        client.settle();
    }
    result
}

/// `C-c C-x C-s` (`org-archive-subtree`): the subtree goes to the archive
/// file, which is written when no editor has it open, and `$` does the
/// same from the agenda. `C-c C-x a` tags a subtree as archived, and
/// `C-u C-c C-x C-s` asks about each subtree without open TODO items.
#[test]
fn archives_subtrees() {
    let root = workspace("archive");
    let mut options = clock();
    options["agendaFiles"] = json!([root.display().to_string()]);
    let mut client = Client::editor(options, None);
    let work = format!("file://{}/work.org", root.display());
    let mut text = WORK.to_string();
    client.open(&work, &text);

    let result = run_edit(&mut client, "org.archive.subtree", &work, (3, 4), json!({}), &mut text);
    let archive = root.join("work.org_archive");
    assert_eq!(result, json!({ "message": format!("Subtree archived in file: {}", archive.display()) }));
    assert_eq!(
        text,
        "#+CATEGORY: work\n* TODO [#A] Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n* Meeting <2026-05-19 Tue 14:00-15:00>\n"
    );
    assert_eq!(
        std::fs::read_to_string(&archive).unwrap(),
        format!(
            "#    -*- mode: org -*-\n\n\nArchived entries from file {}/work.org\n\n\n\
             * TODO Call [[https://example.com][the client]]\nDEADLINE: <2026-05-20 Wed>\n:PROPERTIES:\n\
             :ARCHIVE_TIME: 2026-05-18 Mon 10:30\n:ARCHIVE_FILE: {}/work.org\n:ARCHIVE_CATEGORY: work\n\
             :ARCHIVE_TODO: TODO\n:END:\n",
            root.display(),
            root.display()
        )
    );

    // `$` on an agenda line.
    client.command("org.agenda.todos", VIEW, 0);
    let view = client.show_view();
    let line = view.lines().position(|line| line.contains("Write report")).unwrap() as u32;
    run_edit(&mut client, "org.archive.subtree", VIEW, (line, 0), json!({}), &mut text);
    assert_eq!(text, "#+CATEGORY: work\n* Meeting <2026-05-19 Tue 14:00-15:00>\n");
    assert!(std::fs::read_to_string(&archive).unwrap().contains("\n* TODO [#A] Write report :office:\n"));
    assert!(!client.refetch_view().contains("Write report"));

    // The tag, on and off.
    let result = run_edit(&mut client, "org.archive.tag", &work, (1, 0), json!({}), &mut text);
    assert_eq!(result, json!({ "message": "Subtree archived" }));
    assert!(text.lines().nth(1).unwrap().ends_with(" :ARCHIVE:"), "{text}");
    run_edit(&mut client, "org.archive.tag", &work, (1, 0), json!({}), &mut text);
    assert_eq!(text, "#+CATEGORY: work\n* Meeting <2026-05-19 Tue 14:00-15:00>\n");

    // One question at a time, the answers so far in the context.
    let done = format!("file://{}/done.org", root.display());
    let mut text = "Intro\n* DONE A\n* TODO B\n* DONE C\n".to_string();
    client.open(&done, &text);
    let asked = run_edit(&mut client, "org.archive.done", &done, (0, 0), json!({}), &mut text);
    assert_eq!(asked["input"]["prompt"], "Move subtree to archive? (no open TODO items)");
    assert_eq!(asked["input"]["context"], json!({ "answers": "" }));
    let asked =
        run_edit(&mut client, "org.archive.done", &done, (0, 0), json!({ "input": "n", "answers": "" }), &mut text);
    assert_eq!(asked["input"]["context"], json!({ "answers": "n" }));
    let result =
        run_edit(&mut client, "org.archive.done", &done, (0, 0), json!({ "input": "y", "answers": "n" }), &mut text);
    assert_eq!(result, json!({ "message": "1 trees archived" }));
    assert_eq!(text, "Intro\n* DONE A\n* TODO B\n");
    assert!(std::fs::read_to_string(root.join("done.org_archive")).unwrap().contains("* DONE C\n"));
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// A client that runs no command from a key and does nothing with a
/// command's result (Zed): with `commandActions` the commands are code
/// actions, the agenda among them, and with `messagePrompts` what they ask
/// is asked with `window/showMessageRequest`, a button a choice.
#[test]
fn offers_commands_as_actions_and_asks_with_buttons() {
    let mut options = clock();
    options["todoKeywords"] = json!(["TODO(t)", "WAIT(w)", "|", "DONE(d)"]);
    options["commandActions"] = json!(true);
    options["messagePrompts"] = json!(true);
    options["virtualDocuments"] = json!(false);
    let state = std::env::temp_dir().join(format!("org-lsp-state-{}", std::process::id()));
    let script_path = state.join("org-lsp").join("agenda-command");
    options["commandScript"] = json!(script_path);
    let capabilities = json!({
        "workspace": { "workspaceEdit": { "documentChanges": true } },
        "window": { "showDocument": { "support": true } },
        "general": { "positionEncodings": ["utf-8"] }
    });
    let mut client = Client::start(capabilities, options, None);
    // The server follows the cursor through document highlights.
    assert_eq!(client.initialize["capabilities"]["documentHighlightProvider"], true);
    const WORK_URI: &str = "file:///org/work.org";
    let mut text = WORK.to_string();
    client.open(WORK_URI, &text);
    let kinds = |client: &mut Client, uri: &str, line: u32, only: Value| -> Vec<String> {
        let actions = client
            .request(
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": { "start": { "line": line, "character": 0 }, "end": { "line": line, "character": 0 } },
                    "context": { "diagnostics": [], "only": only }
                }),
            )
            .unwrap();
        actions.as_array().unwrap().iter().map(|action| action["kind"].as_str().unwrap().to_string()).collect()
    };
    let offered = kinds(&mut client, WORK_URI, 1, json!(["org"]));
    for kind in ["org.todo", "org.priority", "org.refile", "org.agenda.list", "org.agenda.todos", "org.capture"] {
        assert!(offered.iter().any(|offered| offered == kind), "{kind} in {offered:?}");
    }
    // The empty kind is above all the others.
    assert_eq!(kinds(&mut client, WORK_URI, 1, json!(["", "quickfix"])), offered);

    // `org.todo` asks with buttons, and the one picked runs it again.
    client.choice = Some("WAIT [w]".to_string());
    client.received.clear();
    let context = json!({ "textDocument": { "uri": WORK_URI }, "position": { "line": 1, "character": 0 } });
    client.request("workspace/executeCommand", json!({ "command": "org.todo", "arguments": [context] })).unwrap();
    client.settle();
    let asked = client.received.iter().find(|request| request.method == "window/showMessageRequest").expect("asked");
    let titles: Vec<&str> =
        asked.params["actions"].as_array().unwrap().iter().map(|action| action["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"WAIT [w]") && titles.contains(&"DONE [d]"), "{titles:?}");
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    text = apply(&text, &edits);
    assert!(text.lines().nth(1).unwrap().starts_with("* WAIT [#A] Write report"), "{text}");

    // What a command has to say comes as a message.
    client.notifications.clear();
    client
        .request("workspace/executeCommand", json!({ "command": "org.priority.down", "arguments": [context] }))
        .unwrap();
    client.settle();
    let said: Vec<&str> = client
        .notifications
        .iter()
        .filter(|notification| notification.method == "window/showMessage")
        .filter_map(|notification| notification.params["message"].as_str())
        .collect();
    assert_eq!(said, ["Priority of current item set to B"]);

    // The agenda, from a code action, is a file shown to the editor; on its
    // lines the actions move it through time too.
    client.received.clear();
    client
        .request("workspace/executeCommand", json!({ "command": "org.agenda.list", "arguments": [context] }))
        .unwrap();
    client.settle();
    let shown = client.received.iter().find(|request| request.method == "window/showDocument").expect("shown");
    let agenda = shown.params["uri"].as_str().unwrap().to_string();
    assert!(agenda.ends_with("Agenda.org-agenda"), "{agenda}");
    let agenda_text = std::fs::read_to_string(agenda.trim_start_matches("file://")).unwrap();
    client.open(&agenda, &agenda_text);
    // The agenda's file is read-only, where Zed runs no code action: its
    // commands come through the command port, whose script is at a place
    // the keys can name.
    let agenda_path = std::path::PathBuf::from(agenda.trim_start_matches("file://"));
    assert!(std::fs::metadata(&agenda_path).unwrap().permissions().readonly());
    assert!(kinds(&mut client, &agenda, 0, json!(["org"])).is_empty());
    let script = std::fs::read_to_string(&script_path).unwrap();
    let words: Vec<&str> = script.lines().last().unwrap().split_whitespace().collect();
    let at = words.iter().position(|word| *word == "command").unwrap();
    let (address, token) = (words[at + 1].to_string(), words[at + 2].to_string());
    // Without a row, a command acts where the cursor was last seen: here
    // on the entry, which `org.agenda.goto` shows.
    let row = agenda_text.lines().position(|line| line.contains("Write report")).unwrap();
    let highlights = client
        .request(
            "textDocument/documentHighlight",
            json!({ "textDocument": { "uri": agenda }, "position": { "line": row, "character": 3 } }),
        )
        .unwrap();
    assert_eq!(highlights, Value::Null);
    client.received.clear();
    org_lsp::port::send(&[address.clone(), token.clone(), "org.agenda.goto".into()]).unwrap();
    let mut shown = None;
    for _ in 0..200 {
        client.settle();
        shown = client.received.iter().find(|request| request.method == "window/showDocument").cloned();
        if shown.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(shown.expect("the entry shown").params["uri"], WORK_URI);
    client.received.clear();
    org_lsp::port::send(&[address, token, "org.agenda.later".into(), "1".into(), "1".into()]).unwrap();
    // The port's command comes on a channel of its own: wait for it.
    let mut later = agenda_text.clone();
    for _ in 0..200 {
        client.settle();
        later = std::fs::read_to_string(&agenda_path).unwrap();
        if later != agenda_text {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_ne!(later, agenda_text);
    assert!(later.starts_with("Week-agenda (W22):"), "{later}");

    // A question that wants typed text cannot be asked: the editor is told.
    client.notifications.clear();
    client.request("workspace/executeCommand", json!({ "command": "org.deadline", "arguments": [context] })).unwrap();
    client.settle();
    assert!(client.notifications.iter().any(|notification| notification.method == "window/showMessage"
        && notification.params["message"].as_str().unwrap().contains("cannot ask for text")));

    // The keys that insert a heading are actions too, which the server
    // applies, the cursor then moved to the new heading.
    let offered = kinds(&mut client, WORK_URI, 3, json!(["org"]));
    assert!(offered.iter().any(|kind| kind == "org.insert.heading.after"), "{offered:?}");
    client.open(WORK_URI, WORK);
    client.received.clear();
    let applied =
        json!({ "textDocument": { "uri": WORK_URI }, "position": { "line": 1, "character": 3 }, "apply": true });
    client
        .request("workspace/executeCommand", json!({ "command": "org.insert.heading.after", "arguments": [applied] }))
        .unwrap();
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    let inserted = apply(WORK, &edits);
    assert_eq!(inserted.lines().nth(3), Some("* "), "{inserted}");
    let shown = client.received.iter().find(|request| request.method == "window/showDocument").expect("moved");
    assert_eq!(shown.params["selection"]["start"], json!({ "line": 3, "character": 2 }));
    client.shutdown();
    // The script goes with the server.
    assert!(!script_path.exists());
    std::fs::remove_dir_all(state).unwrap();
}

/// The shared configuration file's options apply, under the editor's, and
/// a problem with it is shown to the user.
#[test]
fn reads_the_shared_configuration_file() {
    let root = workspace("config-file");
    let config = root.join("config.toml");
    std::fs::write(&config, "todoKeywords = [\"TODO\", \"WAIT\", \"|\", \"DONE\"]\ntagsColumn = \"far\"\n").unwrap();
    let mut client = Client::editor(json!({ "configFile": config.display().to_string() }), None);
    client.settle();
    let shown: Vec<&str> = client
        .notifications
        .iter()
        .filter(|notification| notification.method == "window/showMessage")
        .filter_map(|notification| notification.params["message"].as_str())
        .collect();
    assert_eq!(shown, ["org-lsp: the option `tagsColumn` is malformed and is ignored"]);
    // The file's keywords are the ones in use.
    let uri = format!("file://{}", root.join("a.org").display());
    client.open(&uri, "* TODO Call\n");
    let context = json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 } });
    client.request("workspace/executeCommand", json!({ "command": "org.todo.next", "arguments": [context] })).unwrap();
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    assert_eq!(apply("* TODO Call\n", &edits), "* WAIT Call\n");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// Org's default template captures into `defaultNotesFile`, `~/.notes`
/// unless set. The capture buffer is still an Org file by its name.
#[test]
fn the_capture_buffer_of_a_target_without_org_in_its_name_is_an_org_file() {
    let root = workspace("capture-notes");
    let notes = root.join(".notes");
    let mut options = clock();
    options["defaultNotesFile"] = json!(notes.display().to_string());
    let mut client = Client::editor(options, None);
    let at = json!({ "textDocument": { "uri": format!("file://{}", root.join("a.org").display()) }, "position": { "line": 0, "character": 0 } });
    let mut context = at.clone();
    context["input"] = json!("t");
    client.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [context] })).unwrap();
    client.settle();
    let show = client.received.iter().find(|request| request.method == "window/showDocument").expect("the buffer");
    let buffer = show.params["uri"].as_str().unwrap().to_string();
    assert!(buffer.ends_with("/CAPTURE-.notes.org"), "{buffer}");
    let path = buffer.strip_prefix("file://").unwrap();
    client.open(&buffer, &std::fs::read_to_string(path).unwrap());
    let done = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.capture.finish", "arguments": [{ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } }] }),
    );
    assert_eq!(done, Ok(json!({ "close": buffer })));
    client.settle();
    // The target did not exist: the server wrote it, under its own name.
    assert!(std::fs::read_to_string(&notes).unwrap().starts_with("* Tasks\n** TODO "));
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// A template without a target captures to the end of `defaultNotesFile`.
#[test]
fn a_template_without_a_target_captures_into_the_default_notes_file() {
    let root = workspace("capture-no-target");
    let notes = root.join("notes.org");
    std::fs::write(&notes, "* Old\n").unwrap();
    let mut options = clock();
    options["defaultNotesFile"] = json!(notes.display().to_string());
    options["captureTemplates"] =
        json!([{ "key": "n", "description": "Note", "template": "* Quick note", "immediateFinish": true }]);
    let mut client = Client::editor(options, None);
    let at = json!({ "textDocument": { "uri": format!("file://{}", notes.display()) }, "position": { "line": 0, "character": 0 }, "input": "n" });
    let done = client.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [at] }));
    assert_eq!(done, Ok(Value::Null));
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (file, edits) = edits_of(edit);
    assert_eq!(file, format!("file://{}", notes.display()));
    assert_eq!(apply("* Old\n", &edits), "* Old\n* Quick note\n");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// Capturing from where there is no document (no file open): the
/// template and the answers come without a cursor.
#[test]
fn captures_from_no_document() {
    let root = workspace("capture-no-document");
    let notes = root.join("notes.org");
    std::fs::write(&notes, "* Old\n").unwrap();
    let mut options = clock();
    options["defaultNotesFile"] = json!(notes.display().to_string());
    options["captureTemplates"] = json!([{ "key": "n", "description": "Note", "template": "* %^{Title} %?" }]);
    let mut client = Client::editor(options, None);
    let run = |client: &mut Client, arguments: Value| {
        client.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": arguments }))
    };
    let asked = run(&mut client, json!([])).unwrap();
    assert_eq!(asked["input"]["choices"], json!([{ "key": "n", "label": "Note" }]));
    let asked = run(&mut client, json!([{ "keys": "", "input": "n" }])).unwrap();
    assert_eq!(asked["input"]["prompt"], "Title");
    let mut context = asked["input"]["context"].clone();
    context["input"] = json!("Groceries");
    client.received.clear();
    assert_eq!(run(&mut client, json!([context])), Ok(Value::Null));
    client.settle();
    let show = client.received.iter().find(|request| request.method == "window/showDocument").expect("the buffer");
    let buffer = show.params["uri"].as_str().unwrap();
    assert_eq!(std::fs::read_to_string(buffer.strip_prefix("file://").unwrap()).unwrap(), "* Groceries \n");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `org.capture.finish` with the buffer's text: a client may tell the
/// server of the last changes only after the command (coc.nvim waits),
/// and the text it sends is what gets filed. Text sent from another
/// buffer is not taken for the capture buffer's.
#[test]
fn files_the_capture_buffer_text_the_command_comes_with() {
    let root = workspace("capture-text");
    let notes = root.join("notes.org");
    std::fs::write(&notes, "* Old\n").unwrap();
    let mut options = clock();
    options["defaultNotesFile"] = json!(notes.display().to_string());
    options["captureTemplates"] = json!([{ "key": "n", "description": "Note", "template": "* %?" }]);
    let mut client = Client::editor(options, None);
    let notes_uri = format!("file://{}", notes.display());
    client.open(&notes_uri, "* Old\n");
    let at = json!({ "textDocument": { "uri": notes_uri }, "position": { "line": 0, "character": 0 }, "input": "n" });
    client.received.clear();
    client.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [at] })).unwrap();
    client.settle();
    let show = client.received.iter().find(|request| request.method == "window/showDocument").expect("the buffer");
    let buffer = show.params["uri"].as_str().unwrap().to_string();
    let shown = std::fs::read_to_string(buffer.strip_prefix("file://").unwrap()).unwrap();
    client.open(&buffer, &shown);
    let finish = |client: &mut Client, uri: &str, text: &str| {
        client.received.clear();
        let context =
            json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 }, "text": text });
        client.request("workspace/executeCommand", json!({ "command": "org.capture.finish", "arguments": [context] }))
    };
    // No change of the buffer has reached the server; the text has it.
    finish(&mut client, &buffer, "* Call mom\n").unwrap();
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    assert_eq!(apply("* Old\n", &edits), "* Old\n* Call mom\n");

    // From another buffer, its text is not the capture's.
    let at = json!({ "textDocument": { "uri": notes_uri }, "position": { "line": 0, "character": 0 }, "input": "n" });
    client.received.clear();
    client.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [at] })).unwrap();
    client.settle();
    finish(&mut client, &notes_uri, "* Not this\n").unwrap();
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    assert!(!apply("* Old\n", &edits).contains("Not this"));
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `org-capture`: a template picked by its keys, a question answered, the
/// capture buffer shown as a file of its own, and its text put under the
/// target headline when the capture finishes.
#[test]
fn captures_into_a_file() {
    let root = workspace("capture");
    let notes = root.join("notes.org");
    std::fs::write(&notes, "* Tasks\n** TODO Old\n\n* Ideas\n").unwrap();
    let mut options = clock();
    options["captureTemplates"] = json!([
        { "key": "t", "description": "Task", "target": ["file+headline", notes.display().to_string(), "Tasks"], "template": "* TODO %?\n%U" },
        { "key": "m", "description": "Meetings" },
        { "key": "mx", "description": "Meeting", "target": ["file", "notes.org"], "template": ["* With %^{Who} %?"] }
    ]);
    options["orgDirectory"] = json!(root.display().to_string());
    let mut client = Client::editor(options, None);
    let run = |client: &mut Client, command: &str, context: Value| {
        client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }))
    };
    let at = json!({ "textDocument": { "uri": format!("file://{}", notes.display()) }, "position": { "line": 0, "character": 0 } });

    let asked = run(&mut client, "org.capture", at.clone()).unwrap();
    assert_eq!(asked["input"]["kind"], "key");
    assert_eq!(
        asked["input"]["choices"],
        json!([{ "key": "t", "label": "Task" }, { "key": "m", "label": "Meetings..." }])
    );
    let mut context = at.clone();
    context["keys"] = asked["input"]["context"]["keys"].clone();
    context["input"] = json!("m");
    let asked = run(&mut client, "org.capture", context.clone()).unwrap();
    assert_eq!(asked["input"]["choices"], json!([{ "key": "x", "label": "Meeting" }]));
    context["keys"] = json!("m");
    context["input"] = json!("x");
    let asked = run(&mut client, "org.capture", context.clone()).unwrap();
    assert_eq!((asked["input"]["kind"].as_str(), asked["input"]["prompt"].as_str()), (Some("text"), Some("Who")));
    for (key, value) in asked["input"]["context"].as_object().unwrap() {
        context[key] = value.clone();
    }
    context["input"] = json!("Bob");
    client.received.clear();
    assert_eq!(run(&mut client, "org.capture", context).unwrap(), Value::Null);
    client.settle();
    let show = client.received.iter().find(|request| request.method == "window/showDocument").expect("the buffer");
    let buffer = show.params["uri"].as_str().unwrap().to_string();
    assert!(buffer.ends_with("/CAPTURE-notes.org"), "{buffer}");
    assert_eq!(show.params["selection"]["start"], json!({ "line": 0, "character": 11 }));
    let path = buffer.strip_prefix("file://").unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "* With Bob \n");

    // The editor opens the buffer and the user types; finishing puts it
    // at the end of the file.
    client.open(&buffer, "* With Bob \n");
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": buffer, "version": 2 }, "contentChanges": [{ "text": "* With Bob about lunch\n" }] }),
    );
    client.received.clear();
    let done = run(
        &mut client,
        "org.capture.finish",
        json!({ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } }),
    );
    assert_eq!(done, Ok(json!({ "close": buffer })));
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (file, edits) = edits_of(edit);
    assert_eq!(file, format!("file://{}", notes.display()));
    // A blank line before it, as before "Ideas" (`org-blank-before-new-entry`).
    assert_eq!(
        apply("* Tasks\n** TODO Old\n\n* Ideas\n", &edits),
        "* Tasks\n** TODO Old\n\n* Ideas\n\n* With Bob about lunch\n"
    );

    // A second capture, aborted: nothing changes.
    let mut context = at.clone();
    context["input"] = json!("t");
    client.received.clear();
    run(&mut client, "org.capture", context).unwrap();
    client.settle();
    let buffer =
        client.received.iter().find(|request| request.method == "window/showDocument").unwrap().params["uri"].clone();
    assert_eq!(
        std::fs::read_to_string(buffer.as_str().unwrap().strip_prefix("file://").unwrap()).unwrap(),
        "** TODO \n[2026-05-18 Mon 10:30]"
    );
    client.received.clear();
    let aborted = run(
        &mut client,
        "org.capture.abort",
        json!({ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } }),
    );
    assert_eq!(
        aborted,
        Ok(json!({ "close": buffer, "message": "Capture process aborted and target buffer cleaned up" }))
    );
    client.settle();
    assert!(client.received.iter().all(|request| request.method != "workspace/applyEdit"));

    // Refiled from the capture buffer: the entry goes under a refile target
    // of its file instead (`org-capture-refile`).
    let mut context = at.clone();
    context["input"] = json!("t");
    client.received.clear();
    run(&mut client, "org.capture", context).unwrap();
    client.settle();
    let buffer =
        client.received.iter().find(|request| request.method == "window/showDocument").unwrap().params["uri"].clone();
    let in_buffer = json!({ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } });
    let asked = run(&mut client, "org.capture.refile", in_buffer.clone()).unwrap();
    // A headline with only a keyword has no title.
    assert_eq!(asked["input"]["prompt"], "Refile subtree \"\" to");
    let names: Vec<&str> =
        asked["input"]["choices"].as_array().unwrap().iter().map(|choice| choice["key"].as_str().unwrap()).collect();
    assert_eq!(names, ["Tasks", "Ideas"]);
    let mut answer = in_buffer.clone();
    answer["input"] = json!("Ideas");
    client.received.clear();
    let refiled = run(&mut client, "org.capture.refile", answer).unwrap();
    assert_eq!(refiled["close"], buffer);
    let said = format!("Refile to \"Ideas\" in file {}: done", notes.display());
    assert_eq!(refiled["message"], said);
    client.settle();
    let edit = client.received.iter().find(|request| request.method == "workspace/applyEdit").expect("an edit");
    let (_, edits) = edits_of(edit);
    assert_eq!(
        apply("* Tasks\n** TODO Old\n\n* Ideas\n", &edits),
        // Placed under "Tasks" first, which took the blank line, as in Emacs.
        "* Tasks\n** TODO Old\n* Ideas\n** TODO \n[2026-05-18 Mon 10:30]\n"
    );
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `org-open-at-point` (`org.open`): an internal link moves the cursor in
/// its file, a URL goes to the system, a `file:` link and an `id:` link
/// open the other file at their place, a timestamp opens the agenda of its
/// day, the links of an entry are offered by key; go to definition answers
/// for links too, and on an agenda line the links of its entry open.
#[test]
fn opens_links() {
    let root = std::env::temp_dir().join(format!("org-lsp-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let notes = "* Notes\n* Two\n:PROPERTIES:\n:ID: two-id\n:END:\n";
    std::fs::write(root.join("notes.org"), notes).unwrap();
    let text = "* TODO Read [[https://orgmode.org][the manual]]\nSCHEDULED: <2026-05-19 Tue>\n\
                See [[*Target]], [[file:notes.org::*Two]] and [[id:two-id]].\n* Target\n";
    std::fs::write(root.join("work.org"), text).unwrap();
    let root_uri = format!("file://{}", root.display());
    let work = format!("{root_uri}/work.org");
    let mut client = Client::editor(clock(), Some(&root_uri));
    client.open(&work, text);
    let open = |client: &mut Client, line: u32, character: u32, input: Option<&str>| {
        client.received.clear();
        let mut context =
            json!({ "textDocument": { "uri": work }, "position": { "line": line, "character": character } });
        if let Some(input) = input {
            context["input"] = json!(input);
        }
        let result =
            client.request("workspace/executeCommand", json!({ "command": "org.open", "arguments": [context] }));
        client.settle();
        let shown = client
            .received
            .iter()
            .find(|request| request.method == "window/showDocument")
            .map(|request| request.params.clone());
        (result, shown)
    };
    // `[[*Target]]`: the headline, in the same file.
    let (_, shown) = open(&mut client, 2, 6, None);
    let shown = shown.unwrap();
    assert_eq!(shown["uri"], work);
    assert_eq!(shown["selection"]["start"], json!({ "line": 3, "character": 0 }));
    // `[[file:notes.org::*Two]]` and `[[id:two-id]]`: the other file.
    for character in [20, 47] {
        let (_, shown) = open(&mut client, 2, character, None);
        let shown = shown.unwrap();
        assert_eq!(shown["uri"], format!("{root_uri}/notes.org"));
        assert_eq!(shown["selection"]["start"], json!({ "line": 1, "character": 0 }));
    }
    // A URL is for the system.
    let (_, shown) = open(&mut client, 0, 20, None);
    let shown = shown.unwrap();
    assert_eq!(shown["uri"], "https://orgmode.org");
    assert_eq!(shown["external"], true);
    // A timestamp: the agenda of its day.
    let (_, shown) = open(&mut client, 1, 14, None);
    assert_eq!(shown.unwrap()["uri"], VIEW);
    assert!(client.view_text(VIEW).starts_with("Day-agenda (W21):\nTuesday    19 May 2026\n"));
    // The links of the entry, offered by key, and the one picked opened.
    let (result, _) = open(&mut client, 0, 2, None);
    let input = &result.unwrap()["input"];
    assert_eq!(input["kind"], "key");
    let labels: Vec<&str> =
        input["choices"].as_array().unwrap().iter().map(|choice| choice["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["the manual (https://orgmode.org)", "*Target", "file:notes.org::*Two", "id:two-id"]);
    let (_, shown) = open(&mut client, 0, 2, Some("2"));
    assert_eq!(shown.unwrap()["selection"]["start"], json!({ "line": 3, "character": 0 }));
    // Go to definition on a link.
    let found = client
        .request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": work }, "position": { "line": 2, "character": 6 } }),
        )
        .unwrap();
    assert_eq!(found["range"]["start"], json!({ "line": 3, "character": 0 }));
    // On an agenda line (`org-agenda-open-link`): the links of its entry.
    client.command("org.agenda.list", VIEW, 0);
    let agenda = client.view_text(VIEW);
    let line = agenda.lines().position(|line| line.contains("Read the manual")).unwrap() as u32;
    let result = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.open", "arguments": [{ "textDocument": { "uri": VIEW }, "position": { "line": line, "character": 0 } }] }),
    );
    assert_eq!(result.unwrap()["input"]["choices"].as_array().unwrap().len(), 4);
    // On a planning line, but not on its timestamp: nothing to open.
    let refused = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.open", "arguments": [{ "textDocument": { "uri": work }, "position": { "line": 1, "character": 3 } }] }),
    );
    assert_eq!(refused.unwrap_err(), "No link found");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// `org-add-note` (`org.add.note`): asks for the note, then writes it at
/// the start of the entry's log, headed `Note taken on`; from an agenda
/// line too (`org-agenda-add-note`).
#[test]
fn adds_a_note() {
    const URI: &str = "file:///org/work.org";
    let mut text = WORK.to_string();
    let mut client = Client::editor(clock(), None);
    client.open(URI, &text);
    let at = json!({ "textDocument": { "uri": URI }, "position": { "line": 1, "character": 0 } });
    let asked =
        client.request("workspace/executeCommand", json!({ "command": "org.add.note", "arguments": [at] })).unwrap();
    assert_eq!(asked["input"]["kind"], "note");
    assert_eq!(asked["input"]["prompt"], "Note for this entry");
    let requests =
        client.edit("org.add.note", URI, (1, 0), json!({ "input": "Asked for the numbers", "state": "" }), &mut text);
    assert!(requests.iter().any(|request| request.method == "workspace/applyEdit"));
    assert!(
        text.starts_with(
            "#+CATEGORY: work\n* TODO [#A] Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n\
             - Note taken on [2026-05-18 Mon 10:30] \\\\\n  Asked for the numbers\n"
        ),
        "{text}"
    );
    // Before the first headline there is no entry.
    let refused = client.request(
        "workspace/executeCommand",
        json!({ "command": "org.add.note", "arguments": [{ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 0 } }] }),
    );
    assert_eq!(refused.unwrap_err(), "Before first headline");
    // From the agenda, the line's entry.
    client.command("org.agenda.list", VIEW, 0);
    let agenda = client.view_text(VIEW);
    let line = agenda.lines().position(|line| line.contains("Call")).unwrap() as u32;
    let requests =
        client.edit("org.add.note", VIEW, (line, 0), json!({ "input": "From the agenda", "state": "" }), &mut text);
    assert!(requests.iter().any(|request| request.method == "workspace/applyEdit"));
    assert!(
        text.contains("DEADLINE: <2026-05-20 Wed>\n- Note taken on [2026-05-18 Mon 10:30] \\\\\n  From the agenda\n"),
        "{text}"
    );
    client.shutdown();
}

/// `org-add-note` in an editor with note buffers: the note is written
/// there, stored when finished, and not at all when cancelled.
#[test]
fn adds_a_note_from_a_note_buffer() {
    let mut client = Client::with_date_prompt(clock());
    let open = |client: &mut Client, file: &str| {
        client.open(file, "* Call\n");
        client.received.clear();
        let context = json!({ "textDocument": { "uri": file }, "position": { "line": 0, "character": 2 } });
        client
            .request("workspace/executeCommand", json!({ "command": "org.add.note", "arguments": [context] }))
            .unwrap();
        client.settle();
        let show = client.received.iter().find(|request| request.method == "window/showDocument").expect("the buffer");
        let buffer = show.params["uri"].as_str().unwrap().to_string();
        let text = std::fs::read_to_string(buffer.strip_prefix("file://").unwrap()).unwrap();
        assert_eq!(text, "# Insert note for this entry.\n\n\n");
        buffer
    };
    let finish = |client: &mut Client, command: &str, buffer: &str| {
        client.received.clear();
        let context = json!({ "textDocument": { "uri": buffer }, "position": { "line": 0, "character": 0 } });
        let result =
            client.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] })).unwrap();
        assert_eq!(result["close"], buffer);
        client.settle();
        client
            .received
            .iter()
            .find(|request| request.method == "workspace/applyEdit")
            .map(|edit| apply("* Call\n", &edits_of(edit).1))
    };
    let buffer = open(&mut client, "file:///org/a.org");
    client.open(&buffer, "# Insert note for this entry.\n\nRang twice\n");
    assert_eq!(
        finish(&mut client, "org.capture.finish", &buffer).unwrap(),
        // As Emacs leaves it: the note at the end of the file, without a
        // newline after it.
        "* Call\n- Note taken on [2026-05-18 Mon 10:30] \\\\\n  Rang twice"
    );
    let buffer = open(&mut client, "file:///org/b.org");
    assert_eq!(finish(&mut client, "org.capture.abort", &buffer), None);
    client.shutdown();
}

/// `useTagInheritance` and `usePropertyInheritance` reach the outlines of
/// open documents and of files read from disk alike.
#[test]
fn inheritance_options_apply_to_open_and_read_files() {
    let root = workspace("inheritance");
    let text = "* Projects :work:\n:PROPERTIES:\n:OWNER: alice\n:END:\n** TODO Child\n";
    std::fs::write(root.join("disk.org"), text).unwrap();
    let mut options = clock();
    options["agendaFiles"] = json!([root.display().to_string()]);
    options["useTagInheritance"] = json!(false);
    options["usePropertyInheritance"] = json!(["OWNER"]);
    let mut client = Client::editor(options, None);
    let open = format!("file://{}/open.org", root.display());
    std::fs::write(root.join("open.org"), "").unwrap();
    client.open(&open, &text.replace("Child", "Open child"));
    let tags_view = |client: &mut Client, matching: &str| {
        let context = json!({ "textDocument": { "uri": VIEW }, "position": { "line": 0, "character": 0 },
            "keys": "m", "ask": "match", "input": matching });
        client
            .request("workspace/executeCommand", json!({ "command": "org.agenda.dispatch", "arguments": [context] }))
            .unwrap();
        client.settle();
        client.view_text(VIEW)
    };
    // The children do not get `work`, but do get OWNER.
    let text = tags_view(&mut client, "work");
    assert_eq!(text.matches("Projects").count(), 2, "{text}");
    assert!(!text.contains("Child") && !text.contains("child"), "{text}");
    let text = tags_view(&mut client, "OWNER=\"alice\"");
    assert!(text.contains("TODO Child") && text.contains("TODO Open child"), "{text}");
    client.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

/// Table formulas: `org.table.recalculate` applies the `#+TBLFM:` line to
/// every line, `org.table.recalculate.line` to the cursor's line,
/// `org.table.iterate` until the table is stable, and `C-c C-c` on the
/// `#+TBLFM:` line applies that line's formulas.
#[test]
fn recalculates_tables() {
    const URI: &str = "file:///org/table.org";
    let mut client = Client::editor(json!({}), None);
    let start = "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n| 3 | 4 |   |\n#+TBLFM: $3=$1*$2\n#+TBLFM: $3=$1+$2\n";
    client.open(URI, start);
    let mut version = 1;
    let mut run = |client: &mut Client, text: &mut String, command: &str, line: u32| {
        client.received.clear();
        let at = json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": 2 } });
        let result = client.request("workspace/executeCommand", json!({ "command": command, "arguments": [at] }));
        client.settle();
        if let Some(edit) = client.received.iter().find(|request| request.method == "workspace/applyEdit") {
            let (_, edits) = edits_of(edit);
            *text = apply(text, &edits);
            version += 1;
            client.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": URI, "version": version }, "contentChanges": [{ "text": text }] }),
            );
            client.settle();
        }
        result
    };
    let mut text = start.to_string();
    run(&mut client, &mut text, "org.table.recalculate.line", 3).unwrap();
    assert_eq!(
        text,
        "| a | b |  c |\n|---+---+----|\n| 1 | 2 |    |\n| 3 | 4 | 12 |\n#+TBLFM: $3=$1*$2\n#+TBLFM: $3=$1+$2\n"
    );
    run(&mut client, &mut text, "org.table.recalculate", 0).unwrap();
    assert_eq!(
        text,
        "| a | b |  c |\n|---+---+----|\n| 1 | 2 |  2 |\n| 3 | 4 | 12 |\n#+TBLFM: $3=$1*$2\n#+TBLFM: $3=$1+$2\n"
    );
    // The second formula line, with the key of `C-c C-c`.
    run(&mut client, &mut text, "org.checkbox.toggle", 5).unwrap();
    assert_eq!(
        text,
        "| a | b | c |\n|---+---+---|\n| 1 | 2 | 3 |\n| 3 | 4 | 7 |\n#+TBLFM: $3=$1*$2\n#+TBLFM: $3=$1+$2\n"
    );
    let result = run(&mut client, &mut text, "org.table.iterate", 2).unwrap();
    assert_eq!(result["message"], "Convergence after 2 iterations");
    assert_eq!(
        text,
        "| a | b |  c |\n|---+---+----|\n| 1 | 2 |  2 |\n| 3 | 4 | 12 |\n#+TBLFM: $3=$1*$2\n#+TBLFM: $3=$1+$2\n"
    );
    let result = run(&mut client, &mut text, "org.table.iterate", 2).unwrap();
    assert_eq!(result["message"], "Table was already stable");
    // Errors as Emacs gives them; outside a table, the command does not
    // apply.
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": URI, "version": 100 }, "contentChanges": [{ "text": "text\n| 1 |  |\n#+TBLFM: $2=@-3\n" }] }),
    );
    client.settle();
    let error = client
        .request("workspace/executeCommand", json!({ "command": "org.table.recalculate", "arguments": [{ "textDocument": { "uri": URI }, "position": { "line": 1, "character": 1 } }] }))
        .unwrap_err();
    assert_eq!(error, "Row descriptor -3 leads outside table");
    let refused = client
        .request_raw("workspace/executeCommand", json!({ "command": "org.table.recalculate", "arguments": [{ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 1 } }] }))
        .unwrap_err();
    assert_eq!(refused.message, "Not at a table");
    client.shutdown();
}

/// `agenda.tagsColumn` `"auto"` (the default): tags end at the right edge
/// of the window the editor says the agenda has (`org-lsp/agendaWidth`),
/// at column 80 until it says; a column of the user's stays.
#[test]
fn aligns_tags_to_the_window() {
    let tags_end = |text: &str| {
        let line = text.lines().find(|line| line.contains("Write report")).unwrap().to_string();
        assert!(line.ends_with(":office:"), "{line}");
        line.chars().count()
    };
    let mut client = Client::editor(clock(), None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    let text = client.show_view();
    assert_eq!(tags_end(&text), 80);
    client.notify("org-lsp/agendaWidth", json!({ "width": 100 }));
    client.settle();
    assert_eq!(tags_end(&client.refetch_view()), 100);
    client.notify("org-lsp/agendaWidth", json!({ "width": 90 }));
    client.settle();
    assert_eq!(tags_end(&client.refetch_view()), 90);
    client.shutdown();

    let mut options = clock();
    options["agenda"] = json!({ "tagsColumn": -70 });
    let mut client = Client::editor(options, None);
    client.open("file:///org/work.org", WORK);
    client.command("org.agenda.list", VIEW, 0);
    client.show_view();
    client.notify("org-lsp/agendaWidth", json!({ "width": 100 }));
    client.settle();
    assert_eq!(tags_end(&client.refetch_view()), 70);
    client.shutdown();
}

/// A hover over an agenda line shows its entry: the headline and its text,
/// drawers folded, as Org in Markdown. Other lines, and Org files, have
/// none.
#[test]
fn hover_shows_the_entry() {
    let mut client = Client::editor(clock(), None);
    let work = "\
#+CATEGORY: work
* TODO [#A] Write report                                            :office:
SCHEDULED: <2026-05-18 Mon 09:00>
:PROPERTIES:
:ID: 1
:END:
Draft the summary.
** Sub
* Next
";
    client.open("file:///org/work.org", work);
    client.command("org.agenda.list", "file:///org/work.org", 0);
    client.show_view();
    let hover = |client: &mut Client, uri: &str, line: u32| {
        client
            .request(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 2 } }),
            )
            .unwrap()
    };
    let shown = hover(&mut client, VIEW, 3);
    assert_eq!(shown["contents"]["kind"], "markdown");
    assert_eq!(
        shown["contents"]["value"],
        "```org\n* TODO [#A] Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n:PROPERTIES:...\nDraft the summary.\n```"
    );
    assert_eq!(hover(&mut client, VIEW, 1), Value::Null);
    assert_eq!(hover(&mut client, "file:///org/work.org", 1), Value::Null);
    // `org-lsp/preview`: the same text with its semantic tokens, for an
    // editor that shows it in a popup of its own. The first token is the
    // headline's stars.
    let preview = client
        .request(
            "org-lsp/preview",
            json!({ "textDocument": { "uri": VIEW }, "position": { "line": 3, "character": 2 } }),
        )
        .unwrap();
    assert_eq!(
        preview["text"],
        "* TODO [#A] Write report :office:\nSCHEDULED: <2026-05-18 Mon 09:00>\n:PROPERTIES:...\nDraft the summary."
    );
    let data = preview["data"].as_array().unwrap();
    let legend = &client.initialize["capabilities"]["semanticTokensProvider"]["legend"];
    assert_eq!((data[0].as_u64(), data[1].as_u64(), data[2].as_u64()), (Some(0), Some(0), Some(1)));
    assert_eq!(legend["tokenTypes"][data[3].as_u64().unwrap() as usize], "stars");
    let none = client
        .request(
            "org-lsp/preview",
            json!({ "textDocument": { "uri": VIEW }, "position": { "line": 1, "character": 0 } }),
        )
        .unwrap();
    assert_eq!(none, Value::Null);
    // An entry shown in a popup of the editor's own (`org-preview:`) is no
    // file of an agenda built from the open files.
    client.open_as("org-preview:/1", "org", "* TODO Shown in a popup\nSCHEDULED: <2026-05-18 Mon>\n");
    let (_, text) = client.view_command("org.agenda.redo", 0);
    assert!(!text.contains("Shown in a popup"), "{text}");
    client.shutdown();
}
