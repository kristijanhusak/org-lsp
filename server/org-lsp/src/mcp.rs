//! `org-lsp mcp`: a Model Context Protocol server over standard input and
//! output, for AI assistants to read the agenda as Org builds it, and with
//! `mcp_edits` to change TODO states, capture, refile, and change entries
//! where they are, archive or delete them ([`entry`]). Its prompts
//! ([`prompts`]) give the assistant the data of a task, such as planning
//! the day, with what to do with it.
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
//! [`org_core::capture`], `org-refile` with [`org_core::refile`]), and
//! write the files directly: no editor is asked, and no Emacs is started.
//!
//! Messages are JSON-RPC, one per line, as MCP's stdio transport has them.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use org_core::capture::{self, Filled, Question};
use org_core::date::DateTime;
use serde_json::{json, Value};

use crate::config::{CaptureTemplateOption, Options};
use crate::server::TargetSpec;

mod entry;
mod prompts;

pub const USAGE: &str = "\
Usage: org-lsp mcp

Run a Model Context Protocol server over standard input and output, which
gives AI assistants the agenda, the TODO list and searches of the agenda
files of the shared configuration file (`org-lsp config`). It only reads,
unless that file sets `mcp_edits = true`: then it also changes TODO states,
dates, tags, properties, priorities and efforts, adds notes, captures,
refiles, archives and deletes entries, writing the files.";

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
    // What the configuration file has wrong, in the client's log.
    for problem in crate::config::load(None, &|name| std::env::var(name).ok()).problems {
        eprintln!("org-lsp mcp: {problem}");
    }
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
                "capabilities": { "tools": {}, "prompts": {} },
                "serverInfo": { "name": "org-lsp", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "The user's Org mode agenda, read as Emacs's Org builds it. \
                    Dates are YYYY-MM-DD. The views are text as Org's agenda shows them: \
                    category, time, TODO keyword, headline and tags on each line, then where \
                    the entry's headline is, [file:line], which the edit tools take.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools(&crate::config::load(None, env).options) }),
        "prompts/list" => json!({ "prompts": prompts::list() }),
        "prompts/get" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match prompts::get(name, &arguments, folder, env) {
                Ok(prompt) => prompt,
                Err(message) => return Some(failure(id, -32602, &message)),
            }
        }
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
        tool(
            "refile_targets",
            "Refile targets",
            "The places an entry can be refiled to (org_refile_targets), each named as org-refile \
             offers it, which refile takes, then where it is, [file:line].",
            json!({
                "file": {
                    "type": "string",
                    "description": "The file of the entry to refile, whose headlines are the targets that \
                                    org_refile_targets says are the current file's, named as from there",
                },
                "filter": { "type": "string", "description": "Only the targets whose name has this text" },
            }),
            &[],
        ),
    ];
    tools.extend(reading_tools());
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
    edit(
        "refile",
        "Refile",
        "Move an entry and its subtree under another headline (org-refile), its levels adjusted, \
         logged as org_log_refile says; with copy, keep it where it is too (org-refile-copy). Writes \
         the files. The entry is the [file:line] a view gives; the target, a name refile_targets gives."
            .to_string(),
        json!({
            "file": { "type": "string", "description": "The entry's file: its path, or its name in the views" },
            "line": { "type": "integer", "minimum": 1, "description": "The line of the entry's headline, from 1" },
            "target": {
                "type": "string",
                "description": "A target as refile_targets names it, or its [file:line], or a file alone for its \
                                top level when the targets have files; with \
                                org_refile_allow_creating_parent_nodes, a target, / and a new headline",
            },
            "copy": { "type": "boolean", "description": "Keep the entry where it is too (org-refile-copy)" },
            "note": { "type": "string", "description": "The note, when org_log_refile asks for one" },
            "create": {
                "type": "boolean",
                "description": "Make the new headline when org_refile_allow_creating_parent_nodes is confirm",
            },
            "headline": {
                "type": "string",
                "description": "Text the headline has, such as its title: refused if the line no longer has it",
            },
            "dry_run": dry_run,
        }),
        &["file", "line", "target"],
    );
    tools.extend(entry::tools());
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

    /// The `line` of an entry's headline.
    fn line(&self) -> Result<usize, Call> {
        let line = self.0.get("line").and_then(Value::as_u64).filter(|line| *line > 0);
        Ok(line.ok_or("`line` is required: the line of the entry's headline, from 1")? as usize)
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
    if let Some(result) = reading(name, &arguments, folder, env) {
        return result;
    }
    match name {
        "capture_templates" => Ok(capture_templates(&crate::config::load(None, env).options, folder)),
        "refile_targets" => refile_targets(&arguments, &crate::config::load(None, env).options, folder),
        "set_todo_state" | "capture" | "refile" => {
            let options = crate::config::load(None, env).options;
            if options.mcp_edits != Some(true) {
                return Err(EDITS_OFF.into());
            }
            match name {
                "capture" => capture(&arguments, &options, folder),
                "refile" => refile(&arguments, &options, folder),
                _ => set_todo_state(&arguments, &options, folder),
            }
        }
        name if entry::NAMES.contains(&name) => {
            let options = crate::config::load(None, env).options;
            if options.mcp_edits != Some(true) {
                return Err(EDITS_OFF.into());
            }
            entry::call(name, &arguments, &options, folder)
        }
        _ => view(name, &arguments, folder, env),
    }
}

/// The files the agenda is built from, as the server finds them.
pub(crate) fn agenda_paths(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    agenda_paths_at(options, folder, now())
}

/// [`agenda_paths`] as they are at `now`.
pub(crate) fn agenda_paths_at(options: &Options, folder: Option<&Path>, now: DateTime) -> Vec<PathBuf> {
    let mut paths = workspace(options, folder).files();
    // `org-journal-enable-agenda-integration`: the journal's files of
    // today and the days to come in place of the others, as the server has.
    if let Some(journal) = options.journal() {
        if let Some((directory, names)) = crate::server::journal_agenda_files(&journal, now) {
            paths.retain(|path| !crate::server::is_journal(path, &journal));
            paths.extend(names.iter().map(|name| directory.join(name)));
        }
    }
    paths
}

/// The files `org_agenda_files` names that are not there.
pub(crate) fn missing_agenda_files(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    workspace(options, folder).missing()
}

