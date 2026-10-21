//! `org-resolve-clocks` and `org-clock-resolve`: what to do with a clock
//! left running (a dangling clock) or with idle time, asked about one clock
//! at a time as Emacs asks, each question an input whose answer runs the
//! command again.
//!
//! A clock the server found running in the files, rather than clocked
//! into, is a dangling clock here, as it is in an Emacs that has just
//! started: clocking in asks about it (`org-clock-auto-clock-resolution`),
//! and keeping it (`k`) makes it the clock.

use lsp_types::request::ShowDocument;
use lsp_types::{Range, ShowDocumentParams, Uri};
use org_core::clock;
use org_core::date::DateTime;
use org_core::Refusal;
use serde_json::{json, Value};

use super::{running_lines, Context, RunningClock, Server};
use crate::commands;
use crate::config::FlagOrName;

/// `org-clock-auto-clock-resolution`: when clocking in first resolves the
/// open clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoResolution {
    Never,
    Always,
    WhenNoClockIsRunning,
}

impl AutoResolution {
    pub(super) fn of(option: Option<&FlagOrName>) -> Self {
        match option {
            Some(FlagOrName::Flag(false)) => AutoResolution::Never,
            Some(FlagOrName::Flag(true)) => AutoResolution::Always,
            _ => AutoResolution::WhenNoClockIsRunning,
        }
    }
}

/// A resolution under way (`org-clock-resolving-clocks`).
pub(super) struct Resolving {
    /// The clocks still to ask about, the one being asked about first.
    clocks: Vec<RunningClock>,
    /// `org-clock-resolving-clocks-due-to-idleness`: when the idle time
    /// began, in minutes. The clock is then resolved back to it, instead
    /// of to its start (`last-valid`).
    idle_since: Option<i64>,
    /// `org-clock-clocking-in`: resolving before a clock-in, which nothing
    /// clocks into.
    clocking_in: bool,
    /// `org-clock-leftover-time` as the clock-in that resolves took it.
    pub(super) leftover: Option<DateTime>,
}

/// What a resolution does next: ask about a clock, or nothing more.
pub(super) enum Resolved {
    Asking(Value),
    Done(Resolving),
}

/// When `org-clock-resolve-clock` resolves a clock to.
enum ResolveTo {
    /// It is canceled.
    Cancel,
    /// It runs on, or stops now.
    Now,
    /// It stops then, in minutes.
    At(i64),
}

/// The keys `org-clock-resolve` reads, with the text of its `*Org Clock*`
/// window for each.
const KEYS: [(&str, &str); 13] = [
    ("i", "Ignore this question; the same as keeping all the idle time."),
    ("q", "Ignore this question; the same as keeping all the idle time."),
    ("k", "Keep X minutes of the idle time (default is all)."),
    ("K", "Keep X minutes of the idle time (default is all), then CLOCKED OUT."),
    (
        "t",
        "Like ‘k’, but will ask you to specify a time (when you got distracted away), instead of a number of minutes.",
    ),
    ("T", "Like ‘k’, but will ask you to specify a time (when you got distracted away), then CLOCKED OUT."),
    ("g", "Indicate that you \"got back\" X minutes ago."),
    ("G", "Indicate that you \"got back\" X minutes ago, then CLOCKED OUT."),
    ("s", "Subtract the idle time from the current clock."),
    ("S", "Subtract the idle time from the current clock, then CLOCKED OUT."),
    ("C", "Cancel the open timer altogether.  It will be as though you never clocked in."),
    ("j", "Jump to the current clock, to make manual adjustments."),
    ("J", "Jump to the current clock, to make manual adjustments, then CLOCKED OUT."),
];

fn minutes_to_time(minutes: i64) -> DateTime {
    DateTime { day: minutes.div_euclid(1440), minute: minutes.rem_euclid(1440) }
}

impl Server {
    /// The clock that runs, if it is one Emacs would know of
    /// (`org-clock-is-active`): not one found running in the files.
    pub(super) fn active_clock(&self) -> Option<&RunningClock> {
        self.running.as_ref().filter(|running| self.found_clock.as_ref() != Some(*running))
    }

    /// A clock found running is no clock the next clock-in stops: it is
    /// left as it is, dangling.
    pub(super) fn forget_found_clock(&mut self) {
        if self.running.is_some() && self.running == self.found_clock {
            self.running = None;
        }
    }

