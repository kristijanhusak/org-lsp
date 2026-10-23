//! `org-lsp notify`: reminders of the day's appointments and a list of
//! deadlines, outside any editor, as Emacs's `appt.el` reminds of what
//! `org-agenda-to-appt` gives it. Cron, a systemd timer, launchd or the
//! Task Scheduler runs it each minute (`scripts/install.sh notifications`);
//! it keeps nothing between runs, and tells of what is due that minute.
//!
//! The appointments are those `org-agenda-to-appt` finds with its
//! defaults: the entries of today's agenda with a time of day (scheduled,
//! deadlines and timestamps, repeaters and diary sexps in timestamps
//! included), not done. Each is reminded of `appt-message-warning-time`
//! minutes before it and each `appt-display-interval` minutes after that,
//! as `appt-check` does, said as `appt-disp-window` says it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use org_core::agenda::{self, ItemKind, Matcher, Now};
use org_core::outline::Outline;

use crate::config::{Options, WarningTime};

pub const USAGE: &str = "\
Usage: org-lsp notify [OPTIONS] [FILE...]

Remind of what is due this minute: today's appointments (the entries of
the agenda files with a time of day, as Emacs's org-agenda-to-appt finds
them), each appt_message_warning_time minutes before it and every
appt_display_interval minutes after, and at notification_deadline_summary_time
the deadlines of the day's agenda. Meant to be run each minute (cron, a
systemd timer: scripts/install.sh notifications); it does nothing when
nothing is due. The options are those of the shared configuration file
(`org-lsp config` shows it), with --options on top.

Options:
  --now \"YYYY-MM-DD HH:MM\"  pretend this is the current time
  --dry-run                 print the notifications (title, then text) and
                            send none
  --options JSON            options over those of the configuration file
  --no-config               read no configuration file: --options alone";

/// A notification: its title and its text, and for appointments, each
/// appointment's own title, text and values, which `notification_command`
/// runs with when it has a placeholder of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub parts: Vec<Part>,
}

/// One appointment of a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub title: String,
    pub body: String,
    pub values: Values,
}

/// What the placeholders of an entry stand for, by their letter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values(Vec<(char, String)>);

impl Values {
    fn get(&self, letter: char) -> Option<String> {
        self.0.iter().find(|(name, _)| *name == letter).map(|(_, value)| value.clone())
    }

    fn set(&mut self, letter: char, value: String) {
        self.0.retain(|(name, _)| *name != letter);
        self.0.push((letter, value));
    }
}

/// The placeholders of an entry: headline, keyword, priority, tags,
/// category, scheduled, deadline, time, minutes left, effort, file path,
/// file name, line and the line `appt.el` shows.
const ENTRY_PLACEHOLDERS: &[char] = &['h', 'k', 'p', 'T', 'c', 's', 'd', 'w', 'm', 'e', 'f', 'F', 'l', 'a'];

/// An appointment of `appt.el`: its time in minutes after midnight, its
/// message (`HH:MM` and the entry's text), and the minutes before it that
/// its `APPT_WARNTIME` property gives; and the values of its entry's
/// placeholders. `lead` is org-lsp's own: the minutes of the warning
/// cookie in hours of its deadline or timestamp (`-1h`), which
/// `notification_use_timestamp_warning` reminds at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appointment {
    pub minute: i64,
    pub message: String,
    pub warntime: Option<i64>,
    pub lead: Option<i64>,
    pub values: Values,
}

/// What a run comes to: the notifications due, how they are sent, and
/// what is wrong with the configuration or the files.
pub struct Run {
    pub notifications: Vec<Notification>,
    pub dry_run: bool,
    pub delivery: Delivery,
    pub problems: Vec<String>,
}

/// How notifications are sent: as desktop notifications, through a
/// program of one's own, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub desktop: bool,
    pub command: Option<Vec<String>>,
    pub icon: Icon,
}

/// The image of desktop notifications (`notification_icon`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
    /// None.
    None,
    /// An image of one's own.
    File(PathBuf),
    /// The logo, written to this file of the data directory the first
    /// time it is shown; with no data directory, none.
    Logo(Option<PathBuf>),
}

/// The org-lsp logo, as the VS Code extension shows it.
pub const LOGO: &[u8] = include_bytes!("../../../editors/vscode/icon.png");

impl Icon {
    /// The image to show: the logo written out first if it is not there
    /// yet, or none if it cannot be.
    fn path(&self) -> Option<PathBuf> {
        match self {
            Icon::None | Icon::Logo(None) => None,
            Icon::File(path) => Some(path.clone()),
            Icon::Logo(Some(path)) => write_logo(path).then(|| path.clone()),
        }
    }
}

/// Whether [`LOGO`] is at `path`: written there unless it already is,
/// through a file of its own renamed into place, so that a notification
/// never shows half of it.
fn write_logo(path: &Path) -> bool {
    if std::fs::read(path).is_ok_and(|bytes| bytes == LOGO) {
        return true;
    }
    let Some(directory) = path.parent() else { return false };
    let temporary = directory.join(format!(".icon.png.{}", std::process::id()));
    let written = std::fs::create_dir_all(directory)
        .and_then(|()| std::fs::write(&temporary, LOGO))
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written.is_ok()
}

