//! `org-lsp mcp`: a Model Context Protocol server over standard input and
//! output, for AI assistants to read the agenda as Org builds it, and with
//! `mcp_edits` to change TODO states and capture.
//!
//! The view tools build a view of the agenda files with `org-lsp agenda`
//! ([`crate::cli::agenda_lines`]) and answer its text, each entry's line
//! ending with where its headline is (`[tasks.org:12]`). The agenda files
//! and the options are those of the shared configuration file (`org-lsp
//! config` shows them), read again at each call; with no
//! `org_agenda_files`, the Org files under the directory it runs in.
//!
//! The edit tools go through the code the server's commands go through
//! (`org-todo` with [`org_core::edit::run`], `org-capture` with
//! [`org_core::capture`]), and write the file directly: no editor is
//! asked, and no Emacs is started.
//!
//! Messages are JSON-RPC, one per line, as MCP's stdio transport has them.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use org_core::capture::{self, Filled, Question};
use org_core::date::DateTime;
use serde_json::{json, Value};

use crate::config::{CaptureTemplateOption, Options};
use crate::server::TargetSpec;

pub const USAGE: &str = "\
Usage: org-lsp mcp

Run a Model Context Protocol server over standard input and output, which
gives AI assistants the agenda, the TODO list and searches of the agenda
files of the shared configuration file (`org-lsp config`). It only reads,
unless that file sets `mcp_edits = true`: then it also changes TODO states
and captures, writing the files.";

/// The protocol versions this server speaks, the newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What an edit tool answers while `mcp_edits` is off.
const EDITS_OFF: &str = "org-lsp mcp only reads: set `mcp_edits = true` in the configuration file \
                         (`org-lsp config` shows which one) to let it change files.";

/// What a capture answers when its template needs Emacs.
const NEEDS_EMACS: &str = "This template needs Emacs, which org-lsp mcp does not start: capture it in the editor.";

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
pub fn answer(line: &str, folder: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> Option<Value> {
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
                    category, time, TODO keyword, headline and tags on each line, then where \
                    the entry's headline is, [file:line], which set_todo_state takes.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools(&crate::config::load(None, env).options) }),
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

/// The tools, the edit tools among them when `mcp_edits` is on.
fn tools(options: &Options) -> Value {
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    let edits = json!({
        "readOnlyHint": false,
        "destructiveHint": false,
        "idempotentHint": false,
        "openWorldHint": false,
    });
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
    let dry_run = json!({ "type": "boolean", "description": "Answer the change as a diff and write nothing" });
    let mut tools = vec![
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
        tool(
            "capture_templates",
            "Capture templates",
            "The user's capture templates (org-capture-templates): each one's key, description, \
             target, template and the questions it asks.",
            json!({}),
            &[],
        ),
    ];
    if options.mcp_edits != Some(true) {
        return Value::Array(tools);
    }
    let keys: Vec<String> = options
        .capture_templates()
        .iter()
        .filter(|template| !template.is_group())
        .map(|template| format!("{} ({})", template.key, template.description))
        .collect();
    let mut edit = |name: &str, title: &str, description: String, properties: Value, required: &[&str]| {
        let mut made = tool(name, title, &description, properties, required);
        made["annotations"] = edits.clone();
        tools.push(made);
    };
    edit(
        "set_todo_state",
        "Set TODO state",
        "Change the TODO state of an entry (org-todo), as Org does with the user's keywords and \
         logging: CLOSED, the LOGBOOK, notes, repeaters. Writes the file. The entry is the \
         [file:line] a view gives at the end of its line."
            .to_string(),
        json!({
            "file": { "type": "string", "description": "The entry's file: its path, or its name in the views" },
            "line": { "type": "integer", "minimum": 1, "description": "The line of the entry's headline, from 1" },
            "state": { "type": "string", "description": "A TODO keyword, or \"\" for none" },
            "note": { "type": "string", "description": "The note, for a state that asks for one (@)" },
            "headline": {
                "type": "string",
                "description": "Text the headline has, such as its title: refused if the line no longer has it",
            },
            "dry_run": dry_run,
        }),
        &["file", "line", "state"],
    );
    edit(
        "capture",
        "Capture",
        format!(
            "File a new entry with one of the user's capture templates (org-capture, as with \
             :immediate-finish). Writes the target file. Templates: {}.",
            if keys.is_empty() { "none".to_string() } else { keys.join(", ") }
        ),
        json!({
            "template": { "type": "string", "description": "The template's key" },
            "text": {
                "type": "string",
                "description": "What is captured: %i, or where the template's %? puts the cursor",
            },
            "answers": {
                "type": ["object", "array"],
                "description": "The answers to the template's questions (%^{Prompt}, %^t, %^g…): an object \
                                from each prompt to its answer, or a list in order",
            },
            "link": { "type": "string", "description": "The link of %a and %l, such as file:~/notes.org::*Ideas" },
            "description": { "type": "string", "description": "The link's description" },
            "dry_run": dry_run,
        }),
        &["template"],
    );
    Value::Array(tools)
}

enum Call {
    Unknown,
    Failed(String),
}

impl From<String> for Call {
    fn from(message: String) -> Self {
        Call::Failed(message)
    }
}

impl From<&str> for Call {
    fn from(message: &str) -> Self {
        Call::Failed(message.to_string())
    }
}

impl From<org_core::Refusal> for Call {
    fn from(refusal: org_core::Refusal) -> Self {
        Call::Failed(refusal.message().to_string())
    }
}

/// The arguments of a tool.
struct Arguments<'a>(&'a Value);

impl Arguments<'_> {
    fn text(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(Value::as_str).map(str::to_string)
    }

    fn flag(&self, key: &str) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    fn required(&self, key: &str) -> Result<String, Call> {
        self.text(key).filter(|value| !value.trim().is_empty()).ok_or_else(|| format!("`{key}` is required").into())
    }
}

