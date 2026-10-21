//! `org-lsp mcp`: a Model Context Protocol server over standard input and
//! output, for AI assistants to read the agenda as Org builds it.
//!
//! It is read-only: every tool builds a view of the agenda files with
//! `org-lsp agenda` ([`crate::cli::agenda`]) and answers its text. The
//! agenda files and the options are those of the shared configuration
//! file (`org-lsp config` shows them), read again at each call; with no
//! `org_agenda_files`, the Org files under the directory it runs in.
//!
//! Messages are JSON-RPC, one per line, as MCP's stdio transport has them.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

pub const USAGE: &str = "\
Usage: org-lsp mcp

Run a Model Context Protocol server over standard input and output, which
gives AI assistants the agenda, the TODO list and searches of the agenda
files of the shared configuration file (`org-lsp config`). It only reads.";

/// The protocol versions this server speaks, the newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Serve until standard input ends.
pub fn run() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let folder = std::env::current_dir().ok();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let env = |name: &str| std::env::var(name).ok();
        if let Some(answer) = answer(&line, folder.as_deref(), &env) {
            writeln!(stdout, "{answer}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The answer to one message, or `None` for a notification.
pub fn answer(line: &str, folder: Option<&std::path::Path>, env: &dyn Fn(&str) -> Option<String>) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(failure(Value::Null, -32700, &format!("Parse error: {error}"))),
    };
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked.filter(|asked| VERSIONS.contains(asked)).unwrap_or(VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "org-lsp", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "The user's Org mode agenda, read as Emacs's Org builds it. \
                    Dates are YYYY-MM-DD. The views are text as Org's agenda shows them: \
                    category, time, TODO keyword, headline and tags on each line.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call(name, &arguments, folder, env) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(Call::Unknown) => return Some(failure(id, -32602, &format!("Unknown tool: {name}"))),
                Err(Call::Failed(error)) => json!({ "content": [{ "type": "text", "text": error }], "isError": true }),
            }
        }
        _ => return Some(failure(id, -32601, &format!("Method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn failure(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    let tool = |name: &str, title: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": read_only,
        })
    };
    let date = json!({ "type": "string", "description": "YYYY-MM-DD; the default is today" });
    json!([
        tool(
            "agenda",
            "Agenda",
            "The agenda of some days (org-agenda-list): what is scheduled, due and timed each day, \
             with deadlines coming up and overdue tasks on today.",
            json!({
                "span": {
                    "type": "string",
                    "description": "day, week (the default), fortnight, month, year or a number of days",
                },
                "start": date,
                "log": {
                    "type": "boolean",
                    "description": "Also what was closed and clocked each day (log mode)",
                },
            }),
            &[],
        ),
        tool(
            "todo_list",
            "TODO list",
            "The TODO entries not done (org-todo-list), or those with the given keywords.",
            json!({
                "keywords": {
                    "type": "string",
                    "description": "Keywords separated by |, such as NEXT|WAIT; the default is every one not done",
                },
            }),
            &[],
        ),
        tool(
            "tags_search",
            "Tags and properties search",
            "The entries a match string selects (org-tags-view): tags (work-home), properties \
             (PRIORITY=\"A\", EFFORT>\"1:00\") and TODO keywords after a slash (+work/NEXT).",
            json!({
                "match": { "type": "string", "description": "An Org match string" },
                "todo_only": { "type": "boolean", "description": "Only entries with a TODO keyword not done" },
            }),
            &["match"],
        ),
        tool(
            "text_search",
            "Text search",
            "The entries whose text has words (org-search-view): words in any order, +word and \
             -word to have or not, {regexp}, or a phrase in double quotes.",
            json!({
                "query": { "type": "string", "description": "The words to search for" },
                "todo_only": { "type": "boolean", "description": "Only entries with a TODO keyword not done" },
            }),
            &["query"],
        ),
        tool(
            "custom_agenda",
            "Custom agenda",
            "A command of the user's org_agenda_custom_commands, by its key.",
            json!({ "key": { "type": "string", "description": "The command's key, such as n or wa" } }),
            &["key"],
        ),
        tool("agenda_files", "Agenda files", "The files the agenda is built from.", json!({}), &[]),
    ])
}

enum Call {
    Unknown,
    Failed(String),
}

