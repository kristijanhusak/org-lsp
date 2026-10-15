//! org-journal, when `org_journal_enabled` is on: the commands that make
//! entries in the journal's files and move between them, run on the files
//! of `org_journal_dir` as the editor has them. The files org-journal
//! changes get edits when the editor has them open and are written when
//! not; the cursor goes where org-journal leaves the point, with the
//! headlines folded as it folds them (`org-lsp/folds`).

use std::collections::BTreeMap;
use std::path::Path;

use lsp_types::request::{Request as _, ShowDocument};
use lsp_types::{FoldingRange, Position, Range, ShowDocumentParams, TextDocumentIdentifier, Uri};
use org_core::journal::{self, Command, Outcome, Period, Request, Stop, View};
use org_core::Refusal;
use serde::Deserialize;
use serde_json::{json, Value};

use super::roam::{expand, list_files};
use super::{Pending, Server};
use crate::config::JournalSettings;
use crate::line_index::LineIndex;
use crate::{commands, folding, uri};

/// The files of the search results and of the schedule.
const SEARCH_FILE: &str = "org-journal-search.org";
const SCHEDULE_FILE: &str = "org-journal-schedule.org";

/// What the commands say while org-journal is off.
const OFF: &str = "org-journal is off: set org_journal_enabled to use it";

/// What a journal command is run with.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JournalContext {
    text_document: Option<TextDocumentIdentifier>,
    position: Option<Position>,
    /// The prefix argument: `C-u` is 4.
    count: Option<i64>,
    input: Option<String>,
    /// What `input` answers: `directory`, whether to make the directory;
    /// `question`, a question of org-journal's.
    step: Option<String>,
    /// The answers to org-journal's questions so far, `y` or `n` each.
    answers: Option<String>,
    /// The date read for a command that asked for one.
    date: Option<String>,
    /// The string a search looks for, and its first and last day.
    search: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

/// A question to type the answer of: `kind` `text` or `date`.
fn ask_input(kind: &str, prompt: &str, context: Value) -> Value {
    json!({ "input": { "kind": kind, "prompt": prompt, "value": "", "context": context } })
}

/// A yes-or-no question, answered with `input` and `context` sent back.
fn ask(prompt: &str, context: Value) -> Value {
    json!({ "input": {
        "kind": "key",
        "prompt": prompt,
        "value": "",
        "choices": [{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }],
        "context": context,
    } })
}

fn yes(input: &str) -> bool {
    matches!(input, "y" | "Y" | "yes" | " ")
}

impl Server {
    /// An `org.journal.*` command, run with `argument`.
    pub(super) fn journal_command(&mut self, id: &str, argument: &Value) -> Result<Value, Refusal> {
        let journal = self.journal.clone().ok_or(OFF)?;
        let context: JournalContext = serde_json::from_value(argument.clone()).unwrap_or_default();
        let directory = expand(&journal.directory);
        // `org-journal--create-journal-dir`
        if !directory.is_dir() {
            let shown = format!("{}/", directory.display()).replace("//", "/");
            match (context.step.as_deref(), context.input.as_deref()) {
                (Some("directory"), Some(answer)) if yes(answer) => {
                    std::fs::create_dir_all(&directory).map_err(|error| format!("Cannot make {shown}: {error}"))?
                }
                (Some("directory"), _) => return Err("A journal directory is necessary to use org-journal".into()),
                _ => {
                    let prompt = format!("Journal directory {shown} doesn't exists. Create it? ");
                    return Ok(ask(&prompt, json!({ "step": "directory" })));
                }
            }
        }
        if let Some(asked) = self.journal_search_input(id, &context)? {
            return Ok(asked);
        }
        let dated = matches!(
            id,
            commands::JOURNAL_NEW_DATE_ENTRY | commands::JOURNAL_NEW_SCHEDULED_ENTRY | commands::JOURNAL_RESCHEDULE
        );
        let date = match (&context.date, context.step.as_deref(), &context.input) {
            (Some(date), _, _) => Some(date.clone()),
            (None, None, Some(input)) if dated => Some(input.clone()),
            _ => None,
        };
        if dated && date.is_some() {
            // Answered: a date prompt still open for it is done with.
            let _ = self.date_prompt.take_if(|open| open.id == id);
        }
        if dated && date.is_none() {
            // The date prompt, whose answer comes back through
            // `journal_date`.
            let state = json!({ "position": context.position, "count": context.count });
            return self.journal_ask_date(id, &context, state, None);
        }
        self.journal_run(id, &journal, &directory, &context, date.as_deref())
    }

