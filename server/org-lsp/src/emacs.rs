//! Emacs, for what only Org itself can do (export). The server starts one
//! `emacs --batch` when a command first needs it and keeps it running, so
//! that Org loads once; `bridge.el` answers its requests. Emacs is
//! optional: without it, those commands say so and nothing else changes.
//!
//! A thread of its own talks to Emacs, so that a long export never holds
//! up the server. A request either waits for its answer
//! ([`Emacs::call`], for what is quick and has no request of the editor's
//! to answer later) or is answered later as an [`Event`] the server's loop
//! receives ([`Emacs::start`], and [`Emacs::wait`] for a request of the
//! editor's that is answered once Emacs is done). Questions Emacs asks on
//! the way (`y-or-n-p`) come as events too, and wait for their answer, as
//! do notices for the user (an option of `emacs_options` Emacs did not
//! set). A job may be canceled ([`Emacs::cancel`]): one that runs stops
//! Emacs, which the next job starts again.

use std::collections::{HashSet, VecDeque};
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
    /// What Org said where nothing else gives it back (a diary sexp that
    /// failed), as the echo area shows it.
    Message { text: String },
}

struct Job {
    method: String,
    params: Value,
    reply: Reply,
    /// How long Emacs may take over the job, from when it is sent to it:
    /// the time Emacs takes to start is not the job's. None: as long as an
    /// export may take ([`LATER_TIME`]).
    timeout: Option<Duration>,
    /// Whether the questions Emacs asks go to the user (the server does
    /// not wait for the job), or are answered no.
    asks: bool,
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
            asks: matches!(reply, Reply::Later(_)),
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

/// The link to Emacs. Dropping it stops Emacs, busy or not, with what it
/// started (a code block's program).
pub struct Emacs {
    jobs: Sender<Job>,
    events: Receiver<Event>,
    starting: Arc<AtomicBool>,
    /// The process id of the Emacs running, whose process group it leads.
    running: Arc<Mutex<Option<u32>>>,
    /// The tag of the job Emacs runs, of those answered later.
    current: Arc<Mutex<Option<u64>>>,
    /// The tags of jobs canceled before Emacs runs them.
    canceled: Arc<Mutex<HashSet<u64>>>,
}

impl Drop for Emacs {
    fn drop(&mut self) {
        if let Some(id) = self.running.lock().ok().and_then(|mut running| running.take()) {
            stop_group(id);
        }
    }
}

impl Emacs {
    pub fn new(settings: Settings) -> Emacs {
        let (jobs, job_receiver) = crossbeam_channel::unbounded::<Job>();
        let (event_sender, events) = crossbeam_channel::unbounded();
        let starting = Arc::new(AtomicBool::new(false));
        let running = Arc::new(Mutex::new(None));
        let current = Arc::new(Mutex::new(None));
        let canceled = Arc::new(Mutex::new(HashSet::new()));
        let shared = Shared {
            starting: Arc::clone(&starting),
            process_id: Arc::clone(&running),
            current: Arc::clone(&current),
            canceled: Arc::clone(&canceled),
        };
        let _ = std::thread::Builder::new()
            .name("org-lsp-emacs".to_string())
            .spawn(move || serve(&settings, &job_receiver, &event_sender, &shared));
        Emacs { jobs, events, starting, running, current, canceled }
    }

    /// What the server's loop listens to.
    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }

    /// Run `method` and wait for its answer: at most `timeout` once Emacs
    /// has it, and while Emacs starts, [`START_WAIT`] at most (the server holds
    /// up its other answers meanwhile); an answer that is late says so.
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

    /// Run `method` for a request of the editor's that waits for it,
    /// without holding up the server: its answer comes as [`Event::Done`]
    /// with `tag`, at most `timeout` after Emacs has it. The questions
    /// Emacs asks go to the user.
    pub fn wait(&self, tag: u64, method: &str, params: Value, timeout: Duration) {
        let _ = self.jobs.send(Job::new(method, params, Reply::Later(tag), Some(timeout)));
    }

