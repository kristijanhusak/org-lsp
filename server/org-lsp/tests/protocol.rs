//! org-protocol URLs through `org-lsp protocol URL`, as the system runs it
//! for a bookmarklet: to a running server (store-link, capture,
//! open-source), past the entries of servers that stopped, and with no
//! server, kept for the next or the editor of `org_protocol_editor`
//! started for it. Each test has a state directory and a configuration
//! file of its own.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const BINARY: &str = env!("CARGO_BIN_EXE_org-lsp");

/// A test's own directories: the state directory, the configuration file
/// and the Org files.
struct Place {
    root: PathBuf,
}

impl Place {
    fn new(name: &str, config: &str) -> Place {
        let root = std::env::temp_dir().join(format!("org-lsp-protocol-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("org")).unwrap();
        let config = config.replace("$DIR", &root.display().to_string());
        std::fs::write(root.join("config.toml"), config).unwrap();
        Place { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state").join("org-lsp").join("protocol")
    }

    fn env(&self, command: &mut Command) {
        command
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("ORG_LSP_CONFIG", self.root.join("config.toml"))
            .env("HOME", &self.root)
            .env_remove("XDG_CONFIG_HOME");
    }

    /// `org-lsp protocol URL`: whether it succeeded, what it printed and
    /// what it said went wrong.
    fn protocol(&self, url: &str) -> (bool, String, String) {
        let mut command = Command::new(BINARY);
        command.args(["protocol", url]);
        self.env(&mut command);
        let output = command.output().unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )
    }

    /// The entries of the servers that listen.
    fn entries(&self) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(self.state())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                name.ends_with(".json") && !name.starts_with("pending-")
            })
            .collect();
        entries.sort();
        entries
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A server, run as an editor runs it, over its standard input and
/// output. It answers the server's requests as an editor would.
struct Server {
    child: Child,
    stdin: std::process::ChildStdin,
    messages: crossbeam_channel::Receiver<Value>,
    next: i64,
    /// What came that the test has not looked at yet.
    seen: Vec<Value>,
    /// The button picked in `window/showMessageRequest`: the first whose
    /// title starts with it.
    choose: Option<String>,
}

fn frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse().ok()?;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

impl Server {
    fn start(place: &Place, experimental: Value) -> Server {
        let mut command = Command::new(BINARY);
        command.arg("--stdio").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        place.env(&mut command);
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, messages) = crossbeam_channel::unbounded();
        std::thread::spawn(move || {
            while let Some(message) = read_message(&mut stdout) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        let mut server = Server { child, stdin, messages, next: 1, seen: Vec::new(), choose: None };
        let root = place.root.join("org");
        let capabilities = json!({
            "window": { "showDocument": { "support": true } },
            "experimental": experimental,
        });
        let params = json!({
            "capabilities": capabilities,
            "rootUri": format!("file://{}", root.display()),
            "initializationOptions": { "org_agenda_files": [root] },
        });
        server.request("initialize", params);
        server.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        server
    }

    fn send(&mut self, message: Value) {
        self.stdin.write_all(&frame(&message)).unwrap();
        self.stdin.flush().unwrap();
    }

    /// The next message the server sends, answering its requests.
    fn receive(&mut self, until: Instant) -> Option<Value> {
        let left = until.checked_duration_since(Instant::now())?;
        let message = self.messages.recv_timeout(left).ok()?;
        if let (Some(id), Some(method)) = (message.get("id"), message["method"].as_str()) {
            let result = match method {
                "window/showDocument" => json!({ "success": true }),
                "workspace/applyEdit" => json!({ "applied": true }),
                "window/showMessageRequest" => {
                    let actions = message["params"]["actions"].as_array().cloned().unwrap_or_default();
                    let wanted = self.choose.clone().unwrap_or_default();
                    let picked = actions.into_iter().find(|action| {
                        !wanted.is_empty() && action["title"].as_str().is_some_and(|title| title.starts_with(&wanted))
                    });
                    picked.unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
        }
        Some(message)
    }

    /// The result of a request.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let until = Instant::now() + Duration::from_secs(30);
        while let Some(message) = self.receive(until) {
            if message["id"] == json!(id) && message.get("method").is_none() {
                return message.get("result").cloned().unwrap_or_else(|| json!({ "error": message["error"] }));
            }
            self.seen.push(message);
        }
        panic!("no answer to {method}");
    }

    /// The first message, of those come and to come within ten seconds,
    /// that `wanted` takes.
    fn wait_for(&mut self, what: &str, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.seen.iter().position(&wanted) {
            return self.seen.remove(index);
        }
        let until = Instant::now() + Duration::from_secs(10);
        while let Some(message) = self.receive(until) {
            if wanted(&message) {
                return message;
            }
            self.seen.push(message);
        }
        panic!("no {what}; came: {:#?}", self.seen);
    }

    /// The messages it shows.
    fn shown(&mut self, text: &str) -> Value {
        self.wait_for(&format!("message {text:?}"), |message| {
            message["method"] == "window/showMessage" && message["params"]["message"] == text
        })
    }

    /// What `org.link.insert` offers in `file`.
    fn offered_links(&mut self, file: &Path) -> Value {
        let uri = format!("file://{}", file.display());
        let text = std::fs::read_to_string(file).unwrap();
        self.send(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": { "uri": uri, "languageId": "org", "version": 1, "text": text },
        } }));
        let argument = json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 0 } });
        let result =
            self.request("workspace/executeCommand", json!({ "command": "org.link.insert", "arguments": [argument] }));
        result["input"]["completions"].clone()
    }

    fn stop(mut self) {
        self.request("shutdown", Value::Null);
        self.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
    }
}

