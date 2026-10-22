//! The prompts of `org-lsp mcp`: messages an assistant offers its user
//! (as slash commands in Claude Code), each with the data it is about,
//! built as the tools build it from the files of now, then what to do
//! with it. Those that may lead to changes say to propose them, show them
//! with `dry_run`, and apply them only once the user agrees.

use std::path::{Path, PathBuf};

use org_core::capture::Target;
use org_core::date::Day;
use serde_json::{json, Value};

use super::{
    agenda_paths, build_view, day_text, editable_file, editable_files, entry_ends, file_label, now, outline_of,
    reading, refile_targets, set_variables, target_path, Arguments, Call, EDITABLE,
};
use crate::config::Options;
use crate::server::TargetSpec;

/// A prompt, as `prompts/list` gives it.
struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// Each argument's name and description, and whether it is required.
    arguments: &'static [(&'static str, &'static str, bool)],
}

const PROMPTS: [Prompt; 5] = [
    Prompt {
        name: "plan_my_day",
        title: "Plan my day",
        description: "Plan a day in time blocks from its agenda, the deadlines coming up and the tasks in \
                      progress, flagging conflicts, and offer to schedule tasks.",
        arguments: &[("date", "The day, YYYY-MM-DD; the default is today", false)],
    },
    Prompt {
        name: "weekly_review",
        title: "Weekly review",
        description: "Review a week: what was done and clocked, what slipped, the stuck projects, and the \
                      priorities of the next week.",
        arguments: &[(
            "week",
            "A day of the week, YYYY-MM-DD, where the agenda's week starts; the default is this week",
            false,
        )],
    },
    Prompt {
        name: "process_inbox",
        title: "Process the inbox",
        description: "Go through the entries of the inbox and propose for each a refile target, tags, a \
                      TODO state and a date, then apply them once confirmed.",
        arguments: &[
            (
                "file",
                "The inbox's file; the default is the target of the first capture template, else the file \
                 of an Inbox headline",
                false,
            ),
            ("headline", "The inbox's headline, or its outline path (A/B); none for the file's top level", false),
        ],
    },
    Prompt {
        name: "daily_standup",
        title: "Daily standup",
        description: "A short standup: what was done and clocked the day before, what is planned for the day, \
                      and what is waiting.",
        arguments: &[("date", "The day of the standup, YYYY-MM-DD; the default is today", false)],
    },
    Prompt {
        name: "find_stale_tasks",
        title: "Find stale tasks",
        description: "The open TODO entries without a date or a clock in the last days, with a suggestion for \
                      each: archive, reschedule, close or delete, applied once confirmed.",
        arguments: &[("days", "How many days without a date makes a task stale; the default is 30", false)],
    },
];

/// What a prompt that may lead to changes tells the assistant to do.
const CONFIRM: &str = "Change nothing before I agree. First propose the changes, each with the org-lsp tool \
                       that makes it and the entry's [file:line]; run those tools with `dry_run: true` to show \
                       me their diffs; then ask me to confirm, and only then apply them with the same tools \
                       without `dry_run`. Always pass `headline` (text of the entry's headline) as a guard. A \
                       change moves the lines after it: take each entry's line from the answer of the tool \
                       that changed it, or from a view again. The edit tools are there only when \
                       `mcp_edits = true` is set in org-lsp's configuration file (`org-lsp config` shows \
                       which file); without them, tell me the changes to make myself.";

/// The prompts, for `prompts/list`.
pub(super) fn list() -> Value {
    PROMPTS
        .iter()
        .map(|prompt| {
            let arguments: Vec<Value> = prompt
                .arguments
                .iter()
                .map(|(name, description, required)| {
                    json!({ "name": name, "description": description, "required": required })
                })
                .collect();
            json!({
                "name": prompt.name,
                "title": prompt.title,
                "description": prompt.description,
                "arguments": arguments,
            })
        })
        .collect()
}

