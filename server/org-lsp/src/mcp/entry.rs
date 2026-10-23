//! The edit tools of `org-lsp mcp` that change an entry where it is, or
//! archive or delete it: `schedule`, `deadline`, `set_tags`,
//! `set_property`, `delete_property`, `set_priority`, `set_effort`,
//! `add_note`, `archive` and `delete_entry`. Each goes through the code the
//! server's command goes through (`org-schedule` with
//! [`org_core::planning`], `org-set-tags` and the property commands with
//! [`org_core::edit::run`], `org-archive-subtree` with
//! [`org_core::archive`], `org-cut-subtree` with
//! [`org_core::structure::copy_subtree`]), with the settings of the
//! configuration file, and takes the entry as `set_todo_state` does.

use std::path::{Path, PathBuf};

use org_core::edit::{self, Command};
use org_core::planning::{self, Command as PlanningCommand};
use org_core::{archive, priority, properties, structure, tags, todo};
use serde_json::{json, Value};

use super::{
    agenda_paths, answer_text, canonical, diff, editable_file, editable_files, headline_at_line, is_headline, now,
    write_file, Arguments, Call, EDITABLE,
};
use crate::config::Options;

/// The tools of this module, by name.
pub(super) const NAMES: [&str; 10] = [
    "schedule",
    "deadline",
    "set_tags",
    "set_property",
    "delete_property",
    "set_priority",
    "set_effort",
    "archive",
    "add_note",
    "delete_entry",
];

/// The definitions of the tools, for `tools/list` with `mcp_edits` on.
pub(super) fn tools() -> Vec<Value> {
    let file = json!({ "type": "string", "description": "The entry's file: its path, or its name in the views" });
    let line = json!({ "type": "integer", "minimum": 1, "description": "The line of the entry's headline, from 1" });
    let headline = json!({
        "type": "string",
        "description": "Text the headline has, such as its title: refused if the line no longer has it",
    });
    let dry_run = json!({ "type": "boolean", "description": "Answer the change as a diff and write nothing" });
    let tool = |name: &str, title: &str, description: &str, more: Value, required: &[&str], idempotent: bool| {
        let mut properties = json!({ "file": file, "line": line, "headline": headline, "dry_run": dry_run });
        for (key, value) in more.as_object().into_iter().flatten() {
            properties[key] = value.clone();
        }
        let mut required: Vec<&str> = required.to_vec();
        required.splice(0..0, ["file", "line"]);
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": idempotent,
                "openWorldHint": false,
            },
        })
    };
    let date = |what: &str| {
        json!({
            "date": {
                "type": "string",
                "description": format!(
                    "The {what} as Org's date prompt reads an answer (org-read-date): 2026-11-03, \
                     2026-11-03 10:00, +2d, fri, 10am; a timestamp keeps its repeater or delay \
                     (<2026-11-03 +1w>), and else the old date's is kept"
                ),
            },
            "remove": { "type": "boolean", "description": format!("Take the {what} out (C-u)") },
            "note": {
                "type": "string",
                "description": "The note, when org_log_reschedule or org_log_redeadline asks for one",
            },
        })
    };
    let note = json!({
        "type": "string",
        "description": "The note, when the change of a TODO keyword or a priority asks for one",
    });
    vec![
        tool(
            "schedule",
            "Schedule",
            "Schedule an entry (org-schedule): its SCHEDULED date set to `date`, read as Org's date \
             prompt reads it, or taken out with `remove`; a date it had logged as org_log_reschedule \
             says. Writes the file. The entry is the [file:line] a view gives at the end of its line.",
            date("scheduled date"),
            &[],
            false,
        ),
        tool(
            "deadline",
            "Deadline",
            "Give an entry a deadline (org-deadline): its DEADLINE set to `date`, read as Org's date \
             prompt reads it, or taken out with `remove`; a deadline it had logged as \
             org_log_redeadline says. Writes the file.",
            date("deadline"),
            &[],
            false,
        ),
        tool(
            "set_tags",
            "Set tags",
            "Set the tags of an entry (org-set-tags-command): the tags given replace those it has, \
             aligned at org_tags_column; none removes them. Writes the file.",
            json!({
                "tags": {
                    "type": ["array", "string"],
                    "items": { "type": "string" },
                    "description": "The tags, as a list or separated by colons (:work:urgent:)",
                },
            }),
            &["tags"],
            true,
        ),
        tool(
            "set_property",
            "Set property",
            "Set a property of an entry (org-set-property), in its PROPERTIES drawer; TODO, PRIORITY, \
             SCHEDULED and DEADLINE change the keyword, the priority and the dates as Org does. Only \
             an allowed value will do when the property has them. Writes the file.",
            json!({
                "name": { "type": "string", "description": "The property's name" },
                "value": { "type": "string", "description": "Its value; \"\" keeps the value it has, as Org" },
                "note": note,
            }),
            &["name", "value"],
            true,
        ),
        tool(
            "delete_property",
            "Delete property",
            "Delete a property of an entry (org-delete-property), and its drawer when it is left empty. \
             Writes the file.",
            json!({ "name": { "type": "string", "description": "The property's name" } }),
            &["name"],
            true,
        ),
        tool(
            "set_priority",
            "Set priority",
            "Set the priority of an entry (org-priority): a letter or number in the range of \
             org_priority_highest and org_priority_lowest (or the file's #+PRIORITIES), or \"\" to \
             remove it. Writes the file.",
            json!({
                "priority": {
                    "type": ["string", "integer"],
                    "description": "The priority, such as A or 2, or \"\" to remove it",
                },
            }),
            &["priority"],
            true,
        ),
        tool(
            "set_effort",
            "Set effort",
            "Set the effort of an entry (org-set-effort): its Effort property, a duration such as 1:30 \
             or 2h (org_effort_durations), one of the allowed values when there are some. Writes the \
             file.",
            json!({ "value": { "type": "string", "description": "The effort, such as 0:30, 1:00 or 2d" } }),
            &["value"],
            true,
        ),
        tool(
            "archive",
            "Archive",
            "Archive an entry and its subtree: moved to its archive file (org-archive-subtree, where \
             org_archive_location and the file's #+ARCHIVE say), under the parent's Archive sibling \
             (to_sibling, org-archive-to-archive-sibling), or tagged ARCHIVE or not (toggle_tag, \
             org-toggle-archive-tag). Writes the archive file, then the entry's.",
            json!({
                "method": {
                    "type": "string",
                    "enum": ["subtree", "to_sibling", "toggle_tag"],
                    "description": "How to archive: subtree, the default, to_sibling or toggle_tag",
                },
            }),
            &[],
            false,
        ),
        tool(
            "add_note",
            "Add note",
            "Add a note to an entry (org-add-note), as Org logs it: under the headline or in the \
             drawer of org_log_into_drawer, with the heading of org_log_note_headings. Writes the file.",
            json!({ "note": { "type": "string", "description": "The note's text" } }),
            &["note"],
            false,
        ),
        {
            let mut delete = tool(
                "delete_entry",
                "Delete entry",
                "Delete an entry and its subtree, as org-cut-subtree takes it out: from its headline to the \
                 next headline of its level or above, the blank lines before that one included. Only in an \
                 agenda file, and only with `headline`. Answers Org's message and the text deleted. Writes \
                 the file.",
                json!({
                    "headline": {
                        "type": "string",
                        "description": "Text the headline has, such as its title: required, and refused if the \
                                        line does not have it",
                    },
                }),
                &["headline"],
                false,
            );
            delete["annotations"]["destructiveHint"] = json!(true);
            delete
        },
    ]
}

