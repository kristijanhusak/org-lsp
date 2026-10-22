//! `org-lsp notify` as cron runs it: the shared configuration file of
//! `ORG_LSP_CONFIG`, its agenda files, the time of `--now`, and what it
//! sends: printed with `--dry-run`, else through `notification_command`
//! or `notify-send` (here a stand-in on `PATH` that writes down its
//! arguments).

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_org-lsp");

/// What `org-agenda-to-appt` makes of two entries of [`TASKS`], their
/// tags aligned as Org aligns them (tests/appt/tasks.json).
const STANDUP: &str = "10:00 TODO Standup                            :meeting:";
const BANK: &str = "15:00 Phone the bank                             :call:";

/// The agenda file: a recorded case of `org-agenda-to-appt`.
const TASKS: &str = include_str!("../../../tests/appt/tasks.org");

/// A test's own directory: the configuration file and the Org file.
struct Place {
    root: PathBuf,
}

impl Place {
    fn new(name: &str, config: &str) -> Place {
        Place::with(name, config, TASKS)
    }

    /// A place whose agenda file is `tasks`.
    fn with(name: &str, config: &str, tasks: &str) -> Place {
        let root = std::env::temp_dir().join(format!("org-lsp-notify-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("tasks.org"), tasks).unwrap();
        let config = format!("org_agenda_files = [\"$DIR/tasks.org\"]\n{config}");
        std::fs::write(root.join("config.toml"), config.replace("$DIR", &root.display().to_string())).unwrap();
        Place { root }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let path = format!("{}:{}", self.root.join("bin").display(), std::env::var("PATH").unwrap_or_default());
        Command::new(BINARY)
            .arg("notify")
            .args(arguments)
            .env("ORG_LSP_CONFIG", self.root.join("config.toml"))
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("xdg"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("PATH", path)
            .current_dir(&self.root)
            .output()
            .unwrap()
    }

    /// What `--dry-run` prints at `time` on 22 October 2026.
    fn dry(&self, time: &str) -> String {
        let output = self.run(&["--dry-run", "--now", &format!("2026-10-22 {time}")]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    /// A program in the `PATH` of the runs, named `name`, that writes its
    /// arguments, one a line, to `name.out`.
    fn stand_in(&self, name: &str) -> PathBuf {
        let out = self.root.join(format!("{name}.out"));
        let script = format!(
            "#!/bin/sh\nfor argument in \"$@\"; do printf '%s\\n' \"$argument\"; done >> '{}'\n",
            out.display()
        );
        let program = self.root.join("bin").join(name);
        std::fs::write(&program, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        out
    }
}

impl Place {
    /// Where the logo is written: the data directory of the runs.
    fn icon(&self) -> PathBuf {
        self.root.join("data").join("org-lsp").join("icon.png")
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn reminds_as_appt_does_by_default() {
    let place = Place::new("default", "");
    // Twelve minutes before, then each three minutes, until it is due;
    // the done task at the same time is left out.
    assert_eq!(place.dry("09:48"), format!("Appointment in 12 minutes\n{STANDUP}\n\n"));
    assert_eq!(place.dry("09:57"), format!("Appointment in 3 minutes\n{STANDUP}\n\n"));
    assert_eq!(place.dry("10:00"), format!("Appointment now\n{STANDUP}\n\n"));
    // Not at the minutes in between, nor before or after.
    for time in ["09:47", "09:50", "09:59", "10:01"] {
        assert_eq!(place.dry(time), "", "{time}");
    }
    // A repeater on its day, the start of a time range, a timestamp; the
    // cancelled entry at the same time is left out.
    assert_eq!(place.dry("16:00"), "Appointment now\n16:00 Weekly review\n\n");
    assert_eq!(place.dry("12:21"), "Appointment in 9 minutes\n12:30 Lunch\n\n");
    assert_eq!(place.dry("14:03"), "Appointment in 12 minutes\n14:15 Dentist\n\n");
}

#[test]
fn reminds_at_each_minute_of_a_list() {
    let place = Place::new("list", "appt_message_warning_time = [0, 5, 45]");
    assert_eq!(place.dry("09:15"), format!("Appointment in 45 minutes\n{STANDUP}\n\n"));
    assert_eq!(place.dry("09:55"), format!("Appointment in 5 minutes\n{STANDUP}\n\n"));
    assert_eq!(place.dry("09:57"), "");
    // Two at once, at different times.
    assert_eq!(place.dry("14:15"), format!("Appointments in 0,45 minutes\n0m: 14:15 Dentist\n45m: {BANK}\n\n"));
}

#[test]
fn reminds_of_the_entries_that_match() {
    let place = Place::new("match", "notification_match = \"+call|+meeting\"\nappt_message_warning_time = 0");
    assert_eq!(place.dry("10:00"), format!("Appointment now\n{STANDUP}\n\n"));
    assert_eq!(place.dry("14:15"), "");
    assert_eq!(place.dry("15:00"), format!("Appointment now\n{BANK}\n\n"));
}

#[test]
fn lists_the_deadlines_at_their_time() {
    let place = Place::new("summary", "notification_deadline_summary_time = \"08:30\"");
    assert_eq!(place.dry("08:30"), "Deadlines\ntasks: 2 d. ago: TODO Taxes\ntasks: In 3 d.: TODO Report\n\n");
    assert_eq!(place.dry("08:31"), "");
    // `--options` over the file's.
    let output = place.run(&[
        "--dry-run",
        "--now",
        "2026-10-22 09:00",
        "--options",
        r#"{ "notification_deadline_summary_time": "9:00", "org_deadline_warning_days": 1 }"#,
    ]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Deadlines\ntasks: 2 d. ago: TODO Taxes\n\n");
}

#[test]
fn runs_the_notification_command_with_the_title_and_text() {
    let place = Place::new("command", "notification_command = [\"fake-ntfy\", \"--title=%t\", \"%b\", \"100%%\"]");
    let out = place.stand_in("fake-ntfy");
    let desktop = place.stand_in("notify-send");
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stdout.is_empty());
    assert_eq!(read(&out), format!("--title=Appointment now\n{STANDUP}\n100%\n"));
    // Instead of the desktop notification.
    assert_eq!(read(&desktop), "");
    // With it, when asked for.
    let output = place.run(&["--now", "2026-10-22 10:00", "--options", r#"{ "notification_desktop": true }"#]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let icon = place.icon();
    assert_eq!(
        read(&desktop),
        format!("--app-name=org-lsp\n--icon={}\n--\nAppointment now\n{STANDUP}\n", icon.display())
    );
}

#[test]
fn shows_a_desktop_notification_with_notify_send() {
    let place = Place::new("desktop", "");
    let out = place.stand_in("notify-send");
    let output = place.run(&["--now", "2026-10-22 09:48"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let icon = place.icon();
    assert_eq!(
        read(&out),
        format!("--app-name=org-lsp\n--icon={}\n--\nAppointment in 12 minutes\n{STANDUP}\n", icon.display())
    );
    // Nothing due: nothing run, and no failure.
    std::fs::remove_file(&out).unwrap();
    let output = place.run(&["--now", "2026-10-22 09:49"]);
    assert!(output.status.success());
    assert!(!out.exists());
}

#[test]
fn says_what_failed() {
    let place = Place::new("failed", "notification_command = [\"/nonexistent/notifier\", \"%b\"]");
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("/nonexistent/notifier"));
    let output = place.run(&["--now", "yesterday"]);
    assert_eq!(output.status.code(), Some(2));
    // No agenda files: not the Org files of the directory it runs in.
    let output = place.run(&["--dry-run", "--options", r#"{ "org_agenda_files": [] }"#]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no org_agenda_files"));
}

/// Entries with all a placeholder can say: keyword, priority, a link in
/// the title, own and inherited tags, a category, scheduled and deadline
/// timestamps, an effort.
const PROJECT: &str = "#+TODO: TODO NEXT | DONE
#+FILETAGS: :home:
* Projects :work:
:PROPERTIES:
:CATEGORY: proj
:END:
** NEXT [#A] Review [[https://example.org][the plan]] :meeting:
SCHEDULED: <2026-10-22 Thu 10:00> DEADLINE: <2026-10-24 Sat>
:PROPERTIES:
:EFFORT: 0:30
:END:
** TODO Call Ana :call:
<2026-10-22 Thu 9:55>
";

/// What `org-agenda-to-appt` makes of the entries of [`PROJECT`].
const ANA: &str = "09:55 TODO Call Ana                   :home:work::call:";
const REVIEW: &str = "10:00 NEXT [#A] Review the plan :home:work::meeting:";

/// A [`Place`] whose agenda file is [`PROJECT`].
fn project(name: &str, config: &str) -> Place {
    let place = Place::new(name, config);
    std::fs::write(place.root.join("tasks.org"), PROJECT).unwrap();
    place
}

#[test]
fn fills_in_the_placeholders_of_the_entry() {
    let place = project(
        "placeholders",
        "appt_message_warning_time = [5, 10]\n\
         notification_title_format = \"%k %h\"\n\
         notification_body_format = \"%w|%m|%h|%k|%p|%T|%c|%s|%d|%e|%F|%l|%x|%%|%a\"",
    );
    // Due together: the first one's title, a line of text each.
    assert_eq!(
        place.dry("09:50"),
        format!(
            "TODO Call Ana\n\
             09:55|5|Call Ana|TODO||:home:work:call:|proj||||tasks.org|12|%x|%|{ANA}\n\
             10:00|10|Review the plan|NEXT|A|:home:work:meeting:|proj|<2026-10-22 Thu 10:00>|<2026-10-24 Sat>|0:30|\
             tasks.org|7|%x|%|{REVIEW}\n\n"
        )
    );
    let output = place.run(&[
        "--dry-run",
        "--now",
        "2026-10-22 09:55",
        "--options",
        r#"{ "notification_title_format": "%f", "notification_body_format": "%T" }"#,
    ]);
    let path = place.root.join("tasks.org");
    assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{}\n:home:work:meeting:\n\n", path.display()));
    // Inherited tags left out where the agenda leaves them out.
    let output = place.run(&[
        "--dry-run",
        "--now",
        "2026-10-22 09:50",
        "--options",
        r#"{ "notification_body_format": "%T", "org_agenda_show_inherited_tags": false }"#,
    ]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "TODO Call Ana\n:call:\n:meeting:\n\n");
}

#[test]
fn keeps_the_minutes_before_the_lines_only_by_default() {
    let place = project("format-minutes", "appt_message_warning_time = [5, 10]\nnotification_body_format = \"%a\"");
    assert_eq!(place.dry("09:50"), format!("Appointments in 5,10 minutes\n{ANA}\n{REVIEW}\n\n"));
    let place = project("default-minutes", "appt_message_warning_time = [5, 10]");
    assert_eq!(place.dry("09:50"), format!("Appointments in 5,10 minutes\n5m: {ANA}\n10m: {REVIEW}\n\n"));
}

#[test]
fn runs_the_command_for_each_appointment_when_it_names_an_entry() {
    let both = "appt_message_warning_time = [5, 10]\n";
    let place = project("each", &format!("{both}notification_command = [\"fake-ntfy\", \"%t\", \"%T\", \"%b\"]"));
    let out = place.stand_in("fake-ntfy");
    let output = place.run(&["--now", "2026-10-22 09:50"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        read(&out),
        format!(
            "Appointment in 5 minutes\n:home:work:call:\n{ANA}\n\
             Appointment in 10 minutes\n:home:work:meeting:\n{REVIEW}\n"
        )
    );
    // Without one: once for both.
    let place = project("group", &format!("{both}notification_command = [\"fake-ntfy\", \"%t\", \"%%T\"]"));
    let out = place.stand_in("fake-ntfy");
    let output = place.run(&["--now", "2026-10-22 09:50"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read(&out), "Appointments in 5,10 minutes\n%T\n");
    // The deadline summary is of no one entry: their placeholders are empty.
    let place = project(
        "deadline-command",
        "notification_deadline_summary_time = \"08:30\"\nnotification_command = [\"fake-ntfy\", \"%t\", \"<%h>\"]",
    );
    let out = place.stand_in("fake-ntfy");
    let output = place.run(&["--now", "2026-10-22 08:30"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read(&out), "Deadlines\n<>\n");
}

#[test]
fn writes_the_deadlines_as_the_line_format_says() {
    let place = Place::new(
        "deadline-format",
        "notification_deadline_summary_time = \"08:30\"\nnotification_deadline_line_format = \"[%D] %k %h %d (%c)\"",
    );
    assert_eq!(
        place.dry("08:30"),
        "Deadlines\n[2 d. ago:] TODO Taxes <2026-10-20 Tue> (tasks)\n[In 3 d.:] TODO Report <2026-10-25 Sun> (tasks)\n\n"
    );
    // The default, written out.
    let place = project(
        "deadline-format-project",
        "notification_deadline_summary_time = \"08:30\"\nnotification_deadline_line_format = \"%c: %D %a\"",
    );
    let default = project("deadline-default-project", "notification_deadline_summary_time = \"08:30\"");
    assert_eq!(place.dry("08:30"), default.dry("08:30"));
    assert_eq!(place.dry("08:30"), "Deadlines\nproj: In 2 d.: NEXT [#A] Review the plan :home:work::meeting:\n\n");
}

/// The logo the server holds, as the VS Code extension shows it.
const LOGO: &[u8] = include_bytes!("../../../editors/vscode/icon.png");

#[test]
fn shows_the_logo_written_to_the_data_directory() {
    let place = Place::new("icon", "");
    let out = place.stand_in("notify-send");
    let icon = place.icon();
    // Not by a dry run.
    place.dry("10:00");
    assert!(!icon.exists());
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read(&icon).unwrap(), LOGO);
    assert!(read(&out).starts_with(&format!("--app-name=org-lsp\n--icon={}\n--\n", icon.display())));
    // Another image in its place is written over.
    std::fs::write(&icon, b"not the logo").unwrap();
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read(&icon).unwrap(), LOGO);
    assert_eq!(std::fs::read_dir(icon.parent().unwrap()).unwrap().count(), 1);
    // Where it cannot be written: no image, and no failure.
    std::fs::remove_file(&out).unwrap();
    std::fs::write(place.root.join("blocked"), "").unwrap();
    let blocked = place.root.join("blocked").display().to_string();
    let output = Command::new(BINARY)
        .args(["notify", "--now", "2026-10-22 10:00"])
        .env("ORG_LSP_CONFIG", place.root.join("config.toml"))
        .env("HOME", &place.root)
        .env("XDG_CONFIG_HOME", place.root.join("xdg"))
        .env("XDG_DATA_HOME", &blocked)
        .env("PATH", format!("{}:{}", place.root.join("bin").display(), std::env::var("PATH").unwrap_or_default()))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read(&out), format!("--app-name=org-lsp\n--\nAppointment now\n{STANDUP}\n"));
}

#[test]
fn shows_the_icon_of_the_configuration() {
    let place = Place::new("no-icon", "notification_icon = \"\"");
    let out = place.stand_in("notify-send");
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read(&out), format!("--app-name=org-lsp\n--\nAppointment now\n{STANDUP}\n"));
    assert!(!place.icon().exists());
    // A path of one's own, from the home directory.
    let place = Place::new("own-icon", "notification_icon = \"~/pictures/bell.png\"");
    let out = place.stand_in("notify-send");
    let output = place.run(&["--now", "2026-10-22 10:00"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let bell = place.root.join("pictures").join("bell.png");
    assert_eq!(read(&out), format!("--app-name=org-lsp\n--icon={}\n--\nAppointment now\n{STANDUP}\n", bell.display()));
    assert!(!place.icon().exists());
}

/// Timestamps with warning cookies, on and around 22 October 2026.
const WARNINGS: &str = "\
* TODO Call Anna
DEADLINE: <2026-10-22 Thu 21:00 -1h>
* Concert
<2026-10-22 Thu 19:30 --2h>
* TODO Prepare slides
SCHEDULED: <2026-10-22 Thu 18:00 -1h>
* TODO Pay rent
DEADLINE: <2026-10-22 Thu 13:00 -1d>
* Early flight
<2026-10-23 Fri 01:00 -3h>
* TODO Weekly report
DEADLINE: <2026-10-22 Thu 17:00 +1w -1h>
* Gym
<2026-10-15 Thu 18:45 -1h +1w>
* Doctor
:PROPERTIES:
:APPT_WARNTIME: 30
:END:
<2026-10-22 Thu 11:00 -1h>
* Coffee
<2026-10-22 Thu 10:00>
";

#[test]
fn leaves_warning_cookies_alone_by_default() {
    let place = Place::with("cookies-off", "", WARNINGS);
    for time in ["20:00", "17:30", "22:00", "16:00"] {
        assert_eq!(place.dry(time), "", "{time}");
    }
    assert_eq!(place.dry("20:48"), "Appointment in 12 minutes\n21:00 TODO Call Anna\n\n");
}

#[test]
fn reminds_as_many_hours_before_as_the_warning_cookie_says() {
    let place = Place::with("cookies", "notification_use_timestamp_warning = true", WARNINGS);
    // A deadline, at the minute only; `appt.el`'s reminders as before.
    assert_eq!(place.dry("20:00"), "Appointment in 60 minutes\n21:00 TODO Call Anna\n\n");
    for time in ["19:59", "20:01", "20:03"] {
        assert_eq!(place.dry(time), "", "{time}");
    }
    assert_eq!(place.dry("20:48"), "Appointment in 12 minutes\n21:00 TODO Call Anna\n\n");
    // A timestamp, with `--`.
    assert_eq!(place.dry("17:30"), "Appointment in 120 minutes\n19:30 Concert\n\n");
    // A scheduled date's cookie is a delay, and one in days is the agenda's.
    assert_eq!(place.dry("17:00"), "Appointment now\n17:00 TODO Weekly report\n\n");
    assert_eq!(place.dry("12:00"), "");
    assert_eq!(place.dry("12:48"), "Appointment in 12 minutes\n13:00 TODO Pay rent\n\n");
    // Tomorrow's appointment at 01:00, three hours before.
    assert_eq!(place.dry("22:00"), "Appointment in 180 minutes\n01:00 Early flight\n\n");
    let output = place.run(&["--dry-run", "--now", "2026-10-23 00:48"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Appointment in 12 minutes\n01:00 Early flight\n\n");
}

#[test]
fn reminds_of_each_occurrence_of_a_repeater_at_its_warning() {
    let place = Place::with("cookies-repeat", "notification_use_timestamp_warning = true", WARNINGS);
    assert_eq!(place.dry("16:00"), "Appointment in 60 minutes\n17:00 TODO Weekly report\n\n");
    // A timestamp repeats in the agenda with its repeater last, as Org's
    // regexp wants it.
    assert_eq!(place.dry("17:45"), "Appointment in 60 minutes\n18:45 Gym\n\n");
    let at = |time: &str| String::from_utf8(place.run(&["--dry-run", "--now", time]).stdout).unwrap();
    assert_eq!(at("2026-10-29 17:45"), "Appointment in 60 minutes\n18:45 Gym\n\n");
    assert_eq!(at("2026-10-23 17:45"), "");
}

#[test]
fn reminds_at_the_warning_besides_appt_warntime_and_with_the_others_due() {
    let place = Place::with("cookies-warntime", "notification_use_timestamp_warning = true", WARNINGS);
    // `APPT_WARNTIME` still gives its reminders.
    assert_eq!(place.dry("10:30"), "Appointment in 30 minutes\n11:00 Doctor\n\n");
    // The warning comes with the appointment due then, in one notification.
    assert_eq!(place.dry("10:00"), "Appointments in 0,60 minutes\n0m: 10:00 Coffee\n60m: 11:00 Doctor\n\n");
    let place = Place::with(
        "cookies-format",
        "notification_use_timestamp_warning = true\nnotification_title_format = \"%h in %m\"",
        WARNINGS,
    );
    assert_eq!(place.dry("20:00"), "Call Anna in 60\n21:00 TODO Call Anna\n\n");
}