/// The prompt `name` with `arguments`, for `prompts/get`; else what is
/// wrong with them.
pub(super) fn get(
    name: &str,
    arguments: &Value,
    folder: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Value, String> {
    let prompt = PROMPTS.iter().find(|prompt| prompt.name == name).ok_or_else(|| format!("Unknown prompt: {name}"))?;
    let given = |key: &str| -> Option<String> {
        let value = match arguments.get(key)? {
            Value::String(text) => text.trim().to_string(),
            Value::Number(number) => number.to_string(),
            _ => return None,
        };
        (!value.is_empty()).then_some(value)
    };
    for (argument, _, required) in prompt.arguments {
        if *required && given(argument).is_none() {
            return Err(format!("Missing required argument: {argument}"));
        }
    }
    let day = |key: &str| -> Result<Option<Day>, String> {
        given(key)
            .map(|text| crate::cli::parse_date(&text).map_err(|error| format!("Invalid argument {key}: {error}")))
            .transpose()
    };
    let options = crate::config::load(None, env).options;
    set_variables(&options);
    let here = Here { options, folder, env, today: now().day };
    let text = match name {
        "plan_my_day" => plan_my_day(&here, day("date")?.unwrap_or(here.today)),
        "weekly_review" => weekly_review(&here, day("week")?),
        "process_inbox" => process_inbox(&here, given("file"), given("headline"))?,
        "daily_standup" => daily_standup(&here, day("date")?.unwrap_or(here.today)),
        _ => {
            let days = match given("days") {
                Some(text) => text
                    .parse::<u32>()
                    .ok()
                    .filter(|days| *days > 0)
                    .ok_or_else(|| format!("Invalid argument days: `{text}` is not a number of days"))?,
                None => 30,
            };
            find_stale_tasks(&here, days)
        }
    };
    Ok(json!({
        "description": prompt.description,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}

/// What the prompts read: the configuration, as the tools read it.
struct Here<'a> {
    options: Options,
    folder: Option<&'a Path>,
    env: &'a dyn Fn(&str) -> Option<String>,
    today: Day,
}

impl Here<'_> {
    /// A view of `org-lsp agenda`, with `today` for today.
    fn view(&self, arguments: &[&str], today: Day) -> Result<String, Call> {
        let mut view: Vec<String> = arguments.iter().map(|argument| argument.to_string()).collect();
        view.extend(["--today".to_string(), day_text(today)]);
        build_view(view, self.folder, self.env)
    }

    /// The agenda of `span` from `start`, with what was done and clocked
    /// with `log`.
    fn agenda(&self, span: &str, start: Day, today: Day, log: bool) -> Result<String, Call> {
        let start = day_text(start);
        let mut arguments = vec!["--view", "agenda", "--span", span, "--start", &start];
        if log {
            arguments.push("--log");
        }
        self.view(&arguments, today)
    }

    /// The TODO list of `keywords`, or of every keyword not done.
    fn todo_list(&self, keywords: &[String], today: Day) -> Result<String, Call> {
        let wanted = keywords.join("|");
        let mut arguments = vec!["--view", "todo"];
        if !keywords.is_empty() {
            arguments.extend(["--match", &wanted]);
        }
        self.view(&arguments, today)
    }

    /// The clock report of the days from `start` to `end`, included.
    fn clock_report(&self, start: Day, end: Day) -> Result<String, Call> {
        let arguments = json!({ "start": day_text(start), "end": day_text(end) });
        reading("clock_report", &Arguments(&arguments), self.folder, self.env).unwrap_or(Err(Call::Unknown))
    }

    /// The TODO keywords of the agenda files: each one's name, whether it
    /// is done, and whether it starts its sequence.
    fn keywords(&self) -> Vec<(String, bool, bool)> {
        let defaults = self.options.todo_keywords();
        let mut all: Vec<org_syntax::TodoKeywords> = agenda_paths(&self.options, self.folder)
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .map(|text| org_syntax::parse(&text, &defaults).todo_keywords().clone())
            .collect();
        if all.is_empty() {
            all.push(defaults);
        }
        let mut found: Vec<(String, bool, bool)> = Vec::new();
        for keywords in &all {
            for name in keywords.names() {
                if !found.iter().any(|(known, ..)| known == name) {
                    let first = keywords.head(name) == Some(name);
                    found.push((name.to_string(), keywords.is_done(name), first));
                }
            }
        }
        found
    }
}

/// Whether the TODO keyword `name` says the entry waits on something:
/// WAITING, HOLD, BLOCKED, DELEGATED and the like.
fn waits(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["WAIT", "HOLD", "BLOCK", "DELEG", "PAUSE", "PEND"].iter().any(|word| upper.contains(word))
}

/// A part of a prompt: a title, then the data, or why it is not there.
fn section(out: &mut String, title: &str, data: Result<String, Call>) {
    let body = match data {
        Ok(text) if text.trim().is_empty() => "(nothing)".to_string(),
        Ok(text) => text,
        Err(Call::Failed(message)) => format!("(not available: {message})"),
        Err(Call::Unknown) => "(not available)".to_string(),
    };
    out.push_str(&format!("## {title}\n\n```\n{}\n```\n\n", body.trim_end()));
}

/// How the prompts start: what the data is, and how entries are named.
fn opening(what: &str) -> String {
    format!(
        "{what}, from my Org files as org-lsp reads them. Each entry's line ends with where its headline \
         is, [file:line], which org-lsp's tools take (`entry` reads an entry whole).\n\n"
    )
}

/// A day as `YYYY-MM-DD Weekday`.
fn day_name(day: Day) -> String {
    format!("{} {}", day_text(day), org_core::date::WEEKDAY_NAMES[org_core::date::weekday(day)])
}

fn plan_my_day(here: &Here, day: Day) -> String {
    let mut out = opening(&format!("Plan my day, {}", day_name(day)));
    section(
        &mut out,
        &format!("Agenda of {} (with the deadlines coming up and what is overdue)", day_text(day)),
        here.agenda("day", day, day, false),
    );
    section(
        &mut out,
        "TODO entries with a deadline in the next 14 days or past",
        here.view(&["--view", "tags-todo", "--match", "DEADLINE<=\"<+14d>\""], day),
    );
    let progress: Vec<String> = here
        .keywords()
        .into_iter()
        .filter(|(name, done, first)| !done && !first && !waits(name))
        .map(|(name, ..)| name)
        .collect();
    let title = match progress.is_empty() {
        true => "The TODO list".to_string(),
        false => format!("In progress ({})", progress.join(", ")),
    };
    section(&mut out, &title, here.todo_list(&progress, day));
    out.push_str(
        "Make me a realistic plan of the day in time blocks: the timed entries at their times, then the \
         rest by deadline and priority, sized by their Effort where they have one, with breaks. Leave out \
         what does not fit and say so. Flag conflicts: overlapping times, more work than the day has, \
         deadlines at risk. Then offer to schedule the tasks the plan gives a time that are not scheduled \
         for it, with `schedule` (a `date` such as ",
    );
    out.push_str(&format!("{} 14:00).\n\n{CONFIRM}", day_text(day)));
    out
}

fn weekly_review(here: &Here, week: Option<Day>) -> String {
    let today = here.today;
    let start = week.unwrap_or(today);
    let mut out = opening(&format!("Review my week (the agenda's week of {})", day_text(start)));
    section(&mut out, "The week, with what was done and clocked (log mode)", here.agenda("week", start, today, true));
    // The ISO week, from Monday, as the agenda's week with
    // `org-agenda-start-on-weekday`.
    let span = match week {
        Some(day) => {
            let (number, year) = org_core::date::iso_week(day);
            format!("{year}-W{number:02}")
        }
        None => "thisweek".to_string(),
    };
    let arguments = json!({ "span": span });
    let clocked = reading("clock_report", &Arguments(&arguments), here.folder, here.env).unwrap_or(Err(Call::Unknown));
    section(&mut out, "Time clocked this week", clocked);
    section(&mut out, "Stuck projects (org_stuck_projects)", here.view(&["--view", "stuck"], today));
    section(&mut out, "Next week", here.agenda("week", start + 7, today, false));
    out.push_str(
        "Write my weekly review: what got done (the closed entries and the time clocked, by project); what \
         slipped (scheduled or due this week and not done); the stuck projects and a next action for each; \
         the three to five priorities of next week, from its agenda and deadlines. Then suggest changes: \
         reschedule (`schedule`, `deadline`), close (`set_todo_state`), archive (`archive`) or add a next \
         action (`capture`).\n\n",
    );
    out.push_str(CONFIRM);
    out
}

/// Where the inbox is: its file, and its headline (none for the file's
/// top level).
struct Inbox {
    path: PathBuf,
    text: String,
    heading: Option<usize>,
    /// How it was found.
    found: String,
}

/// The headline of `text` whose title is `wanted` (in any case), or whose
/// outline path with it is (`A/B`).
fn find_headline(text: &str, wanted: &str, options: &Options) -> Option<usize> {
    let wanted = wanted.trim().trim_matches('/');
    let keywords = org_syntax::parse(text, &options.todo_keywords()).todo_keywords().clone();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if super::is_headline(line) {
            if let Some(title) = super::entry::title(line, &keywords) {
                let mut path = org_core::refile::outline_path(text, at, &keywords);
                path.push(title.clone());
                if title.eq_ignore_ascii_case(wanted) || path.join("/").eq_ignore_ascii_case(wanted) {
                    return Some(at);
                }
            }
        }
        at += line.len();
    }
    None
}

/// The inbox `process_inbox` goes through: `file` and `headline`, else the
/// target of the first capture template, else an `Inbox` headline of the
/// agenda files.
fn find_inbox(here: &Here, file: Option<String>, headline: Option<String>) -> Result<Inbox, String> {
    let options = &here.options;
    let read =
        |path: &Path| std::fs::read_to_string(path).map_err(|error| format!("Cannot read {}: {error}", path.display()));
    let files = editable_files(options, here.folder);
    let in_file = |path: PathBuf, headline: Option<&str>, found: String| -> Result<Inbox, String> {
        let text = read(&path)?;
        let heading = match headline {
            Some(wanted) => Some(
                find_headline(&text, wanted, options)
                    .ok_or_else(|| format!("{} has no headline {wanted}", path.display()))?,
            ),
            None => None,
        };
        Ok(Inbox { path, text, heading, found })
    };
    if let Some(file) = file {
        let path = editable_file(&file, &files, EDITABLE, here.folder).map_err(|refusal| match refusal {
            Call::Failed(message) => message,
            Call::Unknown => file.clone(),
        })?;
        return in_file(path, headline.as_deref(), "as asked".to_string());
    }
    if headline.is_none() {
        let first = options.capture_templates().into_iter().find(|template| !template.is_group());
        if let Some(template) = first {
            let found = format!("the target of the capture template {} ({})", template.key, template.description);
            let wanted = match crate::server::capture_spec(&template) {
                Ok((_, TargetSpec::File(Target::File))) => Some(None),
                Ok((_, TargetSpec::File(Target::Headline(wanted)))) => Some(Some(wanted)),
                Ok((_, TargetSpec::File(Target::Path(path)))) => Some(Some(path.join("/"))),
                _ => None,
            };
            // A target not there yet (nothing captured) is no inbox.
            if let Some(Ok(inbox)) =
                wanted.map(|wanted| in_file(target_path(&template, options), wanted.as_deref(), found))
            {
                return Ok(inbox);
            }
        }
    }
    let wanted = headline.unwrap_or_else(|| "Inbox".to_string());
    for path in agenda_paths(options, here.folder) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(heading) = find_headline(&text, &wanted, options) {
                let found = format!("the headline {wanted} of an agenda file");
                return Ok(Inbox { path, text, heading: Some(heading), found });
            }
        }
    }
    Err(format!(
        "No inbox found: no capture template files under a headline or at a file's top level, and no agenda \
         file has a headline {wanted}. Give the inbox's file and headline as the arguments file and headline."
    ))
}

