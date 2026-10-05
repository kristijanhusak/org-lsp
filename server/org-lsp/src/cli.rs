//! Command-line access to the agenda, without an editor:
//!
//! ```text
//! org-lsp agenda [--view agenda|todo|tags|tags-todo|custom] [--match M]
//!                [--key KEYS] [--span day|week|N] [--start DATE]
//!                [--today DATE] [--now HH:MM] [--options JSON] FILE...
//! ```
//!
//! This is what Emacs's `org-batch-agenda` is to Org, and it is how the
//! test suite compares the two.

use std::path::Path;

use org_core::agenda::{self, Now};
use org_core::date::{Date, Day};
use org_core::outline::Outline;
use org_syntax::TodoKeywords;

use crate::config::{AgendaOptions, SpanOption};
use crate::views::{self, Block, BlockKind};

pub const USAGE: &str = "\
Usage: org-lsp agenda [OPTIONS] FILE...

Print the agenda built from the given Org files.

Options:
  --view VIEW          agenda (the default), todo (the TODO list), tags or
                       tags-todo (with --match), or custom (with --key)
  --match MATCH        for todo: keywords (NEXT|WAIT); for tags: a match
                       string (work-home, PRIORITY=\"A\", /NEXT)
  --key KEYS           a command of the `customCommands` agenda option
  --log                log mode: what was closed and clocked each day
  --clockreport        the clock report under the agenda
  --span SPAN          day, week, fortnight, month, year or a number of days
  --start YYYY-MM-DD   a day the agenda should include (default: today)
  --today YYYY-MM-DD   pretend this is today
  --now HH:MM          pretend this is the current time
  --options JSON       agenda settings, as in the server's `agenda` option";

pub fn parse_date(text: &str) -> Result<Day, String> {
    let parts: Vec<i64> = text.split('-').filter_map(|part| part.parse().ok()).collect();
    match parts[..] {
        [year, month, day] if Date::new(year, month, day).is_valid() => Ok(Date::new(year, month, day).to_day()),
        _ => Err(format!("`{text}` is not a date; expected YYYY-MM-DD")),
    }
}

pub fn parse_time(text: &str) -> Result<i64, String> {
    let parsed =
        text.split_once(':').and_then(|(hour, minute)| Some((hour.parse::<i64>().ok()?, minute.parse::<i64>().ok()?)));
    match parsed {
        Some((hour, minute)) if (0..24).contains(&hour) && (0..60).contains(&minute) => Ok(hour * 60 + minute),
        _ => Err(format!("`{text}` is not a time; expected HH:MM")),
    }
}

/// The current local date and time.
pub fn now() -> Now {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    Now {
        today: Date::new(i64::from(now.year()), i64::from(now.month()), i64::from(now.day())).to_day(),
        minute: i64::from(now.hour() * 60 + now.minute()),
    }
}

/// The category of last resort for a file: its name without directory and
/// extension.
pub fn file_name(path: &Path) -> String {
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Run `org-lsp agenda` with the arguments after the subcommand.
pub fn agenda(arguments: &[String]) -> Result<String, String> {
    let mut view = "agenda".to_string();
    let mut matching = None;
    let mut key = None;
    let mut log = false;
    let mut clockreport = false;
    let mut span = None;
    let mut start = None;
    let mut now = now();
    let mut options = AgendaOptions::default();
    let mut files = Vec::new();

    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| arguments.next().ok_or_else(|| format!("{name} needs a value"));
        match argument.as_str() {
            "--view" => view = value("--view")?.clone(),
            "--match" => matching = Some(value("--match")?.clone()),
            "--key" => key = Some(value("--key")?.clone()),
            "--log" => log = true,
            "--clockreport" => clockreport = true,
            "--span" => span = Some(value("--span")?.clone()),
            "--start" => start = Some(parse_date(value("--start")?)?),
            "--today" => now.today = parse_date(value("--today")?)?,
            "--now" => now.minute = parse_time(value("--now")?)?,
            "--options" => {
                options = serde_json::from_str(value("--options")?).map_err(|error| format!("--options: {error}"))?
            }
            "--help" | "-h" => return Ok(format!("{USAGE}\n")),
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            file => files.push(file.to_string()),
        }
    }
    if files.is_empty() {
        return Err("no files given".to_string());
    }

    let mut config = options.to_config();
    config.show_log |= log;
    config.clockreport |= clockreport;
    if let Some(span) = span {
        config.span = SpanOption::Name(span.clone()).to_span().ok_or_else(|| format!("`{span}` is not a span"))?;
    }

    let default_todo = TodoKeywords::default();
    let mut outlines = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
        let parse = org_syntax::parse(&text, &default_todo);
        outlines.push(Outline::new(&parse, &text, &file_name(Path::new(file))));
    }
    let block = |kind| vec![Block { kind, header: None, config: config.clone() }];
    let blocks = match view.as_str() {
        "agenda" => block(BlockKind::Agenda { start, span: config.span }),
        "todo" => block(BlockKind::Todo { keywords: matching }),
        "tags" | "tags-todo" => block(BlockKind::Tags {
            matcher: agenda::Matcher::new(matching.as_deref().unwrap_or("")),
            todo_only: view == "tags-todo",
        }),
        "custom" => {
            let key = key.ok_or("--view custom needs --key")?;
            let commands = options.custom_commands.as_deref().unwrap_or_default();
            let command = crate::custom::find(commands, &key).ok_or_else(|| format!("no custom command `{key}`"))?;
            let read = crate::config::Options::default().read_date_settings();
            crate::custom::blocks(command, &options, &config, now, &read, None)?
        }
        other => return Err(format!("unknown view `{other}`; expected agenda, todo, tags, tags-todo or custom")),
    };
    let named: Vec<(&Outline, String, String)> = outlines
        .iter()
        .zip(&files)
        .map(|(outline, file)| {
            let path = Path::new(file);
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            (outline, file.clone(), name)
        })
        .collect();
    Ok(views::org_text(&blocks, &named, now))
}