/// Where org-lsp keeps its data: `$XDG_DATA_HOME/org-lsp`, else
/// `~/.local/share/org-lsp`; on macOS `~/Library/Application
/// Support/org-lsp`, on Windows `%LOCALAPPDATA%\org-lsp`.
pub fn data_directory(var: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        return set("LOCALAPPDATA").map(|local| PathBuf::from(local).join("org-lsp"));
    }
    let home = set("HOME");
    if cfg!(target_os = "macos") {
        return home.map(|home| PathBuf::from(home).join("Library").join("Application Support").join("org-lsp"));
    }
    if let Some(data) = set("XDG_DATA_HOME") {
        return Some(PathBuf::from(data).join("org-lsp"));
    }
    home.map(|home| PathBuf::from(home).join(".local").join("share").join("org-lsp"))
}

/// `org-lsp notify` with the arguments after the subcommand: the options
/// of the shared configuration file (`env` reads the environment) with
/// those of `--options` on top, and its agenda files unless files are
/// named.
pub fn prepare(arguments: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Run, String> {
    let mut now = crate::cli::now();
    let mut dry_run = false;
    let mut editor: Option<serde_json::Value> = None;
    let mut use_file = true;
    let mut files: Vec<String> = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--now" => now = parse_now(arguments.next().ok_or("--now needs a value")?)?,
            "--dry-run" => dry_run = true,
            "--no-config" => use_file = false,
            "--options" => {
                let json = arguments.next().ok_or("--options needs a value")?;
                editor = Some(serde_json::from_str(json).map_err(|error| format!("--options: {error}"))?);
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            file => files.push(file.to_string()),
        }
    }
    let mut editor = match editor {
        Some(serde_json::Value::Object(fields)) => fields,
        Some(_) => return Err("--options: not an object".to_string()),
        None => serde_json::Map::new(),
    };
    if !use_file {
        editor.insert("config_file".to_string(), serde_json::Value::Bool(false));
    }
    let loaded = crate::config::load(Some(serde_json::Value::Object(editor)), env);
    let options = &loaded.options;
    let mut problems = loaded.problems.clone();
    // The agenda files of the configuration file, never the Org files
    // under the directory it runs in, which for cron is the home
    // directory.
    if files.is_empty() {
        let at = org_core::date::DateTime { day: now.today, minute: now.minute };
        files = crate::mcp::agenda_paths_at(options, None, at).iter().map(|path| path.display().to_string()).collect();
        if files.is_empty() {
            return Err("no files given, and no org_agenda_files in the configuration file".to_string());
        }
    }
    let command = options.notification_command.clone().filter(|command| !command.is_empty());
    let icon = match options.notification_icon.as_deref() {
        Some("") => Icon::None,
        Some(path) => Icon::File(crate::config::expand_home(path, env)),
        None => Icon::Logo(data_directory(env).map(|directory| directory.join("icon.png"))),
    };
    let delivery = Delivery { desktop: options.notification_desktop.unwrap_or(command.is_none()), command, icon };
    let notifications = due(&files, options, now, &mut problems);
    Ok(Run { notifications, dry_run, delivery, problems })
}

/// `--now`: `YYYY-MM-DD HH:MM`.
fn parse_now(text: &str) -> Result<Now, String> {
    let (date, time) = text
        .trim()
        .split_once(char::is_whitespace)
        .ok_or_else(|| format!("`{text}` is not a date and time; expected \"YYYY-MM-DD HH:MM\""))?;
    Ok(Now { today: crate::cli::parse_date(date)?, minute: crate::cli::parse_time(time.trim())? })
}

