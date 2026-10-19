//! The shared end-to-end scenarios (`tests/e2e/scenarios/*.json`), run
//! against the server the way an editor with org-lsp's plugin runs them.
//!
//! A scenario names commands of the manifest, where the cursor is when
//! they run and what they ask for, and says after which steps to look at
//! the agenda, files, the current buffer and the messages. This runner is
//! the reference: what it sees is written down in
//! `tests/e2e/expected/<scenario>.txt`, and each editor's adapter
//! (`tests/nvim/scenarios.lua` for Neovim,
//! `editors/vscode/test/scenarios.test.cjs` for VS Code) presses the
//! editor's keys for the same steps and must see the same.
//! `ORG_LSP_UPDATE_E2E=1` writes the expected files again;
//! `ORG_LSP_E2E=name` runs the scenarios whose name contains `name`.
//!
//! `coverage` checks that every command with a key in the agenda, the date
//! prompt or a capture buffer is run by some scenario, or listed in
//! `tests/e2e/exceptions.json` with the reason it is not.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response, ResponseError};
use serde_json::{json, Value};

const AGENDA: &str = "org-agenda:/Org%20Agenda";
const DATE_PROMPT: &str = "org-agenda:/Org%20Date";
const CAPTURE_PREFIX: &str = "CAPTURE-";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn e2e() -> PathBuf {
    root().join("tests/e2e")
}

/// A document the editor has open, as the server was told of it.
struct Document {
    text: String,
    version: i32,
}

/// What the server asked of the editor, handled once its answer to the
/// request at hand has come.
enum Event {
    Refresh(String),
    Show(Value),
}

/// An editor with org-lsp's plugin, as far as the scenarios need one:
/// documents, a current one with a cursor, the windows opened for the
/// agenda, the date prompt and capture buffers, and the messages shown.
struct Editor {
    connection: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
    directory: PathBuf,
    documents: BTreeMap<String, Document>,
    /// The cursor of each document, line and byte.
    cursors: BTreeMap<String, (u32, u32)>,
    current: Option<String>,
    /// The documents current before a window was opened for another
    /// (the agenda, the date prompt, a capture buffer), to go back to
    /// when it closes.
    opened_from: Vec<(String, String)>,
    events: VecDeque<Event>,
    messages: Vec<String>,
    /// What column view last showed (`org-lsp/columns`).
    columns: Option<Value>,
    /// What the editor would paste: the text last cut or copied.
    register: String,
    /// Whether the command at hand adds below the cursor's line.
    below: bool,
}