/// The agenda files and the folder the server runs in, as the server
/// looks for files there.
fn workspace(options: &Options, folder: Option<&Path>) -> crate::workspace::Workspace {
    let mut workspace =
        crate::workspace::Workspace::new(options.agenda_files.clone(), folder.into_iter().map(Into::into).collect());
    workspace.file_regexp =
        options.agenda_file_regexp.as_deref().and_then(org_core::agenda::matcher::emacs_regex_with_case);
    workspace
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
    let view: Vec<String> = match name {
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
    build_view(view, folder, env)
}

/// The view `org-lsp agenda` builds with the arguments `view` from the
/// agenda files, with where each entry's headline is; no arguments for the
/// list of the agenda files.
fn build_view(
    mut view: Vec<String>,
    folder: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Call> {
    let loaded = crate::config::load(None, env);
    let files: Vec<String> =
        agenda_paths(&loaded.options, folder).iter().map(|path| path.display().to_string()).collect();
    if view.is_empty() {
        return Ok(if files.is_empty() { no_agenda_files(&loaded, folder) } else { files.join("\n") });
    }
    if files.is_empty() {
        return Err(no_agenda_files(&loaded, folder).into());
    }
    view.extend(["--options".into(), loaded.readable.to_string()]);
    view.extend(files.iter().cloned());
    let lines = crate::cli::agenda_lines(&view)?;
    Ok(with_references(lines, &files))
}

/// Why there are no agenda files: what `org_agenda_files` names that is
/// not there, and what the configuration file has wrong.
pub(crate) fn no_agenda_files(loaded: &crate::config::Loaded, folder: Option<&Path>) -> String {
    let missing = missing_agenda_files(&loaded.options, folder);
    let mut out = if loaded.options.agenda_files.is_empty() {
        "No agenda files: set org_agenda_files in the configuration file (`org-lsp config` shows it).".to_string()
    } else if missing.is_empty() {
        "No agenda files: org_agenda_files names no Org file.".to_string()
    } else {
        let missing: Vec<String> = missing.iter().map(|path| path.display().to_string()).collect();
        format!("No agenda files: these are not there: {}", missing.join(", "))
    };
    for problem in &loaded.problems {
        out.push_str(&format!("\nThe configuration: {problem}"));
    }
    out
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

/// What [`editable_files`] are, in a refusal.
const EDITABLE: &str = "neither an agenda file nor the target of a capture template";

/// The file a template files into.
fn target_path(template: &CaptureTemplateOption, options: &Options) -> PathBuf {
    let path = template.target.get(1).map(String::as_str).unwrap_or("");
    crate::server::capture_path(path, &options.org_directory(), &options.default_notes_file())
}

/// A path as the file system has it, to tell whether two name one file.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf())
}

/// The file `given` names, among `files`, those a tool may change: by its
/// path (`~/` the home directory, relative to the folder the server runs
/// in), or by its name as the views give it.
/// `kinds` says which files those are, when `given` is none of them.
fn editable_file(given: &str, files: &[PathBuf], kinds: &str, folder: Option<&Path>) -> Result<PathBuf, Call> {
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
        // A file listed twice (an agenda file that a capture template or
        // `org_refile_targets` names too) is one file.
        let mut seen = std::collections::HashSet::new();
        let named: Vec<&PathBuf> = files
            .iter()
            .filter(|file| file.file_name().is_some_and(|name| name == given))
            .filter(|file| seen.insert(canonical(file)))
            .collect();
        match named[..] {
            [one] => return Ok(one.clone()),
            [_, _, ..] => {
                let paths: Vec<String> = named.iter().map(|file| file.display().to_string()).collect();
                return Err(format!("More than one file is named {given}: give its path ({})", paths.join(", ")).into());
            }
            [] => {}
        }
    }
    Err(format!("{given} is {kinds}: org-lsp mcp changes only those").into())
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

/// The start of `line` (from 1) of `text`, the file `name`, which is a
/// headline, and one with the text of the `headline` argument when that is
/// given: else the file has changed since the line was taken.
fn headline_at_line(text: &str, line: usize, name: &str, arguments: &Arguments) -> Result<usize, Call> {
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
    Ok(heading)
}

/// `org-todo` on the headline at `line` of `file`, to `state`.
fn set_todo_state(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let given = arguments.required("file")?;
    let line = arguments.line()?;
    let state = arguments.text("state").ok_or("`state` is required: a TODO keyword, or \"\" for none")?;
    let state = state.trim();
    let path = editable_file(&given, &editable_files(options, folder), EDITABLE, folder)?;
    let text = std::fs::read_to_string(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let name = path.display().to_string();

    let parse = org_syntax::parse(&text, &options.todo_keywords());
    let keywords = parse.todo_keywords();
    let heading = headline_at_line(&text, line, &name, arguments)?;
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

// ----- refile

/// What the refile tools answer when `org_refile_target_verify_function`
/// is set: Emacs would call it at each target.
const VERIFY_NEEDS_EMACS: &str = "org_refile_target_verify_function is set, which needs Emacs, which org-lsp mcp \
                                  does not start: refile in the editor.";

/// A file refile targets are taken from, read.
struct TargetFile {
    path: PathBuf,
    text: String,
    keywords: org_syntax::TodoKeywords,
    title: Option<String>,
    /// Its name, without directories.
    name: String,
    /// Its path, as Org shows it.
    full: String,
}

impl TargetFile {
    fn read(path: &Path, options: &Options) -> Option<TargetFile> {
        let text = std::fs::read_to_string(path).ok()?;
        let parse = org_syntax::parse(&text, &options.todo_keywords());
        Some(TargetFile {
            path: path.to_path_buf(),
            keywords: parse.todo_keywords().clone(),
            title: org_core::refile::file_title(&parse),
            name: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
            full: path.display().to_string(),
            text,
        })
    }

    fn file(&self) -> org_core::refile::File<'_> {
        org_core::refile::File {
            text: &self.text,
            keywords: &self.keywords,
            name: &self.name,
            path: &self.full,
            title: self.title.as_deref(),
        }
    }
}

/// The selections of `org_refile_targets`, each with the files it looks
/// in (indexes into the files read, `source` first).
type Selections = Vec<(org_core::refile::Select, Vec<usize>)>;

/// The files of `org_refile_targets`, `source` (the file refiled from)
/// first, and which of them each entry of it looks in, as the server
/// gathers them for `org-refile`. Files that cannot be read have no
/// targets. Without a source, `current` stands for every agenda file.
fn refile_files(options: &Options, folder: Option<&Path>, source: Option<&Path>) -> (Vec<TargetFile>, Selections) {
    use crate::config::RefileFiles;
    let mut files: Vec<TargetFile> = Vec::new();
    let mut canonicals: Vec<PathBuf> = Vec::new();
    let mut add = |path: &Path, files: &mut Vec<TargetFile>| -> Option<usize> {
        let wanted = canonical(path);
        if let Some(index) = canonicals.iter().position(|known| *known == wanted) {
            return Some(index);
        }
        let read = TargetFile::read(path, options)?;
        canonicals.push(wanted);
        files.push(read);
        Some(files.len() - 1)
    };
    if let Some(source) = source {
        add(source, &mut files);
    }
    let workspace = workspace(options, folder);
    let agenda = agenda_paths(options, folder);
    let mut selections = Vec::new();
    for target in options.refile_targets() {
        let found: Vec<PathBuf> = match &target.files {
            RefileFiles::Current => source.map_or_else(|| agenda.clone(), |source| vec![source.to_path_buf()]),
            RefileFiles::Agenda => agenda.clone(),
            RefileFiles::Paths(patterns) => patterns.iter().flat_map(|pattern| workspace.org_files(pattern)).collect(),
            // `org-id-files`: the agenda files, `org-id-extra-files` and
            // those `org-id-locations-file` lists, with IDs.
            RefileFiles::IdFiles => {
                let lookup = options.id_lookup();
                let mut paths = Vec::new();
                if lookup.track_globally {
                    paths.extend(agenda.iter().cloned());
                    for pattern in lookup.extra_files.iter().filter(|extra| *extra != "agenda-archives") {
                        paths.extend(workspace.org_files(pattern));
                    }
                    if let Some(file) = &lookup.locations_file {
                        paths.extend(crate::server::id_location_files(file));
                    }
                }
                paths
                    .into_iter()
                    .filter(|path| std::fs::read_to_string(path).is_ok_and(|text| org_core::link::has_ids(&text)))
                    .collect()
            }
        };
        let indexes = found.iter().filter_map(|path| add(path, &mut files)).collect();
        selections.push((target.select, indexes));
    }
    (files, selections)
}

/// The files a refile may change: the agenda files, the targets of the
/// capture templates (as the other edit tools), and the files of
/// `org_refile_targets`.
fn refile_editable(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut files = editable_files(options, folder);
    files.extend(refile_files(options, folder, None).0.into_iter().map(|file| file.path));
    files
}

/// What [`refile_editable`] are, in a refusal.
const REFILE_EDITABLE: &str =
    "neither an agenda file, nor the target of a capture template, nor a file of org_refile_targets";

/// Where a target is, as the views say where entries are: `[file:line]`,
/// or `[file]` for a file's top level.
fn target_reference(files: &[TargetFile], file: usize, offset: Option<usize>) -> String {
    let paths: Vec<String> = files.iter().map(|file| file.full.clone()).collect();
    let label = file_label(&files[file].full, &paths);
    match offset {
        Some(at) => format!("[{label}:{}]", files[file].text[..at].matches('\n').count() + 1),
        None => format!("[{label}]"),
    }
}

/// Whether `org_refile_target_verify_function` is set.
fn verifies(options: &Options) -> bool {
    options.refile_target_verify_function.as_deref().is_some_and(|name| !name.trim().is_empty())
}

/// `org-refile-get-targets`: the targets, each named as `org-refile`
/// offers it, and where it is.
fn refile_targets(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    if verifies(options) {
        return Err(VERIFY_NEEDS_EMACS.into());
    }
    let source = match arguments.text("file").filter(|file| !file.trim().is_empty()) {
        Some(given) => Some(editable_file(&given, &refile_editable(options, folder), REFILE_EDITABLE, folder)?),
        None => None,
    };
    let (files, selections) = refile_files(options, folder, source.as_deref());
    if let (Some(source), true) = (&source, files.is_empty()) {
        return Err(format!("Cannot read {}", source.display()).into());
    }
    let readable: Vec<org_core::refile::File> = files.iter().map(TargetFile::file).collect();
    let current = source.is_some().then_some(0);
    let all = org_core::refile::all_targets(&selections, &readable, current, options.refile_outline_path());
    if all.is_empty() {
        return Err("No refile targets".into());
    }
    let filter = arguments.text("filter").filter(|filter| !filter.is_empty());
    let lowered = filter.as_ref().map(|filter| filter.to_lowercase());
    let mut out = String::new();
    let mut seen: Vec<&str> = Vec::new();
    for (file, target) in &all {
        if seen.contains(&target.name.as_str()) {
            continue;
        }
        seen.push(&target.name);
        if lowered.as_ref().is_some_and(|filter| !target.name.to_lowercase().contains(filter)) {
            continue;
        }
        out.push_str(&format!("{}  {}\n", target.name, target_reference(&files, *file, target.offset)));
    }
    if out.is_empty() {
        return Ok(format!("No refile target has \"{}\".", filter.unwrap_or_default()));
    }
    Ok(out.trim_end().to_string())
}

/// The target `given` is, among `all`: by its name as `org-refile` offers
/// it from the file refiled from (`files[0]`), or as `refile_targets`
/// gives it without a file (`unnamed`, the same targets); else by the
/// `file:line` of a target, or a file alone for its top level.
fn find_target<'a>(
    given: &str,
    all: &'a [(usize, org_core::refile::Target)],
    unnamed: &'a [(usize, org_core::refile::Target)],
    files: &[TargetFile],
    folder: Option<&Path>,
) -> Result<Option<&'a (usize, org_core::refile::Target)>, Call> {
    if let Some(found) = org_core::refile::find(all, given).or_else(|| org_core::refile::find(unnamed, given)) {
        return Ok(Some(found));
    }
    // `[file:line]`, as a view or `refile_targets` gives it.
    let reference = given.trim().trim_start_matches('[').trim_end_matches(']');
    let (file, line) = match reference.rsplit_once(':') {
        Some((file, line)) if !file.is_empty() && line.parse::<usize>().is_ok_and(|line| line > 0) => {
            (file, line.parse::<usize>().ok())
        }
        _ => (reference, None),
    };
    let paths: Vec<PathBuf> = files.iter().map(|file| file.path.clone()).collect();
    let labels: Vec<String> = files.iter().map(|file| file.full.clone()).collect();
    let index = files.iter().position(|known| file_label(&known.full, &labels) == file).or_else(|| {
        let path = editable_file(file, &paths, "", folder).ok()?;
        paths.iter().position(|known| *known == path)
    });
    let Some(index) = index else { return Ok(None) };
    let offset = match line {
        Some(line) => {
            let text = &files[index].text;
            let at = match line {
                1 => Some(0),
                _ => text.match_indices('\n').nth(line - 2).map(|(at, _)| at + 1).filter(|at| *at < text.len()),
            };
            Some(at.ok_or_else(|| format!("{file} has {} lines", text.lines().count()))?)
        }
        None => None,
    };
    match (all.iter().find(|(at, target)| *at == index && target.offset == offset), offset) {
        (Some(found), _) => Ok(Some(found)),
        (None, Some(_)) => {
            Err(format!("{reference} is not a refile target (org_refile_targets): refile_targets lists them").into())
        }
        (None, None) => Err(format!(
            "{file} is not a refile target: a file is one, for its top level, when org_refile_use_outline_path \
             is \"file\", \"full-file-path\" or \"title\""
        )
        .into()),
    }
}

/// `org-refile` (`org-refile-copy` with `copy`) of the entry at `line` of
/// `file`, to `target`.
fn refile(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    use org_core::refile::{self, NewNodes};
    let given = arguments.required("file")?;
    let line = arguments.line()?;
    let wanted = arguments.required("target")?;
    if verifies(options) {
        return Err(VERIFY_NEEDS_EMACS.into());
    }
    let path = editable_file(&given, &refile_editable(options, folder), REFILE_EDITABLE, folder)?;
    let (files, selections) = refile_files(options, folder, Some(&path));
    if files.is_empty() {
        return Err(format!("Cannot read {}", path.display()).into());
    }
    let heading = headline_at_line(&files[0].text, line, &files[0].full, arguments)?;
    let readable: Vec<refile::File> = files.iter().map(TargetFile::file).collect();
    let outline_path = options.refile_outline_path();
    let all = refile::all_targets(&selections, &readable, Some(0), outline_path);
    if all.is_empty() {
        return Err("No refile targets".into());
    }
    let unnamed = refile::all_targets(&selections, &readable, None, outline_path);
    let copy = arguments.flag("copy");
    let action = if copy { "Copy" } else { "Refile" };

    // `org-refile-allow-creating-parent-nodes`: a target named after an
    // existing one, a `/` and a new headline is made, in the text it goes in.
    let mut made: Option<(usize, String)> = None;
    let (target_file, target_name, target_offset) = match find_target(&wanted, &all, &unnamed, &files, folder)? {
        Some((file, target)) => (*file, target.name.clone(), target.offset),
        None => {
            let new_nodes = options.refile_new_nodes();
            let found = refile::new_node(&all, &wanted).or_else(|| refile::new_node(&unnamed, &wanted));
            match found.filter(|_| new_nodes != NewNodes::No) {
                Some(((file, parent), child)) => {
                    if new_nodes == NewNodes::Confirm && !arguments.flag("create") {
                        return Err(format!(
                            "Create new node \"{child}\"? org_refile_allow_creating_parent_nodes is \"confirm\": \
                             give `create: true` to make it"
                        )
                        .into());
                    }
                    let (text, heading) = refile::new_child(&files[*file].text, parent.offset, &child);
                    made = Some((*file, text));
                    (*file, format!("{}/{child}", parent.name), Some(heading))
                }
                None => return Err("Invalid target location".into()),
            }
        }
    };
    let made_in = made.as_ref().map(|(file, _)| *file);
    // The entry, after a new target made above it in its own file.
    let heading = match (&made, target_offset) {
        (Some((0, text)), Some(new)) if new - 1 <= heading => heading + text.len() - files[0].text.len(),
        _ => heading,
    };
    let made_text = made.map(|(_, text)| text);
    let source = if made_in == Some(0) { made_text.as_deref().expect("made") } else { files[0].text.as_str() };
    let other = (target_file != 0).then(|| match made_in {
        Some(file) if file != 0 => made_text.as_deref().expect("made"),
        _ => files[target_file].text.as_str(),
    });
    // The target file's `org-adapt-indentation`, as Emacs pastes there.
    let settings = refile::Settings {
        adapt: options.display().adapt(other.unwrap_or(source), options.indent_settings().adapt),
        ..options.refile_settings()
    };
    let mut refiled = refile::refile(source, heading, None, other, target_offset, &settings, copy)?;

    // `org-log-refile`: the time, or a note, logged in the refiled entry.
    let todo = options.edit_settings().todo;
    if let Some(at) = refiled.pasted {
        let text = refiled.target.as_mut().unwrap_or(&mut refiled.source);
        let note = match org_core::todo::refile_logging(&todo, text) {
            Some(org_syntax::Logging::Note) => match arguments.text("note").filter(|note| !note.trim().is_empty()) {
                Some(note) => Some(note),
                None => return Err("Refiling asks for a note (Note for refiling): give its text as `note`".into()),
            },
            Some(org_syntax::Logging::Time) => Some(String::new()),
            None => None,
        };
        if let Some(note) = note {
            let keywords = org_syntax::parse(text, &options.todo_keywords()).todo_keywords().clone();
            *text = org_core::todo::log_refile(text, &keywords, at, &todo, now(), &note)?;
        }
    }

    let target_path = &files[target_file].full;
    let mut out = format!("{action} to \"{target_name}\" in file {target_path}: done\n");
    // The target first: a failure between the two writes leaves the entry
    // in both files, never in none.
    let mut changes: Vec<(&TargetFile, &str)> = Vec::new();
    if let Some(changed) = &refiled.target {
        changes.push((&files[target_file], changed));
    }
    changes.push((&files[0], &refiled.source));
    changes.retain(|(file, text)| file.text != *text);
    if changes.is_empty() {
        out.push_str("Nothing changed.\n");
    } else if arguments.flag("dry_run") {
        out.push_str("Dry run: nothing was written. The change would be:\n");
        for (file, text) in &changes {
            out.push_str(&diff(&file.full, &file.text, text));
        }
    } else {
        for (file, text) in &changes {
            write_file(&file.path, text)?;
        }
    }
    // Where the entry is now.
    if let Some(at) = refiled.pasted {
        let text = refiled.target.as_ref().unwrap_or(&refiled.source);
        let line = text[..at].matches('\n').count() + 1;
        let headline = text[at..].lines().next().unwrap_or("");
        out.push_str(&format!("{target_path}:{line}: {headline}"));
    }
    Ok(out.trim_end().to_string())
}

// ----- reading: an entry, the clock report, org-roam and org-journal

/// What the org-roam tools answer while org-roam is off.
const ROAM_OFF: &str = "org-roam is not enabled: set `org_roam_enabled = true` in the configuration file \
                        (`org-lsp config` shows which one) to use it.";

/// What the org-journal tool answers while org-journal is off.
const JOURNAL_OFF: &str = "org-journal is not enabled: set `org_journal_enabled = true` in the configuration file \
                           (`org-lsp config` shows which one) to use it.";

/// What [`readable_files`] are, in a refusal.
const READABLE: &str = "neither an agenda file, nor a journal file, nor an org-roam file";

/// The tools that read an entry, the clock report, org-roam's nodes and
/// org-journal's entries: listed whatever the options, as the views are.
fn reading_tools() -> Vec<Value> {
    let tool = |name: &str, title: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        })
    };
    let date = |about: &str| json!({ "type": "string", "description": format!("YYYY-MM-DD: {about}") });
    vec![
        tool(
            "entry",
            "Entry",
            "The whole text of an entry, the [file:line] a view gives: its subtree, with its outline \
             path, planning (SCHEDULED, DEADLINE, CLOSED), tags and properties (its own and those it \
             inherits, as the inheritance options say) and the time clocked in it.",
            json!({
                "file": { "type": "string", "description": "The entry's file: its path, or its name in the views" },
                "line": { "type": "integer", "minimum": 1, "description": "The line of the entry's headline, from 1" },
                "headline": {
                    "type": "string",
                    "description": "Text the headline has, such as its title: refused if the line no longer has it",
                },
                "subtree": {
                    "type": "boolean",
                    "description": "false for the entry alone, up to its first child; the default is its whole subtree",
                },
            }),
            &["file", "line"],
        ),
        tool(
            "clock_report",
            "Clock report",
            "The clock table of the agenda files (org-clock-report), with the user's clock table \
             options, as Org writes it: the time clocked under each headline, over a span of days.",
            json!({
                "span": {
                    "type": "string",
                    "description": "today, yesterday, thisweek, lastweek, thismonth, lastmonth, thisyear, \
                                    lastyear, or a period such as 2026-10, 2026-W42 or 2026-10-21 (:block); \
                                    the default is all the time clocked",
                },
                "start": date("the first day, instead of span"),
                "end": date("the last day, included, instead of span"),
                "file": { "type": "string", "description": "Only this agenda file, as org-clock-report in it" },
                "maxlevel": { "type": "integer", "minimum": 1, "description": "The deepest level of headlines shown" },
            }),
            &[],
        ),
        tool(
            "roam_find",
            "Find org-roam nodes",
            "The org-roam nodes (org-roam-node-find), each title and alias on a line as offered there, \
             with its tags, ID and [file:line]; with query, those whose line has its words.",
            json!({ "query": { "type": "string", "description": "Words the node's line has, in any case" } }),
            &[],
        ),
        tool(
            "roam_node",
            "org-roam node",
            "An org-roam node's text, then what the org-roam buffer shows for it: its backlinks, \
             reflinks and, as org_roam_mode_sections says or with unlinked, its unlinked references, \
             each with the text around it and [file:line].",
            json!({
                "node": { "type": "string", "description": "The node's title, one of its aliases, or its ID" },
                "unlinked": { "type": "boolean", "description": "Also the unlinked references" },
            }),
            &["node"],
        ),
        tool(
            "journal_entries",
            "Journal entries",
            "The org-journal entries of a day, or of the days from start to end, each with its \
             [file:line] (org-journal-read-entry).",
            json!({
                "date": date("the day; the default is today"),
                "start": date("the first day, instead of date"),
                "end": date("the last day, included"),
            }),
            &[],
        ),
    ]
}