    /// What a search still has to ask: the string, then the first and the
    /// last day (`org-journal--read-period`), each answer carried in the
    /// context of the next question.
    fn journal_search_input(&mut self, id: &str, context: &JournalContext) -> Result<Option<Value>, Refusal> {
        if !matches!(id, commands::JOURNAL_SEARCH | commands::JOURNAL_SEARCH_FOREVER | commands::JOURNAL_SEARCH_FUTURE)
        {
            return Ok(None);
        }
        let answer = |step: Option<&str>| match (context.step.as_deref(), &context.input) {
            (found, Some(input)) if found == step => Some(input.clone()),
            _ => None,
        };
        let Some(search) = context.search.clone().or_else(|| answer(None)).or_else(|| answer(Some("search"))) else {
            let prompt = "Enter a string to search for";
            return Ok(Some(ask_input("text", prompt, json!({ "step": "search", "count": context.count }))));
        };
        if id != commands::JOURNAL_SEARCH || context.count.is_some_and(|count| count > 1) {
            return Ok(None);
        }
        let start = context.start.clone().or_else(|| answer(Some("start")));
        let end = context.end.clone().or_else(|| answer(Some("end")));
        let (step, prompt) = match (&start, &end) {
            (None, _) => ("start", "Enter the search start"),
            (Some(_), None) => ("end", "Enter the search end"),
            _ => return Ok(None),
        };
        let state = json!({ "search": search, "start": start, "step": step, "position": context.position });
        self.journal_ask_date(id, context, state, Some(prompt)).map(Some)
    }

    /// Ask for the date of `id`: in the calendar when the editor shows it,
    /// else with its input.
    fn journal_ask_date(
        &mut self,
        id: &str,
        context: &JournalContext,
        state: Value,
        prompt: Option<&str>,
    ) -> Result<Value, Refusal> {
        self.journal_state = Some((id.to_string(), state.clone()));
        if let (Some(prompt), false) = (prompt, self.client.date_prompt) {
            return Ok(ask_input("date", prompt, state));
        }
        let from = context
            .text_document
            .as_ref()
            .map(|document| document.uri.clone())
            .or_else(|| uri::from_path(&std::env::temp_dir()))
            .ok_or("No place to run the command from")?;
        let position = context.position.unwrap_or_default();
        let lsp_context: super::Context =
            serde_json::from_value(json!({ "textDocument": { "uri": from.as_str() }, "position": position }))
                .map_err(|error| error.to_string())?;
        let asked = self.ask_date(id, &lsp_context)?;
        // The calendar's prompt is the search's: `Enter the search start
        // Date+time [2026-10-10]: `.
        if let Some(prompt) = prompt {
            let (now, settings) = (self.date_time(), self.read_date);
            if let Some(open) = self.date_prompt.as_mut().filter(|open| open.id == id) {
                open.label = format!("{prompt} {}", open.label);
                let typed = open.typed.clone();
                open.set_typed(typed, now, &settings);
                self.publish_date_prompt();
            }
        }
        Ok(asked)
    }

    /// The answer of the date prompt of `id`, asked from `from`.
    pub(super) fn journal_date(&mut self, id: &str, from: &Uri, answer: &str) -> Result<(), Refusal> {
        let state = self.journal_state.take().filter(|(pending, _)| pending == id).map(|(_, state)| state);
        let mut argument = state.filter(Value::is_object).unwrap_or_else(|| json!({}));
        argument["textDocument"] = json!({ "uri": from.as_str() });
        // A search's day answers its step; the others' date is the date.
        if argument.get("step").is_some_and(Value::is_string) {
            argument["input"] = json!(answer);
        } else {
            argument["date"] = json!(answer);
        }
        let value = self.journal_command(id, &argument)?;
        // A question that comes after the date: asked with a message, as
        // the prompt is gone.
        if value.get("input").is_some() || value.get("message").is_some() {
            self.prompt_with_messages(id.to_string(), argument, &value);
        }
        Ok(())
    }

