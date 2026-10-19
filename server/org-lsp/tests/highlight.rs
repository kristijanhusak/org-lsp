//! Highlighting as the editors get it: the semantic tokens of the shared
//! sample (`tests/e2e/highlight`: an Org file, an agenda of its own and
//! the date prompt's calendar) cover every token type and modifier of the
//! legend, each at a place written down here, so that a token that goes
//! missing fails; and each look of the manifest's `highlights` table has
//! a token to show on, which the editors' tests look at on screen.
//!
//! `ORG_LSP_HIGHLIGHT_DUMP=1` prints every token, to write the table with.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use lsp_server::{Connection, Message, Request, RequestId, Response};
use serde_json::{json, Value};

const VIEW: &str = "org-agenda:/Org%20Agenda";
const PROMPT: &str = "org-agenda:/Org%20Date";

/// Where each token type and modifier is expected: in which document
/// (`sample`, `agenda` the week's, `todo` the TODO list, `calendar`), on the first line that contains the
/// first text, a token of the second text (without the blanks around it)
/// with exactly that type and those modifiers (`type.modifier.modifier`).
const EXPECTED: &[(&str, &str, &str, &str)] = &[
    ("sample", "#+TITLE:", "#+TITLE:", "keyword"),
    ("sample", "#+TITLE:", "Highlighting sample", "documentTitle"),
    ("sample", "#+SUBTITLE:", "Every construct the server colours", "documentInfo"),
    ("sample", "#+AUTHOR:", "Org LSP", "documentInfo"),
    ("sample", "* Headline level one", "*", "stars.level1"),
    ("sample", "* Headline level one", "Headline level one", "headline.level1"),
    ("sample", "** Headline level two", "Headline level two", "headline.level2"),
    ("sample", "*** Headline level three", "Headline level three", "headline.level3"),
    ("sample", "**** Headline level four", "Headline level four", "headline.level4"),
    ("sample", "***** Headline level five", "Headline level five", "headline.level5"),
    ("sample", "****** Headline level six", "Headline level six", "headline.level6"),
    ("sample", "******* Headline level seven", "Headline level seven", "headline.level7"),
    ("sample", "******** Headline level eight", "********", "stars.level8"),
    ("sample", "******** Headline level eight", "Headline level eight", "headline.level8"),
    ("sample", "* TODO [#A] A task to do", "TODO", "todoKeyword"),
    ("sample", "* TODO [#A] A task to do", "[#A]", "priority.todo"),
    ("sample", "* TODO [#A] A task to do", "A task to do", "headline.todo"),
    ("sample", "* TODO [#A] A task to do", ":work:urgent:", "tag.todo"),
    ("sample", "SCHEDULED: <2026-05-18 Mon>", "SCHEDULED:", "planning"),
    ("sample", "SCHEDULED: <2026-05-18 Mon>", "<2026-05-18 Mon>", "timestamp"),
    ("sample", "SCHEDULED: <2026-05-18 Mon>", "DEADLINE:", "planning"),
    ("sample", "* DONE [#B] A finished task", "DONE", "todoKeyword.done"),
    ("sample", "* DONE [#B] A finished task", "[#B]", "priority.done"),
    ("sample", "* DONE [#B] A finished task", "A finished task", "headline.done"),
    ("sample", "* DONE [#B] A finished task", ":home:", "tag.done"),
    ("sample", "CLOSED: [2026-05-17", "CLOSED:", "planning"),
    ("sample", "CLOSED: [2026-05-17", "[2026-05-17 Sun 09:00]", "timestamp.inactive"),
    ("sample", "* TODO [#C]", "[#C]", "priority.todo"),
    ("sample", "* COMMENT", "COMMENT", "planning"),
    ("sample", "* An archived headline", "An archived headline", "headline.archived"),
    ("sample", "* An archived headline", ":ARCHIVE:", "tag.archived"),
    ("sample", ":PROPERTIES:", ":PROPERTIES:", "drawer"),
    ("sample", ":CUSTOM_ID:", ":CUSTOM_ID:", "property"),
    ("sample", ":CREATED:", "[2026-05-01 Fri]", "timestamp.inactive"),
    ("sample", ":LOGBOOK:", ":LOGBOOK:", "drawer"),
    ("sample", "CLOCK: [2026-05-17", "CLOCK:", "planning"),
    ("sample", "* Lists [1/3]", "[1/3]", "cookie"),
    ("sample", "- [X] A ticked item", "-", "bullet"),
    ("sample", "- [X] A ticked item", "[X]", "checkbox.checked"),
    ("sample", "- [-] A partly ticked item", "[-]", "checkbox.partial"),
    ("sample", "- [ ] An item not ticked", "[ ]", "checkbox"),
    ("sample", "1. A numbered item", "1.", "bullet"),
    ("sample", "- term :: its description", "term ::", "descriptionTerm"),
    ("sample", "* Complete lists [2/2]", "[2/2]", "cookie.done"),
    ("sample", "Text in *bold*", "bold", "text.bold"),
    ("sample", "Text in *bold*", "italic", "text.italic"),
    ("sample", "Text in *bold*", "underline", "text.underline"),
    ("sample", "Text in *bold*", "strike-through", "text.strikethrough"),
    ("sample", "Text in *bold*", "verbatim", "verbatim"),
    ("sample", "Text in *bold*", "code", "code"),
    // `org_hide_emphasis_markers`: the markers are hidden.
    ("sample", "Text in *bold*", "*", "text.bold.hidden"),
    ("sample", "Text in *bold*", "~", "code.hidden"),
    ("sample", "Nested: *bold with", "italic", "text.bold.italic"),
    ("sample", "A [[https://orgmode.org]", "described link", "link"),
    ("sample", "A [[https://orgmode.org]", "[[", "link.hidden"),
    ("sample", "A [[https://orgmode.org]", "[[https://orgmode.org][", "link.hidden"),
    ("sample", "A [[https://orgmode.org]", "https://example.com", "link"),
    ("sample", "A [[https://orgmode.org]", "https://example.org", "link"),
    ("sample", "A <<target>>", "<<target>>", "target"),
    ("sample", "A <<target>>", "[fn:1]", "footnote"),
    ("sample", "A <<target>>", "{{{greet(world)}}}", "macro"),
    ("sample", "Entities \\alpha", "\\alpha", "entity"),
    ("sample", "Entities \\alpha", "$x^2$", "latex"),
    ("sample", "Inline src_python", "src_python{", "code"),
    ("sample", "Inline src_python", "<2026-05-19 Tue>", "timestamp"),
    ("sample", "Inline src_python", "[2026-05-19 Tue]", "timestamp.inactive"),
    ("sample", "# A comment line", "# A comment line", "comment"),
    ("sample", "#+begin_src python", "#+begin_src python :results output", "block"),
    ("sample", "#+end_src", "#+end_src", "block"),
    ("sample", "Quoted text.", "Quoted text.", "quote"),
    ("sample", "Verse text.", "Verse text.", "verse"),
    ("sample", "Commented out.", "Commented out.", "comment"),
    ("sample", "E = mc^2", "E = mc^2", "latex"),
    ("sample", "| Name | Value |", "|", "tableDelimiter"),
    ("sample", "|------+-------|", "|------+-------|", "tableDelimiter"),
    ("sample", ": Fixed-width text", ": Fixed-width text", "verbatim"),
    ("sample", "*************** TODO An inline task", "***************", "stars.inlinetask"),
    ("sample", "*************** TODO An inline task", "TODO", "todoKeyword"),
    ("sample", "*************** TODO An inline task", "An inline task", "headline.todo.inlinetask"),
    ("sample", "*************** END", "END", "headline.inlinetask"),
    ("sample", "[fn:1] The footnote", "[fn:1]", "footnote"),
    ("agenda", "Week-agenda", "Week-agenda (W21):", "agendaStructure"),
    ("agenda", "Monday     18 May 2026", "Monday     18 May 2026 W21", "agendaDate.today"),
    ("agenda", "Tuesday    19 May 2026", "Tuesday    19 May 2026", "agendaDate"),
    ("agenda", "Saturday   23 May 2026", "Saturday   23 May 2026", "agendaDate.weekend"),
    ("agenda", "8:00......", "8:00...... ----------------", "agendaTimeGrid"),
    (
        "agenda",
        "now - - -",
        "10:30...... now - - - - - - - - - - - - - - - - - - - - - - - - -",
        "agendaTimeGrid.current",
    ),
    ("agenda", "Scheduled today", "TODO", "todoKeyword"),
    ("agenda", "Scheduled today", "[#A]", "priority.bold"),
    ("agenda", "Scheduled today", "Scheduled today", "agendaScheduled.today"),
    ("agenda", "Scheduled today", ":office:", "tag"),
    ("agenda", "Scheduled before today", "Scheduled before today", "agendaScheduled.past"),
    ("agenda", "Scheduled later", "Scheduled later", "agendaScheduled"),
    ("agenda", "Deadline today", "Deadline today", "agendaDeadline.imminent"),
    ("agenda", "In   2 d.:", "Deadline in two days", "agendaDeadline"),
    ("agenda", "In   2 d.:", "[#C]", "priority.italic"),
    ("agenda", "In  12 d.:", "Deadline far away", "agendaDeadline.distant"),
    ("agenda", "14:00-15:00 Meeting", "tasks:      14:00-15:00 Meeting", "agendaEvent"),
    (
        "agenda",
        "(1/3):  Conference",
        "tasks:      (1/3):  Conference <2026-05-20 Wed>--<2026-05-22 Fri>",
        "agendaEvent.range",
    ),
    ("agenda", "Done today", "DONE", "todoKeyword.done"),
    ("agenda", "Done today", "tasks:      Scheduled:", "agendaDone"),
    ("agenda", "Done today", "Done today", "agendaDone"),
    ("agenda", "Blocked by its child", "tasks:      Scheduled:  TODO Blocked by its child", "agendaDimmed"),
    ("agenda", "Clocked in", "Clocked in", "agendaScheduled.today.clocking"),
    ("agenda", "Clocked in", "TODO", "todoKeyword.clocking"),
    ("agenda", "Clocked in", ":office:", "tag.clocking"),
    // The parts of the line in no other face, in the TODO list.
    ("todo", "Clocked in", "Clocked in", "agendaClocking"),
    ("todo", "Global list", "Global list of TODO items of type: ALL", "agendaStructure"),
    // A habit's graph: the days before it is due, done on the 15th, due
    // by the 19th; today is ready, the days to come blank.
    ("agenda", "Shave", "*", "agendaHabit"),
    ("agenda", "Shave", "", "agendaHabit.future"),
    ("agenda", "Shave", "", "agendaHabit.ready.future"),
    ("agenda", "Shave", "!", "agendaHabit.ready"),
    ("agenda", "Shave", "", "agendaHabit.alert.future"),
    ("agenda", "Shave", "", "agendaHabit.overdue.future"),
    // One done on the 11th, due by the 12th: the days since overdue.
    ("agenda", "Water the plants", "*", "agendaHabit"),
    ("agenda", "Water the plants", "", "agendaHabit.alert.future"),
    ("agenda", "Water the plants", "", "agendaHabit.overdue"),
    ("agenda", "Water the plants", "!", "agendaHabit.overdue"),
    // Done yesterday, due today.
    ("agenda", "Stretch", "!", "agendaHabit.alert"),
    ("calendar", "May 2026", "May 2026", "calendarHeader"),
    ("calendar", "Mo Tu We", "Mo", "calendarWeekday"),
    ("calendar", "Mo Tu We", "Sa", "calendarWeekday.weekend"),
    ("calendar", "17 18 19 20 21 22 23", "18", "calendarSelected"),
    ("calendar", "16 17 18     10 11 12", "10", "calendarJournal"),
    ("calendar", "24 25 26 27 28 29 30     28", "25", "calendarJournal.future"),
    ("calendar", "16 17 18     10 11 12", "12", "calendarDailies"),
    ("calendar", "SCHEDULED Date+time", "SCHEDULED Date+time [2026-05-18]:", "calendarPrompt"),
];

