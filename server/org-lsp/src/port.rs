//! The command port: how a key reaches the server in an editor whose keys
//! cannot run a language server's command, but can run a program (Zed's
//! tasks). The server listens on a local port from the start, and writes
//! a script in a place of the user's own that sends a command there:
//!
//! ```text
//! ~/.local/state/org-lsp/agenda-command COMMAND [ROW COLUMN]
//! ```
//!
//! runs `COMMAND` (`org.agenda.later`) in the agenda, as
//! `workspace/executeCommand` would: at the row and column given (from 1),
//! or else where the server last saw the cursor; with no agenda open, the
//! commands that need none (`org.agenda.list`, `org.capture`). The place
//! is fixed, so that the editor's keys can name it: the server whose
//! agenda the cursor was last in makes it its own. The script holds a
//! token the server checks, so that no other user's program can run
//! commands; the script is the owner's alone.
//!
//! The script stays when the server stops. Run then, it finds no server,
//! opens the Org file last opened, in its folder, with the editor's
//! command line (`zed`), which starts a server for it, waits for that
//! server to take the script over, and sends the command there. A server
//! that starts takes the script over only when no other answers there.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;

/// A command sent to the port: what to run, and where in the agenda, when
/// the sender knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub position: Option<(u32, u32)>,
}

#[derive(Deserialize)]
struct Sent {
    token: String,
    command: String,
    line: Option<u32>,
    character: Option<u32>,
}

/// An open port: where it listens, the token it asks for, and whether it
/// is to stop listening.
#[derive(Debug, Clone)]
pub struct Port {
    address: SocketAddr,
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

/// Listen on a local port for commands, which go to `sender`.
pub fn open(sender: crossbeam_channel::Sender<Command>) -> std::io::Result<Port> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = Port { address: listener.local_addr()?, token: token(), closed: Default::default() };
    let (token, closed) = (port.token.clone(), port.closed.clone());
    std::thread::Builder::new().name("org-lsp-port".to_string()).spawn(move || {
        for stream in listener.incoming().flatten() {
            if closed.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            if let Some(command) = receive(stream, &token) {
                if sender.send(command).is_err() {
                    return;
                }
            }
        }
    })?;
    Ok(port)
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
        let timeout = std::time::Duration::from_millis(300);
        for _ in 0..100 {
            if TcpStream::connect_timeout(&self.address, timeout).is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Whether the script at `path` sends to this port.
    pub fn owns(&self, path: &Path) -> bool {
        sent_to(path).is_some_and(|(address, _)| address == self.address.to_string())
    }

    /// Whether the script at `path` sends to another server that answers.
    pub fn answered_elsewhere(&self, path: &Path) -> bool {
        sent_to(path).is_some_and(|(address, _)| {
            address != self.address.to_string()
                && address.parse().is_ok_and(|address: SocketAddr| {
                    TcpStream::connect_timeout(&address, std::time::Duration::from_millis(300)).is_ok()
                })
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
fn on_path(name: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ".cmd"] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|directory| {
        suffixes.iter().map(|suffix| directory.join(format!("{name}{suffix}"))).find(|path| path.is_file())
    })
}

/// Read one command, and answer whether it is taken.
fn receive(mut stream: TcpStream, token: &str) -> Option<Command> {
    let mut line = String::new();
    BufReader::new(Read::by_ref(&mut stream).take(4096)).read_line(&mut line).ok()?;
    let sent: Option<Sent> = serde_json::from_str(&line).ok();
    let command = sent
        .filter(|sent| sent.token == token)
        .map(|sent| Command { command: sent.command, position: sent.line.zip(sent.character) });
    let _ = stream.write_all(if command.is_some() { b"ok\n" } else { b"refused\n" });
    command
}

/// A token no other process can guess: the hash keys of the standard
/// library are random per process.
fn token() -> String {
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
fn script(address: &SocketAddr, token: &str, start: &Start) -> std::io::Result<String> {
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', r"'\''"));
    let exe = quote(&std::env::current_exe()?);
    let options = start_options(start, quote);
    Ok(format!(
        "#!/bin/sh\n# Runs a command in the agenda of an org-lsp: COMMAND [ROW COLUMN].\n\
         exec {exe} command --script \"$0\" {options}{address} {token} \"$@\"\n"
    ))
}

#[cfg(not(unix))]
fn script(address: &SocketAddr, token: &str, start: &Start) -> std::io::Result<String> {
    let quote = |path: &Path| format!("\"{}\"", path.display());
    let exe = quote(&std::env::current_exe()?);
    let options = start_options(start, quote);
    Ok(format!(
        "@rem Runs a command in the agenda of an org-lsp: COMMAND [ROW COLUMN].\r\n\
         @{exe} command --script \"%~f0\" {options}{address} {token} %*\r\n"
    ))
}

/// The address and the token of the server the script `text` sends to:
/// the last two words before the command's own arguments.
fn address_and_token(text: &str) -> Option<(String, String)> {
    let words: Vec<&str> = text.lines().last()?.split_whitespace().collect();
    let [.., address, token, _] = words.as_slice() else { return None };
    Some((address.to_string(), token.to_string()))
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
/// EDITOR] ADDRESS TOKEN COMMAND [ROW COLUMN]`: send a command to the port
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
        [address, token, command] | [address, token, command, _, _] => (address, token, command),
        _ => return Err("expected ADDRESS TOKEN COMMAND [ROW COLUMN]".to_string()),
    };
    let number =
        |text: &str| text.parse::<u32>().map(|n| n.saturating_sub(1)).map_err(|_| format!("not a number: {text}"));
    let mut sent = json!({ "token": token, "command": command });
    if let [_, _, _, row, column] = rest {
        sent["line"] = json!(number(row)?);
        sent["character"] = json!(number(column)?);
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
    let mut stream =
        TcpStream::connect(address).map_err(|error| Failure::Unreachable(format!("{address}: {error}")))?;
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
        let port = open(sender).unwrap();
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
            Command { command: "org.agenda.later".to_string(), position: Some((2, 0)) }
        );
        send(&arguments(&token, &[])).unwrap();
        assert_eq!(receiver.recv().unwrap(), Command { command: "org.agenda.later".to_string(), position: None });
        assert!(send(&arguments("guessed", &[])).is_err());
        assert!(receiver.try_recv().is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(std::fs::metadata(script.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
            // The script runs `org-lsp command` with its own path and the port.
            assert!(text.contains(&format!(" command --script \"$0\" {address} {token} \"$@\"")), "{text}");
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
        // A server that stopped: its port is closed.
        let gone = {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            Port { address: listener.local_addr().unwrap(), token: token(), closed: Default::default() }
        };
        gone.claim(&script, &start).unwrap();
        // The new server takes the script over a moment after the editor
        // opened the file.
        let (sender, receiver) = crossbeam_channel::unbounded();
        let new = open(sender).unwrap();
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
        let third = open(third_sender).unwrap();
        assert!(third.answered_elsewhere(&script) && !third.owns(&script));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