impl Editor {
    fn start(directory: &Path, options: Value) -> Editor {
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || org_lsp::run(server_side).expect("server runs"));
        let mut editor = Editor {
            connection: client_side,
            server: Some(server),
            next_id: 0,
            directory: directory.to_path_buf(),
            documents: BTreeMap::new(),
            cursors: BTreeMap::new(),
            current: None,
            opened_from: Vec::new(),
            events: VecDeque::new(),
            messages: Vec::new(),
            columns: None,
            register: String::new(),
            below: false,
        };
        // What Neovim's client and org-lsp's plugin announce.
        let capabilities = json!({
            "workspace": {
                "textDocumentContent": { "dynamicRegistration": false },
                "workspaceEdit": { "documentChanges": true },
                "semanticTokens": { "refreshSupport": true }
            },
            "window": { "showDocument": { "support": true } },
            "general": { "positionEncodings": ["utf-8", "utf-16", "utf-32"] },
            "experimental": {
                "orgClient": true,
                "orgDatePrompt": true,
                "orgVirtualIndent": true,
                "orgLineHighlights": true,
                "orgDisplay": true
            }
        });
        let params = json!({ "capabilities": capabilities, "initializationOptions": options, "rootUri": null });
        editor.request("initialize", params).expect("initialize result");
        editor.notify("initialized", json!({}));
        editor
    }

    fn uri_of(&self, name: &str) -> String {
        format!("file://{}", self.directory.join(name).display())
    }

    /// How a document is named in the snapshots: the agenda, the date
    /// prompt, or a file by its path in the scenario's directory.
    fn name_of(&self, uri: &str) -> String {
        match uri {
            AGENDA => "agenda".to_string(),
            DATE_PROMPT => "date prompt".to_string(),
            _ => {
                let path = uri.strip_prefix("file://").unwrap_or(uri);
                let directory = format!("{}/", self.directory.display());
                match path.strip_prefix(&directory) {
                    Some(relative) => relative.to_string(),
                    None => Path::new(path).file_name().unwrap().to_string_lossy().into_owned(),
                }
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification { method: method.to_string(), params };
        self.connection.sender.send(Message::Notification(notification)).unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, ResponseError> {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request { id: id.clone(), method: method.to_string(), params };
        self.connection.sender.send(Message::Request(request)).unwrap();
        loop {
            match self.connection.receiver.recv().expect("server answers") {
                Message::Response(response) if response.id == id => {
                    return response.response_result;
                }
                Message::Request(request) => self.answer(request),
                Message::Notification(notification) => self.notified(notification),
                Message::Response(_) => {}
            }
        }
    }

    /// Answer a request of the server's. What needs requests of the
    /// editor's own (fetching a document) waits in `events`.
    fn answer(&mut self, request: Request) {
        let result = match request.method.as_str() {
            "workspace/applyEdit" => {
                self.apply_workspace_edit(&request.params["edit"]);
                json!({ "applied": true })
            }
            "window/showDocument" => {
                self.events.push_back(Event::Show(request.params.clone()));
                json!({ "success": true })
            }
            "workspace/textDocumentContent/refresh" => {
                self.events.push_back(Event::Refresh(request.params["uri"].as_str().unwrap_or("").to_string()));
                Value::Null
            }
            "window/showMessageRequest" => {
                self.messages.push(format!("ASKED: {}", request.params["message"].as_str().unwrap_or("")));
                Value::Null
            }
            _ => Value::Null,
        };
        self.connection.sender.send(Message::Response(Response::new_ok(request.id, result))).unwrap();
    }

    fn notified(&mut self, notification: Notification) {
        match notification.method.as_str() {
            "window/showMessage" => {
                self.messages.push(notification.params["message"].as_str().unwrap_or("").to_string());
            }
            "org-lsp/save" => {
                let uris: Vec<String> = notification.params["uris"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|uri| uri.as_str().map(str::to_string))
                    .collect();
                self.save(&uris);
            }
            "org-lsp/columns" => self.columns = Some(notification.params),
            _ => {}
        }
    }

    /// Write the documents at `uris` that are files, as the editor saves a
    /// buffer.
    fn save(&mut self, uris: &[String]) {
        for uri in uris {
            if let (Some(path), Some(document)) = (uri.strip_prefix("file://"), self.documents.get(uri)) {
                if std::fs::read_to_string(path).ok().as_deref() != Some(document.text.as_str()) {
                    std::fs::write(path, &document.text).unwrap();
                    self.notify("textDocument/didSave", json!({ "textDocument": { "uri": uri } }));
                }
            }
        }
    }

    /// Open a file as the editor loads a buffer for it.
    fn open_file(&mut self, uri: &str) {
        if self.documents.contains_key(uri) {
            return;
        }
        let path = uri.strip_prefix("file://").expect("a file");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "org", "version": 1, "text": text } }),
        );
        self.documents.insert(uri.to_string(), Document { text, version: 1 });
    }

    /// Open a document of the server's (the agenda, the date prompt).
    fn open_view(&mut self, uri: &str) {
        if self.documents.contains_key(uri) {
            return;
        }
        let text = self.fetch(uri);
        let language = if uri == DATE_PROMPT { "org-date" } else { "org-agenda" };
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": language, "version": 1, "text": text } }),
        );
        self.documents.insert(uri.to_string(), Document { text, version: 1 });
        if uri == AGENDA {
            self.notify("org-lsp/agendaWidth", json!({ "width": 80 }));
        }
    }

    fn fetch(&mut self, uri: &str) -> String {
        match self.request("workspace/textDocumentContent", json!({ "uri": uri })) {
            Ok(result) => result["text"].as_str().unwrap_or("").to_string(),
            Err(_) => String::new(),
        }
    }

    /// Replace the text of an open document and tell the server.
    fn change(&mut self, uri: &str, text: String) {
        let Some(document) = self.documents.get_mut(uri) else { return };
        if document.text == text {
            return;
        }
        document.version += 1;
        document.text = text;
        let version = document.version;
        let text = document.text.clone();
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        );
        // The cursor stays on its line, inside the text.
        let lines = self.documents[uri].text.split('\n').count().max(1) as u32;
        if let Some(cursor) = self.cursors.get_mut(uri) {
            cursor.0 = cursor.0.min(lines - 1);
        }
    }

    fn close(&mut self, uri: &str) {
        if self.documents.remove(uri).is_none() {
            return;
        }
        self.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
        self.cursors.remove(uri);
        if self.current.as_deref() == Some(uri) {
            let back = self.opened_from.iter().rev().find(|(opened, _)| opened == uri).map(|(_, from)| from.clone());
            self.current = back.filter(|from| self.documents.contains_key(from));
        }
        self.opened_from.retain(|(opened, from)| opened != uri && from != uri);
        if self.current.is_none() {
            self.current = self.opened_from.last().map(|(opened, _)| opened.clone());
        }
    }

    fn apply_workspace_edit(&mut self, edit: &Value) {
        let mut changes: Vec<(String, Vec<Value>)> = Vec::new();
        for change in edit["documentChanges"].as_array().into_iter().flatten() {
            if let Some(uri) = change["textDocument"]["uri"].as_str() {
                changes.push((uri.to_string(), change["edits"].as_array().cloned().unwrap_or_default()));
            }
        }
        for (uri, edits) in edit["changes"].as_object().into_iter().flatten() {
            changes.push((uri.clone(), edits.as_array().cloned().unwrap_or_default()));
        }
        for (uri, edits) in changes {
            self.open_file(&uri);
            let text = apply(&self.documents[&uri].text, &edits);
            self.change(&uri, text);
        }
    }

    /// Wait until the server has handled everything sent so far, and do
    /// what it asked for meanwhile, until it asks for nothing more.
    fn settle(&mut self) {
        loop {
            let _ = self.request("org-lsp/test-sync", Value::Null);
            let _ = self.request("org-lsp/test-sync", Value::Null);
            let Some(event) = self.events.pop_front() else { return };
            match event {
                Event::Refresh(uri) => {
                    if self.documents.contains_key(&uri) {
                        let text = self.fetch(&uri);
                        self.change(&uri, text);
                    }
                }
                Event::Show(params) => self.show(&params),
            }
        }
    }

    /// `window/showDocument`, as org-lsp's Neovim plugin shows documents.
    fn show(&mut self, params: &Value) {
        let Some(uri) = params["uri"].as_str().map(str::to_string) else { return };
        if params["external"] == json!(true) {
            self.messages.push(format!("EXTERNAL: {uri}"));
            return;
        }
        let apart = uri == AGENDA || uri == DATE_PROMPT || self.name_of(&uri).starts_with(CAPTURE_PREFIX);
        let was_open = self.documents.contains_key(&uri);
        if uri.starts_with("org-agenda:") {
            self.open_view(&uri);
        } else {
            self.open_file(&uri);
        }
        if let Some(start) = params["selection"]["start"].as_object() {
            let line = start["line"].as_u64().unwrap_or(0) as u32;
            let character = start["character"].as_u64().unwrap_or(0) as u32;
            self.cursors.insert(uri.clone(), (line, character));
        }
        self.cursors.entry(uri.clone()).or_insert((0, 0));
        if params["takeFocus"] == json!(false) {
            return;
        }
        if apart && !was_open {
            if let Some(from) = &self.current {
                self.opened_from.push((uri.clone(), from.clone()));
            }
        }
        self.current = Some(uri);
    }

    fn current(&self) -> String {
        self.current.clone().expect("a current buffer")
    }

    fn cursor(&self) -> (u32, u32) {
        self.current.as_ref().and_then(|uri| self.cursors.get(uri)).copied().unwrap_or((0, 0))
    }

    fn set_cursor(&mut self, cursor: (u32, u32)) {
        let uri = self.current();
        self.cursors.insert(uri.clone(), cursor);
        // The agenda hears where the cursor is (follow mode, the outline
        // path).
        if uri == AGENDA {
            let params = json!({
                "textDocument": { "uri": uri },
                "position": { "line": cursor.0, "character": cursor.1 },
                "width": 80
            });
            let _ = self.request("org-lsp/agendaCursor", params);
            self.settle();
        }
    }

    /// Run a command as its key does: with the cursor's place, the count,
    /// and each answer the command asks for in turn. Dates are typed in
    /// the date prompt (`org.calendar.type`), as they are in the editor.
    fn run(&mut self, command: &str, count: Option<u64>, answers: &mut VecDeque<Value>) -> Result<(), String> {
        let mut context = match &self.current {
            Some(uri) => {
                let (line, character) = self.cursor();
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } })
            }
            None => json!({}),
        };
        if let Some(count) = count {
            context["count"] = json!(count);
        }
        // Below the cursor's line, as the keys of modal editors add a
        // heading or an item (`<Leader><CR>`).
        if self.below {
            context["below"] = json!(true);
        }
        if manifest_command(command).is_some_and(|entry| entry["clipboard"] == json!(true)) {
            context["clipboard"] = json!(self.register);
        }
        let from = self.current.clone();
        loop {
            let result =
                self.request("workspace/executeCommand", json!({ "command": command, "arguments": [context] }));
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    self.settle();
                    if error.data.as_ref().is_some_and(|data| data["notApplicable"] == json!(true)) {
                        self.messages.push(format!("NOT APPLICABLE: {}", error.message));
                    } else {
                        self.messages.push(error.message);
                    }
                    return Ok(());
                }
            };
            self.settle();
            self.handle_result(&result, from.as_deref());
            let Some(input) = result.get("input").filter(|input| input.is_object()) else { break };
            let Some(answer) = answers.pop_front() else {
                return Err(format!("{command} asks `{}` and the step has no answer left", input["prompt"]));
            };
            for (key, value) in input["context"].as_object().into_iter().flatten() {
                context[key] = value.clone();
            }
            context["input"] = json!(answer_of(&answer, input)?);
        }
        Ok(())
    }

    /// What the editor does with a command's result.
    fn handle_result(&mut self, result: &Value, from: Option<&str>) {
        if let (Some(edits), Some(uri)) = (result["edits"].as_array(), from) {
            if self.documents.contains_key(uri) {
                let text = apply(&self.documents[uri].text, edits);
                self.change(uri, text);
                if let Some(cursor) = result.get("cursor").filter(|cursor| cursor.is_object()) {
                    let line = cursor["line"].as_u64().unwrap_or(0) as u32;
                    let character = cursor["character"].as_u64().unwrap_or(0) as u32;
                    self.cursors.insert(uri.to_string(), (line, character));
                }
                self.settle();
            }
        }
        if let Some(copy) = result["copy"].as_str() {
            self.register = copy.to_string();
        }
        if let Some(save) = result["save"].as_array() {
            let uris: Vec<String> = save.iter().filter_map(|uri| uri.as_str().map(str::to_string)).collect();
            self.save(&uris);
        }
        if let Some(message) = result["message"].as_str() {
            self.messages.push(message.to_string());
        }
        if let Some(close) = result["close"].as_str() {
            // A capture buffer is saved as it closes.
            self.save(&[close.to_string()]);
            self.close(close);
            self.settle();
        }
    }

    /// Answer the questions a command asks in the date prompt, if one is
    /// open and answers are left: type each in its minibuffer.
    fn answer_in_date_prompt(&mut self, answers: &mut VecDeque<Value>) -> Result<(), String> {
        while self.current.as_deref() == Some(DATE_PROMPT) && !answers.is_empty() {
            self.run("org.calendar.type", None, answers)?;
        }
        Ok(())
    }

    fn shutdown(mut self) {
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().expect("server exits cleanly");
    }
}