    /// Run the command `id` on the journal in `directory`.
    fn journal_run(
        &mut self,
        id: &str,
        journal: &JournalSettings,
        directory: &Path,
        context: &JournalContext,
        date: Option<&str>,
    ) -> Result<Value, Refusal> {
        let now = self.date_time();
        let read = match date {
            Some(answer) => Some(org_core::read_date::read_date(answer, None, now, &self.read_date)?),
            None => None,
        };
        let prefix = context.count.is_some_and(|count| count > 1);
        let command = match id {
            commands::JOURNAL_NEW_ENTRY => Command::NewEntry { prefix },
            commands::JOURNAL_NEW_DATE_ENTRY => {
                Command::NewDateEntry { prefix, date: read.clone().ok_or("No date was read")? }
            }
            commands::JOURNAL_NEW_SCHEDULED_ENTRY => {
                Command::NewScheduledEntry { prefix, date: read.clone().ok_or("No date was read")? }
            }
            commands::JOURNAL_NEXT_ENTRY => Command::Next,
            commands::JOURNAL_PREVIOUS_ENTRY => Command::Previous,
            commands::JOURNAL_OPEN_CURRENT => Command::OpenCurrent,
            commands::JOURNAL_RESCHEDULE => Command::Reschedule { date: read.clone().ok_or("No date was read")? },
            commands::JOURNAL_SCHEDULE_VIEW => Command::ScheduleView,
            commands::JOURNAL_SEARCH_FUTURE_SCHEDULED => {
                Command::Search { string: "TODO".to_string(), period: Period::Future }
            }
            commands::JOURNAL_SEARCH | commands::JOURNAL_SEARCH_FOREVER | commands::JOURNAL_SEARCH_FUTURE => {
                let string = context.search.clone().or_else(|| context.input.clone()).unwrap_or_default();
                let day = |answer: &Option<String>| -> Result<org_core::date::Date, Refusal> {
                    let answer = answer.as_deref().ok_or("No date was read")?;
                    Ok(org_core::read_date::read_date(answer, None, now, &self.read_date)?.date)
                };
                let period = match id {
                    commands::JOURNAL_SEARCH_FUTURE => Period::Future,
                    commands::JOURNAL_SEARCH if !prefix => {
                        let end = context.end.clone().or_else(|| context.input.clone());
                        Period::Between(day(&context.start)?, day(&end)?)
                    }
                    _ => Period::Forever,
                };
                Command::Search { string, period }
            }
            other => return Err(format!("org-lsp has no command `{other}`").into()),
        };
        let mut settings = journal.core.clone();
        let shown = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
        settings.directory = format!("{}/", shown.display()).replace("//", "/");
        settings.properties =
            org_core::edit::property_context(&org_syntax::parse("", &self.todo), &self.edit_settings, "");
        settings.tags.tags_column = self.refile.2.tags_column;
        // The files of the directory that are journal files, as the editor
        // has them, and the one the command is run in.
        let from = context.text_document.as_ref().and_then(|document| {
            let path = uri::to_path(&document.uri)?;
            let relative = path.strip_prefix(directory).ok()?.to_string_lossy().replace('\\', "/");
            Some((document.uri.clone(), relative))
        });
        let mut names = Vec::new();
        list_files(directory, "", 0, &mut names);
        let mut files = BTreeMap::new();
        for name in names {
            let is_from = from.as_ref().is_some_and(|(_, relative)| *relative == name);
            if is_from || journal::is_journal_file(&name, &settings) {
                if let Some(text) = self.journal_read(&directory.join(&name)) {
                    files.insert(name, text);
                }
            }
        }
        let position = context.position.unwrap_or_default();
        let from_offset = from.as_ref().and_then(|(uri, relative)| match self.document(uri) {
            Some(document) => {
                let offset = document.index.offset(&document.text, position, self.client.encoding);
                files.insert(relative.clone(), document.text.clone());
                Some((relative.clone(), offset))
            }
            // A file the editor names without having it open, as on disk.
            None => {
                let text = files.get(relative)?;
                Some((relative.clone(), LineIndex::new(text).offset(text, position, self.client.encoding)))
            }
        });
        let mut answered = context.answers.clone().unwrap_or_default();
        if context.step.as_deref() == Some("question") {
            answered.push(if yes(context.input.as_deref().unwrap_or("")) { 'y' } else { 'n' });
        }
        let answers: Vec<bool> = answered.chars().map(|answer| answer == 'y').collect();
        let request = Request {
            files: &files,
            now,
            from: from_offset.as_ref().map(|(file, offset)| (file.as_str(), *offset)),
            answers: &answers,
        };
        // `org-journal-find-file-fn`, which the commands that visit a day's
        // file call: `org-journal-new-entry` when the file is not the one
        // it is run in, `org-journal-open-current-journal-file` always.
        let finds_file = matches!(
            command,
            Command::NewEntry { .. }
                | Command::NewDateEntry { .. }
                | Command::NewScheduledEntry { .. }
                | Command::OpenCurrent
        );
        let other_window = journal.find_file_other_window && finds_file;
        let from_file = from.as_ref().map(|(_, relative)| relative.clone());
        match journal::run(&command, &request, &settings) {
            Ok(outcome) => {
                let other_window = other_window
                    && (matches!(command, Command::OpenCurrent)
                        || outcome.visit.as_ref().is_some_and(|visit| Some(&visit.file) != from_file.as_ref()));
                self.journal_apply(id, directory, &files, outcome, journal.core.hide_entries, other_window)
            }
            Err(Stop::Ask(question)) => {
                let mut sent = json!({ "step": "question", "answers": answered });
                if let Some(date) = date {
                    sent["date"] = json!(date);
                }
                Ok(ask(&question, sent))
            }
            Err(Stop::Error(message)) => Err(message.into()),
        }
    }