/// The notifications due at `now` from the Org files `files`.
pub fn due(files: &[String], options: &Options, now: Now, problems: &mut Vec<String>) -> Vec<Notification> {
    crate::mcp::set_variables(options);
    let todo = options.todo_keywords();
    let inheritance = options.inheritance();
    let mut outlines = Vec::new();
    for file in files {
        match std::fs::read(file) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let parse = org_syntax::parse(&text, &todo);
                let name = crate::cli::file_name(Path::new(file));
                let mut outline = Outline::with_inheritance(&parse, &text, &name, &inheritance);
                outline.file = Some(file.clone());
                outlines.push(outline);
            }
            Err(error) => problems.push(format!("{file}: {error}")),
        }
    }
    let outlines: Vec<&Outline> = outlines.iter().collect();
    let config = options.agenda_config();
    let matcher = options.notification_match.as_deref().filter(|text| !text.trim().is_empty()).map(|text| {
        let matcher = agenda::expanded(&Matcher::new(text), &outlines, &config);
        let clock = agenda::matcher::Clock { today: now.today, minute: now.minute };
        move |outline: &Outline, entry: usize| matcher.matches(outline, entry, false, clock)
    });
    let wanted = |source: Option<agenda::Source>| {
        source.is_some_and(|source| matcher.as_ref().is_none_or(|matches| matches(outlines[source.file], source.entry)))
    };

    let mut notifications = Vec::new();
    let warning = options.appt_message_warning_time.clone().unwrap_or(WarningTime::Minutes(12));
    let interval = options.appt_display_interval.unwrap_or(3).max(1);
    let use_lead = options.notification_use_timestamp_warning.unwrap_or(false);
    let mut reminded = Vec::new();
    for appointment in appointments(&outlines, now.today, now.today, &config, &wanted) {
        let left = appointment.minute - now.minute;
        let warning = appointment.warntime.map_or_else(|| warning.clone(), WarningTime::Minutes);
        if reminds(&warning, interval, left) || (use_lead && appointment.lead == Some(left)) {
            reminded.push((left, appointment));
        }
    }
    // A warning in hours of tomorrow's appointments may fall today: `-3h`
    // at 01:00 reminds at 22:00. Only that one; `appt.el` has today's
    // appointments alone.
    if use_lead {
        for appointment in appointments(&outlines, now.today + 1, now.today, &config, &wanted) {
            let left = appointment.minute + 24 * 60 - now.minute;
            if appointment.lead == Some(left) {
                reminded.push((left, appointment));
            }
        }
    }
    if !reminded.is_empty() {
        let format = |format: &Option<String>| format.clone().filter(|format| !format.is_empty());
        let (title, body) = (format(&options.notification_title_format), format(&options.notification_body_format));
        notifications.push(remind(&reminded, title.as_deref(), body.as_deref()));
    }

    let summary =
        options.notification_deadline_summary_time.as_deref().and_then(|time| crate::cli::parse_time(time).ok());
    if summary == Some(now.minute) {
        let format = options.notification_deadline_line_format.as_deref().filter(|format| !format.is_empty());
        let lines = deadlines(&outlines, now.org_today(config.extend_today_until), &config, &wanted, format);
        if !lines.is_empty() {
            notifications.push(Notification { title: "Deadlines".to_string(), body: lines.join("\n"), parts: vec![] });
        }
    }
    notifications
}

/// The notification of the appointments `reminded`, each with the
/// minutes left before it: `appt.el`'s title and text, or those of the
/// formats `title` and `body`, a line of text each, the title the first
/// one's.
fn remind(reminded: &[(i64, Appointment)], title: Option<&str>, body: Option<&str>) -> Notification {
    let parts: Vec<Part> = reminded
        .iter()
        .map(|(left, appointment)| {
            let mut values = appointment.values.clone();
            values.set('m', left.to_string());
            let lookup = |letter| values.get(letter);
            Part {
                title: title.map_or_else(|| appt_mode_line(&[*left]), |format| expand(format, &lookup)),
                body: body.map_or_else(|| appointment.message.clone(), |format| expand(format, &lookup)),
                values,
            }
        })
        .collect();
    let minutes: Vec<i64> = reminded.iter().map(|(left, _)| *left).collect();
    let messages: Vec<String> = parts.iter().map(|part| part.body.clone()).collect();
    let mut notification = match body {
        None => appt_display(&minutes, &messages),
        Some(_) => Notification { title: appt_mode_line(&minutes), body: messages.join("\n"), parts: vec![] },
    };
    if title.is_some() {
        notification.title = parts[0].title.clone();
    }
    notification.parts = parts;
    notification
}

