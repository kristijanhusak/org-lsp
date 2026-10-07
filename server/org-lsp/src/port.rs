//! The command port: how a key reaches the server in an editor whose keys
//! cannot run a language server's command, but can run a program (Zed's
//! tasks). The server listens on a local port while the agenda is a file,
//! and writes a script in a place of the user's own that sends a command
//! there:
//!
//! ```text
//! ~/.local/state/org-lsp/agenda-command COMMAND [ROW COLUMN]
//! ```
//!
//! runs `COMMAND` (`org.agenda.later`) in the agenda, as
//! `workspace/executeCommand` would: at the row and column given (from 1),
//! or else where the server last saw the cursor. The place is fixed, so
//! that the editor's keys can name it: the server whose agenda the cursor
//! was last in makes it its own. The script holds a token the server
//! checks, so that no other user's program can run commands; the script is
//! the owner's alone.

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

/// An open port: where it listens, and the token it asks for.
#[derive(Debug, Clone)]
pub struct Port {
    address: SocketAddr,
    token: String,
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
    let port = Port { address: listener.local_addr()?, token: token() };
    let token = port.token.clone();
    std::thread::Builder::new().name("org-lsp-port".to_string()).spawn(move || {
        for stream in listener.incoming().flatten() {
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
    /// Make the script at `path` send to this port, unless it does.
    pub fn claim(&self, path: &Path) -> std::io::Result<()> {
        let text = script(&self.address, &self.token)?;
        if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
            return Ok(());
        }
        if let Some(directory) = path.parent() {
            create_private_dir(directory)?;
        }
        write_script(path, &text)
    }

    /// Remove the script at `path` if it sends to this port.
    pub fn release(&self, path: &Path) {
        if script(&self.address, &self.token)
            .is_ok_and(|text| std::fs::read_to_string(path).is_ok_and(|old| old == text))
        {
            let _ = std::fs::remove_file(path);
        }
    }
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

#[cfg(unix)]
fn script(address: &SocketAddr, token: &str) -> std::io::Result<String> {
    let exe = std::env::current_exe()?;
    let quoted = format!("'{}'", exe.display().to_string().replace('\'', r"'\''"));
    Ok(format!(
        "#!/bin/sh\n# Runs a command in the agenda of an org-lsp: COMMAND [ROW COLUMN].\n\
         exec {quoted} command {address} {token} \"$@\"\n"
    ))
}

#[cfg(not(unix))]
fn script(address: &SocketAddr, token: &str) -> std::io::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!(
        "@rem Runs a command in the agenda of an org-lsp: COMMAND [ROW COLUMN].\r\n\
         @\"{}\" command {address} {token} %*\r\n",
        exe.display()
    ))
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

/// `org-lsp command ADDRESS TOKEN COMMAND [ROW COLUMN]`: send a command
/// to the port of a running server.
pub fn send(arguments: &[String]) -> Result<(), String> {
    let (address, token, command, position) = match arguments {
        [address, token, command] => (address, token, command, None),
        [address, token, command, row, column] => (address, token, command, Some((row, column))),
        _ => return Err("expected ADDRESS TOKEN COMMAND [ROW COLUMN]".to_string()),
    };
    let number =
        |text: &str| text.parse::<u32>().map(|n| n.saturating_sub(1)).map_err(|_| format!("not a number: {text}"));
    let mut sent = json!({ "token": token, "command": command });
    if let Some((row, column)) = position {
        sent["line"] = json!(number(row)?);
        sent["character"] = json!(number(column)?);
    }
    let mut stream = TcpStream::connect(address.as_str()).map_err(|error| format!("{address}: {error}"))?;
    stream.write_all(format!("{sent}\n").as_bytes()).map_err(|error| error.to_string())?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).map_err(|error| error.to_string())?;
    match answer.trim() {
        "ok" => Ok(()),
        other => Err(format!("the server answered `{other}`")),
    }
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
        port.claim(&script).unwrap();
        let text = std::fs::read_to_string(&script).unwrap();
        let words: Vec<&str> = text.lines().last().unwrap().split_whitespace().collect();
        let at = words.iter().position(|word| *word == "command").unwrap();
        let (address, token) = (words[at + 1].to_string(), words[at + 2].to_string());
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
        }
        // Another server takes the script over; the first then leaves it.
        let (other_sender, _other_receiver) = crossbeam_channel::unbounded();
        let other = open(other_sender).unwrap();
        other.claim(&script).unwrap();
        port.release(&script);
        assert!(script.exists());
        other.release(&script);
        assert!(!script.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