/// Wait until a server listens.
fn listening(place: &Place, count: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while place.entries().len() < count {
        assert!(Instant::now() < until, "no server listens");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The path of a `file://` URI.
fn path_of(uri: &Value) -> PathBuf {
    PathBuf::from(uri.as_str().unwrap().strip_prefix("file://").unwrap())
}

const TEMPLATES: &str = r#"
org_directory = "$DIR/org"
org_default_notes_file = "$DIR/org/notes.org"

[[org_capture_templates]]
key = "w"
description = "Web"
target = ["file+headline", "", "Web"]
template = ["* %:description", "%:link", "%i", "From %a (%:type)"]

[[org_capture_templates]]
key = "t"
description = "Task"
target = ["file+headline", "", "Tasks"]
template = "* TODO %?"
"#;

const CAPTURE_URL: &str = "org-protocol://capture?template=w&url=https%3A%2F%2Forgmode.org%2Fmanual%2F\
                           &title=The+Org+Manual&body=Org+is+a+mode";

/// What the capture buffer of a URL's capture should have.
fn check_capture_buffer(shown: &Value) {
    let buffer = path_of(&shown["params"]["uri"]);
    assert!(buffer.file_name().unwrap().to_string_lossy().starts_with("CAPTURE-"), "{shown}");
    let text = std::fs::read_to_string(&buffer).unwrap();
    assert_eq!(
        text,
        "** The Org Manual\nhttps://orgmode.org/manual/\nOrg is a mode\n\
         From [[https://orgmode.org/manual/][The Org Manual]] (https)\n"
    );
}

#[test]
fn a_running_server_stores_the_link() {
    let place = Place::new("store", TEMPLATES);
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    let mut server = Server::start(&place, json!({}));
    listening(&place, 1);
    let (ok, out, err) = place.protocol("org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org+mode");
    assert!(ok, "{err}");
    let message = "‘C-c C-l’ to insert new Org link, ‘C-y’ to insert \"https://orgmode.org/\"";
    assert_eq!(out, message);
    server.shown(message);
    // `org-insert-link` offers it.
    let offered = server.offered_links(&place.root.join("org/notes.org"));
    assert!(offered.as_array().unwrap().contains(&json!("https://orgmode.org/")), "{offered}");
    // A URL Org cannot read: its error, the server unbothered.
    let (ok, _, err) = place.protocol("org-protocol://store-link?title=No+URL");
    assert!(!ok);
    assert_eq!(err, "org-lsp protocol: Wrong type argument: stringp, nil");
    let (ok, _, err) = place.protocol("https://orgmode.org");
    assert!(!ok);
    assert!(err.contains("not an org-protocol URL"), "{err}");
    server.stop();
    // Its entry goes with it.
    assert!(place.entries().is_empty());
}

#[test]
fn a_running_server_captures_from_the_url() {
    let place = Place::new("capture", TEMPLATES);
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    // Without `orgRunCommand`, the server runs the capture itself.
    let mut server = Server::start(&place, json!({}));
    listening(&place, 1);
    let (ok, _, err) = place.protocol(CAPTURE_URL);
    assert!(ok, "{err}");
    let shown = server.wait_for("the capture buffer", |message| message["method"] == "window/showDocument");
    assert_eq!(shown["params"]["takeFocus"], true);
    check_capture_buffer(&shown);
    server.shown("Item captured.");
    // The link is stored too.
    let offered = server.offered_links(&place.root.join("org/notes.org"));
    assert!(offered.as_array().unwrap().contains(&json!("https://orgmode.org/manual/")), "{offered}");
    // A template that is not there.
    let (ok, _, err) = place.protocol("org-protocol://capture?template=zz&url=x");
    assert!(!ok);
    assert_eq!(err, "org-lsp protocol: No capture template referred to by \"zz\" keys");
    server.stop();
}

#[test]
fn the_server_asks_for_the_template_itself() {
    // A client that runs no command for the server and does nothing with
    // a command's result (Zed): the server asks with buttons.
    let place = Place::new("menu", TEMPLATES);
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    let mut server = Server::start(&place, json!({}));
    server.choose = Some("Web".to_string());
    listening(&place, 1);
    let (ok, _, err) = place.protocol("org-protocol://capture?url=https%3A%2F%2Fexample.com&title=Example&body=Hi");
    assert!(ok, "{err}");
    let asked = server.wait_for("the menu", |message| message["method"] == "window/showMessageRequest");
    assert_eq!(asked["params"]["message"], "Select a capture template");
    let shown = server.wait_for("the capture buffer", |message| message["method"] == "window/showDocument");
    let text = std::fs::read_to_string(path_of(&shown["params"]["uri"])).unwrap();
    assert_eq!(text, "** Example\nhttps://example.com\nHi\nFrom [[https://example.com][Example]] (https)\n");
    server.shown("Item captured.");
    server.stop();
}

#[test]
fn a_client_that_runs_commands_runs_the_capture() {
    let place = Place::new("run", TEMPLATES);
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    let mut server = Server::start(&place, json!({ "orgRunCommand": true }));
    listening(&place, 1);
    let (ok, _, err) = place.protocol(CAPTURE_URL);
    assert!(ok, "{err}");
    let run = server.wait_for("org-lsp/runCommand", |message| message["method"] == "org-lsp/runCommand");
    assert_eq!(run["params"]["command"], "org.capture");
    let argument = &run["params"]["argument"];
    assert_eq!(argument["keys"], "w");
    // The client runs it as from its key, where its cursor is.
    let result =
        server.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [argument] }));
    assert_eq!(result["message"], "Item captured.");
    let shown = server.wait_for("the capture buffer", |message| message["method"] == "window/showDocument");
    check_capture_buffer(&shown);
    // Without a template: the menu, which keeps what the URL stored for
    // the answer.
    let (ok, _, err) = place.protocol("org-protocol://capture?url=https%3A%2F%2Fexample.com&title=Example");
    assert!(ok, "{err}");
    let run = server.wait_for("org-lsp/runCommand", |message| message["method"] == "org-lsp/runCommand");
    let argument = run["params"]["argument"].clone();
    assert!(argument.get("keys").is_none());
    let asked =
        server.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [argument] }));
    assert_eq!(asked["input"]["prompt"], "Select a capture template");
    let mut answer = argument.clone();
    for (key, value) in asked["input"]["context"].as_object().unwrap() {
        answer[key] = value.clone();
    }
    answer["input"] = json!("w");
    let result = server.request("workspace/executeCommand", json!({ "command": "org.capture", "arguments": [answer] }));
    assert_eq!(result["message"], "Item captured.");
    let shown = server.wait_for("the capture buffer", |message| message["method"] == "window/showDocument");
    let text = std::fs::read_to_string(path_of(&shown["params"]["uri"])).unwrap();
    assert!(text.starts_with("** Example\nhttps://example.com\n"), "{text}");
    server.stop();
}