fn call(
    name: &str,
    arguments: &Value,
    folder: Option<&std::path::Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Call> {
    let text = |key: &str| arguments.get(key).and_then(Value::as_str).map(str::to_string);
    let flag = |key: &str| arguments.get(key).and_then(Value::as_bool).unwrap_or(false);
    let required = |key: &str| {
        text(key).filter(|value| !value.trim().is_empty()).ok_or_else(|| Call::Failed(format!("`{key}` is required")))
    };
    let mut view: Vec<String> = match name {
        "agenda" => {
            let mut view = vec!["--view".into(), "agenda".into()];
            let span = match arguments.get("span") {
                Some(Value::Number(days)) => Some(days.to_string()),
                _ => text("span"),
            };
            view.extend(["--span".into(), span.unwrap_or_else(|| "week".into())]);
            if let Some(start) = text("start") {
                view.extend(["--start".into(), start]);
            }
            if flag("log") {
                view.push("--log".into());
            }
            view
        }
        "todo_list" => {
            let mut view = vec!["--view".into(), "todo".into()];
            if let Some(keywords) = text("keywords").filter(|keywords| !keywords.trim().is_empty()) {
                view.extend(["--match".into(), keywords]);
            }
            view
        }
        "tags_search" => {
            let kind = if flag("todo_only") { "tags-todo" } else { "tags" };
            vec!["--view".into(), kind.into(), "--match".into(), required("match")?]
        }
        "text_search" => {
            let kind = if flag("todo_only") { "search-todo" } else { "search" };
            vec!["--view".into(), kind.into(), "--match".into(), required("query")?]
        }
        "custom_agenda" => vec!["--view".into(), "custom".into(), "--key".into(), required("key")?],
        "agenda_files" => vec![],
        _ => return Err(Call::Unknown),
    };

    let loaded = crate::config::load(None, env);
    let options = &loaded.options;
    let mut workspace =
        crate::workspace::Workspace::new(options.agenda_files.clone(), folder.into_iter().map(Into::into).collect());
    workspace.file_regexp =
        options.agenda_file_regexp.as_deref().and_then(org_core::agenda::matcher::emacs_regex_with_case);
    let mut paths = workspace.files();
    // `org-journal-enable-agenda-integration`: the journal's files of
    // today and the days to come in place of the others, as the server has.
    if let Some(journal) = options.journal() {
        let now = crate::cli::now();
        let now = org_core::date::DateTime { day: now.today, minute: now.minute };
        if let Some((directory, names)) = crate::server::journal_agenda_files(&journal, now) {
            paths.retain(|path| !crate::server::is_journal(path, &journal));
            paths.extend(names.iter().map(|name| directory.join(name)));
        }
    }
    let files: Vec<String> = paths.iter().map(|path| path.display().to_string()).collect();
    if name == "agenda_files" {
        return Ok(if files.is_empty() { "No agenda files.".to_string() } else { files.join("\n") });
    }
    if files.is_empty() {
        return Err(Call::Failed(
            "No agenda files: set org_agenda_files in the configuration file (`org-lsp config` shows it).".to_string(),
        ));
    }
    view.extend(["--options".into(), loaded.merged.to_string()]);
    view.extend(files);
    crate::cli::agenda(&view).map_err(Call::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(line: &str, folder: &std::path::Path) -> Value {
        let no_file = |name: &str| (name == "ORG_LSP_CONFIG").then(|| "/nonexistent/config.toml".to_string());
        answer(line, Some(folder), &no_file).expect("an answer")
    }

    #[test]
    fn answers_initialize_lists_tools_and_builds_views() {
        let folder = std::env::temp_dir().join(format!("org-lsp-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("tasks.org"),
            "* TODO Write the report :work:\nSCHEDULED: <2026-10-21 Wed>\n* TODO Call Ana\n* DONE Old\n",
        )
        .unwrap();

        let init =
            ask(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, &folder);
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(answer(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, Some(&folder), &|_| None).is_none());

        let list = ask(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &folder);
        let names: Vec<&str> =
            list["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        assert_eq!(names, ["agenda", "todo_list", "tags_search", "text_search", "custom_agenda", "agenda_files"]);

        let text = |answer: &Value| answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
        let todo = ask(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"todo_list","arguments":{}}}"#,
            &folder,
        );
        assert!(text(&todo).contains("TODO Write the report"), "{todo}");
        assert!(text(&todo).contains("TODO Call Ana"), "{todo}");
        assert!(!text(&todo).contains("Old"), "{todo}");

        let tags = ask(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"tags_search","arguments":{"match":"work"}}}"#,
            &folder,
        );
        assert!(text(&tags).contains("Write the report"), "{tags}");
        assert!(!text(&tags).contains("Call Ana"), "{tags}");

        let day = ask(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"agenda","arguments":{"span":"day","start":"2026-10-21"}}}"#,
            &folder,
        );
        assert!(text(&day).contains("Scheduled:  TODO Write the report"), "{day}");

        let missing = ask(
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"tags_search","arguments":{}}}"#,
            &folder,
        );
        assert_eq!(missing["result"]["isError"], true);
        let unknown = ask(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nope"}}"#, &folder);
        assert_eq!(unknown["error"]["code"], -32602);
        let method = ask(r#"{"jsonrpc":"2.0","id":8,"method":"resources/list"}"#, &folder);
        assert_eq!(method["error"]["code"], -32601);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn lists_the_journal_files_the_agenda_takes() {
        let folder = std::env::temp_dir().join(format!("org-lsp-mcp-journal-{}", std::process::id()));
        let journal = folder.join("journal");
        std::fs::create_dir_all(&journal).unwrap();
        std::fs::write(folder.join("tasks.org"), "* TODO Task\n").unwrap();
        std::fs::write(journal.join("20000101"), "* Old day\n").unwrap();
        std::fs::write(journal.join("20301230"), "* TODO Far ahead\n").unwrap();
        let config = folder.join("config.toml");
        std::fs::write(
            &config,
            format!(
                "org_agenda_files = [\"{}\"]\norg_journal_enabled = true\norg_journal_dir = \"{}\"\n\
                 org_journal_enable_agenda_integration = true\n",
                folder.join("tasks.org").display(),
                journal.display()
            ),
        )
        .unwrap();
        let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.display().to_string());
        let files = answer(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agenda_files","arguments":{}}}"#,
            Some(&folder),
            &env,
        )
        .unwrap();
        let text = files["result"]["content"][0]["text"].as_str().unwrap_or_default();
        assert!(text.contains("tasks.org") && text.contains("20301230") && !text.contains("20000101"), "{files}");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
