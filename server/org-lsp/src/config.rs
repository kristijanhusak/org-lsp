//! Settings a client passes as `initializationOptions`, and the command
//! line passes as JSON. Names are Org's variable names in camel case
//! without the `org-agenda-` prefix.

use org_core::agenda::{Config as AgendaConfig, FutureRepeats, RemoveTags, Span, TagsColumn, TimeGrid};
use org_core::archive::{ContextInfo, InheritedTags, MarkDone, Settings as ArchiveSettings};
use org_core::edit::Settings as EditSettings;
use org_core::outline::{Inherit, Inheritance};
use org_core::priority::{self, Priorities};
use org_core::read_date::{PreferFuture, Settings as ReadDateSettings};
use org_core::refile::{OutlinePath, Select, Settings as RefileSettings};
use org_core::todo::{RepeatTo, Settings as TodoSettings};
use org_syntax::{Logging, TodoKeywords};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// `org-use-tag-inheritance`: `true`, `false`, a list of tags or a
    /// regexp.
    pub use_tag_inheritance: Option<InheritOption>,
    /// `org-tags-exclude-from-inheritance`
    pub tags_exclude_from_inheritance: Option<Vec<String>>,
    /// `org-use-property-inheritance`: `true`, `false`, a list of
    /// properties or a regexp.
    pub use_property_inheritance: Option<InheritOption>,
    /// `org-clock-into-drawer`: `true` for the drawer of `logIntoDrawer`
    /// (`LOGBOOK`), a drawer name, or `false` for none.
    pub clock_into_drawer: Option<DrawerOption>,
    /// `org-clock-out-remove-zero-time-clocks`
    pub clock_out_remove_zero_time_clocks: Option<bool>,
    /// `org-clock-out-when-done`: a done state stops the clock on the entry.
    pub clock_out_when_done: Option<bool>,
    /// `org-complete-tags-always-offer-all-agenda-tags`: completing a tag
    /// also offers those of the agenda files.
    pub complete_tags_always_offer_all_agenda_tags: Option<bool>,
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
    /// `org-adapt-indentation`: `true`, `false` or `"headline-data"`.
    pub adapt_indentation: Option<serde_json::Value>,
    /// `org-src-preserve-indentation`
    pub src_preserve_indentation: Option<bool>,
    /// `org-startup-indented`: show files with `org-indent-mode`.
    pub startup_indented: Option<bool>,
    /// `org-hide-leading-stars`
    pub hide_leading_stars: Option<bool>,
    /// `org-edit-src-content-indentation`
    pub edit_src_content_indentation: Option<i64>,
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
    /// Whether the client shows documents on `window/showDocument`.
    /// Clients that announce the capability need not say so.
    pub show_document: Option<bool>,
    /// Ask what commands ask for (a key, one of a list) with
    /// `window/showMessageRequest`, and say their messages with
    /// `window/showMessage`: for a client that does nothing with a
    /// command's result (Zed).
    pub message_prompts: Option<bool>,
    /// Offer more commands as code actions (the agenda, capture, refile,
    /// a TODO state or a priority picked from a list, a new heading or
    /// item), and the agenda's commands through the command port (see
    /// `port`): for a client that cannot run a command from a key (Zed).
    pub command_actions: Option<bool>,
    /// Where the command port's script is written, instead of
    /// `~/.local/state/org-lsp/agenda-command`.
    pub command_script: Option<String>,
    /// The client shows `org-lsp/virtualIndent`, as one announcing
    /// `experimental.orgVirtualIndent` does.
    pub virtual_indent: Option<bool>,
    /// The client shows `org-lsp/lineHighlights`, as one announcing
    /// `experimental.orgLineHighlights` does.
    pub line_highlights: Option<bool>,
    /// `org-refile-targets`: where entries can be refiled to. Without it,
    /// the top-level headlines of the entry's own file.
    pub refile_targets: Option<Vec<RefileTargetOption>>,
    /// `org-refile-use-outline-path`: `false`, `true`, `"file"`,
    /// `"full-file-path"` or `"title"`.
    pub refile_use_outline_path: Option<OutlinePathOption>,
    /// `org-reverse-note-order`: refiled entries go first, not last.
    pub reverse_note_order: Option<bool>,
    /// `org-archive-location`: the file and the heading archived subtrees
    /// go to, `"%s_archive::"`.
    pub archive_location: Option<String>,
    /// `org-archive-reversed-order`
    pub archive_reversed_order: Option<bool>,
    /// `org-archive-mark-done`: `false`, `true` or a done keyword.
    pub archive_mark_done: Option<serde_json::Value>,
    /// `org-archive-subtree-add-inherited-tags`: `false`, `true` or
    /// `"infile"`.
    pub archive_subtree_add_inherited_tags: Option<serde_json::Value>,
    /// `org-archive-save-context-info`
    pub archive_save_context_info: Option<Vec<String>>,
    /// `org-archive-sibling-heading`
    pub archive_sibling_heading: Option<String>,
    /// `org-archive-file-header-format`: a format with `%s` for the file,
    /// or `false` for none.
    pub archive_file_header_format: Option<serde_json::Value>,
    /// `org-archive-default-command`: `"subtree"`, `"sibling"` or `"tag"`.
    pub archive_default_command: Option<String>,
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
    /// `org-link-descriptive`: links show their description; the rest is
    /// marked hidden.
    pub link_descriptive: Option<bool>,
    /// `org-hide-emphasis-markers`
    pub hide_emphasis_markers: Option<bool>,
    /// Save the files a command changes other than the one it was run in.
    pub auto_save: Option<bool>,
    /// RET on the first line of a list item makes the next item, as M-RET.
    pub return_uses_meta_return: Option<bool>,
    /// The shared configuration file: `false` for none, or a path instead
    /// of the usual one. Read by `load`, before the options are.
    pub config_file: Option<serde_json::Value>,
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