/// The answer to send for `input`: text as it is, or the key of the choice
/// whose label is `{ "choice": label }`.
fn answer_of(answer: &Value, input: &Value) -> Result<String, String> {
    if let Some(text) = answer.as_str() {
        return Ok(text.to_string());
    }
    if let Some(label) = answer["choice"].as_str() {
        let choices = input["choices"].as_array().cloned().unwrap_or_default();
        return choices
            .iter()
            .find(|choice| choice["label"].as_str() == Some(label))
            .or_else(|| choices.iter().find(|choice| choice["label"].as_str().is_some_and(|text| text.contains(label))))
            .and_then(|choice| choice["key"].as_str().map(str::to_string))
            .ok_or_else(|| format!("no choice `{label}` in {}", input["choices"]));
    }
    Err(format!("an answer is text or {{\"choice\": label}}, not {answer}"))
}

/// Apply text edits to `text`. Positions count bytes (UTF-8, as the server
/// picks).
fn apply(text: &str, edits: &[Value]) -> String {
    let offset = |position: &Value| {
        let line = position["line"].as_u64().unwrap() as usize;
        let starts: Vec<usize> =
            std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect::<Vec<_>>();
        let start = *starts.get(line).unwrap_or(&text.len());
        let end = starts.get(line + 1).map_or(text.len(), |next| next - 1);
        (start + position["character"].as_u64().unwrap() as usize).min(end.max(start))
    };
    let mut edits: Vec<&Value> = edits.iter().collect();
    // From the last, so that the earlier ones' positions still hold.
    edits.sort_by_key(|edit| std::cmp::Reverse((offset(&edit["range"]["start"]), offset(&edit["range"]["end"]))));
    let mut out = text.to_string();
    for edit in edits {
        let (start, end) = (offset(&edit["range"]["start"]), offset(&edit["range"]["end"]));
        out.replace_range(start..end, edit["newText"].as_str().unwrap());
    }
    out
}

