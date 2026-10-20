//! org-protocol: the `org-protocol://` URLs a browser's bookmarklet or
//! another program opens, which the system hands to
//!
//! ```text
//! org-lsp protocol URL
//! ```
//!
//! as it hands them to `emacsclient` for Emacs (`org-protocol.el`). The
//! subcommand gives the URL to a running server, which does what Org does
//! (`store-link`, `capture`, `open-source`); with none running, it starts
//! the editor of `org_protocol_editor` and leaves the URL for the server
//! that starts there.
//!
//! Every server listens from the start on a local socket of its own: a
//! Unix socket in a directory only the user can enter, or on Windows a
//! port of the loopback address with a token. The state directory
//! (`$XDG_STATE_HOME/org-lsp`, `~/.local/state/org-lsp`, on Windows
//! `%LOCALAPPDATA%\org-lsp`) has:
//!
//! - `protocol/PID-ID.json`: a server that listens, and where (its socket,
//!   or its address and token); the server touches it as the user works
//!   in its editor, and the subcommand tries the one touched last first.
//!   An entry whose server does not answer is a server that stopped
//!   without removing it: the subcommand removes it.
//! - `protocol/pending-TIME-PID.json`: a capture left for the server of
//!   the editor the subcommand started, taken by the first that starts
//!   within two minutes.
//! - `protocol/stored-links.jsonl`: links stored while no server ran, which
//!   the next server to start takes into its stored links.
//!
//! Nothing in a URL is ever run: it is read as Org reads it, and the
//! server only stores a link, starts a capture or shows a file.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use serde_json::{json, Value};

/// How long a URL left for a server that starts is good for.
pub const PENDING_FOR: Duration = Duration::from_secs(120);

/// The most a URL may have, as text.
const LONGEST: usize = 256 * 1024;

pub const USAGE: &str = "\
Usage: org-lsp protocol URL

Hand an org-protocol:// URL to org-lsp, as the system does for the
org-protocol scheme: store-link, capture or open-source, as Emacs's
org-protocol.el does them, in the editor where org-lsp runs. With none
running, the editor of `org_protocol_editor` is started for it; without
one, a link is kept for the next time org-lsp starts.";

/// A URL for the server, and where to say what came of it.
pub struct Sent {
    pub url: String,
    pub answer: crossbeam_channel::Sender<Answer>,
}

/// What the server says: Emacs's message, or why nothing was done.
pub type Answer = Result<String, String>;