fn process_inbox(here: &Here, file: Option<String>, headline: Option<String>) -> Result<String, String> {
    let inbox = find_inbox(here, file, headline)?;
    let options = &here.options;
    let (_, outline) = outline_of(&inbox.text, &inbox.path, options);
    let parent = match inbox.heading {
        Some(heading) => Some(outline.entries.iter().position(|entry| entry.offset == heading).ok_or("No inbox")?),
        None => None,
    };
    let mut labels: Vec<String> =
        agenda_paths(options, here.folder).iter().map(|path| path.display().to_string()).collect();
    let name = inbox.path.display().to_string();
    if !labels.contains(&name) {
        labels.push(name.clone());
    }
    let label = file_label(&name, &labels);
    let mut entries = String::new();
    let mut count = 0;
    for (index, entry) in outline.entries.iter().enumerate().filter(|(_, entry)| entry.parent == parent) {
        let line = inbox.text[..entry.offset].matches('\n').count() + 1;
        let end = entry_ends(&outline, index).1;
        entries.push_str(&format!("[{label}:{line}]\n{}\n\n", inbox.text[entry.offset..end].trim_end()));
        count += 1;
    }
    let place = match inbox.heading {
        Some(heading) => {
            let line = inbox.text[..heading].matches('\n').count() + 1;
            let headline = inbox.text[heading..].lines().next().unwrap_or("");
            format!("{headline} [{label}:{line}]")
        }
        None => format!("the top level of {label}"),
    };
    let mut out = opening(&format!("Process my inbox, {place} ({})", inbox.found));
    section(&mut out, &format!("The inbox's entries ({count})"), Ok(entries));
    let arguments = json!({ "file": name });
    section(&mut out, "Refile targets", refile_targets(&Arguments(&arguments), options, here.folder));
    let mut tags: Vec<String> = Vec::new();
    for entry in options.tag_settings().alist {
        if let org_core::tags::TagEntry::Tag(tag, _) = entry {
            tags.push(tag);
        }
    }
    for path in agenda_paths(options, here.folder) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            tags.extend(org_core::tags::buffer_tags(&text));
        }
    }
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags.dedup();
    section(&mut out, "Tags in use", Ok(tags.join(" ")));
    let keywords: Vec<String> =
        here.keywords().into_iter().map(|(name, done, _)| if done { format!("{name} (done)") } else { name }).collect();
    section(&mut out, "TODO keywords", Ok(keywords.join(" ")));
    out.push_str(
        "For each entry of the inbox, propose: where to refile it (a target as listed, for `refile`), its \
         tags (those in use first, for `set_tags`), a TODO state (`set_todo_state`), and a date when it \
         has one to keep (`schedule` or `deadline`); or that it is done with (`archive`, or `delete_entry`). \
         Show the proposals as a table, one row an entry. Once I confirm, change each entry where it is \
         first, then refile it last.\n\n",
    );
    out.push_str(CONFIRM);
    Ok(out)
}