/// Token types the server never sends, which an editor makes of others:
/// VS Code turns headlines and their stars into `outline`, with their
/// level (`org-lsp.headlineLevelColors`).
const MADE_BY_EDITORS: &[(&str, &[&str])] = &[("outline", &["headline", "stars"])];

/// A client that answers what the server asks with yes.
struct Client {
    connection: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
    initialize: Value,
}

impl Client {
    fn start(options: Value) -> Client {
        private_state();
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut client = Client { connection: client_side, server: Some(server), next_id: 0, initialize: Value::Null };
        let capabilities = json!({
            "workspace": {
                "textDocumentContent": { "dynamicRegistration": true },
                "workspaceEdit": { "documentChanges": true },
                "semanticTokens": { "refreshSupport": true }
            },
            "window": { "showDocument": { "support": true } },
            "general": { "positionEncodings": ["utf-8"] },
            "experimental": { "orgClient": true, "orgDatePrompt": true }
        });
        let params = json!({ "capabilities": capabilities, "initializationOptions": options });
        client.initialize = client.request("initialize", params);
        client.notify("initialized", json!({}));
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.request_raw(method, params).unwrap_or_else(|error| panic!("{method}: {error}"))
    }

    fn request_raw(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        self.connection.sender.send(Message::Request(request)).unwrap();
        loop {
            match self.connection.receiver.recv().expect("server answers") {
                Message::Response(response) if response.id == id => {
                    return response.response_result.map_err(|error| error.message);
                }
                Message::Request(request) => {
                    let result = match request.method.as_str() {
                        "workspace/applyEdit" => json!({ "applied": true }),
                        "window/showDocument" => json!({ "success": true }),
                        _ => Value::Null,
                    };
                    self.connection.sender.send(Message::Response(Response::new_ok(request.id, result))).unwrap();
                }
                _ => {}
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = lsp_server::Notification { method: method.to_string(), params };
        self.connection.sender.send(Message::Notification(notification)).unwrap();
    }

    fn open(&self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "org", "version": 1, "text": text } }),
        );
    }

    fn command(&mut self, command: &str, uri: &str, line: u32) -> Value {
        let context = json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 0 } });
        let result = self.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
        // Answered once the server has done what the command started.
        let _ = self.request_raw("org-lsp/test-sync", Value::Null);
        result
    }

    fn text(&mut self, uri: &str) -> String {
        self.request("workspace/textDocumentContent", json!({ "uri": uri }))["text"]
            .as_str()
            .expect("the document has text")
            .to_string()
    }

    /// The tokens of `uri`, whose text is `text`.
    fn tokens(&mut self, uri: &str, text: &str) -> Vec<Seen> {
        let tokens = self.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": uri } }));
        let legend = &self.initialize["capabilities"]["semanticTokensProvider"]["legend"];
        let names = |key: &str| -> Vec<String> {
            legend[key].as_array().unwrap().iter().map(|name| name.as_str().unwrap().to_string()).collect()
        };
        let (types, modifiers) = (names("tokenTypes"), names("tokenModifiers"));
        let lines: Vec<&str> = text.lines().collect();
        let data: Vec<u64> = tokens["data"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).collect();
        let (mut line, mut start) = (0, 0);
        let mut seen = Vec::new();
        for token in data.chunks(5) {
            line += token[0] as usize;
            start = if token[0] == 0 { start + token[1] as usize } else { token[1] as usize };
            let mut selector = types[token[3] as usize].clone();
            for (bit, name) in modifiers.iter().enumerate() {
                if token[4] & (1 << bit) != 0 {
                    selector.push('.');
                    selector.push_str(name);
                }
            }
            let text = lines[line][start..start + token[2] as usize].to_string();
            seen.push(Seen { line, text, selector });
        }
        seen
    }

    fn shutdown(mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().expect("server exits cleanly");
    }
}

