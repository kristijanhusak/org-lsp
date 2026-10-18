//! Settings a client passes as `initializationOptions`, and the command
//! line passes as JSON. Names are Org's variable names with `_` for `-`
//! (`org_agenda_files`; `org-agenda-files` is read too); those of the
//! server's own, without `org_`.

use org_core::agenda::sorting::{Limit, Sorting, Strategies};
use org_core::agenda::todo_ignore::When;
use org_core::agenda::{
    Config as AgendaConfig, FontifyPriorities, FutureRepeats, PreferLastRepeat, RemoveTags, RemoveTimes,
    ShowInheritedTags, SkipByOtherDate, SkipScheduledIfDeadline, Span, TagsColumn, TimeGrid, UseTagInheritance,
};
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
    /// The default TODO sequences: one written like Org's `#+TODO:` line,
    /// `["TODO", "NEXT", "|", "DONE"]`, or a list of them, each perhaps
    /// starting with `sequence` or `type`. Documents can override them.
    #[serde(rename = "org_todo_keywords")]
    pub todo_keywords: Option<TodoKeywordsOption>,
    /// `org-todo-interpretation`: `"sequence"` or `"type"`, for a list of
    /// keywords alone (Org's old syntax).
    #[serde(rename = "org_todo_interpretation")]
    pub todo_interpretation: Option<String>,
    /// `org-todo-keyword-faces`: a face for a keyword, a color or a
    /// property list (`:foreground blue :weight bold`).
    #[serde(rename = "org_todo_keyword_faces")]
    pub todo_keyword_faces: std::collections::BTreeMap<String, String>,
    /// `org-priority-faces`: a face for a priority, by its letter.
    #[serde(rename = "org_priority_faces")]
    pub priority_faces: std::collections::BTreeMap<String, String>,
    /// `org-tag-faces`: a face for a tag.
    #[serde(rename = "org_tag_faces")]
    pub tag_faces: std::collections::BTreeMap<String, String>,
    /// Files, directories and glob patterns the agenda is built from.
    #[serde(rename = "org_agenda_files")]
    pub agenda_files: Vec<String>,
    #[serde(rename = "org_agenda_file_regexp")]
    pub agenda_file_regexp: Option<String>,
    #[serde(rename = "org_agenda_skip_unavailable_files")]
    pub agenda_skip_unavailable_files: Option<bool>,
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
    /// `org-agenda-move-date-from-past-immediately-to-today`
    #[serde(rename = "org_agenda_move_date_from_past_immediately_to_today")]
    pub agenda_move_date_from_past_immediately_to_today: Option<bool>,
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
    /// `org-log-done-with-time`
    #[serde(rename = "org_log_done_with_time")]
    pub log_done_with_time: Option<bool>,
    /// `org-log-note-headings`: headings by kind of note (`done`, `state`,
    /// `note`, `reschedule`, `delschedule`, `redeadline`, `deldeadline`,
    /// `refile`, `clock_out`).
    #[serde(rename = "org_log_note_headings")]
    pub log_note_headings: Option<std::collections::BTreeMap<String, String>>,
    /// `org-log-states-order-reversed`
    #[serde(rename = "org_log_states_order_reversed")]
    pub log_states_order_reversed: Option<bool>,
    /// `org-log-state-notes-insert-after-drawers`
    #[serde(rename = "org_log_state_notes_insert_after_drawers")]
    pub log_state_notes_insert_after_drawers: Option<bool>,
    /// `org-log-note-clock-out`
    #[serde(rename = "org_log_note_clock_out")]
    pub log_note_clock_out: Option<bool>,
    /// `org-provide-todo-statistics`: `true`, `false`, `"all-headlines"`, a
    /// list of keywords, or a list of two lists (not done, done).
    #[serde(rename = "org_provide_todo_statistics")]
    pub provide_todo_statistics: Option<serde_json::Value>,
    /// `org-hierarchical-todo-statistics`
    #[serde(rename = "org_hierarchical_todo_statistics")]
    pub hierarchical_todo_statistics: Option<bool>,
    /// `org-closed-keep-when-no-todo`
    #[serde(rename = "org_closed_keep_when_no_todo")]
    pub closed_keep_when_no_todo: Option<bool>,
    /// `org-M-RET-may-split-line`: a boolean, or one by context
    /// (`headline`, `item`, `table`, `default`).
    #[serde(rename = "org_M_RET_may_split_line")]
    pub m_ret_may_split_line: Option<serde_json::Value>,
    /// `org-blank-before-new-entry`: `heading` and `plain_list_item`, each
    /// `true`, `false` or `"auto"`.
    #[serde(rename = "org_blank_before_new_entry")]
    pub blank_before_new_entry: Option<serde_json::Map<String, serde_json::Value>>,
    /// `org-insert-heading-respect-content`
    #[serde(rename = "org_insert_heading_respect_content")]
    pub insert_heading_respect_content: Option<bool>,
    /// `org-special-ctrl-a/e`: `true`, `false`, `"reversed"`, or a pair
    /// of them for C-a and C-e.
    #[serde(rename = "org_special_ctrl_a_e")]
    pub special_ctrl_a_e: Option<serde_json::Value>,
    /// `org-special-ctrl-k`
    #[serde(rename = "org_special_ctrl_k")]
    pub special_ctrl_k: Option<bool>,
    #[serde(rename = "org_treat_S_cursor_todo_selection_as_state_change")]
    pub treat_s_cursor_todo_selection_as_state_change: Option<bool>,
    #[serde(rename = "org_treat_insert_todo_heading_as_state_change")]
    pub treat_insert_todo_heading_as_state_change: Option<bool>,
    #[serde(rename = "org_edit_timestamp_down_means_later")]
    pub edit_timestamp_down_means_later: Option<bool>,
    #[serde(rename = "org_priority_enable_commands")]
    pub priority_enable_commands: Option<bool>,
    /// `org-track-ordered-property-with-tag`: `true` (the tag `ORDERED`),
    /// `false`, or a tag.
    #[serde(rename = "org_track_ordered_property_with_tag")]
    pub track_ordered_property_with_tag: Option<serde_json::Value>,
    /// `org-list-two-spaces-after-bullet-regexp`: a regexp, or `false`.
    #[serde(rename = "org_list_two_spaces_after_bullet_regexp")]
    pub list_two_spaces_after_bullet_regexp: Option<serde_json::Value>,
    /// `org-list-demote-modify-bullet`: by bullet, the bullet it becomes.
    #[serde(rename = "org_list_demote_modify_bullet")]
    pub list_demote_modify_bullet: Option<std::collections::BTreeMap<String, String>>,
    /// `org-list-indent-offset`
    #[serde(rename = "org_list_indent_offset")]
    pub list_indent_offset: Option<usize>,
    /// `org-list-automatic-rules`: `checkbox` and `indent`, each a boolean.
    #[serde(rename = "org_list_automatic_rules")]
    pub list_automatic_rules: Option<serde_json::Map<String, serde_json::Value>>,
    /// `org-checkbox-hierarchical-statistics`
    #[serde(rename = "org_checkbox_hierarchical_statistics")]
    pub checkbox_hierarchical_statistics: Option<bool>,
    /// `org-table-formula-constants`: by name, the value.
    #[serde(rename = "org_table_formula_constants")]
    pub table_formula_constants: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// `org-table-relative-ref-may-cross-hline`: a boolean or `"error"`.
    #[serde(rename = "org_table_relative_ref_may_cross_hline")]
    pub table_relative_ref_may_cross_hline: Option<serde_json::Value>,
    /// `org-table-formula-create-columns`
    #[serde(rename = "org_table_formula_create_columns")]
    pub table_formula_create_columns: Option<bool>,
    /// `org-table-duration-custom-format`: `seconds`, `minutes`, `hours`,
    /// `days` or `hh:mm`.
    #[serde(rename = "org_table_duration_custom_format")]
    pub table_duration_custom_format: Option<String>,
    /// `org-table-duration-hour-zero-padding`
    #[serde(rename = "org_table_duration_hour_zero_padding")]
    pub table_duration_hour_zero_padding: Option<bool>,
    /// `org-calc-default-modes`: `float_format` (`["fix", 2]`) and
    /// `angle_mode` (`"deg"` or `"rad"`).
    #[serde(rename = "org_calc_default_modes")]
    pub calc_default_modes: Option<serde_json::Map<String, serde_json::Value>>,
    /// `org-table-formula-evaluate-inline`
    #[serde(rename = "org_table_formula_evaluate_inline")]
    pub table_formula_evaluate_inline: Option<bool>,
    /// `org-table-allow-automatic-line-recalculation`
    #[serde(rename = "org_table_allow_automatic_line_recalculation")]
    pub table_allow_automatic_line_recalculation: Option<bool>,
    /// `org-table-number-regexp`
    #[serde(rename = "org_table_number_regexp")]
    pub table_number_regexp: Option<String>,
    /// `org-table-number-fraction`
    #[serde(rename = "org_table_number_fraction")]
    pub table_number_fraction: Option<f64>,
    /// `org-table-automatic-realign`
    #[serde(rename = "org_table_automatic_realign")]
    pub table_automatic_realign: Option<bool>,
    /// `org-table-tab-jumps-over-hlines`
    #[serde(rename = "org_table_tab_jumps_over_hlines")]
    pub table_tab_jumps_over_hlines: Option<bool>,
    /// `org-table-shrunk-column-indicator`
    #[serde(rename = "org_table_shrunk_column_indicator")]
    pub table_shrunk_column_indicator: Option<String>,
    /// `org-todo-state-tags-triggers`: by state (`""` for none, `todo`,
    /// `done`), the tags to set (`+TAG`) or remove (`-TAG`).
    #[serde(rename = "org_todo_state_tags_triggers")]
    pub todo_state_tags_triggers: Option<std::collections::BTreeMap<String, Vec<String>>>,
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
    /// `org-property-separators`: `[names or a regexp, separator]` pairs.
    #[serde(rename = "org_property_separators")]
    pub property_separators: Option<Vec<(InheritOption, String)>>,
    /// `org-property-format`
    #[serde(rename = "org_property_format")]
    pub property_format: Option<String>,
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
    #[serde(rename = "org_clock_in_resume")]
    pub clock_in_resume: Option<bool>,
    #[serde(rename = "org_clock_goto_may_find_recent_task")]
    pub clock_goto_may_find_recent_task: Option<bool>,
    #[serde(rename = "org_clock_history_length")]
    pub clock_history_length: Option<usize>,
    /// `org-clock-clocktable-default-properties`, as the text of a plist.
    #[serde(rename = "org_clock_rounding_minutes")]
    pub clock_rounding_minutes: Option<i64>,
    #[serde(rename = "org_clock_continuously")]
    pub clock_continuously: Option<bool>,
    #[serde(rename = "org_clock_report_include_clocking_task")]
    pub clock_report_include_clocking_task: Option<bool>,
    #[serde(rename = "org_clocktable_defaults")]
    pub clocktable_defaults: Option<String>,
    #[serde(rename = "org_clock_clocktable_language_setup")]
    pub clock_clocktable_language_setup: Option<Vec<Vec<String>>>,
    #[serde(rename = "org_clock_clocktable_default_properties")]
    pub clock_clocktable_default_properties: Option<String>,
    /// `org-clock-total-time-cell-format`
    #[serde(rename = "org_clock_total_time_cell_format")]
    pub clock_total_time_cell_format: Option<String>,
    /// `org-clock-file-time-cell-format`
    #[serde(rename = "org_clock_file_time_cell_format")]
    pub clock_file_time_cell_format: Option<String>,
    /// `org-id-method`: `uuid` (or `uuidgen`), `org` or `ts`.
    #[serde(rename = "org_id_method")]
    pub id_method: Option<String>,
    /// `org-id-prefix`
    #[serde(rename = "org_id_prefix")]
    pub id_prefix: Option<serde_json::Value>,
    /// `org-id-ts-format`
    #[serde(rename = "org_id_ts_format")]
    pub id_ts_format: Option<String>,
    /// `org-id-include-domain`
    #[serde(rename = "org_id_include_domain")]
    pub id_include_domain: Option<bool>,
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
    /// `org-cite-insert-processor`: a processor's name, or `false`.
    #[serde(rename = "org_cite_insert_processor")]
    pub cite_insert_processor: Option<serde_json::Value>,
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
    /// `org-use-fast-todo-selection`: `"auto"`, `"expert"` or `false`.
    #[serde(rename = "org_use_fast_todo_selection")]
    pub use_fast_todo_selection: Option<serde_json::Value>,
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
    /// `org-babel-tangle-lang-exts`: by language, the extension.
    #[serde(rename = "org_babel_tangle_lang_exts")]
    pub babel_tangle_lang_exts: Option<std::collections::BTreeMap<String, String>>,
    #[serde(rename = "org_babel_tangle_use_relative_file_links")]
    pub babel_tangle_use_relative_file_links: Option<bool>,
    #[serde(rename = "org_babel_tangle_comment_format_beg")]
    pub babel_tangle_comment_format_beg: Option<String>,
    #[serde(rename = "org_babel_tangle_comment_format_end")]
    pub babel_tangle_comment_format_end: Option<String>,
    #[serde(rename = "org_babel_tangle_uncomment_comments")]
    pub babel_tangle_uncomment_comments: Option<bool>,
    /// `org-babel-process-comment-text`: the name of a function.
    #[serde(rename = "org_babel_process_comment_text")]
    pub babel_process_comment_text: Option<String>,
    /// `org-babel-tangle-default-file-mode`, in octal: `"644"`.
    #[serde(rename = "org_babel_tangle_default_file_mode")]
    pub babel_tangle_default_file_mode: Option<String>,
    /// `org-babel-tangle-remove-file-before-write`: `true`, `false` or
    /// `"auto"`.
    #[serde(rename = "org_babel_tangle_remove_file_before_write")]
    pub babel_tangle_remove_file_before_write: Option<serde_json::Value>,
    #[serde(rename = "org_babel_noweb_wrap_start")]
    pub babel_noweb_wrap_start: Option<String>,
    #[serde(rename = "org_babel_noweb_wrap_end")]
    pub babel_noweb_wrap_end: Option<String>,
    /// `org-startup-indented`: show files with `org-indent-mode`.
    /// `org-startup-with-link-previews`: show the previews of the links to
    /// images when a file opens.
    #[serde(rename = "org_startup_with_link_previews")]
    pub startup_with_link_previews: Option<bool>,
    /// `org-image-actual-width`
    #[serde(rename = "org_image_actual_width")]
    pub image_actual_width: Option<serde_json::Value>,
    /// `org-image-max-width`
    #[serde(rename = "org_image_max_width")]
    pub image_max_width: Option<serde_json::Value>,
    /// `org-startup-with-latex-preview`: show the LaTeX fragments as images
    /// when a file opens.
    #[serde(rename = "org_startup_with_latex_preview")]
    pub startup_with_latex_preview: Option<bool>,
    #[serde(rename = "org_startup_indented")]
    pub startup_indented: Option<bool>,
    /// `org-hide-leading-stars`
    #[serde(rename = "org_hide_leading_stars")]
    pub hide_leading_stars: Option<bool>,
    /// `org-entities-user`: `[name, LaTeX, math?, HTML, ASCII, Latin-1,
    /// UTF-8]` for each entity of the user's.
    #[serde(rename = "org_entities_user")]
    pub entities_user: Option<Vec<Vec<serde_json::Value>>>,
    #[serde(rename = "org_list_allow_alphabetical")]
    pub list_allow_alphabetical: Option<bool>,
    /// `org-plain-list-ordered-item-terminator`: `true`, `"."` or `")"`.
    #[serde(rename = "org_plain_list_ordered_item_terminator")]
    pub plain_list_ordered_item_terminator: Option<serde_json::Value>,
    #[serde(rename = "org_indent_mode_turns_on_hiding_stars")]
    pub indent_mode_turns_on_hiding_stars: Option<bool>,
    #[serde(rename = "org_indent_mode_turns_off_org_adapt_indentation")]
    pub indent_mode_turns_off_org_adapt_indentation: Option<bool>,
    #[serde(rename = "org_indent_indentation_per_level")]
    pub indent_indentation_per_level: Option<usize>,
    /// `org-fontify-done-headline`, `org-fontify-todo-headline`,
    /// `org-fontify-quote-and-verse-blocks`
    #[serde(rename = "org_fontify_done_headline")]
    pub fontify_done_headline: Option<bool>,
    #[serde(rename = "org_fontify_todo_headline")]
    pub fontify_todo_headline: Option<bool>,
    #[serde(rename = "org_fontify_quote_and_verse_blocks")]
    pub fontify_quote_and_verse_blocks: Option<bool>,
    /// `org-duration-format` and `org-duration-units`
    #[serde(rename = "org_duration_format")]
    pub duration_format: Option<serde_json::Value>,
    #[serde(rename = "org_duration_units")]
    pub duration_units: Option<serde_json::Value>,
    /// `org-ellipsis`
    #[serde(rename = "org_ellipsis")]
    pub ellipsis: Option<String>,
    /// org-bullets: `org-bullets-mode` and `org-bullets-bullet-list`.
    #[serde(rename = "org_bullets_mode")]
    pub bullets_mode: Option<bool>,
    #[serde(rename = "org_bullets_bullet_list")]
    pub bullets_bullet_list: Option<Vec<String>>,
    /// org-bullets.nvim's bullets of lists and marks of checkboxes.
    #[serde(rename = "org_bullets_list_bullet")]
    pub bullets_list_bullet: Option<serde_json::Value>,
    #[serde(rename = "org_bullets_checkboxes")]
    pub bullets_checkboxes: Option<serde_json::Value>,
    /// `org-edit-src-content-indentation`
    #[serde(rename = "org_src_content_indentation")]
    pub src_content_indentation: Option<i64>,
    /// `org-cycle-separator-lines`: how many blank lines after a subtree
    /// keep one of them visible when it is folded.
    #[serde(rename = "org_cycle_separator_lines")]
    pub cycle_separator_lines: Option<i64>,
    /// `org-cycle-include-plain-lists`: `true`, `false` or `"integrate"`.
    #[serde(rename = "org_cycle_include_plain_lists")]
    pub cycle_include_plain_lists: Option<serde_json::Value>,
    /// `org-cycle-max-level`
    #[serde(rename = "org_cycle_max_level")]
    pub cycle_max_level: Option<usize>,
    /// `org-cycle-global-at-bob`
    #[serde(rename = "org_cycle_global_at_bob")]
    pub cycle_global_at_bob: Option<bool>,
    /// `org-cycle-level-after-item/entry-creation`
    #[serde(rename = "org_cycle_level_after_item_entry_creation")]
    pub cycle_level_after_creation: Option<bool>,
    /// `org-startup-shrink-all-tables`
    #[serde(rename = "org_startup_align_all_tables")]
    pub startup_align_all_tables: Option<bool>,
    #[serde(rename = "org_startup_shrink_all_tables")]
    pub startup_shrink_all_tables: Option<bool>,
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
    #[serde(rename = "org_agenda_jump_prefer_future")]
    pub agenda_jump_prefer_future: Option<PreferFutureOption>,
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
    /// The client has org-lsp's editor code: it knows the server's own
    /// token types, asks what a command's result asks for, binds the keys.
    /// A client that can announce it says `experimental.orgClient`.
    pub org_client: Option<bool>,
    /// Offer more commands as code actions (the agenda, capture, refile,
    /// a TODO state or a priority picked from a list, a new heading or
    /// item), and every command, in the agenda and in Org files,
    /// through the command port (see `port`): for a client that cannot
    /// run a command from a key (Zed).
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
    /// The client shows images in place of text (`org.link.preview`,
    /// `org.latex.preview`), as one announcing `experimental.orgImages` does.
    pub images: Option<bool>,
    /// The client reads the agenda's file again when told
    /// (`org-lsp/viewChanged`).
    pub view_changed: Option<bool>,
    /// The client shows the date prompt's calendar, as a file when it
    /// cannot fetch documents, read again when told
    /// (`org-lsp/viewChanged`): for clients that cannot announce
    /// `experimental.orgDatePrompt`.
    pub date_prompt: Option<bool>,
    /// `org-refile-targets`: where entries can be refiled to. Without it,
    /// the top-level headlines of the entry's own file.
    #[serde(rename = "org_refile_targets")]
    pub refile_targets: Option<Vec<RefileTargetOption>>,
    /// `org-id-completion-targets`, as `org_refile_targets` is written.
    #[serde(rename = "org_allow_promoting_top_level_subtree")]
    pub allow_promoting_top_level_subtree: Option<bool>,
    #[serde(rename = "org_odd_levels_only")]
    pub odd_levels_only: Option<bool>,
    #[serde(rename = "org_sparse_tree_open_archived_trees")]
    pub sparse_tree_open_archived_trees: Option<bool>,
    /// `org-occur-case-fold-search`: `true`, `false` or `"smart"`.
    #[serde(rename = "org_occur_case_fold_search")]
    pub occur_case_fold_search: Option<serde_json::Value>,
    /// `org-sparse-tree-default-date-type`: `null` or a type's name.
    #[serde(rename = "org_highlight_sparse_tree_matches")]
    pub highlight_sparse_tree_matches: Option<bool>,
    #[serde(rename = "org_sparse_tree_default_date_type")]
    pub sparse_tree_default_date_type: Option<String>,
    #[serde(rename = "org_yank_adjusted_subtrees")]
    pub yank_adjusted_subtrees: Option<bool>,
    #[serde(rename = "org_file_apps")]
    pub file_apps: Option<Vec<(String, String)>>,
    #[serde(rename = "org_special_ctrl_o")]
    pub special_ctrl_o: Option<bool>,
    #[serde(rename = "org_startup_numerated")]
    pub startup_numerated: Option<bool>,
    #[serde(rename = "org_num_max_level")]
    pub num_max_level: Option<usize>,
    #[serde(rename = "org_num_skip_commented")]
    pub num_skip_commented: Option<bool>,
    #[serde(rename = "org_num_skip_footnotes")]
    pub num_skip_footnotes: Option<bool>,
    #[serde(rename = "org_num_skip_tags")]
    pub num_skip_tags: Option<Vec<String>>,
    #[serde(rename = "org_num_skip_unnumbered")]
    pub num_skip_unnumbered: Option<bool>,
    #[serde(rename = "org_agenda_view_columns_initially")]
    pub agenda_view_columns_initially: Option<bool>,
    #[serde(rename = "org_agenda_persistent_filter")]
    pub agenda_persistent_filter: Option<bool>,
    #[serde(rename = "org_capture_use_agenda_date")]
    pub capture_use_agenda_date: Option<bool>,
    #[serde(rename = "org_open_non_existing_files")]
    pub open_non_existing_files: Option<bool>,
    #[serde(rename = "org_open_directory_means_index_dot_org")]
    pub open_directory_means_index_dot_org: Option<bool>,
    #[serde(rename = "org_link_search_must_match_exact_headline")]
    pub link_search_must_match_exact_headline: Option<LogOption>,
    #[serde(rename = "org_link_doi_server_url")]
    pub link_doi_server_url: Option<String>,
    #[serde(rename = "org_id_completion_targets")]
    pub id_completion_targets: Option<Vec<RefileTargetOption>>,
    /// `org-id-track-globally`
    #[serde(rename = "org_id_track_globally")]
    pub id_track_globally: Option<bool>,
    /// `org-id-search-archives`
    #[serde(rename = "org_id_search_archives")]
    pub id_search_archives: Option<bool>,
    /// `org-id-extra-files`: files or patterns.
    #[serde(rename = "org_id_extra_files")]
    pub id_extra_files: Option<Vec<String>>,
    /// `org-id-locations-file`
    #[serde(rename = "org_id_locations_file")]
    pub id_locations_file: Option<String>,
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
    /// `org-display-custom-times`
    #[serde(rename = "org_display_custom_times")]
    pub display_custom_times: Option<bool>,
    /// `org-timestamp-custom-formats`: the date, and the date with a time.
    #[serde(rename = "org_timestamp_custom_formats")]
    pub timestamp_custom_formats: Option<Vec<String>>,
    /// `org-pretty-entities-include-sub-superscripts`: with
    /// `org_pretty_entities`, sub- and superscripts shown too.
    #[serde(rename = "org_pretty_entities_include_sub_superscripts")]
    pub pretty_entities_include_sub_superscripts: Option<bool>,
    /// `org-use-sub-superscripts`: `true`, `"{}"` (braces needed) or
    /// `false`.
    #[serde(rename = "org_use_sub_superscripts")]
    pub use_sub_superscripts: Option<LogOption>,
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
    /// `org-archive-subtree-save-file-p`: `true`, `false`, `"from-org"`
    /// or `"from-agenda"`.
    #[serde(rename = "org_archive_subtree_save_file_p")]
    pub archive_subtree_save_file_p: Option<serde_json::Value>,
    /// `org-capture-templates`. Without it, Org's default: a TODO under
    /// "Tasks" in `org_default_notes_file`.
    #[serde(rename = "org_capture_templates")]
    pub capture_templates: Option<Vec<CaptureTemplateOption>>,
    /// `org-capture-templates-contexts`: where templates are offered.
    #[serde(rename = "org_capture_templates_contexts")]
    pub capture_templates_contexts: Option<Vec<ContextOption>>,
    /// org-roam: off unless switched on.
    #[serde(rename = "org_roam_enabled")]
    pub roam_enabled: Option<bool>,
    /// `org-roam-directory`
    #[serde(rename = "org_roam_directory")]
    pub roam_directory: Option<String>,
    /// `org-roam-file-extensions`
    #[serde(rename = "org_roam_file_extensions")]
    pub roam_file_extensions: Option<Vec<String>>,
    /// `org-roam-file-exclude-regexp`: a regexp or a list of them.
    #[serde(rename = "org_roam_file_exclude_regexp")]
    pub roam_file_exclude_regexp: Option<RegexpsOption>,
    /// `org-roam-node-display-template`
    #[serde(rename = "org_roam_node_display_template")]
    pub roam_node_display_template: Option<String>,
    /// `org-roam-node-formatter`: a template.
    #[serde(rename = "org_roam_node_formatter")]
    pub roam_node_formatter: Option<String>,
    /// `org-roam-db-extra-links-elements`: `"keyword"`, `"node-property"`.
    #[serde(rename = "org_roam_db_extra_links_elements")]
    pub roam_db_extra_links_elements: Option<Vec<String>>,
    /// `org-roam-db-extra-links-exclude-keys`: by `"keyword"` and
    /// `"node-property"`, the keys whose links are not recorded.
    #[serde(rename = "org_roam_db_extra_links_exclude_keys")]
    pub roam_db_extra_links_exclude_keys: Option<std::collections::BTreeMap<String, Vec<String>>>,
    /// `org-roam-graph-executable`
    #[serde(rename = "org_roam_graph_executable")]
    pub roam_graph_executable: Option<String>,
    /// `org-roam-graph-viewer`
    #[serde(rename = "org_roam_graph_viewer")]
    pub roam_graph_viewer: Option<String>,
    /// `org-roam-graph-filetype`
    #[serde(rename = "org_roam_graph_filetype")]
    pub roam_graph_filetype: Option<String>,
    /// `org-roam-graph-extra-config`, `org-roam-graph-edge-extra-config`
    /// and `org-roam-graph-node-extra-config`: Graphviz's attributes, in
    /// their order.
    #[serde(rename = "org_roam_graph_extra_config")]
    pub roam_graph_extra_config: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(rename = "org_roam_graph_edge_extra_config")]
    pub roam_graph_edge_extra_config: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(rename = "org_roam_graph_node_extra_config")]
    pub roam_graph_node_extra_config: Option<serde_json::Map<String, serde_json::Value>>,
    /// `org-roam-graph-link-hidden-types`
    #[serde(rename = "org_roam_graph_link_hidden_types")]
    pub roam_graph_link_hidden_types: Option<Vec<String>>,
    /// `org-roam-graph-max-title-length`
    #[serde(rename = "org_roam_graph_max_title_length")]
    pub roam_graph_max_title_length: Option<usize>,
    /// `org-roam-graph-shorten-titles`: `truncate`, `wrap`, or `false`.
    #[serde(rename = "org_roam_graph_shorten_titles")]
    pub roam_graph_shorten_titles: Option<serde_json::Value>,
    /// `org-roam-extract-new-file-path`
    #[serde(rename = "org_roam_extract_new_file_path")]
    pub roam_extract_new_file_path: Option<String>,
    /// `org-roam-link-auto-replace`
    #[serde(rename = "org_roam_link_auto_replace")]
    pub roam_link_auto_replace: Option<bool>,
    /// `org-roam-completion-everywhere`
    #[serde(rename = "org_roam_completion_everywhere")]
    pub roam_completion_everywhere: Option<bool>,
    /// `org-roam-mode-sections`: `backlinks`, `reflinks`,
    /// `unlinked-references`.
    #[serde(rename = "org_roam_mode_sections")]
    pub roam_mode_sections: Option<Vec<String>>,
    /// `org-roam-capture-templates`, written as `org_capture_templates`.
    #[serde(rename = "org_roam_dailies_directory")]
    pub roam_dailies_directory: Option<String>,
    #[serde(rename = "org_roam_dailies_capture_templates")]
    pub roam_dailies_capture_templates: Option<Vec<CaptureTemplateOption>>,
    #[serde(rename = "org_roam_capture_templates")]
    pub roam_capture_templates: Option<Vec<CaptureTemplateOption>>,
    /// org-journal: off unless switched on.
    #[serde(rename = "org_journal_enabled")]
    pub journal_enabled: Option<bool>,
    #[serde(rename = "org_journal_dir")]
    pub journal_dir: Option<String>,
    /// `org-journal-file-type`: `daily`, `weekly`, `monthly`, `yearly`.
    #[serde(rename = "org_journal_file_type")]
    pub journal_file_type: Option<String>,
    #[serde(rename = "org_journal_start_on_weekday")]
    pub journal_start_on_weekday: Option<i64>,
    #[serde(rename = "org_journal_file_format")]
    pub journal_file_format: Option<String>,
    #[serde(rename = "org_journal_date_format")]
    pub journal_date_format: Option<String>,
    #[serde(rename = "org_journal_date_prefix")]
    pub journal_date_prefix: Option<String>,
    #[serde(rename = "org_journal_time_format")]
    pub journal_time_format: Option<String>,
    #[serde(rename = "org_journal_time_format_post_midnight")]
    pub journal_time_format_post_midnight: Option<String>,
    #[serde(rename = "org_journal_time_prefix")]
    pub journal_time_prefix: Option<String>,
    #[serde(rename = "org_journal_hide_entries_p")]
    pub journal_hide_entries: Option<bool>,
    /// `org-journal-find-file-fn`: `"find-file-other-window"` or
    /// `"find-file"`.
    #[serde(rename = "org_journal_find_file_fn")]
    pub journal_find_file_fn: Option<String>,
    #[serde(rename = "org_journal_carryover_items")]
    pub journal_carryover_items: Option<String>,
    #[serde(rename = "org_journal_skip_carryover_drawers")]
    pub journal_skip_carryover_drawers: Option<Vec<String>>,
    /// `org-journal-carryover-delete-empty-journal`: `never`, `always`,
    /// `ask`.
    #[serde(rename = "org_journal_carryover_delete_empty_journal")]
    pub journal_carryover_delete_empty_journal: Option<String>,
    #[serde(rename = "org_journal_file_header")]
    pub journal_file_header: Option<String>,
    #[serde(rename = "org_journal_created_property_timestamp_format")]
    pub journal_created_property_timestamp_format: Option<String>,
    #[serde(rename = "org_journal_scheduled_string")]
    pub journal_scheduled_string: Option<String>,
    #[serde(rename = "org_journal_enable_encryption")]
    pub journal_enable_encryption: Option<bool>,
    #[serde(rename = "org_journal_enable_agenda_integration")]
    pub journal_enable_agenda_integration: Option<bool>,
    #[serde(rename = "org_journal_search_result_date_format")]
    pub journal_search_result_date_format: Option<String>,
    /// `org-journal-search-results-order-by`: `asc` or `desc`.
    #[serde(rename = "org_journal_search_results_order_by")]
    pub journal_search_results_order_by: Option<String>,
    /// `org-extend-today-until`: the hour before which it is yesterday.
    #[serde(rename = "org_extend_today_until")]
    pub extend_today_until: Option<i64>,
    #[serde(rename = "org_use_effective_time")]
    pub use_effective_time: Option<bool>,
    #[serde(rename = "org_use_last_clock_out_time_as_effective_time")]
    pub use_last_clock_out_time_as_effective_time: Option<bool>,
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
    /// `org-fontify-emphasized-text`
    #[serde(rename = "org_fontify_emphasized_text")]
    pub fontify_emphasized_text: Option<bool>,
    /// `org-emphasis-alist`: a face by marker.
    #[serde(rename = "org_emphasis_alist")]
    pub emphasis_alist: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// `org-hidden-keywords`
    #[serde(rename = "org_hidden_keywords")]
    pub hidden_keywords: Option<Vec<String>>,
    /// `org-hide-macro-markers`
    #[serde(rename = "org_hide_macro_markers")]
    pub hide_macro_markers: Option<bool>,
    /// Save the files a command changes other than the one it was run in.
    pub auto_save: Option<bool>,
    /// RET on the first line of a list item makes the next item, as M-RET.
    pub return_uses_meta_return: Option<bool>,
    /// `org-return-follows-link`
    #[serde(rename = "org_return_follows_link")]
    pub return_follows_link: Option<bool>,
    /// `org-goto-max-level`
    #[serde(rename = "org_goto_max_level")]
    pub goto_max_level: Option<usize>,
    /// How many months the date prompt's calendar shows: 3, or 1.
    pub calendar_months: Option<usize>,
    /// The day the calendar's weeks start on, 0 for Sunday
    /// (`calendar-week-start-day`).
    pub calendar_week_start_day: Option<usize>,
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
    /// The files known to have IDs (`org-id-files`).
    IdFiles,
    Paths(Vec<String>),
}