/// The values of the placeholders of the entry `source` stands for:
/// headline, keyword, priority, tags as the agenda shows them, category,
/// planning timestamps, effort, file and line.
fn entry_values(files: &[&Outline], source: agenda::Source, config: &agenda::Config) -> Values {
    let outline = files[source.file];
    let entry = agenda::entry(files, source);
    let line = &outline.text[entry.offset..entry.line_end];
    let parts = org_core::outline::complex_heading_parts(line, entry.level, &outline.todo);
    let part = |span: Option<(usize, usize)>| span.map_or("", |(start, end)| &line[start..end]);
    let priority = part(parts.priority).trim_start_matches("[#").trim_end_matches(']');
    let inherited = config.inherited_tags(ItemKind::Scheduled);
    let tags: Vec<&str> =
        entry.tags.iter().filter(|tag| inherited || !tag.inherited).map(|tag| tag.name.as_str()).collect();
    let stamp = |planning: &Option<org_core::outline::Planning>| {
        planning.as_ref().map_or_else(String::new, |planning| {
            if planning.active {
                format!("<{}>", planning.raw)
            } else {
                format!("[{}]", planning.raw)
            }
        })
    };
    let path = outline.file.clone().unwrap_or_default();
    let name = Path::new(&path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut values = Values::default();
    values.set('h', org_core::link::descriptions_only(part(parts.title)).trim().to_string());
    values.set('k', part(parts.keyword).to_string());
    values.set('p', priority.to_string());
    values.set('T', if tags.is_empty() { String::new() } else { format!(":{}:", tags.join(":")) });
    values.set('c', entry.category.clone());
    values.set('s', stamp(&entry.scheduled));
    values.set('d', stamp(&entry.deadline));
    values.set('e', agenda::effort(entry).unwrap_or_default().to_string());
    values.set('f', path);
    values.set('F', name);
    values.set('l', (outline.text[..entry.offset].matches('\n').count() + 1).to_string());
    values
}

/// `org-agenda-to-appt` with its defaults: the entries of the agenda of
/// `today` (`:deadline*`, `:scheduled*` and `:timestamp`) that have a time
/// of day and are not done, as `appt-add` keeps them: by time, each
/// once. `wanted` says which entries to look at. `day` is the day whose
/// agenda it reads, `today` for all but the warnings of tomorrow.
pub fn appointments(
    files: &[&Outline],
    day: org_core::date::Day,
    today: org_core::date::Day,
    config: &agenda::Config,
    wanted: &dyn Fn(Option<agenda::Source>) -> bool,
) -> Vec<Appointment> {
    // No time in the prefix, as Org has none compiled until an agenda is
    // made: the text keeps the spaces around its timestamps.
    let mut config = config.clone();
    config.prefix_format.agenda = "%s".to_string();
    let mut found: Vec<Appointment> = Vec::new();
    for item in agenda::day_items(files, day, today, &config) {
        if !matches!(item.kind, ItemKind::Deadline | ItemKind::Scheduled | ItemKind::Timestamp) || !wanted(item.source)
        {
            continue;
        }
        let (Some(time), Some(source)) = (item.time_of_day, item.source) else { continue };
        let outline = files[source.file];
        // The `txt` of the line: its text without the prefix, its links
        // shown by their descriptions.
        let event = org_core::link::descriptions_only(item.text.get(item.heading..).unwrap_or_default());
        let event = event.trim();
        if is_done(event, &outline.todo) {
            continue;
        }
        let warntime = outline.property(source.entry, "APPT_WARNTIME").and_then(|value| string_to_number(&value));
        let message = format!("{} {event}", time_string(time));
        let mut values = entry_values(files, source, &config);
        values.set('w', time_string_padded(time));
        values.set('a', message.clone());
        let lead = lead_minutes(files, &item);
        let appointment = Appointment { minute: time / 100 * 60 + time % 100, message, warntime, lead, values };
        // `appt-add` keeps one of each time, text and warning time; the
        // one kept takes the other's warning in hours if it has none.
        let same = |other: &&mut Appointment| {
            (other.minute, &other.message, other.warntime)
                == (appointment.minute, &appointment.message, appointment.warntime)
        };
        match found.iter_mut().find(same) {
            Some(kept) => kept.lead = kept.lead.or(appointment.lead),
            None => found.push(appointment),
        }
    }
    // `appt-sort-list`
    found.sort_by_key(|appointment| appointment.minute);
    found
}

/// The minutes of the warning cookie in hours (`-1h`, `--1h`) of the
/// deadline or timestamp `item` stands for. A scheduled date's cookie is
/// a delay, which hides the entry, and cookies in days, weeks, months or
/// years are the agenda's: neither counts.
fn lead_minutes(files: &[&Outline], item: &agenda::Item) -> Option<i64> {
    let source = item.source?;
    let outline = files[source.file];
    let stamp = match item.kind {
        ItemKind::Deadline => agenda::entry(files, source).deadline.as_ref()?.raw.clone(),
        ItemKind::Timestamp => {
            let text = outline.text.get(item.marker?..)?.strip_prefix('<')?;
            text[..text.find('>')?].to_string()
        }
        _ => return None,
    };
    stamp.split_whitespace().find_map(|word| {
        let word = word.strip_prefix("--").or_else(|| word.strip_prefix('-'))?;
        let hours = word.strip_suffix('h')?;
        if hours.is_empty() || !hours.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        hours.parse::<i64>().ok().map(|hours| hours * 60)
    })
}

/// The time `org-agenda-to-appt` gives `appt-add` for the military time
/// `time`: `10:00`, `07:30`, `00:30`, and `0:05` for the first minutes of
/// the day, as its regexp reads `00` and the number.
fn time_string(time: i64) -> String {
    let digits = format!("00{time}");
    let hour_at = digits.len().saturating_sub(4);
    let minute_at = digits.len() - 2;
    format!("{}:{}", &digits[hour_at..minute_at], &digits[minute_at..])
}

/// The military time `time` as `HH:MM`, for `%w`.
fn time_string_padded(time: i64) -> String {
    format!("{:02}:{:02}", time / 100, time % 100)
}

/// Whether `org-agenda-to-appt` leaves out the entry whose text is
/// `event` as done: a word of it is a TODO keyword of the file, and none a
/// keyword that is not done (`org-todo-regexp`, `org-not-done-regexp`).
fn is_done(event: &str, keywords: &org_syntax::TodoKeywords) -> bool {
    let found: Vec<&str> = keywords.names().filter(|keyword| has_word(event, keyword)).collect();
    !found.is_empty() && found.iter().all(|keyword| keywords.is_done(keyword))
}

/// Whether `word` is in `text` as a word of its own (`\<word\>`).
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Emacs's `string-to-number` of a property's value: its leading integer,
/// `0` when there is none; nothing for an empty value.
fn string_to_number(value: &str) -> Option<i64> {
    let value = value.trim_start();
    if value.is_empty() {
        return None;
    }
    let (sign, digits) = match value.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, value.strip_prefix('+').unwrap_or(value)),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    Some(sign * digits.parse::<i64>().unwrap_or(0))
}