#[test]
fn a_running_server_opens_the_source() {
    let config = r#"
[[org_protocol_project_alist]]
name = "Worg"
base_url = "https://orgmode.org/worg/"
working_directory = "$DIR/worg/"
online_suffix = ".html"
working_suffix = ".org"
"#;
    let place = Place::new("source", config);
    std::fs::create_dir_all(place.root.join("worg")).unwrap();
    std::fs::write(place.root.join("worg/index.org"), "* Worg\n").unwrap();
    let mut server = Server::start(&place, json!({}));
    listening(&place, 1);
    let (ok, _, err) = place.protocol("org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Findex.html");
    assert!(ok, "{err}");
    let shown = server.wait_for("the file", |message| message["method"] == "window/showDocument");
    assert_eq!(path_of(&shown["params"]["uri"]), place.root.join("worg/index.org"));
    assert_eq!(shown["params"]["takeFocus"], true);
    // A page whose file is not there: what Org says last.
    let (ok, _, err) = place.protocol("org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Fnone.html");
    assert!(!ok);
    let said = format!("{}/worg/none.org: no such file or directory.", place.root.display());
    assert_eq!(err, format!("org-lsp protocol: {said}"));
    server.shown(&said);
    server.stop();
}

#[test]
fn without_a_server_a_link_waits_for_the_next() {
    let place = Place::new("waits", TEMPLATES);
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    // A server that stopped without taking its entry away, touched last.
    std::fs::create_dir_all(place.state()).unwrap();
    let stale = place.state().join("1-dead.json");
    let socket = place.state().join("1-dead.sock");
    std::fs::write(&stale, json!({ "pid": 1, "socket": socket, "address": "127.0.0.1:9" }).to_string()).unwrap();
    let (ok, out, err) = place.protocol("org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org");
    assert!(ok, "{err}");
    assert_eq!(out, "‘C-c C-l’ to insert new Org link, ‘C-y’ to insert \"https://orgmode.org/\"");
    assert!(!stale.exists(), "the stale entry is left");
    // A capture needs an editor.
    let (ok, _, err) = place.protocol(CAPTURE_URL);
    assert!(!ok);
    assert_eq!(
        err,
        "org-lsp protocol: No org-lsp is running, and `org_protocol_editor` names no editor to start for the capture"
    );
    // The next server has the link.
    let mut server = Server::start(&place, json!({}));
    listening(&place, 1);
    let offered = server.offered_links(&place.root.join("org/notes.org"));
    assert!(offered.as_array().unwrap().contains(&json!("https://orgmode.org/")), "{offered}");
    server.stop();
    assert!(!place.state().join("stored-links.jsonl").exists());
}