/// Run the tool `name`, one of [`NAMES`].
pub(super) fn call(
    name: &str,
    arguments: &Arguments,
    options: &Options,
    folder: Option<&Path>,
) -> Result<String, Call> {
    match name {
        "schedule" => schedule(arguments, options, folder, PlanningCommand::Schedule, None),
        "deadline" => schedule(arguments, options, folder, PlanningCommand::Deadline, None),
        "set_tags" => set_tags(arguments, options, folder),
        "set_property" => set_property(arguments, options, folder),
        "delete_property" => delete_property(arguments, options, folder),
        "set_priority" => set_priority(arguments, options, folder),
        "set_effort" => set_effort(arguments, options, folder),
        "archive" => archive(arguments, options, folder),
        "delete_entry" => delete_entry(arguments, options, folder),
        _ => {
            let note = arguments.text("note").ok_or("`note` is required: the note's text")?;
            edit_entry(arguments, options, folder, &|_| Ok(Some(Command::Note)), Some(note))
        }
    }
}

/// The entry a tool acts on, read: its file, and its headline at `line`.
struct Entry {
    path: PathBuf,
    /// Its path, as the answers name it.
    name: String,
    text: String,
    /// The start of its headline.
    heading: usize,
    line: usize,
}

impl Entry {
    /// The entry of the `file` and `line` arguments, among `files`.
    fn read(arguments: &Arguments, files: &[PathBuf], kinds: &str, folder: Option<&Path>) -> Result<Entry, Call> {
        let given = arguments.required("file")?;
        let line = arguments.line()?;
        let path = editable_file(&given, files, kinds, folder)?;
        let text =
            std::fs::read_to_string(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        let name = path.display().to_string();
        let heading = headline_at_line(&text, line, &name, arguments)?;
        Ok(Entry { path, name, text, heading, line })
    }

    /// The answer of a tool that made `changed` of the entry's file:
    /// `message` (Emacs's), then what changed, written unless this is a
    /// dry run, then the entry's headline, at its line.
    fn finish(&self, arguments: &Arguments, changed: &str, message: Option<String>) -> Result<String, Call> {
        let mut out = message.map(|message| message + "\n").unwrap_or_default();
        if changed == self.text {
            out.push_str("Nothing changed.\n");
        } else if arguments.flag("dry_run") {
            out.push_str("Dry run: nothing was written. The change would be:\n");
            out.push_str(&diff(&self.name, &self.text, changed));
        } else {
            write_file(&self.path, changed)?;
        }
        let headline = changed.lines().nth(self.line - 1).unwrap_or("");
        out.push_str(&format!("{}:{}: {headline}", self.name, self.line));
        Ok(out)
    }
}

/// The settings of the editing commands, for a file whose text is `text`.
fn edit_settings(options: &Options, text: &str) -> edit::Settings {
    let mut settings = options.edit_settings();
    settings.todo.adapt = options.display().adapt(text, options.indent_settings().adapt);
    settings
}

/// What asking for a note answers, when the tool was given none.
fn wants_note(purpose: &str) -> Call {
    format!("This change asks for a note (Note for {purpose}): give its text as `note`").into()
}

/// Run on the entry the command `make` makes from its file's text and
/// parse (`None`: nothing to do), with `note` for the note it wants.
fn edit_entry(
    arguments: &Arguments,
    options: &Options,
    folder: Option<&Path>,
    make: &dyn Fn(&EntryParse) -> Result<Option<Command>, Call>,
    note: Option<String>,
) -> Result<String, Call> {
    let entry = Entry::read(arguments, &editable_files(options, folder), EDITABLE, folder)?;
    let parse = org_syntax::parse(&entry.text, &options.todo_keywords());
    let Some(command) = make(&EntryParse { entry: &entry, parse: &parse, options })? else {
        return entry.finish(arguments, &entry.text, None);
    };
    let settings = edit_settings(options, &entry.text);
    let call = edit::Call { count: None, repeated: false, now: now(), note, skip_note: false, time_only: false };
    let outcome = edit::run(&command, &entry.text, &parse, entry.heading, &call, &settings)?;
    if let Some(wanted) = outcome.note {
        return Err(wants_note(&wanted.purpose));
    }
    let message = match &command {
        // `org-entry-delete` found nothing: Org says nothing.
        Command::DeleteProperty(_) if outcome.text == entry.text => None,
        _ => outcome.message.clone().or_else(|| {
            crate::server::headline_message(
                &entry.text,
                &outcome.text,
                entry.heading,
                parse.todo_keywords(),
                false,
                None,
            )
        }),
    };
    entry.finish(arguments, &outcome.text, message)
}

/// An entry and its file's parse, for [`edit_entry`]'s command.
struct EntryParse<'a> {
    entry: &'a Entry,
    parse: &'a org_syntax::Parse,
    options: &'a Options,
}

