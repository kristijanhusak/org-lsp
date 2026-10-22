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
        let root = std::env::temp_dir().join(format!("org-lsp-notify-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("tasks.org"), TASKS).unwrap();
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
    assert_eq!(read(&desktop), format!("--app-name=org-lsp\n--\nAppointment now\n{STANDUP}\n"));
}

#[test]
fn shows_a_desktop_notification_with_notify_send() {
    let place = Place::new("desktop", "");
    let out = place.stand_in("notify-send");
    let output = place.run(&["--now", "2026-10-22 09:48"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read(&out), format!("--app-name=org-lsp\n--\nAppointment in 12 minutes\n{STANDUP}\n"));
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
