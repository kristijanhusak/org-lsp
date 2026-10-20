//! Programs the server runs and waits for: `man`, `info`, `ctags`,
//! Graphviz's `dot`, a `shell:` link's command. Each is waited for until
//! a deadline, as `emacs.rs` waits for Emacs: one that runs past it is
//! stopped, with what it started, and the command fails saying so, rather
//! than the server waiting for it.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

/// How long `man`, `info`, `ctags` and `dot` may take.
pub const PROGRAM_TIME: Duration = Duration::from_secs(60);

/// `command` run as `Command::output` runs it, its output and its errors
/// read, but stopped after `limit`.
pub fn output(command: &mut Command, limit: Duration) -> io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command)?;
    let deadline = Some(Instant::now() + limit);
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());
    let stdout = collect(&stdout, &mut child, deadline, limit)?;
    let stderr = collect(&stderr, &mut child, deadline, limit)?;
    let status = wait(&mut child, deadline, limit)?;
    Ok(Output { status, stdout, stderr })
}

/// Run `line` in the shell from `directory` and wait for it, until
/// `limit` if there is one: what it wrote to its output and its errors,
/// in the order it wrote them, and its exit code.
pub fn shell_output(
    line: &str,
    directory: Option<&std::path::Path>,
    limit: Option<Duration>,
) -> io::Result<(String, Option<i32>)> {
    let mut shell = shell(line);
    if let Some(directory) = directory {
        shell.current_dir(directory);
    }
    let (reader, writer) = io::pipe()?;
    shell.stdin(Stdio::null()).stdout(writer.try_clone()?).stderr(writer);
    let mut child = spawn(&mut shell)?;
    // The pipe's ends the server held are gone with `shell`, so reading
    // ends when the command does.
    drop(shell);
    let deadline = limit.map(|limit| Instant::now() + limit);
    let limit = limit.unwrap_or_default();
    let output = collect(&read_in_background(Some(reader)), &mut child, deadline, limit)?;
    let status = wait(&mut child, deadline, limit)?;
    Ok((String::from_utf8_lossy(&output).into_owned(), status.code()))
}

/// The shell running `line`: `cmd /C` on Windows, `sh -c` elsewhere.
pub fn shell(line: &str) -> Command {
    if cfg!(windows) {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(line);
        shell
    } else {
        let mut shell = Command::new("sh");
        shell.arg("-c").arg(line);
        shell
    }
}

/// Start `command`, in a process group of its own where there are any, so
/// that what it starts is stopped with it.
fn spawn(command: &mut Command) -> io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// All that `reader` gives, read on a thread of its own.
fn read_in_background<R: Read + Send + 'static>(reader: Option<R>) -> Receiver<io::Result<Vec<u8>>> {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    match reader {
        Some(mut reader) => {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = sender.send(reader.read_to_end(&mut bytes).map(|_| bytes));
            });
        }
        None => {
            let _ = sender.send(Ok(Vec::new()));
        }
    }
    receiver
}

/// What was read into `receiver`, once the reading ends; the program
/// stopped if that is not before `deadline`.
fn collect(
    receiver: &Receiver<io::Result<Vec<u8>>>,
    child: &mut Child,
    deadline: Option<Instant>,
    limit: Duration,
) -> io::Result<Vec<u8>> {
    let received = match deadline {
        Some(deadline) => receiver.recv_deadline(deadline).map_err(|_| stop(child, limit)),
        None => receiver.recv().map_err(|_| io::Error::other("the output was lost")),
    };
    received?
}

/// The program's exit, once it ends; stopped if that is not before
/// `deadline`.
fn wait(child: &mut Child, deadline: Option<Instant>, limit: Duration) -> io::Result<ExitStatus> {
    let Some(deadline) = deadline else { return child.wait() };
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(stop(child, limit));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Stop the program and what it started; the error that says so.
fn stop(child: &mut Child, limit: Duration) -> io::Error {
    #[cfg(unix)]
    {
        // Its process group, which `spawn` made.
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
    let seconds = limit.as_secs_f64();
    io::Error::new(io::ErrorKind::TimedOut, format!("stopped after {seconds} seconds without an end"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn waits_for_a_program_until_its_deadline() {
        let (written, code) = shell_output("echo out; echo err >&2; exit 3", None, Some(PROGRAM_TIME)).unwrap();
        assert_eq!((written.as_str(), code), ("out\nerr\n", Some(3)));
        let found = output_of("printf hi");
        assert_eq!(found.stdout, b"hi");
        // A program that does not end, nor what it started, is stopped.
        let marker = std::env::temp_dir().join(format!("org-lsp-process-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let started = Instant::now();
        let line = format!("(sleep 2; touch {}) & sleep 30", marker.display());
        let error = shell_output(&line, None, Some(Duration::from_millis(300))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "stopped after 0.3 seconds without an end");
        assert!(started.elapsed() < Duration::from_secs(5));
        let error = output(Command::new("sleep").arg("30"), Duration::from_millis(200)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        std::thread::sleep(Duration::from_millis(2500));
        assert!(!marker.exists(), "what the command started ran on");
    }

    fn output_of(line: &str) -> Output {
        output(&mut shell(line), PROGRAM_TIME).unwrap()
    }
}
