//! `org-clock-persist`: the running clock and the clock history kept in
//! Emacs's own file (`org-clock-persist-file`, `org-clock-save.el`) when
//! the server stops (`org-clock-save`), and taken back when it starts
//! (`org-clock-load`). The file is Emacs's, in Emacs's form, so the two
//! can take the clock from each other.
//!
//! Emacs keeps positions as markers; the file has them as character
//! positions from 1. The running clock's is after the start of its
//! `CLOCK:` line's timestamp, a history entry's at its headline.

use std::path::{Path, PathBuf};

use lsp_types::{MessageActionItem, MessageType, ShowMessageRequestParams};

use super::{replace_whole, running_lines, ClockTask, Pending, RunningClock, Server};
use crate::uri;

use crate::config::ClockPersist as Settings;

/// Whether anything is kept.
fn on(settings: &Settings) -> bool {
    settings.clock || settings.history
}

/// What the file holds: `org-clock-stored-resume-clock` and
/// `org-clock-stored-history`, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Stored {
    pub resume: Option<(PathBuf, usize)>,
    pub history: Vec<(PathBuf, usize)>,
}

/// The file's forms read: the `(FILE . POSITION)` pairs after each
/// `setq`.
pub(super) fn parse(text: &str) -> Stored {
    let mut stored = Stored::default();
    for (name, list) in [("org-clock-stored-resume-clock", false), ("org-clock-stored-history", true)] {
        let Some(at) = text.find(&format!("(setq {name} '")) else { continue };
        let rest = &text[at + name.len() + 8..];
        // The form ends where its line does.
        let form = &rest[..rest.find('\n').unwrap_or(rest.len())];
        let pairs = pairs(form);
        if list {
            stored.history = pairs;
        } else {
            stored.resume = pairs.into_iter().next();
        }
    }
    stored
}

/// The `("FILE" . POSITION)` pairs of `form`, in order.
fn pairs(form: &str) -> Vec<(PathBuf, usize)> {
    let mut found = Vec::new();
    let mut chars = form.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '"' {
            continue;
        }
        // A Lisp string, `\` quoting the next character.
        let mut name = String::new();
        while let Some((_, c)) = chars.next() {
            match c {
                '\\' => name.extend(chars.next().map(|(_, c)| c)),
                '"' => break,
                c => name.push(c),
            }
        }
        let Some(&(at, _)) = chars.peek() else { break };
        let after = form[at..].trim_start();
        let Some(number) = after.strip_prefix('.') else { continue };
        let digits: String = number.trim_start().chars().take_while(char::is_ascii_digit).collect();
        if let Ok(position) = digits.parse() {
            found.push((PathBuf::from(name), position));
        }
    }
    found
}

/// A file name as `%S` prints it.
fn lisp_string(path: &Path) -> String {
    let text = path.to_string_lossy();
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The file `org-clock-save` writes: its first line, then the forms.
pub(super) fn render(file: &Path, host: &str, stamp: &str, stored: &Stored) -> String {
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut text = format!(";; {name} - {host} at {stamp}  -*- lexical-binding: t; -*-\n");
    if let Some((path, position)) = &stored.resume {
        text.push_str(&format!("(setq org-clock-stored-resume-clock '({} . {position}))\n", lisp_string(path)));
    }
    if !stored.history.is_empty() {
        let pairs: Vec<String> =
            stored.history.iter().map(|(path, position)| format!("({} . {position})", lisp_string(path))).collect();
        text.push_str(&format!("(setq org-clock-stored-history '({}))\n", pairs.join(" ")));
    }
    text
}

/// The character position from 1 of the byte `offset` of `text`.
fn position(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].chars().count() + 1
}

/// The byte offset of the character position `position` of `text`.
fn offset(text: &str, position: usize) -> usize {
    text.char_indices().nth(position.saturating_sub(1)).map_or(text.len(), |(at, _)| at)
}

/// `system-name`
fn host_name() -> String {
    let from_file = |path: &str| std::fs::read_to_string(path).ok().map(|name| name.trim().to_string());
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| from_file("/proc/sys/kernel/hostname"))
        .or_else(|| from_file("/etc/hostname"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "localhost".to_string())
}

impl Server {
    /// `org-clock-load`: the history the file keeps comes back, and its
    /// running clock runs again if its `CLOCK:` line is still open
    /// (`org-clock-in` with `org-clock-in-resume` `auto-restart`), once
    /// asked with `org-clock-persist-query-resume`.
    pub(super) fn clock_load(&mut self) {
        let settings = self.clock_persist.clone();
        let Some(file) = settings.file.as_ref().filter(|_| on(&settings)) else { return };
        let Ok(text) = std::fs::read_to_string(file) else { return };
        self.clock_loaded = true;
        let stored = parse(&text);
        // `org-clock-history-push` of each, the oldest first.
        for (path, at) in stored.history.iter().filter(|(path, _)| path.exists()) {
            let Some(task) = self.stored_task(path, *at) else { continue };
            self.clock_history.retain(|other| !(other.uri == task.uri && other.title == task.title));
            self.clock_history.insert(0, task);
            self.clock_history.truncate(self.clock_history_length);
        }
        let Some((path, at)) = stored.resume.filter(|(path, _)| path.exists()) else { return };
        let Some((clock, title)) = self.stored_clock(&path, at) else { return };
        // Another clock found running stays the clock; the same one is
        // asked about as the clock to resume.
        if self.running.as_ref().is_some_and(|running| *running != clock) {
            return;
        }
        if !settings.query_resume {
            self.resume_clock(clock);
            return;
        }
        let actions = ["Yes", "No"]
            .into_iter()
            .map(|title| MessageActionItem { title: title.to_string(), properties: Default::default() })
            .collect();
        let params = ShowMessageRequestParams {
            typ: MessageType::INFO,
            message: format!("Resume clock ({title})?"),
            actions: Some(actions),
        };
        self.send_request::<lsp_types::request::ShowMessageRequest>(params, Some(Pending::ResumeClock(clock)));
    }