/// The state directory: `$XDG_STATE_HOME/org-lsp`, else
/// `~/.local/state/org-lsp`, on Windows `%LOCALAPPDATA%\org-lsp`.
pub fn state_directory(var: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|value| !value.is_empty());
    if let Some(state) = set("XDG_STATE_HOME") {
        return Some(PathBuf::from(state).join("org-lsp"));
    }
    if cfg!(windows) {
        if let Some(local) = set("LOCALAPPDATA") {
            return Some(PathBuf::from(local).join("org-lsp"));
        }
    }
    let home = set("HOME").or_else(|| set("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".local").join("state").join("org-lsp"))
}

/// Where the servers say they listen, the pending captures and the
/// stored links are.
pub fn directory(var: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    state_directory(var).map(|state| state.join("protocol"))
}

/// Whether `url` may be an org-protocol URL at all: the scheme, no NUL,
/// not too long.
fn check(url: &str) -> Result<(), String> {
    let scheme = format!("{}:", org_core::protocol::THE_PROTOCOL);
    if !url.get(..scheme.len()).is_some_and(|start| start.eq_ignore_ascii_case(&scheme)) {
        return Err(format!("not an org-protocol URL: {url}"));
    }
    if url.contains('\0') || url.len() > LONGEST {
        return Err("the URL is malformed".to_string());
    }
    Ok(())
}

// ----- the server's side

/// A server's place to listen, and its entry in the directory.
pub struct Listener {
    entry: PathBuf,
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(not(unix))]
    address: std::net::SocketAddr,
    /// The entry's text, written again when touched.
    text: String,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// What a sender writes: the URL, and on Windows the token.
#[derive(Deserialize)]
struct Received {
    url: String,
    #[cfg_attr(unix, allow(dead_code))]
    token: Option<String>,
}

impl Listener {
    /// Listen in `directory` for URLs, which go to `sender`.
    pub fn open(directory: &Path, sender: crossbeam_channel::Sender<Sent>) -> std::io::Result<Listener> {
        create_private_dir(directory)?;
        let name = format!("{}-{}", std::process::id(), &crate::port::token()[..8]);
        let entry = directory.join(format!("{name}.json"));
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bind = |socket: PathBuf| {
                let _ = std::fs::remove_file(&socket);
                std::os::unix::net::UnixListener::bind(&socket).map(|listener| (socket, listener))
            };
            // A socket's name is short (`sun_path`): one too long for the
            // state directory, which Rust refuses as invalid, goes in a
            // private directory of `/tmp`, which the entry names.
            let (socket, listener) = match bind(directory.join(format!("{name}.sock"))) {
                Ok(bound) => bound,
                Err(error) if error.kind() != std::io::ErrorKind::InvalidInput => return Err(error),
                Err(error) => short_socket_directory()
                    .and_then(|short| bind(short.join(format!("{name}.sock"))))
                    .map_err(|short| std::io::Error::new(error.kind(), format!("{error}; in /tmp: {short}")))?,
            };
            // Only the user may connect, in a directory only the user
            // may enter.
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
            let text = json!({ "pid": std::process::id(), "socket": socket }).to_string();
            let done = closed.clone();
            std::thread::Builder::new().name("org-lsp-protocol".to_string()).spawn(move || {
                for stream in listener.incoming().flatten() {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    serve(stream, None, &sender);
                }
            })?;
            let listener = Listener { entry, socket, text, closed };
            listener.touch();
            Ok(listener)
        }
        #[cfg(not(unix))]
        {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
            let address = listener.local_addr()?;
            let token = crate::port::token();
            let text = json!({ "pid": std::process::id(), "address": address.to_string(), "token": token }).to_string();
            let done = closed.clone();
            std::thread::Builder::new().name("org-lsp-protocol".to_string()).spawn(move || {
                for stream in listener.incoming().flatten() {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    serve(stream, Some(&token), &sender);
                }
            })?;
            let listener = Listener { entry, address, text, closed };
            listener.touch();
            Ok(listener)
        }
    }

    /// Write the entry again: this server is the one the user worked in
    /// last, which the subcommand tries first.
    pub fn touch(&self) {
        let _ = write_private(&self.entry, &self.text);
    }

    /// Stop listening, and take the entry away: the server stops.
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.entry);
        // The listener waits for a connection: this one ends the wait.
        #[cfg(unix)]
        {
            let _ = std::os::unix::net::UnixStream::connect(&self.socket);
            let _ = std::fs::remove_file(&self.socket);
        }
        #[cfg(not(unix))]
        {
            let _ = std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(300));
        }
    }
}

/// Read one URL from `stream`, hand it to the server, and write back what
/// it says. A sender that is slow to say anything is not waited for.
fn serve<S: Read + Write + Timeouts>(mut stream: S, token: Option<&str>, sender: &crossbeam_channel::Sender<Sent>) {
    let answer = match receive(&mut stream, token) {
        Ok(url) => {
            let (answer, answered) = crossbeam_channel::bounded(1);
            match sender.send(Sent { url, answer }) {
                Ok(()) => answered
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap_or_else(|_| Err("org-lsp did not answer".to_string())),
                Err(_) => Err("org-lsp is stopping".to_string()),
            }
        }
        Err(error) => Err(error),
    };
    let line = match answer {
        Ok(message) => json!({ "message": message }),
        Err(error) => json!({ "error": error }),
    };
    let _ = stream.write_all(format!("{line}\n").as_bytes());
}

/// The read and write timeouts of a stream.
trait Timeouts {
    fn read_timeout_in(&self, left: Duration) -> std::io::Result<()>;
}

#[cfg(unix)]
impl Timeouts for std::os::unix::net::UnixStream {
    fn read_timeout_in(&self, left: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(left))
    }
}

impl Timeouts for std::net::TcpStream {
    fn read_timeout_in(&self, left: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(left))
    }
}