fn call(
    name: &str,
    arguments: &Value,
    folder: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Call> {
    let arguments = Arguments(arguments);
    match name {
        "capture_templates" => Ok(capture_templates(&crate::config::load(None, env).options, folder)),
        "set_todo_state" | "capture" => {
            let options = crate::config::load(None, env).options;
            if options.mcp_edits != Some(true) {
                return Err(EDITS_OFF.into());
            }
            if name == "capture" {
                capture(&arguments, &options, folder)
            } else {
                set_todo_state(&arguments, &options, folder)
            }
        }
        _ => view(name, &arguments, folder, env),
    }
}

/// The files the agenda is built from, as the server finds them.
fn agenda_paths(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut workspace =
        crate::workspace::Workspace::new(options.agenda_files.clone(), folder.into_iter().map(Into::into).collect());
    workspace.file_regexp =
        options.agenda_file_regexp.as_deref().and_then(org_core::agenda::matcher::emacs_regex_with_case);
    let mut paths = workspace.files();
    // `org-journal-enable-agenda-integration`: the journal's files of
    // today and the days to come in place of the others, as the server has.
    if let Some(journal) = options.journal() {
        if let Some((directory, names)) = crate::server::journal_agenda_files(&journal, now()) {
            paths.retain(|path| !crate::server::is_journal(path, &journal));
            paths.extend(names.iter().map(|name| directory.join(name)));
        }
    }
    paths
}

fn now() -> DateTime {
    let now = crate::cli::now();
    DateTime { day: now.today, minute: now.minute }
}

/// A view of the agenda files, with where each entry's headline is.
fn view(
    name: &str,
    arguments: &Arguments,
    folder: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Call> {
    let mut view: Vec<String> = match name {
        "agenda" => {
            let mut view = vec!["--view".into(), "agenda".into()];
            let span = match arguments.0.get("span") {
                Some(Value::Number(days)) => Some(days.to_string()),
                _ => arguments.text("span"),
            };
            view.extend(["--span".into(), span.unwrap_or_else(|| "week".into())]);
            if let Some(start) = arguments.text("start") {
                view.extend(["--start".into(), start]);
            }
            if arguments.flag("log") {
                view.push("--log".into());
            }
            view
        }
        "todo_list" => {
            let mut view = vec!["--view".into(), "todo".into()];
            if let Some(keywords) = arguments.text("keywords").filter(|keywords| !keywords.trim().is_empty()) {
                view.extend(["--match".into(), keywords]);
            }
            view
        }
        "tags_search" => {
            let kind = if arguments.flag("todo_only") { "tags-todo" } else { "tags" };
            vec!["--view".into(), kind.into(), "--match".into(), arguments.required("match")?]
        }
        "text_search" => {
            let kind = if arguments.flag("todo_only") { "search-todo" } else { "search" };
            vec!["--view".into(), kind.into(), "--match".into(), arguments.required("query")?]
        }
        "custom_agenda" => vec!["--view".into(), "custom".into(), "--key".into(), arguments.required("key")?],
        "agenda_files" => vec![],
        _ => return Err(Call::Unknown),
    };

    let loaded = crate::config::load(None, env);
    let files: Vec<String> =
        agenda_paths(&loaded.options, folder).iter().map(|path| path.display().to_string()).collect();
    if name == "agenda_files" {
        return Ok(if files.is_empty() { "No agenda files.".to_string() } else { files.join("\n") });
    }
    if files.is_empty() {
        return Err(
            "No agenda files: set org_agenda_files in the configuration file (`org-lsp config` shows it).".into()
        );
    }
    view.extend(["--options".into(), loaded.merged.to_string()]);
    view.extend(files.iter().cloned());
    let lines = crate::cli::agenda_lines(&view)?;
    Ok(with_references(lines, &files))
}

/// The lines of a view, each entry's ending with `[file:line]`: the line
/// of its headline in its file, named as `file_label` names it.
fn with_references(lines: Vec<crate::cli::AgendaLine>, files: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&line.text);
        if let Some((file, number)) = &line.entry {
            out.push_str(&format!("  [{}:{number}]", file_label(file, files)));
        }
        out.push('\n');
    }
    out
}

/// How the views name `file`: by its name when no other of `files` has
/// it, else by its path.
fn file_label(file: &str, files: &[String]) -> String {
    let name = |file: &str| Path::new(file).file_name().map(|name| name.to_string_lossy().into_owned());
    let own = name(file);
    if own.is_some() && files.iter().filter(|other| name(other) == own).count() == 1 {
        own.unwrap_or_default()
    } else {
        file.to_string()
    }
}