impl EntryParse<'_> {
    /// What the property commands know of the file: its keywords,
    /// priorities, `org-global-properties` and category.
    fn property_context(&self) -> properties::Context {
        let settings = edit_settings(self.options, &self.entry.text);
        let category = self.entry.path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        edit::property_context(self.parse, &settings, &category)
    }
}

/// `org-schedule` or `org-deadline` (`command`) on the entry, with the
/// `date` argument or `given`, or `C-u` with `remove`.
fn schedule(
    arguments: &Arguments,
    options: &Options,
    folder: Option<&Path>,
    command: PlanningCommand,
    given: Option<String>,
) -> Result<String, Call> {
    let entry = Entry::read(arguments, &editable_files(options, folder), EDITABLE, folder)?;
    let text = &entry.text;
    // An empty value of the property takes the date out.
    let remove = arguments.flag("remove") || given.as_deref().is_some_and(|value| value.trim().is_empty());
    let answer = given.or_else(|| arguments.text("date")).map(|date| date.trim().to_string());
    let adapt = options.display().adapt(text, options.indent_settings().adapt);
    let mut outcome = match answer.filter(|answer| !answer.is_empty()) {
        _ if remove => planning::remove(command, text, entry.heading)?,
        Some(answer) => {
            planning::run_with_time(command, text, entry.heading, &answer, now(), &options.read_date_settings(), adapt)?
        }
        None => {
            return Err("`date` is required: a date as Org's date prompt reads it (2026-11-03, +2d, fri 10:00), \
                 or `remove: true` to take it out"
                .into())
        }
    };
    // `org-log-reschedule` and `org-log-redeadline`: a date the entry had,
    // logged with the time or a note.
    if let Some(changed) = outcome.changed.clone() {
        let settings = edit_settings(options, &outcome.text).todo;
        if let Some(logging) = todo::planning_logging(&settings, &outcome.text, changed.change) {
            let note = match logging {
                org_syntax::Logging::Note => arguments
                    .text("note")
                    .filter(|note| !note.trim().is_empty())
                    .ok_or_else(|| wants_note(changed.change.purpose()))?,
                org_syntax::Logging::Time => String::new(),
            };
            let keywords = org_syntax::parse(&outcome.text, &options.todo_keywords()).todo_keywords().clone();
            outcome.text = todo::log_planning_change(
                &outcome.text,
                &keywords,
                changed.heading,
                &settings,
                now(),
                changed.change,
                &changed.old,
                &note,
            )?;
        }
    }
    entry.finish(arguments, &outcome.text, outcome.message)
}

/// `org-set-tags-command`, with the tags given for the answer.
fn set_tags(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let given = arguments.0.get("tags").ok_or("`tags` is required: a list of tags, or [] for none")?;
    let chosen = tags::parse_tags(&answer_text(given));
    edit_entry(arguments, options, folder, &|_| Ok(Some(Command::Tags(chosen.clone()))), None)
}

