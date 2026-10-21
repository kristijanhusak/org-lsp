use std::io::Write;
use std::process::ExitCode;

const USAGE: &str = "\
org-lsp: a language server for Org mode

Usage: org-lsp [--stdio]
       org-lsp agenda [OPTIONS] FILE...
       org-lsp config [--options JSON]
       org-lsp manifest
       org-lsp mcp
       org-lsp settings
       org-lsp command ADDRESS TOKEN COMMAND ROW COLUMN
       org-lsp protocol URL
       org-lsp --version

The server communicates over standard input and output.";

/// Write to standard output. A reader that stops early (`org-lsp agenda |
/// head`) is not an error.
fn print(text: &str) {
    let _ = std::io::stdout().write_all(text.as_bytes());
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|argument| argument == "agenda") {
        return match org_lsp::cli::agenda(&arguments[1..]) {
            Ok(output) => {
                print(&output);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("org-lsp agenda: {error}\n\n{}", org_lsp::cli::USAGE);
                ExitCode::from(2)
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "manifest") {
        // What editor integrations are built from; see `docs/protocol.org`.
        print(&format!("{:#}\n", org_lsp::manifest::manifest()));
        return ExitCode::SUCCESS;
    }
    if arguments.first().is_some_and(|argument| argument == "config") {
        if arguments[1..].iter().any(|argument| argument == "--help" || argument == "-h") {
            print(&format!("{}\n", org_lsp::cli::CONFIG_USAGE));
            return ExitCode::SUCCESS;
        }
        return match org_lsp::cli::config(&arguments[1..], &|name| std::env::var(name).ok()) {
            Ok(output) => {
                print(&output);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("org-lsp config: {error}\n\n{}", org_lsp::cli::CONFIG_USAGE);
                ExitCode::from(2)
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "command") {
        // What the script next to the agenda runs: a command for the
        // server that wrote it (see `port`).
        return match org_lsp::port::send(&arguments[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("org-lsp command: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "protocol") {
        // What the system runs for an `org-protocol://` URL.
        if arguments[1..].iter().any(|argument| argument == "--help" || argument == "-h") {
            print(&format!("{}\n", org_lsp::protocol::USAGE));
            return ExitCode::SUCCESS;
        }
        return match org_lsp::protocol::run(&arguments[1..], &|name| std::env::var(name).ok()) {
            Ok(message) => {
                if !message.is_empty() {
                    print(&format!("{message}\n"));
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("org-lsp protocol: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "mcp") {
        // A Model Context Protocol server, for AI assistants to read the
        // agenda (`mcp`).
        if arguments[1..].iter().any(|argument| argument == "--help" || argument == "-h") {
            print(&format!("{}\n", org_lsp::mcp::USAGE));
            return ExitCode::SUCCESS;
        }
        return match org_lsp::mcp::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("org-lsp mcp: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "settings") {
        // The options with their types, defaults and meanings, which the
        // documentation is written from.
        print(&format!("{:#}\n", org_lsp::settings::manifest()));
        return ExitCode::SUCCESS;
    }
    for argument in arguments {
        match argument.as_str() {
            // Accepted for compatibility with clients that always pass it.
            "--stdio" => {}
            "--version" | "-V" => {
                print(&format!("org-lsp {}\n", env!("CARGO_PKG_VERSION")));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                print(&format!("{USAGE}\n"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("org-lsp: unknown argument `{other}`\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    // The main thread's stack is small on some platforms and cannot be
    // resized; the syntax tree walks recurse once per outline level.
    let server = std::thread::Builder::new().name("org-lsp".to_string()).stack_size(16 * 1024 * 1024).spawn(|| {
        let (connection, io_threads) = org_lsp::stdio::connection()?;
        org_lsp::run(connection)?;
        // `run` dropped the connection, which lets the I/O threads end.
        io_threads.join()?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });
    let result = match server {
        Ok(handle) => handle.join().unwrap_or_else(|_| Err("the server thread panicked".into())),
        Err(error) => Err(error.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("org-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}
