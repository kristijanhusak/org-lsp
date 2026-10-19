//! The agenda's bulk commands: lines marked (`org-agenda-bulk-mark` and
//! its relatives), then one action for the entries of all of them
//! (`org-agenda-bulk-action`, `B`).
//!
//! An action runs the command it stands for once an entry, as Emacs does.
//! Their edits are staged: each command reads the files as the ones before
//! it left them, and the editor gets one edit a file at the end.

use std::collections::HashMap;

use lsp_types::{ApplyWorkspaceEditParams, Position, TextEdit, Uri, WorkspaceEdit};
use org_core::Refusal;
use serde_json::{json, Value};

use super::{Context, Extra, Pending, Reveal, Server};
use crate::commands;
use crate::document::Document;
use crate::line_index::LineIndex;
use crate::views::{self, EntryRef, ViewLine};

/// A file as the commands of a bulk action have changed it so far.
pub(super) struct Staged {
    pub uri: Uri,
    /// The text the editor has.
    pub original: String,
    pub text: String,
    pub version: Option<i32>,
    /// A file the action makes (an archive file): the edit creates it.
    pub created: bool,
}

pub(super) type Staging = HashMap<String, Staged>;

/// The menu of `B`, without Emacs's `[f]unction`.
const ACTIONS: &[(&str, &str)] = &[
    ("$", "archive"),
    ("A", "archive to the archive sibling"),
    ("t", "TODO state"),
    ("+", "add a tag"),
    ("-", "remove a tag"),
    ("s", "schedule"),
    ("d", "deadline"),
    ("r", "refile"),
    ("S", "scatter over the next days"),
];

/// Where `offset` of a text is once `edits`, made on it, are applied: a
/// place in an edit goes to where the edit starts.
fn moved(edits: &[crate::actions::Edit], offset: usize) -> usize {
    let mut shift: isize = 0;
    for edit in edits {
        let (start, end) = (usize::from(edit.range.start()), usize::from(edit.range.end()));
        if end <= offset {
            shift += edit.new_text.len() as isize - (end - start) as isize;
        } else if start < offset {
            return (start as isize + shift).max(0) as usize;
        }
    }
    (offset as isize + shift).max(0) as usize
}

/// Apply `edits`, made for `text`, to it.
fn apply(text: &str, edits: &[TextEdit], encoding: crate::line_index::Encoding) -> String {
    let index = LineIndex::new(text);
    let mut ranges: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|edit| {
            (
                index.offset(text, edit.range.start, encoding),
                index.offset(text, edit.range.end, encoding),
                edit.new_text.as_str(),
            )
        })
        .collect();
    ranges.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = text.to_string();
    for (start, end, new) in ranges {
        out.replace_range(start..end.max(start), new);
    }
    out
}

