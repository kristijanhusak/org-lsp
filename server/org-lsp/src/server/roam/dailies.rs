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
    /// The `org.roam.dailies.*` commands.
    pub(super) fn roam_dailies(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        if matches!(id, commands::ROAM_DAILIES_GOTO_NEXT | commands::ROAM_DAILIES_GOTO_PREVIOUS) {
            return self.roam_dailies_step(id, context);
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
                let pending = Pending::EditAndShow { uri, position, folds: Value::Null };
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
        let n = if id == commands::ROAM_DAILIES_GOTO_PREVIOUS { -n } else { n };
        let notes = daily_files(&directory, &settings.extensions);
        let here = self.canonical(&current);
        let position = notes
            .iter()
            .position(|note| self.canonical(note) == here)
            .ok_or("Can't find current note file - have you saved it yet?")?;
        if n >= 0 && position + 1 == notes.len() {
            return Err("Already at newest note".into());
        }
        if n < 0 && position == 0 {
            return Err("Already at oldest note".into());
        }
        let note = usize::try_from(position as i64 + n)
            .ok()
            .and_then(|index| notes.get(index))
            .ok_or("Wrong type argument: stringp, nil")?;
        let uri = self.uri_for_path(note).ok_or("Cannot name the daily note")?;
        self.send_request::<ShowDocument>(
            ShowDocumentParams { uri, external: None, take_focus: Some(true), selection: None },
            None,
        );
        Ok(Value::Null)
    }
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