/// The reading tools, by name; `None` for another tool.
fn reading(
    name: &str,
    arguments: &Arguments,
    folder: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<Result<String, Call>> {
    if !matches!(name, "entry" | "clock_report" | "roam_find" | "roam_node" | "journal_entries") {
        return None;
    }
    let options = crate::config::load(None, env).options;
    set_variables(&options);
    Some(match name {
        "entry" => entry(arguments, &options, folder),
        "clock_report" => clock_report(arguments, &options, folder),
        "roam_find" => roam_find(arguments, &options),
        "roam_node" => roam_node(arguments, &options),
        _ => journal_entries(arguments, &options),
    })
}

/// Org's variables the server sets when it starts, which the outline and
/// the clock table read.
pub(crate) fn set_variables(options: &Options) {
    org_core::duration::set(options.durations());
    org_core::inlinetask::set_min_level(options.inlinetask_min_level());
    org_core::structure::set_odd_levels_default(options.odd_levels_only.unwrap_or(false));
}

/// The files of org-journal's directory that are journal files, by their
/// names relative to it, when org-journal is on.
fn journal_names(journal: &crate::config::JournalSettings) -> (PathBuf, Vec<String>) {
    let directory = crate::server::roam::expand(&journal.directory);
    let mut names = Vec::new();
    crate::server::roam::list_files(&directory, "", 0, &mut names);
    names.retain(|name| org_core::journal::is_journal_file(name, &journal.core));
    names.sort();
    (directory, names)
}

/// The files of org-roam's directory that are org-roam's, when it is on.
fn roam_paths(roam: &crate::config::RoamSettings) -> Vec<PathBuf> {
    let directory = crate::server::roam::expand(&roam.directory);
    let mut names = Vec::new();
    crate::server::roam::list_files(&directory, "", 0, &mut names);
    names.retain(|name| org_core::roam::is_roam_file(name, &roam.files));
    org_core::roam::sync_order(&mut names);
    names.iter().map(|name| directory.join(name)).collect()
}

/// The files the reading tools read: the agenda files, the journal's and
/// org-roam's.
fn readable_files(options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut files = agenda_paths(options, folder);
    if let Some(journal) = options.journal() {
        let (directory, names) = journal_names(&journal);
        files.extend(names.iter().map(|name| directory.join(name)));
    }
    if let Some(roam) = options.roam() {
        files.extend(roam_paths(&roam));
    }
    files
}

/// The file `given` names among `files`, as [`editable_file`] finds it,
/// saying that the tool reads rather than changes.
fn readable_file(given: &str, files: &[PathBuf], kinds: &str, folder: Option<&Path>) -> Result<PathBuf, Call> {
    editable_file(given, files, kinds, folder).map_err(|refusal| match refusal {
        Call::Failed(message) => {
            Call::Failed(message.replace("org-lsp mcp changes only those", "this tool reads only those"))
        }
        other => other,
    })
}

/// The line, from 1, of `offset` in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// A day as `YYYY-MM-DD`.
fn day_text(day: org_core::date::Day) -> String {
    let date = org_core::date::Date::from_day(day);
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The text of `file`, read whole.
fn read_text(path: &Path) -> Result<String, Call> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| format!("Cannot read {}: {error}", path.display()).into())
}

