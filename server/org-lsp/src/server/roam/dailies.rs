//! org-roam-dailies: a note a day in `org_roam_dailies_directory`, under
//! org-roam's directory, captured to with
//! `org_roam_dailies_capture_templates` or visited
//! (`org-roam-dailies-capture-today`, `org-roam-dailies-goto-date` …),
//! and the next and previous notes (`org-roam-dailies-goto-next-note`).

use std::path::{Path, PathBuf};

use lsp_types::request::ShowDocument;
use lsp_types::{Range, ShowDocumentParams};
use org_core::date::{Date, DateTime};
use org_core::read_date::{self, PreferFuture};
use org_core::roam::{self, capture as roam_capture};
use org_core::Refusal;
use serde_json::{json, Value};

use super::{RoamContext, OFF};
use crate::prompt::{DatePrompt, Target};
use crate::server::{Pending, Server};
use crate::{commands, uri};

/// The day a capture of org-roam-dailies is for, and whether it only
/// goes to the note (`GOTO`).
#[derive(Debug, Clone, Copy)]
pub(in crate::server) struct Daily {
    pub time: DateTime,
    pub goto: bool,
}

impl Daily {
    /// The day as the answers of its capture carry it.
    pub(super) fn state(&self) -> Value {
        let date = Date::from_day(self.time.day);
        json!({ "day": format!("{date} {:02}:{:02}", self.time.minute / 60, self.time.minute % 60), "goto": self.goto })
    }

    fn read(day: &str, goto: bool) -> Option<Daily> {
        let (date, time) = day.split_once(' ')?;
        let mut parts = date.split('-').map(|part| part.parse::<i64>().ok());
        let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
        let (hour, minute) = time.split_once(':')?;
        let time = DateTime::new(Date::new(year, month, day), hour.parse().ok()?, minute.parse().ok()?);
        Some(Daily { time, goto })
    }
}

impl Server {
    /// The days with a daily note, which every date prompt marks while
    /// org-roam is on (`org-roam-dailies-calendar-mark-entries` in
    /// `calendar-today-visible-hook`): the date in a file's name
    /// (`org-roam-dailies-calendar--file-to-date`).
    pub(in crate::server) fn roam_daily_days(&self) -> Vec<org_core::date::Day> {
        let Some(roam) = self.roam.as_ref() else { return Vec::new() };
        let directory = roam.directory.join(&roam.settings.dailies_directory);
        daily_files(&directory, &roam.settings.files.extensions)
            .iter()
            .filter_map(|file| file_date(&file.file_stem()?.to_string_lossy()))
            .collect()
    }

