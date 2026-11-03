//! The command port: how a key reaches the server in an editor whose keys
//! cannot run a language server's command, but can run a program (Zed's
//! tasks). The server listens from the start (on Unix on a socket beside
//! the script, in a directory only the user may enter; elsewhere on a
//! local TCP port), and writes a script in a place of the user's own that
//! sends a command there:
//!
//! ```text
//! ~/.local/state/org-lsp/agenda-command COMMAND [ROW COLUMN [FILE]]
//! ```
//!
//! runs `COMMAND` (`org.agenda.later`) as `workspace/executeCommand` would:
//! in `FILE` at the row and column given (from 1), an Org file, a capture
//! buffer or the agenda; without a file, in the agenda, where the cursor
//! is when no row is given (the server follows it); with no agenda open,
//! the commands that need none (`org.agenda.list`, `org.capture`).
//! `key:KEY` in place of a command (`key:C-c C-c`) runs what Org's key
//! does there: in a capture buffer, `C-c C-c` files the entry. The place
//! is fixed, so that the editor's keys can name it: the server whose
//! agenda the cursor was last in makes it its own. The script holds a
//! token the server checks, so that no other user's program can run
//! commands (where a TCP port is open to them all); the script is the
//! owner's alone.
//!
//! The script stays when the server stops. Run then, it finds no server,
//! opens the Org file last opened, in its folder, with the editor's
//! command line (`zed`), which starts a server for it, waits for that
//! server to take the script over, and sends the command there. A server
//! that starts takes the script over only when no other answers there.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

/// A command sent to the port: what to run, and where in the agenda, when
/// the sender knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub position: Option<(u32, u32)>,
    pub file: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Sent {
    token: String,
    command: String,
    line: Option<u32>,
    character: Option<u32>,
    file: Option<PathBuf>,
}

/// Where a port listens: on Unix a socket in a directory only the user
/// may enter, which no other user can connect to; elsewhere a local TCP
/// port, which the token keeps to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Address {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Socket(PathBuf),
}

impl Address {
    /// The address as `org-lsp command` takes it: a socket's path, or
    /// `HOST:PORT`.
    fn parse(text: &str) -> Option<Address> {
        #[cfg(unix)]
        if text.contains('/') {
            return Some(Address::Socket(PathBuf::from(text)));
        }
        text.parse().ok().map(Address::Tcp)
    }

    fn text(&self) -> String {
        match self {
            Address::Tcp(address) => address.to_string(),
            #[cfg(unix)]
            Address::Socket(path) => path.display().to_string(),
        }
    }

    /// A connection to the port, or why there is none (no server listens
    /// there any more).
    fn connect(&self, timeout: Duration) -> std::io::Result<Box<dyn Stream>> {
        match self {
            Address::Tcp(address) => Ok(Box::new(TcpStream::connect_timeout(address, timeout)?)),
            #[cfg(unix)]
            Address::Socket(path) => Ok(Box::new(std::os::unix::net::UnixStream::connect(path)?)),
        }
    }
}

/// A connection to or from the port.
trait Stream: Read + Write + Send {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()>;
}

impl Stream for TcpStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        TcpStream::set_read_timeout(self, timeout)
    }
}

#[cfg(unix)]
impl Stream for std::os::unix::net::UnixStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        std::os::unix::net::UnixStream::set_read_timeout(self, timeout)
    }
}

/// An open port: where it listens, the token it asks for, and whether it
/// is to stop listening.
#[derive(Debug, Clone)]
pub struct Port {
    address: Address,
    token: String,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// How the script starts a server when none answers: the Org file to
/// open, the folder to open it in, and the editor's command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Start {
    pub file: Option<PathBuf>,
    pub folder: Option<PathBuf>,
    pub editor: Option<PathBuf>,
}

/// The script, in the home directory.
pub fn script_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let name = if cfg!(unix) { "agenda-command" } else { "agenda-command.cmd" };
    Some(PathBuf::from(home).join(".local").join("state").join("org-lsp").join(name))
}