/// The outline of `text`, the file `path`, with what entries inherit.
fn outline_of(text: &str, path: &Path, options: &Options) -> (org_syntax::Parse, org_core::outline::Outline) {
    let parse = org_syntax::parse(text, &options.todo_keywords());
    let name = crate::cli::file_name(path);
    let outline = org_core::outline::Outline::with_inheritance(&parse, text, &name, &options.inheritance());
    (parse, outline)
}

/// Where the entry of the headline at `heading` ends, before its first
/// child, and where its subtree does.
fn entry_ends(outline: &org_core::outline::Outline, index: usize) -> (usize, usize) {
    let entry = &outline.entries[index];
    let after = &outline.entries[index + 1..];
    let subtree =
        after.iter().find(|other| other.level <= entry.level).map_or(outline.text.len(), |other| other.offset);
    let own = after.first().map_or(subtree, |other| other.offset).min(subtree);
    (own, subtree)
}

/// The text of the subtree of the headline at `offset` of `text`, or of
/// the whole file when `offset` is not at a headline (a file's node, a
/// day's journal file without a headline for its date).
fn subtree_text(text: &str, offset: usize, path: &Path, options: &Options) -> String {
    let (_, outline) = outline_of(text, path, options);
    match outline.entries.iter().position(|entry| entry.offset == offset) {
        Some(index) => text[offset..entry_ends(&outline, index).1].to_string(),
        None => text.to_string(),
    }
}

/// `entry`: the text of the entry at `line` of `file`, with what Org
/// makes of it.
fn entry(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let given = arguments.required("file")?;
    let line = arguments.line()?;
    let files = readable_files(options, folder);
    let path = readable_file(&given, &files, READABLE, folder)?;
    let text = read_text(&path)?;
    let name = path.display().to_string();
    let heading = headline_at_line(&text, line, &name, arguments)?;
    let (parse, outline) = outline_of(&text, &path, options);
    let index = outline
        .entries
        .iter()
        .position(|entry| entry.offset == heading)
        .ok_or_else(|| format!("Line {line} of {name} is not a headline"))?;
    let entry = &outline.entries[index];
    let (own_end, subtree_end) = entry_ends(&outline, index);

    let labels: Vec<String> = files.iter().map(|file| file.display().to_string()).collect();
    let mut out = format!("[{}:{line}]\n", file_label(&name, &labels));
    let path_parts = org_core::refile::outline_path(&text, heading, parse.todo_keywords());
    if !path_parts.is_empty() {
        out.push_str(&format!("Outline path: {}\n", path_parts.join("/")));
    }
    out.push_str(&format!("Category: {}\n", entry.category));
    if let Some(todo) = &entry.todo {
        let state = if entry.done { "done" } else { "not done" };
        out.push_str(&format!("TODO keyword: {todo} ({state})\n"));
    }
    for (keyword, planning) in
        [("SCHEDULED", &entry.scheduled), ("DEADLINE", &entry.deadline), ("CLOSED", &entry.closed)]
    {
        if let Some(planning) = planning {
            let (open, close) = if planning.active { ('<', '>') } else { ('[', ']') };
            out.push_str(&format!("{keyword}: {open}{}{close}\n", planning.raw));
        }
    }
    let tags = |inherited: bool| -> Vec<&str> {
        entry.tags.iter().filter(|tag| tag.inherited == inherited).map(|tag| tag.name.as_str()).collect()
    };
    for (what, names) in [("Tags", tags(false)), ("Inherited tags", tags(true))] {
        if !names.is_empty() {
            out.push_str(&format!("{what}: :{}:\n", names.join(":")));
        }
    }
    // `org-entry-get` with inheritance, for the properties set above the
    // entry that it does not set itself.
    let mut inherited: Vec<(String, String)> = Vec::new();
    let mut above: Vec<&str> = Vec::new();
    let mut parent = entry.parent;
    while let Some(at) = parent {
        above.extend(outline.entries[at].properties.iter().map(|(key, _)| key.as_str()));
        parent = outline.entries[at].parent;
    }
    above.extend(outline.file_properties.iter().map(|(key, _)| key.as_str()));
    above.extend(outline.keyword_properties.iter().map(|(key, _)| key.as_str()));
    for key in above {
        let key = key.trim_end_matches('+');
        let known = |name: &str| name.eq_ignore_ascii_case(key);
        if entry.properties.iter().any(|(own, _)| known(own)) || inherited.iter().any(|(seen, _)| known(seen)) {
            continue;
        }
        if outline.inheritance.inherits_property(key) {
            if let Some(value) = outline.property(index, key) {
                inherited.push((key.to_string(), value));
            }
        }
    }
    if !entry.properties.is_empty() || !inherited.is_empty() {
        out.push_str("Properties:\n");
        for (key, value) in &entry.properties {
            out.push_str(&format!("  {key}: {value}\n"));
        }
        for (key, value) in &inherited {
            out.push_str(&format!("  {key}: {value} (inherited)\n"));
        }
    }
    // `org-clock-sum-current-item`: the subtree's time, and the entry's own.
    let total = org_core::clock::subtree_minutes(&text, heading, None);
    let own = org_core::clock::subtree_minutes(&text[..own_end], heading, None);
    if total > 0 {
        let mut clocked = format!("Clocked: {}", org_core::clock::duration(total));
        if own != total {
            clocked.push_str(&format!(" (the entry itself {})", org_core::clock::duration(own)));
        }
        out.push_str(&clocked);
        out.push('\n');
    }
    if let Some(start) = org_core::clock::unclosed(&text, heading) {
        let date = org_core::date::Date::from_day(start.day);
        out.push_str(&format!(
            "Clocked in since {:04}-{:02}-{:02} {:02}:{:02}\n",
            date.year,
            date.month,
            date.day,
            start.minute / 60,
            start.minute % 60
        ));
    }
    let end = if arguments.0.get("subtree").and_then(Value::as_bool) == Some(false) { own_end } else { subtree_end };
    out.push('\n');
    out.push_str(text[heading..end].trim_end_matches('\n'));
    Ok(out)
}