fn daily_standup(here: &Here, day: Day) -> String {
    let mut out = opening(&format!("Prepare my standup for {}", day_name(day)));
    let before = day - 1;
    section(
        &mut out,
        &format!("{} (the day before), with what was done and clocked", day_text(before)),
        here.agenda("day", before, day, true),
    );
    section(&mut out, &format!("Time clocked on {}", day_text(before)), here.clock_report(before, before));
    section(&mut out, &format!("Planned for {}", day_text(day)), here.agenda("day", day, day, false));
    let keywords = here.keywords();
    let waiting: Vec<String> =
        keywords.iter().filter(|(name, done, _)| !done && waits(name)).map(|(name, ..)| name.clone()).collect();
    let blockers = match waiting.is_empty() {
        true => {
            let names: Vec<&str> = keywords.iter().map(|(name, ..)| name.as_str()).collect();
            Ok(format!("(No TODO keyword says waiting, such as WAITING or HOLD: they are {}.)", names.join(" ")))
        }
        false => here.todo_list(&waiting, day),
    };
    section(&mut out, &format!("Waiting ({})", waiting.join(", ")), blockers);
    out.push_str(
        "Write a short standup in three parts, a few bullets each: Yesterday (what was done and clocked), \
         Today (what is planned, most important first), Blockers (what is waiting, and on what when the \
         entry says). Plain sentences, no file references. Change nothing.",
    );
    out
}