impl Server {
    /// While a bulk action runs: the edit goes to the staged copies of
    /// its files instead of the editor.
    pub(super) fn stage(&mut self, edit: &WorkspaceEdit) {
        let mut documents: Vec<(Uri, Vec<TextEdit>)> = Vec::new();
        let mut created: Vec<Uri> = Vec::new();
        let text_edits = |change: &lsp_types::TextDocumentEdit| -> Vec<TextEdit> {
            change
                .edits
                .iter()
                .map(|edit| match edit {
                    lsp_types::OneOf::Left(edit) => edit.clone(),
                    lsp_types::OneOf::Right(annotated) => annotated.text_edit.clone(),
                })
                .collect()
        };
        match (&edit.document_changes, &edit.changes) {
            (Some(lsp_types::DocumentChanges::Edits(edits)), _) => {
                for change in edits {
                    documents.push((change.text_document.uri.clone(), text_edits(change)));
                }
            }
            // Files made, then edits (archiving to a new file).
            (Some(lsp_types::DocumentChanges::Operations(operations)), _) => {
                for operation in operations {
                    match operation {
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Create(create)) => {
                            created.push(create.uri.clone());
                        }
                        lsp_types::DocumentChangeOperation::Edit(change) => {
                            documents.push((change.text_document.uri.clone(), text_edits(change)));
                        }
                        lsp_types::DocumentChangeOperation::Op(_) => {}
                    }
                }
            }
            (_, Some(changes)) => {
                documents.extend(changes.iter().map(|(uri, edits)| (uri.clone(), edits.clone())));
            }
            _ => {}
        }
        let encoding = self.client.encoding;
        for (uri, edits) in documents {
            let key = uri.as_str().to_string();
            let (base, version) = match self.staging.as_ref().and_then(|staging| staging.get(&key)) {
                Some(staged) => (staged.text.clone(), staged.version),
                None => match self.documents.get(&key) {
                    Some(document) => (document.text.clone(), Some(document.version)),
                    None => {
                        let text = crate::uri::to_path(&uri)
                            .and_then(|path| std::fs::read_to_string(path).ok())
                            .unwrap_or_default();
                        (text, None)
                    }
                },
            };
            let text = apply(&base, &edits, encoding);
            let Some(staging) = self.staging.as_mut() else { return };
            let made = created.contains(&uri);
            staging.entry(key).and_modify(|staged| staged.text = text.clone()).or_insert(Staged {
                uri,
                original: base,
                text,
                version,
                created: made,
            });
        }
    }

    /// The text of the open document `key`, as a bulk action has staged it.
    pub(super) fn open_text(&self, key: &str) -> Option<String> {
        if let Some(staged) = self.staging.as_ref().and_then(|staging| staging.get(key)) {
            return Some(staged.text.clone());
        }
        self.documents.get(key).map(|document| document.text.clone())
    }

    /// The staged file `uri`, as a document, for the commands that read it.
    pub(super) fn staged_document(&self, uri: &Uri) -> Option<(Document, Option<i32>)> {
        let staged = self.staging.as_ref()?.get(uri.as_str())?;
        Some((Document::new(staged.text.clone(), staged.version.unwrap_or(0), &self.todo), staged.version))
    }

    /// Send what the bulk action staged: an edit for every file it changed.
    pub(super) fn finish_staging(&mut self) {
        let Some(staging) = self.staging.take() else { return };
        let mut files: Vec<Staged> = staging.into_values().collect();
        files.sort_by(|a, b| a.uri.as_str().cmp(b.uri.as_str()));
        for file in files {
            let edits = self.text_edits(&file.original, &file.text);
            if edits.is_empty() {
                continue;
            }
            self.edited.insert(file.uri.as_str().to_string());
            let edit = if file.created {
                let edit = lsp_types::TextDocumentEdit {
                    text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                        uri: file.uri.clone(),
                        version: None,
                    },
                    edits: edits.into_iter().map(lsp_types::OneOf::Left).collect(),
                };
                let create = lsp_types::CreateFile { uri: file.uri.clone(), options: None, annotation_id: None };
                WorkspaceEdit {
                    document_changes: Some(lsp_types::DocumentChanges::Operations(vec![
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Create(create)),
                        lsp_types::DocumentChangeOperation::Edit(edit),
                    ])),
                    ..Default::default()
                }
            } else {
                self.workspace_edit(&file.uri, file.version, edits)
            };
            self.apply_edit(ApplyWorkspaceEditParams { label: None, edit }, Pending::Edit);
        }
    }

    /// The lines of the open view.
    fn view_lines(&self) -> Vec<ViewLine> {
        self.view.as_ref().map(|open| open.view.lines.clone()).unwrap_or_default()
    }

    /// The line after `line` where an entry is
    /// (`next-single-property-change` of `org-hd-marker`), or the next
    /// line when none is.
    fn next_entry_line(lines: &[ViewLine], line: usize) -> usize {
        (line + 1..lines.len()).find(|at| lines[*at].entry.is_some()).unwrap_or(line + 1)
    }

    /// Mark the line `line` (`org-agenda-bulk-mark`); the line the cursor
    /// goes to then.
    fn mark_line(&mut self, lines: &[ViewLine], line: usize) -> std::result::Result<usize, Refusal> {
        let entry = lines.get(line).and_then(|line| line.entry.clone());
        if !entry.as_ref().is_some_and(|entry| self.agenda_marks.contains(entry)) {
            let entry = entry.ok_or("Nothing to mark at point")?;
            self.agenda_marks.push(entry);
        }
        Ok(Self::next_entry_line(lines, line))
    }

    /// Unmark the line `line` (`org-agenda-bulk-unmark`); the line the
    /// cursor goes to then, when it was marked.
    fn unmark_line(&mut self, lines: &[ViewLine], line: usize) -> Option<usize> {
        let entry = lines.get(line)?.entry.as_ref()?;
        let at = self.agenda_marks.iter().position(|mark| mark == entry)?;
        self.agenda_marks.remove(at);
        Some(Self::next_entry_line(lines, line))
    }

    /// Show the marks, and the cursor on `line`.
    fn show_marks(&mut self, line: Option<usize>) {
        self.refresh_view();
        if let Some(line) = line {
            let last = self.view.as_ref().map_or(0, |open| open.view.lines.len().saturating_sub(1));
            self.reveal = Some(Reveal::At(Position::new(line.min(last) as u32, 0)));
            self.reveal_when_current();
        }
    }

    /// `m`, `u`, `U`, `M-m`, `*`, `M-*` and `%`.
    pub(super) fn agenda_bulk_mark(
        &mut self,
        id: &str,
        context: Option<&Context>,
    ) -> std::result::Result<Value, Refusal> {
        self.view.as_ref().ok_or("No agenda is open")?;
        let lines = self.view_lines();
        let line = context.map_or(0, |context| context.position.line as usize);
        let count = context.and_then(|context| context.count).filter(|count| *count > 0);
        let message = match id {
            commands::AGENDA_BULK_MARK => {
                let mut at = line;
                for _ in 0..count.unwrap_or(1) {
                    if at >= lines.len() {
                        break;
                    }
                    at = self.mark_line(&lines, at)?;
                }
                self.show_marks(Some(at));
                format!("{} entries marked for bulk action", self.agenda_marks.len())
            }
            commands::AGENDA_BULK_UNMARK if count.is_some() => {
                return self.agenda_bulk_mark(commands::AGENDA_BULK_UNMARK_ALL, context)
            }
            commands::AGENDA_BULK_UNMARK => match self.unmark_line(&lines, line) {
                Some(next) => {
                    self.show_marks(Some(next));
                    format!("{} entries left marked for bulk action", self.agenda_marks.len())
                }
                None => "No entry to unmark here".to_string(),
            },
            commands::AGENDA_BULK_UNMARK_ALL => {
                if self.agenda_marks.is_empty() {
                    "No entry to unmark".to_string()
                } else {
                    self.agenda_marks.clear();
                    self.show_marks(None);
                    return Ok(Value::Null);
                }
            }
            commands::AGENDA_BULK_TOGGLE => {
                let next = match self.unmark_line(&lines, line) {
                    Some(next) => next,
                    None => self.mark_line(&lines, line)?,
                };
                self.show_marks(Some(next));
                format!("{} entries marked for bulk action", self.agenda_marks.len())
            }
            commands::AGENDA_BULK_TOGGLE_ALL => {
                for (at, row) in lines.iter().enumerate() {
                    if row.entry.is_some() && self.unmark_line(&lines, at).is_none() {
                        self.mark_line(&lines, at)?;
                    }
                }
                self.show_marks(None);
                format!("{} entries marked for bulk action", self.agenda_marks.len())
            }
            commands::AGENDA_BULK_MARK_ALL | commands::AGENDA_BULK_MARK_REGEXP => {
                let regexp = if id == commands::AGENDA_BULK_MARK_ALL {
                    ".".to_string()
                } else {
                    let Some(answer) = context.and_then(|context| context.input.clone()) else {
                        return Ok(
                            json!({ "input": { "kind": "text", "prompt": "Mark entries matching regexp", "value": "" } }),
                        );
                    };
                    answer
                };
                self.mark_regexp(&lines, &regexp)?;
                self.show_marks(None);
                format!("{} entries marked for bulk action", self.agenda_marks.len())
            }
            _ => return Err("Unknown command".into()),
        };
        Ok(json!({ "message": message }))
    }

    /// `org-agenda-bulk-mark-regexp`: from the first line with an entry,
    /// each match of `regexp` in the agenda's text marks its line when its
    /// entry's text matches too; the search stops at a match on a line
    /// without an entry, as Emacs's does.
    fn mark_regexp(&mut self, lines: &[ViewLine], regexp: &str) -> std::result::Result<(), Refusal> {
        let regex = org_core::agenda::matcher::emacs_buffer_regex(regexp)
            .ok_or_else(|| Refusal::Failed(format!("Invalid regexp {regexp}")))?;
        let text = self.view.as_ref().map(|open| open.view.text.clone()).unwrap_or_default();
        let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect();
        let line_of = |offset: usize| starts.partition_point(|start| *start <= offset) - 1;
        let Some(first) = lines.iter().position(|line| line.entry.is_some()) else { return Ok(()) };
        let mut at = starts[first];
        while let Some(found) = regex.find_at(&text, at) {
            let line = line_of(found.start());
            let Some(txt) = self.line_text(line) else { break };
            if regex.is_match(&txt) {
                let next = self.mark_line(lines, line)?;
                at = starts.get(next).copied().unwrap_or(text.len());
            } else {
                at = found.end().max(found.start() + 1);
            }
            if at > text.len() {
                break;
            }
        }
        Ok(())
    }

    /// The text of the entry of a line, after its prefix (its `txt`).
    fn line_text(&self, line: usize) -> Option<String> {
        let open = self.view.as_ref()?;
        open.view.lines.get(line)?.entry.as_ref()?;
        let text = open.view.text.lines().nth(line)?;
        // The prefix ends where the category and time leave off: the
        // headline as the entry has it shows from there.
        let reference = open.view.lines[line].entry.as_ref()?;
        let head = reference.head.trim();
        Some(match text.find(head.split_whitespace().next().unwrap_or("")) {
            Some(start) if !head.is_empty() => text[start..].to_string(),
            _ => text.trim_start().to_string(),
        })
    }

    /// `org-agenda-bulk-action` (`B`): asks which action, then what it
    /// needs, then runs it on every marked entry. Without marks, on the
    /// entry of the cursor's line.
    pub(super) fn agenda_bulk_action(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        self.view.as_ref().ok_or("No agenda is open")?;
        let context = context.ok_or("This command needs the cursor position")?;
        if self.agenda_marks.is_empty() {
            let lines = self.view_lines();
            self.mark_line(&lines, context.position.line as usize)?;
            self.refresh_view();
        }
        // Whether the marks stay, toggled by `p`.
        let persist = self.agenda_persistent_marks != (context.answers.as_deref() == Some("p"));
        let toggled = if context.answers.as_deref() == Some("p") { "p" } else { "" };
        let action = context.step.as_deref();
        let answer = context.input.clone();
        let menu = |persist: bool, toggled: &str| {
            let choices: Vec<Value> =
                std::iter::once(("p", if persist { "do not persist marks" } else { "persist marks" }))
                    .chain(ACTIONS.iter().copied())
                    .map(|(key, label)| json!({ "key": key, "label": label }))
                    .collect();
            json!({ "input": {
                "kind": "key",
                "prompt": format!("Bulk ({}[p]ersist marks)", if persist { "" } else { "don't " }),
                "value": "", "choices": choices,
                "context": { "step": "action", "answers": toggled },
            } })
        };
        match (action, answer.as_deref()) {
            (None, _) => Ok(menu(persist, toggled)),
            (Some("action"), Some("p")) => {
                let toggled = if toggled == "p" { "" } else { "p" };
                Ok(menu(!persist, toggled))
            }
            (Some("action"), Some(key)) => self.bulk_question(context, key, toggled),
            (Some("action"), None) => Ok(Value::Null),
            (Some(step), Some(answer)) => self.run_bulk(context, step, answer, persist),
            (Some(_), None) => Ok(Value::Null),
        }
    }

    /// What the action of `key` asks before it runs.
    fn bulk_question(&mut self, context: &Context, key: &str, toggled: &str) -> std::result::Result<Value, Refusal> {
        let ask = |kind: &str, prompt: &str, value: &str, completions: Vec<String>| {
            json!({ "input": {
                "kind": kind, "prompt": prompt, "value": value, "completions": completions,
                "context": { "step": key, "answers": toggled },
            } })
        };
        let first = self.first_marked_file();
        match key {
            "$" | "A" => self.run_bulk(context, key, "", self.agenda_persistent_marks != (toggled == "p")),
            "t" => {
                let names = first
                    .and_then(|uri| {
                        self.with_source(&uri, |_, _, parse, _, _| {
                            parse.todo_keywords().names().map(str::to_string).collect::<Vec<_>>()
                        })
                    })
                    .unwrap_or_default();
                Ok(ask("text", "Todo state", "", names))
            }
            "+" | "-" => {
                let tags = self.agenda_tags();
                Ok(ask("text", if key == "+" { "Tag to add" } else { "Tag to remove" }, "", tags))
            }
            "s" | "d" => {
                let input = commands::input(if key == "s" { commands::SCHEDULE } else { commands::DEADLINE });
                let prompt = if key == "s" { "(Re)Schedule to" } else { "(Re)Set Deadline to" };
                Ok(json!({ "input": {
                    "kind": input.map_or("date", |input| input.kind), "prompt": prompt, "value": "",
                    "context": { "step": key, "answers": toggled },
                } }))
            }
            "r" | "w" => {
                // The targets the refile command offers for the first entry.
                let lines = self.view_lines();
                let line = self
                    .sorted_marks()
                    .first()
                    .and_then(|mark| lines.iter().position(|line| line.entry.as_ref() == Some(mark)))
                    .ok_or("No entry is marked")?;
                let mut asked = context.clone();
                asked.position = Position::new(line as u32, 0);
                asked.input = None;
                asked.step = None;
                asked.answers = None;
                let mut offered = self.refile(&asked)?;
                if let Some(input) = offered.get_mut("input") {
                    input["context"] = json!({ "step": "r", "answers": toggled });
                }
                Ok(offered)
            }
            "S" => {
                let query = self.view.as_ref().map(|open| open.view.query.clone());
                if !matches!(query, Some(views::Query::Agenda { .. } | views::Query::Todo)) {
                    return Err("Can't scatter tasks in this agenda view".into());
                }
                let prompt = if context.count.is_some_and(|count| count > 0) {
                    "Scatter tasks across how many weekdays"
                } else {
                    "Scatter tasks across how many days"
                };
                Ok(ask("text", prompt, "7", Vec::new()))
            }
            other => Err(Refusal::Failed(format!("Invalid bulk action: {other}"))),
        }
    }

    /// The file of the first marked entry.
    fn first_marked_file(&self) -> Option<Uri> {
        self.sorted_marks().first().map(|mark| mark.uri.clone())
    }

    /// The marks in the order the action takes them: by file name, then
    /// by place in the file (parents before their children).
    fn sorted_marks(&self) -> Vec<EntryRef> {
        let mut marks: Vec<EntryRef> = self.agenda_marks.iter().rev().cloned().collect();
        marks.sort_by(|a, b| {
            let name = |mark: &EntryRef| crate::uri::file_name(&mark.uri);
            name(a).cmp(&name(b)).then(a.offset.cmp(&b.offset))
        });
        marks
    }

    /// Run the action `key` with `answer` on every marked entry.
    fn run_bulk(
        &mut self,
        context: &Context,
        key: &str,
        answer: &str,
        persist: bool,
    ) -> std::result::Result<Value, Refusal> {
        let mut marks = self.sorted_marks();
        let original = marks.clone();
        let mut lines = self.view_lines();
        // A date read once for all of them: from the day of the cursor's
        // line when it is a date line, else from today. A `++` answer is
        // read for each entry.
        let date =
            match key {
                "s" | "d" if !answer.trim_start().starts_with("++") => {
                    let header = lines.get(context.position.line as usize).and_then(|line| line.day);
                    Some(self.fixed_date(
                        answer,
                        header.filter(|_| {
                            lines.get(context.position.line as usize).is_some_and(|line| line.entry.is_none())
                        }),
                    )?)
                }
                "s" | "d" => Some(answer.to_string()),
                _ => None,
            };
        let days: Option<i64> = match key {
            "S" => Some(answer.trim().parse().map_err(|_| Refusal::Failed("Please enter a number".to_string()))?),
            _ => None,
        };
        self.staging = Some(Staging::new());
        // The targets of a refile are those of the first entry's file.
        self.refile_current = marks.first().map(|mark| mark.uri.clone()).filter(|_| matches!(key, "r" | "w"));
        let (mut processed, mut skipped) = (0, 0);
        let mut failure = None;
        for number in 0..marks.len() {
            let mark = marks[number].clone();
            let mark = &mark;
            let before = self.with_source(&mark.uri, |text, _, _, _, _| text.to_string());
            let Some(line) = lines.iter().position(|line| line.entry.as_ref() == Some(mark)) else {
                skipped += 1;
                continue;
            };
            let gone = self
                .with_source(&mark.uri, |_, _, _, outline, _| views::resolve(mark, outline).is_none())
                .unwrap_or(true);
            if gone {
                skipped += 1;
                continue;
            }
            let mut at = context.clone();
            at.position = Position::new(line as u32, 0);
            at.count = None;
            at.step = None;
            at.answers = None;
            at.input = None;
            let result = match key {
                "$" => self.archive(commands::ARCHIVE_SUBTREE, &at).map(|_| ()),
                "A" => self.archive(commands::ARCHIVE_SIBLING, &at).map(|_| ()),
                "t" => {
                    // No note is asked for: a time is logged instead.
                    let extra = Extra { time_only: true, ..Extra::default() };
                    let target =
                        org_core::todo::Target::State(Some(answer.to_string()).filter(|state| !state.is_empty()));
                    self.edit_with(&at, commands::TODO, Some(super::EditCommand::Todo(target)), &extra).map(|_| ())
                }
                "+" | "-" => self.toggle_tag(&at, mark, answer, key == "+"),
                "s" | "d" => {
                    let id = if key == "s" { commands::SCHEDULE } else { commands::DEADLINE };
                    let date = date.clone().unwrap_or_default();
                    self.date_target(id, &at).and_then(|target| self.apply_date(id, &target, &date))
                }
                "r" | "w" => {
                    at.input = Some(answer.to_string());
                    self.refile(&at).map(|_| ())
                }
                "S" => {
                    let date =
                        self.scatter_date(days.unwrap_or(7), context.count.is_some_and(|count| count > 0), number);
                    self.date_target(commands::SCHEDULE, &at)
                        .and_then(|target| self.apply_date(commands::SCHEDULE, &target, &date))
                }
                other => Err(Refusal::Failed(format!("Invalid bulk action: {other}"))),
            };
            match result {
                Ok(()) => processed += 1,
                Err(refusal) => {
                    failure = Some(refusal);
                    break;
                }
            }
            // The entries after this one in its file moved by what it
            // changed: the marks still to do, and the view's lines, follow
            // them, or an entry with the same headline nearer the old place
            // would be taken for theirs.
            let after = self.with_source(&mark.uri, |text, _, _, _, _| text.to_string());
            if let (Some(before), Some(after)) = (before, after) {
                if before != after {
                    let edits = crate::actions::edits_between(&before, &after);
                    let follow = |entry: &mut EntryRef| {
                        if entry.uri == mark.uri {
                            entry.offset = moved(&edits, entry.offset);
                            entry.marker = entry.marker.map(|marker| moved(&edits, marker));
                        }
                    };
                    marks.iter_mut().skip(number + 1).for_each(follow);
                    if let Some(open) = self.view.as_mut() {
                        for view in std::iter::once(&mut open.view).chain(open.shown.as_mut()) {
                            view.lines.iter_mut().filter_map(|line| line.entry.as_mut()).for_each(follow);
                        }
                    }
                    lines = self.view_lines();
                }
            }
        }
        // The marks that stay, on their entries with their headlines as
        // they are now. An open document is changed when the editor applies
        // the edit, and its `didChange` moves the marks: there they keep
        // their places in the text before; a closed file is written now.
        let kept: Vec<EntryRef> = if persist {
            original
                .iter()
                .zip(&marks)
                .filter_map(|(before, mark)| {
                    let open = self.documents.contains_key(mark.uri.as_str());
                    self.with_source(&mark.uri, |_, _, _, outline, _| {
                        // The mark followed the edits, so its entry starts
                        // where it says, though its headline may have changed
                        // (a new TODO state); unless the action took entries
                        // away, when another may start there now.
                        let moves = matches!(key, "$" | "A" | "r" | "w");
                        let index = outline
                            .entries
                            .iter()
                            .position(|entry| entry.offset == mark.offset)
                            .filter(|_| !moves)
                            .or_else(|| views::resolve(mark, outline))?;
                        let entry = &outline.entries[index];
                        let (offset, marker) = if open {
                            (before.offset, before.marker)
                        } else {
                            (entry.offset, mark.marker.map(|marker| entry.offset + marker.saturating_sub(mark.offset)))
                        };
                        Some(EntryRef { uri: mark.uri.clone(), offset, head: entry.head.clone(), marker })
                    })
                    .flatten()
                })
                .collect()
        } else {
            Vec::new()
        };
        self.refile_current = None;
        self.finish_staging();
        self.agenda_marks = kept;
        self.release_held();
        self.refresh_view();
        if let Some(failure) = failure {
            return Err(failure);
        }
        let mut message = format!("Acted on {processed} entries");
        if skipped > 0 {
            message.push_str(&format!(", skipped {skipped} (disappeared before their turn)"));
        }
        if persist {
            message.push_str(" (kept marked)");
        }
        // Said instead of what the commands it ran said.
        self.message = None;
        Ok(json!({ "message": message }))
    }

    /// `answer` read as `org-read-date` reads it once for every entry, as a
    /// date the planning commands take as it is. Only the day: Emacs's
    /// bulk action loses the time the answer gives.
    fn fixed_date(&self, answer: &str, header: Option<org_core::date::Day>) -> std::result::Result<String, Refusal> {
        let now = self.date_time();
        let default = header.map(|day| org_core::date::DateTime { day, minute: 0 });
        let read = org_core::read_date::read_date(answer, default, now, &self.read_date).map_err(Refusal::Failed)?;
        Ok(format!("{:04}-{:02}-{:02}", read.date.year, read.date.month, read.date.day))
    }

    /// The day the `S` action schedules the `number`th entry on: one of
    /// the next `days` days, or weekdays.
    fn scatter_date(&self, days: i64, weekdays: bool, number: usize) -> String {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64)
            .wrapping_add(number as u64 * 0x9E37_79B9_7F4A_7C15);
        let mut state = seed | 1;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mut distance = 1 + (state % days.max(1) as u64) as i64;
        let today = self.date_time().day;
        if weekdays {
            // Days of the weekend do not count (`org-agenda-weekend-days`).
            let weekend = &self.agenda.weekend_days;
            let mut day = today;
            let mut counted = 0;
            distance = 0;
            let wanted = 1 + (state % days.max(1) as u64) as i64;
            while counted < wanted {
                day += 1;
                distance += 1;
                if !weekend.contains(&org_core::date::weekday(day)) {
                    counted += 1;
                }
            }
        }
        let date = org_core::date::Date::from_day(today + distance);
        format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
    }

    /// `org-agenda-set-tags` with `on` or `off`: `tag` added to or taken
    /// from the entry's own tags, which are aligned anew either way.
    fn toggle_tag(&mut self, at: &Context, mark: &EntryRef, tag: &str, on: bool) -> std::result::Result<(), Refusal> {
        let tags = self
            .with_source(&mark.uri, |_, _, _, outline, _| {
                let entry = &outline.entries[views::resolve(mark, outline)?];
                Some(entry.tags.iter().filter(|tag| !tag.inherited).map(|tag| tag.name.clone()).collect::<Vec<_>>())
            })
            .flatten()
            .ok_or("The entry is gone; rebuild the agenda")?;
        let mut tags = tags;
        if on && !tags.iter().any(|known| known == tag) {
            tags.push(tag.to_string());
        } else if !on {
            tags.retain(|known| known != tag);
        }
        self.edit_with(at, commands::SET_TAGS, Some(super::EditCommand::Tags(tags)), &Extra::default()).map(|_| ())
    }
}
