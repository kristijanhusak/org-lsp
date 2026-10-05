//! Settings a client passes as `initializationOptions`, and the command
//! line passes as JSON. Names are Org's variable names in camel case
//! without the `org-agenda-` prefix.

use org_core::agenda::{Config as AgendaConfig, FutureRepeats, Span, TimeGrid};
use org_core::edit::Settings as EditSettings;
use org_core::priority::{self, Priorities};
use org_core::read_date::{PreferFuture, Settings as ReadDateSettings};
use org_core::refile::{OutlinePath, Select, Settings as RefileSettings};
use org_core::todo::{RepeatTo, Settings as TodoSettings};
use org_syntax::{Logging, TodoKeywords};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    /// The default TODO sequence, written like Org's `#+TODO:` line:
    /// `["TODO", "NEXT", "|", "DONE"]`. Documents can override it.
    pub todo_keywords: Option<Vec<String>>,
    /// Files, directories and glob patterns the agenda is built from.
    pub agenda_files: Vec<String>,
    pub agenda: AgendaOptions,
    /// `org-log-done`: `false`, `"time"` or `"note"`.
    pub log_done: Option<LogOption>,
    /// `org-log-repeat`: `false`, `"time"` or `"note"`.
    pub log_repeat: Option<LogOption>,
    /// `org-log-into-drawer`: `true` for `LOGBOOK`, or a drawer name.
    pub log_into_drawer: Option<DrawerOption>,
    /// `org-tags-column`
    pub tags_column: Option<i64>,
    /// `org-todo-repeat-to-state`: a keyword, or `true` for the state the
    /// entry had before it was marked done.
    pub todo_repeat_to_state: Option<RepeatToOption>,
    /// `org-priority-highest`: a letter, or a number from 0 to 64.
    pub priority_highest: Option<PriorityOption>,
    /// `org-priority-lowest`
    pub priority_lowest: Option<PriorityOption>,
    /// `org-priority-default`
    pub priority_default: Option<PriorityOption>,
    /// `org-priority-start-cycle-with-default`
    pub priority_start_cycle_with_default: Option<bool>,
    /// The step in which the minutes of a timestamp change: the second
    /// number of `org-time-stamp-rounding-minutes`.
    pub timestamp_rounding_minutes: Option<i64>,
    /// `org-cycle-separator-lines`: how many blank lines after a subtree
    /// keep one of them visible when it is folded.
    pub cycle_separator_lines: Option<i64>,
    /// `org-read-date-prefer-future`: `true`, `false` or `"time"`.
    pub read_date_prefer_future: Option<PreferFutureOption>,
    /// `org-read-date-force-compatible-dates`
    pub read_date_force_compatible_dates: Option<bool>,
    /// Whether the client fetches the agenda from the server as a virtual
    /// document. Clients that announce LSP 3.18's `textDocumentContent`
    /// need not say so.
    pub virtual_documents: Option<bool>,
    /// `org-refile-targets`: where entries can be refiled to. Without it,
    /// the top-level headlines of the entry's own file.
    pub refile_targets: Option<Vec<RefileTargetOption>>,
    /// `org-refile-use-outline-path`: `false`, `true`, `"file"`,
    /// `"full-file-path"` or `"title"`.
    pub refile_use_outline_path: Option<OutlinePathOption>,
    /// `org-reverse-note-order`: refiled entries go first, not last.
    pub reverse_note_order: Option<bool>,
    /// `org-capture-templates`. Without it, Org's default: a TODO under
    /// "Tasks" in `defaultNotesFile`.
    pub capture_templates: Option<Vec<CaptureTemplateOption>>,
    /// `org-directory`: where relative capture targets are. `~/org`.
    pub org_directory: Option<String>,
    /// `org-default-notes-file`: the target of a template whose file is
    /// empty. `~/.notes`.
    pub default_notes_file: Option<String>,
    /// A fixed date and time, for tests.
    pub clock: Option<ClockOption>,
}