/// The URL a sender wrote, on one line of JSON, within two seconds.
fn receive<S: Read + Timeouts>(stream: &mut S, token: Option<&str>) -> Result<String, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut line = Vec::new();
    let mut chunk = [0u8; 4096];
    while !line.contains(&b'\n') {
        if line.len() > LONGEST + 1024 {
            return Err("the URL is too long".to_string());
        }
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or("nothing came")?;
        stream.read_timeout_in(left).map_err(|error| error.to_string())?;
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        line.extend_from_slice(&chunk[..read]);
    }
    let line = line.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let received: Received = serde_json::from_slice(line).map_err(|_| "not a URL".to_string())?;
    if let Some(token) = token {
        if received.token.as_deref() != Some(token) {
            return Err("refused".to_string());
        }
    }
    check(&received.url)?;
    Ok(received.url)
}

/// Take the file at `path` for this process alone: renamed, so that of
/// several servers that start together one takes it. Its text, and the
/// file is gone.
fn take(path: &Path) -> Option<String> {
    let taken = path.with_extension(format!("taken-{}", std::process::id()));
    std::fs::rename(path, &taken).ok()?;
    let text = std::fs::read_to_string(&taken).ok();
    let _ = std::fs::remove_file(&taken);
    text
}

/// The links stored while no server ran (`stored-links.jsonl`), the
/// first stored first, taken out of the file.
pub fn take_stored_links(directory: &Path) -> Vec<(String, Option<String>)> {
    let Some(text) = take(&directory.join(STORED_LINKS)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| serde_json::from_str::<(String, Option<String>)>(line).ok())
        .filter(|(url, _)| check_link(url))
        .collect()
}

/// A stored link may be anything but empty or overlong.
fn check_link(url: &str) -> bool {
    !url.is_empty() && url.len() <= LONGEST
}

/// The URLs left for a server that starts (`pending-*.json`), taken out:
/// those of the last two minutes, the oldest first.
pub fn take_pending(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut pending: Vec<(u64, String)> = Vec::new();
    let now = unix_time(SystemTime::now());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("pending-") && name.ends_with(".json")) {
            continue;
        }
        let Some(text) = take(&entry.path()) else { continue };
        let Ok(left) = serde_json::from_str::<Value>(&text) else { continue };
        let (Some(url), Some(time)) = (left["url"].as_str(), left["time"].as_u64()) else { continue };
        if now.saturating_sub(time) <= PENDING_FOR.as_secs() && check(url).is_ok() {
            pending.push((time, url.to_string()));
        }
    }
    pending.sort();
    pending.into_iter().map(|(_, url)| url).collect()
}

const STORED_LINKS: &str = "stored-links.jsonl";

fn unix_time(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(unix)]
fn create_private_dir(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

/// The directory for a socket whose name would be too long in the state
/// directory: `/tmp/org-lsp-UID`, made only the user's, or
/// taken when it is the user's already and only the user's.
#[cfg(unix)]
fn short_socket_directory() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    // The user's, who runs the server.
    // SAFETY: `getuid` only reads the process's user, and cannot fail.
    let uid = unsafe { libc::getuid() };
    let short = PathBuf::from(format!("/tmp/org-lsp-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&short) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    own_private_directory(&short, uid)?;
    Ok(short)
}

/// Whether `directory` is a directory (not a link) of the user `uid`'s
/// that only that user may enter.
#[cfg(unix)]
fn own_private_directory(directory: &Path, uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(directory)?;
    if metadata.is_dir() && metadata.uid() == uid && metadata.mode() & 0o777 == 0o700 {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("{} is not a directory only this user may enter", directory.display()),
    ))
}

#[cfg(not(unix))]
fn create_private_dir(directory: &Path) -> std::io::Result<()> {
    // `%LOCALAPPDATA%` is the user's alone.
    std::fs::create_dir_all(directory)
}

/// Write `text` to `path` for the user alone, in place at once.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("new-{}", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(text.as_bytes())?;
    drop(file);
    std::fs::rename(&temporary, path)
}

/// Add a line to the file at `path`, for the user alone.
fn append_private(path: &Path, line: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(format!("{line}\n").as_bytes())
}

// ----- the subcommand's side

/// Why a URL did not reach a server.
enum Failure {
    /// None listens there: it stopped.
    Gone,
    /// It answered that it could not.
    Refused(String),
}