/// Where IDs are looked for: the options of `org-id.el`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdLookup {
    /// `org-id-track-globally`: off, an ID is looked for in its own file
    /// only.
    pub track_globally: bool,
    /// `org-id-search-archives`: the archives of the agenda files too.
    pub search_archives: bool,
    /// `org-id-extra-files`: more files or patterns, and the archives of
    /// the agenda files when one is `agenda-archives`.
    pub extra_files: Vec<String>,
    /// `org-id-locations-file`: Emacs's list of the files with IDs, read.
    pub locations_file: Option<PathBuf>,
}

/// An entry of `org-refile-targets`, read.
#[derive(Debug, Clone)]
pub struct RefileTarget {
    pub files: RefileFiles,
    pub select: Select,
}

/// org-roam's settings (`org_roam_*`).
#[derive(Debug, Clone)]
pub struct RoamSettings {
    /// `org-roam-directory`, as written.
    pub directory: String,
    pub files: org_core::roam::Settings,
    pub display_template: String,
    /// `org-roam-node-formatter`: the template of an inserted link's
    /// description.
    pub node_formatter: Option<String>,
    /// `org-roam-db-extra-links-elements`, `org-roam-db-extra-links-exclude-keys`
    pub extra_links: org_core::roam::ExtraLinks,
    /// `org-roam-extract-new-file-path`, relative to the directory.
    pub extract_new_file_path: String,
    /// `org-roam-graph`'s options, and `org-roam-graph-executable`,
    /// `org-roam-graph-viewer` (none: the editor's) and
    /// `org-roam-graph-filetype`.
    pub graph: org_core::roam::graph::Settings,
    pub graph_executable: String,
    pub graph_viewer: Option<String>,
    pub graph_filetype: String,
    pub templates: Vec<CaptureTemplateOption>,
    /// `org-roam-dailies-directory`, relative to the directory, and
    /// `org-roam-dailies-capture-templates`.
    pub dailies_directory: String,
    pub dailies_templates: Vec<CaptureTemplateOption>,
    /// `org-roam-link-auto-replace`
    pub link_auto_replace: bool,
    /// `org-roam-completion-everywhere`
    pub completion_everywhere: bool,
    /// `org-roam-mode-sections`
    pub sections: Vec<org_core::roam::buffer::Kind>,
}