/// An entry of `org-refile-targets`: the files, `"current"` (the entry's
/// own, the default), `"agenda"`, a path or a list of paths, and one of
/// `level`, `maxLevel`, `tag`, `todo`, `regexp` (the `regex` crate's
/// syntax) or `all`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RefileTargetOption {
    pub files: Option<RefileFilesOption>,
    pub level: Option<usize>,
    pub max_level: Option<usize>,
    pub tag: Option<String>,
    pub todo: Option<String>,
    pub regexp: Option<String>,
    pub all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RefileFilesOption {
    One(String),
    Many(Vec<String>),
}

/// Where refile targets are looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefileFiles {
    /// The file of the entry being refiled.
    Current,
    /// The agenda files.
    Agenda,
    Paths(Vec<String>),
}

/// An entry of `org-refile-targets`, read.
#[derive(Debug, Clone)]
pub struct RefileTarget {
    pub files: RefileFiles,
    pub select: Select,
}

/// An entry of `org-capture-templates`. One with only a key and a
/// description groups the templates whose keys start with it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptureTemplateOption {
    pub key: String,
    pub description: String,
    /// `"entry"` (the default) or `"plain"`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// `["file", path]`, `["file+headline", path, headline]` or
    /// `["file+olp", path, headline…]`.
    pub target: Vec<String>,
    /// The template, or its lines.
    pub template: Option<TemplateText>,
    pub prepend: Option<bool>,
    pub empty_lines: Option<usize>,
    pub empty_lines_before: Option<usize>,
    pub empty_lines_after: Option<usize>,
    /// `:immediate-finish`: no capture buffer.
    pub immediate_finish: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TemplateText {
    Text(String),
    Lines(Vec<String>),
}