    /// The clock the file kept runs again, as the files say it does.
    pub(super) fn resume_clock(&mut self, clock: RunningClock) {
        if self.running.is_none() || self.running.as_ref() == Some(&clock) {
            // Resumed, it is the clock Emacs runs, no dangling one.
            if self.found_clock.as_ref() == Some(&clock) {
                self.found_clock = None;
            }
            self.running = Some(clock);
            self.publish_clock();
        }
    }

    /// The entry at the character position `at` of the file `path`, as
    /// the history keeps it.
    fn stored_task(&mut self, path: &Path, at: usize) -> Option<ClockTask> {
        let uri = uri::from_path(path)?;
        let heading = self
            .with_source(&uri, |text, _, _, _, _| org_core::properties::heading_at(text, offset(text, at)))
            .flatten()?;
        self.clock_task(&uri, heading)
    }

    /// The open clock of the entry at the character position `at` of
    /// `path`, and the entry's title.
    fn stored_clock(&mut self, path: &Path, at: usize) -> Option<(RunningClock, String)> {
        let uri = uri::from_path(path)?;
        let (heading, clock) = self
            .with_source(&uri, |text, _, _, outline, _| {
                let heading = org_core::properties::heading_at(text, offset(text, at))?;
                let line = &text[heading..];
                let head = line.lines().next().unwrap_or("").trim_start_matches('*').trim().to_string();
                let end = outline.entries.iter().map(|entry| entry.offset).find(|at| *at > heading);
                let end = end.unwrap_or(text.len());
                let (_, start, _) = running_lines(text)
                    .into_iter()
                    .find(|(line, _, found)| (heading..end).contains(line) && *found == head)?;
                Some((heading, (start, head)))
            })
            .flatten()?;
        let title = self.entry_title(&uri, heading).unwrap_or_default();
        let (start, head) = clock;
        Some((RunningClock { uri, start, head }, title))
    }

    /// `org-clock-save`: the running clock and the history written to
    /// the file, if the file was read, a clock ran, or there is none yet.
    pub(super) fn clock_save(&mut self) {
        let settings = self.clock_persist.clone();
        let Some(file) = settings.file.as_ref().filter(|_| on(&settings)) else { return };
        if !(self.clock_loaded || self.clock_used || !file.exists()) {
            return;
        }
        let mut stored = Stored::default();
        if settings.clock {
            if let Some(running) = self.running.clone() {
                stored.resume = self.clock_line(&running).and_then(|(text, _, line)| {
                    let path = uri::to_path(&running.uri)?;
                    // After the timestamp the line starts with.
                    let end = text[line..].find(']').map(|at| line + at + 1)?;
                    Some((path, position(&text, end)))
                });
            }
        }
        if settings.history {
            let history = self.clock_history.clone();
            for task in history.iter().rev() {
                let Some(heading) = self.find_task(task) else { continue };
                let Some(path) = uri::to_path(&task.uri) else { continue };
                let at = self.with_source(&task.uri, |text, _, _, _, _| position(text, heading));
                if let Some(at) = at {
                    stored.history.push((path, at));
                }
            }
        }
        let stamp = org_core::stamp::format_timestamp(self.date_time(), true, false, "");
        let text = render(file, &host_name(), &stamp, &stored);
        // Written whole or not at all: a server stopped while writing
        // leaves Emacs the file as it was.
        let _ = replace_whole(file, text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_emacs_s_file() {
        let stored = Stored {
            resume: Some((PathBuf::from("/home/me/notes \"a\".org"), 57)),
            history: vec![(PathBuf::from("/home/me/a.org"), 1), (PathBuf::from("/home/me/b.org"), 40)],
        };
        let text = render(Path::new("/home/me/.emacs.d/org-clock-save.el"), "box", "<2026-05-18 Mon 10:30>", &stored);
        assert_eq!(
            text,
            ";; org-clock-save.el - box at <2026-05-18 Mon 10:30>  -*- lexical-binding: t; -*-\n\
             (setq org-clock-stored-resume-clock '(\"/home/me/notes \\\"a\\\".org\" . 57))\n\
             (setq org-clock-stored-history '((\"/home/me/a.org\" . 1) (\"/home/me/b.org\" . 40)))\n"
        );
        assert_eq!(parse(&text), stored);
        // A buffer gone leaves a gap in the history, as `mapconcat` does.
        let gaps = "(setq org-clock-stored-history '((\"/a.org\" . 3)  (\"/b.org\" . 9)))\n";
        assert_eq!(parse(gaps).history, [(PathBuf::from("/a.org"), 3), (PathBuf::from("/b.org"), 9)]);
        assert_eq!(parse("").resume, None);
    }

    #[test]
    fn positions_count_characters_from_one() {
        let text = "* Café\n** B\n";
        assert_eq!(position(text, 0), 1);
        assert_eq!(position(text, text.find("** B").unwrap()), 8);
        assert_eq!(offset(text, 8), text.find("** B").unwrap());
        assert_eq!(offset(text, 100), text.len());
    }
}