/// Listen for commands, which go to `sender`: on Unix on a socket in
/// `directory` (the script's), made only the user's; elsewhere on a local
/// port.
pub fn open(directory: &Path, sender: crossbeam_channel::Sender<Command>) -> std::io::Result<Port> {
    let token = token();
    let closed: std::sync::Arc<std::sync::atomic::AtomicBool> = Default::default();
    let done = closed.clone();
    #[cfg(unix)]
    {
        let (socket, listener) = bind_socket(directory, &token)?;
        let port = Port { address: Address::Socket(socket), token: token.clone(), closed };
        std::thread::Builder::new().name("org-lsp-port".to_string()).spawn(move || {
            for stream in listener.incoming().flatten() {
                if done.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if let Some(command) = receive(Box::new(stream), &token) {
                    if sender.send(command).is_err() {
                        return;
                    }
                }
            }
        })?;
        Ok(port)
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let port = Port { address: Address::Tcp(listener.local_addr()?), token: token.clone(), closed };
        std::thread::Builder::new().name("org-lsp-port".to_string()).spawn(move || {
            for stream in listener.incoming().flatten() {
                if done.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if let Some(command) = receive(Box::new(stream), &token) {
                    if sender.send(command).is_err() {
                        return;
                    }
                }
            }
        })?;
        Ok(port)
    }
}

/// A socket of this server's in `directory`, made only the user's (a
/// private directory of `/tmp` when its name would be too long there, as
/// for org-protocol's), the sockets left by servers that stopped without
/// taking theirs away gone.
#[cfg(unix)]
fn bind_socket(directory: &Path, token: &str) -> std::io::Result<(PathBuf, std::os::unix::net::UnixListener)> {
    use std::os::unix::fs::PermissionsExt;
    create_private_dir(directory)?;
    let name = format!("port-{}-{}.sock", std::process::id(), &token[..8]);
    let bind = |directory: &Path| {
        remove_stale_sockets(directory);
        let socket = directory.join(&name);
        let _ = std::fs::remove_file(&socket);
        std::os::unix::net::UnixListener::bind(&socket).map(|listener| (socket, listener))
    };
    let (socket, listener) = match bind(directory) {
        Ok(bound) => bound,
        Err(error) if error.kind() != std::io::ErrorKind::InvalidInput => return Err(error),
        Err(error) => crate::protocol::short_socket_directory()
            .and_then(|short| bind(&short))
            .map_err(|short| std::io::Error::new(error.kind(), format!("{error}; in /tmp: {short}")))?,
    };
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    Ok((socket, listener))
}

/// Take away the sockets of ports in `directory` that no server listens on.
#[cfg(unix)]
fn remove_stale_sockets(directory: &Path) {
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with("port-")
            && name.ends_with(".sock")
            && std::os::unix::net::UnixStream::connect(&path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused)
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

impl Port {
    /// Make the script at `path` send to this port, and start a server as
    /// `start` says when none answers, unless it does.
    pub fn claim(&self, path: &Path, start: &Start) -> std::io::Result<()> {
        let text = script(&self.address, &self.token, start)?;
        if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
            return Ok(());
        }
        if let Some(directory) = path.parent() {
            create_private_dir(directory)?;
        }
        write_script(path, &text)
    }

    /// Stop listening: the server stops.
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        // The listener waits for a connection: this one ends the wait. Then
        // the port answers no more, for a server that starts next.
        let timeout = Duration::from_millis(300);
        for _ in 0..100 {
            if self.address.connect(timeout).is_err() {
                break;
            }
            #[cfg(unix)]
            if let Address::Socket(path) = &self.address {
                let _ = std::fs::remove_file(path);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Whether the script at `path` sends to this port.
    pub fn owns(&self, path: &Path) -> bool {
        sent_to(path).is_some_and(|(address, _)| address == self.address.text())
    }

    /// Whether the script at `path` sends to another server that answers.
    pub fn answered_elsewhere(&self, path: &Path) -> bool {
        sent_to(path).is_some_and(|(address, _)| {
            address != self.address.text()
                && Address::parse(&address).is_some_and(|address| address.connect(Duration::from_millis(300)).is_ok())
        })
    }
}

/// The address and the token the script at `path` sends to.
fn sent_to(path: &Path) -> Option<(String, String)> {
    address_and_token(&std::fs::read_to_string(path).ok()?)
}

/// The editor's command line, which opens a file in it: beside the
/// program that started the server (Zed's `bin/zed` beside
/// `libexec/zed-editor`, `cli` in the app on macOS), or else on `PATH`.
pub fn editor_command() -> Option<PathBuf> {
    let beside = parent_executable().and_then(|program| {
        let directory = program.parent()?;
        [directory.join("..").join("bin").join("zed"), directory.join("cli"), directory.join("bin").join("zed.exe")]
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| path.canonicalize().unwrap_or(path))
    });
    beside.or_else(|| ["zed", "zeditor", "zedit"].into_iter().find_map(on_path))
}

/// The program of the process that started this one.
fn parent_executable() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{}/exe", std::os::unix::process::parent_id())).ok()
    }
    #[cfg(target_os = "macos")]
    {
        let parent = std::os::unix::process::parent_id().to_string();
        let output = std::process::Command::new("ps").args(["-o", "comm=", "-p", &parent]).output().ok()?;
        let path = String::from_utf8(output.stdout).ok()?;
        Some(PathBuf::from(path.trim())).filter(|path| path.is_absolute())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// `name` in a directory of `PATH`.
pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ".cmd"] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|directory| {
        suffixes.iter().map(|suffix| directory.join(format!("{name}{suffix}"))).find(|path| path.is_file())
    })
}