/// `org-set-property`: the value read as `org-read-property-value` reads
/// it, one of the allowed values when only those will do. The dates go
/// to `org-schedule` and `org-deadline`, as `org-entry-put` sends them.
fn set_property(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let name = arguments.required("name")?.trim().to_string();
    if !properties::valid_name(&name) {
        return Err(format!("Invalid property name: \"{name}\"").into());
    }
    let value = arguments.0.get("value").map(answer_text).ok_or("`value` is required")?;
    match name.as_str() {
        "SCHEDULED" => return schedule(arguments, options, folder, PlanningCommand::Schedule, Some(value)),
        "DEADLINE" => return schedule(arguments, options, folder, PlanningCommand::Deadline, Some(value)),
        _ => {}
    }
    let make = |at: &EntryParse| {
        let question = properties::value_question(&at.entry.text, at.entry.heading, &name, &at.property_context())
            .map_err(Call::Failed)?;
        let value = question.answer(&value);
        allowed(&question, &value, &name)?;
        Ok(Some(Command::Property { name: name.clone(), value }))
    };
    edit_entry(arguments, options, folder, &make, arguments.text("note"))
}

/// Whether `value` answers `question`, which may take only the values it
/// offers.
fn allowed(question: &properties::Question, value: &str, name: &str) -> Result<(), Call> {
    if question.require_match && !question.collection.iter().any(|allowed| allowed == value) {
        return Err(format!(
            "\"{value}\" is not an allowed value of {name}, which takes {}",
            question.collection.iter().map(|value| format!("\"{value}\"")).collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    Ok(())
}

/// `org-delete-property`.
fn delete_property(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let name = arguments.required("name")?.trim().to_string();
    let make = |at: &EntryParse| {
        // The entry's properties, which Org completes from and requires.
        let names = match properties::delete_question(&at.entry.text, at.entry.heading) {
            Ok(question) => question.collection,
            Err(one) if one.is_empty() => Vec::new(),
            Err(one) => vec![one],
        };
        if names.is_empty() {
            return Err("No property to delete in this entry".into());
        }
        if !names.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            return Err(format!("This entry has no property {name}: it has {}", names.join(", ")).into());
        }
        Ok(Some(Command::DeleteProperty(name.clone())))
    };
    edit_entry(arguments, options, folder, &make, None)
}

/// `org-priority` with the priority given for the key: a space for `""`.
fn set_priority(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let given = arguments.0.get("priority").map(answer_text).ok_or("`priority` is required, or \"\" to remove it")?;
    let given = given.trim().to_string();
    let make = |at: &EntryParse| {
        let settings = edit_settings(at.options, &at.entry.text);
        let priorities = edit::document_priorities(at.parse).unwrap_or(settings.priorities);
        if !priorities.numeric() && given.chars().count() > 1 {
            return Err(format!("A priority is one letter, from {}", priority::question(&priorities)).into());
        }
        let answer = if given.is_empty() { " " } else { given.as_str() };
        Ok(Some(Command::Priority(priority::read_answer(answer, &priorities)?)))
    };
    edit_entry(arguments, options, folder, &make, arguments.text("note"))
}

/// `org-set-effort` with the value given, one of the allowed values when
/// only those will do.
fn set_effort(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let value = arguments.0.get("value").map(answer_text).ok_or("`value` is required: an effort such as 1:00")?;
    let value = value.trim().to_string();
    let make = |at: &EntryParse| {
        let question = properties::effort_question(&at.entry.text, at.entry.heading, &at.property_context())
            .map_err(Call::Failed)?;
        allowed(&question, &value, properties::EFFORT)?;
        Ok(Some(Command::Effort(Some(value.clone()))))
    };
    edit_entry(arguments, options, folder, &make, None)
}

// ----- archive

/// The title of the headline that starts `line`, as Org has it, without
/// keyword, priority and tags.
pub(super) fn title(line: &str, keywords: &org_syntax::TodoKeywords) -> Option<String> {
    let parse = org_syntax::parse(line, keywords);
    Some(org_syntax::ast::headline_at(&parse.syntax(), 0.into())?.title_text().trim().to_string())
}

/// Where the headline titled `wanted` is in `text`: the first from the
/// line `from` (from 0) on, its line from 1 and its text.
fn find_headline(
    text: &str,
    from: usize,
    wanted: &str,
    keywords: &org_syntax::TodoKeywords,
) -> Option<(usize, String)> {
    text.lines()
        .enumerate()
        .skip(from)
        .find(|(_, line)| is_headline(line) && title(line, keywords).as_deref() == Some(wanted))
        .map(|(number, line)| (number + 1, line.to_string()))
}

/// The first line (from 0) where `old` and `new` differ.
fn first_change(old: &str, new: &str) -> usize {
    old.lines().zip(new.lines()).take_while(|(a, b)| a == b).count()
}

/// `org-archive-subtree`, `org-archive-to-archive-sibling` or
/// `org-toggle-archive-tag`, after `method`. Only an agenda file is
/// archived from, and only it and its archive files are written: the
/// archive file first, so that a failure between the two writes leaves
/// the entry in both, never in none.
fn archive(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    let method = arguments.text("method").filter(|method| !method.trim().is_empty());
    let method = method.as_deref().map(str::trim).unwrap_or("subtree");
    if !matches!(method, "subtree" | "to_sibling" | "toggle_tag") {
        return Err(format!("`method` is subtree, to_sibling or toggle_tag, not {method}").into());
    }
    let agenda = agenda_paths(options, folder);
    let entry = Entry::read(arguments, &agenda, "not an agenda file", folder)?;
    let text = &entry.text;
    let keywords = org_syntax::parse(text, &options.todo_keywords()).todo_keywords().clone();
    let adapt = options.display().adapt(text, options.indent_settings().adapt);
    let defaults = options.archive_settings();
    let settings = archive::Settings {
        levels: org_core::refile::Settings { adapt, ..defaults.levels },
        todo: todo::Settings { adapt, ..defaults.todo.clone() },
        ..defaults
    };
    let now = now();
    let headline = text[entry.heading..].lines().next().unwrap_or("");
    let wanted = title(headline, &keywords).unwrap_or_default();
    let archived = match method {
        "toggle_tag" => {
            let (source, message) = archive::toggle_tag(text, entry.heading, &settings)?;
            return entry.finish(arguments, &source, Some(message));
        }
        "to_sibling" => {
            let (source, _) = archive::to_sibling(text, entry.heading, &keywords, now, &settings)?;
            archive::Archived { source, files: Vec::new(), message: String::new(), cursor: 0 }
        }
        _ => {
            let path = std::path::absolute(&entry.path).unwrap_or_else(|_| entry.path.clone());
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
            let context = archive::Context { path: &path, home: home.as_deref(), now };
            // The files it archives to, which it may write besides the
            // agenda files.
            let mut writable: Vec<PathBuf> = agenda.iter().map(|file| canonical(file)).collect();
            writable.extend(
                archive::archive_files(text, &keywords, &context, &settings.location)
                    .iter()
                    .map(|file| canonical(file)),
            );
            let mut read = |wanted: &Path| std::fs::read_to_string(wanted).ok();
            let archived = archive::archive_subtree(text, entry.heading, &keywords, &context, &settings, &mut read)?;
            if let Some(other) = archived.files.iter().find(|file| !writable.contains(&canonical(&file.path))) {
                return Err(format!(
                    "{} is neither an agenda file nor one of its archive files: org-lsp mcp changes only those",
                    other.path.display()
                )
                .into());
            }
            archived
        }
    };

    let mut out = if archived.message.is_empty() { String::new() } else { format!("{}\n", archived.message) };
    let mut changes: Vec<(&Path, String, Option<&str>, &str)> = archived
        .files
        .iter()
        .map(|file| (file.path.as_path(), file.path.display().to_string(), file.before.as_deref(), file.text.as_str()))
        .collect();
    changes.push((&entry.path, entry.name.clone(), Some(text.as_str()), archived.source.as_str()));
    changes.retain(|(_, _, before, after)| before.unwrap_or("") != *after || before.is_none());
    if changes.is_empty() {
        out.push_str("Nothing changed.\n");
    } else if arguments.flag("dry_run") {
        out.push_str("Dry run: nothing was written. The change would be:\n");
        for (_, name, before, after) in &changes {
            out.push_str(&diff(name, before.unwrap_or(""), after));
        }
    } else {
        for (path, _, _, after) in &changes {
            write_file(path, after)?;
        }
    }
    // Where the entry is now.
    let found = match archived.files.iter().find(|file| file.before.as_deref() != Some(file.text.as_str())) {
        Some(file) => {
            let from = first_change(file.before.as_deref().unwrap_or(""), &file.text);
            find_headline(&file.text, from, &wanted, &keywords).map(|found| (file.path.display().to_string(), found))
        }
        None if archived.files.is_empty() => {
            find_headline(&archived.source, entry.line - 1, &wanted, &keywords).map(|found| (entry.name.clone(), found))
        }
        None => None,
    };
    if let Some((name, (line, headline))) = found {
        out.push_str(&format!("{name}:{line}: {headline}"));
    }
    Ok(out.trim_end().to_string())
}

// ----- delete

/// `org-cut-subtree` on the entry, its text not kept anywhere but in the
/// answer. The `headline` argument is required: the entry deleted must be
/// the one meant.
fn delete_entry(arguments: &Arguments, options: &Options, folder: Option<&Path>) -> Result<String, Call> {
    if !arguments.text("headline").is_some_and(|headline| !headline.trim().is_empty()) {
        return Err("`headline` is required: text the headline has, such as its title, so that the entry \
                    deleted is the one meant"
            .into());
    }
    let entry = Entry::read(arguments, &agenda_paths(options, folder), "not an agenda file", folder)?;
    let (outcome, cut, message) = structure::copy_subtree(&entry.text, entry.heading, 1, true)?;
    let mut out = format!("{message}\n");
    if arguments.flag("dry_run") {
        out.push_str("Dry run: nothing was written. The change would be:\n");
        out.push_str(&diff(&entry.name, &entry.text, &outcome.text));
        out.push_str("The text it would delete:\n");
    } else {
        write_file(&entry.path, &outcome.text)?;
        out.push_str(&format!(
            "Deleted from {}:{}. The text deleted, which put back at that line restores the entry:\n",
            entry.name, entry.line
        ));
    }
    out.push_str(&cut);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::answer;
    use serde_json::{json, Value};

    /// A folder with `files`, `tasks.org` its agenda file, and a
    /// configuration file with `mcp_edits` on and `config`.
    struct Setup {
        folder: std::path::PathBuf,
        config: std::path::PathBuf,
    }

    impl Setup {
        fn new(name: &str, config: &str, files: &[(&str, &str)]) -> Setup {
            let folder = std::env::temp_dir().join(format!("org-lsp-mcp-entry-{name}-{}", std::process::id()));
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
                format!("org_agenda_files = [{agenda:?}]\norg_directory = {directory:?}\nmcp_edits = true\n{config}\n"),
            )
            .unwrap();
            Setup { folder, config: path }
        }

        /// The text a tool answers, and whether it is an error.
        fn call(&self, name: &str, arguments: Value) -> (String, bool) {
            let config = self.config.display().to_string();
            let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
            let params = json!({ "name": name, "arguments": arguments });
            let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params }).to_string();
            let answer = answer(&line, Some(&self.folder), &env).expect("an answer");
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

    const TASKS: &str = "* Projects\n** TODO Write the report :work:\n** TODO Water the plants\n\
                         SCHEDULED: <2026-10-21 Wed>\n";

    #[test]
    fn the_tools_are_listed_with_mcp_edits_and_refused_without() {
        let setup = Setup::new("list", "", &[("tasks.org", TASKS)]);
        let config = setup.config.display().to_string();
        let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
        let list = answer(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, Some(&setup.folder), &env).unwrap();
        let names: Vec<&str> =
            list["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        for name in super::NAMES {
            assert!(names.contains(&name), "{name} in {names:?}");
        }
        std::fs::write(&setup.config, "").unwrap();
        let list = answer(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, Some(&setup.folder), &env).unwrap();
        let names: Vec<&str> =
            list["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        assert!(!names.contains(&"schedule") && !names.contains(&"archive"), "{names:?}");
        for name in super::NAMES {
            let (text, error) = setup.call(name, json!({ "file": "tasks.org", "line": 2 }));
            assert!(error && text.contains("mcp_edits = true"), "{name}: {text}");
        }
    }

    #[test]
    fn schedules_and_takes_out_deadlines() {
        let config = "org_log_reschedule = \"time\"\norg_log_redeadline = \"note\"\norg_read_date_prefer_future = true";
        let setup = Setup::new("schedule", config, &[("tasks.org", TASKS)]);
        let arguments = json!({ "file": "tasks.org", "line": 2, "date": "2026-11-03 10:00", "headline": "report" });
        let (text, error) = setup.call("schedule", arguments);
        assert!(!error, "{text}");
        assert!(text.starts_with("Scheduled to <2026-11-03 Tue 10:00>\n"), "{text}");
        assert!(text.ends_with(":2: ** TODO Write the report :work:"), "{text}");
        assert!(setup.read("tasks.org").contains(":work:\nSCHEDULED: <2026-11-03 Tue 10:00>\n"));

        // Rescheduled: the old date logged, with the time.
        let (text, error) = setup.call("schedule", json!({ "file": "tasks.org", "line": 4, "date": "2026-10-30" }));
        assert!(!error, "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.contains("SCHEDULED: <2026-10-30 Fri>\n- Rescheduled from \"[2026-10-21 Wed]\" on ["), "{tasks}");

        // A dry run of a deadline with a repeater.
        let arguments = json!({ "file": "tasks.org", "line": 2, "date": "<2026-11-20 +1m>", "dry_run": true });
        let (text, error) = setup.call("deadline", arguments);
        assert!(!error && text.starts_with("Deadline on <2026-11-20 Fri +1m>"), "{text}");
        assert!(text.contains("+DEADLINE: <2026-11-20 Fri +1m> SCHEDULED: <2026-11-03 Tue 10:00>"), "{text}");
        assert_eq!(setup.read("tasks.org"), tasks);
        let arguments = json!({ "file": "tasks.org", "line": 2, "date": "2026-11-20" });
        assert!(!setup.call("deadline", arguments).1);

        // Taking out a deadline asks for a note, as org_log_redeadline says.
        let removed = json!({ "file": "tasks.org", "line": 2, "remove": true });
        let (text, error) = setup.call("deadline", removed.clone());
        assert!(error && text.contains("Note for removing deadline") && text.contains("`note`"), "{text}");
        let mut noted = removed;
        noted["note"] = json!("Not needed");
        let (text, error) = setup.call("deadline", noted);
        assert!(!error && text.starts_with("Entry no longer has a deadline."), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(!tasks.contains("DEADLINE"), "{tasks}");
        assert!(tasks.contains("- Removed deadline, was \"[2026-11-20 Fri]\" on [") && tasks.contains("Not needed"));

        // No date, and not a headline.
        let (text, error) = setup.call("schedule", json!({ "file": "tasks.org", "line": 2 }));
        assert!(error && text.contains("`date` is required"), "{text}");
        let (text, error) = setup.call("schedule", json!({ "file": "tasks.org", "line": 4, "date": "+1d" }));
        assert!(error && text.contains("not a headline"), "{text}");
    }

    #[test]
    fn sets_tags_properties_priorities_and_efforts() {
        let config = "org_tags_column = -40\norg_global_properties = { Effort_ALL = \"0:10 0:30 1:00\" }";
        let tasks = "* TODO Write the report :work:\n:PROPERTIES:\n:COLOR_ALL: red blue\n:END:\n";
        let setup = Setup::new("properties", config, &[("tasks.org", tasks)]);
        let (text, error) =
            setup.call("set_tags", json!({ "file": "tasks.org", "line": 1, "tags": ["home", "urgent"] }));
        assert!(!error, "{text}");
        assert!(text.ends_with(":1: * TODO Write the report    :home:urgent:"), "{text}");
        let (text, error) =
            setup.call("set_tags", json!({ "file": "tasks.org", "line": 1, "tags": "", "dry_run": true }));
        assert!(!error && text.contains("+* TODO Write the report\n"), "{text}");

        let arguments = json!({ "file": "tasks.org", "line": 1, "name": "COLOR", "value": "red" });
        let (text, error) = setup.call("set_property", arguments);
        assert!(!error, "{text}");
        assert!(setup.read("tasks.org").contains(":COLOR_ALL: red blue\n:COLOR:    red\n:END:"));
        let arguments = json!({ "file": "tasks.org", "line": 1, "name": "COLOR", "value": "green" });
        let (text, error) = setup.call("set_property", arguments);
        assert!(error && text.contains("not an allowed value of COLOR"), "{text}");
        let arguments = json!({ "file": "tasks.org", "line": 1, "name": "TODO", "value": "DONE" });
        let (text, error) = setup.call("set_property", arguments);
        assert!(!error && text.ends_with(":1: * DONE Write the report    :home:urgent:"), "{text}");
        let arguments = json!({ "file": "tasks.org", "line": 1, "name": "SCHEDULED", "value": "2026-10-23" });
        let (text, error) = setup.call("set_property", arguments);
        assert!(!error && text.starts_with("Scheduled to <2026-10-23 Fri>"), "{text}");

        let (text, error) = setup.call("delete_property", json!({ "file": "tasks.org", "line": 1, "name": "COLOR" }));
        assert!(!error && text.starts_with("Property \"COLOR\" deleted\n"), "{text}");
        assert!(!setup.read("tasks.org").contains(":COLOR:"), "{}", setup.read("tasks.org"));
        let (text, error) = setup.call("delete_property", json!({ "file": "tasks.org", "line": 1, "name": "SIZE" }));
        assert!(error && text.contains("has no property SIZE"), "{text}");

        let (text, error) = setup.call("set_priority", json!({ "file": "tasks.org", "line": 1, "priority": "a" }));
        assert!(!error && text.starts_with("Priority of current item set to A\n"), "{text}");
        assert!(text.contains(":1: * DONE [#A] Write the report"), "{text}");
        let (text, error) = setup.call("set_priority", json!({ "file": "tasks.org", "line": 1, "priority": "Z" }));
        assert!(error, "{text}");
        let (text, error) = setup.call("set_priority", json!({ "file": "tasks.org", "line": 1, "priority": "" }));
        assert!(!error && text.starts_with("Priority removed\n"), "{text}");

        let (text, error) = setup.call("set_effort", json!({ "file": "tasks.org", "line": 1, "value": "0:30" }));
        assert!(!error && text.starts_with("Effort is now 0:30\n"), "{text}");
        assert!(setup.read("tasks.org").contains(":Effort:   0:30\n"), "{}", setup.read("tasks.org"));
        let (text, error) = setup.call("set_effort", json!({ "file": "tasks.org", "line": 1, "value": "0:45" }));
        assert!(error && text.contains("not an allowed value of Effort"), "{text}");
    }

    #[test]
    fn adds_notes_as_org_logs_them() {
        let config = "org_log_into_drawer = true";
        let setup = Setup::new("note", config, &[("tasks.org", TASKS)]);
        let arguments = json!({ "file": "tasks.org", "line": 3, "note": "Bought more soil", "dry_run": true });
        let (text, error) = setup.call("add_note", arguments);
        assert!(!error && text.contains("+:LOGBOOK:\n+- Note taken on ["), "{text}");
        assert_eq!(setup.read("tasks.org"), TASKS);
        let (text, error) =
            setup.call("add_note", json!({ "file": "tasks.org", "line": 3, "note": "Bought more soil" }));
        assert!(!error && text.ends_with(":3: ** TODO Water the plants"), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.contains("SCHEDULED: <2026-10-21 Wed>\n:LOGBOOK:\n- Note taken on ["), "{tasks}");
        assert!(tasks.contains("\\\\\n  Bought more soil\n:END:\n"), "{tasks}");
        let (text, error) = setup.call("add_note", json!({ "file": "tasks.org", "line": 3 }));
        assert!(error && text.contains("`note` is required"), "{text}");
    }

    #[test]
    fn archives_to_the_archive_file_a_sibling_or_with_the_tag() {
        let setup = Setup::new("archive", "org_archive_save_context_info = []", &[("tasks.org", TASKS)]);
        let path = setup.folder.join("tasks.org_archive").display().to_string();
        let arguments = json!({ "file": "tasks.org", "line": 2, "dry_run": true });
        let (text, error) = setup.call("archive", arguments);
        assert!(!error && text.contains("+* TODO Write the report"), "{text}");
        assert!(!setup.folder.join("tasks.org_archive").exists());
        // The archive file's diff first, as it is written first.
        assert!(text.find("tasks.org_archive\n+++").unwrap() < text.find("tasks.org\n+++").unwrap(), "{text}");

        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 2, "headline": "report" }));
        assert!(!error, "{text}");
        assert!(text.starts_with(&format!("Subtree archived in file: {path}")), "{text}");
        assert!(text.contains(&format!("\n{path}:7: * TODO Write the report ")), "{text}");
        let archive = setup.read("tasks.org_archive");
        assert!(archive.contains("Archived entries from file") && archive.ends_with(" :work:\n"), "{archive}");
        assert_eq!(setup.read("tasks.org"), "* Projects\n** TODO Water the plants\nSCHEDULED: <2026-10-21 Wed>\n");

        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 2, "method": "toggle_tag" }));
        assert!(!error && text.starts_with("Subtree archived\n"), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.contains("** TODO Water the plants ") && tasks.contains(" :ARCHIVE:\n"), "{tasks}");
        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 2, "method": "toggle_tag" }));
        assert!(!error && text.starts_with("Subtree unarchived\n"), "{text}");

        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 2, "method": "to_sibling" }));
        assert!(!error, "{text}");
        assert!(text.ends_with("tasks.org:3: *** TODO Water the plants"), "{text}");
        let tasks = setup.read("tasks.org");
        assert!(tasks.starts_with("* Projects\n** Archive "), "{tasks}");
        assert!(tasks.contains(":ARCHIVE:\n*** TODO Water the plants\n"), "{tasks}");
    }

    #[test]
    fn deletes_an_entry_with_its_subtree_as_org_cut_subtree() {
        let tasks = "* Projects\n** TODO Write the report :work:\n*** Outline\nText.\n\n** TODO Water the plants\n";
        let setup = Setup::new("delete", "", &[("tasks.org", tasks), ("other.org", "* TODO Elsewhere\n")]);
        let list = setup.call("delete_entry", json!({}));
        assert!(list.1, "{list:?}");

        // The headline is required, and must be the line's.
        let (text, error) = setup.call("delete_entry", json!({ "file": "tasks.org", "line": 2 }));
        assert!(error && text.contains("`headline` is required"), "{text}");
        let arguments = json!({ "file": "tasks.org", "line": 2, "headline": "Water" });
        let (text, error) = setup.call("delete_entry", arguments);
        assert!(error && text.contains("not the entry `Water`"), "{text}");
        let arguments = json!({ "file": "other.org", "line": 1, "headline": "Elsewhere" });
        let (text, error) = setup.call("delete_entry", arguments);
        assert!(error && text.contains("not an agenda file"), "{text}");
        assert_eq!(setup.read("tasks.org"), tasks);

        let arguments = json!({ "file": "tasks.org", "line": 2, "headline": "report", "dry_run": true });
        let (text, error) = setup.call("delete_entry", arguments);
        assert!(!error && text.starts_with("Cut: Subtree(s) with 51 characters\nDry run"), "{text}");
        assert!(
            text.contains("-*** Outline\n")
                && text.ends_with("would delete:\n** TODO Write the report :work:\n*** Outline\nText.\n\n")
        );
        assert_eq!(setup.read("tasks.org"), tasks);

        // Its subtree goes, with the blank line before the next headline.
        let (text, error) = setup.call("delete_entry", json!({ "file": "tasks.org", "line": 2, "headline": "report" }));
        assert!(!error && text.starts_with("Cut: Subtree(s) with 51 characters\nDeleted from "), "{text}");
        assert!(
            text.ends_with(
                ":2. The text deleted, which put back at that line restores the entry:\n\
                                ** TODO Write the report :work:\n*** Outline\nText.\n\n"
            ),
            "{text}"
        );
        assert_eq!(setup.read("tasks.org"), "* Projects\n** TODO Water the plants\n");

        std::fs::write(&setup.config, format!("org_agenda_files = [{:?}]", setup.folder.join("tasks.org"))).unwrap();
        let (text, error) = setup.call("delete_entry", json!({ "file": "tasks.org", "line": 2, "headline": "Water" }));
        assert!(error && text.contains("mcp_edits = true"), "{text}");
        assert_eq!(setup.read("tasks.org"), "* Projects\n** TODO Water the plants\n");
    }

    #[test]
    fn delete_entry_is_destructive() {
        let setup = Setup::new("delete-hint", "", &[("tasks.org", TASKS)]);
        let config = setup.config.display().to_string();
        let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
        let list = answer(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, Some(&setup.folder), &env).unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        let delete = tools.iter().find(|tool| tool["name"] == "delete_entry").expect("delete_entry");
        assert_eq!(delete["annotations"]["destructiveHint"], true);
        assert_eq!(delete["annotations"]["readOnlyHint"], false);
        assert_eq!(delete["inputSchema"]["required"], json!(["file", "line", "headline"]));
    }

    #[test]
    fn archives_only_to_the_archive_files_of_agenda_files() {
        let tasks = "#+ARCHIVE: elsewhere.org::\n* TODO Done with\n";
        let setup = Setup::new("archive-where", "", &[("tasks.org", tasks), ("other.org", "* TODO Elsewhere\n")]);
        let (text, error) = setup.call("archive", json!({ "file": "other.org", "line": 1 }));
        assert!(error && text.contains("not an agenda file"), "{text}");
        assert_eq!(setup.read("other.org"), "* TODO Elsewhere\n");
        // The file's #+ARCHIVE names its archive file.
        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 2 }));
        assert!(!error && text.contains("elsewhere.org"), "{text}");
        assert!(setup.read("elsewhere.org").contains("* TODO Done with"));
        assert_eq!(setup.read("tasks.org"), "#+ARCHIVE: elsewhere.org::\n");
        let (text, error) = setup.call("archive", json!({ "file": "tasks.org", "line": 1, "method": "nope" }));
        assert!(error && text.contains("`method`"), "{text}");
    }
}
