//! Command-line access to the agenda, without an editor:
//!
//! ```text
//! org-lsp agenda [--view agenda|todo|tags|tags-todo|custom] [--match M]
//!                [--key KEYS] [--span day|week|N] [--start DATE]
//!                [--log] [--clockreport] [--inactive] [--archives trees|files]
//!                [--entry-text N] [--filters JSON]
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
  --key KEYS           a command of the `org_agenda_custom_commands` agenda option
  --log                log mode: what was closed and clocked each day
  --clockreport        the clock report under the agenda
  --inactive           inactive timestamps too, as `[` in the agenda shows them
  --entry-text N       entry text mode: at most N lines of each entry's
                       text under its line
  --archives WHAT      trees: entries in archived trees too; files: those
                       and the files the given ones archive to
  --span SPAN          day, week, fortnight, month, year or a number of days
  --start YYYY-MM-DD   a day the agenda should include (default: today)
  --today YYYY-MM-DD   pretend this is today
  --now HH:MM          pretend this is the current time
  --options JSON       agenda and inheritance options, named as the server's
  --filters JSON       the agenda's filters, as a list of steps:
                       {\"type\": \"tag\", \"filter\": [\"+work\"]} (category,
                       regexp, effort too), {\"string\": \"+work<0:10\"} (`/`),
                       {\"top\": \"Projects\"} (`^`), {\"remove\": true}";

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
    let mut inactive = false;
    let mut archives = None;
    let mut filters: Option<serde_json::Value> = None;
    let mut entry_text: Option<usize> = None;
    let mut span = None;
    let mut start = None;
    let mut now = now();
    let mut options = AgendaOptions::default();
    let mut inheritance = org_core::outline::Inheritance::default();
    let mut tag_options: Option<crate::config::Options> = None;
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
            "--inactive" => inactive = true,
            "--archives" => archives = Some(value("--archives")?.clone()),
            "--entry-text" => {
                let count = value("--entry-text")?;
                entry_text = Some(count.parse().map_err(|_| format!("--entry-text: `{count}` is not a number"))?);
            }
            "--filters" => {
                let json = value("--filters")?;
                filters = Some(serde_json::from_str(json).map_err(|error| format!("--filters: {error}"))?);
            }
            "--span" => span = Some(value("--span")?.clone()),
            "--start" => start = Some(parse_date(value("--start")?)?),
            "--today" => now.today = parse_date(value("--today")?)?,
            "--now" => now.minute = parse_time(value("--now")?)?,
            "--options" => {
                let json = value("--options")?;
                options = serde_json::from_str(json).map_err(|error| format!("--options: {error}"))?;
                inheritance = serde_json::from_str::<crate::config::InheritanceOptions>(json)
                    .map_err(|error| format!("--options: {error}"))?
                    .inheritance();
                tag_options = serde_json::from_str::<crate::config::Options>(json).ok();
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
    if let Some(tag_options) = &tag_options {
        let tags = tag_options.tag_settings();
        config.tag_alist = tags.alist;
        config.tag_persistent_alist = tags.persistent;
        config.group_tags = tag_options.group_tags.unwrap_or(true);
        config.extend_today_until = tag_options.extend_today_until.unwrap_or(0);
        config.sorting.tags_sort = tags.sort;
        tag_options.habit_notes(&mut config);
    }
    config.show_log |= log;
    config.clockreport |= clockreport;
    config.include_inactive |= inactive;
    if let Some(span) = span {
        config.span = SpanOption::Name(span.clone()).to_span().ok_or_else(|| format!("`{span}` is not a span"))?;
    }

    // `org-agenda-archives-mode`: archived trees, and with `files` the
    // files the given ones archive to (`org-add-archive-files`).
    match archives.as_deref() {
        None => {}
        Some("trees") => config.archives = true,
        Some("files") => {
            config.archives = true;
            let mut all: Vec<String> = Vec::new();
            for file in &files {
                for name in std::iter::once(file.clone()).chain(archive_files_of(file)?) {
                    if !all.iter().any(|other| same_file(other, &name)) {
                        all.push(name);
                    }
                }
            }
            files = all;
        }
        Some(other) => return Err(format!("`{other}` is not an archives mode; expected trees or files")),
    }

    // `org-agenda-text-search-extra-files`, for a search: more files
    // after the agenda's, and the order the search goes through them.
    let agenda_count = files.len();
    let mut search = None;
    let extra = options.text_search_extra_files.clone().unwrap_or_default();
    if blocks_search(&view, &key, &options) && !extra.is_empty() {
        let with_archives = extra.first().is_some_and(|first| first == "agenda-archives");
        let index_of =
            |files: &mut Vec<String>, name: String| match files.iter().position(|other| same_file(other, &name)) {
                Some(index) => index,
                None => {
                    files.push(name);
                    files.len() - 1
                }
            };
        let mut archives: Vec<Vec<usize>> = vec![Vec::new(); agenda_count];
        if with_archives {
            for (index, found) in archives.iter_mut().enumerate() {
                let file = files[index].clone();
                for name in archive_files_of(&file)? {
                    found.push(index_of(&mut files, name));
                }
            }
        }
        let extra: Vec<usize> =
            extra.iter().skip(usize::from(with_archives)).map(|name| index_of(&mut files, name.clone())).collect();
        let paths: Vec<Option<std::path::PathBuf>> = files.iter().map(|file| std::path::absolute(file).ok()).collect();
        let archives_of = |index: usize| archives[index].clone();
        let archives_of: Option<&dyn Fn(usize) -> Vec<usize>> = with_archives.then_some(&archives_of as _);
        let order = views::search_order(&paths, agenda_count, archives_of, &extra);
        search = Some(views::SearchFiles { agenda: agenda_count, search: order });
    }

    let default_todo = TodoKeywords::default();
    let mut outlines = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
        let parse = org_syntax::parse(&text, &default_todo);
        outlines.push(Outline::with_inheritance(&parse, &text, &file_name(Path::new(file)), &inheritance));
    }
    let block = |kind| vec![Block { kind, header: None, config: config.clone() }];
    let mut blocks = match view.as_str() {
        "agenda" => block(BlockKind::Agenda { start, span: config.span }),
        "todo" => block(BlockKind::Todo { keywords: matching }),
        "tags" | "tags-todo" => block(BlockKind::Tags {
            matcher: agenda::Matcher::new(matching.as_deref().unwrap_or("")),
            todo_only: view == "tags-todo",
        }),
        "search" | "search-todo" => {
            let string = matching.unwrap_or_default();
            agenda::search::parse(&string, view == "search-todo", &config.search)?;
            block(BlockKind::Search { string, todo_only: view == "search-todo" })
        }
        "stuck" => block(BlockKind::Stuck),
        "custom" => {
            let key = key.ok_or("--view custom needs --key")?;
            let commands = options.custom_commands.as_deref().unwrap_or_default();
            let command = crate::custom::find(commands, &key).ok_or_else(|| format!("no custom command `{key}`"))?;
            let read = crate::config::Options::default().read_date_settings();
            crate::custom::blocks(command, &options, &config, now, &read, None)?
        }
        other => {
            return Err(format!(
                "unknown view `{other}`; expected agenda, todo, tags, tags-todo, search, search-todo, stuck or custom"
            ))
        }
    };
    if let Some(lines) = entry_text {
        for block in &mut blocks {
            block.config.entry_text = Some(lines);
        }
    }
    let named: Vec<(&Outline, String, String)> = outlines
        .iter()
        .zip(&files)
        .map(|(outline, file)| {
            let path = Path::new(file);
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            (outline, file.clone(), name)
        })
        .collect();
    let filters = match &filters {
        Some(steps) => {
            let represented = views::represented(&blocks, &named, search.as_ref(), now);
            Some(views::filter_steps(steps, &represented)?)
        }
        None => None,
    };
    Ok(views::org_text(&blocks, &named, search.as_ref(), now, filters.as_ref()))
}

/// Whether the view asked for has a search block.
fn blocks_search(view: &str, key: &Option<String>, options: &AgendaOptions) -> bool {
    match view {
        "search" | "search-todo" => true,
        "custom" => key.as_ref().is_some_and(|key| {
            let commands = options.custom_commands.as_deref().unwrap_or_default();
            crate::custom::find(commands, key).is_some_and(crate::custom::has_search)
        }),
        _ => false,
    }
}

/// Whether two names are of the same file.
fn same_file(one: &str, other: &str) -> bool {
    std::path::absolute(one).ok() == std::path::absolute(other).ok()
}

/// `org-all-archive-files`: the files `file` archives to that exist.
fn archive_files_of(file: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
    let path = std::path::absolute(file).map_err(|error| format!("{file}: {error}"))?;
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let context = org_core::archive::Context {
        path: &path,
        home: home.as_deref(),
        now: org_core::date::DateTime::new(Date::new(1970, 1, 1), 0, 0),
    };
    let found = org_core::archive::archive_files(&text, &TodoKeywords::default(), &context, "%s_archive::");
    Ok(found.into_iter().filter(|path| path.exists()).map(|path| path.display().to_string()).collect())
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
        let out = config(
            &["--options".to_string(), r#"{ "config_file": false, "org_log_done": "time" }"#.to_string()],
            &no_env,
        );
        assert_eq!(out.unwrap(), "Configuration file: none\nOptions:\n{\n  \"org_log_done\": \"time\"\n}\n");
        let home = |name: &str| (name == "XDG_CONFIG_HOME").then(|| "/nonexistent".to_string());
        let out = config(&[], &home).unwrap();
        let expected = std::path::Path::new("/nonexistent").join("org-lsp").join("config.toml");
        assert!(out.starts_with(&format!("Configuration file: {} (not found)\n", expected.display())), "{out}");
        assert!(config(&["--options".to_string(), "{".to_string()], &no_env).is_err());
    }
}