    /// Give up the job of `tag`: one that waits is left out, and one that
    /// runs stops Emacs (and what it started), which the next job starts
    /// again. Its answer comes still, as a failure.
    pub fn cancel(&self, tag: u64) {
        if let Ok(mut canceled) = self.canceled.lock() {
            canceled.insert(tag);
        }
        let running = self.current.lock().ok().and_then(|current| *current) == Some(tag);
        if running {
            if let Some(id) = self.running.lock().ok().and_then(|running| *running) {
                stop_group(id);
            }
        }
    }
}

/// What the server and the Emacs thread both look at.
struct Shared {
    starting: Arc<AtomicBool>,
    process_id: Arc<Mutex<Option<u32>>>,
    current: Arc<Mutex<Option<u64>>>,
    canceled: Arc<Mutex<HashSet<u64>>>,
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
        // Looked at often until Emacs has the job.
        let mut until = asked + Duration::from_millis(50);
        loop {
            match receiver.recv_deadline(until) {
                Ok(result) => return result,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Err(STOPPED.to_string()),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            let sent = begun.lock().ok().and_then(|begun| *begun);
            let starting = self.starting.load(Ordering::SeqCst);
            match sent {
                // Emacs has the job: it has its own time from then on.
                Some(sent) if sent + timeout > now => until = sent + timeout,
                Some(_) => {
                    abandoned.store(true, Ordering::SeqCst);
                    return Err(late(method, timeout));
                }
                // Emacs is starting: its start is not the job's time, but
                // the server waits for it only so long. Emacs goes on
                // starting, for the next job.
                None if starting && now < asked + START_WAIT => until = now + Duration::from_millis(50),
                None if starting => {
                    abandoned.store(true, Ordering::SeqCst);
                    return Err(STARTING.to_string());
                }
                // Emacs is busy with a job before this one (an export): the
                // server waits for it only a moment.
                None if now < asked + BUSY_WAIT.min(timeout) => until = now + Duration::from_millis(50),
                None => {
                    abandoned.store(true, Ordering::SeqCst);
                    return Err(BUSY.to_string());
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
        stop_group(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Stop the Emacs of process id `id` and what it started: its process
/// group, which [`start`] made.
fn stop_group(id: u32) {
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(id) {
        // SAFETY: `kill` only sends a signal; a group already gone is an
        // error, ignored.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = id;
}

/// The thread: run each job in Emacs, which starts with the first one and
/// again after it stopped.
fn serve(settings: &Settings, jobs: &Receiver<Job>, events: &Sender<Event>, shared: &Shared) {
    let (starting, process_id) = (&shared.starting, &shared.process_id);
    let mut process: Option<Process> = None;
    for job in jobs {
        // A job the server no longer waits for, behind a long one.
        if job.abandoned.load(Ordering::SeqCst) {
            answer(job.reply, Err(LATE.to_string()), events);
            continue;
        }
        let tag = match job.reply {
            Reply::Later(tag) => Some(tag),
            Reply::Now(_) => None,
        };
        if tag.is_some_and(|tag| shared.canceled.lock().is_ok_and(|mut canceled| canceled.remove(&tag))) {
            answer(job.reply, Err(CANCELED.to_string()), events);
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
                Ok(started) => {
                    if let Ok(mut running) = process_id.lock() {
                        *running = Some(started.child.id());
                    }
                    process = Some(started);
                }
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
        // A job the server does not wait for may ask the user; past its
        // time, Emacs is taken for stuck.
        let deadline = now + job.timeout.unwrap_or(LATER_TIME);
        if let Ok(mut current) = shared.current.lock() {
            *current = tag;
        }
        // Canceled while Emacs started: the job is not run. Canceled from
        // now on, Emacs is stopped.
        let skipped = tag.is_some_and(|tag| shared.canceled.lock().is_ok_and(|canceled| canceled.contains(&tag)));
        let result = if skipped {
            Err(CANCELED.to_string())
        } else {
            request(running, &job.method, job.params, events, deadline, job.asks)
        };
        if let Ok(mut current) = shared.current.lock() {
            *current = None;
        }
        // Canceled while it ran: Emacs was stopped.
        let canceled = tag.is_some_and(|tag| shared.canceled.lock().is_ok_and(|mut canceled| canceled.remove(&tag)));
        let result = if canceled { Err(CANCELED.to_string()) } else { result };
        // An Emacs that stopped, or that is stuck past the time the server
        // waits (a question no one answers, a loop): the next job starts
        // a new one, and this one goes with `process`.
        if (canceled && !skipped) || result.as_ref().is_err_and(|error| error.starts_with(STOPPED) || error == LATE) {
            process = None;
            if let Ok(mut running) = process_id.lock() {
                *running = None;
            }
        }
        let waited = job.timeout.unwrap_or(LATER_TIME);
        let result = result.map_err(|error| if error == LATE { late(&job.method, waited) } else { error });
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
/// The answer of a job given up ([`Emacs::cancel`]).
pub const CANCELED: &str = "Canceled";
const LATE: &str = "Emacs did not answer in time";
const STARTING: &str = "Emacs is still starting (loading `emacs_config_file`, `emacs_config` and Org); \
                        try again once it has";
const BUSY: &str = "Emacs is busy with another command (an export, a code block); try again once it is done";

/// [`LATE`], with what Emacs was asked (the bridge's method) and how long
/// it had: Emacs is then started again.
fn late(method: &str, waited: Duration) -> String {
    format!("{LATE}: `{method}` took over {} s; it is started again", waited.as_secs().max(1))
}

/// The longest Emacs may take to start and load Org and the user's
/// configuration, and to answer a question the user is asked.
const SETUP_TIME: Duration = Duration::from_secs(120);
/// The longest the server waits, holding up its other answers, for Emacs
/// to be done with another job before one it waits for ([`Emacs::call`]).
const BUSY_WAIT: Duration = Duration::from_millis(if cfg!(test) { 300 } else { 3000 });
/// The longest the server waits, holding up its other answers, for Emacs
/// to start before a job it waits for ([`Emacs::call`]); Emacs goes on
/// starting.
const START_WAIT: Duration = Duration::from_millis(if cfg!(test) { 1000 } else { 15_000 });
const QUESTION_TIME: Duration = Duration::from_secs(600);
/// The longest a job the server does not wait for may take (an export, a
/// publication, a preview), questions to the user included; a second in
/// the tests.
const LATER_TIME: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 1800 });

/// Start Emacs and set it up: the user's configuration, then Org.
fn start(settings: &Settings, events: &Sender<Event>) -> Result<Process, String> {
    let program = if settings.program.is_empty() { "emacs" } else { &settings.program };
    let mut command = Command::new(program);
    command
        .args(["--batch", "-Q", "--eval", &format!("(progn {BRIDGE})")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A process group of its own, which goes with it when it is stopped.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child =
        command.spawn().map_err(|error| format!("Cannot start Emacs (`{program}`, the `emacs` option): {error}"))?;
    let stdin = child.stdin.take().ok_or("Emacs has no input")?;
    let stdout = child.stdout.take().ok_or("Emacs has no output")?;
    let stderr = child.stderr.take().ok_or("Emacs has no error output")?;
    let (line_sender, lines) = crossbeam_channel::unbounded();
    let _ = std::thread::Builder::new().name("org-lsp-emacs-out".to_string()).spawn(move || {
        for line in lossy_lines(stdout) {
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
        for line in lossy_lines(stderr) {
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
    request(&mut process, "setup", params, events, Instant::now() + SETUP_TIME, false).map_err(|error| {
        let error = if error == LATE { format!("it took over {} s", SETUP_TIME.as_secs()) } else { error };
        format!("Emacs could not load Org with your configuration: {error}")
    })?;
    Ok(process)
}

/// The lines of `output` until it ends or cannot be read, what is not
/// UTF-8 in them (printed by the user's Lisp or a package) replaced.
fn lossy_lines(output: impl std::io::Read) -> impl Iterator<Item = String> {
    let mut output = BufReader::new(output);
    std::iter::from_fn(move || {
        let mut line = Vec::new();
        match output.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                if line.ends_with(b"\n") {
                    line.pop();
                    if line.ends_with(b"\r") {
                        line.pop();
                    }
                }
                Some(String::from_utf8_lossy(&line).into_owned())
            }
        }
    })
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
    // What Emacs said on its standard error before this request (the
    // warnings of an earlier one) is not why it stops during this one.
    if let Ok(mut kept) = process.stderr.lock() {
        kept.clear();
    }
    let line = ascii_json(&json!({ "id": id, "method": method, "params": params }));
    let stopped = |process: &mut Process| {
        let said: Vec<String> = process.stderr.lock().map(|kept| kept.iter().cloned().collect()).unwrap_or_default();
        let said = said.join("\n");
        if !said.trim().is_empty() {
            return format!("{STOPPED}: {}", said.trim());
        }
        // Nothing said: how it ended, once it has (its output closes
        // first).
        let until = Instant::now() + Duration::from_secs(1);
        loop {
            match process.child.try_wait() {
                Ok(Some(status)) => match status.code() {
                    Some(code) => return format!("{STOPPED} (exit status {code})"),
                    None => return format!("{STOPPED} ({status})"),
                },
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
                _ => return STOPPED.to_string(),
            }
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
        if let Some(text) = message.get("message").and_then(Value::as_str) {
            let _ = events.send(Event::Message { text: text.to_string() });
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

    /// An Emacs that answers each request with its process id, never
    /// answers `hang`, and ends with the status 3 at `quit`; at `bytes` it
    /// prints lines that are not UTF-8 first, at `warn` a warning on its
    /// standard error.
    #[cfg(unix)]
    fn fake_emacs(directory: &std::path::Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("emacs");
        let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"hang"'*) ;;
    *'"method":"quit"'*) exit 3 ;;
    *'"method":"bytes"'*) printf '\377 \036org-lsp \376\n\036org-lsp {"id":%s,"result":%s}\n' "$id" "$$" ;;
    *'"method":"warn"'*) echo 'Warning: old' >&2; printf '\036org-lsp {"id":%s,"result":%s}\n' "$id" "$$" ;;
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
        assert_eq!(
            emacs.call("hang", Value::Null, Duration::from_millis(300)),
            Err(late("hang", Duration::from_millis(300)))
        );
        // The stuck Emacs is gone: a new one answers.
        let again = emacs.call("ping", Value::Null, second).unwrap();
        assert_ne!(first, again);
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// A call behind a long job waits for it only a moment.
    #[cfg(unix)]
    #[test]
    fn says_emacs_is_busy_with_another_job() {
        let directory = std::env::temp_dir().join(format!("org-lsp-taken-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        emacs.call("ping", Value::Null, Duration::from_secs(5)).unwrap();
        emacs.start(1, "hang", Value::Null);
        let asked = Instant::now();
        assert_eq!(emacs.call("ping", Value::Null, Duration::from_secs(30)), Err(BUSY.to_string()));
        assert!(asked.elapsed() < Duration::from_secs(1), "{:?}", asked.elapsed());
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// Dropped while Emacs is busy, Emacs is stopped, with what it started.
    #[cfg(unix)]
    #[test]
    fn stops_a_busy_emacs_when_dropped() {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("org-lsp-busy-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let marker = directory.join("ran-on");
        let path = directory.join("emacs");
        let script = format!(
            "#!/bin/sh\nwhile IFS= read -r line; do\n  case \"$line\" in\n    *'\"method\":\"setup\"'*) \
             printf '\\036org-lsp {{\"id\":1,\"result\":{{}}}}\\n' ;;\n    *) (sleep 0.5; touch {}) & sleep 30 ;;\n  \
             esac\ndone\n",
            marker.display()
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let emacs = Emacs::new(Settings { program: path.display().to_string(), ..Settings::default() });
        emacs.start(1, "run", Value::Null);
        std::thread::sleep(Duration::from_millis(300));
        drop(emacs);
        std::thread::sleep(Duration::from_millis(900));
        assert!(!marker.exists(), "what the busy Emacs started ran on");
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// An Emacs that stops with nothing on its standard error: how it
    /// ended is said.
    #[cfg(unix)]
    #[test]
    fn says_how_emacs_stopped() {
        let directory = std::env::temp_dir().join(format!("org-lsp-quit-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        let second = Duration::from_secs(5);
        assert_eq!(emacs.call("quit", Value::Null, second), Err("Emacs stopped (exit status 3)".to_string()));
        // The next job starts another.
        assert!(emacs.call("ping", Value::Null, second).is_ok());
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// A line that is not UTF-8 is read all the same, and the answer
    /// after it comes from the same Emacs.
    #[cfg(unix)]
    #[test]
    fn reads_what_is_not_utf8_from_emacs() {
        let directory = std::env::temp_dir().join(format!("org-lsp-bytes-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        let second = Duration::from_secs(5);
        let first = emacs.call("ping", Value::Null, second).unwrap();
        assert_eq!(emacs.call("bytes", Value::Null, second), Ok(first.clone()));
        assert_eq!(emacs.call("ping", Value::Null, second), Ok(first));
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// An Emacs that stops with nothing said is not reported with the
    /// warnings of an earlier job.
    #[cfg(unix)]
    #[test]
    fn says_why_emacs_stopped_from_what_it_said_meanwhile() {
        let directory = std::env::temp_dir().join(format!("org-lsp-warned-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: fake_emacs(&directory), ..Settings::default() });
        let second = Duration::from_secs(5);
        assert!(emacs.call("warn", Value::Null, second).is_ok());
        // The warning reaches the server's side before the next job.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(emacs.call("quit", Value::Null, second), Err("Emacs stopped (exit status 3)".to_string()));
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
                assert_eq!(result, Err(late("hang", LATER_TIME)));
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

    /// An Emacs that takes long to start holds up a call only so long, and
    /// goes on starting for the next.
    #[cfg(unix)]
    #[test]
    fn waits_for_emacs_to_start_only_so_long() {
        let directory = std::env::temp_dir().join(format!("org-lsp-slower-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        // Started half a second after the first call gives up
        // (`START_WAIT`), and half a second before the next one would.
        let emacs = Emacs::new(Settings { program: slow_emacs(&directory, 1.5), ..Settings::default() });
        let asked = Instant::now();
        assert_eq!(emacs.call("ping", Value::Null, Duration::from_millis(500)), Err(STARTING.to_string()));
        assert!(asked.elapsed() < Duration::from_millis(1500), "{:?}", asked.elapsed());
        // The same Emacs, started.
        assert!(emacs.call("ping", Value::Null, Duration::from_secs(10)).is_ok());
        drop(emacs);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn does_not_count_the_start_of_emacs_against_a_call() {
        let directory = std::env::temp_dir().join(format!("org-lsp-slow-emacs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let emacs = Emacs::new(Settings { program: slow_emacs(&directory, 0.6), ..Settings::default() });
        // Emacs takes longer to start than the call has (half a second),
        // which has that half second of its own once it has started.
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
        let mut options = vec![
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
            // A list that ends with an inline repeat.
            option(
                "org-latex-classes",
                "ox-latex",
                json!([["mini", "\\documentclass{article}", ["\\section{%s}", "\\section*{%s}"]]]),
            ),
            // In a choice, a constant's name is that constant, a string
            // a string where strings are taken (a keyword aside), and a
            // list that starts with a constant's name is that list.
            option("org-html-infojs-options", "ox-html", json!({ "path": "x.js", "toc": ":with-toc" })),
            option("org-export-with-drawers", "ox", json!(["not", "LOGBOOK"])),
            // A function, by its name.
            option("org-html-format-headline-function", "ox-html", json!("my-headline")),
            // Not what they take.
            option("org-attach-store-link-p", "org-attach", json!("move")),
            option("org-html-format-drawer-function", "ox-html", json!("my-drawer")),
            option("org-export-backends", "ox", json!(["ascii", "nosuch"])),
        ];
        // Org 9.8's (`ORG_LISP_DIR`), not 9.6's: keywords for constants in
        // a property list, and a constant's name in a choice with a string.
        let org_98 = std::env::var_os("ORG_LISP_DIR").is_some();
        if org_98 {
            options.push(option(
                "org-export-smart-quotes-alist",
                "ox",
                json!({ "xx": { "primary-opening": { "utf-8": "«", ":html": "&laquo;" } } }),
            ));
            options.push(option("org-babel-python-command-session", "ob-python", json!("auto")));
        }
        let emacs = Emacs::new(Settings {
            program: "emacs".to_string(),
            config: Some(format!(
                "{load}\n(setq org-attach-method 'ln)\n\
                 (with-eval-after-load 'ox (setq org-export-with-toc 3))\n\
                 (defun my-headline (&rest _) \"\")"
            )),
            options,
            ..Settings::default()
        });
        let value = |name: &str| {
            let sexp = format!(
                "(progn (require 'org-attach) (require 'ox-html) (require 'ox-latex) (require 'ob-python) \
                 (format \"%S\" {name}))"
            );
            // A machine under load takes longer to load Org than a call
            // waits for Emacs to start.
            loop {
                match emacs.call("eval", json!({ "sexp": sexp.clone() }), Duration::from_secs(60)) {
                    Err(error) if error == STARTING => continue,
                    result => return result.unwrap(),
                }
            }
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
        assert_eq!(
            value("org-latex-classes"),
            r#"(("mini" "\\documentclass{article}" ("\\section{%s}" . "\\section*{%s}")))"#
        );
        if org_98 {
            assert_eq!(
                value("org-export-smart-quotes-alist"),
                r#"(("xx" (primary-opening :utf-8 "«" :html "&laquo;")))"#
            );
            assert_eq!(value("org-babel-python-command-session"), "auto");
        }
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

    /// The Lisp of the settings Emacs is told (`org_src_lang_modes`, the
    /// tangling options), which runs before Org is loaded, in a real
    /// Emacs: what adds to Org's lists waits for their library.
    #[test]
    fn sets_the_settings_emacs_is_told_before_org_is_loaded() {
        let found = Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
        if !found {
            assert!(std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
            eprintln!("sets_the_settings_emacs_is_told_before_org_is_loaded: no Emacs, skipped");
            return;
        }
        let load =
            std::env::var("ORG_LISP_DIR").map(|dir| format!("(add-to-list 'load-path {dir:?})")).unwrap_or_default();
        let options: crate::config::Options = serde_json::from_value(json!({
            "org_src_lang_modes": { "ipython": "python-mode" },
            "org_babel_tangle_lang_exts": { "ipython": "py" },
            "org_babel_tangle_use_relative_file_links": false,
        }))
        .unwrap();
        let emacs = Emacs::new(Settings {
            program: "emacs".to_string(),
            config: Some(format!("{load}\n{}", options.emacs_lisp().unwrap())),
            ..Settings::default()
        });
        let sexp = "(progn (require 'org-src) (require 'ob-tangle) \
                    (format \"%S %S %S\" (assoc \"ipython\" org-src-lang-modes) \
                    (assoc \"ipython\" org-babel-tangle-lang-exts) org-babel-tangle-use-relative-file-links))";
        let value = loop {
            match emacs.call("eval", json!({ "sexp": sexp }), Duration::from_secs(60)) {
                Err(error) if error == STARTING => continue,
                result => break result.unwrap(),
            }
        };
        assert_eq!(value, r#"("ipython" . python) ("ipython" . "py") nil"#);
    }

    #[test]
    fn escapes_beyond_ascii() {
        assert_eq!(ascii_json(&json!("\u{e9}\u{1f600}")), r#""\u00e9\ud83d\ude00""#);
    }
}