/// The servers that say they listen, the one touched last first.
fn servers(directory: &Path) -> Vec<(PathBuf, Value)> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut found: Vec<(SystemTime, PathBuf, Value)> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.ends_with(".json") && !name.starts_with("pending-")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            let value = serde_json::from_str(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
            Some((modified, entry.path(), value))
        })
        .collect();
    found.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, path, value)| (path, value)).collect()
}

/// Send `url` to the server of `entry`, and read what it says.
fn deliver(entry: &Value, url: &str) -> Result<Answer, Failure> {
    let mut sent = json!({ "url": url });
    #[cfg(unix)]
    let stream = {
        let socket = entry["socket"].as_str().ok_or(Failure::Gone)?;
        let stream = std::os::unix::net::UnixStream::connect(socket).map_err(|_| Failure::Gone)?;
        stream.set_read_timeout(Some(Duration::from_secs(40))).map_err(|error| Failure::Refused(error.to_string()))?;
        stream
    };
    #[cfg(not(unix))]
    let stream = {
        let address: std::net::SocketAddr =
            entry["address"].as_str().and_then(|address| address.parse().ok()).ok_or(Failure::Gone)?;
        sent["token"] = entry["token"].clone();
        let stream =
            std::net::TcpStream::connect_timeout(&address, Duration::from_millis(500)).map_err(|_| Failure::Gone)?;
        stream.set_read_timeout(Some(Duration::from_secs(40))).map_err(|error| Failure::Refused(error.to_string()))?;
        stream
    };
    #[cfg(unix)]
    let _ = &mut sent;
    let mut writer = &stream;
    writer.write_all(format!("{sent}\n").as_bytes()).map_err(|_| Failure::Gone)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|error| Failure::Refused(error.to_string()))?;
    let answer: Value = serde_json::from_str(&line).map_err(|_| Failure::Gone)?;
    Ok(match (answer["message"].as_str(), answer["error"].as_str()) {
        (Some(message), _) => Ok(message.to_string()),
        (_, Some(error)) => Err(error.to_string()),
        _ => Err("org-lsp answered nothing".to_string()),
    })
}

/// Take away the entry of a server that stopped, and its socket.
fn remove_stale(path: &Path, entry: &Value) {
    let _ = std::fs::remove_file(path);
    if let Some(socket) = entry["socket"].as_str() {
        let _ = std::fs::remove_file(socket);
    }
}

/// `org-lsp protocol URL`: what to print, or why it failed.
pub fn run(arguments: &[String], var: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let url = match arguments {
        [url] => url,
        [dash, url] if dash == "--" => url,
        _ => return Err(format!("expected one URL\n\n{USAGE}")),
    };
    check(url)?;
    let loaded = crate::config::load(None, var);
    let options = &loaded.options;
    let protocol = options.protocol_options();
    // What Org would make of it, before any server is asked: a URL Org
    // would refuse is refused here.
    let request = match org_core::protocol::parse(url, &protocol) {
        Ok(Some(request)) => request,
        Ok(None) => return Err(format!("no org-protocol handler for {url}")),
        Err(refused) => return Err(refused.error),
    };
    let directory = directory(var).ok_or("no home directory")?;
    for (path, entry) in servers(&directory) {
        match deliver(&entry, url) {
            Ok(answer) => return answer,
            Err(Failure::Refused(error)) => return Err(error),
            Err(Failure::Gone) => remove_stale(&path, &entry),
        }
    }
    // No server runs.
    let editor = options.protocol_editor.as_deref().map(str::trim).filter(|editor| !editor.is_empty());
    let home = |path: &str| crate::config::expand_home(path, var);
    match request {
        org_core::protocol::Request::StoreLink { url, title } => {
            create_private_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
            let path = directory.join(STORED_LINKS);
            append_private(&path, &json!([url, title]).to_string())
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if let Some(editor) = editor {
                start(editor, &home(&options.default_notes_file()))?;
            }
            Ok(org_core::protocol::store_link_message(&url))
        }
        org_core::protocol::Request::Capture(_) => {
            let Some(editor) = editor else {
                return Err(
                    "No org-lsp is running, and `org_protocol_editor` names no editor to start for the capture"
                        .to_string(),
                );
            };
            create_private_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
            let now = SystemTime::now();
            let nanos = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = directory.join(format!("pending-{nanos}-{}.json", std::process::id()));
            write_private(&path, &json!({ "url": url, "time": unix_time(now) }).to_string())
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if let Err(error) = start(editor, &home(&options.default_notes_file())) {
                let _ = std::fs::remove_file(&path);
                return Err(error);
            }
            Ok(String::new())
        }
        org_core::protocol::Request::OpenSource { url } => {
            let opened = org_core::protocol::open_source(&url, &protocol.projects, &|name| file_state(&home(name)));
            if let Some(error) = opened.error {
                return Err(error);
            }
            let Some(file) = source_file(&opened, &home)? else {
                return Err(opened
                    .messages
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("No project of `org_protocol_project_alist` maps {url}")));
            };
            match editor {
                Some(editor) => start(editor, &file).map(|()| String::new()),
                None => Err(format!(
                    "No org-lsp is running, and `org_protocol_editor` names no editor to open {}",
                    file.display()
                )),
            }
        }
    }
}