/// Read one command, and answer whether it is taken.
fn receive(mut stream: Box<dyn Stream>, token: &str) -> Option<Command> {
    // The port takes one command at a time: one that never comes, or
    // comes a byte at a time, must not keep the others out. The whole
    // line has two seconds.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut line = Vec::new();
    let mut chunk = [0u8; 512];
    while !line.contains(&b'\n') && line.len() < 4096 {
        let left = deadline.checked_duration_since(std::time::Instant::now()).filter(|left| !left.is_zero())?;
        stream.set_read_timeout(Some(left)).ok()?;
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        line.extend_from_slice(&chunk[..read]);
    }
    let line = line.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let sent: Option<Sent> = serde_json::from_slice(line).ok();
    let command = sent.filter(|sent| same(&sent.token, token)).map(|sent| Command {
        command: sent.command,
        position: sent.line.zip(sent.character),
        file: sent.file,
    });
    let _ = stream.write_all(if command.is_some() { b"ok\n" } else { b"refused\n" });
    command
}

/// A token no other process can guess, 32 hexadecimal digits: 16 bytes
/// of the system's randomness (`/dev/urandom`). Where there is none to
/// read (Windows), the hash keys of the standard library, which it takes
/// from the system's randomness once per thread, through SipHash.
pub(crate) fn token() -> String {
    #[cfg(unix)]
    {
        let mut bytes = [0u8; 16];
        if std::fs::File::open("/dev/urandom").and_then(|mut random| random.read_exact(&mut bytes)).is_ok() {
            return bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        }
    }
    use std::hash::{BuildHasher, Hasher};
    let part = |salt: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.write_u128(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        );
        hasher.finish()
    };
    format!("{:016x}{:016x}", part(std::process::id().into()), part(1))
}

/// Whether `given` is `token`, in a time that does not tell how much of it
/// was right.
fn same(given: &str, token: &str) -> bool {
    given.len() == token.len() && given.bytes().zip(token.bytes()).fold(0, |differ, (a, b)| differ | (a ^ b)) == 0
}

/// The options of `org-lsp command` that say how to start a server.
fn start_options(start: &Start, quote: fn(&Path) -> String) -> String {
    let mut options = String::new();
    if let (Some(file), Some(editor)) = (&start.file, &start.editor) {
        options = format!("--open {} --editor {} ", quote(file), quote(editor));
        if let Some(folder) = &start.folder {
            options = format!("--folder {} {options}", quote(folder));
        }
    }
    options
}

#[cfg(unix)]
fn script(address: &Address, token: &str, start: &Start) -> std::io::Result<String> {
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', r"'\''"));
    let exe = quote(&std::env::current_exe()?);
    let options = start_options(start, quote);
    let address = quote(Path::new(&address.text()));
    // The token goes in the environment, which only the user can read,
    // not on the command line, which every user can.
    Ok(format!(
        "#!/bin/sh\n# Runs a command of an org-lsp: COMMAND [ROW COLUMN [FILE]].\n\
         {TOKEN_VARIABLE}={token} exec {exe} command --script \"$0\" {options}{address} - \"$@\"\n"
    ))
}

#[cfg(not(unix))]
fn script(address: &Address, token: &str, start: &Start) -> std::io::Result<String> {
    let quote = |path: &Path| format!("\"{}\"", path.display());
    let exe = quote(&std::env::current_exe()?);
    let options = start_options(start, quote);
    let address = address.text();
    Ok(format!(
        "@rem Runs a command of an org-lsp: COMMAND [ROW COLUMN [FILE]].\r\n\
         @set {TOKEN_VARIABLE}={token}\r\n\
         @{exe} command --script \"%~f0\" {options}{address} - %*\r\n"
    ))
}