/// org-journal's settings (`org_journal_*`).
#[derive(Debug, Clone)]
pub struct JournalSettings {
    /// `org-journal-dir`, as written.
    pub directory: String,
    /// `org-journal-enable-agenda-integration`
    pub agenda_integration: bool,
    /// `org-journal-find-file-fn` is `find-file-other-window`: the file of
    /// a day shows in another window.
    pub find_file_other_window: bool,
    pub core: org_core::journal::Settings,
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
    /// `:unnarrowed`: the capture buffer shows the whole target file.
    pub unnarrowed: Option<bool>,
}

/// A regexp, or a list of them.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RegexpsOption {
    One(String),
    Many(Vec<String>),
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
    #[serde(rename = "org_property_separators")]
    pub property_separators: Option<Vec<(InheritOption, String)>>,
}

impl InheritanceOptions {
    pub fn inheritance(&self) -> Inheritance {
        let default = Inheritance::default();
        Inheritance {
            tags: self.use_tag_inheritance.as_ref().map_or(default.tags, InheritOption::to_inherit),
            tags_excluded: self.tags_exclude_from_inheritance.clone().unwrap_or_default(),
            properties: self.use_property_inheritance.as_ref().map_or(default.properties, InheritOption::to_inherit),
            separators: self
                .property_separators
                .iter()
                .flatten()
                .filter_map(|(which, separator)| {
                    let which = match which {
                        InheritOption::Names(names) => org_core::outline::Separated::Names(names.clone()),
                        InheritOption::Matching(pattern) => org_core::outline::Separated::Matching(pattern.clone()),
                        InheritOption::Flag(_) => return None,
                    };
                    Some((which, separator.clone()))
                })
                .collect(),
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

/// `org-read-date-prefer-future` and its kin: `true`, `false` or `"time"`.
fn prefer_future(option: &Option<PreferFutureOption>) -> Option<PreferFuture> {
    match option.as_ref()? {
        PreferFutureOption::Flag(false) => Some(PreferFuture::No),
        PreferFutureOption::Name(name) if name == "time" => Some(PreferFuture::Time),
        _ => Some(PreferFuture::Yes),
    }
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

/// `org_todo_keywords`: one sequence, or several, as `org-todo-keywords`
/// has them: `[["TODO", "|", "DONE"], ["type", "Fred", "Sara", "|", "DONE"]]`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TodoKeywordsOption {
    One(Vec<String>),
    Several(Vec<Vec<String>>),
}

impl TodoKeywordsOption {
    /// The keywords, a sequence without `sequence` or `type` first read as
    /// `interpretation` says (`org-todo-interpretation`).
    pub fn keywords_with(&self, interpretation: org_syntax::Interpretation) -> TodoKeywords {
        use org_syntax::Interpretation;
        // `sequence` or `type` first, as in `(sequence "TODO" "|" "DONE")`.
        let read = |words: &[String]| match words.first().map(String::as_str) {
            Some("type") => (Interpretation::Type, words[1..].to_vec()),
            Some("sequence") => (Interpretation::Sequence, words[1..].to_vec()),
            _ => (interpretation, words.to_vec()),
        };
        match self {
            TodoKeywordsOption::One(words) => TodoKeywords::from_sequences([read(words)]),
            TodoKeywordsOption::Several(sequences) => {
                TodoKeywords::from_sequences(sequences.iter().map(|words| read(words)))
            }
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
            property_separators: self.property_separators.clone(),
        }
        .inheritance()
    }

    pub fn todo_keywords(&self) -> TodoKeywords {
        let interpretation = match self.todo_interpretation.as_deref() {
            Some("type") => org_syntax::Interpretation::Type,
            _ => org_syntax::Interpretation::Sequence,
        };
        self.todo_keywords
            .as_ref()
            .map(|keywords| keywords.keywords_with(interpretation))
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

    /// How TAB works: what folds and what TAB does before folding.
    pub fn cycle(&self) -> crate::folding::Cycle {
        use crate::folding::{Cycle, Folding};
        let lists = self.cycle_include_plain_lists.as_ref();
        Cycle {
            folding: Folding {
                separator_lines: self.separator_lines(),
                max_level: self.cycle_max_level.filter(|level| *level > 0),
                plain_lists: lists != Some(&serde_json::Value::Bool(false)),
            },
            integrate_lists: lists.and_then(serde_json::Value::as_str) == Some("integrate"),
            global_at_bob: self.cycle_global_at_bob.unwrap_or(false),
            level_after_creation: self.cycle_level_after_creation.unwrap_or(true),
        }
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
        read_targets(targets)
    }

    /// `org-id-completion-targets`: by default every headline of the
    /// current file and of the files with IDs.
    pub fn id_completion_targets(&self) -> Vec<RefileTarget> {
        let Some(targets) = &self.id_completion_targets else {
            return vec![
                RefileTarget { files: RefileFiles::Current, select: Select::All },
                RefileTarget { files: RefileFiles::IdFiles, select: Select::All },
            ];
        };
        read_targets(targets)
    }

    /// Where IDs are looked for. `org-id-extra-files` is by default
    /// `org-agenda-text-search-extra-files`; `org-id-locations-file`,
    /// `.org-id-locations` in Emacs's directory.
    pub fn id_lookup(&self) -> IdLookup {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        // `locate-user-emacs-file`: `~/.emacs.d` unless only the XDG one is there.
        let emacs_directory = || {
            let home = home.clone()?;
            let dot = home.join(".emacs.d");
            let xdg = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("emacs");
            Some(if !dot.exists() && xdg.exists() { xdg } else { dot })
        };
        let expand = |path: &str| match (path.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(path),
        };
        IdLookup {
            track_globally: self.id_track_globally.unwrap_or(true),
            search_archives: self.id_search_archives.unwrap_or(true),
            extra_files: self
                .id_extra_files
                .clone()
                .or_else(|| self.agenda.text_search_extra_files.clone())
                .unwrap_or_default(),
            locations_file: match &self.id_locations_file {
                Some(path) => Some(expand(path)),
                None => emacs_directory().map(|directory| directory.join(".org-id-locations")),
            },
        }
    }
}

/// `org_refile_targets` and its kin, read.
fn read_targets(targets: &[RefileTargetOption]) -> Vec<RefileTarget> {
    targets
        .iter()
        .filter_map(|target| {
            let files = match &target.files {
                None => RefileFiles::Current,
                Some(RefileFilesOption::One(name)) if name == "current" => RefileFiles::Current,
                Some(RefileFilesOption::One(name)) if name == "agenda" => RefileFiles::Agenda,
                Some(RefileFilesOption::One(name)) if name == "org-id-files" => RefileFiles::IdFiles,
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

impl Options {
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

    /// org-roam's settings, when it is on: its directory, which files
    /// are its own, how nodes are offered, and the templates of new ones.
    /// org-journal's settings, when it is on.
    pub fn journal(&self) -> Option<JournalSettings> {
        if self.journal_enabled != Some(true) {
            return None;
        }
        use org_core::journal::{DeleteEmpty, FileType, Settings};
        let defaults = Settings::default();
        let text = |value: &Option<String>, default: String| value.clone().unwrap_or(default);
        let core = Settings {
            file_type: match self.journal_file_type.as_deref() {
                Some("weekly") => FileType::Weekly,
                Some("monthly") => FileType::Monthly,
                Some("yearly") => FileType::Yearly,
                _ => FileType::Daily,
            },
            start_on_weekday: self.journal_start_on_weekday.unwrap_or(defaults.start_on_weekday),
            file_format: text(&self.journal_file_format, defaults.file_format),
            date_format: text(&self.journal_date_format, defaults.date_format),
            date_prefix: text(&self.journal_date_prefix, defaults.date_prefix),
            time_format: text(&self.journal_time_format, defaults.time_format),
            time_format_post_midnight: text(
                &self.journal_time_format_post_midnight,
                defaults.time_format_post_midnight,
            ),
            time_prefix: text(&self.journal_time_prefix, defaults.time_prefix),
            hide_entries: self.journal_hide_entries.unwrap_or(defaults.hide_entries),
            carryover_items: text(&self.journal_carryover_items, defaults.carryover_items),
            skip_carryover_drawers: self.journal_skip_carryover_drawers.clone().unwrap_or_default(),
            carryover_delete_empty_journal: match self.journal_carryover_delete_empty_journal.as_deref() {
                Some("always") => DeleteEmpty::Always,
                Some("ask") => DeleteEmpty::Ask,
                _ => DeleteEmpty::Never,
            },
            file_header: text(&self.journal_file_header, defaults.file_header),
            created_property_timestamp_format: text(
                &self.journal_created_property_timestamp_format,
                defaults.created_property_timestamp_format,
            ),
            scheduled_string: text(&self.journal_scheduled_string, defaults.scheduled_string),
            search_result_date_format: text(
                &self.journal_search_result_date_format,
                defaults.search_result_date_format.clone(),
            ),
            search_results_descending: self.journal_search_results_order_by.as_deref() == Some("desc"),
            enable_encryption: self.journal_enable_encryption.unwrap_or(false),
            extend_today_until: self.extend_today_until.unwrap_or(0),
            ..defaults
        };
        Some(JournalSettings {
            directory: self.journal_dir.clone().unwrap_or_else(|| "~/Documents/journal/".to_string()),
            agenda_integration: self.journal_enable_agenda_integration.unwrap_or(false),
            find_file_other_window: self.journal_find_file_fn.as_deref() != Some("find-file"),
            core,
        })
    }

    /// `org-roam-graph`'s options, the defaults for those not given.
    fn roam_graph_settings(&self) -> org_core::roam::graph::Settings {
        use org_core::roam::graph::{Settings, Shorten};
        let defaults = Settings::default();
        let pairs = |map: &serde_json::Map<String, serde_json::Value>| -> Vec<(String, String)> {
            map.iter()
                .map(|(key, value)| (key.clone(), value.as_str().map_or_else(|| value.to_string(), str::to_string)))
                .collect()
        };
        Settings {
            extra: self.roam_graph_extra_config.as_ref().map_or(defaults.extra, pairs),
            edge: self.roam_graph_edge_extra_config.as_ref().map_or(defaults.edge, pairs),
            node: self.roam_graph_node_extra_config.as_ref().map_or(defaults.node, |map| {
                map.iter()
                    .map(|(kind, value)| (kind.clone(), value.as_object().map(pairs).unwrap_or_default()))
                    .collect()
            }),
            hidden: self.roam_graph_link_hidden_types.clone().unwrap_or(defaults.hidden),
            max_title_length: self.roam_graph_max_title_length.unwrap_or(defaults.max_title_length),
            shorten: match &self.roam_graph_shorten_titles {
                None => defaults.shorten,
                Some(value) => match value.as_str() {
                    Some("truncate") => Shorten::Truncate,
                    Some("wrap") => Shorten::Wrap,
                    _ => Shorten::No,
                },
            },
        }
    }

    pub fn roam(&self) -> Option<RoamSettings> {
        if self.roam_enabled != Some(true) {
            return None;
        }
        let defaults = org_core::roam::Settings::default();
        Some(RoamSettings {
            directory: self.roam_directory.clone().unwrap_or_else(|| "~/org-roam/".to_string()),
            files: org_core::roam::Settings {
                extensions: self.roam_file_extensions.clone().unwrap_or(defaults.extensions),
                exclude: match &self.roam_file_exclude_regexp {
                    None => defaults.exclude,
                    Some(RegexpsOption::One(regexp)) => vec![regexp.clone()],
                    Some(RegexpsOption::Many(regexps)) => regexps.clone(),
                },
            },
            display_template: self.roam_node_display_template.clone().unwrap_or_else(|| "${title}".to_string()),
            node_formatter: self.roam_node_formatter.clone().filter(|template| !template.is_empty()),
            extra_links: {
                let mut links = org_core::roam::ExtraLinks::default();
                if let Some(elements) = &self.roam_db_extra_links_elements {
                    links.keywords = elements.iter().any(|element| element == "keyword");
                    links.properties = elements.iter().any(|element| element == "node-property");
                }
                // A list given replaces the default one whole, as Emacs's
                // alist does.
                if let Some(excluded) = &self.roam_db_extra_links_exclude_keys {
                    links.excluded_keywords = excluded.get("keyword").cloned().unwrap_or_default();
                    links.excluded_properties = excluded.get("node-property").cloned().unwrap_or_default();
                }
                links
            },
            graph: self.roam_graph_settings(),
            graph_executable: self.roam_graph_executable.clone().unwrap_or_else(|| "dot".to_string()),
            graph_viewer: self.roam_graph_viewer.clone().filter(|viewer| !viewer.is_empty()),
            graph_filetype: self.roam_graph_filetype.clone().unwrap_or_else(|| "svg".to_string()),
            extract_new_file_path: self
                .roam_extract_new_file_path
                .clone()
                .unwrap_or_else(|| "%<%Y%m%d%H%M%S>-${slug}.org".to_string()),
            dailies_directory: self.roam_dailies_directory.clone().unwrap_or_else(|| "daily/".to_string()),
            dailies_templates: self.roam_dailies_capture_templates.clone().unwrap_or_else(|| {
                vec![CaptureTemplateOption {
                    key: "d".to_string(),
                    description: "default".to_string(),
                    kind: Some("entry".to_string()),
                    target: vec![
                        "file+head".to_string(),
                        "%<%Y-%m-%d>.org".to_string(),
                        "#+title: %<%Y-%m-%d>\n".to_string(),
                    ],
                    template: Some(TemplateText::Text("* %?".to_string())),
                    ..CaptureTemplateOption::default()
                }]
            }),
            link_auto_replace: self.roam_link_auto_replace.unwrap_or(true),
            completion_everywhere: self.roam_completion_everywhere.unwrap_or(false),
            sections: self
                .roam_mode_sections
                .clone()
                .unwrap_or_else(|| vec!["backlinks".to_string(), "reflinks".to_string()])
                .iter()
                .filter_map(|name| match name.as_str() {
                    "backlinks" => Some(org_core::roam::buffer::Kind::Backlinks),
                    "reflinks" => Some(org_core::roam::buffer::Kind::Reflinks),
                    "unlinked-references" => Some(org_core::roam::buffer::Kind::UnlinkedReferences),
                    _ => None,
                })
                .collect(),
            templates: self.roam_capture_templates.clone().unwrap_or_else(|| {
                vec![CaptureTemplateOption {
                    key: "d".to_string(),
                    description: "default".to_string(),
                    kind: Some("plain".to_string()),
                    target: vec![
                        "file+head".to_string(),
                        "%<%Y%m%d%H%M%S>-${slug}.org".to_string(),
                        "#+title: ${title}\n".to_string(),
                    ],
                    template: Some(TemplateText::Text("%?".to_string())),
                    unnarrowed: Some(true),
                    ..CaptureTemplateOption::default()
                }]
            }),
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

    /// `org-entities-user`: each entity's name and what it is in UTF-8.
    pub fn user_entities(&self) -> Vec<(String, String)> {
        self.entities_user
            .iter()
            .flatten()
            .filter_map(|entity| {
                let name = entity.first()?.as_str()?;
                let utf8 = entity.get(6)?.as_str()?;
                Some((name.to_string(), utf8.to_string()))
            })
            .collect()
    }

    /// `org-entities-user` as Emacs Lisp, for the server's Emacs to export
    /// with.
    pub fn user_entities_lisp(&self) -> Option<String> {
        let entities = self.entities_user.as_ref().filter(|entities| !entities.is_empty())?;
        let lisp_string = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
        let entries: Vec<String> = entities
            .iter()
            .map(|entity| {
                let fields: Vec<String> = entity
                    .iter()
                    .map(|field| match field {
                        serde_json::Value::String(text) => lisp_string(text),
                        serde_json::Value::Bool(true) => "t".to_string(),
                        _ => "nil".to_string(),
                    })
                    .collect();
                format!("({})", fields.join(" "))
            })
            .collect();
        Some(format!("(setq org-entities-user '({}))", entries.join(" ")))
    }

    /// `org-list-allow-alphabetical` and
    /// `org-plain-list-ordered-item-terminator`: how ordered items are
    /// written.
    pub fn list_syntax(&self) -> org_syntax::ListSyntax {
        let terminator = match self.plain_list_ordered_item_terminator.as_ref().and_then(serde_json::Value::as_str) {
            Some(".") => org_syntax::Terminator::Dot,
            Some(")") => org_syntax::Terminator::Paren,
            _ => org_syntax::Terminator::Both,
        };
        org_syntax::ListSyntax { alphabetical: self.list_allow_alphabetical == Some(true), terminator }
    }

    /// The options Emacs must know, as Emacs Lisp: `org-entities-user`, the
    /// tangling options, `org-list-allow-alphabetical`,
    /// `org-plain-list-ordered-item-terminator`.
    pub fn emacs_lisp(&self) -> Option<String> {
        let mut forms: Vec<String> = self.user_entities_lisp().into_iter().collect();
        forms.extend(self.tangle_lisp());
        let syntax = self.list_syntax();
        if syntax != org_syntax::ListSyntax::default() {
            let terminator = match syntax.terminator {
                org_syntax::Terminator::Both => "t",
                org_syntax::Terminator::Dot => "?.",
                org_syntax::Terminator::Paren => "?\\)",
            };
            forms.push(format!(
                "(setq org-list-allow-alphabetical {} org-plain-list-ordered-item-terminator {terminator})",
                if syntax.alphabetical { "t" } else { "nil" }
            ));
            // When the configuration file has loaded Org already, its
            // parser reads them again.
            forms.push("(when (featurep 'org-element) (org-element-update-syntax))".to_string());
        }
        (!forms.is_empty()).then(|| forms.join("\n"))
    }

    /// How tangling goes, but for `org-src-preserve-indentation`.
    pub fn tangle_settings(&self) -> org_core::tangle::Settings {
        let defaults = org_core::tangle::Settings::default();
        org_core::tangle::Settings {
            preserve_indentation: self.src_preserve_indentation.unwrap_or(false),
            lang_exts: self
                .babel_tangle_lang_exts
                .iter()
                .flatten()
                .map(|(lang, extension)| (lang.clone(), extension.clone()))
                .collect(),
            relative_links: self.babel_tangle_use_relative_file_links.unwrap_or(true),
            comment_format: (
                self.babel_tangle_comment_format_beg.clone().unwrap_or(defaults.comment_format.0),
                self.babel_tangle_comment_format_end.clone().unwrap_or(defaults.comment_format.1),
            ),
            uncomment_comments: self.babel_tangle_uncomment_comments.unwrap_or(false),
            process_comment_text: self
                .babel_process_comment_text
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or(defaults.process_comment_text),
            default_file_mode: self
                .babel_tangle_default_file_mode
                .as_deref()
                .and_then(|mode| u32::from_str_radix(mode, 8).ok())
                .unwrap_or(defaults.default_file_mode),
            noweb_wrap: (
                self.babel_noweb_wrap_start.clone().unwrap_or(defaults.noweb_wrap.0),
                self.babel_noweb_wrap_end.clone().unwrap_or(defaults.noweb_wrap.1),
            ),
        }
    }

    /// `org-babel-tangle-remove-file-before-write`: always (`Some(true)`),
    /// never (`Some(false)`), or when the file is read-only (`None`).
    pub fn tangle_remove_file_before_write(&self) -> Option<bool> {
        match &self.babel_tangle_remove_file_before_write {
            Some(serde_json::Value::Bool(remove)) => Some(*remove),
            _ => None,
        }
    }

    /// The tangling options the server's Emacs is told, as Emacs Lisp
    /// (for tangling and detangling it does).
    fn tangle_lisp(&self) -> Vec<String> {
        let lisp_string = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
        let mut forms = Vec::new();
        let mut set = |name: &str, value: String| forms.push(format!("(setq {name} {value})"));
        if let Some(exts) = self.babel_tangle_lang_exts.as_ref().filter(|exts| !exts.is_empty()) {
            for (lang, extension) in exts {
                set(
                    "org-babel-tangle-lang-exts",
                    format!(
                        "(cons (cons {} {}) org-babel-tangle-lang-exts)",
                        lisp_string(lang),
                        lisp_string(extension)
                    ),
                );
            }
        }
        if self.babel_tangle_use_relative_file_links == Some(false) {
            set("org-babel-tangle-use-relative-file-links", "nil".to_string());
        }
        if let Some(format) = &self.babel_tangle_comment_format_beg {
            set("org-babel-tangle-comment-format-beg", lisp_string(format));
        }
        if let Some(format) = &self.babel_tangle_comment_format_end {
            set("org-babel-tangle-comment-format-end", lisp_string(format));
        }
        if self.babel_tangle_uncomment_comments == Some(true) {
            set("org-babel-tangle-uncomment-comments", "t".to_string());
        }
        if let Some(name) = self
            .babel_process_comment_text
            .as_ref()
            .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || "-_/:*+<>=!?".contains(c)))
        {
            set("org-babel-process-comment-text", format!("'{name}"));
        }
        if let Some(mode) =
            self.babel_tangle_default_file_mode.as_deref().and_then(|mode| u32::from_str_radix(mode, 8).ok())
        {
            set("org-babel-tangle-default-file-mode", format!("#o{mode:o}"));
        }
        match self.tangle_remove_file_before_write() {
            Some(true) => set("org-babel-tangle-remove-file-before-write", "t".to_string()),
            Some(false) => set("org-babel-tangle-remove-file-before-write", "nil".to_string()),
            None => {}
        }
        if let Some(start) = &self.babel_noweb_wrap_start {
            set("org-babel-noweb-wrap-start", lisp_string(start));
        }
        if let Some(end) = &self.babel_noweb_wrap_end {
            set("org-babel-noweb-wrap-end", lisp_string(end));
        }
        forms
    }

    /// `org-cite-insert-processor`: none for `false`.
    pub fn cite_insert_processor(&self) -> Option<String> {
        match &self.cite_insert_processor {
            None => Some("basic".to_string()),
            Some(serde_json::Value::String(name)) if !name.is_empty() => Some(name.clone()),
            Some(_) => None,
        }
    }

    /// `org-occur-case-fold-search`
    pub fn occur_case_fold(&self) -> org_core::sparse::CaseFold {
        use org_core::sparse::CaseFold;
        match &self.occur_case_fold_search {
            Some(serde_json::Value::Bool(false)) => CaseFold::Never,
            Some(serde_json::Value::String(smart)) if smart == "smart" => CaseFold::Smart,
            _ => CaseFold::Always,
        }
    }

    /// `org-sparse-tree-default-date-type`
    pub fn sparse_date_type(&self) -> org_core::sparse::DateType {
        self.sparse_tree_default_date_type
            .as_deref()
            .and_then(org_core::sparse::DateType::from_name)
            .unwrap_or_default()
    }

    /// `org-archive-subtree-save-file-p`: whether the archive file is
    /// saved, archiving from an Org file (`.0`) and from the agenda (`.1`).
    pub fn archive_save(&self) -> (bool, bool) {
        match &self.archive_subtree_save_file_p {
            Some(serde_json::Value::Bool(true)) => (true, true),
            Some(serde_json::Value::Bool(false)) => (false, false),
            Some(serde_json::Value::String(when)) if when == "from-agenda" => (false, true),
            _ => (true, false),
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

    /// How M-RET and its kin insert: `org-M-RET-may-split-line`,
    /// `org-blank-before-new-entry`, `org-insert-heading-respect-content`.
    pub fn insert_settings(&self) -> org_core::meta_return::Settings {
        // `org-get-alist-option`: the context's value, else `default`.
        let split = |context: &str| match &self.m_ret_may_split_line {
            None => true,
            Some(serde_json::Value::Object(contexts)) => contexts
                .get(context)
                .or_else(|| contexts.get("default"))
                .is_some_and(|value| value.as_bool().unwrap_or(false)),
            Some(value) => value.as_bool().unwrap_or(false),
        };
        // `auto` is none.
        let blank = |names: &[&str]| {
            let entries = self.blank_before_new_entry.as_ref()?;
            match names.iter().find_map(|name| entries.get(*name)) {
                None => Some(false),
                Some(serde_json::Value::Bool(on)) => Some(*on),
                Some(_) => None,
            }
        };
        org_core::meta_return::Settings {
            split: org_core::meta_return::Split {
                headline: split("headline"),
                item: split("item"),
                table: split("table"),
            },
            blank_heading: blank(&["heading"]),
            blank_item: blank(&["plain_list_item", "plain-list-item"]),
            respect_content: self.insert_heading_respect_content.unwrap_or(false),
            statistics: self.todo_statistics(),
            lists: self.list_settings(),
            table: self.table_settings(),
            ..org_core::meta_return::Settings::default()
        }
    }

    /// How lists are edited: the options of `org-list.el`.
    pub fn list_settings(&self) -> org_core::list::Settings {
        let defaults = org_core::list::Settings::default();
        // `(cdr (assq RULE org-list-automatic-rules))`: a rule left out
        // of the option is off.
        let rule = |name: &str, default: bool| match &self.list_automatic_rules {
            None => default,
            Some(rules) => rules.get(name).and_then(serde_json::Value::as_bool).unwrap_or(false),
        };
        org_core::list::Settings {
            two_spaces: self
                .list_two_spaces_after_bullet_regexp
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .and_then(org_core::list::BulletRegexp::new),
            demote_bullets: self.list_demote_modify_bullet.clone().unwrap_or_default().into_iter().collect(),
            indent_offset: self.list_indent_offset.unwrap_or(defaults.indent_offset),
            checkbox_rule: rule("checkbox", defaults.checkbox_rule),
            indent_rule: rule("indent", defaults.indent_rule),
            checkbox_hierarchical: self.checkbox_hierarchical_statistics.unwrap_or(defaults.checkbox_hierarchical),
        }
    }

    /// `org-provide-todo-statistics` and `org-hierarchical-todo-statistics`.
    pub fn todo_statistics(&self) -> org_core::todo::Statistics {
        use org_core::todo::ProvideStatistics;
        let strings = |value: &serde_json::Value| -> Vec<String> {
            value.as_array().into_iter().flatten().filter_map(|item| item.as_str().map(str::to_string)).collect()
        };
        let provide = match &self.provide_todo_statistics {
            None | Some(serde_json::Value::Bool(true)) => ProvideStatistics::Todo,
            Some(serde_json::Value::String(symbol)) if symbol == "all-headlines" => ProvideStatistics::All,
            Some(serde_json::Value::Array(items)) if items.first().is_some_and(serde_json::Value::is_array) => {
                ProvideStatistics::Split(strings(&items[0]), items.get(1).map(strings).unwrap_or_default())
            }
            Some(value @ serde_json::Value::Array(_)) => ProvideStatistics::Keywords(strings(value)),
            Some(_) => ProvideStatistics::No,
        };
        org_core::todo::Statistics { provide, hierarchical: self.hierarchical_todo_statistics.unwrap_or(true) }
    }

    /// Habits read their notes as notes are written
    /// (`org-log-note-headings`, `org-log-states-order-reversed`).
    pub fn habit_notes(&self, agenda: &mut AgendaConfig) {
        if let Some(habits) = &mut agenda.habits {
            let todo = self.todo_settings();
            habits.done_heading = todo.note_headings.done;
            habits.states_order_reversed = todo.states_order_reversed;
        }
    }

    /// The agenda settings, with the priorities the agenda sorts by.
    pub fn agenda_config(&self) -> AgendaConfig {
        let tags = self.tag_settings();
        let mut agenda = self.agenda.to_config();
        self.habit_notes(&mut agenda);
        AgendaConfig {
            sorting: Sorting { tags_sort: tags.sort, ..agenda.sorting },
            priorities: self.priorities(),
            tag_alist: tags.alist,
            tag_persistent_alist: tags.persistent,
            group_tags: self.group_tags.unwrap_or(true),
            extend_today_until: self.extend_today_until.unwrap_or(0),
            clocktable: org_core::clocktable::Settings {
                extend_today_until: self.extend_today_until.unwrap_or(0),
                keywords: self.todo_keywords(),
                inheritance: self.inheritance(),
                ..self.clocktable_settings()
            },
            ..agenda
        }
    }

    /// How answers to the date prompt are read.
    /// `org-num-mode`'s options.
    pub fn num_settings(&self) -> org_core::num::Settings {
        org_core::num::Settings {
            max_level: self.num_max_level,
            skip_commented: self.num_skip_commented.unwrap_or(false),
            skip_footnotes: self.num_skip_footnotes.unwrap_or(false),
            skip_tags: self.num_skip_tags.clone().unwrap_or_default(),
            skip_unnumbered: self.num_skip_unnumbered.unwrap_or(false),
        }
    }

    pub fn read_date_settings(&self) -> ReadDateSettings {
        let defaults = ReadDateSettings::default();
        ReadDateSettings {
            prefer_future: prefer_future(&self.read_date_prefer_future).unwrap_or(defaults.prefer_future),
            force_compatible_dates: self.read_date_force_compatible_dates.unwrap_or(defaults.force_compatible_dates),
            extend_today_until: self.extend_today_until.unwrap_or(0),
        }
    }

    /// `org-agenda-jump-prefer-future`: how the agenda's `j` reads a date,
    /// when not as every other prompt does.
    pub fn agenda_jump_prefer_future(&self) -> Option<PreferFuture> {
        prefer_future(&self.agenda_jump_prefer_future)
    }

    /// How clocking writes (`org-clock-into-drawer` and its relatives).
    /// How links are followed (`org-link-search-must-match-exact-headline`,
    /// `org-link-doi-server-url`).
    pub fn open_settings(&self) -> org_core::open::Settings {
        use org_core::open::ExactHeadline;
        let defaults = org_core::open::Settings::default();
        org_core::open::Settings {
            exact_headline: match &self.link_search_must_match_exact_headline {
                Some(LogOption::Flag(false)) => ExactHeadline::No,
                Some(LogOption::Name(name)) if name == "query-to-create" => ExactHeadline::QueryToCreate,
                Some(_) => ExactHeadline::Yes,
                None => defaults.exact_headline,
            },
            doi_server: self.link_doi_server_url.clone().unwrap_or(defaults.doi_server),
        }
    }

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
            states_order_reversed: self.log_states_order_reversed.unwrap_or(true),
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
                .src_content_indentation
                .and_then(|columns| usize::try_from(columns).ok())
                .unwrap_or(defaults.src_content_indentation),
            two_spaces: self.list_settings().two_spaces,
            property_format: self.property_format.clone().unwrap_or(defaults.property_format),
        }
    }

    /// How files show: `org-startup-indented`, `org-hide-leading-stars`.
    pub fn display(&self) -> org_core::indent::Display {
        org_core::indent::Display {
            startup_indented: self.startup_indented.unwrap_or(false),
            hide_leading_stars: self.hide_leading_stars.unwrap_or(false),
            bullets: self.bullets().is_some(),
            indent_hides_stars: self.indent_mode_turns_on_hiding_stars.unwrap_or(true),
            indent_turns_off_adapt: self.indent_mode_turns_off_org_adapt_indentation.unwrap_or(true),
            per_level: self.indent_indentation_per_level.unwrap_or(2),
        }
    }

    /// `org-image-actual-width`: `true` (the default), `false`, a number, or
    /// a list of one number.
    pub fn image_actual_width(&self) -> org_core::link_preview::ActualWidth {
        use org_core::link_preview::{ActualWidth, Number};
        let number = |value: &serde_json::Value| match value.as_i64() {
            Some(integer) => Some(Number::Integer(integer)),
            None => value.as_f64().map(Number::Float),
        };
        match &self.image_actual_width {
            None | Some(serde_json::Value::Bool(true)) => ActualWidth::Actual,
            Some(serde_json::Value::Bool(false)) | Some(serde_json::Value::Null) => ActualWidth::Attributes(None),
            Some(serde_json::Value::Array(items)) => ActualWidth::Attributes(items.first().and_then(number)),
            Some(value) => number(value).map_or(ActualWidth::Actual, ActualWidth::Fixed),
        }
    }

    /// `org-image-max-width`, as the editors are told it: `{ "columns" }`
    /// for `fill-column` (the default, Emacs's 70), `{ "fraction" }` of the
    /// window for `window` (all of it) or a float, `{ "pixels" }` for an
    /// integer; null for none.
    pub fn image_max_width(&self) -> serde_json::Value {
        match &self.image_max_width {
            None => serde_json::json!({ "columns": 70 }),
            Some(serde_json::Value::String(name)) if name == "fill-column" => serde_json::json!({ "columns": 70 }),
            Some(serde_json::Value::String(name)) if name == "window" => serde_json::json!({ "fraction": 1.0 }),
            Some(serde_json::Value::Number(number)) if number.is_f64() => {
                serde_json::json!({ "fraction": number.as_f64() })
            }
            Some(serde_json::Value::Number(number)) => serde_json::json!({ "pixels": number }),
            _ => serde_json::Value::Null,
        }
    }

    /// `org-use-fast-todo-selection`: whether `org-todo` picks the state by
    /// key, and whether it lists the keys in its prompt alone (`expert`).
    /// `true` is `auto`, as Org's `:set` makes it.
    pub fn fast_todo_selection(&self) -> FastTodo {
        match &self.use_fast_todo_selection {
            Some(serde_json::Value::Bool(false)) | Some(serde_json::Value::Null) => FastTodo::Never,
            Some(serde_json::Value::String(name)) if name == "expert" => FastTodo::Expert,
            _ => FastTodo::Auto,
        }
    }

    /// How durations are written and read (`org-duration-format`,
    /// `org-duration-units`): Org's defaults for what is not set, or does
    /// not hold together (a unit of the format that the units do not have).
    pub fn durations(&self) -> org_core::duration::Durations {
        use org_core::duration::{Durations, Format, Special, Unit};
        let defaults = Durations::default();
        let units = match &self.duration_units {
            Some(serde_json::Value::Array(pairs)) => pairs
                .iter()
                .filter_map(|pair| {
                    let name = pair.get(0)?.as_str()?;
                    let minutes = pair.get(1)?;
                    Some(Unit { name: name.to_string(), minutes: minutes.as_f64()?, float: minutes.is_f64() })
                })
                .collect(),
            _ => defaults.units.clone(),
        };
        let format = match &self.duration_format {
            Some(serde_json::Value::String(name)) if name == "h:mm" => Format::HMm,
            Some(serde_json::Value::String(name)) if name == "h:mm:ss" => Format::HMmSs,
            Some(serde_json::Value::Array(items)) => {
                let (mut list, mut special, mut compact) = (Vec::new(), None, false);
                for item in items {
                    match (item.as_str(), item.get(0).and_then(serde_json::Value::as_str), item.get(1)) {
                        (Some("compact"), _, _) => compact = true,
                        (_, Some("special"), Some(value)) => {
                            special = match value.as_str() {
                                Some("h:mm") => Some(Special::HMm),
                                Some("h:mm:ss") => Some(Special::HMmSs),
                                _ => value.as_u64().map(|digits| Special::Precision(digits as usize)),
                            }
                        }
                        (_, Some(unit), required) => list
                            .push((unit.to_string(), required.and_then(serde_json::Value::as_bool).unwrap_or(false))),
                        _ => {}
                    }
                }
                Format::Units { units: list, special, compact }
            }
            _ => defaults.format.clone(),
        };
        let durations = Durations { format, units };
        if durations.is_valid() {
            durations
        } else {
            defaults
        }
    }

    /// How headlines and blocks are shown (`org-fontify-…`).
    pub fn fontify(&self) -> crate::highlight::Fontify {
        let defaults = crate::highlight::Fontify::default();
        crate::highlight::Fontify {
            done_headline: self.fontify_done_headline.unwrap_or(defaults.done_headline),
            todo_headline: self.fontify_todo_headline.unwrap_or(defaults.todo_headline),
            quote_and_verse: self.fontify_quote_and_verse_blocks.unwrap_or(defaults.quote_and_verse),
            emphasis: self.emphasis_looks(),
            hidden_keywords: self.hidden_keywords.iter().flatten().fold(0, |bits, name| {
                let bit = crate::highlight::HIDDEN_KEYWORDS.iter().position(|hidden| *hidden == name.as_str());
                bit.map_or(bits, |bit| bits | (1 << bit))
            }),
            hide_macro_markers: self.hide_macro_markers.unwrap_or(false),
            odd_levels: self.odd_levels_only.unwrap_or(false),
        }
    }

    /// The looks of emphasis (`org-emphasis-alist`), none with
    /// `org-fontify-emphasized-text` off.
    fn emphasis_looks(&self) -> Option<[crate::highlight::Emphasis; 6]> {
        use crate::highlight::Emphasis;
        if self.fontify_emphasized_text == Some(false) {
            return None;
        }
        let Some(alist) = &self.emphasis_alist else {
            return Some(Emphasis::DEFAULTS);
        };
        Some(Emphasis::MARKERS.map(|marker| alist.get(marker).map_or(Emphasis::NONE, emphasis_face)))
    }

    /// Whether the markers of emphasis are hidden: with
    /// `org-hide-emphasis-markers`, where emphasis is shown at all.
    pub fn conceal_emphasis(&self) -> bool {
        self.hide_emphasis_markers.unwrap_or(false) && self.fontify_emphasized_text != Some(false)
    }

    /// What a folded heading shows after its text (`org-ellipsis`): the
    /// string when it is one and not empty, else the standard three dots.
    pub fn ellipsis(&self) -> String {
        self.ellipsis.clone().filter(|ellipsis| !ellipsis.is_empty()).unwrap_or_else(|| "...".to_string())
    }

    /// What org-bullets shows, when it is on: the bullets of headlines,
    /// and, unless `false`, those of list items and the marks of
    /// checkboxes.
    pub fn bullets(&self) -> Option<org_core::pretty::Bullets> {
        if self.bullets_mode != Some(true) {
            return None;
        }
        let headlines = || ["◉", "○", "✸", "✿"].iter().map(|bullet| bullet.to_string()).collect();
        let list = match &self.bullets_list_bullet {
            Some(serde_json::Value::String(bullet)) => Some(bullet.clone()),
            Some(serde_json::Value::Bool(false)) => None,
            _ => Some("•".to_string()),
        };
        let mark = |name: &str, default: &str| {
            let given = self.bullets_checkboxes.as_ref().and_then(|marks| marks.get(name));
            given.and_then(serde_json::Value::as_str).unwrap_or(default).to_string()
        };
        let checkboxes = match &self.bullets_checkboxes {
            Some(serde_json::Value::Bool(false)) => None,
            _ => Some(org_core::pretty::Checkboxes {
                todo: mark("todo", " "),
                half: mark("half", "-"),
                done: mark("done", "✓"),
            }),
        };
        Some(org_core::pretty::Bullets {
            headlines: self.bullets_bullet_list.clone().unwrap_or_else(headlines),
            list,
            checkboxes,
        })
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
            formulas: self.table_settings().formulas,
            defaults: self.clocktable_defaults.clone().unwrap_or(defaults.defaults.clone()),
            // Each a language, then its words.
            languages: match &self.clock_clocktable_language_setup {
                Some(languages) => {
                    languages.iter().filter_map(|entry| Some((entry.first()?.clone(), entry[1..].to_vec()))).collect()
                }
                None => defaults.languages.clone(),
            },
            ..defaults
        }
    }

    /// How tables are edited and calculated: the options of
    /// `org-table.el`.
    pub fn table_settings(&self) -> org_core::table::Settings {
        use org_core::tblfm::{CrossHline, DurationFormat};
        let defaults = org_core::table::Settings::default();
        let mut calc = defaults.formulas.calc;
        if let Some(modes) = &self.calc_default_modes {
            if let Some([kind, digits]) =
                modes.get("float_format").and_then(|value| value.as_array()).map(Vec::as_slice)
            {
                let digits = digits.as_u64().unwrap_or(0).min(1000) as usize;
                calc.float_format = match kind.as_str() {
                    Some("fix") => org_core::calc::FloatFormat::Fix(digits),
                    Some("sci") => org_core::calc::FloatFormat::Sci(digits),
                    _ => org_core::calc::FloatFormat::Float(digits),
                };
            }
            if let Some(angle) = modes.get("angle_mode").and_then(serde_json::Value::as_str) {
                calc.radians = angle == "rad";
            }
        }
        let formulas = org_core::tblfm::Settings {
            constants: self
                .table_formula_constants
                .iter()
                .flatten()
                .map(|(name, value)| {
                    let value = match value {
                        serde_json::Value::String(text) => text.clone(),
                        other => other.to_string(),
                    };
                    (name.clone(), value)
                })
                .collect(),
            cross_hline: match &self.table_relative_ref_may_cross_hline {
                Some(serde_json::Value::Bool(false)) => CrossHline::Stop,
                Some(serde_json::Value::String(word)) if word == "error" => CrossHline::Error,
                _ => CrossHline::Cross,
            },
            create_columns: self.table_formula_create_columns.unwrap_or(defaults.formulas.create_columns),
            duration_format: match self.table_duration_custom_format.as_deref() {
                Some("seconds") => DurationFormat::Seconds,
                Some("minutes") => DurationFormat::Minutes,
                Some("days") => DurationFormat::Days,
                Some("hh:mm") => DurationFormat::HoursMinutes,
                _ => DurationFormat::Hours,
            },
            hour_zero_padding: self.table_duration_hour_zero_padding.unwrap_or(defaults.formulas.hour_zero_padding),
            calc,
            alignment: self.table_alignment(),
        };
        org_core::table::Settings {
            formulas,
            evaluate_inline: self.table_formula_evaluate_inline.unwrap_or(defaults.evaluate_inline),
            line_recalculation: self.table_allow_automatic_line_recalculation.unwrap_or(defaults.line_recalculation),
            automatic_realign: self.table_automatic_realign.unwrap_or(defaults.automatic_realign),
            tab_jumps_over_hlines: self.table_tab_jumps_over_hlines.unwrap_or(defaults.tab_jumps_over_hlines),
            shrunk_column_indicator: self
                .table_shrunk_column_indicator
                .clone()
                .unwrap_or(defaults.shrunk_column_indicator),
        }
    }

    /// When a column of a table aligns right: `org-table-number-regexp`
    /// and `org-table-number-fraction`.
    pub fn table_alignment(&self) -> org_core::format::Alignment {
        let defaults = org_core::format::Alignment::default();
        org_core::format::Alignment {
            number: self.table_number_regexp.as_deref().and_then(org_core::text::EmacsRegexp::new),
            fraction: self.table_number_fraction.filter(|fraction| fraction.is_finite()).unwrap_or(defaults.fraction),
        }
    }

    /// The table options set, as Emacs variables and their values in Lisp,
    /// for the formulas Emacs computes in org-lsp's place.
    pub fn table_emacs_variables(&self) -> Vec<(String, String)> {
        let lisp_string = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
        let flag = |on: bool| if on { "t" } else { "nil" }.to_string();
        let settings = self.table_settings();
        let formulas = &settings.formulas;
        let mut out = Vec::new();
        if self.table_formula_constants.is_some() {
            let pairs: Vec<String> = formulas
                .constants
                .iter()
                .map(|(name, value)| format!("({} . {})", lisp_string(name), lisp_string(value)))
                .collect();
            out.push(("org-table-formula-constants".to_string(), format!("'({})", pairs.join(" "))));
        }
        if self.table_relative_ref_may_cross_hline.is_some() {
            let value = match formulas.cross_hline {
                org_core::tblfm::CrossHline::Cross => "t",
                org_core::tblfm::CrossHline::Stop => "nil",
                org_core::tblfm::CrossHline::Error => "'error",
            };
            out.push(("org-table-relative-ref-may-cross-hline".to_string(), value.to_string()));
        }
        if let Some(on) = self.table_formula_create_columns {
            out.push(("org-table-formula-create-columns".to_string(), flag(on)));
        }
        if let Some(format) = &self.table_duration_custom_format {
            if ["seconds", "minutes", "hours", "days", "hh:mm"].contains(&format.as_str()) {
                out.push(("org-table-duration-custom-format".to_string(), format!("'{format}")));
            }
        }
        if let Some(regexp) = &self.table_number_regexp {
            out.push(("org-table-number-regexp".to_string(), lisp_string(regexp)));
        }
        if let Some(fraction) = self.table_number_fraction.filter(|fraction| fraction.is_finite()) {
            out.push(("org-table-number-fraction".to_string(), format!("{fraction:?}")));
        }
        if let Some(on) = self.table_duration_hour_zero_padding {
            out.push(("org-table-duration-hour-zero-padding".to_string(), flag(on)));
        }
        if self.calc_default_modes.is_some() {
            let float = match formulas.calc.float_format {
                org_core::calc::FloatFormat::Float(digits) => format!("(float {digits})"),
                org_core::calc::FloatFormat::Fix(digits) => format!("(fix {digits})"),
                org_core::calc::FloatFormat::Sci(digits) => format!("(sci {digits})"),
            };
            let angle = if formulas.calc.radians { "rad" } else { "deg" };
            out.push((
                "org-calc-default-modes".to_string(),
                format!(
                    "(plist-put (plist-put (copy-sequence (default-value 'org-calc-default-modes)) \
                     'calc-float-format '{float}) 'calc-angle-mode '{angle})"
                ),
            ));
        }
        out
    }

    /// How new IDs are made (`org-id-new`).
    pub fn id_settings(&self) -> org_core::link::IdSettings {
        use org_core::link::{IdMethod, IdSettings};
        let defaults = IdSettings::default();
        IdSettings {
            method: match self.id_method.as_deref() {
                Some("org") => IdMethod::Org,
                Some("ts") => IdMethod::Ts,
                _ => IdMethod::Uuid,
            },
            prefix: self.id_prefix.as_ref().and_then(serde_json::Value::as_str).map(str::to_string),
            ts_format: self.id_ts_format.clone().unwrap_or(defaults.ts_format),
            include_domain: self.id_include_domain.unwrap_or(defaults.include_domain),
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
            move_date_from_past_to_today: self
                .agenda_move_date_from_past_immediately_to_today
                .unwrap_or(defaults.move_date_from_past_to_today),
            insert: self.insert_settings(),
            special_ctrl_a_e: self.special_ctrl_a_e(),
            special_ctrl_k: self.special_ctrl_k.unwrap_or(false),
            shift_todo_is_state_change: self.treat_s_cursor_todo_selection_as_state_change.unwrap_or(true),
            timestamp_down_means_later: self.edit_timestamp_down_means_later.unwrap_or(false),
            priority_commands: self.priority_enable_commands.unwrap_or(true),
            ordered_tag: match &self.track_ordered_property_with_tag {
                Some(serde_json::Value::Bool(true)) => Some("ORDERED".to_string()),
                Some(serde_json::Value::String(tag)) if !tag.is_empty() => Some(tag.clone()),
                _ => None,
            },
        }
    }

    /// `org-special-ctrl-a/e` for C-a and for C-e: one value for both, or
    /// a pair.
    pub fn special_ctrl_a_e(&self) -> (org_core::line::Special, org_core::line::Special) {
        use org_core::line::Special;
        let one = |value: &serde_json::Value| match value {
            serde_json::Value::Bool(true) => Special::On,
            serde_json::Value::String(name) if name == "reversed" => Special::Reversed,
            _ => Special::Off,
        };
        match &self.special_ctrl_a_e {
            Some(serde_json::Value::Array(pair)) if pair.len() == 2 => (one(&pair[0]), one(&pair[1])),
            Some(value) => (one(value), one(value)),
            None => (Special::Off, Special::Off),
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
        settings.extend_today_until = self.extend_today_until.unwrap_or(0);
        settings.use_effective_time = self.use_effective_time.unwrap_or(false);
        settings.last_clock_out_as_effective_time = self.use_last_clock_out_time_as_effective_time.unwrap_or(false);
        settings.repeat_to_state = match &self.todo_repeat_to_state {
            Some(RepeatToOption::Flag(true)) => RepeatTo::Previous,
            Some(RepeatToOption::State(state)) => RepeatTo::State(state.clone()),
            _ => RepeatTo::Head,
        };
        settings.adapt = self.adapt();
        if let Some(format) = &self.property_format {
            settings.property_format = format.clone();
        }
        settings.log_done_with_time = self.log_done_with_time.unwrap_or(true);
        settings.states_order_reversed = self.log_states_order_reversed.unwrap_or(true);
        settings.notes_after_drawers = self.log_state_notes_insert_after_drawers.unwrap_or(false);
        settings.note_clock_out = self.log_note_clock_out.unwrap_or(false);
        settings.statistics = self.todo_statistics();
        settings.closed_keep_when_no_todo = self.closed_keep_when_no_todo.unwrap_or(false);
        settings.state_tags_triggers = self
            .todo_state_tags_triggers
            .iter()
            .flatten()
            .map(|(state, changes)| {
                let changes = changes
                    .iter()
                    .filter_map(|change| match change.split_at_checked(1) {
                        Some(("+", tag)) if !tag.is_empty() => Some((tag.to_string(), true)),
                        Some(("-", tag)) if !tag.is_empty() => Some((tag.to_string(), false)),
                        _ => None,
                    })
                    .collect();
                (state.clone(), changes)
            })
            .collect();
        settings.tags_sort = self.tag_settings().sort;
        settings.lists = self.list_settings();
        settings.table = self.table_settings();
        for (kind, heading) in self.log_note_headings.iter().flatten() {
            let headings = &mut settings.note_headings;
            let slot = match kind.as_str() {
                "done" => &mut headings.done,
                "state" => &mut headings.state,
                "note" => &mut headings.note,
                "reschedule" => &mut headings.reschedule,
                "delschedule" => &mut headings.delschedule,
                "redeadline" => &mut headings.redeadline,
                "deldeadline" => &mut headings.deldeadline,
                "refile" => &mut headings.refile,
                "clock_out" | "clock-out" => &mut headings.clock_out,
                _ => continue,
            };
            *slot = heading.clone();
        }
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
    ("startupShrinkAllTables", "org_startup_shrink_all_tables"),
    ("cycleHideBlockStartup", "org_cycle_hide_block_startup"),
    ("cycleHideDrawerStartup", "org_cycle_hide_drawer_startup"),
    ("cycleOpenArchivedTrees", "org_cycle_open_archived_trees"),
    ("adaptIndentation", "org_adapt_indentation"),
    ("startupIndented", "org_startup_indented"),
    ("hideLeadingStars", "org_hide_leading_stars"),
    ("srcPreserveIndentation", "org_src_preserve_indentation"),
    ("editSrcContentIndentation", "org_src_content_indentation"),
    // Org 9.8's name for `org-edit-src-content-indentation`.
    ("org_edit_src_content_indentation", "org_src_content_indentation"),
    ("virtualDocuments", "virtual_documents"),
    ("showDocument", "show_document"),
    ("messagePrompts", "message_prompts"),
    ("orgClient", "org_client"),
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
    ("inactiveLeader", "org_agenda_inactive_leader"),
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
            "org_capture_templates"
            | "org_roam_capture_templates"
            | "org_roam_dailies_capture_templates"
            | "org_refile_targets" => {
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
/// A face of `org-emphasis-alist` as a look: Org's faces of emphasis by
/// name, the attributes editors can show of a property list, or a list of
/// them. Colors and other faces add nothing.
fn emphasis_face(face: &serde_json::Value) -> crate::highlight::Emphasis {
    use crate::highlight::{modifier, Emphasis, TokenType};
    let mut look = Emphasis::NONE;
    let mut add = |name: &str| {
        let name = name.trim();
        if name.starts_with(':') {
            let words: Vec<&str> = name.split_whitespace().collect();
            for pair in words.chunks(2) {
                let on = pair.get(1).is_some_and(|value| !matches!(*value, "nil" | "normal"));
                look.modifiers |= match (pair[0], pair.get(1).copied()) {
                    (":weight", Some("bold" | "extra-bold" | "ultra-bold" | "semi-bold" | "heavy" | "black")) => {
                        modifier::BOLD
                    }
                    (":slant", Some("italic" | "oblique")) => modifier::ITALIC,
                    (":underline", _) if on => modifier::UNDERLINE,
                    (":strike-through", _) if on => modifier::STRIKETHROUGH,
                    _ => 0,
                };
            }
            return;
        }
        match name {
            "bold" => look.modifiers |= modifier::BOLD,
            "italic" => look.modifiers |= modifier::ITALIC,
            "bold-italic" => look.modifiers |= modifier::BOLD | modifier::ITALIC,
            "underline" => look.modifiers |= modifier::UNDERLINE,
            "org-code" => look.ty = look.ty.or(Some(TokenType::Code)),
            "org-verbatim" => look.ty = look.ty.or(Some(TokenType::Verbatim)),
            _ => {}
        }
    };
    match face {
        serde_json::Value::String(name) => add(name),
        serde_json::Value::Array(names) => names.iter().filter_map(serde_json::Value::as_str).for_each(&mut add),
        _ => {}
    }
    look
}

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
    #[serde(rename = "org_agenda_skip_comment_trees")]
    pub skip_comment_trees: Option<bool>,
    #[serde(rename = "org_agenda_skip_archived_trees")]
    pub skip_archived_trees: Option<bool>,
    #[serde(rename = "org_agenda_include_deadlines")]
    pub include_deadlines: Option<bool>,
    #[serde(rename = "org_agenda_skip_additional_timestamps_same_entry")]
    pub skip_additional_timestamps_same_entry: Option<bool>,
    /// `true`, `false` or `"cookies"`.
    #[serde(rename = "org_agenda_fontify_priorities")]
    pub fontify_priorities: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_log_mode_add_notes")]
    pub log_mode_add_notes: Option<bool>,
    #[serde(rename = "org_agenda_clock_report_header")]
    pub clock_report_header: Option<String>,
    /// The text of a plist (`:link t :maxlevel 2`).
    #[serde(rename = "org_agenda_clockreport_parameter_plist")]
    pub clockreport_parameter_plist: Option<String>,
    #[serde(rename = "org_agenda_time_leading_zero")]
    pub time_leading_zero: Option<bool>,
    #[serde(rename = "org_agenda_timegrid_use_ampm")]
    pub timegrid_use_ampm: Option<bool>,
    #[serde(rename = "org_agenda_search_headline_for_time")]
    pub search_headline_for_time: Option<bool>,
    /// `true`, `false` or `"beg"`.
    #[serde(rename = "org_agenda_remove_times_when_in_prefix")]
    pub remove_times_when_in_prefix: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_default_appointment_duration")]
    pub default_appointment_duration: Option<i64>,
    #[serde(rename = "org_agenda_remove_timeranges_from_blocks")]
    pub remove_timeranges_from_blocks: Option<bool>,
    #[serde(rename = "org_agenda_diary_sexp_prefix")]
    pub diary_sexp_prefix: Option<String>,
    /// `true`, `"always"`, `false` or a list of kinds of views.
    #[serde(rename = "org_agenda_show_inherited_tags")]
    pub show_inherited_tags: Option<serde_json::Value>,
    /// `true` or a list of kinds of views.
    #[serde(rename = "org_agenda_use_tag_inheritance")]
    pub use_tag_inheritance: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_hide_tags_regexp")]
    pub hide_tags_regexp: Option<String>,
    /// `org-agenda-prefer-last-repeat`: `true`, or a list of TODO keywords.
    #[serde(rename = "org_agenda_prefer_last_repeat")]
    pub prefer_last_repeat: Option<serde_json::Value>,
    /// `true`, `"pre-scheduled"` or a number of days.
    #[serde(rename = "org_agenda_skip_deadline_prewarning_if_scheduled")]
    pub skip_deadline_prewarning_if_scheduled: Option<serde_json::Value>,
    /// `true`, `"post-deadline"` or a number of days.
    #[serde(rename = "org_agenda_skip_scheduled_delay_if_deadline")]
    pub skip_scheduled_delay_if_deadline: Option<serde_json::Value>,
    /// `true`, `"not-today"` or `"repeated-after-deadline"`.
    #[serde(rename = "org_agenda_skip_scheduled_if_deadline_is_shown")]
    pub skip_scheduled_if_deadline_is_shown: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_skip_scheduled_repeats_after_deadline")]
    pub skip_scheduled_repeats_after_deadline: Option<bool>,
    #[serde(rename = "org_agenda_todo_list_sublevels")]
    pub todo_list_sublevels: Option<bool>,
    #[serde(rename = "org_agenda_todo_ignore_with_date")]
    pub todo_ignore_with_date: Option<bool>,
    /// `org-agenda-todo-ignore-scheduled` and the two after it: `true`, a
    /// name or a number of days.
    #[serde(rename = "org_agenda_todo_ignore_scheduled")]
    pub todo_ignore_scheduled: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_todo_ignore_deadlines")]
    pub todo_ignore_deadlines: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_todo_ignore_timestamp")]
    pub todo_ignore_timestamp: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_todo_ignore_time_comparison_use_seconds")]
    pub todo_ignore_time_comparison_use_seconds: Option<bool>,
    #[serde(rename = "org_agenda_tags_todo_honor_ignore_options")]
    pub tags_todo_honor_ignore_options: Option<bool>,
    /// `org-tags-match-list-sublevels`: `true`, `false` or `"indented"`.
    #[serde(rename = "org_tags_match_list_sublevels")]
    pub tags_match_list_sublevels: Option<serde_json::Value>,
    /// `org-agenda-sorting-strategy`: a list of strategies for each kind
    /// of view (`agenda`, `todo`, `tags`, `search`), or one list for all.
    #[serde(rename = "org_agenda_sorting_strategy")]
    pub sorting_strategy: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_sort_notime_is_late")]
    pub sort_notime_is_late: Option<bool>,
    #[serde(rename = "org_agenda_sort_noeffort_is_high")]
    pub sort_noeffort_is_high: Option<bool>,
    /// `org-agenda-max-entries` and its kin: a number for every kind of
    /// view, or one for each kind.
    #[serde(rename = "org_agenda_max_entries")]
    pub max_entries: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_max_todos")]
    pub max_todos: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_max_tags")]
    pub max_tags: Option<serde_json::Value>,
    #[serde(rename = "org_agenda_max_effort")]
    pub max_effort: Option<serde_json::Value>,
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
    #[serde(rename = "org_agenda_show_outline_path")]
    pub show_outline_path: Option<OutlinePathOption>,
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
    #[serde(rename = "org_super_agenda_mode")]
    pub super_agenda_mode: Option<bool>,
    #[serde(rename = "org_super_agenda_groups")]
    pub super_agenda_groups: Option<Vec<serde_json::Value>>,
    #[serde(rename = "org_super_agenda_unmatched_name")]
    pub super_agenda_unmatched_name: Option<String>,
    #[serde(rename = "org_super_agenda_unmatched_order")]
    pub super_agenda_unmatched_order: Option<i64>,
    #[serde(rename = "org_super_agenda_header_separator")]
    pub super_agenda_header_separator: Option<String>,
    #[serde(rename = "org_super_agenda_header_prefix")]
    pub super_agenda_header_prefix: Option<String>,
    #[serde(rename = "org_super_agenda_final_group_separator")]
    pub super_agenda_final_group_separator: Option<String>,
    #[serde(rename = "org_super_agenda_date_format")]
    pub super_agenda_date_format: Option<String>,
    #[serde(rename = "org_super_agenda_hide_empty_groups")]
    pub super_agenda_hide_empty_groups: Option<bool>,
    #[serde(rename = "org_super_agenda_keep_order")]
    pub super_agenda_keep_order: Option<bool>,
    #[serde(rename = "org_super_agenda_group_property_name")]
    pub super_agenda_group_property_name: Option<String>,
    #[serde(rename = "org_super_agenda_properties_inherit")]
    pub super_agenda_properties_inherit: Option<bool>,
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
    /// `org-agenda-deadline-faces`: `[part, face]` pairs.
    #[serde(rename = "org_agenda_deadline_faces")]
    pub deadline_faces: Option<Vec<(f64, String)>>,
    /// `org-agenda-timerange-leaders`
    #[serde(rename = "org_agenda_timerange_leaders")]
    pub timerange_leaders: Option<Vec<String>>,
    /// `org-agenda-inactive-leader`
    #[serde(rename = "org_agenda_inactive_leader")]
    pub inactive_leader: Option<String>,
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
            skip_comment_trees,
            skip_archived_trees,
            include_deadlines,
            skip_additional_timestamps_same_entry,
            log_mode_add_notes,
            remove_timeranges_from_blocks,
            skip_scheduled_repeats_after_deadline,
            todo_list_sublevels,
            use_time_grid,
            show_current_time_in_grid,
            compact_blocks,
            hide_empty_blocks
        );
        if let Some(on) = self.time_leading_zero {
            config.times.leading_zero = on;
        }
        if let Some(on) = self.timegrid_use_ampm {
            config.times.ampm = on;
        }
        if let Some(on) = self.search_headline_for_time {
            config.times.headline_has_none = !on;
        }
        if let Some(value) = &self.remove_times_when_in_prefix {
            config.times.remove_in_prefix = match value {
                serde_json::Value::Bool(false) | serde_json::Value::Null => RemoveTimes::No,
                serde_json::Value::String(symbol) if symbol == "beg" => RemoveTimes::Beginning,
                _ => RemoveTimes::Yes,
            };
        }
        if self.default_appointment_duration.is_some() {
            config.times.appointment_duration = self.default_appointment_duration;
        }
        // A list of kinds of views (`agenda`, `todo`, `search`).
        let views = |value: &serde_json::Value| -> Vec<String> {
            value.as_array().into_iter().flatten().filter_map(|view| view.as_str().map(str::to_string)).collect()
        };
        if let Some(value) = &self.show_inherited_tags {
            config.show_inherited_tags = match value {
                serde_json::Value::Bool(true) => ShowInheritedTags::Yes,
                serde_json::Value::String(symbol) if symbol == "always" => ShowInheritedTags::Always,
                serde_json::Value::Array(_) => ShowInheritedTags::Views(views(value)),
                _ => ShowInheritedTags::No,
            };
        }
        if let Some(value) = &self.use_tag_inheritance {
            config.use_tag_inheritance = match value {
                serde_json::Value::Bool(true) => UseTagInheritance::All,
                _ => UseTagInheritance::Views(views(value)),
            };
        }
        if let Some(value) = &self.fontify_priorities {
            config.fontify_priorities = match value {
                serde_json::Value::Bool(false) | serde_json::Value::Null => FontifyPriorities::No,
                serde_json::Value::String(symbol) if symbol == "cookies" => FontifyPriorities::Cookies,
                _ => FontifyPriorities::Lines,
            };
        }
        if let Some(plist) = &self.clockreport_parameter_plist {
            config.clockreport_parameters = plist.clone();
        }
        if self.clock_report_header.is_some() {
            config.clock_report_header = self.clock_report_header.clone();
        }
        if self.hide_tags_regexp.is_some() {
            config.hide_tags_regexp = self.hide_tags_regexp.clone().filter(|regexp| !regexp.is_empty());
        }
        if self.diary_sexp_prefix.is_some() {
            config.diary_sexp_prefix = self.diary_sexp_prefix.clone().filter(|prefix| !prefix.is_empty());
        }
        if let Some(value) = &self.prefer_last_repeat {
            config.prefer_last_repeat = match value {
                serde_json::Value::Bool(true) => PreferLastRepeat::All,
                serde_json::Value::Array(states) => PreferLastRepeat::States(
                    states.iter().filter_map(|state| state.as_str().map(str::to_string)).collect(),
                ),
                _ => PreferLastRepeat::None,
            };
        }
        // `t`, the symbol of the other date, or a number of days.
        let by_other_date = |value: &serde_json::Value, name: &str| match value {
            serde_json::Value::Bool(true) => Some(SkipByOtherDate::All),
            serde_json::Value::String(symbol) if symbol == name => Some(SkipByOtherDate::ByOtherDate),
            serde_json::Value::String(_) => Some(SkipByOtherDate::All),
            serde_json::Value::Number(number) => number.as_i64().map(SkipByOtherDate::Days),
            _ => None,
        };
        if let Some(value) = &self.skip_deadline_prewarning_if_scheduled {
            config.skip_deadline_prewarning_if_scheduled = by_other_date(value, "pre-scheduled");
        }
        if let Some(value) = &self.skip_scheduled_delay_if_deadline {
            config.skip_scheduled_delay_if_deadline = by_other_date(value, "post-deadline");
        }
        if let Some(value) = &self.skip_scheduled_if_deadline_is_shown {
            config.skip_scheduled_if_deadline_is_shown = match value.as_str() {
                _ if value == &serde_json::Value::Bool(true) => SkipScheduledIfDeadline::Yes,
                Some("not-today") => SkipScheduledIfDeadline::NotToday,
                Some("repeated-after-deadline") => SkipScheduledIfDeadline::RepeatedAfterDeadline,
                _ => SkipScheduledIfDeadline::No,
            };
        }
        // `org-agenda-todo-ignore-*`: `false` is nil, as unset.
        let when = |value: &Option<serde_json::Value>| value.as_ref().and_then(When::parse);
        if let Some(on) = self.todo_ignore_with_date {
            config.todo_ignore.with_date = on;
        }
        if self.todo_ignore_scheduled.is_some() {
            config.todo_ignore.scheduled = when(&self.todo_ignore_scheduled);
        }
        if self.todo_ignore_deadlines.is_some() {
            config.todo_ignore.deadlines = when(&self.todo_ignore_deadlines);
        }
        if self.todo_ignore_timestamp.is_some() {
            config.todo_ignore.timestamp = when(&self.todo_ignore_timestamp);
        }
        if let Some(on) = self.todo_ignore_time_comparison_use_seconds {
            config.todo_ignore.use_seconds = on;
        }
        config.tags_match_list_sublevels = match &self.tags_match_list_sublevels {
            Some(serde_json::Value::Bool(false)) => org_core::agenda::Sublevels::Skipped,
            Some(serde_json::Value::String(name)) if name == "indented" => org_core::agenda::Sublevels::Indented,
            _ => org_core::agenda::Sublevels::Listed,
        };
        if let Some(on) = self.tags_todo_honor_ignore_options {
            config.tags_todo_honor_ignore = on;
        }
        if let Some(on) = self.search_view_always_boolean {
            config.search.always_boolean = on;
        }
        // `org-agenda-sorting-strategy`: a list for each kind of view, or
        // one list for all of them.
        let names = |value: &serde_json::Value| -> Option<Vec<String>> {
            value.as_array()?.iter().map(|name| name.as_str().map(str::to_string)).collect()
        };
        match &self.sorting_strategy {
            Some(serde_json::Value::Object(views)) => {
                let lists = views.iter().filter_map(|(view, list)| Some((view.clone(), names(list)?))).collect();
                config.sorting.strategies = Strategies::ByView(lists);
            }
            Some(list) => {
                if let Some(list) = names(list) {
                    config.sorting.strategies = Strategies::All(list);
                }
            }
            None => {}
        }
        if let Some(on) = self.sort_notime_is_late {
            config.sorting.notime_is_late = on;
        }
        if let Some(on) = self.sort_noeffort_is_high {
            config.sorting.noeffort_is_high = on;
        }
        // A number for every kind of view, or one for each kind.
        let limit = |value: &Option<serde_json::Value>| match value {
            Some(serde_json::Value::Number(number)) => number.as_i64().map(Limit::All),
            Some(serde_json::Value::Object(views)) => Some(Limit::ByView(
                views.iter().filter_map(|(view, limit)| Some((view.clone(), limit.as_i64()?))).collect(),
            )),
            _ => None,
        };
        config.sorting.max_entries = limit(&self.max_entries);
        config.sorting.max_todos = limit(&self.max_todos);
        config.sorting.max_tags = limit(&self.max_tags);
        config.sorting.max_effort = limit(&self.max_effort);
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
        // org-super-agenda, when `org-super-agenda-mode` is on.
        if self.super_agenda_mode == Some(true) {
            use org_core::agenda::super_agenda::{Separator, Settings};
            // One character is repeated across the window, as `?-` is.
            let separator = |text: &str| {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Separator::Char(c),
                    _ => Separator::Text(text.to_string()),
                }
            };
            let defaults = Settings::default();
            config.super_agenda = Some(Settings {
                groups: self.super_agenda_groups.clone().unwrap_or_default(),
                unmatched_name: self.super_agenda_unmatched_name.clone().unwrap_or(defaults.unmatched_name),
                unmatched_order: self.super_agenda_unmatched_order.unwrap_or(defaults.unmatched_order),
                header_separator: self
                    .super_agenda_header_separator
                    .as_deref()
                    .map_or(defaults.header_separator, separator),
                header_prefix: self.super_agenda_header_prefix.clone().unwrap_or(defaults.header_prefix),
                final_group_separator: self
                    .super_agenda_final_group_separator
                    .as_deref()
                    .map_or(defaults.final_group_separator, separator),
                date_format: self.super_agenda_date_format.clone().unwrap_or(defaults.date_format),
                hide_empty_groups: self.super_agenda_hide_empty_groups.unwrap_or(defaults.hide_empty_groups),
                keep_order: self.super_agenda_keep_order.unwrap_or(defaults.keep_order),
                group_property_name: self
                    .super_agenda_group_property_name
                    .clone()
                    .unwrap_or(defaults.group_property_name),
                properties_inherit: self.super_agenda_properties_inherit.unwrap_or(defaults.properties_inherit),
            });
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
        // `org-agenda-deadline-faces`: Org's faces by name, `org-warning`
        // as `org-imminent-deadline`, which inherits it; any other is the
        // plain look.
        if let Some(faces) = &self.deadline_faces {
            use org_core::agenda::Face;
            config.deadline_faces = faces
                .iter()
                .map(|(part, name)| {
                    let face = match name.as_str() {
                        "org-imminent-deadline" | "org-warning" => Face::DeadlineWarning,
                        "org-upcoming-deadline" => Face::DeadlineUpcoming,
                        "org-upcoming-distant-deadline" => Face::DeadlineDistant,
                        _ => Face::Default,
                    };
                    (*part, face)
                })
                .collect();
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
        if let Some(leader) = &self.inactive_leader {
            config.inactive_leader = leader.clone();
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

    /// `org-entities-user` as the server's Emacs is told it.
    #[test]
    fn user_entities_for_emacs() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "org_entities_user": [["myarrow", "\\to", true, "&rarr;", "->", "\"q\"", "→"]]
        }))
        .unwrap();
        assert_eq!(options.user_entities(), [("myarrow".to_string(), "→".to_string())]);
        assert_eq!(
            options.user_entities_lisp().unwrap(),
            r#"(setq org-entities-user '(("myarrow" "\\to" t "&rarr;" "->" "\"q\"" "→")))"#
        );
    }

    /// `org-list-allow-alphabetical` and
    /// `org-plain-list-ordered-item-terminator`, for the parser and the
    /// server's Emacs.
    #[test]
    fn list_syntax_from_the_options() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "org_list_allow_alphabetical": true,
            "org_plain_list_ordered_item_terminator": ")"
        }))
        .unwrap();
        assert_eq!(
            options.list_syntax(),
            org_syntax::ListSyntax { alphabetical: true, terminator: org_syntax::Terminator::Paren }
        );
        assert_eq!(
            options.emacs_lisp().unwrap(),
            "(setq org-list-allow-alphabetical t org-plain-list-ordered-item-terminator ?\\))\n\
             (when (featurep 'org-element) (org-element-update-syntax))"
        );
        assert_eq!(Options::default().emacs_lisp(), None);
    }

    /// The tangling options, for org-core and as the server's Emacs is
    /// told them.
    #[test]
    fn tangle_options() {
        let options: Options = serde_json::from_value(serde_json::json!({
            "org_babel_tangle_lang_exts": { "python": "py" },
            "org_babel_tangle_use_relative_file_links": false,
            "org_babel_tangle_comment_format_end": "end \"%source-name\"",
            "org_babel_process_comment_text": "identity",
            "org_babel_tangle_default_file_mode": "600",
            "org_babel_tangle_remove_file_before_write": true,
            "org_babel_noweb_wrap_start": "{{"
        }))
        .unwrap();
        let settings = options.tangle_settings();
        assert_eq!(settings.lang_exts, [("python".to_string(), "py".to_string())]);
        assert!(!settings.relative_links);
        assert_eq!(settings.comment_format.0, "[[%link][%source-name]]");
        assert_eq!(settings.default_file_mode, 0o600);
        assert_eq!(settings.noweb_wrap, ("{{".to_string(), ">>".to_string()));
        assert_eq!(options.tangle_remove_file_before_write(), Some(true));
        assert_eq!(
            options.emacs_lisp().unwrap(),
            [
                r#"(setq org-babel-tangle-lang-exts (cons (cons "python" "py") org-babel-tangle-lang-exts))"#,
                "(setq org-babel-tangle-use-relative-file-links nil)",
                r#"(setq org-babel-tangle-comment-format-end "end \"%source-name\"")"#,
                "(setq org-babel-process-comment-text 'identity)",
                "(setq org-babel-tangle-default-file-mode #o600)",
                "(setq org-babel-tangle-remove-file-before-write t)",
                r#"(setq org-babel-noweb-wrap-start "{{")"#,
            ]
            .join("\n")
        );
    }

    /// `org-todo-interpretation`: a list of keywords alone is a sequence,
    /// or with `type` one of types, each going straight to done.
    #[test]
    fn keywords_alone_are_read_as_org_todo_interpretation_says() {
        let options = |interpretation: Option<&str>| -> Options {
            let mut value = serde_json::json!({ "org_todo_keywords": ["Fred", "Sara", "|", "DONE"] });
            if let Some(interpretation) = interpretation {
                value["org_todo_interpretation"] = serde_json::json!(interpretation);
            }
            serde_json::from_value(value).unwrap()
        };
        assert_eq!(options(None).todo_keywords().cycle(Some("Fred"), None, false), Some("Sara"));
        assert_eq!(options(Some("type")).todo_keywords().cycle(Some("Fred"), None, false), Some("DONE"));
        // A sequence that says what it is keeps it.
        let typed: Options = serde_json::from_value(serde_json::json!({
            "org_todo_keywords": ["sequence", "Fred", "Sara", "|", "DONE"],
            "org_todo_interpretation": "type"
        }))
        .unwrap();
        assert_eq!(typed.todo_keywords().cycle(Some("Fred"), None, false), Some("Sara"));
    }

    #[test]
    fn emphasis_looks_as_org_emphasis_alist_gives_them() {
        use crate::highlight::{modifier, Emphasis, TokenType};
        let options: Options = serde_json::from_value(serde_json::json!({
            "org_emphasis_alist": {
                "*": ["bold", "italic"],
                "/": ":slant italic :foreground red",
                "~": "org-verbatim",
                "+": false
            }
        }))
        .unwrap();
        let looks = options.fontify().emphasis.unwrap();
        assert_eq!(looks[0], Emphasis { ty: None, modifiers: modifier::BOLD | modifier::ITALIC });
        assert_eq!(looks[1], Emphasis { ty: None, modifiers: modifier::ITALIC });
        // Left out: no look, as in Emacs.
        assert_eq!(looks[2], Emphasis::NONE);
        assert_eq!(looks[4], Emphasis { ty: Some(TokenType::Verbatim), modifiers: 0 });
        assert_eq!(looks[5], Emphasis::NONE);
        let off: Options = serde_json::from_value(
            serde_json::json!({ "org_fontify_emphasized_text": false, "org_hide_emphasis_markers": true }),
        )
        .unwrap();
        assert_eq!(off.fontify().emphasis, None);
        assert!(!off.conceal_emphasis());
        let hidden: Options =
            serde_json::from_value(serde_json::json!({ "org_hidden_keywords": ["author", "title"] })).unwrap();
        assert_eq!(hidden.fontify().hidden_keywords, 0b101);
    }

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
        assert_eq!(dropped, ["org_agenda_files", "org_deadline_warning_days", "org_agenda_time_grid"]);
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

    #[test]
    fn super_agenda_only_when_turned_on() {
        let text = "org_super_agenda_header_separator = \"=\"\n\n[[org_super_agenda_groups]]\nname = \"Today\"\ntime-grid = true\ntodo = \"NEXT\"\n";
        let mut value = parse_config(std::path::Path::new("config.toml"), text).unwrap();
        let options = |value: &serde_json::Value| serde_json::from_value::<AgendaOptions>(value.clone()).unwrap();
        // Groups alone do nothing.
        assert_eq!(options(&value).to_config().super_agenda, None);
        value["org_super_agenda_mode"] = serde_json::json!(true);
        let settings = options(&value).to_config().super_agenda.unwrap();
        // The selectors keep their order, as a plist does.
        let keys: Vec<&String> = settings.groups[0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["name", "time-grid", "todo"]);
        assert_eq!(settings.header_separator, org_core::agenda::super_agenda::Separator::Char('='));
        assert_eq!(settings.unmatched_name, "Other items");
    }
}

/// `org-use-fast-todo-selection`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastTodo {
    /// `nil`: the next state, always.
    Never,
    /// `auto`: by key where keywords have keys.
    Auto,
    /// `expert`: by key, the keys in the prompt alone.
    Expert,
}