/// The days of the timestamps of `text`: `<2026-10-22 …>` and
/// `[2026-10-22 …]`, active and inactive, in planning lines, clock lines,
/// logged notes and the text.
fn dates(text: &str) -> Vec<Day> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices(['<', '[']) {
        let Some(date) = text.get(at + 1..at + 11) else { continue };
        let digits = date.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
        if digits {
            if let Ok(day) = crate::cli::parse_date(date) {
                found.push(day);
            }
        }
    }
    found
}

fn find_stale_tasks(here: &Here, days: u32) -> String {
    let options = &here.options;
    let since = here.today - i64::from(days);
    let paths = agenda_paths(options, here.folder);
    let labels: Vec<String> = paths.iter().map(|path| path.display().to_string()).collect();
    let mut stale: Vec<(Option<Day>, String)> = Vec::new();
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let (_, outline) = outline_of(&text, path, options);
        let label = file_label(&path.display().to_string(), &labels);
        for (index, entry) in outline.entries.iter().enumerate() {
            if entry.todo.is_none() || entry.done || entry.archived || entry.commented {
                continue;
            }
            // The entry's own text, up to its first child: its dates,
            // clocks and notes.
            let own = &text[entry.offset..entry_ends(&outline, index).0];
            let last = dates(own).into_iter().max();
            if last.is_some_and(|last| last >= since) {
                continue;
            }
            let line = text[..entry.offset].matches('\n').count() + 1;
            let when = match last {
                Some(last) => format!("last date {}", day_text(last)),
                None => "no date".to_string(),
            };
            stale.push((last, format!("{}: {}  [{label}:{line}]  ({when})", entry.category, entry.head)));
        }
    }
    stale.sort_by_key(|(last, _)| *last);
    let list: Vec<String> = stale.into_iter().map(|(_, line)| line).collect();
    let mut out = opening(&format!(
        "Find my stale tasks: the open TODO entries of my agenda files with no date since {} ({days} days ago)",
        day_text(since)
    ));
    out.push_str(
        "No date there means no SCHEDULED or DEADLINE, no clock, no logged note and no other timestamp in \
         the entry since then, nor one ahead.\n\n",
    );
    section(&mut out, &format!("Stale tasks ({}), the oldest first", list.len()), Ok(list.join("\n")));
    out.push_str(
        "For each task, suggest one thing, with a reason in a few words: reschedule it (`schedule` or \
         `deadline`), close it (`set_todo_state` with a done keyword), archive it (`archive`), delete it \
         (`delete_entry`, only for what is worth nothing kept), or keep it as it is. Group them by \
         suggestion. Read an entry with `entry` when its headline does not say enough.\n\n",
    );
    out.push_str(CONFIRM);
    out
}

