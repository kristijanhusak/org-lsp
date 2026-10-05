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
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "org", "version": 1, "text": text } }),
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
            { "key": "TAB", "in": "agenda", "preset": "emacs" },
            { "key": "<CR>", "in": "agenda", "preset": "vim" },
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
            "org.agenda.goto"
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

    // The day view starts from the day under the cursor.
    let lines: Vec<&str> = text.lines().collect();
    let wednesday = lines.iter().position(|line| line.starts_with("Wednesday")).unwrap() as u32;
    let (_, text) = client.view_command("org.agenda.day", wednesday + 1);
    assert_eq!(text, "Day-agenda (W21):\nWednesday  20 May 2026\n  work:       Deadline:   TODO Call the client\n");
    let (requests, text) = client.view_command("org.agenda.month", 1);
    assert!(text.starts_with("Month-agenda (W18-W22):\nFriday      1 May 2026\n"));
    // The cursor goes to today, far below the one line the day view had.
    assert_eq!(
        text.lines().nth(requests[1].params["selection"]["start"]["line"].as_u64().unwrap() as usize).unwrap(),
        "Monday     18 May 2026 W21"
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
            { "key": "t", "label": "List of all TODO entries" }
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
        ["org.todo.next", "org.todo.previous", "org.priority.up", "org.priority.down", "org.agenda.goto"]
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
    assert_eq!(client.initialize["capabilities"]["completionProvider"]["triggerCharacters"], json!([":"]));
    client.shutdown();
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