fn manifest() -> &'static Value {
    static MANIFEST: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    MANIFEST.get_or_init(org_lsp::manifest::manifest)
}

fn manifest_command(id: &str) -> Option<&'static Value> {
    manifest()["commands"].as_array().unwrap().iter().find(|command| command["command"] == id)
}

/// `value` with `$DIR` in its strings replaced by `directory`.
fn substitute(value: &Value, directory: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("$DIR", directory)),
        Value::Array(items) => Value::Array(items.iter().map(|item| substitute(item, directory)).collect()),
        Value::Object(fields) => {
            Value::Object(fields.iter().map(|(key, item)| (key.clone(), substitute(item, directory))).collect())
        }
        other => other.clone(),
    }
}

fn emacs_found() -> bool {
    std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success())
}

struct Scenario {
    name: String,
    spec: Value,
}

fn scenarios() -> Vec<Scenario> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(e2e().join("scenarios"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let spec: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            Scenario { name: path.file_stem().unwrap().to_string_lossy().into_owned(), spec }
        })
        .collect()
}

/// The text of the scenario's files, by path: those of `files_from` (a
/// directory under `tests/`), then those of `files`.
fn fixture_files(spec: &Value) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    if let Some(from) = spec["files_from"].as_str() {
        let directory = root().join("tests").join(from);
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".org") || name.ends_with(".org_archive") {
                files.insert(name, std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    for (name, text) in spec["files"].as_object().into_iter().flatten() {
        let text = match text {
            Value::Array(lines) => lines.iter().map(|line| format!("{}\n", line.as_str().unwrap())).collect(),
            other => other.as_str().unwrap().to_string(),
        };
        files.insert(name.clone(), text);
    }
    files
}

/// The options the server starts with: the scenario's on top of the
/// defaults every scenario has (the clock, the agenda files).
fn options_of(spec: &Value, directory: &str) -> Value {
    let clock = spec.get("clock").cloned().unwrap_or(json!({ "today": "2026-05-18", "now": "10:30" }));
    let mut options = json!({
        "config_file": false,
        "clock": clock,
        "org_agenda_files": [directory],
        "org_directory": directory,
        "org_default_notes_file": format!("{directory}/notes.org"),
    });
    for (key, value) in spec["options"].as_object().into_iter().flatten() {
        options[key] = substitute(value, directory);
    }
    if spec["emacs"] == json!(true) {
        let mut config = options["emacs_config"].as_str().unwrap_or("").to_string();
        if let Ok(lisp) = std::env::var("ORG_LISP_DIR") {
            config = format!("(add-to-list 'load-path {lisp:?}) {config}");
        }
        options["emacs_config"] = json!(config);
    }
    options
}

/// The snapshots of the steps that ask for them, as the expected files
/// hold them: a header `=== N KIND` and the text.
fn run_scenario(scenario: &Scenario) -> Result<String, String> {
    let spec = &scenario.spec;
    let directory = std::env::temp_dir().join(format!("org-lsp-e2e-{}-{}", scenario.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    for (name, text) in fixture_files(spec) {
        let path = directory.join(&name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let shown = directory.display().to_string();
    let mut editor = Editor::start(&directory, options_of(spec, &shown));
    let mut out = String::new();
    let mut error = None;
    let links = regex::Regex::new(r"\[\[[^\]]*\]\[([^\]]*)\]\]").unwrap();
    for (index, step) in spec["steps"].as_array().unwrap().iter().enumerate() {
        if let Err(message) = run_step(&mut editor, step) {
            error = Some(format!("step {}: {message}", index + 1));
            break;
        }
        for kind in step["expect"].as_array().into_iter().flatten() {
            let kind = kind.as_str().unwrap();
            let text = snapshot(&mut editor, kind).replace(&shown, "$DIR");
            if let Some(recorded) = step["emacs"].as_str().filter(|_| kind == "agenda") {
                let emacs = std::fs::read_to_string(root().join("tests").join(recorded)).unwrap();
                // The clock report's links show as their descriptions, as
                // Emacs shows them.
                let shown = links.replace_all(&emacs, "$1");
                if text.trim_end() != emacs.trim_end() && text.trim_end() != shown.trim_end() {
                    error = Some(format!("step {}: the agenda differs from Emacs's {recorded}", index + 1));
                }
            }
            out.push_str(&format!("=== {} {kind}\n{text}", index + 1));
        }
        if step["expect"].as_array().is_some_and(|kinds| kinds.iter().any(|kind| kind == "messages")) {
            editor.messages.clear();
        }
    }
    editor.shutdown();
    let _ = std::fs::remove_dir_all(&directory);
    match error {
        Some(error) => Err(format!("{error}\n{out}")),
        None => Ok(out),
    }
}

fn run_step(editor: &mut Editor, step: &Value) -> Result<(), String> {
    if let Some(name) = step["open"].as_str() {
        let uri = editor.uri_of(name);
        editor.open_file(&uri);
        editor.cursors.entry(uri.clone()).or_insert((0, 0));
        editor.current = Some(uri);
        editor.settle();
    }
    if let Some(name) = step["focus"].as_str() {
        let uri = if name == "agenda" { AGENDA.to_string() } else { editor.uri_of(name) };
        if !editor.documents.contains_key(&uri) {
            return Err(format!("no buffer {name} to go to"));
        }
        editor.current = Some(uri);
    }
    if step["close"] == json!(true) {
        let uri = editor.current();
        editor.close(&uri);
        editor.settle();
    }
    if let Some(cursor) = place(editor, step)? {
        editor.set_cursor(cursor);
    }
    if let Some(text) = step["type"].as_str() {
        let uri = editor.current();
        let (line, character) = editor.cursor();
        let document = &editor.documents[&uri].text;
        let at = offset_of(document, line, character);
        let mut changed = document.clone();
        changed.insert_str(at, text);
        // The cursor after what was typed.
        let before = &changed[..at + text.len()];
        let line = before.matches('\n').count() as u32;
        let character = (before.len() - before.rfind('\n').map_or(0, |at| at + 1)) as u32;
        editor.change(&uri, changed);
        editor.cursors.insert(uri, (line, character));
        editor.settle();
    }
    if let Some(command) = step["command"].as_str() {
        if manifest_command(command).is_none() {
            return Err(format!("{command} is not a command of the manifest"));
        }
        if step["wait"] == json!("columns") {
            editor.columns = None;
        }
        let mut answers: VecDeque<Value> = step["answers"].as_array().cloned().unwrap_or_default().into();
        editor.below = step["below"] == json!(true);
        editor.run(command, step["count"].as_u64(), &mut answers)?;
        editor.answer_in_date_prompt(&mut answers)?;
        if let Some(wait) = step["wait"].as_str() {
            wait_for(editor, wait)?;
        }
        if !answers.is_empty() {
            return Err(format!("{command} did not ask for {answers:?}"));
        }
    }
    Ok(())
}

/// Wait, half a minute at most, for what Emacs does in the background.
fn wait_for(editor: &mut Editor, what: &str) -> Result<(), String> {
    let start = std::time::Instant::now();
    loop {
        let done = match what {
            "columns" => editor.columns.is_some(),
            _ => return Err(format!("nothing to wait for called {what}")),
        };
        if done {
            return Ok(());
        }
        if start.elapsed() > std::time::Duration::from_secs(30) {
            return Err(format!("waited half a minute for {what}"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        editor.settle();
    }
}

/// Byte offset of line `line`, byte `character` in `text`.
fn offset_of(text: &str, line: u32, character: u32) -> usize {
    let start: usize = text.split_inclusive('\n').take(line as usize).map(str::len).sum();
    let length = text[start..].find('\n').unwrap_or(text.len() - start);
    start + (character as usize).min(length)
}

/// Where a step puts the cursor: on line `line` (from 1), or at the first
/// place `at` is found in the current buffer.
fn place(editor: &Editor, step: &Value) -> Result<Option<(u32, u32)>, String> {
    let column = step["column"].as_u64().unwrap_or(0) as u32;
    if let Some(line) = step["line"].as_u64() {
        return Ok(Some((line as u32 - 1, column)));
    }
    let Some(text) = step["at"].as_str() else { return Ok(None) };
    let uri = editor.current();
    let document = &editor.documents[&uri].text;
    for (number, line) in document.split('\n').enumerate() {
        if let Some(at) = line.find(text) {
            return Ok(Some((number as u32, at as u32 + column)));
        }
    }
    Err(format!("`{text}` is not in {}", editor.name_of(&uri)))
}

/// What a step shows of the editor:
/// - `agenda`: the agenda's text;
/// - `file:NAME`: a file's text, as the editor has it, else on disk;
/// - `buffer`: the current buffer's name and text;
/// - `cursor`: the current buffer, the cursor's line and its text;
/// - `messages`: what was said since the last snapshot of them;
/// - `columns`: what column view shows.
fn snapshot(editor: &mut Editor, kind: &str) -> String {
    let ended = |text: &str| format!("{}\n", text.trim_end_matches('\n'));
    match kind {
        "agenda" => match editor.documents.get(AGENDA) {
            Some(document) => ended(&document.text),
            None => "(no agenda)\n".to_string(),
        },
        "buffer" => {
            let uri = editor.current();
            format!("[{}]\n{}", editor.name_of(&uri), ended(&editor.documents[&uri].text))
        }
        "cursor" => {
            let uri = editor.current();
            let (line, _) = editor.cursor();
            let text = editor.documents[&uri].text.split('\n').nth(line as usize).unwrap_or("").to_string();
            format!("{}:{}: {}\n", editor.name_of(&uri), line + 1, text)
        }
        "messages" => editor.messages.iter().map(|message| format!("{message}\n")).collect(),
        "columns" => match &editor.columns {
            Some(columns) => columns_text(columns),
            None => "(no column view)\n".to_string(),
        },
        _ => match kind.strip_prefix("file:") {
            Some(name) => {
                let uri = editor.uri_of(name);
                match editor.documents.get(&uri) {
                    Some(document) => ended(&document.text),
                    None => match std::fs::read_to_string(editor.directory.join(name)) {
                        Ok(text) => ended(&text),
                        Err(_) => "(no file)\n".to_string(),
                    },
                }
            }
            None => panic!("unknown snapshot kind {kind}"),
        },
    }
}

/// Column view as text: the header, then each row's line and values.
fn columns_text(columns: &Value) -> String {
    let mut out = String::new();
    if columns["lines"].is_null() {
        return "(column view off)\n".to_string();
    }
    out.push_str(&format!("{}\n", columns["header"].as_str().unwrap_or("").trim_end()));
    for row in columns["lines"].as_array().into_iter().flatten() {
        let values: Vec<&str> = row[2].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        out.push_str(&format!("{}: {}\n", row[0].as_u64().unwrap_or(0) + 1, values.join(" | ").trim_end()));
    }
    out
}

#[test]
fn scenarios_match_their_snapshots() {
    let update = std::env::var_os("ORG_LSP_UPDATE_E2E").is_some();
    let only = std::env::var("ORG_LSP_E2E").ok();
    let emacs = emacs_found();
    let mut failures = Vec::new();
    let mut ran = 0;
    for scenario in scenarios() {
        if only.as_ref().is_some_and(|only| !scenario.name.contains(only.as_str())) {
            continue;
        }
        if scenario.spec["emacs"] == json!(true) && !emacs {
            assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
            eprintln!("{}: no Emacs, skipped", scenario.name);
            continue;
        }
        ran += 1;
        let path = e2e().join("expected").join(format!("{}.txt", scenario.name));
        match run_scenario(&scenario) {
            Ok(actual) => {
                if update {
                    std::fs::write(&path, &actual).unwrap();
                    continue;
                }
                let expected = std::fs::read_to_string(&path).unwrap_or_default();
                if actual != expected {
                    failures.push(format!("{}:\n{}", scenario.name, difference(&expected, &actual)));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", scenario.name)),
        }
    }
    assert!(ran > 0 || only.is_some(), "no scenarios ran");
    assert!(failures.is_empty(), "{} scenarios failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

/// The lines that differ, with a little around them.
fn difference(expected: &str, actual: &str) -> String {
    let (expected, actual): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let mut out = String::new();
    let mut shown = 0;
    for index in 0..expected.len().max(actual.len()) {
        let (want, got) = (expected.get(index), actual.get(index));
        if want != got {
            out.push_str(&format!("line {}:\n  expected: {:?}\n  actual:   {:?}\n", index + 1, want, got));
            shown += 1;
            if shown == 8 {
                break;
            }
        }
    }
    out
}

/// Every command with a key in the agenda, the date prompt or a capture
/// buffer is run by a scenario, unless the exceptions say why not.
#[test]
fn scenarios_cover_the_agenda_calendar_and_capture() {
    let exceptions: Value =
        serde_json::from_str(&std::fs::read_to_string(e2e().join("exceptions.json")).unwrap()).unwrap();
    let excepted: BTreeSet<&str> = exceptions["commands"]
        .as_object()
        .map(|commands| commands.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut used = BTreeSet::new();
    for scenario in scenarios() {
        for step in scenario.spec["steps"].as_array().unwrap() {
            if let Some(command) = step["command"].as_str() {
                assert!(manifest_command(command).is_some(), "{}: {command} is not a command", scenario.name);
                used.insert(command.to_string());
            }
        }
    }
    let mut missing = Vec::new();
    for command in manifest()["commands"].as_array().unwrap() {
        let id = command["command"].as_str().unwrap();
        let scoped = command["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| ["agenda", "calendar", "capture"].contains(&key["in"].as_str().unwrap_or("")));
        if scoped && !used.contains(id) && !excepted.contains(id) {
            missing.push(id);
        }
    }
    assert!(
        missing.is_empty(),
        "commands of the agenda, the date prompt and capture no scenario runs:\n{}",
        missing.join("\n")
    );
    // An exception for a command a scenario runs is out of date.
    let stale: Vec<&&str> = excepted.iter().filter(|id| used.contains(**id)).collect();
    assert!(stale.is_empty(), "excepted, but run by a scenario: {stale:?}");
}