/// `true`, `false`, a list of names, or a regexp.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InheritOption {
    Flag(bool),
    Names(Vec<String>),
    Matching(String),
}

impl InheritOption {
    fn to_inherit(&self) -> Inherit {
        match self {
            InheritOption::Flag(true) => Inherit::All,
            InheritOption::Flag(false) => Inherit::Nothing,
            InheritOption::Names(names) => Inherit::Names(names.clone()),
            InheritOption::Matching(pattern) => Inherit::Matching(pattern.clone()),
        }
    }
}

/// What entries inherit, from the options that say so; the command line
/// reads these from its `--options` as well.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InheritanceOptions {
    pub use_tag_inheritance: Option<InheritOption>,
    pub tags_exclude_from_inheritance: Option<Vec<String>>,
    pub use_property_inheritance: Option<InheritOption>,
}

impl InheritanceOptions {
    pub fn inheritance(&self) -> Inheritance {
        let default = Inheritance::default();
        Inheritance {
            tags: self.use_tag_inheritance.as_ref().map_or(default.tags, InheritOption::to_inherit),
            tags_excluded: self.tags_exclude_from_inheritance.clone().unwrap_or_default(),
            properties: self.use_property_inheritance.as_ref().map_or(default.properties, InheritOption::to_inherit),
        }
    }
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
    /// `org-use-tag-inheritance`, `org-tags-exclude-from-inheritance` and
    /// `org-use-property-inheritance`.
    pub fn inheritance(&self) -> Inheritance {
        InheritanceOptions {
            use_tag_inheritance: self.use_tag_inheritance.clone(),
            tags_exclude_from_inheritance: self.tags_exclude_from_inheritance.clone(),
            use_property_inheritance: self.use_property_inheritance.clone(),
        }
        .inheritance()
    }

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
            adapt: self.adapt(),
            src_preserve_indentation: self.src_preserve_indentation.unwrap_or(false),
        }
    }

    /// How archiving goes (`org-archive-location` and its relatives).
    pub fn archive_settings(&self) -> ArchiveSettings {
        let defaults = ArchiveSettings::default();
        ArchiveSettings {
            location: self.archive_location.clone().unwrap_or(defaults.location),
            reversed_order: self.archive_reversed_order.unwrap_or(defaults.reversed_order),
            mark_done: match &self.archive_mark_done {
                Some(serde_json::Value::Bool(true)) => MarkDone::Yes,
                Some(serde_json::Value::String(word)) if !word.is_empty() => MarkDone::Keyword(word.clone()),
                _ => MarkDone::No,
            },
            add_inherited_tags: match &self.archive_subtree_add_inherited_tags {
                Some(serde_json::Value::Bool(false)) => InheritedTags::Never,
                Some(serde_json::Value::Bool(true)) => InheritedTags::Always,
                _ => InheritedTags::InFile,
            },
            context_info: match &self.archive_save_context_info {
                Some(names) => names.iter().filter_map(|name| ContextInfo::from_name(name)).collect(),
                None => defaults.context_info,
            },
            sibling_heading: self.archive_sibling_heading.clone().unwrap_or(defaults.sibling_heading),
            file_header_format: match &self.archive_file_header_format {
                Some(serde_json::Value::String(format)) => Some(format.clone()),
                Some(serde_json::Value::Bool(false)) => None,
                _ => defaults.file_header_format,
            },
            levels: self.refile_settings(),
            todo: self.todo_settings(),
        }
    }

    /// `org-archive-default-command`
    pub fn archive_default_command(&self) -> &str {
        match self.archive_default_command.as_deref() {
            Some(command @ ("sibling" | "tag")) => command,
            _ => "subtree",
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

    /// How clocking writes (`org-clock-into-drawer` and its relatives).
    pub fn clock_settings(&self) -> org_core::clock::Settings {
        let log_drawer = match &self.log_into_drawer {
            Some(DrawerOption::Name(name)) if !name.is_empty() => name.clone(),
            _ => "LOGBOOK".to_string(),
        };
        org_core::clock::Settings {
            drawer: match &self.clock_into_drawer {
                Some(DrawerOption::Flag(false)) => None,
                Some(DrawerOption::Name(name)) if !name.is_empty() => Some(name.clone()),
                _ => Some(log_drawer),
            },
            remove_zero: self.clock_out_remove_zero_time_clocks.unwrap_or(false),
            adapt: self.adapt(),
            log_into_drawer: self.todo_settings().log_into_drawer,
        }
    }

    /// How lines are indented (`org-adapt-indentation` and its
    /// relatives).
    pub fn indent_settings(&self) -> org_core::indent::Settings {
        let defaults = org_core::indent::Settings::default();
        org_core::indent::Settings {
            adapt: self.adapt(),
            log_into_drawer: self.todo_settings().log_into_drawer,
            src_preserve_indentation: self.src_preserve_indentation.unwrap_or(defaults.src_preserve_indentation),
            src_content_indentation: self
                .edit_src_content_indentation
                .and_then(|columns| usize::try_from(columns).ok())
                .unwrap_or(defaults.src_content_indentation),
        }
    }

    /// How files show: `org-startup-indented`, `org-hide-leading-stars`.
    pub fn display(&self) -> org_core::indent::Display {
        org_core::indent::Display {
            startup_indented: self.startup_indented.unwrap_or(false),
            hide_leading_stars: self.hide_leading_stars.unwrap_or(false),
        }
    }

    /// `org-adapt-indentation`
    fn adapt(&self) -> org_core::indent::Adapt {
        match &self.adapt_indentation {
            Some(serde_json::Value::Bool(true)) => org_core::indent::Adapt::All,
            Some(serde_json::Value::String(name)) if name == "headline-data" => org_core::indent::Adapt::HeadlineData,
            _ => org_core::indent::Adapt::Off,
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
        settings.adapt = self.adapt();
        settings
    }
}

/// The options and where they came from.
pub struct Loaded {
    pub options: Options,
    /// The shared configuration file that was looked for, if any.
    pub file: Option<PathBuf>,
    /// Whether it was there and read.
    pub read: bool,
    /// The options as given: the file's, with the editor's on top.
    pub merged: serde_json::Value,
    /// What could not be read, to tell the user.
    pub problems: Vec<String>,
}

/// The shared configuration file when nothing names another one:
/// `$ORG_LSP_CONFIG`, else `config.toml` or `config.json` in
/// `$XDG_CONFIG_HOME/org-lsp`, `~/.config/org-lsp`, or on Windows
/// `%APPDATA%\org-lsp`; `config.toml` when neither is there. With both
/// there, `config.toml` is the one, and the second value says so. `var`
/// reads the environment.
pub fn default_config_file(var: &dyn Fn(&str) -> Option<String>) -> (Option<PathBuf>, Option<String>) {
    let set = |name: &str| var(name).filter(|value| !value.is_empty());
    if let Some(path) = set("ORG_LSP_CONFIG") {
        return (Some(expand_home(&path, var)), None);
    }
    let directory = if let Some(config) = set("XDG_CONFIG_HOME") {
        PathBuf::from(config)
    } else if cfg!(windows) {
        match set("APPDATA") {
            Some(directory) => PathBuf::from(directory),
            None => return (None, None),
        }
    } else {
        match set("HOME") {
            Some(home) => PathBuf::from(home).join(".config"),
            None => return (None, None),
        }
    };
    let directory = directory.join("org-lsp");
    let (toml, json) = (directory.join("config.toml"), directory.join("config.json"));
    match (toml.exists(), json.exists()) {
        (true, true) => {
            let warning = format!("{} is ignored, as {} is there too", json.display(), toml.display());
            (Some(toml), Some(warning))
        }
        (false, true) => (Some(json), None),
        _ => (Some(toml), None),
    }
}

/// The options in the configuration file at `path`, which `text` holds:
/// JSON for a `.json` file, with comments and trailing commas allowed as in
/// VS Code's settings, and TOML for any other.
fn parse_config(path: &std::path::Path, text: &str) -> Result<serde_json::Value, String> {
    if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")) {
        let value: serde_json::Value =
            serde_json::from_str(&strip_json_comments(text)).map_err(|error| format!("is not valid JSON: {error}"))?;
        if value.is_object() {
            Ok(value)
        } else {
            Err("is not a JSON object".to_string())
        }
    } else {
        let table = text.parse::<toml::Table>().map_err(|error| format!("is not valid TOML: {error}"))?;
        Ok(serde_json::to_value(table).unwrap_or_default())
    }
}

/// JSON with comments (`//`, `/* */`) and trailing commas made plain JSON.
/// What goes is replaced by spaces, so that errors keep their lines and
/// columns.
fn strip_json_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    // Where the last comma outside a string is in `out`, while only
    // blanks follow it.
    let mut comma: Option<usize> = None;
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c == '"' {
            comma = None;
            out.push(c);
            index += 1;
            while index < chars.len() {
                let c = chars[index];
                out.push(c);
                index += 1;
                if c == '\\' && index < chars.len() {
                    out.push(chars[index]);
                    index += 1;
                } else if c == '"' {
                    break;
                }
            }
        } else if c == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                out.push(' ');
                index += 1;
            }
        } else if c == '/' && chars.get(index + 1) == Some(&'*') {
            out.extend([' ', ' ']);
            index += 2;
            while index < chars.len() && !(chars[index] == '*' && chars.get(index + 1) == Some(&'/')) {
                out.push(if chars[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            if index < chars.len() {
                out.extend([' ', ' ']);
                index += 2;
            }
        } else {
            if (c == '}' || c == ']') && comma.is_some() {
                out[comma.take().unwrap_or_default()] = ' ';
            } else if c == ',' {
                comma = Some(out.len());
            } else if !c.is_whitespace() {
                comma = None;
            }
            out.push(c);
            index += 1;
        }
    }
    out.into_iter().collect()
}

/// `path` with a leading `~/` for the home directory.
pub fn expand_home(path: &str, var: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    match (path.strip_prefix("~/"), var("HOME").or_else(|| var("USERPROFILE"))) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// The options: those of the shared configuration file, with the
/// editor's (`initializationOptions`) on top. An object such as `agenda`
/// is merged key by key; anything else the editor gives replaces the
/// file's. The editor's `configFile` says which file to read: `false` for
/// none, a path, or by default `default_config_file`.
pub fn load(editor: Option<serde_json::Value>, var: &dyn Fn(&str) -> Option<String>) -> Loaded {
    let mut problems = Vec::new();
    let mut editor = match editor {
        Some(serde_json::Value::Object(fields)) => fields,
        // No options; Neovim sends an empty table as `[]`.
        None | Some(serde_json::Value::Null) => serde_json::Map::new(),
        Some(serde_json::Value::Array(items)) if items.is_empty() => serde_json::Map::new(),
        Some(_) => {
            problems.push("the options are not an object and are ignored".to_string());
            serde_json::Map::new()
        }
    };
    // A file the editor names must be there; the usual one may not be.
    let usual = |problems: &mut Vec<String>| {
        let (file, warning) = default_config_file(var);
        problems.extend(warning);
        (file, false)
    };
    let (file, named) = match editor.remove("configFile") {
        Some(serde_json::Value::Bool(false)) => (None, false),
        Some(serde_json::Value::String(path)) if !path.is_empty() => (Some(expand_home(&path, var)), true),
        None | Some(serde_json::Value::Bool(true)) => usual(&mut problems),
        Some(_) => {
            problems.push("the option `configFile` is malformed and is ignored".to_string());
            usual(&mut problems)
        }
    };
    let mut merged = serde_json::json!({});
    let mut read = false;
    if let Some(path) = &file {
        match std::fs::read_to_string(path) {
            Ok(text) => match parse_config(path, &text) {
                Ok(options) => {
                    merged = options;
                    read = true;
                }
                Err(error) => problems.push(format!("{} {error}; it is ignored", path.display())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !named => {}
            Err(error) => problems.push(format!("cannot read {}: {error}", path.display())),
        }
    }
    merge(&mut merged, serde_json::Value::Object(editor));
    let (options, dropped) = Options::read(merged.clone());
    problems.extend(dropped.into_iter().map(|name| format!("the option `{name}` is malformed and is ignored")));
    Loaded { options, file, read, merged, problems }
}

/// `over` on top of `base`: objects key by key, anything else replaced.
pub(crate) fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(base), serde_json::Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(slot) if slot.is_object() && value.is_object() => merge(slot, value),
                    _ => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

/// `org-agenda-prefix-format`, by kind of view.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrefixFormatOptions {
    pub agenda: Option<String>,
    pub todo: Option<String>,
    pub tags: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
    /// `org-agenda-tags-column`: a column, or `"auto"`.
    pub tags_column: Option<serde_json::Value>,
    pub use_time_grid: Option<bool>,
    pub time_grid: TimeGridOptions,
    pub show_current_time_in_grid: Option<bool>,
    pub weekend_days: Option<Vec<usize>>,
    /// `org-agenda-start-with-log-mode`
    pub start_with_log_mode: Option<bool>,
    /// `org-agenda-start-with-clockreport-mode`
    pub start_with_clockreport_mode: Option<bool>,
    /// `org-agenda-start-with-archives-mode`: `"trees"` or `"files"`.
    pub start_with_archives_mode: Option<ArchivesOption>,
    /// `org-agenda-log-mode-items`: `closed`, `clock` and `state`.
    pub log_mode_items: Option<Vec<String>>,
    /// `org-agenda-start-day`: the day the agenda starts from, as the date
    /// prompt reads it (`-1d`, `2026-10-01`).
    pub start_day: Option<String>,
    /// `org-agenda-block-separator`: a character to repeat, a text, or
    /// `false`.
    pub block_separator: Option<serde_json::Value>,
    /// `org-agenda-compact-blocks`
    pub compact_blocks: Option<bool>,
    /// Leave out the empty blocks of a block agenda.
    pub hide_empty_blocks: Option<bool>,
    /// `org-agenda-custom-commands`
    pub custom_commands: Option<Vec<CustomCommandOption>>,
    /// `org-agenda-prefix-format`
    pub prefix_format: PrefixFormatOptions,
    /// `org-agenda-todo-keyword-format`
    pub todo_keyword_format: Option<String>,
    /// `org-agenda-scheduled-leaders`
    pub scheduled_leaders: Option<Vec<String>>,
    /// `org-agenda-deadline-leaders`
    pub deadline_leaders: Option<Vec<String>>,
    /// `org-agenda-timerange-leaders`
    pub timerange_leaders: Option<Vec<String>>,
    /// `org-agenda-remove-tags`: `true`, `false` or `"prefix"`.
    pub remove_tags: Option<serde_json::Value>,
    /// `org-agenda-breadcrumbs-separator`
    pub breadcrumbs_separator: Option<String>,
    /// `org-agenda-format-date`, as a `format-time-string` format.
    pub format_date: Option<String>,
}

/// A command of `org-agenda-custom-commands`: one view (`type`, `match`),
/// a block agenda (`blocks`), or, with neither, a prefix key that groups
/// the commands whose keys start with it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomCommandOption {
    pub key: String,
    pub description: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub matching: Option<String>,
    /// `org-agenda-overriding-header`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Agenda options for this command, over `agenda`'s: `span`,
    /// `startDay`, `deadlineWarningDays`…
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<CustomBlockOption>>,
}

/// One block of a block agenda.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomBlockOption {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub matching: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

/// What the agenda shows of archives (`org-agenda-archives-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchivesMode {
    Off,
    /// Entries in archived trees too (`trees`, `v a`).
    Trees,
    /// Those and the files the agenda files archive to (`t`, `v A`).
    Files,
}

/// `agenda.startWithArchivesMode`: `"trees"`, `"files"` or `false`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ArchivesOption {
    Flag(bool),
    Name(String),
}