/// Whether an appointment `left` minutes away is reminded of now: as
/// `appt-check` does from `minutes` before it each `interval` minutes, or
/// at each of the minutes of a list.
fn reminds(warning: &WarningTime, interval: i64, left: i64) -> bool {
    match warning {
        WarningTime::Minutes(minutes) => (0..=*minutes).contains(&left) && (minutes - left) % interval == 0,
        WarningTime::Each(minutes) => minutes.contains(&left),
    }
}

/// `appt-mode-line`: `Appointment in 10 minutes`, `Appointments in 5,10
/// minutes`, `Appointment now`; and, org-lsp's own, whole hours as hours,
/// as the reminder of a warning in hours comes (`Appointment in 3 hours`),
/// where `appt.el` says `in 180 minutes`.
fn appt_mode_line(minutes: &[i64]) -> String {
    let multiple = minutes.len() > 1;
    let same = minutes.iter().all(|minute| *minute == minutes[0]);
    let first = (!multiple || same).then(|| minutes[0]);
    let hours = minutes.iter().all(|minute| *minute >= 60 && minute % 60 == 0);
    let plural = |count: i64, unit: &str| format!("{count} {unit}{}", if count == 1 { "" } else { "s" });
    let when = match first {
        Some(0) => "now".to_string(),
        Some(minute) if hours => format!("in {}", plural(minute / 60, "hour")),
        Some(minute) => format!("in {}", plural(minute, "minute")),
        None if hours => {
            let all: Vec<String> = minutes.iter().map(|minute| (minute / 60).to_string()).collect();
            format!("in {} hours", all.join(","))
        }
        None => {
            let all: Vec<String> = minutes.iter().map(i64::to_string).collect();
            format!("in {} minutes", all.join(","))
        }
    };
    format!("Appointment{} {when}", if multiple { "s" } else { "" })
}

