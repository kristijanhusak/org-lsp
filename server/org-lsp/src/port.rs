//! The command port: how a key reaches the server in an editor whose keys
//! cannot run a language server's command, but can run a program (Zed's
//! tasks). The server listens on a local port while the agenda is a file,
//! and writes next to it a script that sends a command there:
//!
//! ```text
//! org-lsp-command COMMAND ROW COLUMN
//! ```
//!
//! runs `COMMAND` (`org.agenda.later`) where the cursor is in the agenda,
//! as `workspace/executeCommand` would. The row and column count from 1.
//! The script holds a token the server checks, so that no other user's
//! program can run commands; the script is the owner's alone.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;

/// A command sent to the port: what to run, and where in the agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Deserialize)]
struct Sent {
    token: String,
    command: String,
    line: u32,
    character: u32,
}

/// The name of the script next to the agenda.
#[cfg(unix)]
pub const SCRIPT: &str = "org-lsp-command";
#[cfg(not(unix))]
pub const SCRIPT: &str = "org-lsp-command.cmd";

/// Listen on a local port for commands, which go to `sender`, and write
/// the script that sends them into `directory`. Returns the script.
pub fn open(directory: &Path, sender: crossbeam_channel::Sender<Command>) -> std::io::Result<PathBuf> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let token = token();
    let script = directory.join(SCRIPT);
    write_script(&script, &address, &token)?;
    std::thread::Builder::new().name("org-lsp-port".to_string()).spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Some(command) = receive(stream, &token) {
                if sender.send(command).is_err() {
                    return;
                }
            }
        }
    })?;
    Ok(script)
}

/// Read one command, and answer whether it is taken.
fn receive(mut stream: TcpStream, token: &str) -> Option<Command> {
    let mut line = String::new();
    BufReader::new(Read::by_ref(&mut stream).take(4096)).read_line(&mut line).ok()?;
    let sent: Option<Sent> = serde_json::from_str(&line).ok();
    let command = sent.filter(|sent| sent.token == token).map(|sent| Command {
        command: sent.command,
        line: sent.line,
        character: sent.character,
    });
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
fn write_script(script: &Path, address: &SocketAddr, token: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let exe = std::env::current_exe()?;
    let quoted = format!("'{}'", exe.display().to_string().replace('\'', r"'\''"));
    let text = format!(
        "#!/bin/sh\n# Runs a command of the org-lsp that wrote the agenda here: COMMAND ROW COLUMN.\n\
         exec {quoted} command {address} {token} \"$@\"\n"
    );
    let _ = std::fs::remove_file(script);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(script)?;
    file.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_script(script: &Path, address: &SocketAddr, token: &str) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let text = format!(
        "@rem Runs a command of the org-lsp that wrote the agenda here: COMMAND ROW COLUMN.\r\n\
         @\"{}\" command {address} {token} %*\r\n",
        exe.display()
    );
    std::fs::write(script, text)
}

/// `org-lsp command ADDRESS TOKEN COMMAND ROW COLUMN`: send a command to
/// the port of a running server.
pub fn send(arguments: &[String]) -> Result<(), String> {
    let [address, token, command, row, column] = arguments else {
        return Err("expected ADDRESS TOKEN COMMAND ROW COLUMN".to_string());
    };
    let number =
        |text: &str| text.parse::<u32>().map(|n| n.saturating_sub(1)).map_err(|_| format!("not a number: {text}"));
    let sent = json!({ "token": token, "command": command, "line": number(row)?, "character": number(column)? });
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
        std::fs::create_dir_all(&directory).unwrap();
        let (sender, receiver) = crossbeam_channel::unbounded();
        let script = open(&directory, sender).unwrap();
        let text = std::fs::read_to_string(&script).unwrap();
        let words: Vec<&str> = text.lines().last().unwrap().split_whitespace().collect();
        let at = words.iter().position(|word| *word == "command").unwrap();
        let (address, token) = (words[at + 1].to_string(), words[at + 2].to_string());
        let arguments =
            |token: &str| [address.clone(), token.to_string(), "org.agenda.later".into(), "3".into(), "1".into()];
        send(&arguments(&token)).unwrap();
        assert_eq!(
            receiver.recv().unwrap(),
            Command { command: "org.agenda.later".to_string(), line: 2, character: 0 }
        );
        assert!(send(&arguments("guessed")).is_err());
        assert!(receiver.try_recv().is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o777, 0o700);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
