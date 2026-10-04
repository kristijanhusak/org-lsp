//! Command-line access to the agenda, without an editor:
//!
//! ```text
//! org-lsp agenda [--view agenda|todo] [--span day|week|N] [--start DATE]
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

pub const USAGE: &str = "\
Usage: org-lsp agenda [OPTIONS] FILE...

Print the agenda built from the given Org files.

Options:
  --view agenda|todo   the day/week agenda (default) or the TODO list
  --span SPAN          day, week, fortnight, month, year or a number of days
  --start YYYY-MM-DD   a day the agenda should include (default: today)
  --today YYYY-MM-DD   pretend this is today
  --now HH:MM          pretend this is the current time
  --options JSON       agenda settings, as in the server's `agenda` option";

fn parse_date(text: &str) -> Result<Day, String> {
    let parts: Vec<i64> = text.split('-').filter_map(|part| part.parse().ok()).collect();
    match parts[..] {
        [year, month, day] if Date::new(year, month, day).is_valid() => Ok(Date::new(year, month, day).to_day()),
        _ => Err(format!("`{text}` is not a date; expected YYYY-MM-DD")),
    }
}

fn parse_time(text: &str) -> Result<i64, String> {
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
    let outlines: Vec<&Outline> = outlines.iter().collect();
    let rendered = match view.as_str() {
        "agenda" => agenda::view(&outlines, now, start, config.span, &config),
        "todo" => agenda::todo_view(&outlines, &config),
        other => return Err(format!("unknown view `{other}`; expected agenda or todo")),
    };
    Ok(rendered.text())
}