    /// The open clocks of `org-files-list`: the open files that are not
    /// agenda files, then the agenda files, each file's last clock first
    /// (`org-find-open-clocks`).
    fn open_clocks_to_resolve(&mut self) -> Vec<RunningClock> {
        let agenda: Vec<Uri> = self.agenda_files().into_iter().map(|(uri, _)| uri).collect();
        let mut files: Vec<Uri> = self
            .documents
            .keys()
            .filter_map(|key| key.parse::<Uri>().ok())
            .filter(|uri| !self.is_view(uri) && !agenda.contains(uri))
            .collect();
        files.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        files.extend(agenda);
        let mut found = Vec::new();
        for uri in files {
            let lines = self.with_source(&uri, |text, _, _, _, _| running_lines(text)).unwrap_or_default();
            for (_, start, head) in lines.into_iter().rev() {
                found.push(RunningClock { uri: uri.clone(), start, head });
            }
        }
        found
    }

    /// `org-resolve-clocks` (`C-c C-x C-z`): each clock that runs in the
    /// open and agenda files asked about in turn; with a count (`C-u`),
    /// only the dangling ones, those that are not the clock.
    pub(super) fn resolve_clocks(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let answering =
            context.and_then(|context| context.step.as_deref()).is_some_and(|step| step.starts_with("resolve"));
        if answering && self.resolving.as_ref().is_some_and(|resolving| resolving.idle_since.is_none()) {
            let context = context.expect("an answer comes with its context");
            return Ok(match self.resolve_answer(context)? {
                Resolved::Asking(question) => question,
                Resolved::Done(_) => Value::Null,
            });
        }
        let only_dangling = context.and_then(|context| context.count).is_some_and(|count| count != 0);
        let mut clocks = self.open_clocks_to_resolve();
        if only_dangling {
            let active = self.active_clock().cloned();
            clocks.retain(|clock| Some(clock) != active.as_ref());
        }
        self.resolving = Some(Resolving { clocks, idle_since: None, clocking_in: false, leftover: None });
        Ok(match self.resolve_next() {
            Resolved::Asking(question) => question,
            Resolved::Done(_) => Value::Null,
        })
    }

    /// Before `org-clock-in` clocks in: with `org-clock-auto-clock-resolution`,
    /// the open clocks are resolved first (`Err` asks about one). `Ok` is
    /// the `org-clock-leftover-time` the clock-in took before resolving.
    pub(super) fn resolve_before_clock_in(
        &mut self,
        context: &Context,
    ) -> std::result::Result<std::result::Result<Option<DateTime>, Value>, Refusal> {
        let answering = context.step.as_deref().is_some_and(|step| step.starts_with("resolve"));
        if answering && self.resolving.as_ref().is_some_and(|resolving| resolving.clocking_in) {
            return Ok(match self.resolve_answer(context)? {
                Resolved::Asking(question) => Err(question),
                Resolved::Done(resolving) => Ok(resolving.leftover),
            });
        }
        let leftover = self.clock_leftover;
        let interrupting = self.active_clock().is_some();
        let resolve = match self.clock_auto_resolution {
            AutoResolution::Never => false,
            AutoResolution::Always => true,
            AutoResolution::WhenNoClockIsRunning => !interrupting,
        };
        if !resolve {
            return Ok(Ok(leftover));
        }
        self.clock_leftover = None;
        let clocks = self.open_clocks_to_resolve();
        self.resolving = Some(Resolving { clocks, idle_since: None, clocking_in: true, leftover });
        Ok(match self.resolve_next() {
            Resolved::Asking(question) => Err(question),
            Resolved::Done(resolving) => Ok(resolving.leftover),
        })
    }