/// A token: its line, its text and `type.modifier…`.
#[derive(Debug)]
struct Seen {
    line: usize,
    text: String,
    selector: String,
}

/// A document by name (`sample`, …), its text and its tokens.
type Document = (&'static str, String, Vec<Seen>);

/// The fixture copied to a directory of its own, and its options with
/// `$DIR` that directory.
fn fixture() -> (PathBuf, Value) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/e2e/highlight");
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!("org-lsp-highlight-{}-{next}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    copy(&source, &directory);
    let options = std::fs::read_to_string(source.join("options.json")).unwrap();
    let mut options: Value = serde_json::from_str(&options.replace("$DIR", &directory.display().to_string())).unwrap();
    options["config_file"] = json!(false);
    (directory, options)
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

/// The sample, the week's agenda and the calendar of `org.schedule`, with
/// their tokens.
fn documents() -> (Vec<Document>, Value) {
    let (directory, options) = fixture();
    let mut client = Client::start(options);
    let manifest = client.initialize["capabilities"]["experimental"]["org"].clone();
    let mut documents = Vec::new();
    for (name, file) in [("sample", "sample.org"), ("agenda.org", "agenda.org")] {
        let text = std::fs::read_to_string(directory.join(file)).unwrap();
        let uri = format!("file://{}", directory.join(file).display());
        client.open(&uri, &text);
        if name == "sample" {
            let tokens = client.tokens(&uri, &text);
            documents.push(("sample", text.clone(), tokens));
            let line = text.lines().position(|line| line == "* Headline level one").unwrap() as u32;
            client.command("org.schedule", &uri, line);
            let calendar = client.text(PROMPT);
            let tokens = client.tokens(PROMPT, &calendar);
            documents.push(("calendar", calendar, tokens));
            client.command("org.calendar.cancel", PROMPT, 0);
        }
    }
    let agenda_file = format!("file://{}", directory.join("agenda.org").display());
    client.command("org.agenda.list", &agenda_file, 0);
    let agenda = client.text(VIEW);
    let tokens = client.tokens(VIEW, &agenda);
    documents.push(("agenda", agenda, tokens));
    // The TODO list, where the line of the clocked entry has parts in no
    // other face.
    client.command("org.agenda.todos", &agenda_file, 0);
    let todos = client.text(VIEW);
    let tokens = client.tokens(VIEW, &todos);
    documents.push(("todo", todos, tokens));
    client.shutdown();
    let _ = std::fs::remove_dir_all(&directory);
    (documents, manifest)
}

fn dump(documents: &[Document]) {
    if std::env::var_os("ORG_LSP_HIGHLIGHT_DUMP").is_none() {
        return;
    }
    for (name, text, tokens) in documents {
        eprintln!("== {name}\n{text}");
        for token in tokens {
            eprintln!("{name} {:3} {:?} {}", token.line + 1, token.text, token.selector);
        }
    }
}

#[test]
fn the_sample_has_every_token_where_it_is_expected() {
    let (documents, manifest) = documents();
    dump(&documents);
    let mut failures = Vec::new();
    for (document, line, text, selector) in EXPECTED {
        let (_, content, tokens) = documents.iter().find(|(name, _, _)| name == document).unwrap();
        let Some(row) = content.lines().position(|candidate| candidate.contains(line)) else {
            failures.push(format!("{document}: no line with {line:?}"));
            continue;
        };
        let on_line: Vec<&Seen> = tokens.iter().filter(|token| token.line == row).collect();
        if !on_line.iter().any(|token| token.text.trim() == *text && token.selector == *selector) {
            let found: Vec<String> =
                on_line.iter().map(|token| format!("{:?} {}", token.text, token.selector)).collect();
            failures.push(format!("{document} {line:?}: no {text:?} as {selector}; the line has {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // Every type and modifier of the legend is in the table.
    let names = |key: &str| -> Vec<String> {
        manifest[key].as_array().unwrap().iter().map(|entry| entry["id"].as_str().unwrap().to_string()).collect()
    };
    let parts: Vec<&str> = EXPECTED.iter().flat_map(|(_, _, _, selector)| selector.split('.')).collect();
    let types: Vec<&str> = EXPECTED.iter().map(|(_, _, _, selector)| selector.split('.').next().unwrap()).collect();
    for ty in names("tokenTypes") {
        if MADE_BY_EDITORS.iter().any(|(made, _)| *made == ty) {
            continue;
        }
        assert!(types.contains(&ty.as_str()), "the token type `{ty}` is not in EXPECTED");
    }
    for modifier in names("tokenModifiers") {
        assert!(parts[..].contains(&modifier.as_str()), "the modifier `{modifier}` is not in EXPECTED");
    }
}

/// Each look of the manifest (`type`, `type.modifier`, `*.modifier`,
/// `type.modifier.modifier`) has
/// a token of the sample to show on: what the editors' tests look at. The
/// look of a type an editor makes shows on the tokens it is made of.
#[test]
fn every_look_has_a_token_to_show_on() {
    let (documents, manifest) = documents();
    let mut missing = Vec::new();
    for look in manifest["highlights"].as_array().unwrap() {
        let selector = look["token"].as_str().unwrap();
        let mut wanted = selector.split('.');
        let ty = wanted.next().unwrap();
        let modifiers: Vec<&str> = wanted.collect();
        let made_of = MADE_BY_EDITORS.iter().find(|(made, _)| *made == ty).map(|(_, of)| *of);
        let found = documents.iter().flat_map(|(_, _, tokens)| tokens).any(|token| {
            let mut parts = token.selector.split('.');
            let token_type = parts.next().unwrap();
            let parts: Vec<&str> = parts.collect();
            let of_type = ty == "*" || ty == token_type || made_of.is_some_and(|of| of.contains(&token_type));
            of_type && modifiers.iter().all(|modifier| parts.contains(modifier))
        });
        if !found {
            missing.push(selector);
        }
    }
    assert!(missing.is_empty(), "no token for {missing:?}");
}

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
