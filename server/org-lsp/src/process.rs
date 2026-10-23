//! Programs the server runs and waits for: `man`, `info`, `ctags`,
//! Graphviz's `dot`, a `shell:` link's command. Each is waited for until
//! a deadline, as `emacs.rs` waits for Emacs, and read up to a size: one
//! that runs past either is stopped and the command fails saying so (a
//! `shell:` link shows the start of its output), rather than the server
//! waiting for it or keeping all it writes. On Unix the program runs in a
//! process group of its own, stopped whole with what it started; on
//! Windows, with no job object, only the program itself is stopped.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

/// How long `man`, `info`, `ctags` and `dot` may take.
pub const PROGRAM_TIME: Duration = Duration::from_secs(60);

/// How much `man`, `info`, `ctags` and `dot` may write, to their output
/// and to their errors each: more than any page.
pub const PROGRAM_SIZE: usize = 16 << 20;

/// `command` run as `Command::output` runs it, its output and its errors
/// read, but stopped after `limit`, or once either is longer than `size`.
pub fn output(command: &mut Command, limit: Duration, size: usize) -> io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(command)?;
    let deadline = Some(Instant::now() + limit);
    let stdout = read_in_background(child.stdout.take(), size);
    let stderr = read_in_background(child.stderr.take(), size);
    let (stdout, cut) = collect(&stdout, &mut child, deadline, limit)?;
    if cut {
        return Err(too_much(&mut child, size));
    }
    let (stderr, cut) = collect(&stderr, &mut child, deadline, limit)?;
    if cut {
        return Err(too_much(&mut child, size));
    }
    let status = wait(&mut child, deadline, limit)?;
    Ok(Output { status, stdout, stderr })
}

/// What a command run in the shell wrote, and how it ended.
#[derive(Debug)]
pub struct ShellOutput {
    /// Its output and its errors, in the order it wrote them.
    pub text: String,
    /// Its exit code; none when it was stopped, or killed by a signal.
    pub code: Option<i32>,
    /// Whether it wrote more than it could and was stopped: `text` is the
    /// start of what it wrote.
    pub cut: bool,
}

/// Run `line` in the shell from `directory` and wait for it, until
/// `limit` if there is one; stopped once it has written `size` bytes.
pub fn shell_output(
    line: &str,
    directory: Option<&std::path::Path>,
    limit: Option<Duration>,
    size: usize,
) -> io::Result<ShellOutput> {
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
    let (output, cut) = collect(&read_in_background(Some(reader), size), &mut child, deadline, limit)?;
    let text = String::from_utf8_lossy(&output).into_owned();
    if cut {
        kill(&mut child);
        return Ok(ShellOutput { text, code: None, cut });
    }
    let status = wait(&mut child, deadline, limit)?;
    Ok(ShellOutput { text, code: status.code(), cut })
}

/// The shell running `line`: `cmd /C` on Windows, `sh -c` elsewhere.
pub fn shell(line: &str) -> Command {
    #[cfg(windows)]
    {
        // The line as it is: `cmd` does not read its command line as
        // programs read the arguments Rust quotes.
        use std::os::windows::process::CommandExt;
        let mut shell = Command::new("cmd");
        shell.raw_arg(format!("/C {line}"));
        shell
    }
    #[cfg(not(windows))]
    {
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

/// What `reader` gives, read on a thread of its own: `size` bytes at
/// most, and whether there was more. The reading ends there, which
/// closes the pipe.
fn read_in_background<R: Read + Send + 'static>(
    reader: Option<R>,
    size: usize,
) -> Receiver<io::Result<(Vec<u8>, bool)>> {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    match reader {
        Some(reader) => {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let read = reader.take(size as u64 + 1).read_to_end(&mut bytes).map(|_| {
                    let cut = bytes.len() > size;
                    bytes.truncate(size);
                    (bytes, cut)
                });
                let _ = sender.send(read);
            });
        }
        None => {
            let _ = sender.send(Ok((Vec::new(), false)));
        }
    }
    receiver
}

/// What was read into `receiver` once the reading ends, and whether it
/// was cut; the program stopped if that is not before `deadline`.
fn collect(
    receiver: &Receiver<io::Result<(Vec<u8>, bool)>>,
    child: &mut Child,
    deadline: Option<Instant>,
    limit: Duration,
) -> io::Result<(Vec<u8>, bool)> {
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

/// Stop the program that ran past `limit`; the error that says so.
fn stop(child: &mut Child, limit: Duration) -> io::Error {
    kill(child);
    let seconds = limit.as_secs_f64();
    io::Error::new(io::ErrorKind::TimedOut, format!("stopped after {seconds} seconds without an end"))
}

/// Stop the program that wrote more than `size`; the error that says so.
fn too_much(child: &mut Child, size: usize) -> io::Error {
    kill(child);
    io::Error::other(format!("stopped after writing more than {} MB", size >> 20))
}

/// Stop the program, with what it started where it has a process group,
/// and reap it.
fn kill(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // Its process group, which `spawn` made, numbered as it is.
        // SAFETY: `kill` only sends a signal; a group already gone is an
        // error, ignored.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn waits_for_a_program_until_its_deadline() {
        let ran = shell_output("echo out; echo err >&2; exit 3", None, Some(PROGRAM_TIME), PROGRAM_SIZE).unwrap();
        assert_eq!((ran.text.as_str(), ran.code, ran.cut), ("out\nerr\n", Some(3), false));
        let found = output_of("printf hi");
        assert_eq!(found.stdout, b"hi");
        // A program that does not end, nor what it started, is stopped.
        let marker = std::env::temp_dir().join(format!("org-lsp-process-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let started = Instant::now();
        let line = format!("(sleep 2; touch {}) & sleep 30", marker.display());
        let error = shell_output(&line, None, Some(Duration::from_millis(300)), PROGRAM_SIZE).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "stopped after 0.3 seconds without an end");
        assert!(started.elapsed() < Duration::from_secs(5));
        let error = output(Command::new("sleep").arg("30"), Duration::from_millis(200), PROGRAM_SIZE).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        std::thread::sleep(Duration::from_millis(2500));
        assert!(!marker.exists(), "what the command started ran on");
    }

    #[test]
    fn stops_a_program_that_writes_too_much() {
        // With no deadline it is the size that stops `yes`, and what it
        // started, at once: its output cut.
        let started = Instant::now();
        let ran = shell_output("yes & yes", None, None, 1 << 20).unwrap();
        assert!(ran.cut && ran.code.is_none());
        assert_eq!(ran.text.len(), 1 << 20);
        assert!(ran.text.starts_with("y\ny\n"));
        for line in ["head -c 300000000 /dev/zero", "head -c 300000000 /dev/zero >&2", "trap '' PIPE; yes"] {
            let error = output(&mut shell(line), PROGRAM_TIME, 1 << 20).unwrap_err();
            assert_eq!(error.to_string(), "stopped after writing more than 1 MB", "{line}");
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        // As much as it may write is not too much.
        let written = output(&mut shell("head -c 1048576 /dev/zero"), PROGRAM_TIME, 1 << 20).unwrap();
        assert_eq!(written.stdout.len(), 1 << 20);
    }

    fn output_of(line: &str) -> Output {
        output(&mut shell(line), PROGRAM_TIME, PROGRAM_SIZE).unwrap()
    }
}