/// The file `open-source` shows: the one `opened` names, `~` expanded
/// by `home`, once it is found to be in the working directory of the
/// project that maps the URL to it, both with their links and `..`
/// resolved. Org shows any file the URL leads to, and `%2e%2e` in a URL
/// goes up out of the project's directory; org-lsp shows none outside,
/// and none for a working directory that is not absolute (empty, or
/// relative to wherever the server runs).
pub fn source_file(
    opened: &org_core::protocol::Opened,
    home: &dyn Fn(&str) -> PathBuf,
) -> Result<Option<PathBuf>, String> {
    let Some(file) = &opened.file else { return Ok(None) };
    let path = resolved(&home(file));
    let directory = opened
        .working_directory
        .as_deref()
        .map(home)
        .filter(|directory| directory.is_absolute())
        .map(|directory| resolved(&directory));
    match directory {
        Some(directory) if path.starts_with(&directory) => Ok(Some(path)),
        _ => Err(format!(
            "{} is not in the working directory of its project ({}), and is not opened",
            path.display(),
            opened.working_directory.as_deref().unwrap_or_default()
        )),
    }
}

/// `path` made absolute, `.` and `..` resolved, and its links: those of
/// the part of it that is there.
fn resolved(path: &Path) -> PathBuf {
    use std::path::Component;
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other),
        }
    }
    let mut existing = lexical.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(mut real) = std::fs::canonicalize(existing) {
            real.extend(rest.into_iter().rev());
            return real;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return lexical,
        }
    }
}

/// Whether the file named `path` is there and readable, as
/// `file-exists-p` and `file-readable-p` tell.
pub fn file_state(path: &Path) -> org_core::protocol::FileState {
    use org_core::protocol::FileState;
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() || std::fs::File::open(path).is_ok() => FileState::Readable,
        Ok(_) => FileState::Unreadable,
        Err(_) => FileState::Missing,
    }
}

/// Start the editor of the command line `editor` on `file`, on its own:
/// it goes on when the subcommand ends.
fn start(editor: &str, file: &Path) -> Result<(), String> {
    let words = split_command_line(editor)?;
    let (program, arguments) = words.split_first().ok_or("`org_protocol_editor` is empty")?;
    let mut command = std::process::Command::new(program);
    command
        .args(arguments)
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // A group of its own, out of reach of the terminal's hangup.
        command.process_group(0);
    }
    command.spawn().map(drop).map_err(|error| format!("cannot start {program}: {error}"))
}