    /// The days of the journal's entries, which the calendar marks
    /// (`org-journal-mark-entries`, on `calendar-today-visible-hook`).
    pub(super) fn journal_days(&self) -> Vec<org_core::date::Day> {
        let Some(journal) = self.journal.as_ref() else { return Vec::new() };
        let directory = expand(&journal.directory);
        let mut names = Vec::new();
        list_files(&directory, "", 0, &mut names);
        let files: BTreeMap<String, String> = names
            .into_iter()
            .filter(|name| journal::is_journal_file(name, &journal.core))
            // A daily file's day is in its name: its text is not read.
            .filter_map(|name| {
                let text =
                    if journal.core.daily() { String::new() } else { self.journal_read(&directory.join(&name))? };
                Some((name, text))
            })
            .collect();
        journal::entry_days(&files, &journal.core)
    }

    /// A file as the editor has it, or as it is on disk.
    fn journal_read(&self, path: &Path) -> Option<String> {
        match self.journal_open(path) {
            Some((_, _, text)) => Some(text),
            None => std::fs::read_to_string(path).ok(),
        }
    }

    /// The document open at `path`: its URI, version and text.
    fn journal_open(&self, path: &Path) -> Option<(Uri, i32, String)> {
        let key = self
            .documents
            .keys()
            .find(|key| key.parse::<Uri>().ok().and_then(|uri| uri::to_path(&uri)).as_deref() == Some(path))?
            .clone();
        let document = &self.documents[&key];
        Some((key.parse().ok()?, document.version, self.open_text(&key)?))
    }