/// Where the script gives `org-lsp command` the token: `-` in its place
/// on the command line reads it from there.
const TOKEN_VARIABLE: &str = "ORG_LSP_TOKEN";

/// The address and the token of the server the script `text` sends to:
/// the last two words before the command's own arguments, the token in
/// `TOKEN_VARIABLE` when the second is `-`.
fn address_and_token(text: &str) -> Option<(String, String)> {
    let words = shell_words(text.lines().last()?);
    let [.., address, token, _] = words.as_slice() else { return None };
    let token = match token.as_str() {
        "-" => text.split_whitespace().find_map(|word| word.strip_prefix(TOKEN_VARIABLE)?.strip_prefix('='))?,
        token => token,
    };
    Some((address.clone(), token.to_string()))
}

/// The words of a line of the script, as the shell splits them: quotes
/// (`'…'`, `"…"`) and a backslash keep spaces in a word, and go.
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => words.extend(word.take()),
            '\'' => word.get_or_insert_default().extend(chars.by_ref().take_while(|c| *c != '\'')),
            '"' if cfg!(unix) => word.get_or_insert_default().extend(chars.by_ref().take_while(|c| *c != '"')),
            '\\' if cfg!(unix) => word.get_or_insert_default().extend(chars.next()),
            c => word.get_or_insert_default().push(c),
        }
    }
    words.extend(word);
    words
}

#[cfg(unix)]
fn create_private_dir(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(directory)?;
    // Only the owner may read the token.
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(directory: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)
}

#[cfg(unix)]
fn write_script(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    // A new file, renamed into place, so that a key never runs half a script.
    let temporary = path.with_extension(format!("{}.new", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(&temporary)?;
    file.write_all(text.as_bytes())?;
    drop(file);
    std::fs::rename(&temporary, path)
}

#[cfg(not(unix))]
fn write_script(path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)
}

/// Why a command did not reach a server.
enum Failure {
    /// No server listens there (it stopped).
    Unreachable(String),
    /// The server would not take it.
    Refused(String),
}

/// `org-lsp command [--script PATH] [--folder DIR] [--open FILE --editor
/// EDITOR] ADDRESS TOKEN COMMAND [ROW COLUMN [FILE]]`: send a command to the port
/// of a running server; with none there, start one (see the module).
pub fn send(arguments: &[String]) -> Result<(), String> {
    let mut rest = arguments;
    let (mut script, mut start) = (None, Start::default());
    while let [flag, value, tail @ ..] = rest {
        let slot = match flag.as_str() {
            "--script" => &mut script,
            "--folder" => &mut start.folder,
            "--open" => &mut start.file,
            "--editor" => &mut start.editor,
            _ => break,
        };
        *slot = Some(PathBuf::from(value));
        rest = tail;
    }
    let (address, token, command) = match rest {
        [address, token, command] | [address, token, command, _, _] | [address, token, command, _, _, _] => {
            (address, token, command)
        }
        _ => return Err("expected ADDRESS TOKEN COMMAND [ROW COLUMN [FILE]]".to_string()),
    };
    let number =
        |text: &str| text.parse::<u32>().map(|n| n.saturating_sub(1)).map_err(|_| format!("not a number: {text}"));
    let token = match token.as_str() {
        "-" => std::env::var(TOKEN_VARIABLE).map_err(|_| format!("{TOKEN_VARIABLE} is not set"))?,
        token => token.to_string(),
    };
    let mut sent = json!({ "token": token, "command": command });
    if let [_, _, _, row, column, ..] = rest {
        sent["line"] = json!(number(row)?);
        sent["character"] = json!(number(column)?);
    }
    if let [_, _, _, _, _, file] = rest {
        // An absolute path: the server runs elsewhere.
        let file = std::path::absolute(file).map_err(|error| format!("{file}: {error}"))?;
        sent["file"] = json!(file);
    }
    match deliver(address, &sent) {
        Ok(()) => Ok(()),
        Err(Failure::Refused(error)) => Err(error),
        Err(Failure::Unreachable(error)) => match script {
            Some(script) if start.file.is_some() && start.editor.is_some() => start_and_send(&script, &start, sent),
            _ => Err(error),
        },
    }
}

/// Send `sent` to the server at `address`.
fn deliver(address: &str, sent: &serde_json::Value) -> Result<(), Failure> {
    let unreachable = |error: std::io::Error| Failure::Unreachable(format!("{address}: {error}"));
    let to = Address::parse(address).ok_or_else(|| Failure::Refused(format!("not an address: {address}")))?;
    let mut stream = to.connect(Duration::from_secs(5)).map_err(unreachable)?;
    stream.write_all(format!("{sent}\n").as_bytes()).map_err(|error| Failure::Refused(error.to_string()))?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).map_err(|error| Failure::Refused(error.to_string()))?;
    match answer.trim() {
        "ok" => Ok(()),
        other => Err(Failure::Refused(format!("the server answered `{other}`"))),
    }
}