/// A command line as a shell splits it into words: blanks between them,
/// quotes around blanks, a backslash before a character for itself
/// (outside single quotes). Nothing else of a shell's.
pub fn split_command_line(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' => words.extend(word.take()),
            '\'' => {
                let word = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err(format!("unbalanced quote in `{line}`")),
                    }
                }
            }
            '"' => {
                let word = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => word.push(c),
                            Some(c) => {
                                word.push('\\');
                                word.push(c);
                            }
                            None => return Err(format!("unbalanced quote in `{line}`")),
                        },
                        Some(c) => word.push(c),
                        None => return Err(format!("unbalanced quote in `{line}`")),
                    }
                }
            }
            '\\' if cfg!(unix) => {
                if let Some(c) = chars.next() {
                    word.get_or_insert_with(String::new).push(c);
                }
            }
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    words.extend(word);
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_a_source_only_in_an_absolute_working_directory() {
        let root = std::env::temp_dir().join(format!("org-lsp-source-{}", std::process::id()));
        let file = root.join("worg/index.org");
        let home = |path: &str| PathBuf::from(path.replace('~', &root.to_string_lossy()));
        let opened = |directory: &str| org_core::protocol::Opened {
            file: Some(file.to_string_lossy().into_owned()),
            working_directory: Some(directory.to_string()),
            ..Default::default()
        };
        assert_eq!(source_file(&opened("~/worg/"), &home), Ok(Some(file.clone())));
        // An empty or relative directory would be that of the server.
        for directory in ["", ".", "worg"] {
            assert_eq!(
                source_file(&opened(directory), &home),
                Err(format!(
                    "{} is not in the working directory of its project ({directory}), and is not opened",
                    file.display()
                )),
                "{directory:?}"
            );
        }
    }

    #[test]
    fn splits_a_command_line_into_words() {
        let words = |line: &str| split_command_line(line).unwrap();
        assert_eq!(words("code --wait"), ["code", "--wait"]);
        assert_eq!(words("  kitty  -e nvim "), ["kitty", "-e", "nvim"]);
        assert_eq!(words(r#"'/My Apps/zed' "a b" c\ d"#), ["/My Apps/zed", "a b", "c d"]);
        assert_eq!(words(r#"x "" ''"#), ["x", "", ""]);
        assert!(split_command_line("'open").is_err());
    }

    /// A state directory whose socket's name would be too long: the socket
    /// is in `/tmp/org-lsp-UID`, which the entry names, and takes URLs.
    #[cfg(unix)]
    #[test]
    fn listens_in_tmp_when_the_state_directory_is_too_deep() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = std::env::temp_dir().join(format!("org-lsp-long-state-{}", std::process::id()));
        let directory = root.join("d".repeat(120)).join("protocol");
        let (sender, receiver) = crossbeam_channel::unbounded();
        let listener = Listener::open(&directory, sender).unwrap();
        let entry: Value = serde_json::from_str(&std::fs::read_to_string(&listener.entry).unwrap()).unwrap();
        let socket = PathBuf::from(entry["socket"].as_str().unwrap());
        let uid = std::fs::metadata(&directory).unwrap().uid();
        assert_eq!(socket.parent(), Some(PathBuf::from(format!("/tmp/org-lsp-{uid}")).as_path()));
        assert_eq!(std::fs::metadata(socket.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        let answering = std::thread::spawn(move || {
            let sent = receiver.recv().unwrap();
            let _ = sent.answer.send(Ok(format!("got {}", sent.url)));
        });
        let answer = deliver(&entry, "org-protocol://store-link?url=x").ok().unwrap();
        assert_eq!(answer, Ok("got org-protocol://store-link?url=x".to_string()));
        answering.join().unwrap();
        listener.close();
        assert!(!socket.exists());
        let _ = std::fs::remove_dir_all(&root);
        // A directory there that is not the user's alone is not used.
        let open = root.join("open");
        std::fs::create_dir_all(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(own_private_directory(&open, uid).is_err());
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(own_private_directory(&open, uid).is_ok());
        assert!(own_private_directory(&open, uid + 1).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn takes_only_org_protocol_urls() {
        assert!(check("org-protocol://capture?url=x").is_ok());
        assert!(check("ORG-PROTOCOL://store-link?url=x").is_ok());
        assert!(check("https://example.com").is_err());
        assert!(check("org-protocol://x\0y").is_err());
        assert!(check("org-").is_err());
    }

    #[test]
    fn finds_the_state_directory() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
        };
        assert_eq!(
            state_directory(&env(&[("XDG_STATE_HOME", "/s"), ("HOME", "/h")])),
            Some(PathBuf::from("/s/org-lsp"))
        );
        if cfg!(unix) {
            assert_eq!(state_directory(&env(&[("HOME", "/h")])), Some(PathBuf::from("/h/.local/state/org-lsp")));
        }
        assert_eq!(state_directory(&env(&[])), None);
    }
}