/// The files a clock table reads, from disk.
struct DiskFiles {
    agenda: Vec<String>,
}

impl org_core::clocktable::Files for DiskFiles {
    fn agenda_files(&self) -> Vec<String> {
        self.agenda.clone()
    }

    fn archive_files(&self, path: &str, _: &str) -> Vec<String> {
        crate::cli::archive_files_of(path).unwrap_or_default()
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

/// `clock_report`: `org-clock-report` with a table of the agenda files,
/// or in `file`, over the span asked.
fn clock_report(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    use org_core::clocktable;
    let agenda = agenda_paths(options, folder);
    if agenda.is_empty() {
        return Err(
            "No agenda files: set org_agenda_files in the configuration file (`org-lsp config` shows it).".into()
        );
    }
    let config = options.agenda_config();
    let settings = clocktable::Settings { now: now(), ..config.clocktable };
    // The block `org-clock-report` makes, with
    // `org-clock-clocktable-default-properties` and then the arguments,
    // which take their place.
    let mut parameters = settings.default_properties.trim().to_string();
    let file = arguments.text("file").filter(|file| !file.trim().is_empty());
    let path = match &file {
        Some(given) => Some(readable_file(given, &agenda, "not an agenda file", folder)?),
        None => None,
    };
    parameters.push_str(if path.is_some() { " :scope file" } else { " :scope agenda" });
    let span = arguments.text("span").filter(|span| !span.trim().is_empty());
    let (start, end) = (arguments.text("start"), arguments.text("end"));
    if span.is_some() && (start.is_some() || end.is_some()) {
        return Err("Give either `span`, or `start` and `end`".into());
    }
    if let Some(span) = span {
        let span = span.trim();
        if !span.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("`{span}` is not a span: today, thisweek, lastmonth, 2026-10, 2026-W42…").into());
        }
        parameters.push_str(&format!(" :block {span}"));
    }
    if let Some(start) = start {
        let day = crate::cli::parse_date(&start)?;
        parameters.push_str(&format!(" :tstart \"<{}>\"", day_text(day)));
    }
    if let Some(end) = end {
        // The last day is counted whole: the table stops where the next
        // one starts.
        let day = crate::cli::parse_date(&end)?;
        parameters.push_str(&format!(" :tend \"<{}>\"", day_text(day + 1)));
    }
    if let Some(level) = arguments.0.get("maxlevel").and_then(Value::as_u64).filter(|level| *level > 0) {
        parameters.push_str(&format!(" :maxlevel {level}"));
    }
    let block = format!("#+BEGIN: clocktable {parameters}\n#+END:\n");
    let (text, name) = match &path {
        // At the top of the file, before its first headline.
        Some(path) => (format!("{block}{}", read_text(path)?), Some(path.display().to_string())),
        None => (block, None),
    };
    let files = DiskFiles { agenda: agenda.iter().map(|path| path.display().to_string()).collect() };
    let context = clocktable::Context { path: name.as_deref(), settings: &settings, files: &files };
    let done = clocktable::update(&text, 0, false, &context)?;
    // What is between the block's `#+BEGIN:` and `#+END:` lines.
    let mut table = String::new();
    for line in done.text.lines().skip(1) {
        if line.trim_start().to_ascii_uppercase().starts_with("#+END:") {
            break;
        }
        table.push_str(line);
        table.push('\n');
    }
    Ok(table.trim_end().to_string())
}

/// org-roam's database, as `org-roam-db-sync` makes it of the files now.
fn roam_db(
    options: &Options,
) -> Result<(crate::config::RoamSettings, org_core::roam::Db, std::collections::HashMap<String, u64>), Call> {
    let roam = options.roam().ok_or(ROAM_OFF)?;
    let (db, mtimes) = crate::server::roam::disk_db(&roam, &options.todo_keywords(), &options.inheritance());
    Ok((roam, db, mtimes))
}

/// Where a node is, `[file:line]`, its file named as among the files of
/// `db`; `texts` keeps the files read.
fn node_reference(
    db: &org_core::roam::Db,
    file: &str,
    offset: usize,
    texts: &mut std::collections::HashMap<String, String>,
) -> String {
    let labels: Vec<String> = db.files.iter().map(|(name, _)| name.clone()).collect();
    let text = texts.entry(file.to_string()).or_insert_with(|| std::fs::read_to_string(file).unwrap_or_default());
    format!("[{}:{}]", file_label(file, &labels), line_of(text, offset))
}

/// A node's aliases and tags, in the order org-roam lists them.
fn node_names<'a>(db: &'a org_core::roam::Db, id: &str) -> (Vec<&'a str>, Vec<&'a str>) {
    let of = |rows: &'a [(String, String)]| {
        let mut found: Vec<&str> =
            rows.iter().filter(|(owner, _)| owner == id).map(|(_, name)| name.as_str()).collect();
        found.dedup();
        found
    };
    (of(&db.aliases), of(&db.tags))
}

/// `roam_find`: the lines `org-roam-node-find` offers, with where each node is.
fn roam_find(arguments: &Arguments, options: &Options) -> Result<String, Call> {
    let (roam, db, mtimes) = roam_db(options)?;
    let candidates = db.candidates(&roam.display_template, crate::server::roam::WIDTH, &|file| {
        mtimes.get(file).copied().unwrap_or(0)
    })?;
    if candidates.is_empty() {
        return Ok("No org-roam nodes.".to_string());
    }
    let query = arguments.text("query").unwrap_or_default().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut texts = std::collections::HashMap::new();
    let mut out = String::new();
    for candidate in &candidates {
        let lowered = candidate.display.to_lowercase();
        if !words.iter().all(|word| lowered.contains(word)) {
            continue;
        }
        let Some((file, node)) = db.nodes.iter().find(|(_, node)| node.id == candidate.id) else { continue };
        let mut line = candidate.display.trim_end().to_string();
        if candidate.title != node.title {
            line.push_str(&format!(" (alias of {})", node.title));
        }
        let (_, tags) = node_names(&db, &node.id);
        if !tags.is_empty() {
            line.push_str(&format!("  :{}:", tags.join(":")));
        }
        line.push_str(&format!("  id:{}  {}", node.id, node_reference(&db, file, node.offset, &mut texts)));
        out.push_str(&line);
        out.push('\n');
    }
    if out.is_empty() {
        return Ok(format!("No org-roam node has \"{}\".", arguments.text("query").unwrap_or_default()));
    }
    Ok(out.trim_end().to_string())
}

/// `roam_node`: a node's text and its org-roam buffer.
fn roam_node(arguments: &Arguments, options: &Options) -> Result<String, Call> {
    use org_core::roam::buffer::{self, Item, Kind};
    let wanted = arguments.required("node")?;
    let wanted = wanted.trim();
    let (roam, db, _) = roam_db(options)?;
    // By `ID`, else by title, else by alias; in any case at last.
    let ids: Vec<&str> = db.nodes.iter().map(|(_, node)| node.id.as_str()).collect();
    let by = |same: &dyn Fn(&str) -> bool| -> Vec<String> {
        let mut found: Vec<String> =
            db.nodes.iter().filter(|(_, node)| same(&node.title)).map(|(_, node)| node.id.clone()).collect();
        if found.is_empty() {
            found = db.aliases.iter().filter(|(_, alias)| same(alias)).map(|(id, _)| id.clone()).collect();
        }
        found.dedup();
        found
    };
    let mut found = match ids.iter().find(|id| **id == wanted) {
        Some(id) => vec![id.to_string()],
        None => by(&|name| name == wanted),
    };
    if found.is_empty() {
        found = by(&|name| name.to_lowercase() == wanted.to_lowercase());
    }
    let id = match &found[..] {
        [one] => one.clone(),
        [] => return Err(format!("No org-roam node is \"{wanted}\": roam_find lists them").into()),
        _ => {
            let mut texts = std::collections::HashMap::new();
            let listed: Vec<String> = found
                .iter()
                .filter_map(|id| db.nodes.iter().find(|(_, node)| node.id == *id))
                .map(|(file, node)| {
                    format!("{} id:{} {}", node.title, node.id, node_reference(&db, file, node.offset, &mut texts))
                })
                .collect();
            return Err(format!("More than one node is \"{wanted}\": give its ID ({})", listed.join("; ")).into());
        }
    };
    let (file, node) = db.nodes.iter().find(|(_, node)| node.id == id).ok_or("No such node")?;
    let mut texts = std::collections::HashMap::new();
    let mut out = format!("{}  id:{}  {}\n", node.title, node.id, node_reference(&db, file, node.offset, &mut texts));
    let (aliases, tags) = node_names(&db, &id);
    if !aliases.is_empty() {
        out.push_str(&format!("Aliases: {}\n", aliases.join(", ")));
    }
    if !tags.is_empty() {
        out.push_str(&format!("Tags: :{}:\n", tags.join(":")));
    }
    let refs: Vec<&str> =
        db.refs.iter().filter(|reference| reference.node == id).map(|reference| reference.reference.as_str()).collect();
    if !refs.is_empty() {
        out.push_str(&format!("Refs: {}\n", refs.join(" ")));
    }
    let text = texts.get(file.as_str()).cloned().unwrap_or_default();
    let shown = if node.level == 0 { text.clone() } else { subtree_text(&text, node.offset, Path::new(file), options) };
    out.push('\n');
    out.push_str(shown.trim_end_matches('\n'));
    out.push('\n');

    // The org-roam buffer, as `org_roam_mode_sections` says.
    let mut kinds = roam.sections.clone();
    if arguments.flag("unlinked") && !kinds.contains(&Kind::UnlinkedReferences) {
        kinds.push(Kind::UnlinkedReferences);
    }
    let directory = crate::server::roam::expand(&roam.directory);
    let grep = crate::server::roam::buffer::grep_files(&directory, &roam.files.extensions);
    let read = |file: &str| std::fs::read_to_string(file).ok();
    let sections = buffer::sections(&db, &id, &kinds, &read, &grep);
    for section in &sections {
        out.push_str(&format!("\n{}\n", buffer::heading(section)));
        for item in &section.items {
            match item {
                Item::Node { title, outline, preview, file, offset } => {
                    let reference = node_reference(&db, file, *offset, &mut texts);
                    out.push_str(&format!("- {title} ({})  {reference}\n", buffer::outline_text(outline)));
                    for line in preview.lines() {
                        out.push_str(&format!("  {line}\n").replace("  \n", "\n"));
                    }
                }
                Item::Grep { file, row, line, .. } => {
                    let labels: Vec<String> = grep.clone();
                    out.push_str(&format!("- {}  [{}:{row}]\n", line.trim(), file_label(file, &labels)));
                }
            }
        }
    }
    Ok(out.trim_end().to_string())
}

