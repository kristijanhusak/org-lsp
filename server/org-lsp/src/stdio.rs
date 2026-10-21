//! The connection over standard input and output. It is `lsp-server`'s,
//! except that a message that is not one (text that is not JSON, a batch
//! array) is left out with a line in the log, where `lsp-server` stops
//! reading and the server with it.

use std::io::{self, BufRead};
use std::thread::JoinHandle;

use crossbeam_channel::{bounded, Sender};
use lsp_server::{Connection, Message, Notification};

/// The threads that read and write; `join` waits for them once the server
/// has dropped its connection.
pub struct IoThreads {
    reader: JoinHandle<io::Result<()>>,
    writer: JoinHandle<io::Result<()>>,
}

impl IoThreads {
    pub fn join(self) -> io::Result<()> {
        let reader = self.reader.join().unwrap_or_else(|_| Err(io::Error::other("the reader panicked")));
        let writer = self.writer.join().unwrap_or_else(|_| Err(io::Error::other("the writer panicked")));
        reader.and(writer)
    }
}

/// A connection over standard input and output.
pub fn connection() -> io::Result<(Connection, IoThreads)> {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let writer = std::thread::Builder::new().name("org-lsp writer".to_string()).spawn(move || {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        writer_receiver.into_iter().try_for_each(|message| message.write(&mut stdout))
    })?;
    let (reader_sender, reader_receiver) = bounded::<Message>(0);
    let log = writer_sender.clone();
    let reader = std::thread::Builder::new().name("org-lsp reader".to_string()).spawn(move || {
        let stdin = io::stdin();
        read_messages(&mut stdin.lock(), &reader_sender, &log)
    })?;
    let connection = Connection { sender: writer_sender, receiver: reader_receiver };
    Ok((connection, IoThreads { reader, writer }))
}

/// Read messages from `input` and hand them to the server until the
/// input ends or the editor says `exit`. One that is not a message is
/// said in the log (`window/logMessage`, and standard error) and left
/// out; JSON-RPC's answer to it, an error without an id, would match no
/// request of the editor's.
pub fn read_messages(input: &mut dyn BufRead, server: &Sender<Message>, log: &Sender<Message>) -> io::Result<()> {
    while let Some(text) = read_text(input)? {
        let message = match serde_json::from_str::<Message>(&text) {
            Ok(message) => message,
            Err(error) => {
                let shown: String = text.chars().take(200).collect();
                let said = format!("org-lsp: left out a message that is not one ({error}): {shown}");
                eprintln!("{said}");
                let params = serde_json::json!({ "type": 1, "message": said });
                let _ = log.send(Message::Notification(Notification::new("window/logMessage".to_string(), params)));
                continue;
            }
        };
        let exit = matches!(&message, Message::Notification(notification) if notification.method == "exit");
        server.send(message).map_err(io::Error::other)?;
        if exit {
            break;
        }
    }
    Ok(())
}

/// The text of the next message: its headers, then as many bytes as its
/// `Content-Length` says. Headers that cannot be read end the connection,
/// since where the next message starts is then unknown.
fn read_text(input: &mut dyn BufRead) -> io::Result<Option<String>> {
    let invalid = |text: String| io::Error::new(io::ErrorKind::InvalidData, text);
    let mut size = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let Some(header) = line.strip_suffix("\r\n") else {
            return Err(invalid(format!("malformed header: {line:?}")));
        };
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(": ").ok_or_else(|| invalid(format!("malformed header: {header:?}")))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            size = Some(value.trim().parse::<usize>().map_err(|error| invalid(error.to_string()))?);
        }
    }
    let size = size.ok_or_else(|| invalid("no Content-Length".to_string()))?;
    let mut body = vec![0; size];
    input.read_exact(&mut body)?;
    // Text that is not UTF-8 is a message that is not one, not the end.
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

    #[test]
    fn leaves_out_what_is_not_a_message() {
        let input = [
            frame("{not json}"),
            frame(r#"[{"jsonrpc":"2.0","method":"initialized","params":{}}]"#),
            frame(r#"{"jsonrpc":"2.0","id":1,"method":"shutdown"}"#),
            frame(r#"{"jsonrpc":"2.0","method":"exit"}"#),
            frame(r#"{"jsonrpc":"2.0","id":2,"method":"never read"}"#),
        ]
        .concat();
        let (server, received) = crossbeam_channel::unbounded();
        let (log, logged) = crossbeam_channel::unbounded();
        read_messages(&mut input.as_bytes(), &server, &log).unwrap();
        let methods: Vec<String> = received
            .try_iter()
            .map(|message| match message {
                Message::Request(request) => request.method,
                Message::Notification(notification) => notification.method,
                Message::Response(_) => "response".to_string(),
            })
            .collect();
        assert_eq!(methods, ["shutdown", "exit"]);
        let logged: Vec<Message> = logged.try_iter().collect();
        assert_eq!(logged.len(), 2);
        let Message::Notification(notification) = &logged[0] else { panic!("{logged:?}") };
        assert_eq!(notification.method, "window/logMessage");
        assert!(notification.params["message"].as_str().unwrap().contains("{not json}"), "{notification:?}");
    }

    #[test]
    fn ends_at_the_end_of_the_input_and_on_a_broken_header() {
        let (server, _received) = crossbeam_channel::unbounded();
        let (log, _logged) = crossbeam_channel::unbounded();
        assert!(read_messages(&mut "".as_bytes(), &server, &log).is_ok());
        assert!(read_messages(&mut "Content-Length 3\r\n\r\n{}".as_bytes(), &server, &log).is_err());
    }
}