    /// The `org.roam.dailies.*` commands.
    pub(super) fn roam_dailies(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        if matches!(id, commands::ROAM_DAILIES_GOTO_NEXT | commands::ROAM_DAILIES_GOTO_PREVIOUS) {
            return self.roam_dailies_step(id, context);
        }
        if id == commands::ROAM_DAILIES_FIND_DIRECTORY {
            return self.roam_dailies_find_directory(context);
        }
        let now = self.date_time();
        let count = context.count;
        let daily = match (&context.day, context.step.as_deref()) {
            // A question of the capture answered: the day it is for.
            (Some(day), _) => Daily::read(day, context.goto == Some(true)).ok_or("Invalid date")?,
            (None, Some("date")) => {
                let goto = id == commands::ROAM_DAILIES_GOTO_DATE || context.goto == Some(true);
                // `org-read-date` with `org-read-date-prefer-future` nil.
                let settings = read_date::Settings { prefer_future: PreferFuture::No, ..self.read_date };
                let answer = context.input.clone().unwrap_or_default();
                let read = read_date::read_date(&answer, None, now, &settings).map_err(Refusal::Failed)?;
                let (hour, minute) = read.time.unwrap_or((0, 0));
                Daily { time: DateTime::new(read.date, hour, minute), goto }
            }
            (None, _) => {
                let days = |n: i64| DateTime { day: now.day + n, minute: now.minute };
                // `interactive "p"`: the count, else 1; `"P"`: `C-u` goes
                // to the note.
                let n = count.unwrap_or(1);
                let prefix = count.is_some();
                match id {
                    commands::ROAM_DAILIES_CAPTURE_TODAY => Daily { time: now, goto: prefix },
                    commands::ROAM_DAILIES_GOTO_TODAY => Daily { time: now, goto: true },
                    commands::ROAM_DAILIES_CAPTURE_TOMORROW => Daily { time: days(n), goto: false },
                    commands::ROAM_DAILIES_GOTO_TOMORROW => Daily { time: days(n), goto: true },
                    commands::ROAM_DAILIES_CAPTURE_YESTERDAY => Daily { time: days(-n), goto: false },
                    commands::ROAM_DAILIES_GOTO_YESTERDAY => Daily { time: days(-n), goto: true },
                    commands::ROAM_DAILIES_CAPTURE_DATE | commands::ROAM_DAILIES_GOTO_DATE => {
                        let goto = id == commands::ROAM_DAILIES_GOTO_DATE || prefix;
                        let prompt = if goto { "Find daily-note" } else { "Capture to daily-note" };
                        // `org-read-date` with its calendar, in an editor
                        // that shows it.
                        if self.client.date_prompt {
                            return self.roam_dailies_calendar(id, context, prompt, goto);
                        }
                        return Ok(json!({ "input": {
                            "kind": "date",
                            "prompt": prompt,
                            "value": "",
                            "context": { "step": "date", "goto": goto },
                        } }));
                    }
                    other => return Err(format!("org-lsp has no command `{other}`").into()),
                }
            }
        };
        let (db, _) = self.roam_db()?;
        // A question of the capture comes back as a question of a
        // template (`keys`, `capture`), with the day.
        let mut context = context.clone();
        if context.step.as_deref() == Some("date") {
            context.step = None;
            context.input = None;
        }
        self.roam_capture(id, &context, &db, "", None, Some(daily))
    }

    /// The date prompt's calendar for the day of `id`, its minibuffer line
    /// starting with `prompt` as `org-read-date`'s does (`Find daily-note:
    /// Date+time [2026-05-18]: `). The days with a note are marked in it.
    fn roam_dailies_calendar(
        &mut self,
        id: &str,
        context: &RoamContext,
        prompt: &str,
        goto: bool,
    ) -> Result<Value, Refusal> {
        let from = context
            .text_document
            .as_ref()
            .map(|document| document.uri.clone())
            .or_else(|| uri::from_path(&std::env::temp_dir()))
            .ok_or("No place to run the command from")?;
        let now = self.date_time();
        let settings = self.read_date_of(id);
        let layout = (self.calendar_months, self.calendar_week_start);
        let command = org_core::planning::Command::Timestamp { inactive: false };
        let target = Target::Dailies { from, goto };
        let mut open = DatePrompt::new(id, command, target, None, String::new(), now, &settings, layout);
        // `(concat prompt " ")` before `Date+time [...]: `, and the prompt
        // ends with `: `.
        open.label = format!("{prompt}:  {}", open.label);
        let typed = open.typed.clone();
        open.set_typed(typed, now, &settings);
        self.open_date_prompt(open);
        Ok(Value::Null)
    }

    /// The answer of the date prompt of `id`, asked from `from`: the note
    /// of that day, gone to or captured to.
    pub(in crate::server) fn roam_dailies_date(
        &mut self,
        id: &str,
        from: &lsp_types::Uri,
        goto: bool,
        answer: &str,
    ) -> Result<(), Refusal> {
        let argument = json!({
            "textDocument": { "uri": from.as_str() },
            "step": "date",
            "input": answer,
            "goto": goto,
        });
        let context: RoamContext = serde_json::from_value(argument.clone()).map_err(|error| error.to_string())?;
        let value = self.roam_dailies(id, &context)?;
        // What the capture asks next (the template's key): asked with a
        // message, as the prompt is gone.
        if value.get("input").is_some() || value.get("message").is_some() {
            self.prompt_with_messages(id.to_string(), argument, &value);
        }
        Ok(())
    }