impl TemplateText {
    pub fn text(&self) -> String {
        match self {
            TemplateText::Text(text) => text.clone(),
            TemplateText::Lines(lines) => lines.join("\n"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OutlinePathOption {
    Flag(bool),
    Name(String),
}

/// `false`, `true`, `"time"` or `"note"`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum LogOption {
    Flag(bool),
    Name(String),
}

impl LogOption {
    fn to_logging(&self) -> Option<Logging> {
        match self {
            LogOption::Flag(false) => None,
            LogOption::Flag(true) => Some(Logging::Time),
            LogOption::Name(name) if name == "note" => Some(Logging::Note),
            LogOption::Name(name) if name == "time" => Some(Logging::Time),
            LogOption::Name(_) => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DrawerOption {
    Flag(bool),
    Name(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RepeatToOption {
    Flag(bool),
    State(String),
}

/// `true`, `false` or `"time"`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PreferFutureOption {
    Flag(bool),
    Name(String),
}

/// A priority: `"A"` or `1`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PriorityOption {
    Number(i64),
    Letter(String),
}

impl PriorityOption {
    fn value(&self) -> Option<i64> {
        match self {
            PriorityOption::Number(number) => Some(*number),
            PriorityOption::Letter(letter) => priority::value(letter),
        }
    }
}

impl Options {
    /// The options in `value`. One that is malformed is left out, not
    /// the rest with it; its dotted name is returned with the options.
    pub fn read(value: serde_json::Value) -> (Options, Vec<String>) {
        // No options; Neovim sends an empty table as `[]`.
        if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
            return (Options::default(), Vec::new());
        }
        let mut dropped = Vec::new();
        let value = keep_readable(&mut Vec::new(), value, &mut dropped).unwrap_or_default();
        (serde_json::from_value(value).unwrap_or_default(), dropped)
    }

    /// The default TODO sequence: `todoKeywords`, or Org's `TODO | DONE`.
    pub fn todo_keywords(&self) -> TodoKeywords {
        self.todo_keywords
            .clone()
            .map(TodoKeywords::from_sequence)
            .filter(|keywords| !keywords.is_empty())
            .unwrap_or_default()
    }

    /// `org-directory`
    pub fn org_directory(&self) -> String {
        self.org_directory.clone().unwrap_or_else(|| "~/org".to_string())
    }

    /// `org-default-notes-file`
    pub fn default_notes_file(&self) -> String {
        self.default_notes_file.clone().unwrap_or_else(|| "~/.notes".to_string())
    }

    /// `org-cycle-separator-lines`
    pub fn separator_lines(&self) -> i64 {
        self.cycle_separator_lines.unwrap_or(crate::folding::SEPARATOR_LINES)
    }

    /// `org-refile-targets`, with Org's default: the top-level headlines
    /// of the entry's own file. An entry that selects nothing known is
    /// left out.
    pub fn refile_targets(&self) -> Vec<RefileTarget> {
        let Some(targets) = &self.refile_targets else {
            return vec![RefileTarget { files: RefileFiles::Current, select: Select::Level(1) }];
        };
        targets
            .iter()
            .filter_map(|target| {
                let files = match &target.files {
                    None => RefileFiles::Current,
                    Some(RefileFilesOption::One(name)) if name == "current" => RefileFiles::Current,
                    Some(RefileFilesOption::One(name)) if name == "agenda" => RefileFiles::Agenda,
                    Some(RefileFilesOption::One(path)) => RefileFiles::Paths(vec![path.clone()]),
                    Some(RefileFilesOption::Many(paths)) => RefileFiles::Paths(paths.clone()),
                };
                let select = if target.all == Some(true) {
                    Select::All
                } else if let Some(level) = target.level {
                    Select::Level(level)
                } else if let Some(level) = target.max_level {
                    Select::MaxLevel(level)
                } else if let Some(tag) = &target.tag {
                    Select::Tag(tag.clone())
                } else if let Some(keyword) = &target.todo {
                    Select::Todo(keyword.clone())
                } else {
                    Select::regexp(target.regexp.as_deref()?)?
                };
                Some(RefileTarget { files, select })
            })
            .collect()
    }

    /// `org-refile-use-outline-path`
    pub fn refile_outline_path(&self) -> OutlinePath {
        match &self.refile_use_outline_path {
            None | Some(OutlinePathOption::Flag(false)) => OutlinePath::No,
            Some(OutlinePathOption::Flag(true)) => OutlinePath::Yes,
            Some(OutlinePathOption::Name(name)) => match name.as_str() {
                "file" => OutlinePath::File,
                "full-file-path" => OutlinePath::FullFilePath,
                "title" => OutlinePath::Title,
                _ => OutlinePath::Yes,
            },
        }
    }

    /// `org-capture-templates`, or Org's default one.
    pub fn capture_templates(&self) -> Vec<CaptureTemplateOption> {
        self.capture_templates.clone().unwrap_or_else(|| {
            vec![CaptureTemplateOption {
                key: "t".to_string(),
                description: "Task".to_string(),
                target: vec!["file+headline".to_string(), String::new(), "Tasks".to_string()],
                template: Some(TemplateText::Text("* TODO %?\n  %u\n  %a".to_string())),
                ..CaptureTemplateOption::default()
            }]
        })
    }

    pub fn refile_settings(&self) -> RefileSettings {
        RefileSettings {
            reverse_note_order: self.reverse_note_order.unwrap_or(false),
            tags_column: self.tags_column.unwrap_or(-77),
        }
    }

    /// The priorities in use: Org's `A` to `C` unless set otherwise.
    pub fn priorities(&self) -> Priorities {
        let mut priorities = Priorities::default();
        let value = |option: &Option<PriorityOption>| option.as_ref().and_then(PriorityOption::value);
        if let Some(highest) = value(&self.priority_highest) {
            priorities.highest = highest;
        }
        if let Some(lowest) = value(&self.priority_lowest) {
            priorities.lowest = lowest;
        }
        if let Some(default) = value(&self.priority_default) {
            priorities.default = default;
        }
        priorities
    }

    /// The agenda settings, with the priorities the agenda sorts by.
    pub fn agenda_config(&self) -> AgendaConfig {
        AgendaConfig { priorities: self.priorities(), ..self.agenda.to_config() }
    }

    /// How answers to the date prompt are read.
    pub fn read_date_settings(&self) -> ReadDateSettings {
        let defaults = ReadDateSettings::default();
        ReadDateSettings {
            prefer_future: match &self.read_date_prefer_future {
                Some(PreferFutureOption::Flag(false)) => PreferFuture::No,
                Some(PreferFutureOption::Name(name)) if name == "time" => PreferFuture::Time,
                Some(_) => PreferFuture::Yes,
                None => defaults.prefer_future,
            },
            force_compatible_dates: self.read_date_force_compatible_dates.unwrap_or(defaults.force_compatible_dates),
        }
    }

    /// The settings of the commands that edit a document.
    pub fn edit_settings(&self) -> EditSettings {
        let defaults = EditSettings::default();
        EditSettings {
            todo: self.todo_settings(),
            priorities: self.priorities(),
            priority_start_cycle_with_default: self
                .priority_start_cycle_with_default
                .unwrap_or(defaults.priority_start_cycle_with_default),
            timestamp_rounding_minutes: self
                .timestamp_rounding_minutes
                .filter(|minutes| *minutes >= 0)
                .unwrap_or(defaults.timestamp_rounding_minutes),
        }
    }

    /// The settings for changing TODO states: Org's defaults with the
    /// given options applied.
    pub fn todo_settings(&self) -> TodoSettings {
        let mut settings = TodoSettings::default();
        if let Some(option) = &self.log_done {
            settings.log_done = option.to_logging();
        }
        if let Some(option) = &self.log_repeat {
            settings.log_repeat = option.to_logging();
        }
        settings.log_into_drawer = match &self.log_into_drawer {
            Some(DrawerOption::Flag(true)) => Some("LOGBOOK".to_string()),
            Some(DrawerOption::Name(name)) if !name.is_empty() => Some(name.clone()),
            _ => None,
        };
        if let Some(column) = self.tags_column {
            settings.tags_column = column;
        }
        settings.repeat_to_state = match &self.todo_repeat_to_state {
            Some(RepeatToOption::Flag(true)) => RepeatTo::Previous,
            Some(RepeatToOption::State(state)) => RepeatTo::State(state.clone()),
            _ => RepeatTo::Head,
        };
        settings
    }
}

/// `value`, at `path` among the options, without what cannot be read: an
/// object loses the fields that cannot, anything else is left out whole.
fn keep_readable(
    path: &mut Vec<String>,
    value: serde_json::Value,
    dropped: &mut Vec<String>,
) -> Option<serde_json::Value> {
    let alone = path.iter().rev().fold(value.clone(), |inner, key| serde_json::json!({ key: inner }));
    if serde_json::from_value::<Options>(alone).is_ok() {
        return Some(value);
    }
    let serde_json::Value::Object(fields) = value else {
        dropped.push(path.join("."));
        return None;
    };
    let mut kept = serde_json::Map::new();
    for (key, field) in fields {
        path.push(key.clone());
        if let Some(field) = keep_readable(path, field, dropped) {
            kept.insert(key, field);
        }
        path.pop();
    }
    Some(serde_json::Value::Object(kept))
}

/// `{ "today": "2026-05-18", "now": "10:30" }`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClockOption {
    pub today: String,
    pub now: Option<String>,
}

impl ClockOption {
    pub fn to_now(&self) -> Option<org_core::agenda::Now> {
        let today = crate::cli::parse_date(&self.today).ok()?;
        let minute = match &self.now {
            Some(now) => crate::cli::parse_time(now).ok()?,
            None => 0,
        };
        Some(org_core::agenda::Now { today, minute })
    }
}

/// A span given as a name or as a number of days.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SpanOption {
    Name(String),
    Days(i64),
}

impl SpanOption {
    pub fn to_span(&self) -> Option<Span> {
        match self {
            SpanOption::Days(days) if *days > 0 => Some(Span::from_days(*days)),
            SpanOption::Days(_) => None,
            SpanOption::Name(name) => Some(match name.as_str() {
                "day" => Span::Day,
                "week" => Span::Week,
                "fortnight" => Span::Fortnight,
                "month" => Span::Month,
                "year" => Span::Year,
                other => return other.parse().ok().filter(|days| *days > 0).map(Span::from_days),
            }),
        }
    }
}

/// `org-agenda-show-future-repeats`: `true`, `false` or `"next"`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FutureRepeatsOption {
    Flag(bool),
    Name(String),
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimeGridOptions {
    pub daily: Option<bool>,
    pub today: Option<bool>,
    pub weekly: Option<bool>,
    pub require_timed: Option<bool>,
    pub remove_match: Option<bool>,
    pub times: Option<Vec<i64>>,
    pub trailer: Option<String>,
    pub line: Option<String>,
    pub now: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgendaOptions {
    pub span: Option<SpanOption>,
    /// 0 is Sunday; `false` starts week views on the current day.
    pub start_on_weekday: Option<serde_json::Value>,
    pub deadline_warning_days: Option<i64>,
    pub scheduled_delay_days: Option<i64>,
    pub scheduled_past_days: Option<i64>,
    pub deadline_past_days: Option<i64>,
    pub skip_scheduled_if_done: Option<bool>,
    pub skip_deadline_if_done: Option<bool>,
    pub skip_timestamp_if_done: Option<bool>,
    pub show_future_repeats: Option<FutureRepeatsOption>,
    pub show_all_dates: Option<bool>,
    pub tags_column: Option<i64>,
    pub use_time_grid: Option<bool>,
    pub time_grid: TimeGridOptions,
    pub show_current_time_in_grid: Option<bool>,
    pub weekend_days: Option<Vec<usize>>,
}

impl AgendaOptions {
    /// Org's defaults with the given settings applied. Values that make
    /// no sense are ignored.
    pub fn to_config(&self) -> AgendaConfig {
        let mut config = AgendaConfig::default();
        macro_rules! set {
            ($($field:ident),*) => {$(
                if let Some(value) = &self.$field {
                    config.$field = value.clone();
                }
            )*};
        }
        set!(
            deadline_warning_days,
            scheduled_delay_days,
            scheduled_past_days,
            deadline_past_days,
            skip_scheduled_if_done,
            skip_deadline_if_done,
            skip_timestamp_if_done,
            show_all_dates,
            tags_column,
            use_time_grid,
            show_current_time_in_grid
        );
        if let Some(span) = self.span.as_ref().and_then(SpanOption::to_span) {
            config.span = span;
        }
        match &self.start_on_weekday {
            Some(serde_json::Value::Bool(false)) => config.start_on_weekday = None,
            Some(serde_json::Value::Number(day)) => {
                if let Some(day) = day.as_u64().filter(|day| *day < 7) {
                    config.start_on_weekday = Some(day as usize);
                }
            }
            _ => {}
        }
        match &self.show_future_repeats {
            Some(FutureRepeatsOption::Flag(true)) => config.show_future_repeats = FutureRepeats::All,
            Some(FutureRepeatsOption::Flag(false)) => config.show_future_repeats = FutureRepeats::None,
            Some(FutureRepeatsOption::Name(name)) if name == "next" => config.show_future_repeats = FutureRepeats::Next,
            _ => {}
        }
        if let Some(days) = &self.weekend_days {
            config.weekend_days = days.iter().copied().filter(|day| *day < 7).collect();
        }
        let grid = &self.time_grid;
        let target: &mut TimeGrid = &mut config.time_grid;
        macro_rules! set_grid {
            ($($field:ident),*) => {$(
                if let Some(value) = &grid.$field {
                    target.$field = value.clone();
                }
            )*};
        }
        set_grid!(daily, today, weekly, require_timed, remove_match, times, trailer, line, now);
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_orgs() {
        let options: Options = serde_json::from_str("{}").unwrap();
        assert_eq!(options.agenda.to_config(), AgendaConfig::default());
    }

    #[test]
    fn reads_settings() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "agendaFiles": ["~/org"],
            "agenda": {
                "span": "day",
                "startOnWeekday": 0,
                "skipScheduledIfDone": true,
                "showFutureRepeats": "next",
                "tagsColumn": -100,
                "timeGrid": { "times": [900, 1700], "weekly": true },
                "unknown": 1
            }
        }))
        .unwrap();
        let config = options.agenda.to_config();
        assert_eq!(options.agenda_files, ["~/org"]);
        assert_eq!(config.span, Span::Day);
        assert_eq!(config.start_on_weekday, Some(0));
        assert!(config.skip_scheduled_if_done);
        assert_eq!(config.show_future_repeats, FutureRepeats::Next);
        assert_eq!(config.tags_column, -100);
        assert_eq!(config.time_grid.times, [900, 1700]);
        assert!(config.time_grid.weekly && config.time_grid.today);
    }

    #[test]
    fn reads_priorities_and_rounding() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "priorityHighest": "A", "priorityLowest": "E", "priorityDefault": "C",
            "priorityStartCycleWithDefault": false, "timestampRoundingMinutes": 15, "tagsColumn": -90
        }))
        .unwrap();
        let settings = options.edit_settings();
        assert_eq!(settings.priorities, Priorities { highest: 65, lowest: 69, default: 67 });
        assert!(!settings.priority_start_cycle_with_default);
        assert_eq!(settings.timestamp_rounding_minutes, 15);
        assert_eq!(settings.todo.tags_column, -90);
        assert_eq!(options.agenda_config().priorities, settings.priorities);
        let numbers: Options = serde_json::from_value(
            serde_json::json!({ "priorityHighest": 1, "priorityLowest": 9, "priorityDefault": 5 }),
        )
        .unwrap();
        assert_eq!(numbers.priorities(), Priorities { highest: 1, lowest: 9, default: 5 });
        let defaults: Options = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.edit_settings(), EditSettings::default());
    }

    #[test]
    fn a_malformed_option_leaves_the_others() {
        let (options, dropped) = Options::read(serde_json::json!({
            "logDone": "time",
            "agendaFiles": "~/org",
            "agenda": { "span": "day", "deadlineWarningDays": "soon", "timeGrid": { "times": [900], "daily": 1 } },
            "unknown": true
        }));
        assert_eq!(dropped, ["agenda.deadlineWarningDays", "agenda.timeGrid.daily", "agendaFiles"]);
        assert_eq!(options.todo_settings().log_done, Some(Logging::Time));
        let config = options.agenda_config();
        assert_eq!((config.span, config.deadline_warning_days, config.time_grid.times), (Span::Day, 14, vec![900]));
        let (whole, none) = Options::read(serde_json::json!({ "logDone": "time" }));
        assert!(none.is_empty() && whole.log_done.is_some());
        assert!(Options::read(serde_json::json!([1])).1 == [""]);
        // Neovim sends an empty table as `[]`.
        assert!(Options::read(serde_json::json!([])).1.is_empty());
        assert!(Options::read(serde_json::Value::Null).1.is_empty());
    }

    #[test]
    fn spans() {
        let span = |value: serde_json::Value| serde_json::from_value::<SpanOption>(value).unwrap().to_span();
        assert_eq!(span(serde_json::json!(7)), Some(Span::Week));
        assert_eq!(span(serde_json::json!(3)), Some(Span::Days(3)));
        assert_eq!(span(serde_json::json!("10")), Some(Span::Days(10)));
        assert_eq!(span(serde_json::json!("month")), Some(Span::Month));
        assert_eq!(span(serde_json::json!(0)), None);
        assert_eq!(span(serde_json::json!("soon")), None);
        let off: AgendaOptions = serde_json::from_value(serde_json::json!({ "startOnWeekday": false })).unwrap();
        assert_eq!(off.to_config().start_on_weekday, None);
    }
}
