//! Settings a client passes as `initializationOptions`, and the command
//! line passes as JSON. Names are Org's variable names with `_` for `-`
//! (`org_agenda_files`; `org-agenda-files` is read too); those of the
//! server's own, without `org_`.

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
#[serde(default)]
pub struct Options {
    /// The default TODO sequence, written like Org's `#+TODO:` line:
    /// `["TODO", "NEXT", "|", "DONE"]`. Documents can override it.
    #[serde(rename = "org_todo_keywords")]
    pub todo_keywords: Option<Vec<String>>,
    /// Files, directories and glob patterns the agenda is built from.
    #[serde(rename = "org_agenda_files")]
    pub agenda_files: Vec<String>,
    #[serde(flatten)]
    pub agenda: AgendaOptions,
    /// `org-log-done`: `false`, `"time"` or `"note"`.
    #[serde(rename = "org_log_done")]
    pub log_done: Option<LogOption>,
    /// `org-log-repeat`: `false`, `"time"` or `"note"`.
    #[serde(rename = "org_log_repeat")]
    pub log_repeat: Option<LogOption>,
    /// `org-list-use-circular-motion`
    #[serde(rename = "org_list_use_circular_motion")]
    pub list_use_circular_motion: Option<bool>,
    /// `org-agenda-dim-blocked-tasks`
    #[serde(rename = "org_agenda_dim_blocked_tasks")]
    pub agenda_dim_blocked_tasks: Option<LogOption>,
    /// `org-enforce-todo-dependencies`
    #[serde(rename = "org_enforce_todo_dependencies")]
    pub enforce_todo_dependencies: Option<bool>,
    /// `org-enforce-todo-checkbox-dependencies`
    #[serde(rename = "org_enforce_todo_checkbox_dependencies")]
    pub enforce_todo_checkbox_dependencies: Option<bool>,
    /// `org-log-refile`: `false`, `"time"` or `"note"`.
    #[serde(rename = "org_log_refile")]
    pub log_refile: Option<LogOption>,
    /// `org-log-reschedule`: `false`, `"time"` or `"note"`.
    #[serde(rename = "org_log_reschedule")]
    pub log_reschedule: Option<LogOption>,
    /// `org-log-redeadline`: `false`, `"time"` or `"note"`.
    #[serde(rename = "org_log_redeadline")]
    pub log_redeadline: Option<LogOption>,
    /// `org-log-into-drawer`: `true` for `LOGBOOK`, or a drawer name.
    #[serde(rename = "org_log_into_drawer")]
    pub log_into_drawer: Option<DrawerOption>,
    /// `org-tags-column`
    #[serde(rename = "org_tags_column")]
    pub tags_column: Option<i64>,
    /// `org-use-tag-inheritance`: `true`, `false`, a list of tags or a
    /// regexp.
    #[serde(rename = "org_use_tag_inheritance")]
    pub use_tag_inheritance: Option<InheritOption>,
    /// `org-tags-exclude-from-inheritance`
    #[serde(rename = "org_tags_exclude_from_inheritance")]
    pub tags_exclude_from_inheritance: Option<Vec<String>>,
    /// `org-use-property-inheritance`: `true`, `false`, a list of
    /// properties or a regexp.
    #[serde(rename = "org_use_property_inheritance")]
    pub use_property_inheritance: Option<InheritOption>,
    /// `org-global-properties`: properties every entry inherits, such as
    /// the allowed values of one (`Effort_ALL`).
    #[serde(rename = "org_global_properties")]
    pub global_properties: Option<std::collections::BTreeMap<String, String>>,
    /// `org-clock-into-drawer`: `true` for the drawer of `org_log_into_drawer`
    /// (`LOGBOOK`), a drawer name, or `false` for none.
    #[serde(rename = "org_clock_into_drawer")]
    pub clock_into_drawer: Option<DrawerOption>,
    /// `org-clock-out-remove-zero-time-clocks`
    #[serde(rename = "org_clock_out_remove_zero_time_clocks")]
    pub clock_out_remove_zero_time_clocks: Option<bool>,
    /// `org-clock-out-when-done`: a done state stops the clock on the entry.
    #[serde(rename = "org_clock_out_when_done")]
    pub clock_out_when_done: Option<bool>,
    /// `org-clock-display-default-range`
    #[serde(rename = "org_clock_display_default_range")]
    pub clock_display_default_range: Option<String>,
    /// `org-clock-idle-time`, in minutes.
    #[serde(rename = "org_clock_idle_time")]
    pub clock_idle_time: Option<i64>,
    /// `org-clock-history-length`
    #[serde(rename = "org_clock_history_length")]
    pub clock_history_length: Option<usize>,
    /// `org-clock-clocktable-default-properties`, as the text of a plist.
    #[serde(rename = "org_clock_clocktable_default_properties")]
    pub clock_clocktable_default_properties: Option<String>,
    /// `org-clock-total-time-cell-format`
    #[serde(rename = "org_clock_total_time_cell_format")]
    pub clock_total_time_cell_format: Option<String>,
    /// `org-clock-file-time-cell-format`
    #[serde(rename = "org_clock_file_time_cell_format")]
    pub clock_file_time_cell_format: Option<String>,
    /// `org-id-link-to-org-use-id`: a boolean or the name of a choice.
    #[serde(rename = "org_id_link_to_org_use_id")]
    pub id_link_to_org_use_id: Option<serde_json::Value>,
    /// `org-link-context-for-files`
    #[serde(rename = "org_link_context_for_files")]
    pub link_context_for_files: Option<bool>,
    /// `org-id-link-use-context`
    #[serde(rename = "org_id_link_use_context")]
    pub id_link_use_context: Option<bool>,
    /// `org-id-link-consider-parent-id`
    #[serde(rename = "org_id_link_consider_parent_id")]
    pub id_link_consider_parent_id: Option<bool>,
    /// `org-link-file-path-type`
    #[serde(rename = "org_link_file_path_type")]
    pub link_file_path_type: Option<String>,
    /// `org-link-keep-stored-after-insertion`
    #[serde(rename = "org_link_keep_stored_after_insertion")]
    pub link_keep_stored_after_insertion: Option<bool>,
    /// Whether Org's linter (`org-lint`) checks the documents.
    #[serde(rename = "org_lint")]
    pub lint: Option<bool>,
    /// The `org-lint` checkers left out, by name.
    #[serde(rename = "org_lint_ignore")]
    pub lint_ignore: Option<Vec<String>>,
    /// `org-clock-in-switch-to-state`
    #[serde(rename = "org_clock_in_switch_to_state")]
    pub clock_in_switch_to_state: Option<String>,
    /// `org-clock-out-switch-to-state`
    #[serde(rename = "org_clock_out_switch_to_state")]
    pub clock_out_switch_to_state: Option<String>,
    /// `org-complete-tags-always-offer-all-agenda-tags`: completing a tag
    /// also offers those of the agenda files.
    #[serde(rename = "org_complete_tags_always_offer_all_agenda_tags")]
    pub complete_tags_always_offer_all_agenda_tags: Option<bool>,
    /// `org-cite-global-bibliography`: bibliography files for every
    /// document, after those of its `#+BIBLIOGRAPHY:` lines.
    #[serde(rename = "org_cite_global_bibliography")]
    pub cite_global_bibliography: Option<Vec<String>>,
    /// `org-link-abbrev-alist`: link abbreviations, each with what it
    /// stands for (`%s` the tag, `%h` the tag URL-encoded).
    #[serde(rename = "org_link_abbrev_alist")]
    pub link_abbrev_alist: Option<std::collections::BTreeMap<String, String>>,
    /// `org-tag-alist`: the tags offered when a file has no `#+TAGS:`
    /// line, written like one (`"work(w) { @office(o) @home(h) }"`), or as
    /// its words.
    #[serde(rename = "org_tag_alist")]
    pub tag_alist: Option<TagsOption>,
    /// `org-tag-persistent-alist`: tags offered in every file.
    #[serde(rename = "org_tag_persistent_alist")]
    pub tag_persistent_alist: Option<TagsOption>,
    /// `org-group-tags`
    #[serde(rename = "org_group_tags")]
    pub group_tags: Option<bool>,
    /// `org-use-fast-tag-selection`: `true`, `false` or `"auto"`.
    #[serde(rename = "org_use_fast_tag_selection")]
    pub use_fast_tag_selection: Option<serde_json::Value>,
    /// `org-fast-tag-selection-single-key`
    #[serde(rename = "org_fast_tag_selection_single_key")]
    pub fast_tag_selection_single_key: Option<bool>,
    /// `org-tags-sort-function`: `"string<"` or `"string>"`.
    #[serde(rename = "org_tags_sort_function")]
    pub tags_sort_function: Option<String>,
    /// `org-auto-align-tags`
    #[serde(rename = "org_auto_align_tags")]
    pub auto_align_tags: Option<bool>,
    /// `org-todo-repeat-to-state`: a keyword, or `true` for the state the
    /// entry had before it was marked done.
    #[serde(rename = "org_todo_repeat_to_state")]
    pub todo_repeat_to_state: Option<RepeatToOption>,
    /// `org-priority-highest`: a letter, or a number from 0 to 64.
    #[serde(rename = "org_priority_highest")]
    pub priority_highest: Option<PriorityOption>,
    /// `org-priority-lowest`
    #[serde(rename = "org_priority_lowest")]
    pub priority_lowest: Option<PriorityOption>,
    /// `org-priority-default`
    #[serde(rename = "org_priority_default")]
    pub priority_default: Option<PriorityOption>,
    /// `org-priority-start-cycle-with-default`
    #[serde(rename = "org_priority_start_cycle_with_default")]
    pub priority_start_cycle_with_default: Option<bool>,
    /// The step in which the minutes of a timestamp change: the second
    /// number of `org-time-stamp-rounding-minutes`.
    #[serde(rename = "org_time_stamp_rounding_minutes")]
    pub timestamp_rounding_minutes: Option<i64>,
    /// `org-adapt-indentation`: `true`, `false` or `"headline-data"`.
    #[serde(rename = "org_adapt_indentation")]
    pub adapt_indentation: Option<serde_json::Value>,
    /// `org-src-preserve-indentation`
    #[serde(rename = "org_src_preserve_indentation")]
    pub src_preserve_indentation: Option<bool>,
    /// `org-startup-indented`: show files with `org-indent-mode`.
    #[serde(rename = "org_startup_indented")]
    pub startup_indented: Option<bool>,
    /// `org-hide-leading-stars`
    #[serde(rename = "org_hide_leading_stars")]
    pub hide_leading_stars: Option<bool>,
    /// `org-edit-src-content-indentation`
    #[serde(rename = "org_edit_src_content_indentation")]
    pub edit_src_content_indentation: Option<i64>,
    /// `org-cycle-separator-lines`: how many blank lines after a subtree
    /// keep one of them visible when it is folded.
    #[serde(rename = "org_cycle_separator_lines")]
    pub cycle_separator_lines: Option<i64>,
    /// `org-startup-folded`
    #[serde(rename = "org_startup_folded")]
    pub startup_folded: Option<String>,
    /// `org-cycle-hide-block-startup`
    #[serde(rename = "org_cycle_hide_block_startup")]
    pub cycle_hide_block_startup: Option<bool>,
    /// `org-cycle-hide-drawer-startup`
    #[serde(rename = "org_cycle_hide_drawer_startup")]
    pub cycle_hide_drawer_startup: Option<bool>,
    /// `org-cycle-open-archived-trees`
    #[serde(rename = "org_cycle_open_archived_trees")]
    pub cycle_open_archived_trees: Option<bool>,
    /// `org-read-date-prefer-future`: `true`, `false` or `"time"`.
    #[serde(rename = "org_read_date_prefer_future")]
    pub read_date_prefer_future: Option<PreferFutureOption>,
    /// `org-read-date-force-compatible-dates`
    #[serde(rename = "org_read_date_force_compatible_dates")]
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
    #[serde(rename = "org_refile_targets")]
    pub refile_targets: Option<Vec<RefileTargetOption>>,
    /// `org-refile-use-outline-path`: `false`, `true`, `"file"`,
    /// `"full-file-path"` or `"title"`.
    #[serde(rename = "org_refile_use_outline_path")]
    pub refile_use_outline_path: Option<OutlinePathOption>,
    /// `org-refile-allow-creating-parent-nodes`: `false`, `true` or
    /// `"confirm"`.
    /// `org-pretty-entities`: entities (`\\alpha`) shown as their
    /// character.
    #[serde(rename = "org_pretty_entities")]
    pub pretty_entities: Option<bool>,
    #[serde(rename = "org_refile_allow_creating_parent_nodes")]
    pub refile_allow_creating_parent_nodes: Option<LogOption>,
    /// `org-refile-target-verify-function`: the name of an Emacs Lisp
    /// function, run by the server's Emacs at each target.
    #[serde(rename = "org_refile_target_verify_function")]
    pub refile_target_verify_function: Option<String>,
    /// `org-reverse-note-order`: refiled entries go first, not last.
    #[serde(rename = "org_reverse_note_order")]
    pub reverse_note_order: Option<bool>,
    /// `org-archive-location`: the file and the heading archived subtrees
    /// go to, `"%s_archive::"`.
    #[serde(rename = "org_archive_location")]
    pub archive_location: Option<String>,
    /// `org-archive-reversed-order`
    #[serde(rename = "org_archive_reversed_order")]
    pub archive_reversed_order: Option<bool>,
    /// `org-archive-mark-done`: `false`, `true` or a done keyword.
    #[serde(rename = "org_archive_mark_done")]
    pub archive_mark_done: Option<serde_json::Value>,
    /// `org-archive-subtree-add-inherited-tags`: `false`, `true` or
    /// `"infile"`.
    #[serde(rename = "org_archive_subtree_add_inherited_tags")]
    pub archive_subtree_add_inherited_tags: Option<serde_json::Value>,
    /// `org-archive-save-context-info`
    #[serde(rename = "org_archive_save_context_info")]
    pub archive_save_context_info: Option<Vec<String>>,
    /// `org-archive-sibling-heading`
    #[serde(rename = "org_archive_sibling_heading")]
    pub archive_sibling_heading: Option<String>,
    /// `org-archive-file-header-format`: a format with `%s` for the file,
    /// or `false` for none.
    #[serde(rename = "org_archive_file_header_format")]
    pub archive_file_header_format: Option<serde_json::Value>,
    /// `org-archive-default-command`: `"subtree"`, `"sibling"` or `"tag"`.
    #[serde(rename = "org_archive_default_command")]
    pub archive_default_command: Option<String>,
    /// `org-capture-templates`. Without it, Org's default: a TODO under
    /// "Tasks" in `org_default_notes_file`.
    #[serde(rename = "org_capture_templates")]
    pub capture_templates: Option<Vec<CaptureTemplateOption>>,
    /// `org-capture-templates-contexts`: where templates are offered.
    #[serde(rename = "org_capture_templates_contexts")]
    pub capture_templates_contexts: Option<Vec<ContextOption>>,
    /// `org-directory`: where relative capture targets are. `~/org`.
    pub org_directory: Option<String>,
    /// `org-default-notes-file`: the target of a template whose file is
    /// empty. `~/.notes`.
    #[serde(rename = "org_default_notes_file")]
    pub default_notes_file: Option<String>,
    /// A fixed date and time, for tests.
    pub clock: Option<ClockOption>,
    /// `org-link-descriptive`: links show their description; the rest is
    /// marked hidden.
    #[serde(rename = "org_link_descriptive")]
    pub link_descriptive: Option<bool>,
    /// `org-hide-emphasis-markers`
    #[serde(rename = "org_hide_emphasis_markers")]
    pub hide_emphasis_markers: Option<bool>,
    /// Save the files a command changes other than the one it was run in.
    pub auto_save: Option<bool>,
    /// RET on the first line of a list item makes the next item, as M-RET.
    pub return_uses_meta_return: Option<bool>,
    /// `org-goto-max-level`
    #[serde(rename = "org_goto_max_level")]
    pub goto_max_level: Option<usize>,
    /// How many months the date prompt's calendar shows: 3, or 1.
    pub calendar_months: Option<usize>,
    /// The Emacs program for what only Org itself does (export).
    pub emacs: Option<String>,
    /// A file of Emacs Lisp that Emacs loads before Org.
    pub emacs_config_file: Option<String>,
    /// Emacs Lisp that Emacs evaluates before loading Org.
    pub emacs_config: Option<String>,
    /// The shared configuration file: `false` for none, or a path instead
    /// of the usual one. Read by `load`, before the options are.
    pub config_file: Option<serde_json::Value>,
}