#[test]
fn stale_entries_are_skipped_for_a_live_server() {
    let place = Place::new("stale", TEMPLATES);
    let mut server = Server::start(&place, json!({}));
    listening(&place, 1);
    let live = place.entries();
    // A server that stopped, touched after the live one.
    let stale = place.state().join("2-gone.json");
    let socket = place.state().join("2-gone.sock");
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    std::fs::write(&stale, json!({ "pid": 2, "socket": socket }).to_string()).unwrap();
    let (ok, _, err) = place.protocol("org-protocol://store-link?url=https%3A%2F%2Fexample.com");
    assert!(ok, "{err}");
    server.shown("‘C-c C-l’ to insert new Org link, ‘C-y’ to insert \"https://example.com\"");
    assert_eq!(place.entries(), live);
    assert!(!socket.exists());
    server.stop();
}

#[test]
fn without_a_server_the_editor_starts_and_its_server_captures() {
    let place = Place::new("editor", "");
    let editor = place.root.join("editor.sh");
    let started = place.root.join("started");
    std::fs::write(&editor, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", started.display())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let config = format!("org_protocol_editor = \"'{}' --new-window\"\n{TEMPLATES}", editor.display())
        .replace("$DIR", &place.root.display().to_string());
    std::fs::write(place.root.join("config.toml"), config).unwrap();
    std::fs::write(place.root.join("org/notes.org"), "* Web\n").unwrap();
    // Left there from long ago: not taken.
    std::fs::create_dir_all(place.state()).unwrap();
    let old = place.state().join("pending-1-1.json");
    std::fs::write(&old, json!({ "url": "org-protocol://capture?template=t", "time": 1 }).to_string()).unwrap();
    let (ok, out, err) = place.protocol(CAPTURE_URL);
    assert!(ok, "{err}");
    assert_eq!(out, "");
    // The editor was started on the notes.
    let until = Instant::now() + Duration::from_secs(10);
    while !started.exists() {
        assert!(Instant::now() < until, "the editor was not started");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(100));
    let arguments = std::fs::read_to_string(&started).unwrap();
    assert_eq!(arguments, format!("--new-window\n{}\n", place.root.join("org/notes.org").display()));
    // The server it starts captures.
    let mut server = Server::start(&place, json!({}));
    let shown = server.wait_for("the capture buffer", |message| message["method"] == "window/showDocument");
    check_capture_buffer(&shown);
    assert!(!old.exists());
    let pending: Vec<_> = std::fs::read_dir(place.state())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("pending-"))
        .collect();
    assert!(pending.is_empty());
    server.stop();
}

#[cfg(unix)]
#[test]
fn only_the_user_reaches_the_socket() {
    use std::os::unix::fs::PermissionsExt;
    let place = Place::new("private", "");
    let server = Server::start(&place, json!({}));
    listening(&place, 1);
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&place.state()), 0o700);
    let entry = &place.entries()[0];
    assert_eq!(mode(entry), 0o600);
    let value: Value = serde_json::from_str(&std::fs::read_to_string(entry).unwrap()).unwrap();
    assert_eq!(mode(Path::new(value["socket"].as_str().unwrap())), 0o600);
    // What is not a URL is refused.
    let mut stream = std::os::unix::net::UnixStream::connect(value["socket"].as_str().unwrap()).unwrap();
    stream.write_all(b"{\"url\": \"file:///etc/passwd\"}\n").unwrap();
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer).unwrap();
    assert!(answer.contains("not an org-protocol URL"), "{answer}");
    server.stop();
}