/// `appt-disp-window`: the mode line as the title, and the window's text:
/// the messages, each after its minutes when they are not all due at once.
fn appt_display(minutes: &[i64], messages: &[String]) -> Notification {
    let same = minutes.iter().all(|minute| *minute == minutes[0]);
    let body = if same {
        messages.join("\n")
    } else {
        minutes
            .iter()
            .zip(messages)
            .map(|(minute, message)| match minute {
                minute if *minute >= 60 && minute % 60 == 0 => format!("{}h: {message}", minute / 60),
                minute => format!("{minute}m: {message}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Notification { title: appt_mode_line(minutes), body, parts: vec![] }
}

/// The deadlines the agenda of `today` shows that are not done, each as
/// its category and the line's leader say it: `work: In 3 d.: TODO Report`,
/// or as the format `line` says it.
fn deadlines(
    files: &[&Outline],
    today: org_core::date::Day,
    config: &agenda::Config,
    wanted: &dyn Fn(Option<agenda::Source>) -> bool,
    line: Option<&str>,
) -> Vec<String> {
    let mut config = config.clone();
    config.prefix_format.agenda = if line.is_some() { "%s" } else { "%c: %s" }.to_string();
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    agenda::day_items(files, today, today, &config)
        .into_iter()
        .filter(|item| item.kind == ItemKind::Deadline && !item.keyword_done && wanted(item.source))
        .map(|item| {
            let (Some(format), Some(source)) = (line, item.source) else {
                return words(&org_core::link::descriptions_only(&item.text));
            };
            let (leader, heading) = item.text.split_at(item.heading.min(item.text.len()));
            let mut values = entry_values(files, source, &config);
            values.set('w', item.time_of_day.map(time_string_padded).unwrap_or_default());
            values.set('m', String::new());
            values.set('a', words(&org_core::link::descriptions_only(heading)));
            values.set('D', words(leader));
            expand(format, &|letter| values.get(letter))
        })
        .collect()
}

/// How long the program that shows a notification may take.
const SEND_TIME: Duration = Duration::from_secs(30);

impl Delivery {
    /// Send `notification` each way it goes; what failed, said.
    pub fn send(&self, notification: &Notification) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Some(command) = &self.command {
            // Once for each appointment when the command says something
            // of an entry, else once for them all.
            let each = command.iter().any(|argument| has_entry_placeholder(argument));
            let runs: Vec<(&str, &str, Option<&Values>)> = if each && !notification.parts.is_empty() {
                notification.parts.iter().map(|part| (&*part.title, &*part.body, Some(&part.values))).collect()
            } else {
                vec![(&notification.title, &notification.body, None)]
            };
            for (title, body, values) in runs {
                let arguments: Vec<String> =
                    command.iter().map(|argument| fill(argument, title, body, values)).collect();
                let mut program = Command::new(&arguments[0]);
                program.args(&arguments[1..]);
                if let Err(error) = run(&mut program) {
                    errors.push(format!("{}: {error}", arguments[0]));
                }
            }
        }
        if self.desktop {
            let icon = self.icon.path();
            let (name, mut program) = desktop(System::current(), notification, icon.as_deref(), &find_program);
            if let Err(error) = run(&mut program) {
                errors.push(format!("{name}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

/// An argument of `notification_command` with `%t` the title, `%b` the
/// text, the placeholders of an entry its `values` (empty without them)
/// and `%%` a `%`.
fn fill(argument: &str, title: &str, body: &str, values: Option<&Values>) -> String {
    expand(argument, &|letter| match letter {
        't' => Some(title.to_string()),
        'b' => Some(body.to_string()),
        letter => values
            .and_then(|values| values.get(letter))
            .or_else(|| ENTRY_PLACEHOLDERS.contains(&letter).then(String::new)),
    })
}

/// `template` with each `%x` that `value` knows replaced by its value,
/// `%%` by `%`, and any other `%x` left as it is.
fn expand(template: &str, value: &dyn Fn(char) -> Option<String>) -> String {
    let mut filled = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        let found = match (c, chars.peek()) {
            ('%', Some('%')) => Some("%".to_string()),
            ('%', Some(letter)) => value(*letter),
            _ => None,
        };
        match found {
            Some(text) => {
                filled.push_str(&text);
                chars.next();
            }
            None => filled.push(c),
        }
    }
    filled
}

/// Whether `template` has a placeholder of an entry.
fn has_entry_placeholder(template: &str) -> bool {
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.next() {
                Some(letter) if ENTRY_PLACEHOLDERS.contains(&letter) => return true,
                _ => {}
            }
        }
    }
    false
}

/// Run `program` and wait for it, until [`SEND_TIME`].
fn run(program: &mut Command) -> Result<(), String> {
    program.stdin(std::process::Stdio::null());
    // Run from the Task Scheduler with no console, a program would open a
    // window of its own (`CREATE_NO_WINDOW`).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        program.creation_flags(0x0800_0000);
    }
    let output = crate::process::output(program, SEND_TIME, 1 << 20).map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if said.is_empty() { output.status.to_string() } else { said })
}

/// The kinds of system whose desktop notifications differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    MacOs,
    Windows,
    /// Linux and the other Unix systems.
    Other,
}

impl System {
    fn current() -> System {
        if cfg!(target_os = "macos") {
            System::MacOs
        } else if cfg!(windows) {
            System::Windows
        } else {
            System::Other
        }
    }
}

/// The program named `name` in a directory of `PATH`, or on macOS in
/// one of Homebrew's, which launchd leaves out of the `PATH` of a
/// LaunchAgent.
fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let homebrew = if cfg!(target_os = "macos") { &["/opt/homebrew/bin", "/usr/local/bin"][..] } else { &[] };
    std::env::split_paths(&path)
        .chain(homebrew.iter().map(PathBuf::from))
        .map(|directory| directory.join(name))
        .find(|program| program.is_file())
}

/// The program that shows `notification` on the desktop of `system`,
/// with the image `icon`, and its name: `notify-send` (Linux and other
/// Unix systems); on macOS `terminal-notifier` when `find` finds it,
/// else `osascript`, which shows no image; PowerShell on Windows. Title
/// and text are arguments of their own, or the environment, never part
/// of a script.
fn desktop(
    system: System,
    notification: &Notification,
    icon: Option<&Path>,
    find: &dyn Fn(&str) -> Option<PathBuf>,
) -> (&'static str, Command) {
    let terminal_notifier = if system == System::MacOs { find("terminal-notifier") } else { None };
    match (system, terminal_notifier) {
        (System::MacOs, Some(terminal_notifier)) => {
            let mut program = Command::new(terminal_notifier);
            program.args(["-title", &notification.title, "-message", &notification.body, "-group", "org-lsp"]);
            if let Some(icon) = icon {
                program.arg("-appIcon").arg(icon);
            }
            ("terminal-notifier", program)
        }
        (System::MacOs, None) => {
            let mut program = Command::new("osascript");
            program.args([
                "-e",
                "on run argv",
                "-e",
                "display notification (item 2 of argv) with title (item 1 of argv)",
                "-e",
                "end run",
                &notification.title,
                &notification.body,
            ]);
            ("osascript", program)
        }
        (System::Windows, _) => {
            let mut program = Command::new("powershell.exe");
            program.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", WINDOWS_TOAST]);
            program.env("ORG_LSP_TITLE", &notification.title).env("ORG_LSP_BODY", &notification.body);
            program.env("ORG_LSP_ICON", icon.map(Path::as_os_str).unwrap_or_default());
            ("powershell.exe", program)
        }
        (System::Other, _) => {
            let mut program = Command::new("notify-send");
            program.arg("--app-name=org-lsp");
            if let Some(icon) = icon {
                let mut argument = std::ffi::OsString::from("--icon=");
                argument.push(icon);
                program.arg(argument);
            }
            program.args(["--", &notification.title, &notification.body]);
            ("notify-send", program)
        }
    }
}

/// A toast with the title and text of `ORG_LSP_TITLE` and `ORG_LSP_BODY`
/// and the image of `ORG_LSP_ICON` if it is set: BurntToast's when it is
/// installed, else Windows's own, in the name of PowerShell.
const WINDOWS_TOAST: &str = "$title = $env:ORG_LSP_TITLE; $body = $env:ORG_LSP_BODY; $icon = $env:ORG_LSP_ICON; \
if (Get-Module -ListAvailable -Name BurntToast) { \
Import-Module BurntToast; \
if ($icon) { New-BurntToastNotification -Text $title, $body -AppLogo $icon } \
else { New-BurntToastNotification -Text $title, $body } \
} else { \
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
$template = if ($icon) { [Windows.UI.Notifications.ToastTemplateType]::ToastImageAndText02 } \
else { [Windows.UI.Notifications.ToastTemplateType]::ToastText02 }; \
$xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent($template); \
$texts = $xml.GetElementsByTagName('text'); \
[void]$texts.Item(0).AppendChild($xml.CreateTextNode($title)); \
[void]$texts.Item(1).AppendChild($xml.CreateTextNode($body)); \
if ($icon) { \
$xml.GetElementsByTagName('image').Item(0).Attributes.GetNamedItem('src').NodeValue = ([System.Uri]$icon).AbsoluteUri \
}; \
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
$app = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe'; \
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($app).Show($toast) \
}";

#[cfg(test)]
mod tests {
    use super::*;

    /// The appointments `org-agenda-to-appt` made of each file of
    /// tests/appt on 22 October 2026 (scripts/appt_reference.py).
    #[test]
    fn finds_the_appointments_org_agenda_to_appt_finds() {
        let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/appt");
        let options = Options::default();
        let today = org_core::date::Date::new(2026, 10, 22).to_day();
        let mut names: Vec<_> = std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "org"))
            .collect();
        names.sort();
        assert!(!names.is_empty());
        for path in names {
            let text = std::fs::read_to_string(&path).unwrap();
            let parse = org_syntax::parse(&text, &options.todo_keywords());
            let outline = Outline::with_inheritance(&parse, &text, "file", &options.inheritance());
            let found: Vec<serde_json::Value> =
                appointments(&[&outline], today, today, &options.agenda_config(), &|_| true)
                    .into_iter()
                    .map(|appointment| {
                        serde_json::json!([appointment.minute, appointment.message, appointment.warntime])
                    })
                    .collect();
            let expected: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path.with_extension("json")).unwrap()).unwrap();
            assert_eq!(serde_json::Value::Array(found), expected, "{}", path.display());
        }
    }

    #[test]
    fn writes_the_time_as_org_agenda_to_appt_does() {
        assert_eq!(time_string(1000), "10:00");
        assert_eq!(time_string(730), "07:30");
        assert_eq!(time_string(30), "00:30");
        assert_eq!(time_string(5), "0:05");
    }

    #[test]
    fn says_what_appt_says() {
        assert_eq!(appt_mode_line(&[10]), "Appointment in 10 minutes");
        assert_eq!(appt_mode_line(&[60]), "Appointment in 1 hour");
        assert_eq!(appt_mode_line(&[180]), "Appointment in 3 hours");
        assert_eq!(appt_mode_line(&[90]), "Appointment in 90 minutes");
        assert_eq!(appt_mode_line(&[60, 120]), "Appointments in 1,2 hours");
        assert_eq!(appt_mode_line(&[1]), "Appointment in 1 minute");
        assert_eq!(appt_mode_line(&[0]), "Appointment now");
        assert_eq!(appt_mode_line(&[5, 5]), "Appointments in 5 minutes");
        assert_eq!(appt_mode_line(&[0, 0]), "Appointments now");
        assert_eq!(appt_mode_line(&[5, 10]), "Appointments in 5,10 minutes");
        let two = appt_display(&[5, 10], &["10:00 A".to_string(), "10:05 B".to_string()]);
        assert_eq!(two.body, "5m: 10:00 A\n10m: 10:05 B");
        let same = appt_display(&[5, 5], &["10:00 A".to_string(), "10:00 B".to_string()]);
        assert_eq!(same.body, "10:00 A\n10:00 B");
    }

    #[test]
    fn reminds_as_appt_check_does_or_at_the_minutes_given() {
        let at = |warning: &WarningTime, interval| {
            (-2..20).filter(|left| reminds(warning, interval, *left)).collect::<Vec<_>>()
        };
        assert_eq!(at(&WarningTime::Minutes(12), 3), [0, 3, 6, 9, 12]);
        // Not a multiple: the last reminder comes before the time.
        assert_eq!(at(&WarningTime::Minutes(10), 3), [1, 4, 7, 10]);
        assert_eq!(at(&WarningTime::Minutes(0), 3), [0]);
        assert_eq!(at(&WarningTime::Each(vec![0, 5, 10]), 3), [0, 5, 10]);
    }

    #[test]
    fn reads_numbers_as_emacs_does() {
        assert_eq!(string_to_number("30"), Some(30));
        assert_eq!(string_to_number(" 15 minutes"), Some(15));
        assert_eq!(string_to_number("soon"), Some(0));
        assert_eq!(string_to_number(""), None);
    }

    #[test]
    fn fills_in_the_title_and_text() {
        assert_eq!(fill("%t: %b (%%t) %x", "T", "B 100%", None), "T: B 100% (%t) %x");
        let mut values = Values::default();
        values.set('h', "Standup".to_string());
        assert_eq!(fill("%t %h%k%", "T", "B", Some(&values)), "T Standup%");
        assert_eq!(fill("%h|%k", "T", "B", None), "|");
        assert!(has_entry_placeholder("Tags: %T"));
        assert!(!has_entry_placeholder("%t %b %%h %x"));
    }

    #[test]
    fn leaves_out_what_is_done_as_org_agenda_to_appt_does() {
        let keywords = org_syntax::TodoKeywords::from_sequence(["TODO", "NEXT", "|", "DONE", "CANCELLED"]);
        assert!(is_done("DONE Call", &keywords));
        assert!(is_done("CANCELLED Call :work:", &keywords));
        assert!(!is_done("TODO Call", &keywords));
        assert!(!is_done("Call", &keywords));
        // Any word, as the regexps find it.
        assert!(is_done("Call about DONE things", &keywords));
        assert!(!is_done("TODO Review DONE things", &keywords));
    }

    /// The program and arguments of `program`.
    fn command_line(program: &Command) -> Vec<String> {
        std::iter::once(program.get_program())
            .chain(program.get_args())
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn shows_the_icon_with_the_program_of_each_system() {
        let notification = Notification { title: "T".to_string(), body: "B".to_string(), parts: Vec::new() };
        let icon = Path::new("/data/org-lsp/icon.png");
        let brew = "/opt/homebrew/bin/terminal-notifier";
        let with = |name: &str| (name == "terminal-notifier").then(|| PathBuf::from(brew));
        let without = |_: &str| None;
        let (name, program) = desktop(System::MacOs, &notification, Some(icon), &with);
        assert_eq!(name, "terminal-notifier");
        assert_eq!(
            command_line(&program),
            [brew, "-title", "T", "-message", "B", "-group", "org-lsp", "-appIcon", "/data/org-lsp/icon.png"]
        );
        let (_, program) = desktop(System::MacOs, &notification, None, &with);
        assert_eq!(command_line(&program), [brew, "-title", "T", "-message", "B", "-group", "org-lsp"]);
        // Elsewhere it is not looked for.
        let (name, _) = desktop(System::Other, &notification, None, &with);
        assert_eq!(name, "notify-send");
        let (name, program) = desktop(System::MacOs, &notification, Some(icon), &without);
        assert_eq!(name, "osascript");
        assert_eq!(command_line(&program)[7..], ["T", "B"]);
        let (_, program) = desktop(System::Other, &notification, Some(icon), &without);
        assert_eq!(
            command_line(&program),
            ["notify-send", "--app-name=org-lsp", "--icon=/data/org-lsp/icon.png", "--", "T", "B"]
        );
        let (_, program) = desktop(System::Windows, &notification, Some(icon), &without);
        let icon_variable = program.get_envs().find(|(name, _)| *name == "ORG_LSP_ICON").and_then(|(_, value)| value);
        assert_eq!(icon_variable, Some(icon.as_os_str()));
        let (_, program) = desktop(System::Windows, &notification, None, &without);
        let icon_variable = program.get_envs().find(|(name, _)| *name == "ORG_LSP_ICON").and_then(|(_, value)| value);
        assert_eq!(icon_variable, Some(std::ffi::OsStr::new("")));
    }

    #[test]
    fn writes_the_logo_once() {
        let directory = std::env::temp_dir().join(format!("org-lsp-notify-logo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let path = directory.join("org-lsp").join("icon.png");
        assert_eq!(Icon::Logo(Some(path.clone())).path(), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), LOGO);
        // Another image there is replaced.
        std::fs::write(&path, b"old").unwrap();
        assert_eq!(Icon::Logo(Some(path.clone())).path(), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), LOGO);
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        // Where it cannot be written: no image.
        let blocked = directory.join("file");
        std::fs::write(&blocked, b"").unwrap();
        assert_eq!(Icon::Logo(Some(blocked.join("icon.png"))).path(), None);
        assert_eq!(Icon::Logo(None).path(), None);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