    /// `org-roam-dailies-find-directory`: `find-file` on the directory of
    /// the notes, whose Dired buffer is a list of its files here, to pick
    /// one from and visit.
    fn roam_dailies_find_directory(&mut self, context: &RoamContext) -> Result<Value, Refusal> {
        let roam = self.roam.as_ref().ok_or(OFF)?;
        let directory = roam.directory.join(&roam.settings.dailies_directory);
        let notes = daily_files(&directory, &roam.settings.files.extensions);
        if let (Some("file"), Some(answer)) = (context.step.as_deref(), context.input.as_deref()) {
            let note = directory.join(answer);
            if !notes.contains(&note) {
                return Err(format!("No daily note {answer}").into());
            }
            let uri = self.uri_for_path(&note).ok_or("Cannot name the daily note")?;
            self.send_request::<ShowDocument>(
                ShowDocumentParams { uri, external: None, take_focus: Some(true), selection: None },
                None,
            );
            return Ok(Value::Null);
        }
        let shown = format!("{}/", crate::server::abbreviate_path(&directory)).replace("//", "/");
        // In the order of their names, as Dired lists them.
        let mut names: Vec<String> = notes
            .iter()
            .filter_map(|note| note.strip_prefix(&directory).ok())
            .map(|name| name.to_string_lossy().replace('\\', "/"))
            .collect();
        names.sort();
        if names.is_empty() {
            return Err(format!("No daily notes in {shown}").into());
        }
        let choices: Vec<Value> = names.iter().map(|name| json!({ "key": name, "label": name })).collect();
        Ok(json!({ "input": {
            "kind": "choice",
            "prompt": format!("Find file in {shown}"),
            "value": "",
            "choices": choices,
            "context": { "step": "file" },
        } }))
    }

    /// `org-roam-capture-` with `:goto`: the note's file made if it is
    /// not there (saved, where Emacs leaves its buffer unsaved) and shown
    /// at its start, where `org-capture-goto-target` leaves the point.
    pub(super) fn roam_goto(&mut self, located: roam_capture::Located) -> Result<Value, Refusal> {
        let uri = self.uri_for_path(&located.path).ok_or("Cannot name the daily note")?;
        let position = lsp_types::Position::new(0, 0);
        let open = self.document(&uri).map(|document| (document.text.clone(), document.version));
        match open {
            Some((now, version)) if now != located.text => {
                let edits = self.text_edits(&now, &located.text);
                let edit = self.workspace_edit(&uri, Some(version), edits);
                let pending = Pending::EditAndShow { uri, position, folds: Value::Null, other_window: false };
                self.apply_edit(lsp_types::ApplyWorkspaceEditParams { label: None, edit }, pending);
                return Ok(Value::Null);
            }
            Some(_) => {}
            None => {
                let on_disk = std::fs::read_to_string(&located.path).ok();
                if on_disk.as_deref() != Some(located.text.as_str()) {
                    if let Some(directory) = located.path.parent() {
                        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                    }
                    crate::server::write_whole(&located.path, &located.text)?;
                }
            }
        }
        let selection = Some(Range::new(position, position));
        self.send_request::<ShowDocument>(
            ShowDocumentParams { uri, external: None, take_focus: Some(true), selection },
            None,
        );
        Ok(Value::Null)
    }

    /// `org-roam-dailies-goto-next-note` and `…-previous-note`: the note
    /// a count of notes on or back from the one shown.
    fn roam_dailies_step(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        let roam = self.roam.as_ref().ok_or(OFF)?;
        let directory = roam.directory.join(&roam.settings.dailies_directory);
        let settings = roam.settings.files.clone();
        let roam_directory = roam.directory.clone();
        let current = context.text_document.as_ref().and_then(|document| uri::to_path(&document.uri));
        // `org-roam-dailies--daily-note-p`: an org-roam file under the
        // daily notes' directory.
        let daily = current.as_ref().is_some_and(|path| {
            let canonical = self.canonical(path);
            let relative = canonical.strip_prefix(self.canonical(&roam_directory)).ok();
            relative.is_some_and(|name| roam::is_roam_file(&name.to_string_lossy(), &settings))
                && canonical.starts_with(self.canonical(&directory))
        });
        let Some(current) = current.filter(|_| daily) else {
            return Err("Not in a daily-note".into());
        };
        let n = context.count.unwrap_or(1);
        let previous = id == commands::ROAM_DAILIES_GOTO_PREVIOUS;
        let notes = daily_files(&directory, &settings.extensions);
        let here = self.canonical(&current);
        let position = notes
            .iter()
            .position(|note| self.canonical(note) == here)
            .ok_or("Can't find current note file - have you saved it yet?")?;
        let note = &notes[step(position, notes.len(), n, previous)?];
        let uri = self.uri_for_path(note).ok_or("Cannot name the daily note")?;
        self.send_request::<ShowDocument>(
            ShowDocumentParams { uri, external: None, take_focus: Some(true), selection: None },
            None,
        );
        Ok(Value::Null)
    }
}