// ----- edits

/// The files the edit tools may change: the agenda files and the targets
/// of the capture templates.
fn editable_files(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut files = agenda_paths(options, folder);
    for template in options.capture_templates().iter().filter(|template| !template.is_group()) {
        if let Ok((_, TargetSpec::File(_) | TargetSpec::DateTree(..))) = crate::server::capture_spec(template) {
            files.push(target_path(template, options));
        }
    }
    files
}

/// The file a template files into.
fn target_path(template: &CaptureTemplateOption, options: &Options) -> PathBuf {
    let path = template.target.get(1).map(String::as_str).unwrap_or("");
    crate::server::capture_path(path, &options.org_directory(), &options.default_notes_file())
}

/// A path as the file system has it, to tell whether two name one file.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf())
}

/// The file `given` names, among those the edit tools may change: by its
/// path (`~/` the home directory, relative to the folder the server runs
/// in), or by its name as the views give it.
fn editable_file(given: &str, options: &Options, folder: Option<&Path>) -> Result<PathBuf, Call> {
    let files = editable_files(options, folder);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = match (given.strip_prefix("~/"), &home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(given),
    };
    let path = match folder {
        Some(folder) if path.is_relative() => folder.join(path),
        _ => path,
    };
    let wanted = canonical(&path);
    if let Some(found) = files.iter().find(|file| canonical(file) == wanted) {
        return Ok(found.clone());
    }
    if !given.contains(['/', '\\']) {
        let named: Vec<&PathBuf> =
            files.iter().filter(|file| file.file_name().is_some_and(|name| name == given)).collect();
        match named[..] {
            [one] => return Ok(one.clone()),
            [_, _, ..] => {
                let paths: Vec<String> = named.iter().map(|file| file.display().to_string()).collect();
                return Err(format!("More than one file is named {given}: give its path ({})", paths.join(", ")).into());
            }
            [] => {}
        }
    }
    Err(format!(
        "{given} is neither an agenda file nor the target of a capture template: org-lsp mcp changes only those"
    )
    .into())
}

/// Write `text` to `path` at once: into a file beside it, which then
/// takes its place, so that no one reads it half written.
fn write_file(path: &Path, text: &str) -> Result<(), Call> {
    // A link stays a link: the file it points to is written.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory).map_err(|error| format!("Cannot make {}: {error}", directory.display()))?;
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = directory.join(format!(".{name}.org-lsp-{}.tmp", std::process::id()));
    let written = std::fs::write(&temporary, text).and_then(|()| {
        if let Ok(metadata) = std::fs::metadata(&path) {
            std::fs::set_permissions(&temporary, metadata.permissions())?;
        }
        std::fs::rename(&temporary, &path)
    });
    written.map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        Call::Failed(format!("Cannot write {}: {error}", path.display()))
    })
}

/// The lines that differ between `old` and `new`, as a unified diff with
/// two lines around them.
fn diff(name: &str, old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a_end, b_end) = (a.len() - suffix, b.len() - suffix);
    let start = prefix.saturating_sub(2);
    let after = suffix.min(2);
    let mut out = format!(
        "--- {name}\n+++ {name}\n@@ -{},{} +{},{} @@\n",
        start + 1,
        a_end + after - start,
        start + 1,
        b_end + after - start
    );
    for line in &a[start..prefix] {
        out.push_str(&format!(" {line}\n"));
    }
    for line in &a[prefix..a_end] {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &b[prefix..b_end] {
        out.push_str(&format!("+{line}\n"));
    }
    for line in &a[a_end..a_end + after] {
        out.push_str(&format!(" {line}\n"));
    }
    out
}

/// Whether `line` is a headline: stars, then a space.
fn is_headline(line: &str) -> bool {
    let stars = line.bytes().take_while(|byte| *byte == b'*').count();
    stars > 0 && line.as_bytes().get(stars) == Some(&b' ')
}

