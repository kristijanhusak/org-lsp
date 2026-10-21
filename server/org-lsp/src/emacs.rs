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
//! as events too, and wait for their answer, as do notices for the user
//! (an option of `emacs_options` Emacs did not set).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    /// Org's options to set after both, each once its library is loaded
    /// (`emacs_options`).
    pub options: Vec<OrgOption>,
}

/// An option of Org that Emacs sets to `value` once the library it is in,
/// `feature`, is loaded, as Customize would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgOption {
    pub name: String,
    pub feature: String,
    pub value: Value,
}

/// What the Emacs thread has for the server's loop.
pub enum Event {
    /// The answer to the request [`Emacs::start`] was given `tag` for.
    Done { tag: u64, result: Result<Value, String> },
    /// Emacs asks a yes-or-no question; the answer goes to `answer`.
    Ask { prompt: String, answer: Sender<bool> },
    /// Emacs asked a question while the server waited for it, which could
    /// not be put to the user: it was answered no.
    Declined { prompt: String },
    /// Emacs has something to tell the user: an option of `emacs_options`
    /// it did not set.
    Notice { text: String },
}

struct Job {
    method: String,
    params: Value,
    reply: Reply,
    /// How long Emacs may take over a job the server waits for
    /// (`Reply::Now`), from when the job is sent to it: the time Emacs
    /// takes to start is not the job's.
    timeout: Option<Duration>,
    /// When the job was sent to Emacs, for the server that waits.
    begun: Arc<Mutex<Option<Instant>>>,
    /// The server stopped waiting before the job was sent to Emacs.
    abandoned: Arc<AtomicBool>,
}

impl Job {
    fn new(method: &str, params: Value, reply: Reply, timeout: Option<Duration>) -> Job {
        Job {
            method: method.to_string(),
            params,
            reply,
            timeout,
            begun: Arc::new(Mutex::new(None)),
            abandoned: Arc::new(AtomicBool::new(false)),
        }
    }
}

enum Reply {
    Now(Sender<Result<Value, String>>),
    Later(u64),
}

/// The link to Emacs. Dropping it stops Emacs.
pub struct Emacs {
    jobs: Sender<Job>,
    events: Receiver<Event>,
    starting: Arc<AtomicBool>,
}

impl Emacs {
    pub fn new(settings: Settings) -> Emacs {
        let (jobs, job_receiver) = crossbeam_channel::unbounded::<Job>();
        let (event_sender, events) = crossbeam_channel::unbounded();
        let starting = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&starting);
        let _ = std::thread::Builder::new()
            .name("org-lsp-emacs".to_string())
            .spawn(move || serve(&settings, &job_receiver, &event_sender, &flag));
        Emacs { jobs, events, starting }
    }

    /// What the server's loop listens to.
    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }

    /// Run `method` and wait for its answer: at most `timeout` once Emacs
    /// has it, and while Emacs starts, until it has started.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.caller().call(method, params, timeout)
    }

    /// A handle that runs methods and waits for them, as [`Emacs::call`],
    /// for those that hold no reference to the server.
    pub fn caller(&self) -> Caller {
        Caller { jobs: self.jobs.clone(), starting: Arc::clone(&self.starting) }
    }

    /// Run `method`; its answer comes as [`Event::Done`] with `tag`.
    pub fn start(&self, tag: u64, method: &str, params: Value) {
        let _ = self.jobs.send(Job::new(method, params, Reply::Later(tag), None));
    }
}

/// See [`Emacs::caller`].
#[derive(Clone)]
pub struct Caller {
    jobs: Sender<Job>,
    starting: Arc<AtomicBool>,
}

