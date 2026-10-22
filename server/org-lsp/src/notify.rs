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

use std::path::Path;
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

/// A notification: its title and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// An appointment of `appt.el`: its time in minutes after midnight, its
/// message (`HH:MM` and the entry's text), and the minutes before it that
/// its `APPT_WARNTIME` property gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appointment {
    pub minute: i64,
    pub message: String,
    pub warntime: Option<i64>,
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
    let delivery = Delivery { desktop: options.notification_desktop.unwrap_or(command.is_none()), command };
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
                outlines.push(Outline::with_inheritance(&parse, &text, &name, &inheritance));
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
    let (mut minutes, mut messages) = (Vec::new(), Vec::new());
    for appointment in appointments(&outlines, now.today, &config, &wanted) {
        let left = appointment.minute - now.minute;
        let warning = appointment.warntime.map_or_else(|| warning.clone(), WarningTime::Minutes);
        if reminds(&warning, interval, left) {
            minutes.push(left);
            messages.push(appointment.message);
        }
    }
    if !minutes.is_empty() {
        notifications.push(appt_display(&minutes, &messages));
    }

    let summary =
        options.notification_deadline_summary_time.as_deref().and_then(|time| crate::cli::parse_time(time).ok());
    if summary == Some(now.minute) {
        let lines = deadlines(&outlines, now.org_today(config.extend_today_until), &config, &wanted);
        if !lines.is_empty() {
            notifications.push(Notification { title: "Deadlines".to_string(), body: lines.join("\n") });
        }
    }
    notifications
}

/// `org-agenda-to-appt` with its defaults: the entries of the agenda of
/// `today` (`:deadline*`, `:scheduled*` and `:timestamp`) that have a time
/// of day and are not done, as `appt-add` keeps them: by time, each
/// once. `wanted` says which entries to look at.
pub fn appointments(
    files: &[&Outline],
    today: org_core::date::Day,
    config: &agenda::Config,
    wanted: &dyn Fn(Option<agenda::Source>) -> bool,
) -> Vec<Appointment> {
    // No time in the prefix, as Org has none compiled until an agenda is
    // made: the text keeps the spaces around its timestamps.
    let mut config = config.clone();
    config.prefix_format.agenda = "%s".to_string();
    let mut found: Vec<Appointment> = Vec::new();
    for item in agenda::day_items(files, today, today, &config) {
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
        let appointment = Appointment {
            minute: time / 100 * 60 + time % 100,
            message: format!("{} {event}", time_string(time)),
            warntime,
        };
        if !found.contains(&appointment) {
            found.push(appointment);
        }
    }
    // `appt-sort-list`
    found.sort_by_key(|appointment| appointment.minute);
    found
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
/// minutes`, `Appointment now`.
fn appt_mode_line(minutes: &[i64]) -> String {
    let multiple = minutes.len() > 1;
    let same = minutes.iter().all(|minute| *minute == minutes[0]);
    let first = (!multiple || same).then(|| minutes[0]);
    let when = match first {
        Some(0) => "now".to_string(),
        Some(minute) => format!("in {minute} minute{}", if minute == 1 { "" } else { "s" }),
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
            .map(|(minute, message)| format!("{minute}m: {message}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Notification { title: appt_mode_line(minutes), body }
}

/// The deadlines the agenda of `today` shows that are not done, each as
/// its category and the line's leader say it: `work: In 3 d.: TODO Report`.
fn deadlines(
    files: &[&Outline],
    today: org_core::date::Day,
    config: &agenda::Config,
    wanted: &dyn Fn(Option<agenda::Source>) -> bool,
) -> Vec<String> {
    let mut config = config.clone();
    config.prefix_format.agenda = "%c: %s".to_string();
    agenda::day_items(files, today, today, &config)
        .into_iter()
        .filter(|item| item.kind == ItemKind::Deadline && !item.keyword_done && wanted(item.source))
        .map(|item| org_core::link::descriptions_only(&item.text).split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

/// How long the program that shows a notification may take.
const SEND_TIME: Duration = Duration::from_secs(30);

impl Delivery {
    /// Send `notification` each way it goes; what failed, said.
    pub fn send(&self, notification: &Notification) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Some(command) = &self.command {
            let arguments: Vec<String> = command.iter().map(|argument| fill(argument, notification)).collect();
            let mut program = Command::new(&arguments[0]);
            program.args(&arguments[1..]);
            if let Err(error) = run(&mut program) {
                errors.push(format!("{}: {error}", arguments[0]));
            }
        }
        if self.desktop {
            let (name, mut program) = desktop(notification);
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
/// text and `%%` a `%`.
fn fill(argument: &str, notification: &Notification) -> String {
    let mut filled = String::new();
    let mut chars = argument.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('%', Some('t')) => filled.push_str(&notification.title),
            ('%', Some('b')) => filled.push_str(&notification.body),
            ('%', Some('%')) => filled.push('%'),
            _ => {
                filled.push(c);
                continue;
            }
        }
        chars.next();
    }
    filled
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

/// The program that shows `notification` on the desktop, and its name:
/// `notify-send` (Linux and other Unix systems), `osascript` (macOS) or
/// PowerShell (Windows). Title and text are arguments of their own, or
/// the environment, never part of a script.
fn desktop(notification: &Notification) -> (&'static str, Command) {
    if cfg!(target_os = "macos") {
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
    } else if cfg!(windows) {
        let mut program = Command::new("powershell.exe");
        program.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", WINDOWS_TOAST]);
        program.env("ORG_LSP_TITLE", &notification.title).env("ORG_LSP_BODY", &notification.body);
        ("powershell.exe", program)
    } else {
        let mut program = Command::new("notify-send");
        program.args(["--app-name=org-lsp", "--", &notification.title, &notification.body]);
        ("notify-send", program)
    }
}

/// A toast with the title and text of `ORG_LSP_TITLE` and `ORG_LSP_BODY`:
/// BurntToast's when it is installed, else Windows's own, in the name of
/// PowerShell.
const WINDOWS_TOAST: &str = "$title = $env:ORG_LSP_TITLE; $body = $env:ORG_LSP_BODY; \
if (Get-Module -ListAvailable -Name BurntToast) { \
Import-Module BurntToast; New-BurntToastNotification -Text $title, $body \
} else { \
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
$xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
$texts = $xml.GetElementsByTagName('text'); \
[void]$texts.Item(0).AppendChild($xml.CreateTextNode($title)); \
[void]$texts.Item(1).AppendChild($xml.CreateTextNode($body)); \
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
            let found: Vec<serde_json::Value> = appointments(&[&outline], today, &options.agenda_config(), &|_| true)
                .into_iter()
                .map(|appointment| serde_json::json!([appointment.minute, appointment.message, appointment.warntime]))
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
        let notification = Notification { title: "T".to_string(), body: "B 100%".to_string() };
        assert_eq!(fill("%t: %b (%%t) %x", &notification), "T: B 100% (%t) %x");
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
}
