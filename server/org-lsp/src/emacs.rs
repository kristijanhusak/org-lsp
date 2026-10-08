//! Emacs, for what only Org itself can do (export). The server starts one
//! `emacs --batch` when a command first needs it and keeps it running, so
//! that Org loads once; `bridge.el` answers its requests. Emacs is
//! optional: without it, those commands say so and nothing else changes.
//!
//! A thread of its own talks to Emacs, so that a long export never holds
//! up the server. A request either waits for its answer
//! ([`Emacs::call`], for what is quick, such as the export menu) or is
//! answered later as an [`Event`] the server's loop receives
//! ([`Emacs::start`]). Questions Emacs asks on the way (`y-or-n-p`) come
//! as events too, and wait for their answer.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use serde_json::{json, Value};

/// The Lisp that answers the server's requests.
const BRIDGE: &str = include_str!("bridge.el");
/// What starts each line of Emacs's that is meant for the server.
const MARKER: &str = "\u{1e}org-lsp ";
/// How many of the last lines Emacs wrote on its standard error are kept,
/// to say why it stopped.
const STDERR_LINES: usize = 20;

/// How Emacs is started, and what it loads before Org.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// The program (`emacs`).
    pub program: String,
    /// A file of Emacs Lisp to load first (`emacs_config_file`).
    pub config_file: Option<String>,
    /// Emacs Lisp to evaluate after it (`emacs_config`).
    pub config: Option<String>,
}

/// What the Emacs thread has for the server's loop.
pub enum Event {
    /// The answer to the request [`Emacs::start`] was given `tag` for.
    Done { tag: u64, result: Result<Value, String> },
    /// Emacs asks a yes-or-no question; the answer goes to `answer`.
    Ask { prompt: String, answer: Sender<bool> },
}

struct Job {
    method: String,
    params: Value,
    reply: Reply,
}

enum Reply {
    Now(Sender<Result<Value, String>>),
    Later(u64),
}

/// The link to Emacs. Dropping it stops Emacs.
pub struct Emacs {
    jobs: Sender<Job>,
    events: Receiver<Event>,
}

impl Emacs {
    pub fn new(settings: Settings) -> Emacs {
        let (jobs, job_receiver) = crossbeam_channel::unbounded::<Job>();
        let (event_sender, events) = crossbeam_channel::unbounded();
        let _ = std::thread::Builder::new()
            .name("org-lsp-emacs".to_string())
            .spawn(move || serve(&settings, &job_receiver, &event_sender));
        Emacs { jobs, events }
    }

    /// What the server's loop listens to.
    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }

    /// Run `method` and wait for its answer, at most `timeout`.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        self.jobs
            .send(Job { method: method.to_string(), params, reply: Reply::Now(sender) })
            .map_err(|_| "Emacs is not running".to_string())?;
        receiver.recv_timeout(timeout).map_err(|_| "Emacs did not answer in time".to_string())?
    }

    /// A handle that runs methods and waits for them, as [`Emacs::call`],
    /// for those that hold no reference to the server.
    pub fn caller(&self) -> Caller {
        Caller { jobs: self.jobs.clone() }
    }

    /// Run `method`; its answer comes as [`Event::Done`] with `tag`.
    pub fn start(&self, tag: u64, method: &str, params: Value) {
        let _ = self.jobs.send(Job { method: method.to_string(), params, reply: Reply::Later(tag) });
    }
}

/// See [`Emacs::caller`].
#[derive(Clone)]
pub struct Caller {
    jobs: Sender<Job>,
}

impl Caller {
    /// [`Emacs::call`]
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        self.jobs
            .send(Job { method: method.to_string(), params, reply: Reply::Now(sender) })
            .map_err(|_| "Emacs is not running".to_string())?;
        receiver.recv_timeout(timeout).map_err(|_| "Emacs did not answer in time".to_string())?
    }
}

