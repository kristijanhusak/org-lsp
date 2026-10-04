use std::process::ExitCode;

const USAGE: &str = "\
org-lsp: a language server for Org mode

Usage: org-lsp [--stdio]
       org-lsp agenda [OPTIONS] FILE...
       org-lsp --version

The server communicates over standard input and output.";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|argument| argument == "agenda") {
        return match org_lsp::cli::agenda(&arguments[1..]) {
            Ok(output) => {
                print!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("org-lsp agenda: {error}\n\n{}", org_lsp::cli::USAGE);
                ExitCode::from(2)
            }
        };
    }
    for argument in arguments {
        match argument.as_str() {
            // Accepted for compatibility with clients that always pass it.
            "--stdio" => {}
            "--version" | "-V" => {
                println!("org-lsp {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
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
        let (connection, io_threads) = lsp_server::Connection::stdio();
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