    /// What org-journal did, done in the editor and on disk.
    fn journal_apply(
        &mut self,
        id: &str,
        directory: &Path,
        files: &BTreeMap<String, String>,
        outcome: Outcome,
        hide_entries: bool,
        other_window: bool,
    ) -> Result<Value, Refusal> {
        let mut documents = Vec::new();
        let mut texts: BTreeMap<String, String> = files.clone();
        for (name, text) in &outcome.changed {
            let path = directory.join(name);
            match text {
                Some(text) => {
                    texts.insert(name.clone(), text.clone());
                    match self.journal_open(&path) {
                        Some((uri, version, now)) => {
                            let edits = self.text_edits(&now, text);
                            if !edits.is_empty() {
                                documents.push((uri, Some(version), edits));
                            }
                        }
                        None => super::write_whole(&path, text)?,
                    }
                }
                None => {
                    texts.remove(name);
                    // Emacs kills the file's buffer before deleting it: one
                    // the editor has open is emptied, or saving it would
                    // bring back the entries carried over.
                    if let Some((uri, version, now)) = self.journal_open(&path) {
                        let edits = self.text_edits(&now, "");
                        if !edits.is_empty() {
                            documents.push((uri, Some(version), edits));
                        }
                    }
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        if let Some(message) = outcome.messages.last() {
            self.message = Some(message.clone());
        }
        if let Some(view) = &outcome.view {
            self.journal_view(id, directory, files, view)?;
            return Ok(Value::Null);
        }
        // Where the window is, when org-journal changed the buffer of
        // another (a rescheduled entry's new day).
        let visit = match &outcome.shown {
            Some((file, offset)) => Some(journal::Visit {
                file: file.clone(),
                offset: *offset,
                hidden: Vec::new(),
                hidden_lines: Vec::new(),
            }),
            None => outcome.visit,
        };
        let Some(visit) = visit else {
            if !documents.is_empty() {
                let edit = self.workspace_edits(documents);
                self.apply_edit(lsp_types::ApplyWorkspaceEditParams { label: None, edit }, Pending::Edit);
            }
            return Ok(Value::Null);
        };
        let path = directory.join(&visit.file);
        let uri = uri::from_path(&path).ok_or("Cannot name the journal file")?;
        let text = texts.get(&visit.file).cloned().unwrap_or_default();
        let index = LineIndex::new(&text);
        let position = index.position(&text, visit.offset.min(text.len()), self.client.encoding);
        let folds = self.journal_folds(&text, &index, &visit.hidden_lines, position.line, hide_entries);
        for (uri, _, _) in &documents {
            self.edited.insert(uri.as_str().to_string());
        }
        if documents.is_empty() {
            let selection = Some(Range::new(position, position));
            let params = ShowDocumentParams { uri: uri.clone(), external: None, take_focus: Some(true), selection };
            let mut params = serde_json::to_value(params).unwrap_or_default();
            if other_window {
                params["orgOtherWindow"] = json!(true);
            }
            self.send_raw_request(ShowDocument::METHOD, params, Some(Pending::Folds { uri, folds }));
        } else {
            let edit = self.workspace_edits(documents);
            let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
            self.apply_edit(params, Pending::EditAndShow { uri, position, folds, other_window });
        }
        Ok(Value::Null)
    }

    /// The search results or the schedule, in a file of the server's
    /// (`*Org-journal search*`, `*Org-journal schedule*`), shown. Each
    /// result's date links to the line it was found on.
    fn journal_view(
        &mut self,
        id: &str,
        directory: &Path,
        files: &BTreeMap<String, String>,
        view: &View,
    ) -> Result<(), Refusal> {
        let mut text = view.text.clone();
        for link in view.links.iter().rev() {
            let source = files.get(&link.source).map(String::as_str).unwrap_or_default();
            let line = source[..link.offset.min(source.len())].matches('\n').count() + 1;
            let end = text[link.at..].find('\t').map_or(text.len(), |tab| link.at + tab);
            let path = directory.join(&link.source);
            let label = text[link.at..end].to_string();
            // Two blanks after the link, where Emacs has a tab: an editor
            // that hides the link's brackets widens a tab after them by as
            // much, past the line found.
            let after = if end < text.len() { end + 1 } else { end };
            text.replace_range(link.at..after, &format!("[[file:{}::{line}][{label}]]  ", path.display()));
        }
        let name = if id == commands::JOURNAL_SCHEDULE_VIEW { SCHEDULE_FILE } else { SEARCH_FILE };
        let path = self.journal_views.path().join(name);
        self.own_file_show(&path, &text, true, true)?;
        Ok(())
    }

    /// The headlines' folds as org-journal leaves them: `close` those whose
    /// subtree it hides, `open` the others. Without
    /// `org_journal_hide_entries_p` only the entry shows
    /// (`outline-show-subtree`), the rest as it was.
    fn journal_folds(
        &self,
        text: &str,
        index: &LineIndex,
        hidden_lines: &[usize],
        cursor: u32,
        hide_entries: bool,
    ) -> Value {
        let parse = org_syntax::parse(text, &self.todo);
        let folds = folding::headline_folds(&parse.syntax(), text, index, self.cycle.folding);
        let hidden = |line: u32| hidden_lines.contains(&(line as usize + 1));
        let (mut close, mut open): (Vec<FoldingRange>, Vec<FoldingRange>) = (Vec::new(), Vec::new());
        if hide_entries {
            for fold in folds {
                if (fold.start_line + 1..=fold.end_line).all(hidden) {
                    close.push(fold);
                } else {
                    open.push(fold);
                }
            }
        } else {
            // The entry around the cursor, and what it holds.
            let top = folds
                .iter()
                .filter(|fold| fold.start_line <= cursor && cursor <= fold.end_line)
                .min_by_key(|fold| fold.start_line);
            if let Some(top) = top.cloned() {
                open = folds
                    .into_iter()
                    .filter(|fold| top.start_line <= fold.start_line && fold.end_line <= top.end_line)
                    .collect();
            }
        }
        // Innermost first, as `org-lsp/startupFolds` gives them.
        close.sort_by_key(|fold| std::cmp::Reverse(fold.start_line));
        json!({ "close": close, "open": open })
    }
}