#[cfg(test)]
mod tests {
    use super::super::answer;
    use serde_json::{json, Value};

    /// A folder with `tasks.org`, its agenda file, and a configuration
    /// file with `config`.
    struct Setup {
        folder: std::path::PathBuf,
        config: std::path::PathBuf,
    }

    impl Setup {
        fn new(name: &str, config: &str, tasks: &str) -> Setup {
            let folder = std::env::temp_dir().join(format!("org-lsp-mcp-prompts-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("tasks.org"), tasks).unwrap();
            let path = folder.join("config.toml");
            let agenda = folder.join("tasks.org").display().to_string();
            std::fs::write(&path, format!("org_agenda_files = [{agenda:?}]\n{config}\n")).unwrap();
            Setup { folder, config: path }
        }

        fn ask(&self, method: &str, params: Value) -> Value {
            let config = self.config.display().to_string();
            let env = |name: &str| (name == "ORG_LSP_CONFIG").then(|| config.clone());
            let line = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
            answer(&line, Some(&self.folder), &env).expect("an answer")
        }

        /// The text of the prompt `name` with `arguments`.
        fn get(&self, name: &str, arguments: Value) -> String {
            let got = self.ask("prompts/get", json!({ "name": name, "arguments": arguments }));
            assert_eq!(got["result"]["messages"][0]["role"], "user", "{got}");
            assert_eq!(got["result"]["messages"][0]["content"]["type"], "text", "{got}");
            got["result"]["messages"][0]["content"]["text"].as_str().unwrap_or_default().to_string()
        }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    const TASKS: &str = "#+TODO: TODO NEXT WAITING | DONE\n* Inbox\n** Buy a lamp\nSeen at the shop.\n\
                         ** TODO Call Ana\n* Projects\n** NEXT Write the report :work:\nDEADLINE: <2026-10-25 Sun>\n\
                         ** WAITING Answer from Bob\n** TODO Old thing\n:LOGBOOK:\n\
                         CLOCK: [2020-01-01 Wed 10:00]--[2020-01-01 Wed 11:00] =>  1:00\n:END:\n\
                         ** TODO Far ahead\nSCHEDULED: <2100-01-01 Fri>\n** DONE Finished\n\
                         CLOSED: [2026-10-21 Wed 10:00]\n:LOGBOOK:\n\
                         CLOCK: [2026-10-21 Wed 09:00]--[2026-10-21 Wed 10:00] =>  1:00\n:END:\n";

    #[test]
    fn lists_the_prompts_and_declares_them() {
        let setup = Setup::new("list", "", TASKS);
        let init = setup.ask("initialize", json!({}));
        assert!(init["result"]["capabilities"]["prompts"].is_object(), "{init}");
        let list = setup.ask("prompts/list", json!({}));
        let prompts = list["result"]["prompts"].as_array().unwrap();
        let names: Vec<&str> = prompts.iter().filter_map(|prompt| prompt["name"].as_str()).collect();
        assert_eq!(names, ["plan_my_day", "weekly_review", "process_inbox", "daily_standup", "find_stale_tasks"]);
        for prompt in prompts {
            assert!(prompt["title"].is_string() && prompt["description"].is_string(), "{prompt}");
            for argument in prompt["arguments"].as_array().unwrap() {
                assert!(argument["name"].is_string() && argument["required"].is_boolean(), "{argument}");
            }
        }
    }

    #[test]
    fn plans_the_day_from_its_agenda() {
        let setup = Setup::new("plan", "", TASKS);
        let text = setup.get("plan_my_day", json!({ "date": "2026-10-22" }));
        assert!(text.starts_with("Plan my day, 2026-10-22 Thursday"), "{text}");
        assert!(text.contains("In   3 d.:  NEXT Write the report"), "{text}");
        assert!(text.contains("## In progress (NEXT)\n"), "{text}");
        assert!(text.contains("[tasks.org:7]") && !text.contains("Old thing"), "{text}");
        assert!(text.contains("`schedule`") && text.contains("`dry_run: true`") && text.contains("mcp_edits"));
        // The deadline is not within 14 days of an earlier day.
        let early = setup.get("plan_my_day", json!({ "date": "2026-10-01" }));
        let deadlines = early.split("## ").find(|part| part.starts_with("TODO entries with a deadline")).unwrap();
        assert!(!deadlines.contains("Write the report"), "{early}");
    }

    #[test]
    fn reviews_the_week_and_prepares_the_standup() {
        let setup = Setup::new("week", "", TASKS);
        let text = setup.get("weekly_review", json!({ "week": "2026-10-19" }));
        assert!(text.contains("Closed:     DONE Finished"), "{text}");
        assert!(text.contains("| *1:00*"), "{text}");
        assert!(text.contains("## Stuck projects") && text.contains("## Next week"), "{text}");
        assert!(text.contains("Week-agenda (W44)"), "{text}");

        let text = setup.get("daily_standup", json!({ "date": "2026-10-22" }));
        assert!(text.contains("## 2026-10-21 (the day before)") && text.contains("DONE Finished"), "{text}");
        assert!(text.contains("## Waiting (WAITING)") && text.contains("WAITING Answer from Bob"), "{text}");
        assert!(text.contains("Change nothing."), "{text}");
    }

    #[test]
    fn processes_the_inbox_of_the_capture_template() {
        let folder = std::env::temp_dir().join(format!("org-lsp-mcp-prompts-inbox-{}", std::process::id()));
        let tasks = folder.join("tasks.org").display().to_string();
        let template = format!(
            "org_capture_templates = [{{ key = \"t\", description = \"Task\", \
             target = [\"file+headline\", {tasks:?}, \"Inbox\"], template = \"* TODO %?\" }}]"
        );
        let setup = Setup::new("inbox", &template, TASKS);
        let text = setup.get("process_inbox", json!({}));
        assert!(text.starts_with("Process my inbox, * Inbox [tasks.org:2] (the target of the capture template t"));
        assert!(text.contains("## The inbox's entries (2)\n\n```\n[tasks.org:3]\n** Buy a lamp\nSeen at the shop."));
        assert!(text.contains("[tasks.org:5]\n** TODO Call Ana\n") && !text.contains("[tasks.org:7]\n"), "{text}");
        assert!(text.contains("Projects  [tasks.org:6]") && text.contains("## Tags in use\n\n```\nwork\n"), "{text}");
        assert!(text.contains("`refile`") && text.contains("`dry_run: true`"), "{text}");

        // Without a template, an Inbox headline; or the one given.
        let setup = Setup::new("inbox-headline", "", TASKS);
        let text = setup.get("process_inbox", json!({}));
        assert!(text.contains("(the headline Inbox of an agenda file)"), "{text}");
        let text = setup.get("process_inbox", json!({ "file": "tasks.org", "headline": "Projects" }));
        assert!(text.contains("## The inbox's entries (5)") && text.contains("[tasks.org:7]"), "{text}");
        let missing = setup.ask("prompts/get", json!({ "name": "process_inbox", "arguments": { "headline": "Nope" } }));
        assert_eq!(missing["error"]["code"], -32602, "{missing}");
    }

    #[test]
    fn finds_the_tasks_without_a_recent_date() {
        let setup = Setup::new("stale", "", TASKS);
        let text = setup.get("find_stale_tasks", json!({}));
        assert!(text.contains("## Stale tasks (") && text.contains("), the oldest first"), "{text}");
        let call = text.find("TODO Call Ana  [tasks.org:5]  (no date)").expect(&text);
        let old = text.find("TODO Old thing  [tasks.org:10]  (last date 2020-01-01)").expect(&text);
        assert!(call < old && text.contains("WAITING Answer from Bob"), "{text}");
        // Neither a date ahead nor a done entry.
        assert!(!text.contains("Far ahead") && !text.contains("Finished"), "{text}");
        assert!(text.contains("`delete_entry`") && text.contains("`archive`"), "{text}");
    }

    #[test]
    fn refuses_unknown_prompts_and_bad_arguments() {
        let setup = Setup::new("refuse", "", TASKS);
        for (name, arguments) in [
            ("nope", json!({})),
            ("plan_my_day", json!({ "date": "tomorrow" })),
            ("weekly_review", json!({ "week": "2026-13-01" })),
            ("find_stale_tasks", json!({ "days": "0" })),
        ] {
            let got = setup.ask("prompts/get", json!({ "name": name, "arguments": arguments }));
            assert_eq!(got["error"]["code"], -32602, "{name}: {got}");
        }
        let got = setup.ask("prompts/get", json!({ "name": "find_stale_tasks", "arguments": { "days": 10 } }));
        assert!(got["result"]["messages"].is_array(), "{got}");
    }
}