impl Caller {
    /// [`Emacs::call`]
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let job = Job::new(method, params, Reply::Now(sender), Some(timeout));
        let (begun, abandoned) = (Arc::clone(&job.begun), Arc::clone(&job.abandoned));
        self.jobs.send(job).map_err(|_| "Emacs is not running".to_string())?;
        let asked = Instant::now();
        let mut until = asked + timeout;
        loop {
            match receiver.recv_deadline(until) {
                Ok(result) => return result,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Err(STOPPED.to_string()),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            let sent = begun.lock().ok().and_then(|begun| *begun);
            match sent {
                // Emacs has the job: it has its own time from then on.
                Some(sent) if sent + timeout > now => until = sent + timeout,
                // Emacs is starting: its start is not the job's time.
                None if self.starting.load(Ordering::SeqCst) && now < asked + SETUP_TIME => {
                    until = now + Duration::from_millis(50);
                }
                _ => {
                    abandoned.store(true, Ordering::SeqCst);
                    return Err(LATE.to_string());
                }
            }
        }
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
fn serve(settings: &Settings, jobs: &Receiver<Job>, events: &Sender<Event>, starting: &AtomicBool) {
    let mut process: Option<Process> = None;
    for job in jobs {
        // A job the server no longer waits for, behind a long one.
        if job.abandoned.load(Ordering::SeqCst) {
            answer(job.reply, Err(LATE.to_string()), events);
            continue;
        }
        if process.is_none() {
            starting.store(true, Ordering::SeqCst);
            let started = start(settings, events);
            // The job's time starts before Emacs stops starting: a caller
            // that looks in between sees one or the other.
            if started.is_ok() {
                if let Ok(mut begun) = job.begun.lock() {
                    *begun = Some(Instant::now());
                }
            }
            starting.store(false, Ordering::SeqCst);
            match started {
                Ok(started) => process = Some(started),
                Err(error) => {
                    answer(job.reply, Err(error), events);
                    continue;
                }
            }
            // The server stopped waiting while Emacs started: Emacs stays,
            // for the next job.
            if job.abandoned.load(Ordering::SeqCst) {
                answer(job.reply, Err(LATE.to_string()), events);
                continue;
            }
        }
        let Some(running) = process.as_mut() else { continue };
        let now = Instant::now();
        if let Ok(mut begun) = job.begun.lock() {
            *begun = Some(now);
        }
        // A job the server does not wait for may ask the user, and has
        // a long time; past it, Emacs is taken for stuck.
        let asks = job.timeout.is_none();
        let deadline = now + job.timeout.unwrap_or(LATER_TIME);
        let result = request(running, &job.method, job.params, events, deadline, asks);
        // An Emacs that stopped, or that is stuck past the time the server
        // waits (a question no one answers, a loop): the next job starts
        // a new one, and this one goes with `process`.
        if result.as_ref().is_err_and(|error| error.starts_with(STOPPED) || error == LATE) {
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
const LATE: &str = "Emacs did not answer in time";

/// The longest Emacs may take to start and load Org and the user's
/// configuration, and to answer a question the user is asked.
const SETUP_TIME: Duration = Duration::from_secs(120);
const QUESTION_TIME: Duration = Duration::from_secs(600);
/// The longest a job the server does not wait for may take (an export, a
/// publication, a preview), questions to the user included; a second in
/// the tests.
const LATER_TIME: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 1800 });

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
    // Each option's value as JSON text, which Emacs reads as it reads a
    // value of its own: no Lisp is written from what the user gave.
    let options: Vec<Value> = settings
        .options
        .iter()
        .map(|option| json!({ "name": option.name, "feature": option.feature, "json": option.value.to_string() }))
        .collect();
    let params = json!({ "config_file": settings.config_file, "config": settings.config, "options": options });
    request(&mut process, "setup", params, events, Instant::now() + SETUP_TIME, false)
        .map_err(|error| format!("Emacs could not load Org with your configuration: {error}"))?;
    Ok(process)
}

/// Send one request and wait for its answer until `deadline`. The
/// questions Emacs asks meanwhile go to the user when it `asks`, or are
/// answered no.
fn request(
    process: &mut Process,
    method: &str,
    params: Value,
    events: &Sender<Event>,
    deadline: Instant,
    asks: bool,
) -> Result<Value, String> {
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
        let line = match process.lines.recv_deadline(deadline) {
            Ok(line) => line,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => return Err(LATE.to_string()),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Err(stopped(process)),
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(question) = message.get("ask") {
            let prompt = question.get("prompt").and_then(Value::as_str).unwrap_or("").to_string();
            // While the server waits for this job it cannot ask the user:
            // no. Otherwise the user is asked, and no answer is no.
            let (sender, receiver) = crossbeam_channel::bounded(1);
            let answer = if asks {
                events.send(Event::Ask { prompt, answer: sender }).is_ok()
                    && receiver.recv_timeout(QUESTION_TIME).unwrap_or(false)
            } else {
                let _ = events.send(Event::Declined { prompt });
                false
            };
            let reply = ascii_json(&json!({ "answer": answer }));
            if writeln!(process.stdin, "{reply}").and_then(|()| process.stdin.flush()).is_err() {
                return Err(stopped(process));
            }
            continue;
        }
        if let Some(text) = message.get("notice").and_then(Value::as_str) {
            let _ = events.send(Event::Notice { text: text.to_string() });
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

    /// An Emacs that answers each request with its process id, and never
    /// answers `hang`.
    #[cfg(unix)]
    fn fake_emacs(directory: &std::path::Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("emacs");
        let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"hang"'*) ;;
    *) printf '\036org-lsp {"id":%s,"result":%s}\n' "$id" "$$" ;;
  esac
done
"#;
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[cfg(unix)]
    #[test]
    fn starts_emacs_again_after_one_that_does_not_answer() {
        let directory = std::env::temp_dir().join(format!("org-lsp-fake-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        let second = Duration::from_secs(5);
        let first = emacs.call("ping", Value::Null, second).unwrap();
        assert_eq!(emacs.call("hang", Value::Null, Duration::from_millis(300)), Err(LATE.to_string()));
        // The stuck Emacs is gone: a new one answers.
        let again = emacs.call("ping", Value::Null, second).unwrap();
        assert_ne!(first, again);
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// A job the server does not wait for has a deadline too: an Emacs
    /// stuck on it is stopped, the job fails, and the next starts anew.
    #[cfg(unix)]
    #[test]
    fn gives_up_a_job_no_one_waits_for_when_emacs_is_stuck() {
        let directory = std::env::temp_dir().join(format!("org-lsp-stuck-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        let first = emacs.call("ping", Value::Null, Duration::from_secs(5)).unwrap();
        emacs.start(7, "hang", Value::Null);
        match emacs.events().recv_timeout(Duration::from_secs(10)) {
            Ok(Event::Done { tag, result }) => {
                assert_eq!(tag, 7);
                assert_eq!(result, Err(LATE.to_string()));
            }
            _ => panic!("the job did not end"),
        }
        let again = emacs.call("ping", Value::Null, Duration::from_secs(5)).unwrap();
        assert_ne!(first, again, "a new Emacs answers");
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// An Emacs that takes `seconds` to start, then answers as
    /// `fake_emacs` does.
    #[cfg(unix)]
    fn slow_emacs(directory: &std::path::Path, seconds: f64) -> String {
        use std::os::unix::fs::PermissionsExt;
        let fast = fake_emacs(directory);
        let path = directory.join("slow-emacs");
        std::fs::write(&path, format!("#!/bin/sh\nsleep {seconds}\nexec {fast}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[cfg(unix)]
    #[test]
    fn does_not_count_the_start_of_emacs_against_a_call() {
        let directory = std::env::temp_dir().join(format!("org-lsp-slow-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: slow_emacs(&directory, 1.0), ..Settings::default() });
        // Emacs takes a second to start; the call has half a second of its
        // own once it has started.
        let first = emacs.call("ping", Value::Null, Duration::from_millis(500)).unwrap();
        let again = emacs.call("ping", Value::Null, Duration::from_millis(500)).unwrap();
        assert_eq!(first, again, "the same Emacs answers both");
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// `emacs_options` in a real Emacs: each value made what the option's
    /// type wants, set after the user's Lisp, or left out with a notice.
    #[test]
    fn sets_org_options_as_their_types_want() {
        let found = Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
        if !found {
            assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
            eprintln!("sets_org_options_as_their_types_want: no Emacs, skipped");
            return;
        }
        let load =
            std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
        let option = |name: &str, feature: &str, value: Value| OrgOption {
            name: name.to_string(),
            feature: feature.to_string(),
            value,
        };
        let emacs = Emacs::new(Settings {
            program: "emacs".to_string(),
            config: Some(format!(
                "{load}\n(setq org-attach-method 'ln)\n\
                 (with-eval-after-load 'ox (setq org-export-with-toc 3))\n\
                 (defun my-headline (&rest _) \"\")"
            )),
            options: vec![
                // A choice of constants, a string for a symbol.
                option("org-attach-method", "org-attach", json!("mv")),
                option("org-attach-use-inheritance", "org-attach", json!(true)),
                // A choice of constants and an integer.
                option("org-export-with-toc", "ox", json!(false)),
                option("org-export-with-sub-superscripts", "ox", json!("{}")),
                option("org-export-exclude-tags", "ox", json!(["private", "draft"])),
                // An alist with symbols for keys, and a list of pairs
                // from a table.
                option("org-html-text-markup-alist", "ox-html", json!({ "bold": "<b>%s</b>" })),
                option("org-structure-template-alist", "org", json!({ "q": "quote", "x": "example" })),
                // A property list.
                option("org-format-latex-options", "org", json!({ "scale": 2.0 })),
                option("org-html-table-default-attributes", "ox-html", json!({ "border": "1" })),
                // Keywords for constants, in a property list.
                option(
                    "org-export-smart-quotes-alist",
                    "ox",
                    json!({ "xx": { "primary-opening": { "utf-8": "«", ":html": "&laquo;" } } }),
                ),
                // A list that ends with an inline repeat.
                option(
                    "org-latex-classes",
                    "ox-latex",
                    json!([["mini", "\\documentclass{article}", ["\\section{%s}", "\\section*{%s}"]]]),
                ),
                // In a choice, a constant's name is that constant, a string
                // a string where strings are taken (a keyword aside), and a
                // list that starts with a constant's name is that list.
                option("org-babel-python-command-session", "ob-python", json!("auto")),
                option("org-html-infojs-options", "ox-html", json!({ "path": "x.js", "toc": ":with-toc" })),
                option("org-export-with-drawers", "ox", json!(["not", "LOGBOOK"])),
                // A function, by its name.
                option("org-html-format-headline-function", "ox-html", json!("my-headline")),
                // Not what they take.
                option("org-attach-store-link-p", "org-attach", json!("move")),
                option("org-html-format-drawer-function", "ox-html", json!("my-drawer")),
                option("org-export-backends", "ox", json!(["ascii", "nosuch"])),
            ],
            ..Settings::default()
        });
        let value = |name: &str| {
            let sexp = format!(
                "(progn (require 'org-attach) (require 'ox-html) (require 'ox-latex) (require 'ob-python) \
                 (format \"%S\" {name}))"
            );
            emacs.call("eval", json!({ "sexp": sexp }), Duration::from_secs(60)).unwrap()
        };
        assert_eq!(value("org-attach-method"), "mv", "after `emacs_config`, over it");
        assert_eq!(value("org-attach-use-inheritance"), "t");
        assert_eq!(value("org-export-with-toc"), "nil", "after the user's `with-eval-after-load`");
        assert_eq!(value("org-export-with-sub-superscripts"), "{}");
        assert_eq!(value("org-export-exclude-tags"), r#"("private" "draft")"#);
        assert_eq!(value("org-html-text-markup-alist"), r#"((bold . "<b>%s</b>"))"#);
        assert_eq!(value("org-structure-template-alist"), r#"(("q" . "quote") ("x" . "example"))"#);
        assert_eq!(value("org-format-latex-options"), "(:scale 2.0)");
        assert_eq!(value("org-html-table-default-attributes"), r#"(:border "1")"#);
        assert_eq!(value("org-export-smart-quotes-alist"), r#"(("xx" (primary-opening :utf-8 "«" :html "&laquo;")))"#);
        assert_eq!(
            value("org-latex-classes"),
            r#"(("mini" "\\documentclass{article}" ("\\section{%s}" . "\\section*{%s}")))"#
        );
        assert_eq!(value("org-babel-python-command-session"), "auto");
        assert_eq!(value("org-html-infojs-options"), r#"((path . "x.js") (toc . :with-toc))"#);
        assert_eq!(value("org-export-with-drawers"), r#"(not "LOGBOOK")"#);
        assert_eq!(value("org-html-format-headline-function"), "my-headline");
        assert_eq!(value("org-attach-store-link-p"), "attached", "left as it was");
        let mut notices = Vec::new();
        while let Ok(event) = emacs.events().try_recv() {
            if let Event::Notice { text } = event {
                notices.push(text);
            }
        }
        assert_eq!(
            notices,
            [
                "`emacs_options` leaves out `org-attach-store-link-p`: \"move\" is not false, true, \"attached\" \
                 or \"file\"",
                // The item that is not what the option takes.
                "`emacs_options` leaves out `org-export-backends`: \"nosuch\" is not \"ascii\", \"beamer\", \
                 \"html\", \"icalendar\", \"latex\", \"man\", \"md\", \"odt\", \"org\", \"texinfo\", \
                 \"confluence\", \"deck\", \"freemind\", \"groff\", \"koma-letter\", \"rss\", \"s5\" or \
                 \"taskjuggler\"",
                "`emacs_options` leaves out `org-html-format-drawer-function`: \"my-drawer\" is not the name of a \
                 defined function",
            ]
        );
    }

    #[test]
    fn escapes_beyond_ascii() {
        assert_eq!(ascii_json(&json!("\u{e9}\u{1f600}")), r#""\u00e9\ud83d\ude00""#);
    }
}