/// `journal_entries`: `org-journal-read-entry` for each day with an entry.
fn journal_entries(arguments: &Arguments, options: &Options) -> Result<String, Call> {
    use org_core::journal::{self, Command, Request};
    let journal = options.journal().ok_or(JOURNAL_OFF)?;
    let today = now().day;
    let day = |key: &str| -> Result<Option<org_core::date::Day>, Call> {
        match arguments.text(key).filter(|text| !text.trim().is_empty()) {
            Some(text) => Ok(Some(crate::cli::parse_date(text.trim())?)),
            None => Ok(None),
        }
    };
    let (first, last) = match (day("date")?, day("start")?, day("end")?) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => return Err("Give either `date`, or `start` and `end`".into()),
        (Some(date), None, None) => (date, date),
        (None, Some(start), end) => (start, end.unwrap_or(today)),
        (None, None, Some(end)) => (end, end),
        (None, None, None) => (today, today),
    };
    if last < first {
        return Err("Period end cannot be before the start".into());
    }
    let (directory, names) = journal_names(&journal);
    let files: std::collections::BTreeMap<String, String> = names
        .iter()
        .filter_map(|name| Some((name.clone(), std::fs::read_to_string(directory.join(name)).ok()?)))
        .collect();
    let mut settings = journal.core.clone();
    let shown = std::fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
    settings.directory = format!("{}/", shown.display()).replace("//", "/");
    let edit = options.edit_settings();
    settings.properties = org_core::edit::property_context(&org_syntax::parse("", &options.todo_keywords()), &edit, "");
    let labels: Vec<String> = names.iter().map(|name| directory.join(name).display().to_string()).collect();
    let mut days = journal::entry_days(&files, &settings);
    days.retain(|day| (first..=last).contains(day));
    days.sort();
    days.dedup();
    let mut out = String::new();
    for day in days {
        let request = Request { files: &files, now: now(), from: None, answers: &[] };
        let Ok(outcome) = journal::run(&Command::ReadEntry { day, display: true }, &request, &settings) else {
            continue;
        };
        let Some(visit) = outcome.visit else { continue };
        let Some(text) = files.get(&visit.file) else { continue };
        let path = directory.join(&visit.file);
        let offset = visit.offset.min(text.len());
        let shown = subtree_text(text, offset, &path, options);
        let start = if shown.len() == text.len() { 0 } else { offset };
        let label = file_label(&path.display().to_string(), &labels);
        out.push_str(&format!("{}  [{label}:{}]\n", day_text(day), line_of(text, start)));
        out.push_str(shown.trim_end_matches('\n'));
        out.push_str("\n\n");
    }
    if out.is_empty() {
        return Ok(if first == last {
            "No journal entry for this date.".to_string()
        } else {
            format!("No journal entry from {} to {}.", day_text(first), day_text(last))
        });
    }
    Ok(out.trim_end().to_string())
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
            [
                "agenda",
                "todo_list",
                "tags_search",
                "text_search",
                "custom_agenda",
                "agenda_files",
                "capture_templates",
                "refile_targets",
                "entry",
                "clock_report",
                "roam_find",
                "roam_node",
                "journal_entries"
            ]
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
            self.ask_in(&self.folder, method, params)
        }

        /// [`Setup::ask`] of a server started in `folder`.
        fn ask_in(&self, folder: &std::path::Path, method: &str, params: Value) -> Value {
            let config = self.config.display().to_string();
            let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
            let line = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
            answer(&line, Some(folder), &env).expect("an answer")
        }

        /// The text a tool answers, and whether it is an error.
        fn call(&self, name: &str, arguments: Value) -> (String, bool) {
            self.call_in(&self.folder, name, arguments)
        }

        /// [`Setup::call`] of a server started in `folder`.
        fn call_in(&self, folder: &std::path::Path, name: &str, arguments: Value) -> (String, bool) {
            let answer = self.ask_in(folder, "tools/call", json!({ "name": name, "arguments": arguments }));
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
    fn a_malformed_option_leaves_the_others_to_the_views() {
        let config = "org_todo_keywords = [\"TODO\", \"NEXT\", \"|\", \"DONE\"]\norg_log_done = 5\norg_agenda_span = {}";
        let setup = Setup::new("malformed", config, &[("tasks.org", "* NEXT Call the plumber\n")]);
        let (todo, error) = setup.call("todo_list", json!({}));
        assert!(!error && todo.contains("NEXT Call the plumber"), "{todo}");
        // No agenda files: what is not there, and what the file has wrong.
        std::fs::remove_file(setup.folder.join("tasks.org")).unwrap();
        let (text, error) = setup.call("todo_list", json!({}));
        assert!(error && text.contains("these are not there") && text.contains("tasks.org"), "{text}");
        assert!(text.contains("`org_log_done` is malformed"), "{text}");
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

    const PROJECTS: &str = "* Garden\n** Beds\n* House\n";

    /// Targets: the top two levels of the agenda files, and the top level
    /// of `projects.org`, by their outline paths.
    const REFILE: &str = "mcp_edits = true\norg_refile_use_outline_path = true\n\
                          org_refile_targets = [{ files = \"agenda\", maxlevel = 2 }, \
                          { files = \"projects.org\", level = 1 }]\n";

    fn refile_setup(name: &str, config: &str) -> Setup {
        Setup::new(name, &format!("{REFILE}{config}"), &[("tasks.org", TASKS), ("projects.org", PROJECTS)])
    }

    #[test]
    fn lists_the_refile_targets() {
        let setup = refile_setup("targets", "");
        let (text, error) = setup.call("refile_targets", json!({}));
        assert!(!error, "{text}");
        assert!(text.contains("Inbox/ (tasks.org)  [tasks.org:2]\n"), "{text}");
        assert!(text.contains("Garden/ (projects.org)  [projects.org:1]\n"), "{text}");
        assert!(text.ends_with("House/ (projects.org)  [projects.org:3]"), "{text}");
        assert!(!text.contains("Beds"), "{text}");
        // From the entry's file, its own targets go without its name.
        let (text, _) = setup.call("refile_targets", json!({ "file": "tasks.org" }));
        assert!(text.starts_with("Inbox/  [tasks.org:2]\n"), "{text}");
        let (text, _) = setup.call("refile_targets", json!({ "filter": "gar" }));
        assert_eq!(text, "Garden/ (projects.org)  [projects.org:1]");
        let (text, error) = setup.call("refile_targets", json!({ "filter": "nothing" }));
        assert!(!error && text == "No refile target has \"nothing\".", "{text}");

        // Listed with mcp_edits off too, and refile is not.
        let off = Setup::new("targets-off", "", &[("tasks.org", TASKS)]);
        let (text, error) = off.call("refile_targets", json!({ "file": "tasks.org" }));
        assert_eq!(text, "Inbox  [tasks.org:2]\nWrite the report  [tasks.org:3]\nWater the plants  [tasks.org:4]");
        assert!(!error);
        let list = off.ask("tools/list", json!({}));
        let names: Vec<&str> =
            list["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        assert!(names.contains(&"refile_targets") && !names.contains(&"refile"), "{names:?}");
        let (text, error) = off.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "Inbox" }));
        assert!(error && text.contains("mcp_edits = true"), "{text}");
        assert_eq!(off.read("tasks.org"), TASKS);
    }

    #[test]
    fn a_file_listed_twice_is_named_by_its_name() {
        // The agenda file is a capture target and, by default, the refile
        // targets' file too; the server runs in another folder.
        let config = "mcp_edits = true\n[[org_capture_templates]]\nkey = \"t\"\ndescription = \"Task\"\n\
                      target = [\"file+headline\", \"tasks.org\", \"Inbox\"]\ntemplate = \"* TODO %?\"\n";
        let setup = Setup::new("twice", config, &[("tasks.org", TASKS)]);
        let elsewhere = std::env::temp_dir();
        let arguments = json!({ "file": "tasks.org", "line": 3, "state": "DONE", "dry_run": true });
        let (text, error) = setup.call_in(&elsewhere, "set_todo_state", arguments);
        assert!(!error && text.contains("Dry run"), "{text}");
        let (text, error) = setup.call_in(&elsewhere, "refile_targets", json!({ "file": "tasks.org" }));
        assert!(!error && text.starts_with("Inbox  [tasks.org:2]"), "{text}");
        let arguments = json!({ "file": "tasks.org", "line": 4, "target": "Inbox", "dry_run": true });
        let (text, error) = setup.call_in(&elsewhere, "refile", arguments);
        assert!(!error && text.contains("Dry run"), "{text}");
    }

    #[test]
    fn refiles_to_another_file() {
        let setup = refile_setup("across", "");
        let arguments = json!({
            "file": "tasks.org",
            "line": 4,
            "headline": "Water the plants",
            "target": "Garden/ (projects.org)",
        });
        let (text, error) = setup.call("refile", arguments);
        assert!(!error, "{text}");
        let projects = setup.folder.join("projects.org").display().to_string();
        assert!(
            text.starts_with(&format!("Refile to \"Garden/ (projects.org)\" in file {projects}: done\n")),
            "{text}"
        );
        assert!(text.ends_with(&format!("{projects}:3: ** TODO Water the plants")), "{text}");
        assert_eq!(
            setup.read("projects.org"),
            "* Garden\n** Beds\n** TODO Water the plants\nSCHEDULED: <2026-10-21 Wed +1w>\n* House\n"
        );
        assert_eq!(setup.read("tasks.org"), "#+title: Tasks\n* Inbox\n* TODO Write the report :work:\n");

        // By where the target is, and kept with copy.
        let arguments = json!({ "file": "tasks.org", "line": 3, "target": "[projects.org:5]", "copy": true });
        let (text, error) = setup.call("refile", arguments);
        assert!(!error && text.starts_with("Copy to \"House/ (projects.org)\""), "{text}");
        assert_eq!(setup.read("tasks.org"), "#+title: Tasks\n* Inbox\n* TODO Write the report :work:\n");
        assert!(
            setup.read("projects.org").contains("* House\n** TODO Write the report"),
            "{}",
            setup.read("projects.org")
        );
    }

    #[test]
    fn refiles_in_the_same_file() {
        let setup = refile_setup("same", "");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "Inbox/" }));
        assert!(!error, "{text}");
        assert!(text.contains(":3: ** TODO Write the report ") && text.ends_with(":work:"), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.starts_with("#+title: Tasks\n* Inbox\n** TODO Write the report "), "{tasks}");
        assert!(tasks.ends_with(":work:\n* TODO Water the plants\nSCHEDULED: <2026-10-21 Wed +1w>\n"), "{tasks}");

        // Not into itself or its own subtree, as Org says.
        let (text, error) =
            setup.call("refile", json!({ "file": "tasks.org", "line": 2, "target": "Inbox/Write the report/" }));
        assert!(error && text == "Cannot refile to position inside the tree or region", "{text}");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 2, "target": "Inbox" }));
        assert!(error && text == "Cannot refile to position inside the tree or region", "{text}");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 2, "target": "Nowhere" }));
        assert!(error && text == "Invalid target location", "{text}");
        assert_eq!(setup.read("tasks.org"), tasks);
    }

    #[test]
    fn a_dry_run_or_a_stale_line_writes_nothing() {
        let setup = refile_setup("refile-dry", "");
        let arguments = json!({ "file": "tasks.org", "line": 3, "target": "House/ (projects.org)", "dry_run": true });
        let (text, error) = setup.call("refile", arguments);
        assert!(!error, "{text}");
        assert!(text.contains("Dry run: nothing was written"), "{text}");
        assert!(text.contains("+** TODO Write the report"), "{text}");
        assert!(text.contains("-* TODO Write the report :work:"), "{text}");
        // The target's diff first, as it is written first.
        assert!(text.find("projects.org\n+++").unwrap() < text.find("tasks.org\n+++").unwrap(), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);
        assert_eq!(setup.read("projects.org"), PROJECTS);

        let arguments = json!({ "file": "tasks.org", "line": 3, "target": "House", "headline": "Water the plants" });
        let (text, error) = setup.call("refile", arguments);
        assert!(error && text.contains("not the entry"), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);
        assert_eq!(setup.read("projects.org"), PROJECTS);
    }

    #[test]
    fn refiling_logs_as_org_log_refile_says() {
        let setup = refile_setup("refile-log", "org_log_refile = \"time\"\n");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "Garden" }));
        assert!(!error, "{text}");
        let projects = setup.read("projects.org");
        assert!(projects.contains("** Beds\n** TODO Write the report"), "{projects}");
        assert!(projects.contains(":work:\n- Refiled on ["), "{projects}");

        let setup = refile_setup("refile-note", "org_log_refile = \"note\"\n");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "Garden" }));
        assert!(error && text.contains("`note`"), "{text}");
        assert_eq!(setup.read("projects.org"), PROJECTS);
        let arguments = json!({ "file": "tasks.org", "line": 3, "target": "Garden", "note": "For the spring" });
        let (text, error) = setup.call("refile", arguments);
        assert!(!error, "{text}");
        let projects = setup.read("projects.org");
        assert!(projects.contains("- Refiled on [") && projects.contains("\\\\\n  For the spring"), "{projects}");
    }

    #[test]
    fn makes_new_parents_and_files_at_the_top_level() {
        let config = "org_refile_allow_creating_parent_nodes = \"confirm\"\n";
        let setup = refile_setup("refile-new", config);
        let arguments = json!({ "file": "tasks.org", "line": 3, "target": "Garden/Herbs" });
        let (text, error) = setup.call("refile", arguments.clone());
        assert!(error && text.starts_with("Create new node \"Herbs\"?"), "{text}");
        assert_eq!(setup.read("projects.org"), PROJECTS);
        let mut arguments = arguments;
        arguments["create"] = json!(true);
        let (text, error) = setup.call("refile", arguments);
        assert!(!error && text.contains("Refile to \"Garden/ (projects.org)/Herbs\""), "{text}");
        let projects = setup.read("projects.org");
        assert!(projects.starts_with("* Garden\n** Beds\n\n** Herbs\n*** TODO Write the report"), "{projects}");

        // With the files as targets, a file alone is its top level.
        let config = "mcp_edits = true\norg_refile_use_outline_path = \"file\"\n\
                      org_refile_targets = [{ files = \"projects.org\", level = 1 }]\n";
        let setup = Setup::new("refile-file", config, &[("tasks.org", TASKS), ("projects.org", PROJECTS)]);
        let (text, _) = setup.call("refile_targets", json!({}));
        assert!(text.starts_with("projects.org/  [projects.org]\nprojects.org/Garden/  [projects.org:1]"), "{text}");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "projects.org" }));
        assert!(!error, "{text}");
        let projects = setup.read("projects.org");
        assert!(projects.starts_with("* Garden\n** Beds\n* House\n* TODO Write the report "), "{projects}");
    }

    #[test]
    fn a_verify_function_needs_emacs() {
        let setup = refile_setup("refile-verify", "org_refile_target_verify_function = \"my-verify\"\n");
        let (text, error) = setup.call("refile_targets", json!({}));
        assert!(error && text.contains("needs Emacs"), "{text}");
        let (text, error) = setup.call("refile", json!({ "file": "tasks.org", "line": 3, "target": "Garden" }));
        assert!(error && text.contains("needs Emacs"), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);
    }

    const CLOCKED: &str = "#+title: Tasks\n#+PROPERTY: OWNER Ana\n* Projects :work:\n:PROPERTIES:\n:CATEGORY: proj\n:END:\n\
                           ** TODO Write the report :urgent:\nSCHEDULED: <2026-10-21 Wed> DEADLINE: <2026-10-23 Fri>\n\
                           :PROPERTIES:\n:EFFORT: 1:00\n:END:\n:LOGBOOK:\n\
                           CLOCK: [2026-10-20 Tue 09:00]--[2026-10-20 Tue 10:30] =>  1:30\n:END:\nSome notes.\n\
                           *** Part one\n:LOGBOOK:\nCLOCK: [2026-10-21 Wed 09:00]--[2026-10-21 Wed 09:45] =>  0:45\n:END:\n\
                           * Other\n";

    #[test]
    fn reads_an_entry_with_what_it_inherits() {
        let config = "org_use_property_inheritance = true";
        let setup = Setup::new("entry", config, &[("tasks.org", CLOCKED), ("other.org", "* Elsewhere\n")]);
        let (text, error) = setup.call("entry", json!({ "file": "tasks.org", "line": 7, "headline": "Write" }));
        assert!(!error, "{text}");
        assert!(text.starts_with("[tasks.org:7]\nOutline path: Projects\nCategory: proj\n"), "{text}");
        assert!(
            text.contains("TODO keyword: TODO (not done)\nSCHEDULED: <2026-10-21 Wed>\nDEADLINE: <2026-10-23 Fri>\n")
        );
        assert!(text.contains("Tags: :urgent:\nInherited tags: :work:\n"), "{text}");
        assert!(text.contains("  EFFORT: 1:00\n  CATEGORY: proj (inherited)\n  OWNER: Ana (inherited)\n"), "{text}");
        assert!(text.contains("Clocked: 2:15 (the entry itself 1:30)\n\n** TODO Write the report"), "{text}");
        assert!(text.ends_with(
            "*** Part one\n:LOGBOOK:\nCLOCK: [2026-10-21 Wed 09:00]--[2026-10-21 Wed 09:45] =>  0:45\n:END:"
        ));

        // The entry alone, up to its first child.
        let (text, _) = setup.call("entry", json!({ "file": "tasks.org", "line": 7, "subtree": false }));
        assert!(text.ends_with(":END:\nSome notes."), "{text}");

        // Without inheritance of properties, none is inherited.
        let plain = Setup::new("entry-plain", "", &[("tasks.org", CLOCKED)]);
        let (text, _) = plain.call("entry", json!({ "file": "tasks.org", "line": 7 }));
        assert!(text.contains("Properties:\n  EFFORT: 1:00\nClocked"), "{text}");

        let (text, error) = setup.call("entry", json!({ "file": "tasks.org", "line": 7, "headline": "Other" }));
        assert!(error && text.contains("not the entry"), "{text}");
        let (text, error) = setup.call("entry", json!({ "file": "tasks.org", "line": 2 }));
        assert!(error && text.contains("not a headline"), "{text}");
        let (text, error) = setup.call("entry", json!({ "file": "other.org", "line": 1 }));
        assert!(error && text.contains("neither an agenda file") && text.contains("reads only those"), "{text}");
    }

    #[test]
    fn builds_the_clock_report() {
        let setup = Setup::new("clock-report", "", &[("tasks.org", CLOCKED)]);
        let (text, error) = setup.call("clock_report", json!({}));
        assert!(!error, "{text}");
        assert!(text.starts_with("#+CAPTION: Clock summary at ["), "{text}");
        assert!(text.contains("| tasks.org | *File time*          | *2:15* |      |\n"), "{text}");
        assert!(text.ends_with("|           | \\_  Write the report |        | 2:15 |"), "{text}");

        // One day, its end included, three levels deep.
        let arguments = json!({ "start": "2026-10-21", "end": "2026-10-21", "maxlevel": 3 });
        let (text, error) = setup.call("clock_report", arguments);
        assert!(!error, "{text}");
        assert!(text.contains("ALL *Total time*     | *0:45*"), "{text}");
        assert!(text.ends_with("|           | \\_    Part one       |        |      | 0:45 |"), "{text}");

        // In the file, as org-clock-report there: no file column.
        let (text, error) = setup.call("clock_report", json!({ "span": "2026-10", "file": "tasks.org" }));
        assert!(!error, "{text}");
        assert!(text.contains(", for October 2026.\n| Headline "), "{text}");
        assert!(text.contains("| *Total time*         | *2:15* |"), "{text}");
        let (text, error) = setup.call("clock_report", json!({ "span": "2026-09" }));
        assert!(!error && text.contains("ALL *Total time* | *0:00*"), "{text}");

        let (text, error) = setup.call("clock_report", json!({ "span": "today", "start": "2026-10-21" }));
        assert!(error && text.contains("either"), "{text}");
        let (text, error) = setup.call("clock_report", json!({ "span": "today) (x" }));
        assert!(error && text.contains("is not a span"), "{text}");
    }

    /// A setup with org-roam's and org-journal's directories in its folder.
    fn plugins_setup(name: &str, files: &[(&str, &str)]) -> Setup {
        let folder = std::env::temp_dir().join(format!("org-lsp-mcp-{name}-{}", std::process::id()));
        let config = format!(
            "org_roam_enabled = true\norg_roam_directory = {:?}\norg_journal_enabled = true\norg_journal_dir = {:?}\n",
            folder.join("roam").display().to_string(),
            folder.join("journal").display().to_string()
        );
        let setup = Setup::new(name, &config, &[("tasks.org", TASKS)]);
        for (file, text) in files {
            let path = setup.folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        setup
    }

    const GARDEN: &str =
        ":PROPERTIES:\n:ID: garden-id\n:ROAM_ALIASES: Yard\n:END:\n#+title: Garden\n#+filetags: :home:\n\n\
                          Plants and beds.\n";
    const HERBS: &str =
        ":PROPERTIES:\n:ID: herbs-id\n:END:\n#+title: Herbs\n\nHerbs grow in the [[id:garden-id][Garden]].\n\
                         * Basil\n:PROPERTIES:\n:ID: basil-id\n:END:\nBasil likes the garden sun.\n* Mint\n";

    #[test]
    fn finds_org_roam_nodes_and_their_backlinks() {
        let off = Setup::new("roam-off", "", &[("tasks.org", TASKS)]);
        let (text, error) = off.call("roam_find", json!({}));
        assert!(error && text.starts_with("org-roam is not enabled"), "{text}");
        let (text, error) = off.call("roam_node", json!({ "node": "Garden" }));
        assert!(error && text.starts_with("org-roam is not enabled"), "{text}");

        let setup = plugins_setup("roam", &[("roam/garden.org", GARDEN), ("roam/herbs.org", HERBS)]);
        let (text, error) = setup.call("roam_find", json!({}));
        assert!(!error, "{text}");
        assert!(text.contains("Garden  :home:  id:garden-id  [garden.org:1]\n"), "{text}");
        assert!(text.contains("Basil  id:basil-id  [herbs.org:7]"), "{text}");
        let (text, _) = setup.call("roam_find", json!({ "query": "YAR" }));
        assert_eq!(text, "Yard (alias of Garden)  :home:  id:garden-id  [garden.org:1]");
        let (text, _) = setup.call("roam_find", json!({ "query": "nothing" }));
        assert_eq!(text, "No org-roam node has \"nothing\".");

        // By alias: the file's text, then its backlinks.
        let (text, error) = setup.call("roam_node", json!({ "node": "yard" }));
        assert!(!error, "{text}");
        assert!(text.starts_with("Garden  id:garden-id  [garden.org:1]\nAliases: Yard\nTags: :home:\n\n:PROPERTIES:"));
        assert!(text.contains("\nPlants and beds.\n\nBacklinks (1)\n- Herbs (Top)  [herbs.org:6]\n"), "{text}");
        assert!(!text.contains("Unlinked"), "{text}");
        let (text, _) = setup.call("roam_node", json!({ "node": "garden-id", "unlinked": true }));
        assert!(text.contains("Unlinked References (") && text.contains("[herbs.org:11]"), "{text}");

        // By ID, a headline's node: its subtree only.
        let (text, error) = setup.call("roam_node", json!({ "node": "basil-id" }));
        assert!(!error, "{text}");
        assert!(
            text.ends_with("[herbs.org:7]\n\n* Basil\n:PROPERTIES:\n:ID: basil-id\n:END:\nBasil likes the garden sun.")
        );
        let (text, error) = setup.call("roam_node", json!({ "node": "Nowhere" }));
        assert!(error && text.contains("roam_find"), "{text}");

        // An org-roam file is one entry reads.
        let (text, error) = setup.call("entry", json!({ "file": "herbs.org", "line": 7 }));
        assert!(!error && text.starts_with("[herbs.org:7]\n"), "{text}");
    }

    #[test]
    fn reads_the_journal_of_some_days() {
        let off = Setup::new("journal-off", "", &[("tasks.org", TASKS)]);
        let (text, error) = off.call("journal_entries", json!({}));
        assert!(error && text.starts_with("org-journal is not enabled"), "{text}");

        let setup = plugins_setup(
            "journal-days",
            &[
                ("journal/20261019", "* Monday, 10/19/26\n** 10:00 Start\n"),
                ("journal/20261021", "* Wednesday, 10/21/26\n** 09:00 Morning\nWrote things.\n"),
            ],
        );
        let (text, error) = setup.call("journal_entries", json!({ "date": "2026-10-21" }));
        assert!(!error, "{text}");
        assert_eq!(text, "2026-10-21  [20261021:1]\n* Wednesday, 10/21/26\n** 09:00 Morning\nWrote things.");
        let (text, _) = setup.call("journal_entries", json!({ "start": "2026-10-01", "end": "2026-10-31" }));
        assert!(
            text.starts_with("2026-10-19  [20261019:1]\n* Monday, 10/19/26\n** 10:00 Start\n\n2026-10-21"),
            "{text}"
        );
        let (text, error) = setup.call("journal_entries", json!({ "date": "2026-10-22" }));
        assert!(!error && text == "No journal entry for this date.", "{text}");
        let (text, error) = setup.call("journal_entries", json!({ "start": "2026-10-22", "end": "2026-10-01" }));
        assert!(error && text == "Period end cannot be before the start", "{text}");
        // A journal file is one entry reads.
        let (text, error) = setup.call("entry", json!({ "file": "20261021", "line": 2 }));
        assert!(!error && text.ends_with("** 09:00 Morning\nWrote things."), "{text}");
    }

    #[test]
    fn a_weekly_journal_has_an_entry_a_day() {
        let weekly = "* Monday, 10/19/26\n:PROPERTIES:\n:CREATED:  20261019\n:END:\n** 10:00 Start\n\
                      * Wednesday, 10/21/26\n:PROPERTIES:\n:CREATED:  20261021\n:END:\n** 09:00 Morning\n";
        let setup = plugins_setup("journal-weekly", &[("journal/20261019", weekly)]);
        let config = format!("{}org_journal_file_type = \"weekly\"\n", std::fs::read_to_string(&setup.config).unwrap());
        std::fs::write(&setup.config, config).unwrap();
        let (text, error) = setup.call("journal_entries", json!({ "date": "2026-10-21" }));
        assert!(!error, "{text}");
        assert!(text.starts_with("2026-10-21  [20261019:6]\n* Wednesday, 10/21/26\n"), "{text}");
        assert!(!text.contains("Monday"), "{text}");
    }
}