/// Where `org-roam-dailies--goto-next-note` goes from the note at
/// `position` of `len`, `n` notes on, or back when `previous`; a count
/// too large for a number goes past the last note as well.
fn step(position: usize, len: usize, n: i64, previous: bool) -> Result<usize, &'static str> {
    let n = if previous { n.saturating_neg() } else { n };
    if n >= 0 && position + 1 == len {
        return Err("Already at newest note");
    }
    if n < 0 && position == 0 {
        return Err("Already at oldest note");
    }
    i64::try_from(position)
        .ok()
        .and_then(|position| position.checked_add(n))
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < len)
        .ok_or("Wrong type argument: stringp, nil")
}

/// `org-parse-time-string` of a file's name without its extension: the
/// first date in it, `org-ts-regexp0`, when it is one.
fn file_date(name: &str) -> Option<org_core::date::Day> {
    static DATE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"([0-9]{4})-([0-9]{2})-([0-9]{2})").unwrap());
    let found = DATE.captures(name)?;
    let (year, month, day) = (found[1].parse().ok()?, found[2].parse().ok()?, found[3].parse().ok()?);
    let date = Date::new(year, month, day);
    // A date that is no day (February 30th) is left out.
    (Date::from_day(date.to_day()) == date).then(|| date.to_day())
}

/// `org-roam-dailies--list-files`: the files of `directory` and those
/// below it whose extension is one of org-roam's, as
/// `directory-files-recursively` lists them (a directory's own files after
/// those of the directories in it, each in the order of their names),
/// without auto-save, backup and hidden files.
fn daily_files(directory: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    let (mut found, mut files) = (Vec::new(), Vec::new());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Not into links to directories, as without FOLLOW-SYMLINKS.
        let kind = entry.file_type().ok();
        if kind.is_some_and(|kind| kind.is_dir()) {
            if name != "." && name != ".." {
                found.extend(daily_files(&entry.path(), extensions));
            }
            continue;
        }
        let ours = extensions.iter().any(|extension| name.ends_with(&format!(".{extension}")));
        let auto_save = name.starts_with('#') && name.ends_with('#');
        if ours && !auto_save && !name.ends_with('~') && !name.starts_with('.') {
            files.push(entry.path());
        }
    }
    found.extend(files);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daily_note_is_for_the_date_in_its_name() {
        assert_eq!(file_date("2026-05-18"), Some(Date::new(2026, 5, 18).to_day()));
        assert_eq!(file_date("notes-2026-05-18-work"), Some(Date::new(2026, 5, 18).to_day()));
        assert_eq!(file_date("2026-02-30"), None);
        assert_eq!(file_date("journal"), None);
    }

    #[test]
    fn steps_between_notes_with_any_count() {
        assert_eq!(step(1, 3, 1, false), Ok(2));
        assert_eq!(step(1, 3, 1, true), Ok(0));
        assert_eq!(step(2, 3, 1, false), Err("Already at newest note"));
        assert_eq!(step(0, 3, 1, true), Err("Already at oldest note"));
        assert_eq!(step(1, 3, 5, false), Err("Wrong type argument: stringp, nil"));
        assert_eq!(step(1, 3, i64::MIN, true), Err("Wrong type argument: stringp, nil"));
        assert_eq!(step(1, 3, i64::MIN, false), Err("Wrong type argument: stringp, nil"));
        assert_eq!(step(1, 3, i64::MAX, false), Err("Wrong type argument: stringp, nil"));
    }
}