/// An entry of `org-refile-targets`: the files, `"current"` (the entry's
/// own, the default), `"agenda"`, a path or a list of paths, and one of
/// `level`, `maxlevel`, `tag`, `todo`, `regexp` (the `regex` crate's
/// syntax) or `all`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RefileTargetOption {
    pub files: Option<RefileFilesOption>,
    pub level: Option<usize>,
    #[serde(rename = "maxlevel")]
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
#[serde(default)]
pub struct CaptureTemplateOption {
    pub key: String,
    pub description: String,
    /// `"entry"` (the default), `"plain"`, `"item"`, `"checkitem"` or
    /// `"table-line"`.
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
    /// `:tree-type` of a `file+olp+datetree` target: `"day"`, `"week"`,
    /// `"month"`, or the levels (`["year", "quarter", "month"]`).
    pub tree_type: Option<TreeTypeOption>,
    /// `:time-prompt`: ask for the date of the tree entry, which then
    /// stands for the time of the capture.
    pub time_prompt: Option<bool>,
    /// `:table-line-pos`: where a table line goes, `"II-1"`.
    pub table_line_pos: Option<String>,
    /// `:clock-in`: the captured entry is clocked while the capture goes on.
    pub clock_in: Option<bool>,
    /// `:clock-keep`: the clock keeps running when the capture is filed.
    pub clock_keep: Option<bool>,
    /// `:clock-resume`: the clock the capture stopped starts again.
    pub clock_resume: Option<bool>,
    /// `:jump-to-captured`: the target shows the entry when it is filed.
    pub jump_to_captured: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TreeTypeOption {
    Name(String),
    Levels(Vec<String>),
}

impl TreeTypeOption {
    pub fn names(&self) -> Vec<String> {
        match self {
            TreeTypeOption::Name(name) => vec![name.clone()],
            TreeTypeOption::Levels(levels) => levels.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TemplateText {
    Text(String),
    Lines(Vec<String>),
}

impl CaptureTemplateOption {
    /// A key and a description only: the templates whose keys start with
    /// its key.
    pub fn is_group(&self) -> bool {
        self.template.is_none() && self.target.is_empty() && self.kind.is_none()
    }
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
#[serde(default)]
pub struct InheritanceOptions {
    #[serde(rename = "org_use_tag_inheritance")]
    pub use_tag_inheritance: Option<InheritOption>,
    #[serde(rename = "org_tags_exclude_from_inheritance")]
    pub tags_exclude_from_inheritance: Option<Vec<String>>,
    #[serde(rename = "org_use_property_inheritance")]
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

/// One context of `org-capture-templates-contexts` or
/// `org-agenda-custom-commands-contexts`: the key it is about, the key it
/// stands for where it holds (`use`, the key itself without it) and its
/// rules, each one of `in_file`, `in_mode`, `in_buffer`, `not_in_file`,
/// `not_in_mode` or `not_in_buffer` with a regexp.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextOption {
    pub key: String,
    #[serde(rename = "use", default, skip_serializing_if = "Option::is_none")]
    pub use_key: Option<String>,
    pub contexts: Vec<std::collections::BTreeMap<String, String>>,
}

/// The contexts of `options`, rules that cannot be read left out.
pub fn contexts(options: Option<&[ContextOption]>) -> Vec<org_core::contexts::Context> {
    use org_core::contexts::{Context, Kind, Rule};
    options
        .unwrap_or_default()
        .iter()
        .map(|option| Context {
            key: option.key.clone(),
            use_key: option.use_key.clone().filter(|key| !key.is_empty()).unwrap_or_else(|| option.key.clone()),
            rules: option
                .contexts
                .iter()
                .flatten()
                .filter_map(|(kind, pattern)| Rule::new(Kind::named(kind)?, pattern).ok())
                .collect(),
        })
        .collect()
}

/// A tag table: a `#+TAGS:` line, or its words.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TagsOption {
    Line(String),
    Words(Vec<String>),
}

impl TagsOption {
    fn to_alist(&self) -> Vec<org_core::tags::TagEntry> {
        match self {
            TagsOption::Line(line) => org_core::tags::tag_string_to_alist(line),
            TagsOption::Words(words) => org_core::tags::tag_string_to_alist(&words.join(" ")),
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

    /// The default TODO sequence: `org_todo_keywords`, or Org's `TODO | DONE`.
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

    /// How files show when they are opened.
    pub fn startup(&self) -> crate::folding::Startup {
        let default = crate::folding::Startup::default();
        crate::folding::Startup {
            folded: self.startup_folded.clone().unwrap_or(default.folded),
            hide_blocks: self.cycle_hide_block_startup.unwrap_or(default.hide_blocks),
            hide_drawers: self.cycle_hide_drawer_startup.unwrap_or(default.hide_drawers),
            open_archived: self.cycle_open_archived_trees.unwrap_or(default.open_archived),
        }
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
    pub fn refile_new_nodes(&self) -> org_core::refile::NewNodes {
        use org_core::refile::NewNodes;
        match &self.refile_allow_creating_parent_nodes {
            None | Some(LogOption::Flag(false)) => NewNodes::No,
            Some(LogOption::Name(name)) if name == "confirm" => NewNodes::Confirm,
            Some(_) => NewNodes::Yes,
        }
    }

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
        let tags = self.tag_settings();
        AgendaConfig {
            priorities: self.priorities(),
            tag_alist: tags.alist,
            tag_persistent_alist: tags.persistent,
            group_tags: self.group_tags.unwrap_or(true),
            ..self.agenda.to_config()
        }
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
    /// The options of the link commands.
    pub fn link_settings(&self) -> org_core::link::Settings {
        use org_core::link::{PathType, UseId};
        let defaults = org_core::link::Settings::default();
        org_core::link::Settings {
            use_id: match &self.id_link_to_org_use_id {
                Some(serde_json::Value::Bool(true)) => UseId::Always,
                Some(serde_json::Value::String(choice)) => match choice.as_str() {
                    "t" => UseId::Always,
                    "create-if-interactive" => UseId::CreateIfInteractive,
                    "create-if-interactive-and-no-custom-id" => UseId::CreateIfInteractiveAndNoCustomId,
                    "use-existing" => UseId::UseExisting,
                    _ => UseId::Never,
                },
                _ => UseId::Never,
            },
            context_for_files: self.link_context_for_files.unwrap_or(defaults.context_for_files),
            id_use_context: self.id_link_use_context.unwrap_or(defaults.id_use_context),
            consider_parent_id: self.id_link_consider_parent_id.unwrap_or(defaults.consider_parent_id),
            path_type: match self.link_file_path_type.as_deref() {
                Some("absolute") => PathType::Absolute,
                Some("noabbrev") => PathType::NoAbbrev,
                Some("relative") => PathType::Relative,
                _ => PathType::Adaptive,
            },
            keep_stored: self.link_keep_stored_after_insertion.unwrap_or(defaults.keep_stored),
        }
    }

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
    /// How clock tables are written; the clock, the keywords and the
    /// inheritance come from the server.
    pub fn clocktable_settings(&self) -> org_core::clocktable::Settings {
        let defaults = org_core::clocktable::Settings::default();
        org_core::clocktable::Settings {
            default_properties: self
                .clock_clocktable_default_properties
                .clone()
                .unwrap_or(defaults.default_properties.clone()),
            total_time_cell_format: self
                .clock_total_time_cell_format
                .clone()
                .unwrap_or(defaults.total_time_cell_format.clone()),
            file_time_cell_format: self
                .clock_file_time_cell_format
                .clone()
                .unwrap_or(defaults.file_time_cell_format.clone()),
            ..defaults
        }
    }

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
            tags: self.tag_settings(),
            global_properties: self.global_properties.clone().unwrap_or_default().into_iter().collect(),
            list_circular_motion: self.list_use_circular_motion.unwrap_or(false),
        }
    }

    /// The tag table and how tags are set (`org-tag-alist` and its
    /// relatives).
    pub fn tag_settings(&self) -> org_core::tags::Settings {
        use org_core::tags::{Fast, Settings, Sort};
        let defaults = Settings::default();
        Settings {
            alist: self.tag_alist.as_ref().map(TagsOption::to_alist).unwrap_or_default(),
            persistent: self.tag_persistent_alist.as_ref().map(TagsOption::to_alist).unwrap_or_default(),
            fast: match &self.use_fast_tag_selection {
                Some(serde_json::Value::Bool(true)) => Fast::Always,
                Some(serde_json::Value::Bool(false)) => Fast::Never,
                _ => Fast::Auto,
            },
            single_key: self.fast_tag_selection_single_key.unwrap_or(defaults.single_key),
            sort: match self.tags_sort_function.as_deref() {
                Some("string<") => Some(Sort::Ascending),
                Some("string>") => Some(Sort::Descending),
                _ => None,
            },
            auto_align: self.auto_align_tags.unwrap_or(defaults.auto_align),
            tags_column: self.tags_column.unwrap_or(defaults.tags_column),
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
        settings.enforce_todo_dependencies = self.enforce_todo_dependencies.unwrap_or(false);
        settings.enforce_todo_checkbox_dependencies = self.enforce_todo_checkbox_dependencies.unwrap_or(false);
        if let Some(option) = &self.log_refile {
            settings.log_refile = option.to_logging();
        }
        if let Some(option) = &self.log_reschedule {
            settings.log_reschedule = option.to_logging();
        }
        if let Some(option) = &self.log_redeadline {
            settings.log_redeadline = option.to_logging();
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
/// file's. The editor's `config_file` says which file to read: `false` for
/// none, a path, or by default `default_config_file`.
pub fn load(editor: Option<serde_json::Value>, var: &dyn Fn(&str) -> Option<String>) -> Loaded {
    let mut problems = Vec::new();
    let editor = match editor {
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
    let mut renamed = Vec::new();
    let mut editor = match current_names(serde_json::Value::Object(editor), &mut renamed) {
        serde_json::Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    let (file, named) = match editor.remove("config_file") {
        Some(serde_json::Value::Bool(false)) => (None, false),
        Some(serde_json::Value::String(path)) if !path.is_empty() => (Some(expand_home(&path, var)), true),
        None | Some(serde_json::Value::Bool(true)) => usual(&mut problems),
        Some(_) => {
            problems.push("the option `config_file` is malformed and is ignored".to_string());
            usual(&mut problems)
        }
    };
    let mut merged = serde_json::json!({});
    let mut read = false;
    if let Some(path) = &file {
        match std::fs::read_to_string(path) {
            Ok(text) => match parse_config(path, &text) {
                Ok(options) => {
                    merged = current_names(options, &mut renamed);
                    read = true;
                }
                Err(error) => problems.push(format!("{} {error}; it is ignored", path.display())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !named => {}
            Err(error) => problems.push(format!("cannot read {}: {error}", path.display())),
        }
    }
    merge(&mut merged, serde_json::Value::Object(editor));
    if !renamed.is_empty() {
        renamed.dedup();
        problems.push(format!("options are named after Org's variables now: {}", renamed.join(", ")));
    }
    let unknown: Vec<String> = merged
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key)
        .filter(|key| *key != "clock" && !crate::settings::SETTINGS.iter().any(|setting| setting.name == key.as_str()))
        .map(|key| format!("`{key}`"))
        .collect();
    if !unknown.is_empty() {
        problems.push(format!("unknown options, which are ignored: {}", unknown.join(", ")));
    }
    let (options, dropped) = Options::read(merged.clone());
    problems.extend(dropped.into_iter().map(|name| format!("the option `{name}` is malformed and is ignored")));
    Loaded { options, file, read, merged, problems }
}

// ----- names of before

/// The options' names before they were Org's: the old name and the new.
const OLD_NAMES: &[(&str, &str)] = &[
    ("configFile", "config_file"),
    ("agendaFiles", "org_agenda_files"),
    ("todoKeywords", "org_todo_keywords"),
    ("logDone", "org_log_done"),
    ("logRepeat", "org_log_repeat"),
    ("logIntoDrawer", "org_log_into_drawer"),
    ("tagsColumn", "org_tags_column"),
    ("useTagInheritance", "org_use_tag_inheritance"),
    ("tagsExcludeFromInheritance", "org_tags_exclude_from_inheritance"),
    ("usePropertyInheritance", "org_use_property_inheritance"),
    ("clockIntoDrawer", "org_clock_into_drawer"),
    ("clockOutRemoveZeroTimeClocks", "org_clock_out_remove_zero_time_clocks"),
    ("clockOutWhenDone", "org_clock_out_when_done"),
    ("completeTagsAlwaysOfferAllAgendaTags", "org_complete_tags_always_offer_all_agenda_tags"),
    ("todoRepeatToState", "org_todo_repeat_to_state"),
    ("priorityHighest", "org_priority_highest"),
    ("priorityLowest", "org_priority_lowest"),
    ("priorityDefault", "org_priority_default"),
    ("priorityStartCycleWithDefault", "org_priority_start_cycle_with_default"),
    ("timestampRoundingMinutes", "org_time_stamp_rounding_minutes"),
    ("readDatePreferFuture", "org_read_date_prefer_future"),
    ("readDateForceCompatibleDates", "org_read_date_force_compatible_dates"),
    ("refileTargets", "org_refile_targets"),
    ("refileUseOutlinePath", "org_refile_use_outline_path"),
    ("reverseNoteOrder", "org_reverse_note_order"),
    ("archiveLocation", "org_archive_location"),
    ("archiveReversedOrder", "org_archive_reversed_order"),
    ("archiveMarkDone", "org_archive_mark_done"),
    ("archiveSubtreeAddInheritedTags", "org_archive_subtree_add_inherited_tags"),
    ("archiveSaveContextInfo", "org_archive_save_context_info"),
    ("archiveSiblingHeading", "org_archive_sibling_heading"),
    ("archiveFileHeaderFormat", "org_archive_file_header_format"),
    ("archiveDefaultCommand", "org_archive_default_command"),
    ("captureTemplates", "org_capture_templates"),
    ("orgDirectory", "org_directory"),
    ("defaultNotesFile", "org_default_notes_file"),
    ("linkDescriptive", "org_link_descriptive"),
    ("hideEmphasisMarkers", "org_hide_emphasis_markers"),
    ("returnUsesMetaReturn", "return_uses_meta_return"),
    ("autoSave", "auto_save"),
    ("cycleSeparatorLines", "org_cycle_separator_lines"),
    ("startupFolded", "org_startup_folded"),
    ("cycleHideBlockStartup", "org_cycle_hide_block_startup"),
    ("cycleHideDrawerStartup", "org_cycle_hide_drawer_startup"),
    ("cycleOpenArchivedTrees", "org_cycle_open_archived_trees"),
    ("adaptIndentation", "org_adapt_indentation"),
    ("startupIndented", "org_startup_indented"),
    ("hideLeadingStars", "org_hide_leading_stars"),
    ("srcPreserveIndentation", "org_src_preserve_indentation"),
    ("editSrcContentIndentation", "org_edit_src_content_indentation"),
    ("virtualDocuments", "virtual_documents"),
    ("showDocument", "show_document"),
    ("messagePrompts", "message_prompts"),
    ("lineHighlights", "line_highlights"),
    ("virtualIndent", "virtual_indent"),
    ("commandActions", "command_actions"),
    ("commandScript", "command_script"),
];

/// The options of `agenda` before they were Org's, by their old names.
const OLD_AGENDA_NAMES: &[(&str, &str)] = &[
    ("span", "org_agenda_span"),
    ("startOnWeekday", "org_agenda_start_on_weekday"),
    ("deadlineWarningDays", "org_deadline_warning_days"),
    ("scheduledDelayDays", "org_scheduled_delay_days"),
    ("scheduledPastDays", "org_scheduled_past_days"),
    ("deadlinePastDays", "org_deadline_past_days"),
    ("skipScheduledIfDone", "org_agenda_skip_scheduled_if_done"),
    ("skipDeadlineIfDone", "org_agenda_skip_deadline_if_done"),
    ("skipTimestampIfDone", "org_agenda_skip_timestamp_if_done"),
    ("showFutureRepeats", "org_agenda_show_future_repeats"),
    ("showAllDates", "org_agenda_show_all_dates"),
    ("tagsColumn", "org_agenda_tags_column"),
    ("useTimeGrid", "org_agenda_use_time_grid"),
    ("showCurrentTimeInGrid", "org_agenda_show_current_time_in_grid"),
    ("weekendDays", "org_agenda_weekend_days"),
    ("startWithLogMode", "org_agenda_start_with_log_mode"),
    ("startWithClockreportMode", "org_agenda_start_with_clockreport_mode"),
    ("startWithArchivesMode", "org_agenda_start_with_archives_mode"),
    ("logModeItems", "org_agenda_log_mode_items"),
    ("blockSeparator", "org_agenda_block_separator"),
    ("compactBlocks", "org_agenda_compact_blocks"),
    ("hideEmptyBlocks", "agenda_hide_empty_blocks"),
    ("prefixFormat", "org_agenda_prefix_format"),
    ("todoKeywordFormat", "org_agenda_todo_keyword_format"),
    ("scheduledLeaders", "org_agenda_scheduled_leaders"),
    ("deadlineLeaders", "org_agenda_deadline_leaders"),
    ("timerangeLeaders", "org_agenda_timerange_leaders"),
    ("removeTags", "org_agenda_remove_tags"),
    ("breadcrumbsSeparator", "org_agenda_breadcrumbs_separator"),
    ("formatDate", "org_agenda_format_date"),
    ("startDay", "org_agenda_start_day"),
    ("customCommands", "org_agenda_custom_commands"),
];

/// The keys of an object as Org writes them, with `-` (`org-agenda-files`,
/// `:empty-lines`), as the options name them, with `_`.
fn underscored(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(fields) = value {
        *fields = std::mem::take(fields).into_iter().map(|(key, value)| (key.replace('-', "_"), value)).collect();
    }
}

/// The options in `value` under the names they have now: Org's spelling
/// with `-` read as `_`, and the names options had before Org's, which
/// are read and named in `renamed`.
pub fn current_names(value: serde_json::Value, renamed: &mut Vec<String>) -> serde_json::Value {
    let serde_json::Value::Object(fields) = value else { return value };
    let mut out = serde_json::Map::new();
    for (key, mut field) in fields {
        let key = key.replace('-', "_");
        if key == "agenda" && field.is_object() {
            let serde_json::Value::Object(agenda) = field else { unreachable!() };
            for (key, field) in agenda {
                if let Some((name, field)) = old_agenda_option(&key, field) {
                    renamed.push(format!("agenda.{key} → {name}"));
                    out.insert(name, field);
                }
            }
            continue;
        }
        let name = match OLD_NAMES.iter().find(|(old, _)| *old == key) {
            Some((old, new)) => {
                renamed.push(format!("{old} → {new}"));
                new.to_string()
            }
            None => key,
        };
        match name.as_str() {
            "org_capture_templates" | "org_refile_targets" => {
                for item in field.as_array_mut().into_iter().flatten() {
                    underscored(item);
                    if let Some(fields) = item.as_object_mut() {
                        for (old, new) in [
                            ("emptyLines", "empty_lines"),
                            ("emptyLinesBefore", "empty_lines_before"),
                            ("emptyLinesAfter", "empty_lines_after"),
                            ("immediateFinish", "immediate_finish"),
                            ("treeType", "tree_type"),
                            ("timePrompt", "time_prompt"),
                            ("tableLinePos", "table_line_pos"),
                            ("clockIn", "clock_in"),
                            ("clockKeep", "clock_keep"),
                            ("clockResume", "clock_resume"),
                            ("jumpToCaptured", "jump_to_captured"),
                            ("maxLevel", "maxlevel"),
                        ] {
                            if let Some(value) = fields.remove(old) {
                                renamed.push(format!("{old} → {new}"));
                                fields.insert(new.to_string(), value);
                            }
                        }
                    }
                }
            }
            "org_agenda_custom_commands" => {
                for command in field.as_array_mut().into_iter().flatten() {
                    current_settings(command, renamed);
                    for block in
                        command.get_mut("blocks").and_then(|blocks| blocks.as_array_mut()).into_iter().flatten()
                    {
                        current_settings(block, renamed);
                    }
                }
            }
            _ => {}
        }
        out.insert(name, field);
    }
    serde_json::Value::Object(out)
}

/// The `settings` of a custom agenda command or block under the names
/// they have now.
fn current_settings(holder: &mut serde_json::Value, renamed: &mut Vec<String>) {
    let Some(settings) = holder.get_mut("settings") else { return };
    let serde_json::Value::Object(fields) = std::mem::take(settings) else { return };
    let mut out = serde_json::Map::new();
    for (key, field) in fields {
        let key = key.replace('-', "_");
        match old_agenda_option(&key, field.clone()) {
            Some((name, field)) => {
                renamed.push(format!("{key} → {name}"));
                out.insert(name, field);
            }
            None => {
                out.insert(key, field);
            }
        }
    }
    *settings = serde_json::Value::Object(out);
}

/// An option of `agenda` as it was named before, under its name now.
/// `org_agenda_time_grid` was an object; `org_agenda_time_grid` is Org's list, and
/// its `now` is `org_agenda_current_time_string`.
fn old_agenda_option(key: &str, field: serde_json::Value) -> Option<(String, serde_json::Value)> {
    if key == "timeGrid" {
        let grid = TimeGrid::default();
        let flag = |name: &str, default: bool| field.get(name).and_then(serde_json::Value::as_bool).unwrap_or(default);
        let kinds: Vec<&str> = [
            ("daily", flag("daily", grid.daily)),
            ("weekly", flag("weekly", grid.weekly)),
            ("today", flag("today", grid.today)),
            ("require-timed", flag("requireTimed", grid.require_timed)),
            ("remove-match", flag("removeMatch", grid.remove_match)),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(kind, _)| kind)
        .collect();
        let times = field.get("times").cloned().unwrap_or_else(|| serde_json::json!(grid.times));
        let trailer = field.get("trailer").cloned().unwrap_or_else(|| serde_json::json!(grid.trailer));
        let line = field.get("line").cloned().unwrap_or_else(|| serde_json::json!(grid.line));
        return Some(("org_agenda_time_grid".to_string(), serde_json::json!([kinds, times, trailer, line])));
    }
    let (_, new) = OLD_AGENDA_NAMES.iter().find(|(old, _)| *old == key)?;
    Some((new.to_string(), field))
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
    /// How many seconds a minute of Emacs's timers (appointments) lasts.
    #[serde(default)]
    pub minute: Option<f64>,
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

/// `org-agenda-time-grid`, as Org writes it: the kinds of days that have
/// the grid (`daily`, `weekly`, `today`, `require-timed`,
/// `remove-match`), the times of its lines (`800` for 8:00), what follows
/// a time without an end, and the text of a line:
/// `[["daily", "today", "require-timed"], [800, 1000], "......", "----"]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "Vec<serde_json::Value>", into = "Vec<serde_json::Value>")]
pub struct TimeGridOption {
    pub kinds: Vec<String>,
    pub times: Vec<i64>,
    pub trailer: String,
    pub line: String,
}

impl TryFrom<Vec<serde_json::Value>> for TimeGridOption {
    type Error = String;

    fn try_from(items: Vec<serde_json::Value>) -> Result<Self, String> {
        let grid = TimeGrid::default();
        let strings = |value: &serde_json::Value| -> Option<Vec<String>> {
            value.as_array()?.iter().map(|item| item.as_str().map(str::to_string)).collect()
        };
        let kinds = items.first().map(|value| strings(value).ok_or("the kinds of days are not a list of names"));
        let times = items.get(1).map(|value| {
            value
                .as_array()
                .and_then(|times| times.iter().map(serde_json::Value::as_i64).collect::<Option<Vec<_>>>())
                .ok_or("the times are not a list of numbers")
        });
        let text = |index: usize, default: &str| -> Result<String, String> {
            match items.get(index) {
                None => Ok(default.to_string()),
                Some(value) => value.as_str().map(str::to_string).ok_or_else(|| format!("item {index} is not a text")),
            }
        };
        let default_kinds: Vec<String> = ["daily", "today", "require-timed"].map(String::from).to_vec();
        Ok(TimeGridOption {
            kinds: kinds.transpose()?.unwrap_or(default_kinds),
            times: times.transpose()?.unwrap_or(grid.times),
            trailer: text(2, &grid.trailer)?,
            line: text(3, &grid.line)?,
        })
    }
}

impl From<TimeGridOption> for Vec<serde_json::Value> {
    fn from(grid: TimeGridOption) -> Self {
        vec![
            serde_json::json!(grid.kinds),
            serde_json::json!(grid.times),
            serde_json::json!(grid.trailer),
            serde_json::json!(grid.line),
        ]
    }
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
#[serde(default)]
pub struct AgendaOptions {
    #[serde(rename = "org_agenda_span")]
    pub span: Option<SpanOption>,
    /// 0 is Sunday; `false` starts week views on the current day.
    #[serde(rename = "org_agenda_start_on_weekday")]
    pub start_on_weekday: Option<serde_json::Value>,
    #[serde(rename = "org_deadline_warning_days")]
    pub deadline_warning_days: Option<i64>,
    #[serde(rename = "org_scheduled_delay_days")]
    pub scheduled_delay_days: Option<i64>,
    #[serde(rename = "org_scheduled_past_days")]
    pub scheduled_past_days: Option<i64>,
    /// `org-modules`: `org-habit` turns habits on.
    #[serde(rename = "org_modules")]
    pub modules: Option<Vec<String>>,
    #[serde(rename = "org_habit_graph_column")]
    pub habit_graph_column: Option<usize>,
    #[serde(rename = "org_habit_preceding_days")]
    pub habit_preceding_days: Option<i64>,
    #[serde(rename = "org_habit_following_days")]
    pub habit_following_days: Option<i64>,
    #[serde(rename = "org_habit_show_habits")]
    pub habit_show_habits: Option<bool>,
    #[serde(rename = "org_habit_show_habits_only_for_today")]
    pub habit_show_habits_only_for_today: Option<bool>,
    #[serde(rename = "org_habit_show_all_today")]
    pub habit_show_all_today: Option<bool>,
    #[serde(rename = "org_habit_today_glyph")]
    pub habit_today_glyph: Option<String>,
    #[serde(rename = "org_habit_completed_glyph")]
    pub habit_completed_glyph: Option<String>,
    #[serde(rename = "org_habit_show_done_always_green")]
    pub habit_show_done_always_green: Option<bool>,
    #[serde(rename = "org_habit_scheduled_past_days")]
    pub habit_scheduled_past_days: Option<i64>,
    #[serde(rename = "org_deadline_past_days")]
    pub deadline_past_days: Option<i64>,
    #[serde(rename = "org_agenda_skip_scheduled_if_done")]
    pub skip_scheduled_if_done: Option<bool>,
    #[serde(rename = "org_agenda_skip_deadline_if_done")]
    pub skip_deadline_if_done: Option<bool>,
    #[serde(rename = "org_agenda_skip_timestamp_if_done")]
    pub skip_timestamp_if_done: Option<bool>,
    #[serde(rename = "org_agenda_show_future_repeats")]
    pub show_future_repeats: Option<FutureRepeatsOption>,
    #[serde(rename = "org_agenda_show_all_dates")]
    pub show_all_dates: Option<bool>,
    /// `org-agenda-tags-column`: a column, or `"auto"`.
    #[serde(rename = "org_agenda_tags_column")]
    pub tags_column: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_use_time_grid")]
    pub use_time_grid: Option<bool>,
    #[serde(rename = "org_agenda_time_grid")]
    pub time_grid: Option<TimeGridOption>,
    /// `org-agenda-current-time-string`
    #[serde(rename = "org_agenda_current_time_string")]
    pub current_time_string: Option<String>,
    #[serde(rename = "org_agenda_show_current_time_in_grid")]
    pub show_current_time_in_grid: Option<bool>,
    #[serde(rename = "org_agenda_weekend_days")]
    pub weekend_days: Option<Vec<usize>>,
    /// `org-agenda-start-with-log-mode`
    #[serde(rename = "org_agenda_start_with_log_mode")]
    pub start_with_log_mode: Option<bool>,
    /// `org-agenda-start-with-clockreport-mode`
    #[serde(rename = "org_agenda_start_with_clockreport_mode")]
    pub start_with_clockreport_mode: Option<bool>,
    /// `org-agenda-start-with-archives-mode`: `"trees"` or `"files"`.
    #[serde(rename = "org_agenda_start_with_archives_mode")]
    pub start_with_archives_mode: Option<ArchivesOption>,
    /// `org-agenda-persistent-marks`
    #[serde(rename = "org_agenda_persistent_marks")]
    pub persistent_marks: Option<bool>,
    /// `org-agenda-start-with-follow-mode`
    #[serde(rename = "org_agenda_start_with_follow_mode")]
    pub start_with_follow_mode: Option<bool>,
    /// `org-agenda-start-with-entry-text-mode`
    #[serde(rename = "org_agenda_start_with_entry_text_mode")]
    pub start_with_entry_text_mode: Option<bool>,
    /// `org-agenda-entry-text-maxlines`
    #[serde(rename = "org_agenda_entry_text_maxlines")]
    pub entry_text_maxlines: Option<usize>,
    /// `org-agenda-entry-text-leaders`
    #[serde(rename = "org_agenda_entry_text_leaders")]
    pub entry_text_leaders: Option<String>,
    /// `org-agenda-entry-text-exclude-regexps`
    #[serde(rename = "org_agenda_entry_text_exclude_regexps")]
    pub entry_text_exclude_regexps: Option<Vec<String>>,
    /// `org-agenda-log-mode-items`: `closed`, `clock` and `state`.
    #[serde(rename = "org_agenda_log_mode_items")]
    pub log_mode_items: Option<Vec<String>>,
    /// `org-agenda-start-day`: the day the agenda starts from, as the date
    /// prompt reads it (`-1d`, `2026-10-01`).
    #[serde(rename = "org_agenda_start_day")]
    pub start_day: Option<String>,
    /// `org-agenda-block-separator`: a character to repeat, a text, or
    /// `false`.
    #[serde(rename = "org_agenda_block_separator")]
    pub block_separator: Option<serde_json::Value>,
    /// `org-agenda-compact-blocks`
    #[serde(rename = "org_agenda_compact_blocks")]
    pub compact_blocks: Option<bool>,
    /// Leave out the empty blocks of a block agenda.
    #[serde(rename = "agenda_hide_empty_blocks")]
    pub hide_empty_blocks: Option<bool>,
    /// `org-agenda-search-view-always-boolean`
    #[serde(rename = "org_agenda_search_view_always_boolean")]
    pub search_view_always_boolean: Option<bool>,
    /// `org-agenda-search-view-force-full-words`
    #[serde(rename = "org_agenda_search_view_force_full_words")]
    pub search_view_force_full_words: Option<bool>,
    /// `org-agenda-search-view-max-outline-level`
    #[serde(rename = "org_agenda_search_view_max_outline_level")]
    pub search_view_max_outline_level: Option<usize>,
    /// `org-agenda-text-search-extra-files`: files the search view
    /// searches besides the agenda files; `agenda-archives` first adds
    /// the files those archive to.
    #[serde(rename = "org_agenda_text_search_extra_files")]
    pub text_search_extra_files: Option<Vec<String>>,
    /// `org-agenda-custom-commands`
    #[serde(rename = "org_agenda_custom_commands")]
    pub custom_commands: Option<Vec<CustomCommandOption>>,
    /// `org-agenda-custom-commands-contexts`: where commands are offered.
    #[serde(rename = "org_agenda_custom_commands_contexts")]
    pub custom_commands_contexts: Option<Vec<ContextOption>>,
    /// `org-agenda-prefix-format`
    #[serde(rename = "org_agenda_prefix_format")]
    pub prefix_format: PrefixFormatOptions,
    /// `org-agenda-todo-keyword-format`
    #[serde(rename = "org_agenda_todo_keyword_format")]
    pub todo_keyword_format: Option<String>,
    /// `org-agenda-scheduled-leaders`
    #[serde(rename = "org_agenda_scheduled_leaders")]
    pub scheduled_leaders: Option<Vec<String>>,
    /// `org-agenda-deadline-leaders`
    #[serde(rename = "org_agenda_deadline_leaders")]
    pub deadline_leaders: Option<Vec<String>>,
    /// `org-agenda-timerange-leaders`
    #[serde(rename = "org_agenda_timerange_leaders")]
    pub timerange_leaders: Option<Vec<String>>,
    /// `org-agenda-remove-tags`: `true`, `false` or `"prefix"`.
    #[serde(rename = "org_agenda_remove_tags")]
    pub remove_tags: Option<serde_json::Value>,
    /// `org-agenda-breadcrumbs-separator`
    #[serde(rename = "org_agenda_breadcrumbs_separator")]
    pub breadcrumbs_separator: Option<String>,
    /// `org-agenda-format-date`, as a `format-time-string` format.
    #[serde(rename = "org_agenda_format_date")]
    pub format_date: Option<String>,
    /// `org-stuck-projects`: the match string of projects, the TODO
    /// keywords and the tags of next actions, and a regexp.
    #[serde(rename = "org_stuck_projects")]
    pub stuck_projects: Option<(String, Vec<String>, Vec<String>, String)>,
}

/// A command of `org-agenda-custom-commands`: one view (`type`, `match`),
/// a block agenda (`blocks`), or, with neither, a prefix key that groups
/// the commands whose keys start with it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
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
    /// `org_agenda_start_day`, `org_deadline_warning_days`…
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<CustomBlockOption>>,
}

/// One block of a block agenda.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
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

/// `org_agenda_start_with_archives_mode`: `"trees"`, `"files"` or `false`.
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
        if let Some(on) = self.search_view_always_boolean {
            config.search.always_boolean = on;
        }
        // `org-habit`, when it is one of `org-modules`.
        if self.modules.iter().flatten().any(|module| module == "org-habit") {
            let mut habits = org_core::agenda::habit::Settings::default();
            if let Some(column) = self.habit_graph_column {
                habits.graph_column = column;
            }
            if let Some(days) = self.habit_preceding_days {
                habits.preceding_days = days.max(0);
            }
            if let Some(days) = self.habit_following_days {
                habits.following_days = days.max(0);
            }
            if let Some(on) = self.habit_show_habits {
                habits.show_habits = on;
            }
            if let Some(on) = self.habit_show_habits_only_for_today {
                habits.show_habits_only_for_today = on;
            }
            if let Some(on) = self.habit_show_all_today {
                habits.show_all_today = on;
            }
            if let Some(glyph) = self.habit_today_glyph.as_deref().and_then(|glyph| glyph.chars().next()) {
                habits.today_glyph = glyph;
            }
            if let Some(glyph) = self.habit_completed_glyph.as_deref().and_then(|glyph| glyph.chars().next()) {
                habits.completed_glyph = glyph;
            }
            if let Some(on) = self.habit_show_done_always_green {
                habits.show_done_always_green = on;
            }
            habits.scheduled_past_days = self.habit_scheduled_past_days;
            config.habits = Some(habits);
        }
        if let Some(on) = self.search_view_force_full_words {
            config.search.full_words = on;
        }
        if let Some(level) = self.search_view_max_outline_level {
            config.search.max_level = level;
        }
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
        let entry_text = &mut config.entry_text_settings;
        if let Some(lines) = self.entry_text_maxlines.filter(|lines| *lines > 0) {
            entry_text.max_lines = lines;
        }
        if let Some(leaders) = &self.entry_text_leaders {
            entry_text.leaders = leaders.clone();
        }
        if let Some(exclude) = &self.entry_text_exclude_regexps {
            entry_text.exclude = exclude.clone();
        }
        config.entry_text = self.start_with_entry_text_mode.unwrap_or(false).then_some(entry_text.max_lines);
        if let Some(items) = &self.log_mode_items {
            let has = |name: &str| items.iter().any(|item| item == name);
            config.log_items =
                org_core::agenda::LogItems { closed: has("closed"), clock: has("clock"), state: has("state") };
        }
        if let Some(days) = &self.weekend_days {
            config.weekend_days = days.iter().copied().filter(|day| *day < 7).collect();
        }
        if let Some(grid) = &self.time_grid {
            let has = |kind: &str| grid.kinds.iter().any(|item| item == kind);
            let target: &mut TimeGrid = &mut config.time_grid;
            target.daily = has("daily");
            target.weekly = has("weekly");
            target.today = has("today");
            target.require_timed = has("require-timed");
            target.remove_match = has("remove-match");
            target.times = grid.times.clone();
            target.trailer = grid.trailer.clone();
            target.line = grid.line.clone();
        }
        if let Some(now) = &self.current_time_string {
            config.time_grid.now = now.clone();
        }
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
        if let Some((matcher, todo, tags, regexp)) = &self.stuck_projects {
            config.stuck_projects = org_core::agenda::StuckProjects {
                matcher: matcher.clone(),
                todo: todo.clone(),
                tags: tags.clone(),
                regexp: regexp.clone(),
            };
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
            "org_agenda_files": ["~/org"],
            "org_agenda_span": "day",
            "org_agenda_start_on_weekday": 0,
            "org_agenda_skip_scheduled_if_done": true,
            "org_agenda_show_future_repeats": "next",
            "org_agenda_tags_column": -100,
            "org_agenda_time_grid": [["weekly", "today"], [900, 1700]],
            "unknown": 1
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
        assert!(config.time_grid.weekly && config.time_grid.today && !config.time_grid.daily);
        assert_eq!(config.time_grid.trailer, "......", "Org's, when the list ends early");
    }

    /// The names options had before they were Org's still work, and are
    /// named; Org's own spelling, with `-`, works too.
    #[test]
    fn reads_old_names_and_orgs_spelling() {
        let loaded = load(
            Some(serde_json::json!({
                "configFile": false,
                "logDone": "time",
                "org-log-repeat": "note",
                "agenda": { "span": "day", "timeGrid": { "times": [900], "weekly": true } },
                "captureTemplates": [{ "key": "t", "description": "Task", "immediateFinish": true }],
                "org_agenda_custom_commands": [{ "key": "x", "description": "X", "type": "agenda", "settings": { "span": 3, "org-agenda-start-day": "-1d" } }],
                "agendaFile": []
            })),
            &no_env,
        );
        let options = &loaded.options;
        assert_eq!(options.todo_settings().log_done, Some(Logging::Time));
        assert_eq!(options.todo_settings().log_repeat, Some(Logging::Note));
        let config = options.agenda_config();
        assert_eq!(config.span, Span::Day);
        assert_eq!(config.time_grid.times, [900]);
        assert!(config.time_grid.weekly && config.time_grid.daily && config.time_grid.require_timed);
        assert_eq!(options.capture_templates()[0].immediate_finish, Some(true));
        assert_eq!(
            loaded.merged["org_agenda_custom_commands"][0]["settings"],
            serde_json::json!({ "org_agenda_span": 3, "org_agenda_start_day": "-1d" })
        );
        assert_eq!(loaded.problems.len(), 2, "{:?}", loaded.problems);
        assert!(loaded.problems[0].contains("logDone → org_log_done"), "{:?}", loaded.problems);
        assert!(loaded.problems[0].contains("agenda.timeGrid → org_agenda_time_grid"), "{:?}", loaded.problems);
        assert!(loaded.problems[0].contains("immediateFinish → immediate_finish"), "{:?}", loaded.problems);
        assert_eq!(loaded.problems[1], "unknown options, which are ignored: `agendaFile`");
    }

    #[test]
    fn reads_priorities_and_rounding() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "org_priority_highest": "A", "org_priority_lowest": "E", "org_priority_default": "C",
            "org_priority_start_cycle_with_default": false, "org_time_stamp_rounding_minutes": 15, "org_tags_column": -90
        }))
        .unwrap();
        let settings = options.edit_settings();
        assert_eq!(settings.priorities, Priorities { highest: 65, lowest: 69, default: 67 });
        assert!(!settings.priority_start_cycle_with_default);
        assert_eq!(settings.timestamp_rounding_minutes, 15);
        assert_eq!(settings.todo.tags_column, -90);
        assert_eq!(options.agenda_config().priorities, settings.priorities);
        let numbers: Options = serde_json::from_value(
            serde_json::json!({ "org_priority_highest": 1, "org_priority_lowest": 9, "org_priority_default": 5 }),
        )
        .unwrap();
        assert_eq!(numbers.priorities(), Priorities { highest: 1, lowest: 9, default: 5 });
        let defaults: Options = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.edit_settings(), EditSettings::default());
    }

    #[test]
    fn a_malformed_option_leaves_the_others() {
        let (options, dropped) = Options::read(serde_json::json!({
            "org_log_done": "time",
            "org_agenda_files": "~/org",
            "org_agenda_span": "day",
            "org_deadline_warning_days": "soon",
            "org_agenda_time_grid": [["daily"], ["900"]],
            "unknown": true
        }));
        assert_eq!(dropped, ["org_agenda_files", "org_agenda_time_grid", "org_deadline_warning_days"]);
        assert_eq!(options.todo_settings().log_done, Some(Logging::Time));
        let config = options.agenda_config();
        assert_eq!((config.span, config.deadline_warning_days, config.time_grid.times.len()), (Span::Day, 14, 7));
        let (whole, none) = Options::read(serde_json::json!({ "org_log_done": "time" }));
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
            "org_log_done = \"time\"\norg-todo-keywords = [\"TODO\", \"NEXT\", \"|\", \"DONE\"]\norg_deadline_warning_days = 3\norg_agenda_span = \"week\"\n",
        );
        let editor = serde_json::json!({
            "config_file": file.display().to_string(),
            "org_log_done": "note",
            "org_agenda_span": "day"
        });
        let loaded = load(Some(editor), &no_env);
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!((loaded.file, loaded.read), (Some(file.clone()), true));
        assert_eq!(
            loaded.merged,
            serde_json::json!({
                "org_log_done": "note",
                "org_todo_keywords": ["TODO", "NEXT", "|", "DONE"],
                "org_deadline_warning_days": 3,
                "org_agenda_span": "day"
            })
        );
        let config = loaded.options.agenda_config();
        assert_eq!((config.span, config.deadline_warning_days), (Span::Day, 3));
        assert_eq!(loaded.options.todo_settings().log_done, Some(Logging::Note));

        // `false`: no file.
        let alone = load(Some(serde_json::json!({ "config_file": false, "org_log_done": "time" })), &no_env);
        assert_eq!(
            (alone.file, alone.read, alone.merged),
            (None, false, serde_json::json!({ "org_log_done": "time" }))
        );
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_and_left_out() {
        let file = temp_file("broken", "org_log_done = \"time\"\nagenda = [\n");
        let loaded = load(
            Some(serde_json::json!({ "config_file": file.display().to_string(), "org_log_repeat": false })),
            &no_env,
        );
        assert!(!loaded.read);
        assert_eq!(loaded.problems.len(), 1);
        assert!(
            loaded.problems[0].contains("is not valid TOML") && loaded.problems[0].contains("line 2"),
            "{:?}",
            loaded.problems
        );
        assert_eq!(loaded.merged, serde_json::json!({ "org_log_repeat": false }));

        // A malformed option in the file goes on its own.
        std::fs::write(&file, "org_log_done = \"time\"\norg_tags_column = \"far\"\n").unwrap();
        let loaded = load(Some(serde_json::json!({ "config_file": file.display().to_string() })), &no_env);
        assert_eq!(loaded.problems, ["the option `org_tags_column` is malformed and is ignored"]);
        assert_eq!(loaded.options.todo_settings().log_done, Some(Logging::Time));

        // A file the editor names must be there; the usual one need not be.
        let missing = file.with_file_name("missing.toml");
        let named = load(Some(serde_json::json!({ "config_file": missing.display().to_string() })), &no_env);
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
  "org_agenda_files": ["~/org", "https://x//y", "/* kept */",],
  /* Org's own
     default. */
  "org_agenda_prefix_format": { "agenda": "%c", },
}
"#;
        assert_eq!(
            parse_config(std::path::Path::new("config.json"), text),
            Ok(
                serde_json::json!({ "org_agenda_files": ["~/org", "https://x//y", "/* kept */"], "org_agenda_prefix_format": { "agenda": "%c" } })
            )
        );
        // Errors keep their lines.
        let error = parse_config(std::path::Path::new("config.json"), "{\n  // a\n  \"logDone\": time\n}").unwrap_err();
        assert!(error.contains("line 3"), "{error}");
        assert_eq!(parse_config(std::path::Path::new("c.JSON"), "[1]"), Err("is not a JSON object".to_string()));
        // Any other name is TOML.
        assert_eq!(
            parse_config(std::path::Path::new("work.conf"), "org_log_done = \"time\""),
            Ok(serde_json::json!({ "org_log_done": "time" }))
        );
    }

    #[test]
    fn config_toml_or_config_json() {
        let toml = temp_file("formats", "org_log_done = \"time\"\n");
        let directory = toml.parent().unwrap().to_path_buf();
        let config = directory.join("org-lsp");
        std::fs::create_dir_all(&config).unwrap();
        let home = directory.display().to_string();
        let env = |name: &str| (name == "XDG_CONFIG_HOME").then(|| home.clone());
        // Only JSON: that one.
        std::fs::write(config.join("config.json"), "{ \"org_log_done\": \"note\", }").unwrap();
        assert_eq!(default_config_file(&env), (Some(config.join("config.json")), None));
        let loaded = load(None, &env);
        assert_eq!((loaded.read, loaded.merged), (true, serde_json::json!({ "org_log_done": "note" })));
        // Both: TOML, and a word about the other.
        std::fs::rename(&toml, config.join("config.toml")).unwrap();
        let (file, warning) = default_config_file(&env);
        assert_eq!(file, Some(config.join("config.toml")));
        assert!(warning.unwrap().contains("config.json is ignored"));
        let loaded = load(None, &env);
        assert_eq!(loaded.merged, serde_json::json!({ "org_log_done": "time" }));
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
        let off: AgendaOptions =
            serde_json::from_value(serde_json::json!({ "org_agenda_start_on_weekday": false })).unwrap();
        assert_eq!(off.to_config().start_on_weekday, None);
    }
}