impl AgendaOptions {
    /// The archives mode the agenda starts in.
    pub fn archives_mode(&self) -> ArchivesMode {
        match &self.start_with_archives_mode {
            Some(ArchivesOption::Name(name)) if name == "trees" => ArchivesMode::Trees,
            Some(ArchivesOption::Name(name)) if name == "files" => ArchivesMode::Files,
            _ => ArchivesMode::Off,
        }
    }

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
            use_time_grid,
            show_current_time_in_grid,
            compact_blocks,
            hide_empty_blocks
        );
        match &self.block_separator {
            Some(serde_json::Value::Bool(false)) => config.block_separator = None,
            Some(serde_json::Value::String(text)) if text.chars().count() == 1 => {
                config.block_separator = Some(text.repeat(79));
            }
            Some(serde_json::Value::String(text)) => config.block_separator = Some(text.clone()),
            _ => {}
        }
        match &self.tags_column {
            Some(serde_json::Value::Number(column)) => {
                if let Some(column) = column.as_i64() {
                    config.tags_column = TagsColumn::Column(column);
                }
            }
            Some(serde_json::Value::String(name)) if name == "auto" => config.tags_column = TagsColumn::Auto,
            _ => {}
        }
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
        config.show_log = self.start_with_log_mode.unwrap_or(false);
        config.clockreport = self.start_with_clockreport_mode.unwrap_or(false);
        config.archives = self.archives_mode() != ArchivesMode::Off;
        if let Some(items) = &self.log_mode_items {
            let has = |name: &str| items.iter().any(|item| item == name);
            config.log_items =
                org_core::agenda::LogItems { closed: has("closed"), clock: has("clock"), state: has("state") };
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
        let formats = &self.prefix_format;
        let target = &mut config.prefix_format;
        macro_rules! set_format {
            ($($field:ident),*) => {$(
                if let Some(value) = &formats.$field {
                    target.$field = value.clone();
                }
            )*};
        }
        set_format!(agenda, todo, tags, search);
        if let Some(format) = &self.todo_keyword_format {
            config.todo_keyword_format = format.clone();
        }
        if let Some(Ok(leaders)) = self.scheduled_leaders.clone().map(<[String; 2]>::try_from) {
            config.scheduled_leaders = leaders;
        }
        if let Some(Ok(leaders)) = self.deadline_leaders.clone().map(<[String; 3]>::try_from) {
            config.deadline_leaders = leaders;
        }
        if let Some(Ok(leaders)) = self.timerange_leaders.clone().map(<[String; 2]>::try_from) {
            config.timerange_leaders = leaders;
        }
        match &self.remove_tags {
            Some(serde_json::Value::Bool(true)) => config.remove_tags = RemoveTags::Yes,
            Some(serde_json::Value::Bool(false)) => config.remove_tags = RemoveTags::No,
            Some(serde_json::Value::String(name)) if name == "prefix" => config.remove_tags = RemoveTags::Prefix,
            _ => {}
        }
        if let Some(separator) = &self.breadcrumbs_separator {
            config.breadcrumbs_separator = separator.clone();
        }
        if let Some(format) = &self.format_date {
            config.format_date = Some(format.clone());
        }
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
        assert_eq!(config.tags_column, TagsColumn::Column(-100));
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

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("org-lsp-config-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn the_editors_options_go_on_top_of_the_files() {
        let file = temp_file(
            "merge",
            "logDone = \"time\"\ntodoKeywords = [\"TODO\", \"NEXT\", \"|\", \"DONE\"]\n\n[agenda]\ndeadlineWarningDays = 3\nspan = \"week\"\n",
        );
        let editor = serde_json::json!({
            "configFile": file.display().to_string(),
            "logDone": "note",
            "agenda": { "span": "day" }
        });
        let loaded = load(Some(editor), &no_env);
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!((loaded.file, loaded.read), (Some(file.clone()), true));
        assert_eq!(
            loaded.merged,
            serde_json::json!({
                "logDone": "note",
                "todoKeywords": ["TODO", "NEXT", "|", "DONE"],
                "agenda": { "deadlineWarningDays": 3, "span": "day" }
            })
        );
        let config = loaded.options.agenda_config();
        assert_eq!((config.span, config.deadline_warning_days), (Span::Day, 3));
        assert_eq!(loaded.options.todo_settings().log_done, Some(Logging::Note));

        // `false`: no file.
        let alone = load(Some(serde_json::json!({ "configFile": false, "logDone": "time" })), &no_env);
        assert_eq!((alone.file, alone.read, alone.merged), (None, false, serde_json::json!({ "logDone": "time" })));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_and_left_out() {
        let file = temp_file("broken", "logDone = \"time\"\nagenda = [\n");
        let loaded =
            load(Some(serde_json::json!({ "configFile": file.display().to_string(), "logRepeat": false })), &no_env);
        assert!(!loaded.read);
        assert_eq!(loaded.problems.len(), 1);
        assert!(
            loaded.problems[0].contains("is not valid TOML") && loaded.problems[0].contains("line 2"),
            "{:?}",
            loaded.problems
        );
        assert_eq!(loaded.merged, serde_json::json!({ "logRepeat": false }));

        // A malformed option in the file goes on its own.
        std::fs::write(&file, "logDone = \"time\"\ntagsColumn = \"far\"\n").unwrap();
        let loaded = load(Some(serde_json::json!({ "configFile": file.display().to_string() })), &no_env);
        assert_eq!(loaded.problems, ["the option `tagsColumn` is malformed and is ignored"]);
        assert_eq!(loaded.options.todo_settings().log_done, Some(Logging::Time));

        // A file the editor names must be there; the usual one need not be.
        let missing = file.with_file_name("missing.toml");
        let named = load(Some(serde_json::json!({ "configFile": missing.display().to_string() })), &no_env);
        assert!(named.problems[0].starts_with("cannot read"), "{:?}", named.problems);
        let home = file.parent().unwrap().display().to_string();
        let usual = load(None, &|name| (name == "HOME").then(|| home.clone()));
        assert!(usual.problems.is_empty() && !usual.read);
        assert_eq!(usual.file, Some(file.parent().unwrap().join(".config/org-lsp/config.toml")));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_json_file_may_have_comments_and_trailing_commas() {
        let text = r#"{
  // Where the agenda is built from.
  "agendaFiles": ["~/org", "https://x//y", "/* kept */",],
  /* Org's own
     default. */
  "agenda": { "span": "day", },
}
"#;
        assert_eq!(
            parse_config(std::path::Path::new("config.json"), text),
            Ok(
                serde_json::json!({ "agendaFiles": ["~/org", "https://x//y", "/* kept */"], "agenda": { "span": "day" } })
            )
        );
        // Errors keep their lines.
        let error = parse_config(std::path::Path::new("config.json"), "{\n  // a\n  \"logDone\": time\n}").unwrap_err();
        assert!(error.contains("line 3"), "{error}");
        assert_eq!(parse_config(std::path::Path::new("c.JSON"), "[1]"), Err("is not a JSON object".to_string()));
        // Any other name is TOML.
        assert_eq!(
            parse_config(std::path::Path::new("work.conf"), "logDone = \"time\""),
            Ok(serde_json::json!({ "logDone": "time" }))
        );
    }

    #[test]
    fn config_toml_or_config_json() {
        let toml = temp_file("formats", "logDone = \"time\"\n");
        let directory = toml.parent().unwrap().to_path_buf();
        let config = directory.join("org-lsp");
        std::fs::create_dir_all(&config).unwrap();
        let home = directory.display().to_string();
        let env = |name: &str| (name == "XDG_CONFIG_HOME").then(|| home.clone());
        // Only JSON: that one.
        std::fs::write(config.join("config.json"), "{ \"logDone\": \"note\", }").unwrap();
        assert_eq!(default_config_file(&env), (Some(config.join("config.json")), None));
        let loaded = load(None, &env);
        assert_eq!((loaded.read, loaded.merged), (true, serde_json::json!({ "logDone": "note" })));
        // Both: TOML, and a word about the other.
        std::fs::rename(&toml, config.join("config.toml")).unwrap();
        let (file, warning) = default_config_file(&env);
        assert_eq!(file, Some(config.join("config.toml")));
        assert!(warning.unwrap().contains("config.json is ignored"));
        let loaded = load(None, &env);
        assert_eq!(loaded.merged, serde_json::json!({ "logDone": "time" }));
        assert_eq!(loaded.problems.len(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn where_the_shared_file_is() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
        };
        let file = |pairs| default_config_file(&env(pairs)).0;
        assert_eq!(file(&[("ORG_LSP_CONFIG", "~/my.toml"), ("HOME", "/h")]), Some(PathBuf::from("/h/my.toml")));
        assert_eq!(file(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]), Some(PathBuf::from("/x/org-lsp/config.toml")));
        if !cfg!(windows) {
            assert_eq!(file(&[("HOME", "/h")]), Some(PathBuf::from("/h/.config/org-lsp/config.toml")));
        }
        assert_eq!(file(&[]), None);
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