    /// Note a message from the editor: one after `org-clock-idle-time`
    /// without any while a clock runs asks what to do with the idle time
    /// (`org-resolve-clocks-if-idle`).
    pub(super) fn notice_activity(&mut self) {
        let now = clock::minutes_of(self.date_time());
        let asking = self.resolving.as_ref().is_some_and(|resolving| resolving.idle_since.is_some());
        if let (Some(idle), Some(last)) = (self.clock_idle_time, self.last_activity) {
            if let Some(running) = self.active_clock().cloned().filter(|_| !asking && now - last > idle) {
                let uri = running.uri.as_str().to_string();
                self.resolving = Some(Resolving {
                    clocks: vec![running],
                    idle_since: Some(last),
                    clocking_in: false,
                    leftover: None,
                });
                if let Resolved::Asking(question) = self.resolve_next() {
                    let argument = json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 } });
                    self.prompt_with_messages(commands::CLOCK_RESOLVE_IDLE.to_string(), argument, &question);
                }
            }
        }
        self.last_activity = Some(now);
    }

    /// The answer to the idle question.
    pub(super) fn resolve_idle(&mut self, context: Option<&Context>) -> std::result::Result<Value, Refusal> {
        let context = context.ok_or("No idle time to resolve")?;
        if !self.resolving.as_ref().is_some_and(|resolving| resolving.idle_since.is_some()) {
            return Err("No idle time to resolve".into());
        }
        Ok(match self.resolve_answer(context)? {
            Resolved::Asking(question) => question,
            Resolved::Done(_) => Value::Null,
        })
    }

    /// The question about the next clock still open, or the end of the
    /// resolution.
    fn resolve_next(&mut self) -> Resolved {
        loop {
            let Some(resolving) = self.resolving.as_mut() else {
                return Resolved::Done(Resolving {
                    clocks: Vec::new(),
                    idle_since: None,
                    clocking_in: false,
                    leftover: None,
                });
            };
            let Some(clock) = resolving.clocks.first().cloned() else {
                return Resolved::Done(self.resolving.take().expect("a resolution"));
            };
            // One resolved already, by the answer about another.
            if self.clock_line(&clock).is_none() {
                self.resolving.as_mut().expect("a resolution").clocks.remove(0);
                continue;
            }
            return Resolved::Asking(self.resolve_question(&clock));
        }
    }

    /// What `org-clock-resolve` asks about `clock`, after showing it
    /// (`org-clock-jump-to-current-clock`), unless for idle time.
    fn resolve_question(&mut self, clock: &RunningClock) -> Value {
        let now = clock::minutes_of(self.date_time());
        let idle_since = self.resolving.as_ref().and_then(|resolving| resolving.idle_since);
        let prompt = match idle_since {
            Some(since) => format!("Clocked in & idle for {:.1} mins", (now - since) as f64),
            None => {
                self.show_clock(clock);
                format!("Dangling clock started {} mins ago", now - clock::minutes_of(clock.start))
            }
        };
        let choices: Vec<Value> = KEYS.iter().map(|(key, label)| json!({ "key": key, "label": label })).collect();
        let mut input = json!({
            "kind": "key",
            "prompt": format!("{prompt} [jkKtTgGSscCiq]? "),
            "value": "",
            "choices": choices,
            "context": { "step": "resolve" },
        });
        // `org-clock-resolve-expert`: no `*Org Clock*` window.
        if self.clock_resolve_expert {
            input["expert"] = json!(true);
        }
        json!({ "input": input })
    }

    /// Shows where `clock` is, after its timestamp.
    fn show_clock(&mut self, clock: &RunningClock) {
        let encoding = self.client.encoding;
        let Some(position) = self
            .with_source(&clock.uri, |text, index, _, _, _| {
                let (line, ..) = running_lines(text)
                    .into_iter()
                    .find(|(_, start, head)| *start == clock.start && *head == clock.head)?;
                let end = line + text[line..].find(']').map_or(0, |at| at + 1);
                Some(index.position(text, end, encoding))
            })
            .flatten()
        else {
            return;
        };
        let params = ShowDocumentParams {
            uri: clock.uri.clone(),
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(position, position)),
        };
        self.send_request::<ShowDocument>(params, None);
    }

    /// The answer to a question of the resolution under way: a key, then
    /// for some a number of minutes or a time. Resolves the clock asked
    /// about and asks about the next one.
    fn resolve_answer(&mut self, context: &Context) -> std::result::Result<Resolved, Refusal> {
        let Some(clock) = self.resolving.as_ref().and_then(|resolving| resolving.clocks.first().cloned()) else {
            return Ok(self.resolve_next());
        };
        let resolving = self.resolving.as_ref().expect("a resolution");
        let (idle_since, clocking_in) = (resolving.idle_since, resolving.clocking_in);
        let now = clock::minutes_of(self.date_time());
        let last_valid = idle_since.unwrap_or_else(|| clock::minutes_of(clock.start));
        let default = now - last_valid;
        let input = context.input.clone().unwrap_or_default();
        let step = context.step.as_deref().unwrap_or("resolve");
        let key = match step {
            "resolve" => input.clone(),
            _ => context.answers.clone().unwrap_or_default(),
        };
        let accepted = ["k", "K", "g", "G", "s", "S", "C", "j", "J", "i", "q", "t", "T"];
        if !accepted.contains(&key.as_str()) {
            // `ding`, and the question again.
            return Ok(Resolved::Asking(self.resolve_question(&clock)));
        }
        // How many minutes, or when: `read-number`, `org-read-date`.
        let number = |prompt: &str, step: &str| {
            json!({ "input": {
                "kind": "text",
                "prompt": format!("{prompt} (default {default}): "),
                "value": "",
                "context": { "step": step, "answers": key },
            } })
        };
        let (mut keep, mut gotback) = (None, None);
        match (key.as_str(), step) {
            ("k" | "K", "resolve") => return Ok(Resolved::Asking(number("Keep how many minutes", "resolve-keep"))),
            ("g" | "G", "resolve") => {
                return Ok(Resolved::Asking(number("Got back how many minutes ago", "resolve-gotback")));
            }
            ("t" | "T", "resolve") => {
                return Ok(Resolved::Asking(json!({ "input": {
                    "kind": "date",
                    "prompt": "Date+time",
                    "value": "",
                    "context": { "step": "resolve-date", "answers": key },
                } })));
            }
            (_, "resolve-keep" | "resolve-gotback") => {
                let minutes = if input.trim().is_empty() {
                    default
                } else {
                    match input.trim().parse::<f64>() {
                        Ok(minutes) => minutes.floor() as i64,
                        Err(_) => {
                            self.message = Some("Please enter a number.".to_string());
                            let (prompt, step) = if step == "resolve-keep" {
                                ("Keep how many minutes", step)
                            } else {
                                ("Got back how many minutes ago", step)
                            };
                            return Ok(Resolved::Asking(number(prompt, step)));
                        }
                    }
                };
                if step == "resolve-keep" {
                    keep = Some(minutes);
                } else {
                    gotback = Some(minutes);
                }
            }
            (_, "resolve-date") => {
                let read = org_core::read_date::read_date(&input, None, self.date_time(), &self.read_date)?;
                let (hour, minute) = read.time.unwrap_or((0, 0));
                let at = clock::minutes_of(DateTime::new(read.date, hour, minute));
                keep = Some(at - last_valid);
            }
            _ => {}
        }
        self.resolving.as_mut().expect("a resolution").clocks.remove(0);
        let key = key.chars().next().unwrap_or('i');
        match key {
            'j' | 'J' => {
                self.show_clock(&clock);
                if key == 'J' {
                    self.resolve_clock(&clock, ResolveTo::Now, None, true, false, clocking_in)?;
                }
            }
            'i' | 'q' => {}
            _ => {
                let subtract = matches!(key, 's' | 'S');
                // Less than a minute on the clock before the idle time, all
                // of it subtracted: as if it never ran.
                let barely_started = last_valid - clock::minutes_of(clock.start) < 1;
                let start_over = subtract && barely_started;
                let to = if key == 'C' || start_over {
                    ResolveTo::Cancel
                } else if subtract || gotback == Some(0) {
                    ResolveTo::At(last_valid)
                } else if keep == Some(default) || gotback == Some(default) {
                    ResolveTo::Now
                } else if let Some(keep) = keep {
                    ResolveTo::At(last_valid + keep)
                } else if let Some(gotback) = gotback {
                    ResolveTo::At(now - gotback)
                } else {
                    return Err("Unexpected, please report this as a bug".into());
                };
                let clock_out_time = gotback.map(|_| last_valid);
                let close = matches!(key, 'K' | 'G' | 'S' | 'T');
                let restart = start_over && !matches!(key, 'K' | 'G' | 'S' | 'C');
                self.resolve_clock(&clock, to, clock_out_time, close, restart, clocking_in)?;
            }
        }
        Ok(self.resolve_next())
    }

    /// `org-clock-resolve-clock`: `clock` canceled, kept running, or
    /// stopped at a time; with `close` it stays stopped, else its entry is
    /// clocked into again, unless the resolution is for a clock-in.
    fn resolve_clock(
        &mut self,
        clock: &RunningClock,
        to: ResolveTo,
        clock_out_time: Option<i64>,
        close: bool,
        restart: bool,
        clocking_in: bool,
    ) -> std::result::Result<(), Refusal> {
        let (text, version, line) = self.clock_line(clock).ok_or("Clock start time is gone")?;
        // The headline stays where it is: the clock's lines are below it.
        let heading = Self::heading_above(&text, line).ok_or("Clock start time is gone")?;
        let now = clock::minutes_of(self.clock_time(false));
        match to {
            ResolveTo::Cancel => {
                let canceled = clock::cancel(&text, line, &self.clock_settings)?;
                if self.running.as_ref() == Some(clock) {
                    self.running = None;
                }
                self.message = Some("Clock canceled".to_string());
                let change = (clock.uri.clone(), version, text, canceled);
                if restart && !clocking_in {
                    self.forget_found_clock();
                    let start = self.clock_time(true);
                    self.clock_in_after(clock.uri.clone(), heading, start, vec![change])?;
                } else {
                    self.send_resolution(change);
                    self.publish_clock();
                }
            }
            ResolveTo::Now => {
                if close || clocking_in {
                    self.clock_out_at(clock, minutes_to_time(now))?;
                } else if self.active_clock() != Some(clock) {
                    // `org-clock-clock-in` with `org-clock-in-resume`: the
                    // clock goes on from its start, the one that ran stops.
                    self.forget_found_clock();
                    let resume = std::mem::replace(&mut self.clock_in_resume, true);
                    let start = self.clock_time(true);
                    let result = self.clock_in_after(clock.uri.clone(), heading, start, Vec::new());
                    self.clock_in_resume = resume;
                    result?;
                }
            }
            ResolveTo::At(at) => {
                if at > now {
                    return Err("RESOLVE-TO must refer to a time in the past".into());
                }
                self.clock_out_at(clock, minutes_to_time(clock_out_time.unwrap_or(at)))?;
                // Before a clock-in, nothing more; closed, the time it
                // stopped is left over; else its entry is clocked again.
                if clocking_in {
                    return Ok(());
                }
                if close {
                    self.clock_leftover = clock_out_time.is_none().then(|| minutes_to_time(at));
                } else {
                    let start = if clock_out_time.is_some() { minutes_to_time(at) } else { self.clock_time(true) };
                    self.forget_found_clock();
                    self.clock_in_after(clock.uri.clone(), heading, start, Vec::new())?;
                }
            }
        }
        Ok(())
    }

    /// `org-clock-clock-out`: `clock` stopped at `at`, as `org-clock-out`
    /// stops the clock.
    fn clock_out_at(&mut self, clock: &RunningClock, at: DateTime) -> std::result::Result<(), Refusal> {
        let (text, version, line) = self.clock_line(clock).ok_or("Clock start time is gone")?;
        let out = clock::clock_out(&text, line, at, &self.clock_settings)?;
        let new_text = match Self::heading_above(&text, line) {
            Some(heading) => self.switch_state(&out.text, heading, self.clock_out_switch.as_deref())?,
            None => out.text,
        };
        self.clock_out_time = Some(at);
        self.message = Some(out.message);
        if self.running.as_ref() == Some(clock) {
            self.running = None;
        }
        self.send_resolution((clock.uri.clone(), version, text, new_text));
        self.publish_clock();
        Ok(())
    }

    /// A change a resolution makes: sent, unless the resolution is for a
    /// clock-in, which sends it with its own change in one edit (both
    /// change the text of the same version).
    fn send_resolution(&mut self, change: (Uri, Option<i32>, String, String)) {
        if self.resolving.as_ref().is_some_and(|resolving| resolving.clocking_in) {
            self.resolved_changes.push(change);
        } else {
            self.send_texts(vec![change]);
        }
    }

    /// The changes of a resolution before a clock-in that clocked nothing
    /// in (it asks something first, or fails), sent alone.
    pub(super) fn send_resolved_changes(&mut self) {
        let changes = std::mem::take(&mut self.resolved_changes);
        if !changes.is_empty() {
            self.send_texts(changes);
        }
    }
}