/// `org-todo` on the headline at `line` of `file`, to `state`.
fn set_todo_state(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let given = arguments.required("file")?;
    let line = arguments
        .0
        .get("line")
        .and_then(Value::as_u64)
        .filter(|line| *line > 0)
        .ok_or("`line` is required: the line of the entry's headline, from 1")? as usize;
    let state = arguments.text("state").ok_or("`state` is required: a TODO keyword, or \"\" for none")?;
    let state = state.trim();
    let path = editable_file(&given, options, folder)?;
    let text = std::fs::read_to_string(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let name = path.display().to_string();

    let parse = org_syntax::parse(&text, &options.todo_keywords());
    let keywords = parse.todo_keywords();
    let heading = match line {
        1 => 0,
        _ => text
            .match_indices('\n')
            .nth(line - 2)
            .map(|(at, _)| at + 1)
            .filter(|at| *at < text.len())
            .ok_or_else(|| format!("{name} has {} lines", text.lines().count()))?,
    };
    let current = text[heading..].lines().next().unwrap_or("");
    if !is_headline(current) {
        return Err(format!("Line {line} of {name} is not a headline: {current}").into());
    }
    if let Some(expected) = arguments.text("headline").filter(|expected| !expected.trim().is_empty()) {
        if !current.contains(expected.trim()) {
            return Err(format!(
                "Line {line} of {name} is `{current}`, which is not the entry `{}`: the file has changed, \
                 look for the entry again",
                expected.trim()
            )
            .into());
        }
    }
    let target = match state {
        "" => None,
        state if keywords.contains(state) => Some(state.to_string()),
        state => {
            let known: Vec<&str> = keywords.names().collect();
            return Err(format!("{state} is not a TODO keyword of {name}, which has {}", known.join(" ")).into());
        }
    };

    let mut settings = options.edit_settings();
    settings.todo.adapt = options.display().adapt(&text, options.indent_settings().adapt);
    let call = org_core::edit::Call {
        count: None,
        repeated: false,
        now: now(),
        note: arguments.text("note"),
        skip_note: false,
        time_only: false,
    };
    let command = org_core::edit::Command::Todo(org_core::todo::Target::State(target.clone()));
    let outcome = org_core::edit::run(&command, &text, &parse, heading, &call, &settings)?;
    if let Some(wanted) = outcome.note {
        return Err(
            format!("This change asks for a note (Note for {}): give its text as `note`", wanted.purpose).into()
        );
    }
    let message = outcome
        .message
        .clone()
        .or_else(|| crate::server::headline_message(&text, &outcome.text, heading, keywords, false, Some(&target)));
    let mut out = message.map(|message| message + "\n").unwrap_or_default();
    if outcome.text == text {
        out.push_str("Nothing changed.\n");
    } else if arguments.flag("dry_run") {
        out.push_str("Dry run: nothing was written. The change would be:\n");
        out.push_str(&diff(&name, &text, &outcome.text));
    } else {
        write_file(&path, &outcome.text)?;
    }
    let headline = outcome.text.lines().nth(line - 1).unwrap_or("");
    out.push_str(&format!("{name}:{line}: {headline}"));
    Ok(out)
}

/// The answers given to a template's questions.
enum Answers {
    /// In the order the questions come.
    List(Vec<String>),
    /// By prompt.
    Named(serde_json::Map<String, Value>),
}

impl Answers {
    fn new(given: Option<&Value>) -> Result<Answers, Call> {
        Ok(match given {
            None | Some(Value::Null) => Answers::List(Vec::new()),
            Some(Value::Array(items)) => Answers::List(items.iter().map(answer_text).collect()),
            Some(Value::Object(named)) => Answers::Named(named.clone()),
            Some(Value::String(one)) => Answers::List(vec![one.clone()]),
            Some(_) => return Err("`answers` is an object from prompts to answers, or a list of answers".into()),
        })
    }

    /// The answer to the question `asked`-th (from 0), whose prompt is `prompt`.
    fn get(&self, asked: usize, prompt: &str) -> Option<String> {
        match self {
            Answers::List(items) => items.get(asked).cloned(),
            Answers::Named(named) => named
                .get(prompt)
                .or_else(|| named.iter().find(|(key, _)| key.eq_ignore_ascii_case(prompt)).map(|(_, value)| value))
                .map(answer_text),
        }
    }
}

/// An answer as text: tags given as a list are joined with colons.
fn answer_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(answer_text).collect::<Vec<_>>().join(":"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A question's prompt, as the answers name it, and what it wants.
fn question_text(question: &Question) -> (String, String) {
    match question {
        Question::Text { prompt, default, choices } => {
            let mut about = "text".to_string();
            if !default.is_empty() {
                about.push_str(&format!(", default {default}"));
            }
            if !choices.is_empty() {
                about.push_str(&format!(", one of {}", choices.join(", ")));
            }
            (prompt.clone(), about)
        }
        Question::Date { prompt, with_time, .. } => {
            let about = if *with_time { "a date and time" } else { "a date" };
            let about = format!("{about} as Org reads it (2026-10-21, +2d, fri 10:00)");
            (prompt.clone().unwrap_or_else(|| "Date".to_string()), about)
        }
        Question::Tags { prompt, .. } => {
            (prompt.clone().unwrap_or_else(|| "Tags".to_string()), "tags separated by colons".to_string())
        }
        Question::Property { name, default } => {
            let mut about = format!("the value of the property {name}");
            if !default.is_empty() {
                about.push_str(&format!(", default {default}"));
            }
            (name.clone(), about)
        }
        Question::Clipboard { .. } => ("Clipboard/kill value".to_string(), "text".to_string()),
    }
}

/// Why a template could not be filled.
enum Unfilled {
    /// These questions have no answer: their prompts, and what they want.
    Unanswered(Vec<(String, String)>),
    Failed(String),
}

impl From<org_core::Refusal> for Unfilled {
    fn from(refusal: org_core::Refusal) -> Self {
        Unfilled::Failed(refusal.message().to_string())
    }
}

/// A template filled, with the day of a date tree's entry.
struct Template {
    text: String,
    tree_date: org_core::date::Date,
    /// `text` has no `%i`: what is captured goes where `%?` is.
    at_cursor: bool,
}

/// Expand `template` as `org-capture-fill-template` does, with `text` and
/// `link` for `%i` and `%a`, and the questions answered from `answers`:
/// first the date of a tree entry with `:time-prompt`.
fn fill(
    template: &CaptureTemplateOption,
    options: &Options,
    folder: Option<&Path>,
    text: &str,
    link: (Option<String>, Option<String>),
    answers: &Answers,
) -> Result<Template, Unfilled> {
    if template.clock_in == Some(true) {
        return Err(Unfilled::Failed(
            "This template clocks in (`clock_in`), which needs the editor's clock: capture it in the editor."
                .to_string(),
        ));
    }
    let read = options.read_date_settings();
    let now = now();
    let mut asked = 0;
    let mut unanswered = Vec::new();
    let mut default_time = now;
    let mut tree_date = capture::tree_date(now, &read);
    if template.time_prompt == Some(true) {
        let prompt = "Date for tree entry";
        match answers.get(asked, prompt) {
            Some(answer) => {
                default_time = capture::prompted_time(&answer, now, &read).map_err(Unfilled::Failed)?;
                tree_date = default_time.date();
            }
            None => unanswered.push((prompt.to_string(), "a date as Org reads it (2026-10-21, +2d)".to_string())),
        }
        asked += 1;
    }
    let source = crate::server::template_text(template);
    let at_cursor = !source.contains("%i");
    let annotation = match &link {
        (Some(link), Some(description)) if !description.is_empty() => format!("[[{link}][{description}]]"),
        (Some(link), _) => format!("[[{link}]]"),
        (None, _) => String::new(),
    };
    let mut link_props = Vec::new();
    if let (Some(link), description) = &link {
        link_props.push((":link".to_string(), link.clone()));
        link_props.push((":description".to_string(), description.clone().unwrap_or_default()));
    }
    let context = capture::Context {
        now: default_time,
        file: None,
        initial: if at_cursor { String::new() } else { text.to_string() },
        annotation,
        link_props,
        user_name: String::new(),
        clock_heading: String::new(),
        clock_link: String::new(),
        tags_column: options.edit_settings().todo.tags_column,
        clipboard: String::new(),
    };
    // `%[file]`: relative to the folder the server runs in.
    let insert_file = |name: &str| crate::server::capture_file_text(name, folder);
    let mut eval = |_: &str| -> Result<Option<String>, String> { Err(NEEDS_EMACS.to_string()) };
    let mut given: Vec<String> = Vec::new();
    loop {
        let outside = capture::Outside { insert_file: &insert_file, eval: &mut eval };
        let filled = match capture::fill_with(&source, &context, &given, &read, Some(outside)) {
            Ok(filled) => filled,
            // An answer left empty may not do: the questions so far are
            // what is missing.
            Err(_) if !unanswered.is_empty() => return Err(Unfilled::Unanswered(unanswered)),
            Err(refusal) => return Err(refusal.into()),
        };
        match filled {
            Filled::Text(filled) if unanswered.is_empty() => {
                return Ok(Template { text: filled, tree_date, at_cursor });
            }
            Filled::Text(_) => return Err(Unfilled::Unanswered(unanswered)),
            Filled::Ask(question) => {
                let (prompt, about) = question_text(&question);
                match answers.get(asked, &prompt) {
                    Some(answer) => given.push(answer),
                    None => {
                        unanswered.push((prompt, about));
                        given.push(String::new());
                    }
                }
                asked += 1;
                if asked > 100 {
                    return Err(Unfilled::Failed("The template asks too many questions".to_string()));
                }
            }
        }
    }
}

/// `org-capture` with the template of key `template`, filed at once.
fn capture(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let key = arguments.required("template")?;
    let templates = options.capture_templates();
    let template = templates
        .iter()
        .find(|template| template.key == key && !template.is_group())
        .ok_or_else(|| format!("No capture template referred to by \"{key}\" keys"))?;
    let text = arguments.text("text").unwrap_or_default();
    let answers = Answers::new(arguments.0.get("answers"))?;
    let link = (arguments.text("link").filter(|link| !link.is_empty()), arguments.text("description"));
    let filled = match fill(template, options, folder, &text, link, &answers) {
        Ok(filled) => filled,
        Err(Unfilled::Failed(message)) => return Err(message.into()),
        Err(Unfilled::Unanswered(questions)) => {
            let mut message = "The template asks for these, which `answers` does not give (an object from each \
                               prompt to its answer, or a list in order):"
                .to_string();
            for (prompt, about) in questions {
                message.push_str(&format!("\n- {prompt}: {about}"));
            }
            return Err(message.into());
        }
    };
    if filled.at_cursor && !text.is_empty() && !filled.text.contains("%?") {
        return Err(format!("Template \"{key}\" has neither %i nor %?: the text has no place in it").into());
    }

    let (kind, spec) = crate::server::capture_spec(template)?;
    let (path, target) = match spec {
        TargetSpec::File(target) => (target_path(template, options), target),
        TargetSpec::DateTree(path, groups) => {
            (target_path(template, options), capture::Target::DateTree { path, date: filled.tree_date, groups })
        }
        TargetSpec::Id(id) => {
            let todo = options.todo_keywords();
            agenda_paths(options, folder)
                .into_iter()
                .find_map(|path| {
                    let text = std::fs::read_to_string(&path).ok()?;
                    let found = org_core::open::find_id(&org_syntax::parse(&text, &todo), &id)?;
                    Some((path, capture::Target::Heading(found)))
                })
                .ok_or_else(|| format!("Cannot find target ID \"{id}\""))?
        }
        TargetSpec::Clock => return Err("No running clock that could be used as capture target".into()),
        TargetSpec::Function { .. } => return Err(NEEDS_EMACS.into()),
    };
    let before = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Cannot read {}: {error}", path.display()).into()),
    };
    let keywords = org_syntax::parse(&before, &options.todo_keywords()).todo_keywords().clone();
    let properties = capture::Properties {
        prepend: template.prepend.unwrap_or(false),
        empty_lines: template.empty_lines,
        empty_lines_before: template.empty_lines_before,
        empty_lines_after: template.empty_lines_after,
        table_line_pos: template.table_line_pos.clone(),
    };
    let levels = org_core::refile::Settings {
        adapt: options.display().adapt(&before, options.indent_settings().adapt),
        ..options.refile_settings()
    };
    let settings = options.edit_settings();
    let placed =
        capture::place(&before, &keywords, &target, kind, &filled.text, &properties, &levels, &settings.todo.lists)?;
    // What the user would type at `%?`, and `C-c C-c`.
    let mut edited = placed.shown().to_string();
    if filled.at_cursor {
        edited.insert_str(placed.cursor - placed.start, &text);
    }
    let done = placed.finalize(&edited, &keywords, &settings.todo, now());
    let start = placed.begin.min(done.len());
    let heading = start + done[start..].len() - done[start..].trim_start_matches(['\n', ' ', '\t']).len();
    let line = done[..heading].matches('\n').count() + 1;
    let name = path.display().to_string();
    let filed = edited.trim_matches('\n');
    if arguments.flag("dry_run") {
        return Ok(format!(
            "Dry run: nothing was written. It would go in {name} at line {line}:\n{filed}\nThe change would be:\n{}",
            diff(&name, &before, &done)
        ));
    }
    write_file(&path, &done)?;
    Ok(format!("Filed in {name} at line {line}:\n{filed}"))
}

/// The capture templates, with their targets and their questions.
fn capture_templates(options: &Options, folder: Option<&Path>) -> String {
    let templates = options.capture_templates();
    let mut out = String::new();
    for template in &templates {
        if template.is_group() {
            out.push_str(&format!(
                "{}: {} (the templates whose keys start with {0})\n",
                template.key, template.description
            ));
            continue;
        }
        out.push_str(&format!("{}: {}\n", template.key, template.description));
        let kind = template.kind.as_deref().unwrap_or("entry");
        let target = template.target.iter().map(|part| format!("{part:?}")).collect::<Vec<_>>().join(" ");
        let file = match crate::server::capture_spec(template) {
            Ok((_, TargetSpec::File(_) | TargetSpec::DateTree(..))) => {
                format!(", in {}", target_path(template, options).display())
            }
            _ => String::new(),
        };
        out.push_str(&format!("  {kind}, target ({target}){file}\n"));
        out.push_str(&format!("  template: {:?}\n", crate::server::template_text(template)));
        let none = Answers::List(Vec::new());
        match fill(template, options, folder, "", (None, None), &none) {
            Ok(_) => out.push_str("  asks: nothing\n"),
            Err(Unfilled::Unanswered(questions)) => {
                let asked: Vec<String> =
                    questions.iter().map(|(prompt, about)| format!("{prompt} ({about})")).collect();
                out.push_str(&format!("  asks: {}\n", asked.join("; ")));
            }
            Err(Unfilled::Failed(message)) => out.push_str(&format!("  cannot be captured here: {message}\n")),
        }
        match crate::server::capture_spec(template) {
            Ok((_, TargetSpec::Function { .. })) => {
                out.push_str(&format!("  cannot be captured here: {NEEDS_EMACS}\n"))
            }
            Ok((_, TargetSpec::Clock)) => out.push_str("  cannot be captured here: it files under the running clock\n"),
            Err(refusal) => out.push_str(&format!("  cannot be captured here: {}\n", refusal.message())),
            Ok(_) => {}
        }
    }
    if out.is_empty() {
        out.push_str("No capture templates.");
    }
    out.trim_end().to_string()
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
        assert_eq!(
            names,
            ["agenda", "todo_list", "tags_search", "text_search", "custom_agenda", "agenda_files", "capture_templates"]
        );

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

    /// A folder with `files` and a configuration file that sets `config`
    /// besides the folder's `tasks.org` for agenda file and the folder
    /// for `org_directory`.
    struct Setup {
        folder: std::path::PathBuf,
        config: std::path::PathBuf,
    }

    impl Setup {
        fn new(name: &str, config: &str, files: &[(&str, &str)]) -> Setup {
            let folder = std::env::temp_dir().join(format!("org-lsp-mcp-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).unwrap();
            for (file, text) in files {
                std::fs::write(folder.join(file), text).unwrap();
            }
            let path = folder.join("config.toml");
            let agenda = folder.join("tasks.org").display().to_string();
            let directory = folder.display().to_string();
            std::fs::write(
                &path,
                format!("org_agenda_files = [{agenda:?}]\norg_directory = {directory:?}\n{config}\n"),
            )
            .unwrap();
            Setup { folder, config: path }
        }

        fn ask(&self, method: &str, params: Value) -> Value {
            let config = self.config.display().to_string();
            let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
            let line = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
            answer(&line, Some(&self.folder), &env).expect("an answer")
        }

        /// The text a tool answers, and whether it is an error.
        fn call(&self, name: &str, arguments: Value) -> (String, bool) {
            let answer = self.ask("tools/call", json!({ "name": name, "arguments": arguments }));
            let text = answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
            (text, answer["result"]["isError"] == true)
        }

        fn read(&self, file: &str) -> String {
            std::fs::read_to_string(self.folder.join(file)).unwrap_or_default()
        }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    const TASKS: &str = "#+title: Tasks\n* Inbox\n* TODO Write the report :work:\n\
                         * TODO Water the plants\nSCHEDULED: <2026-10-21 Wed +1w>\n";

    #[test]
    fn edits_are_off_unless_mcp_edits_is_on() {
        let setup = Setup::new("off", "", &[("tasks.org", TASKS)]);
        let list = setup.ask("tools/list", json!({}));
        let names: Vec<&str> =
            list["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        assert!(!names.contains(&"set_todo_state") && !names.contains(&"capture"), "{names:?}");
        let (text, error) = setup.call("set_todo_state", json!({ "file": "tasks.org", "line": 3, "state": "DONE" }));
        assert!(error && text.contains("mcp_edits = true"), "{text}");
        let (text, error) = setup.call("capture", json!({ "template": "t", "text": "x" }));
        assert!(error && text.contains("mcp_edits = true"), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);

        let on = Setup::new("on", "mcp_edits = true", &[("tasks.org", TASKS)]);
        let list = on.ask("tools/list", json!({}));
        let tools = list["result"]["tools"].as_array().unwrap();
        let edit = tools.iter().find(|tool| tool["name"] == "set_todo_state").expect("set_todo_state");
        assert_eq!(edit["annotations"]["readOnlyHint"], false);
        assert_eq!(edit["annotations"]["destructiveHint"], false);
        assert_eq!(edit["annotations"]["idempotentHint"], false);
        assert!(tools.iter().any(|tool| tool["name"] == "capture"));
    }

    #[test]
    fn views_say_where_each_entry_is() {
        let setup = Setup::new("references", "", &[("tasks.org", TASKS)]);
        let (todo, _) = setup.call("todo_list", json!({}));
        let line = todo.lines().find(|line| line.contains("Write the report")).expect("the report");
        assert!(line.ends_with("  [tasks.org:3]"), "{todo}");
        let line = todo.lines().find(|line| line.contains("Water the plants")).expect("the plants");
        assert!(line.ends_with("  [tasks.org:4]"), "{todo}");
        assert!(!todo.lines().next().unwrap_or_default().contains('['), "{todo}");
    }

    #[test]
    fn sets_todo_states_as_org_todo_does() {
        let config = "mcp_edits = true\norg_log_done = \"time\"\n\
                      org_todo_keywords = [\"TODO\", \"WAIT(w@)\", \"|\", \"DONE(d)\"]\n";
        let setup = Setup::new("todo", config, &[("tasks.org", TASKS), ("other.org", "* TODO Elsewhere\n")]);
        // Done, with a CLOSED stamp.
        let arguments = json!({ "file": "tasks.org", "line": 3, "state": "DONE", "headline": "Write the report" });
        let (text, error) = setup.call("set_todo_state", arguments);
        assert!(!error, "{text}");
        assert!(text.contains(":3: * DONE Write the report ") && text.ends_with(":work:"), "{text}");
        let written = setup.read("tasks.org");
        assert!(written.contains(":work:\nCLOSED: ["), "{written}");

        // The line holds another entry now: refused.
        let arguments = json!({ "file": "tasks.org", "line": 5, "state": "DONE", "headline": "Write the report" });
        let (text, error) = setup.call("set_todo_state", arguments);
        assert!(error && text.contains("not the entry"), "{text}");

        // A state that wants a note: refused without one, logged with one.
        let path = setup.folder.join("tasks.org").display().to_string();
        let (text, error) = setup.call("set_todo_state", json!({ "file": path, "line": 5, "state": "WAIT" }));
        assert!(error && text.contains("`note`"), "{text}");
        assert_eq!(setup.read("tasks.org"), written);
        let arguments = json!({ "file": path, "line": 5, "state": "WAIT", "note": "Until it rains" });
        let (text, error) = setup.call("set_todo_state", arguments);
        assert!(!error, "{text}");
        let noted = setup.read("tasks.org");
        assert!(noted.contains("- State \"WAIT\"       from \"TODO\"       ["), "{noted}");
        assert!(noted.contains("\\\\\n  Until it rains"), "{noted}");

        // Not a keyword, not a headline, not a file the tools may change.
        let (text, error) = setup.call("set_todo_state", json!({ "file": "tasks.org", "line": 5, "state": "NOPE" }));
        assert!(error && text.contains("not a TODO keyword"), "{text}");
        let (text, error) = setup.call("set_todo_state", json!({ "file": "tasks.org", "line": 1, "state": "DONE" }));
        assert!(error && text.contains("not a headline"), "{text}");
        let (text, error) = setup.call("set_todo_state", json!({ "file": "other.org", "line": 1, "state": "DONE" }));
        assert!(error && text.contains("neither an agenda file"), "{text}");
        assert_eq!(setup.read("other.org"), "* TODO Elsewhere\n");
    }

    #[test]
    fn a_repeating_task_moves_its_date() {
        let setup = Setup::new("repeat", "mcp_edits = true", &[("tasks.org", TASKS)]);
        // A dry run writes nothing, and shows the change.
        let arguments = json!({ "file": "tasks.org", "line": 4, "state": "DONE", "dry_run": true });
        let (text, error) = setup.call("set_todo_state", arguments);
        assert!(!error, "{text}");
        assert!(text.contains("Entry repeats: SCHEDULED: <2026-10-28 Wed +1w>"), "{text}");
        assert!(text.contains("-SCHEDULED: <2026-10-21 Wed +1w>\n"), "{text}");
        assert!(text.contains("+SCHEDULED: <2026-10-28 Wed +1w>\n"), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);
        let (text, error) = setup.call("set_todo_state", json!({ "file": "tasks.org", "line": 4, "state": "DONE" }));
        assert!(!error, "{text}");
        assert!(text.ends_with(":4: * TODO Water the plants"), "{text}");
        let written = setup.read("tasks.org");
        assert!(written.contains("* TODO Water the plants\nSCHEDULED: <2026-10-28 Wed +1w>"), "{written}");
    }

    #[test]
    fn a_blocked_task_says_so() {
        let tasks = "* TODO Parent\n** TODO Child\n";
        let config = "mcp_edits = true\norg_enforce_todo_dependencies = true";
        let setup = Setup::new("blocked", config, &[("tasks.org", tasks)]);
        let (text, error) = setup.call("set_todo_state", json!({ "file": "tasks.org", "line": 1, "state": "DONE" }));
        assert!(error && text.starts_with("TODO state change from TODO to DONE blocked"), "{text}");
        assert_eq!(setup.read("tasks.org"), tasks);
    }

    #[test]
    fn captures_with_the_templates() {
        let config = r#"mcp_edits = true
[[org_capture_templates]]
key = "t"
description = "Task"
target = ["file+headline", "tasks.org", "Inbox"]
template = "* TODO %?"

[[org_capture_templates]]
key = "m"
description = "Meeting"
target = ["file", "meetings.org"]
template = "* %^{Who} on %^{Topic|plans}\n%i"
"#;
        let setup = Setup::new("capture", config, &[("tasks.org", TASKS)]);
        let (templates, _) = setup.call("capture_templates", json!({}));
        assert!(templates.contains("t: Task"), "{templates}");
        assert!(templates.contains("asks: Who (text); Topic (text, default plans)"), "{templates}");

        let (text, error) = setup.call("capture", json!({ "template": "t", "text": "Buy milk" }));
        assert!(!error, "{text}");
        assert!(text.ends_with("tasks.org at line 3:\n** TODO Buy milk"), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.starts_with("#+title: Tasks\n* Inbox\n** TODO Buy milk\n* TODO Write"), "{tasks}");

        // The questions not answered, all of them.
        let (text, error) = setup.call("capture", json!({ "template": "m", "text": "Notes" }));
        assert!(error && text.contains("- Who: text") && text.contains("- Topic: text, default plans"), "{text}");
        assert!(!setup.folder.join("meetings.org").exists());
        let answers = json!({ "Who": "Ana" });
        let (text, error) = setup.call("capture", json!({ "template": "m", "text": "Notes", "answers": answers }));
        assert!(error && !text.contains("Who") && text.contains("Topic"), "{text}");
        let answers = json!({ "Who": "Ana", "Topic": "" });
        let (text, error) = setup.call("capture", json!({ "template": "m", "text": "Notes", "answers": answers }));
        assert!(!error, "{text}");
        assert_eq!(setup.read("meetings.org"), "* Ana on plans\nNotes\n");
        let arguments = json!({ "template": "m", "answers": ["Ben", "budget"], "dry_run": true });
        let (text, error) = setup.call("capture", arguments);
        assert!(!error && text.contains("+* Ben on budget"), "{text}");
        assert_eq!(setup.read("meetings.org"), "* Ana on plans\nNotes\n");

        // A capture target is a file set_todo_state may change.
        let (text, error) = setup.call("set_todo_state", json!({ "file": "meetings.org", "line": 1, "state": "TODO" }));
        assert!(!error, "{text}");
        assert_eq!(setup.read("meetings.org"), "* TODO Ana on plans\nNotes\n");
        let (text, error) = setup.call("capture", json!({ "template": "x" }));
        assert!(error && text == "No capture template referred to by \"x\" keys", "{text}");
    }

    #[test]
    fn a_template_that_needs_emacs_is_refused() {
        let config = r#"mcp_edits = true
[[org_capture_templates]]
key = "l"
description = "Lisp"
target = ["file", "notes.org"]
template = "* %(format-time-string \"%Y\") %?"
"#;
        let setup = Setup::new("emacs", config, &[("tasks.org", TASKS)]);
        let (text, error) = setup.call("capture", json!({ "template": "l", "text": "x" }));
        assert!(error && text.contains("needs Emacs"), "{text}");
        assert!(!setup.folder.join("notes.org").exists());
    }
}