pub const CONFIG_USAGE: &str = "\
Usage: org-lsp config [--options JSON]

Show the shared configuration file and the options the server makes of it,
with the editor's options given as JSON on top (`initializationOptions`),
as the server reads them when it starts.";

/// `org-lsp config`: which configuration file is read, what is wrong with
/// it, and the options it comes to. `var` reads the environment.
pub fn config(arguments: &[String], var: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut editor = None;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--options" => {
                let value = arguments.next().ok_or("--options needs a value")?;
                editor = Some(serde_json::from_str(value).map_err(|error| format!("--options: {error}"))?);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let loaded = crate::config::load(editor, var);
    let mut out = match (&loaded.file, loaded.read) {
        (Some(file), true) => format!("Configuration file: {}\n", file.display()),
        (Some(file), false) if !file.exists() => format!("Configuration file: {} (not found)\n", file.display()),
        (Some(file), false) => format!("Configuration file: {} (not read)\n", file.display()),
        (None, _) => "Configuration file: none\n".to_string(),
    };
    for problem in &loaded.problems {
        out.push_str(&format!("Problem: {problem}\n"));
    }
    out.push_str(&format!("Options:\n{:#}\n", loaded.merged));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_shows_the_file_and_the_options() {
        let no_env = |_: &str| None;
        let out =
            config(&["--options".to_string(), r#"{ "configFile": false, "logDone": "time" }"#.to_string()], &no_env);
        assert_eq!(out.unwrap(), "Configuration file: none\nOptions:\n{\n  \"logDone\": \"time\"\n}\n");
        let home = |name: &str| (name == "XDG_CONFIG_HOME").then(|| "/nonexistent".to_string());
        let out = config(&[], &home).unwrap();
        let expected = std::path::Path::new("/nonexistent").join("org-lsp").join("config.toml");
        assert!(out.starts_with(&format!("Configuration file: {} (not found)\n", expected.display())), "{out}");
        assert!(config(&["--options".to_string(), "{".to_string()], &no_env).is_err());
    }
}