/// No server answers: open the file of `start` in its folder with the
/// editor, which starts one for it, wait for that server to take `script`
/// over, and send it `sent`.
fn start_and_send(script: &Path, start: &Start, mut sent: serde_json::Value) -> Result<(), String> {
    let (Some(file), Some(editor)) = (&start.file, &start.editor) else { return Err("nothing to start".to_string()) };
    let before = std::fs::read_to_string(script).unwrap_or_default();
    // The folder too: Zed then opens the file in it, as a project, whose
    // server the agenda shares.
    std::process::Command::new(editor)
        .args(start.folder.iter())
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("no org-lsp is running, and {} could not start: {error}", editor.display()))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let text = std::fs::read_to_string(script).unwrap_or_default();
        let Some((address, token)) = (text != before).then(|| address_and_token(&text)).flatten() else { continue };
        sent["token"] = json!(token);
        return deliver(&address, &sent).map_err(|(Failure::Unreachable(error) | Failure::Refused(error))| error);
    }
    Err(format!("opened {} to start org-lsp, but none started within 30 seconds", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_a_command_with_the_token_and_refuses_one_without() {
        let directory = std::env::temp_dir().join(format!("org-lsp-port-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let script = directory.join("state").join("agenda-command");
        let (sender, receiver) = crossbeam_channel::unbounded();
        let port = open(script.parent().unwrap(), sender).unwrap();
        port.claim(&script, &Start::default()).unwrap();
        let text = std::fs::read_to_string(&script).unwrap();
        let (address, token) = address_and_token(&text).unwrap();
        let arguments = |token: &str, rest: &[&str]| {
            [address.clone(), token.to_string(), "org.agenda.later".into()]
                .into_iter()
                .chain(rest.iter().map(|part| part.to_string()))
                .collect::<Vec<_>>()
        };
        send(&arguments(&token, &["3", "1"])).unwrap();
        assert_eq!(
            receiver.recv().unwrap(),
            Command { command: "org.agenda.later".to_string(), position: Some((2, 0)), file: None }
        );
        send(&arguments(&token, &[])).unwrap();
        assert_eq!(
            receiver.recv().unwrap(),
            Command { command: "org.agenda.later".to_string(), position: None, file: None }
        );
        // In a file: the row, the column and the file, its path absolute.
        let work = directory.join("work.org");
        send(&arguments(&token, &["2", "5", &work.display().to_string()])).unwrap();
        assert_eq!(
            receiver.recv().unwrap(),
            Command { command: "org.agenda.later".to_string(), position: Some((1, 4)), file: Some(work) }
        );
        assert!(send(&arguments("guessed", &[])).is_err());
        // `-` for the token: the one in the environment, as the script runs it.
        std::env::set_var(TOKEN_VARIABLE, &token);
        send(&arguments("-", &[])).unwrap();
        std::env::remove_var(TOKEN_VARIABLE);
        assert_eq!(receiver.recv().unwrap().command, "org.agenda.later");
        assert!(receiver.try_recv().is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(std::fs::metadata(script.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
            // The script runs `org-lsp command` with its own path and the
            // port, the token in its environment, off the command line.
            assert!(text.contains(&format!(" command --script \"$0\" '{address}' - \"$@\"")), "{text}");
            assert!(text.contains(&format!("{TOKEN_VARIABLE}={token} exec ")), "{text}");
            // The port is a socket beside the script, only the user's.
            let socket = PathBuf::from(&address);
            assert_eq!(socket.parent(), script.parent(), "{address}");
            assert_eq!(std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777, 0o600);
            port.close();
            assert!(!socket.exists());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// With the server gone, the script opens the Org file with the
    /// editor's command line and sends to the server that then takes the
    /// script over.
    #[cfg(unix)]
    #[test]
    fn starts_a_server_when_none_answers() {
        let directory = std::env::temp_dir().join(format!("org-lsp-port-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let script = directory.join("state").join("agenda-command");
        let file = directory.join("notes.org");
        // The "editor" says it was asked to open the file.
        let editor = directory.join("editor");
        let opened = directory.join("opened");
        std::fs::write(&editor, format!("#!/bin/sh\necho \"$@\" > '{}'\n", opened.display())).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let start = Start { file: Some(file.clone()), folder: Some(directory.clone()), editor: Some(editor.clone()) };
        // A server that stopped: its socket is there, but no one listens.
        let state = script.parent().unwrap();
        create_private_dir(state).unwrap();
        let gone = {
            let socket = state.join("port-1-gone.sock");
            drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
            Port { address: Address::Socket(socket), token: token(), closed: Default::default() }
        };
        gone.claim(&script, &start).unwrap();
        // The new server takes the script over a moment after the editor
        // opened the file.
        let (sender, receiver) = crossbeam_channel::unbounded();
        let new = open(state, sender).unwrap();
        // It took the stopped server's socket away.
        assert!(!state.join("port-1-gone.sock").exists());
        // The script's server does not answer: a new one may take it over.
        assert!(!new.answered_elsewhere(&script));
        let (watched, claimed) = (opened.clone(), script.clone());
        let taker = std::thread::spawn(move || {
            while !watched.exists() {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            new.claim(&claimed, &Start::default()).unwrap();
            new
        });
        // What the script runs.
        let text = std::fs::read_to_string(&script).unwrap();
        assert!(
            text.contains(&format!(
                "--folder '{}' --open '{}' --editor '{}' ",
                directory.display(),
                file.display(),
                editor.display()
            )),
            "{text}"
        );
        let (address, token) = address_and_token(&text).unwrap();
        let path = |path: &Path| path.display().to_string();
        let arguments = ["--script".to_string(), path(&script), "--folder".into(), path(&directory), "--open".into()]
            .into_iter()
            .chain([path(&file), "--editor".into(), path(&editor), address, token, "org.agenda.list".into()])
            .collect::<Vec<_>>();
        send(&arguments).unwrap();
        assert_eq!(
            std::fs::read_to_string(&opened).unwrap().trim(),
            format!("{} {}", directory.display(), file.display())
        );
        assert_eq!(receiver.recv().unwrap().command, "org.agenda.list");
        // Now it answers: another server leaves the script alone.
        let new = taker.join().unwrap();
        assert!(new.owns(&script));
        let (third_sender, _third_receiver) = crossbeam_channel::unbounded();
        let third = open(state, third_sender).unwrap();
        assert!(third.answered_elsewhere(&script) && !third.owns(&script));
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// The token is the system's randomness, and is compared whole.
    #[test]
    fn tokens_are_random_and_compared_whole() {
        let (one, two) = (token(), token());
        assert_eq!(one.len(), 32);
        assert!(one.chars().all(|c| c.is_ascii_hexdigit()), "{one}");
        assert_ne!(one, two);
        assert!(same(&one, &one.clone()));
        assert!(!same(&one, &two));
        assert!(!same(&one[..31], &one));
        assert!(!same("", &one));
    }

    /// A socket in a directory whose name has a space and a quote: the
    /// script names it so that the shell, and `org-lsp command`, read it
    /// whole.
    #[cfg(unix)]
    #[test]
    fn names_a_socket_whose_path_has_spaces() {
        let directory = std::env::temp_dir().join(format!("org-lsp-port 'quoted' {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let script = directory.join("agenda-command");
        let (sender, receiver) = crossbeam_channel::unbounded();
        let port = open(&directory, sender).unwrap();
        port.claim(&script, &Start::default()).unwrap();
        assert!(port.owns(&script));
        let (address, token) = address_and_token(&std::fs::read_to_string(&script).unwrap()).unwrap();
        assert!(address.starts_with(&directory.display().to_string()), "{address}");
        send(&[address, token, "org.agenda.list".to_string()]).unwrap();
        assert_eq!(receiver.recv().unwrap().command, "org.agenda.list");
        port.close();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