/// A running Emacs.
struct Process {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    next_id: u64,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The thread: run each job in Emacs, which starts with the first one and
/// again after it stopped.
fn serve(settings: &Settings, jobs: &Receiver<Job>, events: &Sender<Event>) {
    let mut process: Option<Process> = None;
    for job in jobs {
        if process.is_none() {
            match start(settings, events) {
                Ok(started) => process = Some(started),
                Err(error) => {
                    answer(job.reply, Err(error), events);
                    continue;
                }
            }
        }
        let Some(running) = process.as_mut() else { continue };
        let result = request(running, &job.method, job.params, events);
        if result.as_ref().is_err_and(|error| error.starts_with(STOPPED)) {
            process = None;
        }
        answer(job.reply, result, events);
    }
}

fn answer(reply: Reply, result: Result<Value, String>, events: &Sender<Event>) {
    match reply {
        Reply::Now(sender) => {
            let _ = sender.send(result);
        }
        Reply::Later(tag) => {
            let _ = events.send(Event::Done { tag, result });
        }
    }
}

const STOPPED: &str = "Emacs stopped";

/// Start Emacs and set it up: the user's configuration, then Org.
fn start(settings: &Settings, events: &Sender<Event>) -> Result<Process, String> {
    let program = if settings.program.is_empty() { "emacs" } else { &settings.program };
    let mut child = Command::new(program)
        .args(["--batch", "-Q", "--eval", &format!("(progn {BRIDGE})")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Cannot start Emacs (`{program}`, the `emacs` option): {error}"))?;
    let stdin = child.stdin.take().ok_or("Emacs has no input")?;
    let stdout = child.stdout.take().ok_or("Emacs has no output")?;
    let stderr = child.stderr.take().ok_or("Emacs has no error output")?;
    let (line_sender, lines) = crossbeam_channel::unbounded();
    let _ = std::thread::Builder::new().name("org-lsp-emacs-out".to_string()).spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(answer) = line.strip_prefix(MARKER) {
                if line_sender.send(answer.to_string()).is_err() {
                    break;
                }
            }
        }
    });
    let kept = Arc::new(Mutex::new(VecDeque::new()));
    let keeping = Arc::clone(&kept);
    let _ = std::thread::Builder::new().name("org-lsp-emacs-err".to_string()).spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if let Ok(mut kept) = keeping.lock() {
                if kept.len() == STDERR_LINES {
                    kept.pop_front();
                }
                kept.push_back(line);
            }
        }
    });
    let mut process = Process { child, stdin, lines, stderr: kept, next_id: 0 };
    let params = json!({ "config_file": settings.config_file, "config": settings.config });
    request(&mut process, "setup", params, events)
        .map_err(|error| format!("Emacs could not load Org with your configuration: {error}"))?;
    Ok(process)
}

/// Send one request and wait for its answer, passing on the questions
/// Emacs asks meanwhile.
fn request(process: &mut Process, method: &str, params: Value, events: &Sender<Event>) -> Result<Value, String> {
    process.next_id += 1;
    let id = process.next_id;
    let line = ascii_json(&json!({ "id": id, "method": method, "params": params }));
    let stopped = |process: &mut Process| {
        let said: Vec<String> = process.stderr.lock().map(|kept| kept.iter().cloned().collect()).unwrap_or_default();
        let said = said.join("\n");
        if said.trim().is_empty() {
            STOPPED.to_string()
        } else {
            format!("{STOPPED}: {}", said.trim())
        }
    };
    if writeln!(process.stdin, "{line}").and_then(|()| process.stdin.flush()).is_err() {
        return Err(stopped(process));
    }
    loop {
        let Ok(line) = process.lines.recv() else { return Err(stopped(process)) };
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(question) = message.get("ask") {
            let prompt = question.get("prompt").and_then(Value::as_str).unwrap_or("").to_string();
            let (sender, receiver) = crossbeam_channel::bounded(1);
            let answer = if events.send(Event::Ask { prompt, answer: sender }).is_ok() {
                receiver.recv().unwrap_or(false)
            } else {
                false
            };
            let reply = ascii_json(&json!({ "answer": answer }));
            if writeln!(process.stdin, "{reply}").and_then(|()| process.stdin.flush()).is_err() {
                return Err(stopped(process));
            }
            continue;
        }
        if message.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = message.get("error").and_then(Value::as_str) {
            return Err(error.to_string());
        }
        return Ok(message.get("result").cloned().unwrap_or(Value::Null));
    }
}

/// `value` as JSON on one line with every character beyond ASCII escaped,
/// whatever coding system Emacs reads its input with.
fn ascii_json(value: &Value) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_beyond_ascii() {
        assert_eq!(ascii_json(&json!("\u{e9}\u{1f600}")), r#""\u00e9\ud83d\ude00""#);
    }
}
