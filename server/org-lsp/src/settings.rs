//! The server's options, with their types, defaults and meanings.
//!
//! This table is the single source for the documentation of the options:
//! `org-lsp settings` prints it, and the configuration pages of each
//! editor and the options in `docs/protocol.org` are written from it. A test checks
//! that the defaults written here are the ones `config` applies.

use serde_json::{json, Value};

pub struct Setting {
    /// The option's name; `org_agenda_span` is `span` inside `agenda`.
    pub name: &'static str,
    /// The values it takes, in JSON's terms.
    pub kind: &'static str,
    /// The default as JSON, or `None` when the default depends on
    /// something else; `unset` then says what applies.
    pub default: Option<&'static str>,
    pub unset: &'static str,
    /// The Org variable it stands for, if any.
    pub org: &'static str,
    pub doc: &'static str,
    /// An option for editor integrations rather than for users.
    pub integration: bool,
    /// An option an editor gives, which the shared configuration file
    /// does not.
    pub editor_only: bool,
}

const fn setting(
    name: &'static str,
    kind: &'static str,
    default: &'static str,
    org: &'static str,
    doc: &'static str,
) -> Setting {
    Setting { name, kind, default: Some(default), unset: "", org, doc, integration: false, editor_only: false }
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        default: None,
        unset: "the shared file, `~/.config/org-lsp/config.toml` or `config.json`",
        editor_only: true,
        ..setting(
            "config_file",
            "false, or a path",
            "",
            "",
            "The shared configuration file, whose options this editor's go on top of: `false` for none, or \
             the path of another file (JSON if it ends in `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, \
             else `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`; \
             `%APPDATA%\\org-lsp` on Windows).",
        )
    },
    setting(
        "org_agenda_files",
        "string[]",
        "[]",
        "org-agenda-files",
        "Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built from. `~/` is the home \
         directory, and a directory stands for the Org files directly in it. When empty, the agenda is built \
         from the Org files under the workspace folders (hidden directories, `node_modules` and `target` left \
         out), or else from the Org files that are open.",
    ),
    setting(
        "org_todo_keywords",
        "string[] | string[][]",
        r#"["TODO", "|", "DONE"]"#,
        "org-todo-keywords",
        "The TODO sequence, written like Org's `#+TODO:` line. `|` separates the states that count as done; \
         without it the last keyword is the only done state. `TODO(t)` gives a keyword a key for fast \
         selection, and `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering and leaving it. \
         Several sequences go in a list of lists, `[[\"TODO\", \"|\", \"DONE\"], [\"REPORT\", \"|\", \"FIXED\"]]`; \
         cycling stays in the sequence of the entry, and `C-S-<right>` switches to the next. A list that starts \
         with `type` is a type sequence (`#+TYP_TODO:`): each of its keywords goes straight to done. A file's \
         own `#+TODO:` lines take precedence.",
    ),
    setting(
        "org_todo_keyword_faces",
        "{ [keyword]: string }",
        "{}",
        "org-todo-keyword-faces",
        "Faces for particular TODO keywords: a color, `{ \"WAIT\": \"orange\" }`, or attributes written as Org \
         writes them, `{ \"CANCELLED\": \":foreground gray :slant italic :strike-through t\" }` (`:foreground`, \
         `:background`, `:weight bold`, `:slant italic`, `:underline t`, `:strike-through t`). Shown in Org files \
         and in the agenda by Neovim, Vim and VS Code.",
    ),
    setting(
        "org_priority_faces",
        "{ [priority]: string }",
        "{}",
        "org-priority-faces",
        "Faces for particular priorities, by their letter, written as in `org_todo_keyword_faces`: \
         `{ \"A\": \":foreground red :weight bold\" }`.",
    ),
    setting(
        "org_tag_faces",
        "{ [tag]: string }",
        "{}",
        "org-tag-faces",
        "Faces for particular tags of headlines, written as in `org_todo_keyword_faces`: \
         `{ \"work\": \"orange\" }`. Shown in Org files (not the agenda, as in Emacs) by Neovim, Vim and VS Code.",
    ),
    setting(
        "org_log_done",
        r#"false, "time", "note""#,
        "false",
        "org-log-done",
        "Write `CLOSED:` with the time when an entry is done, and with `note` ask for a note too. A file's \
         `#+STARTUP:` words `logdone`, `lognotedone` and `nologdone` change it there; an entry's `LOGGING` \
         property (inherited) says what its state changes log instead: those words, `logrepeat` and its \
         relatives, and keywords with what they log (`WAIT(@/!)`), or `nil` for nothing.",
    ),
    setting(
        "org_log_repeat",
        r#"false, "time", "note""#,
        r#""time""#,
        "org-log-repeat",
        "Record when a repeating entry is done and moves to its next date. `#+STARTUP:` `logrepeat`, \
         `lognoterepeat` and `nologrepeat` change it in a file, and the `LOGGING` property in an entry.",
    ),
    setting(
        "org_M_RET_may_split_line",
        "boolean, object",
        "true",
        "org-M-RET-may-split-line",
        "Inserting a heading, an item or a table row (M-RET) in the middle of a line splits it, the rest going \
         to the new one; `false`: it goes in after the line. One by context: `{ \"headline\": false, \"item\": \
         true, \"table\": true }`, `default` for those not named.",
    ),
    Setting {
        default: None,
        unset: "`auto` for both",
        ..setting(
            "org_blank_before_new_entry",
            "object",
            "",
            "org-blank-before-new-entry",
            "Whether a new heading (`heading`) and a new item (`plain_list_item`) get an empty line before them: \
             `true`, `false`, or `\"auto\"`, as the headings or items around them have.",
        )
    },
    setting(
        "org_insert_heading_respect_content",
        "boolean",
        "false",
        "org-insert-heading-respect-content",
        "M-RET and M-S-RET outside lists put the new heading after the subtree, as C-RET does.",
    ),
    setting(
        "org_special_ctrl_a_e",
        r#"false, true, "reversed", [C-a, C-e]"#,
        "false",
        "org-special-ctrl-a/e",
        "How C-a and C-e (`org.beginning.of.line`, `org.end.of.line`) move on a headline or an item: `true` \
         to the start of the headline's title, after the stars, TODO keyword and priority, or after the \
         item's bullet and check-box, and to the end of the title, before the tags; the same key again to \
         the line's own start or end. `reversed` goes to the line's start or end first, and to those \
         places on the same key right after. A pair, as `[true, false]`, sets C-a and C-e apart. With \
         `false` the key keeps the editor's own meaning.",
    ),
    setting(
        "org_sparse_tree_open_archived_trees",
        "boolean",
        "false",
        "org-sparse-tree-open-archived-trees",
        "A sparse tree (`C-c /`, and a link's `/regexp/`) shows its matches in archived subtrees (tagged \
         `ARCHIVE`); with `false` those subtrees stay folded under their headline.",
    ),
    setting(
        "org_yank_adjusted_subtrees",
        "boolean",
        "false",
        "org-yank-adjusted-subtrees",
        "C-y (`org.yank`, Emacs keys) of one or more subtrees at the start of a line, or after the stars of \
         an otherwise empty headline, pastes them as `org.paste.subtree` does: at the level of the headlines \
         around them. Anything else is the editor's own paste.",
    ),
    setting(
        "org_allow_promoting_top_level_subtree",
        "boolean",
        "false",
        "org-allow-promoting-top-level-subtree",
        "Promoting a headline of level 1 makes it a comment line: `# ` in place of `* `, its children a level \
         up with a subtree's promotion. With `false` it is an error.",
    ),
    setting(
        "org_special_ctrl_k",
        "boolean",
        "false",
        "org-special-ctrl-k",
        "C-k (`org.kill.line`) on a headline kills the text to the tags, and the tags when the cursor is \
         past the title; at the start of the line, the whole line. With `false` the key keeps the editor's \
         own meaning.",
    ),
    setting(
        "org_list_two_spaces_after_bullet_regexp",
        "false, string",
        "false",
        "org-list-two-spaces-after-bullet-regexp",
        "An Emacs regexp: the bullets it matches get two spaces after them when Org writes them (a new item, \
         a bullet changed, a list repaired).",
    ),
    setting(
        "org_list_demote_modify_bullet",
        "object",
        "{}",
        "org-list-demote-modify-bullet",
        "By bullet (`-`, `+`, `*`, `1.`, `1)`), the bullet an item gets when it is indented: \
         `{ \"+\": \"-\", \"-\": \"+\" }`.",
    ),
    setting(
        "org_list_indent_offset",
        "integer",
        "0",
        "org-list-indent-offset",
        "How many more columns sub-items are indented, past their parent's bullet.",
    ),
    setting(
        "org_list_automatic_rules",
        "object",
        r#"{ "checkbox": true, "indent": true }"#,
        "org-list-automatic-rules",
        "`checkbox`: the statistics cookies are counted again when a box changes, and colored as done or \
         not. `indent`: indenting the first item of a list with its children (M-S-<right>) moves the whole \
         list. A rule left out of the object is off.",
    ),
    setting(
        "org_checkbox_hierarchical_statistics",
        "boolean",
        "true",
        "org-checkbox-hierarchical-statistics",
        "A checkbox cookie counts the boxes of the items right below it; `false` counts every box further \
         down too, as `recursive` in a `COOKIE_DATA` property does.",
    ),
    Setting {
        default: None,
        unset: "Org's: integers, decimals, `2.4e-08`, `0x1F`, `nan`, `inf` and their relatives",
        ..setting(
            "org_table_number_regexp",
            "string",
            "",
            "org-table-number-regexp",
            "An Emacs regexp: the fields it matches are numbers, and a column mostly of numbers aligns right.",
        )
    },
    setting(
        "org_table_number_fraction",
        "number",
        "0.5",
        "org-table-number-fraction",
        "How much of a column's filled fields must be numbers for it to align right.",
    ),
    setting(
        "org_table_automatic_realign",
        "boolean",
        "true",
        "org-table-automatic-realign",
        "TAB and RET in a table align it before they move; `false`: only C-c C-c and the commands that change \
         rows or columns do.",
    ),
    setting(
        "org_table_tab_jumps_over_hlines",
        "boolean",
        "true",
        "org-table-tab-jumps-over-hlines",
        "TAB at the end of a row before a horizontal line goes to the row after it; `false`: it makes a row \
         before the line.",
    ),
    setting(
        "org_table_shrunk_column_indicator",
        "string",
        r#""…""#,
        "org-table-shrunk-column-indicator",
        "What shows at the end of each field of a shrunk column.",
    ),
    setting(
        "org_table_formula_constants",
        "object",
        "{}",
        "org-table-formula-constants",
        "Constants for the table formulas of every file, by name: `{ \"c\": \"299792458.\" }` gives `$c`. A \
         file's `#+CONSTANTS:` come first.",
    ),
    setting(
        "org_table_relative_ref_may_cross_hline",
        r#"true, false, "error""#,
        "true",
        "org-table-relative-ref-may-cross-hline",
        "Whether a relative reference in a formula (`@-1`) goes past a horizontal line: `false` stops at the \
         field next to it, `\"error\"` makes the formula fail.",
    ),
    setting(
        "org_table_formula_create_columns",
        "boolean",
        "false",
        "org-table-formula-create-columns",
        "A field formula past the last column adds the columns it needs, rather than failing.",
    ),
    setting(
        "org_table_duration_custom_format",
        r#""seconds", "minutes", "hours", "days", "hh:mm""#,
        r#""hours""#,
        "org-table-duration-custom-format",
        "How a formula with `;t` writes a duration: a number of seconds, minutes, hours or days, or hours and \
         minutes.",
    ),
    setting(
        "org_table_duration_hour_zero_padding",
        "boolean",
        "true",
        "org-table-duration-hour-zero-padding",
        "Durations of formulas (`;T`, `;U`) have their hours on two digits: `08:32:34` rather than `8:32:34`.",
    ),
    setting(
        "org_calc_default_modes",
        "object",
        r#"{ "float_format": ["float", 8], "angle_mode": "deg" }"#,
        "org-calc-default-modes",
        "The modes table formulas start with: `float_format`, `[\"float\", digits]`, `[\"fix\", decimals]` or \
         `[\"sci\", digits]`, and `angle_mode`, `\"deg\"` or `\"rad\"`. A formula's own modes (`;f2`) win; \
         Calc's other modes are not offered.",
    ),
    setting(
        "org_table_formula_evaluate_inline",
        "boolean",
        "true",
        "org-table-formula-evaluate-inline",
        "TAB and RET on a field that is a formula (`=$1*2`, `:=...`) evaluate it, which org-lsp refuses (the \
         formula goes in a `#+TBLFM:` line); `false` leaves the field as text.",
    ),
    setting(
        "org_table_allow_automatic_line_recalculation",
        "boolean",
        "true",
        "org-table-allow-automatic-line-recalculation",
        "TAB, RET and C-c C-c on a line marked `#` recalculate it.",
    ),
    setting(
        "org_list_use_circular_motion",
        "boolean",
        "false",
        "org-list-use-circular-motion",
        "Moving to the next item from the last goes to the first, and back; moving the last item down puts \
         it first, the first up puts it last.",
    ),
    setting(
        "org_enforce_todo_dependencies",
        "boolean",
        "false",
        "org-enforce-todo-dependencies",
        "An entry cannot be done while a child is not, or, under a parent with the `ORDERED` property, while \
         an earlier sibling is not: the change is refused with what blocks it.",
    ),
    setting(
        "org_agenda_dim_blocked_tasks",
        r#"true, false, "invisible""#,
        "true",
        "org-agenda-dim-blocked-tasks",
        "Dim the tasks of the agenda and the TODO list that their dependencies keep from being done (with \
         `org_enforce_todo_dependencies` or `org_enforce_todo_checkbox_dependencies`); `invisible` leaves them \
         out instead, except those blocked by checkboxes, which stay dimmed.",
    ),
    setting(
        "org_enforce_todo_checkbox_dependencies",
        "boolean",
        "false",
        "org-enforce-todo-checkbox-dependencies",
        "An entry cannot be done while it has an unchecked box.",
    ),
    setting(
        "org_log_refile",
        r#"false, "time", "note""#,
        "false",
        "org-log-refile",
        "Record when an entry is refiled (`Refiled on [2026-05-18 Mon 10:30]`), with `note` asking for a note \
         too. `#+STARTUP:` `logrefile`, `lognoterefile` and `nologrefile` change it in the file refiled to.",
    ),
    setting(
        "org_log_reschedule",
        r#"false, "time", "note""#,
        "false",
        "org-log-reschedule",
        "Record when a scheduled date changes or is removed (`Rescheduled from \"[2026-05-18 Mon]\" on …`), \
         with `note` asking for a note too. `#+STARTUP:` `logreschedule`, `lognotereschedule` and \
         `nologreschedule` change it in a file.",
    ),
    setting(
        "org_log_redeadline",
        r#"false, "time", "note""#,
        "false",
        "org-log-redeadline",
        "Record when a deadline changes or is removed (`New deadline from …`), with `note` asking for a note \
         too. `#+STARTUP:` `logredeadline`, `lognoteredeadline` and `nologredeadline` change it in a file.",
    ),
    setting(
        "org_log_into_drawer",
        "boolean, string",
        "false",
        "org-log-into-drawer",
        "Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name of a drawer. \
         `#+STARTUP:` `logdrawer` and `nologdrawer` change it in a file; an entry's `LOG_INTO_DRAWER` \
         property (inherited) says which drawer for it (`t` for `LOGBOOK`, `nil` for none).",
    ),
    setting(
        "org_log_done_with_time",
        "boolean",
        "true",
        "org-log-done-with-time",
        "The `CLOSED:` timestamp of an entry done has the time, not only the date.",
    ),
    Setting {
        default: None,
        unset: "Org's headings",
        ..setting(
            "org_log_note_headings",
            "object",
            "",
            "org-log-note-headings",
            "The heading of each kind of note, by kind: `done` (`CLOSING NOTE %t`), `state` (`State %-12s from \
             %-12S %t`), `note`, `reschedule`, `delschedule`, `redeadline`, `deldeadline`, `refile` and \
             `clock_out`. In a heading `%t` is the time, `%T` the time active, `%d` and `%D` the date, `%s` \
             the new state or date and `%S` the old one, `%u` the login name and `%U` the full name; an empty \
             heading leaves the note its text only. Kinds left out keep Org's headings. Habits take notes \
             with the `done` heading for days done.",
        )
    },
    setting(
        "org_log_states_order_reversed",
        "boolean",
        "true",
        "org-log-states-order-reversed",
        "Put a new state note, note or clock line above the others (`false`: below them). `#+STARTUP:` \
         `logstatesreversed` and `nologstatesreversed` change it in a file.",
    ),
    setting(
        "org_log_state_notes_insert_after_drawers",
        "boolean",
        "false",
        "org-log-state-notes-insert-after-drawers",
        "Without a log drawer, put notes after the drawers right under the headline and planning line, not \
         before them.",
    ),
    setting(
        "org_log_note_clock_out",
        "boolean",
        "false",
        "org-log-note-clock-out",
        "Ask for a note when the clock stops (`org.clock.out`), which goes right after the clock line. \
         `#+STARTUP:` `lognoteclock-out` and `nolognoteclock-out` change it in a file.",
    ),
    setting(
        "org_provide_todo_statistics",
        r#"boolean, "all-headlines", string[], [string[], string[]]"#,
        "true",
        "org-provide-todo-statistics",
        "Update the `[1/3]` and `[33%]` cookies of the parent when a state changes, counting the entries \
         with a keyword (`true`), every entry (`all-headlines`, those without a keyword as not done), those \
         with these keywords and the done ones (a list), or those of the first list as not done and of the \
         second as done (two lists). `false` leaves the cookies.",
    ),
    setting(
        "org_hierarchical_todo_statistics",
        "boolean",
        "true",
        "org-hierarchical-todo-statistics",
        "TODO statistics count the children only; `false`: the whole subtree, and the cookies above too. A \
         `COOKIE_DATA` property with `recursive` does this for a subtree.",
    ),
    setting(
        "org_closed_keep_when_no_todo",
        "boolean",
        "false",
        "org-closed-keep-when-no-todo",
        "Keep the `CLOSED:` timestamp when an entry is left with no keyword.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_todo_state_tags_triggers",
            "object",
            "",
            "org-todo-state-tags-triggers",
            "Tags that change with the state, by state: a keyword, `\"\"` for none, `todo` for any not done, \
             `done` for any done; each a list of tags to set (`+TAG`) or remove (`-TAG`), in order: \
             `{ \"WAIT\": [\"+waiting\"], \"todo\": [\"-done\"], \"done\": [\"+done\", \"-waiting\"] }`.",
        )
    },
    setting(
        "org_tags_column",
        "number",
        "-77",
        "org-tags-column",
        "Where tags are aligned after a change to a headline. Negative: they end at that column.",
    ),
    setting(
        "org_use_tag_inheritance",
        "boolean, string[], string",
        "true",
        "org-use-tag-inheritance",
        "Which tags entries inherit from their parents and from `#+FILETAGS:`: `true` for all, `false` for none, \
         a list of tags, or a regexp (Emacs syntax) tags must match. Inherited tags count in tag searches and \
         show in the agenda.",
    ),
    setting(
        "org_tags_exclude_from_inheritance",
        "string[]",
        "[]",
        "org-tags-exclude-from-inheritance",
        "Tags never inherited, when `org_use_tag_inheritance` is `true` or a regexp.",
    ),
    setting(
        "org_use_property_inheritance",
        "boolean, string[], string",
        "false",
        "org-use-property-inheritance",
        "Which properties an entry without them takes from its parents, the file's property drawer and \
         `#+PROPERTY:` lines, in property searches (`OWNER=\"alice\"`): `true` for all, `false` for none, a list \
         of names, or a regexp (Emacs syntax). `CATEGORY` and `ARCHIVE` are always inherited.",
    ),
    setting(
        "org_global_properties",
        "{ [name]: string }",
        "{}",
        "org-global-properties",
        "Properties every entry inherits, after those of `#+PROPERTY:` lines: chiefly the allowed values of a \
         property, which setting it offers and `S-<right>` and `S-<left>` go through on its line \
         (`{ \"Effort_ALL\": \"0:10 0:30 1:00 2:00\" }`; `:ETC` allows others too).",
    ),
    setting(
        "org_clock_into_drawer",
        "boolean, string",
        "true",
        "org-clock-into-drawer",
        "Where `CLOCK:` lines go: `true` for the drawer of `org_log_into_drawer` (`LOGBOOK` when that is not a name), \
         the name of a drawer, or `false` for the entry itself.",
    ),
    setting(
        "org_clock_out_remove_zero_time_clocks",
        "boolean",
        "false",
        "org-clock-out-remove-zero-time-clocks",
        "Clocking out after less than a minute removes the `CLOCK:` line.",
    ),
    setting(
        "org_clock_out_when_done",
        "boolean",
        "true",
        "org-clock-out-when-done",
        "Marking the clocked entry done stops the clock.",
    ),
    setting(
        "org_clock_display_default_range",
        "\"today\" | \"yesterday\" | \"thisweek\" | \"lastweek\" | \"thismonth\" | \"lastmonth\" | \"thisyear\" | \"lastyear\" | \"untilnow\"",
        "\"thisyear\"",
        "org-clock-display-default-range",
        "The span whose clocks `org.clock.display` (`C-c C-x C-d`) adds up.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_clock_idle_time",
            "integer",
            "",
            "org-clock-idle-time",
            "After how many minutes without a message from the editor a clock that runs asks what to do with the idle time: keep it, subtract it (and clock out), or cancel the clock.",
        )
    },
    setting(
        "org_clock_history_length",
        "integer",
        "5",
        "org-clock-history-length",
        "How many entries clocked into the clock history keeps, for `org.clock.in` with a count and `org.clock.in.last`.",
    ),
    setting(
        "org_clock_clocktable_default_properties",
        "string",
        "\":maxlevel 2\"",
        "org-clock-clocktable-default-properties",
        "The parameters a new clock table gets on its `#+BEGIN:` line from `org.clock.report` (`C-c C-x C-r`), after `:scope`, as Emacs Lisp reads them (`:maxlevel 3 :link t`).",
    ),
    Setting {
        default: None,
        unset: "Org's: `:maxlevel 2 :lang \"en\" :scope file :narrow 40! :indent t`, the others `nil`",
        ..setting(
            "org_clocktable_defaults",
            "string",
            "",
            "org-clocktable-defaults",
            "The parameters every clock table has unless its `#+BEGIN:` line gives them, as Emacs Lisp reads \
             them. Set, it replaces Org's whole list.",
        )
    },
    Setting {
        default: None,
        unset: "Org's: `en`, `de`, `es`, `fr`, `nl`, `nn`, `pl`, `pt-BR` and `sk`",
        ..setting(
            "org_clock_clocktable_language_setup",
            "string[][]",
            "",
            "org-clock-clocktable-language-setup",
            "The words of a clock table in each language its `:lang` names: the language, then its words for \
             `File`, `L`, `Timestamp`, `Headline`, `Time`, `ALL`, `Total time`, `File time` and `Clock summary \
             at`. A word left out stays English. Set, it replaces Org's languages.",
        )
    },
    setting(
        "org_clock_report_include_clocking_task",
        "boolean",
        "false",
        "org-clock-report-include-clocking-task",
        "A clock table counts the time of the clock that runs, for its entry, when the table covers a time \
         (`:block`, `:tstart` and `:tend`) and the clock started within it.",
    ),
    setting(
        "org_clock_rounding_minutes",
        "number",
        "0",
        "org-clock-rounding-minutes",
        "Clocking in and out happens at the multiple of this many minutes nearest now; a clock never starts \
         later than now. Below 1, at the minute.",
    ),
    setting(
        "org_clock_continuously",
        "boolean",
        "false",
        "org-clock-continuously",
        "A clock starts when the last one stopped, not now.",
    ),
    setting(
        "org_clock_total_time_cell_format",
        "string",
        "\"*%s*\"",
        "org-clock-total-time-cell-format",
        "How a clock table writes its total time and the words before it; `%s` is the text.",
    ),
    setting(
        "org_clock_file_time_cell_format",
        "string",
        "\"*%s*\"",
        "org-clock-file-time-cell-format",
        "How a clock table over several files writes the words before a file's time; `%s` is the text.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_clock_in_switch_to_state",
            "string",
            "",
            "org-clock-in-switch-to-state",
            "The TODO state an entry is put in when the clock starts on it (`NEXT`), unless it has it already.",
        )
    },
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_clock_out_switch_to_state",
            "string",
            "",
            "org-clock-out-switch-to-state",
            "The TODO state an entry is put in when the clock stops on it.",
        )
    },
    setting(
        "org_lint",
        "boolean",
        "true",
        "org-lint",
        "Org's linter checks the documents and reports what it finds as diagnostics: misplaced headings and planning lines, links to nowhere, duplicate names, footnotes without definitions… What it finds is information, a hint for a checker Org trusts little: an Org file is text, nothing in it is an error or a warning.",
    ),
    setting(
        "org_lint_ignore",
        "string[]",
        "[]",
        "org-lint",
        "The `org-lint` checkers to leave out, by name, as a diagnostic's code shows it (`\"misplaced-heading\"`).",
    ),
    setting(
        "org_id_method",
        r#""uuid", "org", "ts""#,
        r#""uuid""#,
        "org-id-method",
        "How a new `ID` is made (`org.id.create`, a link stored by ID, org-roam): a random UUID, the \
         time in base 36 (`org`), or the time as `org_id_ts_format` writes it (`ts`).",
    ),
    setting(
        "org_id_prefix",
        "false, string",
        "false",
        "org-id-prefix",
        "What a new `ID` starts with, before a colon: `\"Org\"` makes `Org:…`.",
    ),
    setting(
        "org_id_ts_format",
        "string",
        r#""%Y%m%dT%H%M%S.%6N""#,
        "org-id-ts-format",
        "How the `ts` method writes the time, as `format-time-string` does.",
    ),
    setting(
        "org_id_include_domain",
        "boolean",
        "false",
        "org-id-include-domain",
        "The `org` and `ts` methods end a new `ID` with `@` and the machine's domain.",
    ),
    setting(
        "org_id_track_globally",
        "boolean",
        "true",
        "org-id-track-globally",
        "IDs are looked for in other files than the link's own: those of `org_id_search_archives`, \
         `org_id_extra_files` and `org_id_locations_file`, the agenda files, the open ones and org-roam's. Off, \
         an `id:` link leads only within its file.",
    ),
    setting(
        "org_id_search_archives",
        "boolean",
        "true",
        "org-id-search-archives",
        "IDs are looked for in the archive files of the agenda files too.",
    ),
    Setting {
        default: None,
        unset: "`org_agenda_text_search_extra_files`",
        ..setting(
            "org_id_extra_files",
            "string[]",
            "",
            "org-id-extra-files",
            "More files to look for IDs in: paths or glob patterns.",
        )
    },
    setting(
        "org_id_locations_file",
        "string",
        r#""~/.emacs.d/.org-id-locations""#,
        "org-id-locations-file",
        "The file where Emacs keeps the files it found IDs in: org-lsp looks in them too, and never writes it. \
         Names in it that are not absolute are relative to its directory.",
    ),
    setting(
        "org_id_completion_targets",
        "object[]",
        r#"[{ "files": "current", "all": true }, { "files": "org-id-files", "all": true }]"#,
        "org-id-completion-targets",
        "The headlines an `id:` link completes to (`org.link.insert`), as `org_refile_targets` names them: by \
         default every headline of the file and of the files with IDs.",
    ),
    setting(
        "org_link_search_must_match_exact_headline",
        r#"boolean, "query-to-create""#,
        r#""query-to-create""#,
        "org-link-search-must-match-exact-headline",
        "What a link to a headline that is not there does: `\"query-to-create\"` asks whether to make it, at \
         the end of the file, when the link is followed (`org.open`); `true` is an error; `false` looks for its \
         words in the text.",
    ),
    setting(
        "org_link_doi_server_url",
        "string",
        r#""https://doi.org/""#,
        "org-link-doi-server-url",
        "What a `doi:` link is opened at: this, then the DOI.",
    ),
    setting(
        "org_id_link_to_org_use_id",
        "boolean, string",
        "false",
        "org-id-link-to-org-use-id",
        "Whether `org.link.store` (`C-c l`) links to an entry by its ID: `true` makes one when the entry has none, `\"create-if-interactive\"` only from the command (not for capture's `%a`), `\"create-if-interactive-and-no-custom-id\"` likewise unless the entry has a `CUSTOM_ID`, `\"use-existing\"` only when it has one already. Off, the link searches for the headline.",
    ),
    setting(
        "org_link_context_for_files",
        "boolean",
        "true",
        "org-link-context-for-files",
        "Links to a place in a file search for it (`file:notes.org::*Heading`); `C-u` turns it around for one link.",
    ),
    setting(
        "org_id_link_use_context",
        "boolean",
        "true",
        "org-id-link-use-context",
        "ID links to a place below the entry's headline search for it as well (`id:…::name`).",
    ),
    setting(
        "org_id_link_consider_parent_id",
        "boolean",
        "false",
        "org-id-link-consider-parent-id",
        "An ID link may use the ID of a parent entry, with a search for the place.",
    ),
    setting(
        "org_link_file_path_type",
        "string",
        "\"adaptive\"",
        "org-link-file-path-type",
        "How links to files write the path: `\"adaptive\"` relative below the file's directory and absolute elsewhere, `\"absolute\"` (with `~`), `\"noabbrev\"` (without), `\"relative\"`.",
    ),
    setting(
        "org_link_keep_stored_after_insertion",
        "boolean",
        "false",
        "org-link-keep-stored-after-insertion",
        "Stored links stay stored once inserted.",
    ),
    setting(
        "org_complete_tags_always_offer_all_agenda_tags",
        "boolean",
        "false",
        "org-complete-tags-always-offer-all-agenda-tags",
        "Completing a tag offers the tags of every agenda file, after those of the file. Off, only the \
         file's own. Setting tags offers them too.",
    ),
    setting(
        "org_cite_global_bibliography",
        "string[]",
        "[]",
        "org-cite-global-bibliography",
        "Bibliography files for the citations of every document (`[cite:@key]`), after those its \
         `#+BIBLIOGRAPHY:` lines name: BibTeX (`.bib`, `.bibtex`) or CSL-JSON (`.json`). Their entries are \
         completed after `@`, shown over a key, and where following a key goes; a key none has is marked.",
    ),
    setting(
        "org_link_abbrev_alist",
        "{ [abbreviation]: string }",
        "{}",
        "org-link-abbrev-alist",
        "Link abbreviations: `[[gh:org-lsp]]` with `{ \"gh\": \"https://github.com/%s\" }` follows \
         `https://github.com/org-lsp`. `%s` stands for what follows the colon, `%h` for it URL-encoded; without \
         either it goes at the end. A file's `#+LINK:` lines come first. `%(my-function)` calls an Emacs function with what follows the colon (through Emacs).",
    ),
    setting(
        "org_file_apps",
        "[match, command][]",
        r#"[["auto-mode", "emacs"], ["directory", "emacs"], ["\\.mm\\'", "default"], ["\\.x?html?\\'", "default"], ["\\.pdf\\'", "default"]]"#,
        "org-file-apps",
        "What opens a linked file, the first entry that matches: a match is an extension (`txt`), an Emacs \
         regular expression for the file name (`\\\\.pdf\\\\'` in JSON; with groups `%1`, `%2`... in the command, it \
         matches the file followed by the link's `::` part), `directory`, `remote`, `system` (what `C-u C-u C-c C-o` uses), \
         `auto-mode` (every file the editor can show) or `t` (any other file). A command is `emacs` (the \
         editor), `default` (the `system` entry, else the editor), `system` or `mailcap` (the system's \
         application, through the editor), or a shell command where `%s` is the file. `C-u C-c C-o` opens \
         the file in the editor. A Lisp function is not a command here.",
    ),
    setting(
        "org_tag_alist",
        "string, string[]",
        r#""""#,
        "org-tag-alist",
        "The tags offered when setting tags in a file without a `#+TAGS:` line, written like one: \
         `\"work(w) home(h) { @office(o) @home(m) } [ GTD : Control Persp ]\"` gives `work` the key `w`, \
         makes `@office` and `@home` exclusive, and groups `Control` and `Persp` under `GTD`; `\\n` starts \
         a new line. A list of these words does as well. Unset, the tags used in the file are offered.",
    ),
    setting(
        "org_tag_persistent_alist",
        "string, string[]",
        r#""""#,
        "org-tag-persistent-alist",
        "Tags offered in every file, before those of `#+TAGS:` or `org_tag_alist`, written the same way.",
    ),
    setting(
        "org_group_tags",
        "boolean",
        "true",
        "org-group-tags",
        "A group tag (`[ GTD : Control Persp ]`, `{ Context : @home @office }` in `#+TAGS:` or `org_tag_alist`) \
         in the match string of a tags view stands for itself and the tags of its group.",
    ),
    setting(
        "org_use_fast_todo_selection",
        r#""auto", "expert", false"#,
        r#""auto""#,
        "org-use-fast-todo-selection",
        "How `C-c C-t` (`org.todo`) changes the TODO state where keywords have keys (`TODO(t)`): `auto` picks \
         the state by key from a menu of them; `expert` by key too, the keys listed in the prompt alone (in \
         Neovim and Vim; other editors show their menu); `false` goes to the next state, as where no keyword \
         has a key. `true` is `auto`.",
    ),
    setting(
        "org_use_fast_tag_selection",
        r#"true, false, "auto""#,
        r#""auto""#,
        "org-use-fast-tag-selection",
        "Pick tags by key, each key turning its tag on or off until RET: `auto` when some tag has a key, \
         `true` always (keys are given to the tags without one), `false` never: the tags are typed instead.",
    ),
    setting(
        "org_fast_tag_selection_single_key",
        "boolean",
        "false",
        "org-fast-tag-selection-single-key",
        "In fast tag selection, the first key that changes the tags also ends it.",
    ),
    Setting {
        default: None,
        unset: "the order they were picked in",
        ..setting(
            "org_tags_sort_function",
            r#""string<", "string>""#,
            "",
            "org-tags-sort-function",
            "Sort the tags of a headline when they are set: alphabetically, or the other way.",
        )
    },
    setting(
        "org_auto_align_tags",
        "boolean",
        "true",
        "org-auto-align-tags",
        "Align the tags of a headline to `org_tags_column` when they are set.",
    ),
    Setting {
        default: None,
        unset: "the first keyword of the sequence",
        ..setting(
            "org_todo_repeat_to_state",
            "string, true",
            "",
            "org-todo-repeat-to-state",
            "The state a repeating entry returns to when it is done: a keyword, or `true` for the state it \
             had.",
        )
    },
    setting("org_priority_highest", "letter, or 0 to 64", r#""A""#, "org-priority-highest", "The highest priority."),
    setting("org_priority_lowest", "letter, or 0 to 64", r#""C""#, "org-priority-lowest", "The lowest priority."),
    setting(
        "org_priority_default",
        "letter, or 0 to 64",
        r#""B""#,
        "org-priority-default",
        "The priority of an entry without a cookie. A file's `#+PRIORITIES:` line takes precedence over the \
         three priority options.",
    ),
    setting(
        "org_priority_start_cycle_with_default",
        "boolean",
        "true",
        "org-priority-start-cycle-with-default",
        "Raising or lowering the priority of an entry that has none starts at the default.",
    ),
    setting(
        "org_time_stamp_rounding_minutes",
        "number",
        "5",
        "org-time-stamp-rounding-minutes",
        "The step in which the minutes of a timestamp change (the second number of Org's option).",
    ),
    setting(
        "org_read_date_prefer_future",
        r#"boolean, "time""#,
        "true",
        "org-read-date-prefer-future",
        "A date typed without a year or month is the next one to come. With `time`, a time that has passed \
         today is tomorrow's.",
    ),
    Setting {
        default: None,
        unset: "as `org_read_date_prefer_future`",
        ..setting(
            "org_agenda_jump_prefer_future",
            r#"boolean, "time""#,
            "",
            "org-agenda-jump-prefer-future",
            "How the agenda's `j` (`org.agenda.goto.date`) reads a date typed without a year or month, as \
             `org_read_date_prefer_future` does for every other prompt.",
        )
    },
    setting(
        "org_read_date_force_compatible_dates",
        "boolean",
        "true",
        "org-read-date-force-compatible-dates",
        "A typed year is kept within 1970 to 2037.",
    ),
    setting(
        "org_refile_targets",
        "object[]",
        r#"[{ "files": "current", "level": 1 }]"#,
        "org-refile-targets",
        "Where entries can be refiled to. Each entry has `files`: `current` (the entry's own file), `agenda`, \
         `org-id-files` (the files with IDs), a path or a list of paths; and one of `level`, `maxlevel`, `tag`, `todo`, `regexp` (the Rust `regex` \
         crate's syntax) or `all` (true), which says which headlines there are targets.",
    ),
    setting(
        "org_refile_use_outline_path",
        r#"boolean, "file", "full-file-path", "title""#,
        "false",
        "org-refile-use-outline-path",
        "Name targets by their outline path (`Projects/Alpha/`). The three names start the path with the \
         file, and offer the file itself, for its top level.",
    ),
    setting(
        "org_refile_allow_creating_parent_nodes",
        r#"boolean, "confirm""#,
        "false",
        "org-refile-allow-creating-parent-nodes",
        "A target that is not there yet can be named after an existing one, `Projects/New`: it is made, as the \
         last child of `Projects`, and the entry goes under it. `confirm` asks first.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
        "org_refile_target_verify_function",
        "string",
        "",
        "org-refile-target-verify-function",
        "The name of an Emacs Lisp function that decides which headlines are refile targets: the server's Emacs \
         calls it at the start of each, and keeps those it says yes to. Where it leaves the point, the search \
         goes on: a function can skip a subtree.",
        )
    },
    setting(
        "org_reverse_note_order",
        "boolean",
        "false",
        "org-reverse-note-order",
        "A refiled entry goes first under its target, not last.",
    ),
    setting(
        "org_archive_location",
        "string",
        r#""%s_archive::""#,
        "org-archive-location",
        "Where archived subtrees go: a file, `::`, and a headline under which they go (they go to the end of the \
         file without one). `%s` is the name of the file the subtree is in; an empty file part is that file. \
         `datetree/` as the headline files them under year, month and day, by when they were closed. An \
         `ARCHIVE` property of the entry or a parent, or a `#+ARCHIVE:` line, says otherwise.",
    ),
    setting(
        "org_archive_reversed_order",
        "boolean",
        "false",
        "org-archive-reversed-order",
        "An archived subtree goes first under its archive headline, not last.",
    ),
    setting(
        "org_archive_mark_done",
        "boolean, string",
        "false",
        "org-archive-mark-done",
        "Mark archived entries done: `true` for the first done keyword, or the keyword to use.",
    ),
    setting(
        "org_archive_subtree_add_inherited_tags",
        r#"boolean, "infile""#,
        r#""infile""#,
        "org-archive-subtree-add-inherited-tags",
        "Give an archived subtree the tags it inherited: always, never, or when it stays in its file.",
    ),
    setting(
        "org_archive_save_context_info",
        "string[]",
        r#"["time", "file", "olpath", "category", "todo", "itags"]"#,
        "org-archive-save-context-info",
        "What an archived entry keeps as `ARCHIVE_…` properties: `time`, `file`, `olpath` (its outline path), \
         `category`, `todo`, `itags` (inherited tags), `ltags` (its own tags) and `olid` (its parent's ID).",
    ),
    setting(
        "org_archive_sibling_heading",
        "string",
        r#""Archive""#,
        "org-archive-sibling-heading",
        "The headline of the sibling `org.archive.sibling` archives under.",
    ),
    setting(
        "org_archive_file_header_format",
        "string, false",
        r#""\nArchived entries from file %s\n\n""#,
        "org-archive-file-header-format",
        "What a new archive file starts with; `%s` is the path of the file the subtree came from.",
    ),
    setting(
        "org_archive_default_command",
        r#""subtree", "sibling", "tag""#,
        r#""subtree""#,
        "org-archive-default-command",
        "What `org.archive.default` does: archive the subtree, archive it under its sibling, or tag it.",
    ),
    setting(
        "org_capture_templates_contexts",
        "object[]",
        "[]",
        "org-capture-templates-contexts",
        "Where templates are offered. Each has the `key` of a template, `contexts`, rules such as \
         `{ \"in_file\": \"work\" }` (`in_file`, `in_mode`, `in_buffer`, `not_in_file`, `not_in_mode` or \
         `not_in_buffer`, each with a regexp of the `regex` crate's syntax, matched ignoring case), and maybe \
         `use`, the key of the template it stands for there. A template with contexts is offered only where one \
         of their rules holds. The mode is `org-mode` in an Org file and `org-agenda-mode` in the agenda, where \
         there is no file; the buffer's name is the file's.",
    ),
    setting(
        "org_capture_templates",
        "object[]",
        r#"[{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }]"#,
        "org-capture-templates",
        "Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the default, `plain`, \
         `item`, `checkitem` or `table-line`), a `target` and a `template` (text, or a list of lines), and may \
         have `prepend`, `empty_lines`, `empty_lines_before`, `empty_lines_after`, `immediate_finish`, \
         `tree_type`, `time_prompt`, `table_line_pos`, `clock_in`, `clock_keep`, `clock_resume`, \
         `jump_to_captured` and `unnarrowed`. The target is a list: `file` and a path; \
         `file+headline`, a path and a headline; `file+olp`, a path and the headlines of an outline path; \
         `file+regexp`, a path and a regular expression; `file+olp+datetree`, a path and the headlines of an \
         outline path, maybe none, for a date tree under them; `id` and an ID; `clock`, the entry clocked \
         now; `file+function`, a path and the name of an Emacs Lisp function that moves the point there, or \
         `function` and the name of one that visits the file too (run by the server's Emacs). An empty path is `org_default_notes_file`, and without a target the entry goes at the end of \
         `org_default_notes_file`. A template with only a key and a description groups the templates whose keys \
         start with its key.",
    ),
    setting(
        "org_directory",
        "string",
        r#""~/org""#,
        "org-directory",
        "Where the relative paths of capture targets are.",
    ),
    setting(
        "org_default_notes_file",
        "string",
        r#""~/.notes""#,
        "org-default-notes-file",
        "The target of a capture template whose file is empty.",
    ),
    setting(
        "org_link_descriptive",
        "boolean",
        "true",
        "org-link-descriptive",
        "Show a link as its description (or its target, when it has none) and hide the rest: the brackets, \
         and the target of a link with a description. The text is marked `hidden` in the highlighting, \
         and editors that can conceal text conceal it.",
    ),
    setting(
        "org_hide_emphasis_markers",
        "boolean",
        "false",
        "org-hide-emphasis-markers",
        "Hide the markers around `*bold*`, `/italic/`, `_underline_`, `+strike-through+`, `=verbatim=` \
         and `~code~`, as for links.",
    ),
    setting(
        "org_fontify_emphasized_text",
        "boolean",
        "true",
        "org-fontify-emphasized-text",
        "Show `*bold*`, `/italic/`, `_underline_`, `+strike-through+`, `=verbatim=` and `~code~` in their \
         looks. With `false` they look as the text around them, their markers shown.",
    ),
    Setting {
        default: None,
        unset: "Org's: `bold`, `italic`, `underline`, `org-verbatim`, `org-code` and `:strike-through t`",
        ..setting(
            "org_emphasis_alist",
            "{ [marker]: face | face[] | false }",
            "",
            "org-emphasis-alist",
            "The look of each kind of emphasis, by its marker (`*`, `/`, `_`, `=`, `~`, `+`): a face of Org's, \
             `bold`, `italic`, `bold-italic`, `underline`, `org-code` or `org-verbatim`, attributes as Org \
             writes them (`:weight bold :slant italic :underline t :strike-through t`), or a list of them; \
             `false` for none. As in Emacs, a marker left out has no look. Colors are not shown.",
        )
    },
    setting(
        "org_hidden_keywords",
        r#"("title" | "subtitle" | "author" | "email" | "date")[]"#,
        "[]",
        "org-hidden-keywords",
        "The keywords hidden before their values, such as `#+TITLE:` with `[\"title\"]`, in editors that \
         conceal text.",
    ),
    setting(
        "org_hide_macro_markers",
        "boolean",
        "false",
        "org-hide-macro-markers",
        "Hide the braces of macro calls, `{{{` and `}}}`, in editors that conceal text.",
    ),
    setting(
        "org_display_custom_times",
        "boolean",
        "false",
        "org-display-custom-times",
        "Show timestamps in `org_timestamp_custom_formats`: the date and time inside the brackets is hidden \
         and the custom form shows in its place, as entities do. `#+STARTUP: customtime` turns it on for a \
         file, and `C-c C-x C-t` (`org.timestamp.overlays.toggle`) on and off. Exports through Emacs use \
         them too; the date prompt shows its answer in Org's own form.",
    ),
    setting(
        "org_timestamp_custom_formats",
        "[date, date with time]",
        r#"["%m/%d/%y %a", "%m/%d/%y %a %H:%M"]"#,
        "org-timestamp-custom-formats",
        "The formats of `org_display_custom_times`, as `format-time-string` reads them (`%d.%m.%Y`); \
         brackets around a format are dropped.",
    ),
    setting(
        "org_pretty_entities",
        "boolean",
        "false",
        "org-pretty-entities",
        "Show entities (`\\alpha`, `\\to`) as their character (`α`, `→`): the entity is hidden as link \
         brackets are, and its character is an inlay hint in its place.",
    ),
    setting(
        "org_pretty_entities_include_sub_superscripts",
        "boolean",
        "true",
        "org-pretty-entities-include-sub-superscripts",
        "With `org_pretty_entities`, show sub- and superscripts (`x^2`, `H_{2}O`, `a_{n}`) raised and lowered: \
         in Unicode (`x²`, `H₂O`, `aₙ`) where every character has a raised or lowered form, as an inlay hint \
         in place of the hidden text; the others are left as they are.",
    ),
    setting(
        "org_use_sub_superscripts",
        "true, \"{}\" or false",
        "true",
        "org-use-sub-superscripts",
        "Which sub- and superscripts `org_pretty_entities` shows: all, only those in braces (`a_{b}`, \
         `\"{}\"`), or none.",
    ),
    setting(
        "return_uses_meta_return",
        "boolean",
        "false",
        "",
        "RET on the first line of a list item makes the next item, numbered and with a checkbox as M-RET \
         (`org-meta-return`) makes it, the items after it renumbered, as nvim-orgmode's \
         `org_return_uses_meta_return` does. Off, RET is Org's: a new line in the item. \
         `org.return.item` (`<S-CR>`, Shift+Enter, with the vim keys) always does it.",
    ),
    setting(
        "org_return_follows_link",
        "boolean",
        "false",
        "org-return-follows-link",
        "RET (`org.return`) on a link, a timestamp or a citation follows it, as `org.open` (`C-c C-o`) does. \
         In a table RET still moves down.",
    ),
    setting(
        "org_goto_max_level",
        "number",
        "5",
        "org-goto-max-level",
        "The deepest headings `org.goto` (`C-c C-j`) offers.",
    ),
    setting(
        "calendar_months",
        "1 | 3",
        "3",
        "",
        "How many months the calendar of the date prompt shows: 3 side by side, as Emacs's calendar shows them, \
         the one with the selected day in the middle; or 1, that month alone, for a narrow window, with the \
         answer on a line under the prompt.",
    ),
    setting(
        "calendar_week_start_day",
        "0 to 6 (0 is Sunday)",
        "0",
        "calendar-week-start-day",
        "The day the weeks of the date prompt's calendar start on: 1 starts them on Monday.",
    ),
    setting(
        "emacs",
        "string",
        r#""emacs""#,
        "",
        "The Emacs program, for what only Org itself does: export (`org.export.dispatch`). The server starts \
         it (`emacs --batch -Q`) when that is first asked for and keeps it running, so that Org loads once. \
         Without Emacs, everything else works as before.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "emacs_config_file",
            "string",
            "",
            "",
            "A file of Emacs Lisp that Emacs loads before Org, for the variables export reads \
             (`org-export-backends`, `org-html-…`, `org-latex-…`, `org-publish-project-alist`): a file of its \
             own, or your init file. `~/` is the home directory.",
        )
    },
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "emacs_config",
            "string",
            "",
            "",
            "Emacs Lisp that Emacs evaluates after `emacs_config_file` and before loading Org, for a few \
             settings without a file: `(setq org-export-with-toc nil)`, or `(add-to-list 'load-path \
             \"~/src/org-mode/lisp\")` for another Org. A yes-or-no question Emacs asks while it works \
             (`org-confirm-babel-evaluate`) is asked in the editor.",
        )
    },
    setting(
        "auto_save",
        "boolean",
        "true",
        "",
        "Save the files a command changes other than the one it was run in: an entry's file changed from \
         the agenda, the target of a capture or of a refile. The editor saves them once it has made the \
         change. Off, they are left with unsaved changes, as Emacs leaves them.",
    ),
    setting(
        "org_cycle_separator_lines",
        "number",
        "2",
        "org-cycle-separator-lines",
        "With at least this many blank lines after a subtree, the last one stays outside its fold. Negative: \
         all of them stay outside. `0`: none does.",
    ),
    setting(
        "org_cycle_include_plain_lists",
        r#"true, false, "integrate""#,
        "true",
        "org-cycle-include-plain-lists",
        "Whether list items fold, as subtrees do. `integrate` makes them part of the outline: TAB showing \
         a headline's children folds its lists to their top items too. `false`: they do not fold.",
    ),
    Setting {
        default: None,
        unset: "every level",
        ..setting(
            "org_cycle_max_level",
            "number",
            "",
            "org-cycle-max-level",
            "The deepest headlines that fold; deeper ones are text of the subtree they are in.",
        )
    },
    setting(
        "org_cycle_global_at_bob",
        "boolean",
        "false",
        "org-cycle-global-at-bob",
        "TAB at the very start of the file, when its first line is no headline, cycles the whole file as \
         S-TAB does.",
    ),
    setting(
        "org_cycle_level_after_item_entry_creation",
        "boolean",
        "true",
        "org-cycle-level-after-item/entry-creation",
        "TAB on a new, empty headline makes it a child of the headline before, then each level up, then a \
         sibling again; on an empty item, it indents and outdents the item the same way.",
    ),
    setting(
        "org_startup_folded",
        "string",
        "\"showeverything\"",
        "org-startup-folded",
        "How files show when opened: `\"overview\"` (or `\"fold\"`) only the top headlines, `\"content\"` all \
         the headlines, `\"show2levels\"` to `\"show5levels\"` the headlines down to that level, \
         `\"showeverything\"` (or `\"nofold\"`, `\"showall\"`) everything. `#+STARTUP:` decides for a file. \
         Unless everything shows, a subtree's `VISIBILITY` property (`folded`, `children`, `content`, `all`) \
         decides for it, archived subtrees are folded, and drawers too. Folds hide whole subtrees, so where \
         Org shows a headline's children but not its text, the text shows.",
    ),
    setting(
        "org_startup_shrink_all_tables",
        "boolean",
        "false",
        "org-startup-shrink-all-tables",
        "Shrink the columns of every table that have a width cookie (`<10>`) when a file opens. `#+STARTUP: shrink` \
         decides for a file. Shrunk fields show their first characters and `…`, in editors that can hide text.",
    ),
    setting(
        "org_cycle_hide_block_startup",
        "boolean",
        "false",
        "org-cycle-hide-block-startup",
        "Fold blocks when a file opens folded (`org_startup_folded`). `#+STARTUP: hideblocks` and `nohideblocks` \
         decide for a file.",
    ),
    setting(
        "org_cycle_hide_drawer_startup",
        "boolean",
        "true",
        "org-cycle-hide-drawer-startup",
        "Fold drawers when a file opens folded (`org_startup_folded`).",
    ),
    setting(
        "org_cycle_open_archived_trees",
        "boolean",
        "false",
        "org-cycle-open-archived-trees",
        "Leave archived subtrees open when a file opens folded (`org_startup_folded`).",
    ),
    setting(
        "org_adapt_indentation",
        "boolean | \"headline-data\"",
        "false",
        "org-adapt-indentation",
        "Indent the lines of an entry past its headline's stars: `true` for all of them, `\"headline-data\"` for \
         the planning line, drawers and clock lines right below the headline. Applies to new lines, to \
         re-indenting (range formatting) and to what commands write.",
    ),
    setting(
        "org_image_actual_width",
        "boolean, number, or [number]",
        "true",
        "org-image-actual-width",
        "How wide the image of a link shows: `true` at its own width; a number, that many pixels; `false` or a \
         list of one number, the `:width` of the paragraph's `#+ATTR_ORG:` or else of another `#+ATTR_…:` \
         (pixels, `50%` or a fraction up to 2 of the text's width, `t` for its own width), else the list's \
         number. An entry's `ORG-IMAGE-ACTUAL-WIDTH` property decides for its own text.",
    ),
    setting(
        "org_image_max_width",
        "\"fill-column\", \"window\", number, or null",
        r#""fill-column""#,
        "org-image-max-width",
        "How wide at most an image shown at its own width is: `\"fill-column\"`, 70 columns; `\"window\"`, \
         the window's width; an integer, that many pixels; a float, that part of the window's width; null for \
         no limit.",
    ),
    setting(
        "org_startup_with_link_previews",
        "boolean",
        "false",
        "org-startup-with-link-previews",
        "Show the images of links to image files in place when a file opens, as `C-c C-x C-v` \
         (`org.link.preview`) shows them. `#+STARTUP: inlineimages` (or `linkpreviews`) and `noinlineimages` \
         decide for a file. Editors that can show images: Neovim from 0.13 (`vim.ui.img`), in a terminal with \
         the Kitty graphics protocol.",
    ),
    setting(
        "org_startup_with_latex_preview",
        "boolean",
        "false",
        "org-startup-with-latex-preview",
        "Show the LaTeX fragments as images when a file opens, as `C-c C-x C-l` (`org.latex.preview`) shows \
         them, made by Emacs. `#+STARTUP: latexpreview` and `nolatexpreview` decide for a file.",
    ),
    setting(
        "org_startup_indented",
        "boolean",
        "false",
        "org-startup-indented",
        "Show files indented by level without changing them (`org-indent-mode`): the text under a headline lines up \
         with its title, and leading stars are hidden. `#+STARTUP: indent` and `noindent` decide for a file. It \
         turns `org_adapt_indentation` off in such files, unless that is `\"headline-data\"`. Editors that can show it: \
         Neovim, and VS Code (not on wrapped lines).",
    ),
    setting(
        "org_hide_leading_stars",
        "boolean",
        "false",
        "org-hide-leading-stars",
        "Hide all the stars of a headline but the last; they keep their place. `#+STARTUP: hidestars` and \
         `showstars` decide for a file.",
    ),
    setting(
        "org_duration_format",
        r#""h:mm", "h:mm:ss", or a list"#,
        r#"[["d", false], ["special", "h:mm"]]"#,
        "org-duration-format",
        "How durations are written (clock tables, clock lines, efforts): `\"h:mm\"` or `\"h:mm:ss\"`, or a list \
         of units each with whether it is written when zero (`[\"d\", false]`), with perhaps `[\"special\", \
         \"h:mm\"]` (the hours and minutes as `H:MM` after the larger units), `[\"special\", \"h:mm:ss\"]`, \
         `[\"special\", 2]` (one unit with 2 decimals) and `\"compact\"` (no space between units). The units \
         are those of `org_duration_units`.",
    ),
    setting(
        "org_duration_units",
        "[unit, minutes][]",
        r#"[["min", 1], ["h", 60], ["d", 1440], ["w", 10080], ["m", 43200], ["y", 525960.0]]"#,
        "org-duration-units",
        "The units of durations and the minutes in each, for reading efforts and writing durations: \
         `[\"d\", 480]` for days of 8 hours.",
    ),
    setting(
        "org_fontify_done_headline",
        "boolean",
        "true",
        "org-fontify-done-headline",
        "Show a headline marked done in the look of done headlines past its keyword (`org-headline-done`).",
    ),
    setting(
        "org_fontify_todo_headline",
        "boolean",
        "false",
        "org-fontify-todo-headline",
        "Show a headline marked with a keyword not done in the look of TODO headlines past its keyword \
         (`org-headline-todo`, the token modifier `todo`).",
    ),
    setting(
        "org_fontify_quote_and_verse_blocks",
        "boolean",
        "false",
        "org-fontify-quote-and-verse-blocks",
        "Show the lines inside quote and verse blocks in their own look (`org-quote`, `org-verse`: the token \
         types `quote` and `verse`).",
    ),
    Setting {
        default: None,
        unset: "the standard three dots, `...`",
        ..setting(
            "org_ellipsis",
            "string",
            "",
            "org-ellipsis",
            "What a closed fold shows after its first line, such as `\" ▾\"`, in the highlight group \
             `OrgLspEllipsis` (Neovim). Editors that can show it: Neovim and Vim.",
        )
    },
    setting(
        "org_bullets_mode",
        "boolean",
        "false",
        "org-bullets-mode",
        "org-bullets: the last star of each headline shows as a bullet, those of `org_bullets_bullet_list` by \
         level, and the stars before it are hidden, as with `org_hide_leading_stars`.",
    ),
    setting(
        "org_bullets_bullet_list",
        "string[]",
        r#"["◉", "○", "✸", "✿"]"#,
        "org-bullets-bullet-list",
        "The bullets of `org_bullets_mode`, one a level, repeating after the last.",
    ),
    setting(
        "org_bullets_list_bullet",
        "false, or a string",
        r#""•""#,
        "",
        "What `org_bullets_mode` shows in place of the `-`, `+` or `*` of an item, as org-bullets.nvim does; \
         `false` leaves them as they are.",
    ),
    setting(
        "org_bullets_checkboxes",
        "false, or { todo, half, done }",
        r#"{ "todo": " ", "half": "-", "done": "✓" }"#,
        "",
        "What `org_bullets_mode` shows between the brackets of a checkbox, `[ ]`, `[-]` and `[X]`, as \
         org-bullets.nvim does; `false` leaves them as they are. A state left out keeps its default.",
    ),
    setting(
        "org_src_preserve_indentation",
        "boolean",
        "false",
        "org-src-preserve-indentation",
        "Leave the code of source blocks as it is indented when re-indenting.",
    ),
    setting(
        "org_src_content_indentation",
        "number",
        "2",
        "org-src-content-indentation",
        "How far code is indented past its source block: when re-indenting, and when the code of a block \
         edited in a file of its own (`C-c '`) is written back. Formerly `org_edit_src_content_indentation`, \
         which is still read.",
    ),
    setting(
        "org_agenda_span",
        r#""day", "week", "fortnight", "month", "year", or a number of days"#,
        r#""week""#,
        "org-agenda-span",
        "How much time the agenda shows.",
    ),
    setting(
        "org_agenda_start_on_weekday",
        "0 to 6 (0 is Sunday), or false",
        "1",
        "org-agenda-start-on-weekday",
        "The day a week or fortnight starts on; `false` starts it today.",
    ),
    setting(
        "org_deadline_warning_days",
        "number",
        "14",
        "org-deadline-warning-days",
        "How many days before a deadline it shows up.",
    ),
    setting(
        "org_scheduled_delay_days",
        "number",
        "0",
        "org-scheduled-delay-days",
        "How many days after its date a scheduled entry shows up.",
    ),
    setting(
        "org_scheduled_past_days",
        "number",
        "10000",
        "org-scheduled-past-days",
        "How many days a scheduled entry that is not done keeps showing up.",
    ),
    setting(
        "org_modules",
        "string[]",
        "[]",
        "org-modules",
        "Org modules to load. `org-habit` turns habits on: entries with the `STYLE` property `habit` and a \
         scheduled date with a repeater (`.+2d`, `.+2d/4d` for a deadline too) show in the agenda with a graph \
         of the days they were done. Others are ignored.",
    ),
    setting(
        "org_habit_graph_column",
        "number",
        "40",
        "org-habit-graph-column",
        "The column the consistency graph of a habit starts at in the agenda.",
    ),
    setting(
        "org_habit_preceding_days",
        "number",
        "21",
        "org-habit-preceding-days",
        "How many days before today a habit's graph shows.",
    ),
    setting(
        "org_habit_following_days",
        "number",
        "7",
        "org-habit-following-days",
        "How many days after today a habit's graph shows.",
    ),
    setting(
        "org_habit_show_habits",
        "boolean",
        "true",
        "org-habit-show-habits",
        "Show habits in the agenda.",
    ),
    setting(
        "org_habit_show_habits_only_for_today",
        "boolean",
        "true",
        "org-habit-show-habits-only-for-today",
        "Show habits only on today's date, not on future days of the agenda.",
    ),
    setting(
        "org_habit_show_all_today",
        "boolean",
        "false",
        "org-habit-show-all-today",
        "Show every habit today, even those not scheduled for today.",
    ),
    setting(
        "org_habit_today_glyph",
        "string",
        r#""!""#,
        "org-habit-today-glyph",
        "The mark of today in a habit's graph.",
    ),
    setting(
        "org_habit_completed_glyph",
        "string",
        r#""*""#,
        "org-habit-completed-glyph",
        "The mark of a day a habit was done in its graph.",
    ),
    setting(
        "org_habit_show_done_always_green",
        "boolean",
        "false",
        "org-habit-show-done-always-green",
        "A day a habit was done is green in its graph even when it was done late.",
    ),
    Setting {
        default: None,
        unset: "`org_scheduled_past_days`",
        ..setting(
            "org_habit_scheduled_past_days",
            "number",
            "",
            "org-habit-scheduled-past-days",
            "How many days a habit that is not done keeps showing up.",
        )
    },
    setting(
        "org_deadline_past_days",
        "number",
        "10000",
        "org-deadline-past-days",
        "How many days a missed deadline keeps showing up.",
    ),
    setting(
        "org_agenda_skip_scheduled_if_done",
        "boolean",
        "false",
        "org-agenda-skip-scheduled-if-done",
        "Leave out scheduled entries that are done.",
    ),
    setting(
        "org_agenda_skip_deadline_if_done",
        "boolean",
        "false",
        "org-agenda-skip-deadline-if-done",
        "Leave out deadlines of entries that are done.",
    ),
    setting(
        "org_agenda_skip_timestamp_if_done",
        "boolean",
        "false",
        "org-agenda-skip-timestamp-if-done",
        "Leave out timestamps of entries that are done.",
    ),
    setting(
        "org_agenda_time_leading_zero",
        "boolean",
        "false",
        "org-agenda-time-leading-zero",
        "Write times with two digits for the hour (`08:00`).",
    ),
    setting(
        "org_agenda_timegrid_use_ampm",
        "boolean",
        "false",
        "org-agenda-timegrid-use-ampm",
        "Write the times of the agenda, the time grid's too, in 12-hour form (`8:00am`).",
    ),
    setting(
        "org_agenda_search_headline_for_time",
        "boolean",
        "true",
        "org-agenda-search-headline-for-time",
        "Take a time in a headline (`Call at 15:00`) as the time of its entry.",
    ),
    setting(
        "org_agenda_remove_times_when_in_prefix",
        r#"boolean, "beg""#,
        "true",
        "org-agenda-remove-times-when-in-prefix",
        "Take a time out of the headline when the prefix shows it; `beg` only one at the start of the \
         headline.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_agenda_default_appointment_duration",
            "integer",
            "",
            "org-agenda-default-appointment-duration",
            "Minutes an entry with a start time and no end time lasts: the agenda shows it as a range.",
        )
    },
    setting(
        "org_agenda_remove_timeranges_from_blocks",
        "boolean",
        "false",
        "org-agenda-remove-timeranges-from-blocks",
        "Take a date range out of the headline of its lines.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_agenda_diary_sexp_prefix",
            "string",
            "",
            "org-agenda-diary-sexp-prefix",
            "A regexp (Emacs's syntax): what it matches of a diary sexp's text goes in front of the line, as \
             a scheduled date's `Scheduled:` does.",
        )
    },
    setting(
        "org_agenda_show_inherited_tags",
        r#"boolean, "always", string[]"#,
        "true",
        "org-agenda-show-inherited-tags",
        "Show the inherited tags of an entry on its agenda lines: where `org_agenda_use_tag_inheritance` has \
         them (`true`), always, never (`false`), or in these kinds of views (`agenda`, `todo`, `search`). \
         Sorting by tag and `org_agenda_max_tags` go by the tags a line shows.",
    ),
    setting(
        "org_agenda_use_tag_inheritance",
        "boolean, string[]",
        r#"["todo", "search", "agenda"]"#,
        "org-agenda-use-tag-inheritance",
        "The kinds of views whose lines have inherited tags (`true`: all). Tags matches always have them.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_agenda_hide_tags_regexp",
            "string",
            "",
            "org-agenda-hide-tags-regexp",
            "A regexp (Emacs's syntax): the tags it matches are not shown on agenda lines, but filters still \
             see them.",
        )
    },
    setting(
        "org_agenda_include_deadlines",
        "boolean",
        "true",
        "org-agenda-include-deadlines",
        "Show deadlines in the agenda.",
    ),
    setting(
        "org_agenda_skip_additional_timestamps_same_entry",
        "boolean",
        "false",
        "org-agenda-skip-additional-timestamps-same-entry",
        "Once a timestamp of an entry has made a line for a day, leave out its other timestamps on that day.",
    ),
    setting(
        "org_agenda_fontify_priorities",
        r#"boolean, "cookies""#,
        r#""cookies""#,
        "org-agenda-fontify-priorities",
        "Highlight the priority cookies of agenda lines (`cookies`), or each line from its cookie on \
         (`true`; links and tags keep their own look); the highest priority shows in bold, the lowest in \
         italic. `false` highlights none. Org's list of faces by priority is not read here: \
         `org_priority_faces` gives them.",
    ),
    setting(
        "org_agenda_prefer_last_repeat",
        "boolean, string[]",
        "false",
        "org-agenda-prefer-last-repeat",
        "Show a repeating scheduled date, deadline or timestamp at its last repeat up to today instead of its \
         base date, for every entry (`true`) or for those with one of these TODO keywords.",
    ),
    setting(
        "org_agenda_skip_deadline_prewarning_if_scheduled",
        r#"boolean, "pre-scheduled", number"#,
        "false",
        "org-agenda-skip-deadline-prewarning-if-scheduled",
        "For a scheduled entry, show its deadline only on its date (`true`), not before its scheduled date \
         (`pre-scheduled`), or from N days before it (a number).",
    ),
    setting(
        "org_agenda_skip_scheduled_delay_if_deadline",
        r#"boolean, "post-deadline", number"#,
        "false",
        "org-agenda-skip-scheduled-delay-if-deadline",
        "For an entry with a deadline, drop the delay of its scheduled date (`--2d`) (`true`), or keep it no \
         later than the deadline (`post-deadline`). A number drops it too, as Org's code does.",
    ),
    setting(
        "org_agenda_skip_scheduled_if_deadline_is_shown",
        r#"boolean, "not-today", "repeated-after-deadline""#,
        "false",
        "org-agenda-skip-scheduled-if-deadline-is-shown",
        "Leave out a scheduled date on a day its entry's deadline shows (`true`), only when it was scheduled \
         before today (`not-today`); `repeated-after-deadline` is \
         `org_agenda_skip_scheduled_repeats_after_deadline`. Not for habits.",
    ),
    setting(
        "org_agenda_skip_scheduled_repeats_after_deadline",
        "boolean",
        "false",
        "org-agenda-skip-scheduled-repeats-after-deadline",
        "Leave out the repeats of a scheduled date that come after the entry's deadline.",
    ),
    setting(
        "org_agenda_skip_comment_trees",
        "boolean",
        "true",
        "org-agenda-skip-comment-trees",
        "Leave out of every view the entries of a commented subtree (`COMMENT` after the stars).",
    ),
    setting(
        "org_agenda_skip_archived_trees",
        "boolean",
        "true",
        "org-agenda-skip-archived-trees",
        "Leave out of every view the entries of an archived subtree (tagged `ARCHIVE`), unless archives \
         mode (`v a`) shows them. Org advises leaving it on and using `v a` instead.",
    ),
    setting(
        "org_agenda_todo_list_sublevels",
        "boolean",
        "true",
        "org-agenda-todo-list-sublevels",
        "Show in the TODO list the TODO entries below another one. Off, the list skips the subtree of \
         each entry it shows or leaves out for a date.",
    ),
    setting(
        "org_agenda_todo_ignore_with_date",
        "boolean",
        "false",
        "org-agenda-todo-ignore-with-date",
        "Leave out of the TODO list the entries with an active timestamp anywhere in their text, \
         scheduled and deadline dates too.",
    ),
    setting(
        "org_agenda_todo_ignore_scheduled",
        r#"boolean, "future", "past", "all", number"#,
        "false",
        "org-agenda-todo-ignore-scheduled",
        "Leave out of the TODO list the scheduled entries: all of them (`true`, `all`), those scheduled \
         after today (`future`), today or before (`past`), or with a number N, N days from now or later \
         (a negative N: N days ago or before).",
    ),
    setting(
        "org_agenda_todo_ignore_deadlines",
        r#"boolean, "near", "far", "all", "future", "past", number"#,
        "false",
        "org-agenda-todo-ignore-deadlines",
        "Leave out of the TODO list the entries with a deadline: in its warning period, that is, shown in \
         the agenda (`true`, `near`), not yet in it (`far`), all of them (`all`), after today (`future`), \
         today or before (`past`), or with a number as for `org_agenda_todo_ignore_scheduled`.",
    ),
    setting(
        "org_agenda_todo_ignore_timestamp",
        r#"boolean, "future", "past", "all", number"#,
        "false",
        "org-agenda-todo-ignore-timestamp",
        "Leave out of the TODO list the entries with an active timestamp that is no scheduled or \
         deadline date, with the values of `org_agenda_todo_ignore_scheduled`.",
    ),
    setting(
        "org_agenda_todo_ignore_time_comparison_use_seconds",
        "boolean",
        "false",
        "org-agenda-todo-ignore-time-comparison-use-seconds",
        "Compare the dates of the `org_agenda_todo_ignore_*` settings with now to the second instead of \
         by day: `future` is then later than now, and a number counts seconds.",
    ),
    setting(
        "org_tags_match_list_sublevels",
        r#"true, false, "indented""#,
        "true",
        "org-tags-match-list-sublevels",
        "Whether tags matches (agenda, sparse trees) and stuck projects list the entries below one they \
         list: `false` passes over its subtree, `indented` shows them with a dot for each level below the \
         first.",
    ),
    setting(
        "org_agenda_tags_todo_honor_ignore_options",
        "boolean",
        "false",
        "org-agenda-tags-todo-honor-ignore-options",
        "Have the `org_agenda_todo_ignore_*` settings leave entries out of `tags-todo` matches as well.",
    ),
    setting(
        "org_agenda_sorting_strategy",
        "{ agenda?, todo?, tags?, search? } of string[], or string[]",
        r#"{ "agenda": ["habit-down", "time-up", "urgency-down", "category-keep"], "todo": ["urgency-down", "category-keep"], "tags": ["urgency-down", "category-keep"], "search": ["category-keep"] }"#,
        "org-agenda-sorting-strategy",
        "How the lines of a day (`agenda`), the TODO list (`todo`), a tags match (`tags`, `tags-todo` too) \
         and a search (`search`) are ordered: the strategies in turn, each deciding when the ones before do \
         not. A view without a list takes `agenda`'s, and without that `time-up`, `category-keep`, \
         `urgency-down`; one list alone is for every view. `time-up`/`-down` the time of day; \
         `timestamp-`, `scheduled-`, `deadline-`, `ts-` (active) and `tsia-` (inactive) `up`/`down` the date \
         of that kind (in the TODO list and a tags match, the entry's); `priority-` the priority, \
         `urgency-` the priority with how near or overdue the date is; `effort-`, `category-` (and \
         `category-keep`, the order the entries come in), `tag-` (the last tag), `todo-state-` (where the \
         keyword is in the file's list, done ones first), `habit-`, `alpha-` (the headline), `stats-` (the \
         statistics cookie). `user-defined-up` and `-down` call Lisp, which the server does not run.",
    ),
    setting(
        "org_agenda_sort_notime_is_late",
        "boolean",
        "true",
        "org-agenda-sort-notime-is-late",
        "Lines without a time (or without the date a strategy sorts by) come after those with one.",
    ),
    setting(
        "org_agenda_sort_noeffort_is_high",
        "boolean",
        "true",
        "org-agenda-sort-noeffort-is-high",
        "Entries without an effort count as having the highest when sorted, filtered or limited by effort.",
    ),
    Setting {
        default: None,
        unset: "no limit",
        ..setting(
            "org_agenda_max_entries",
            "integer, or { agenda?, todo?, tags?, search? } of integers",
            "",
            "org-agenda-max-entries",
            "How many entries a day of the agenda, the TODO list, a tags match or a search shows at most: one \
             number for all, or one for each kind of view. Other lines, such as those of the time grid, go \
             as well; with a negative number they stay.",
        )
    },
    Setting {
        default: None,
        unset: "no limit",
        ..setting(
            "org_agenda_max_todos",
            "integer, or { agenda?, todo?, tags?, search? } of integers",
            "",
            "org-agenda-max-todos",
            "How many entries with a TODO keyword a view shows at most, the other lines going as well; with a \
             negative number, those stay.",
        )
    },
    Setting {
        default: None,
        unset: "no limit",
        ..setting(
            "org_agenda_max_tags",
            "integer, or { agenda?, todo?, tags?, search? } of integers",
            "",
            "org-agenda-max-tags",
            "How many entries with tags a view shows at most, the other lines going as well; with a negative \
             number, those stay.",
        )
    },
    Setting {
        default: None,
        unset: "no limit",
        ..setting(
            "org_agenda_max_effort",
            "integer, or { agenda?, todo?, tags?, search? } of integers",
            "",
            "org-agenda-max-effort",
            "The most minutes of effort the entries of a view add up to: entries show while their efforts, \
             in order, stay within it (an entry without one counts as the highest, or as -1 with \
             `org_agenda_sort_noeffort_is_high` off).",
        )
    },
    setting(
        "org_agenda_show_future_repeats",
        r#"boolean, "next""#,
        "true",
        "org-agenda-show-future-repeats",
        "Show the future dates of repeating entries: all of them, none, or only the next.",
    ),
    setting("org_agenda_show_all_dates", "boolean", "true", "org-agenda-show-all-dates", "Show days without entries."),
    setting(
        "org_agenda_tags_column",
        r#"number, "auto""#,
        r#""auto""#,
        "org-agenda-tags-column",
        "Where tags are aligned in the agenda: a column (negative: where they end), or `auto` for the right \
         edge of the window. With `auto`, the editors that tell the server how wide the agenda's window is \
         (Neovim, Vim, Sublime Text) have the tags there, and the agenda follows the window as it is \
         resized; the others (VS Code, Zed) have them end at column 80.",
    ),
    setting("org_agenda_use_time_grid", "boolean", "true", "org-agenda-use-time-grid", "Draw the time grid."),
    setting(
        "org_agenda_time_grid",
        "[string[], number[], string, string]",
        r#"[["daily", "today", "require-timed"], [800, 1000, 1200, 1400, 1600, 1800, 2000], "......", "----------------"]"#,
        "org-agenda-time-grid",
        "The time grid, as Org writes it: the days that have it (`daily`: a one-day agenda; `weekly`: every \
         day of a longer one; `today`: today; `require-timed`: only days with an entry that has a time; \
         `remove-match`: no grid line at a time an entry already has), the times of its lines (`800` for \
         8:00), what follows a time that has no end time, and the text of a grid line.",
    ),
    setting(
        "org_agenda_current_time_string",
        "string",
        r#""now - - - - - - - - - - - - - - - - - - - - - - - - -""#,
        "org-agenda-current-time-string",
        "The text of the line for the current time.",
    ),
    setting(
        "org_agenda_show_current_time_in_grid",
        "boolean",
        "true",
        "org-agenda-show-current-time-in-grid",
        "Show the current time in the grid.",
    ),
    setting(
        "org_agenda_weekend_days",
        "number[]",
        "[6, 0]",
        "org-agenda-weekend-days",
        "The days drawn as the weekend (0 is Sunday).",
    ),
    setting(
        "org_agenda_start_with_log_mode",
        "boolean",
        "false",
        "org-agenda-start-with-log-mode",
        "Start the agenda in log mode, which shows what was closed and clocked each day (`l` toggles it).",
    ),
    setting(
        "org_agenda_start_with_clockreport_mode",
        "boolean",
        "false",
        "org-agenda-start-with-clockreport-mode",
        "Start the agenda with the clock report under it (`R` toggles it).",
    ),
    setting(
        "org_agenda_clockreport_parameter_plist",
        "string",
        r#"":link t :maxlevel 2""#,
        "org-agenda-clockreport-parameter-plist",
        "The parameters of the clock report's table, as those of a clock table block (`:maxlevel 3 :tags t`); \
         `:block`, `:tstart`, `:tend` and `:scope` are the agenda's.",
    ),
    Setting {
        default: None,
        unset: "none",
        ..setting(
            "org_agenda_clock_report_header",
            "string",
            "",
            "org-agenda-clock-report-header",
            "Text on a line of its own (or lines) before the clock report.",
        )
    },
    setting(
        "org_agenda_start_with_archives_mode",
        "\"trees\" | \"files\" | false",
        "false",
        "org-agenda-start-with-archives-mode",
        "Start the agenda with entries in archived trees (`trees`; `v a` toggles it), or those and the files the \
         agenda files archive to (`files`; `v A`).",
    ),
    setting(
        "org_agenda_log_mode_items",
        "string[]",
        r#"["closed", "clock"]"#,
        "org-agenda-log-mode-items",
        "What log mode shows: `closed` entries, `clock` lines and `state` changes.",
    ),
    setting(
        "org_agenda_log_mode_add_notes",
        "boolean",
        "true",
        "org-agenda-log-mode-add-notes",
        "Show the note of a state change or a clock line after the headline in log mode.",
    ),
    setting(
        "org_agenda_move_date_from_past_immediately_to_today",
        "boolean",
        "true",
        "org-agenda-move-date-from-past-immediately-to-today",
        "Moving a date of the past one day later from the agenda (`S-<right>`) takes it to today.",
    ),
    setting(
        "org_agenda_persistent_marks",
        "boolean",
        "false",
        "org-agenda-persistent-marks",
        "Keep the marks of a bulk action (`B`) after it (`p` in its menu toggles this for one action).",
    ),
    setting(
        "org_agenda_show_outline_path",
        r#"boolean, "title""#,
        "true",
        "org-agenda-show-outline-path",
        "As the cursor moves in the agenda, show where the entry of its line is: the file's name and the \
         headlines above it (`work.org/Projects/Report`), in the echo area or the status bar. With `title` \
         the file's `#+TITLE:` comes first instead of its name.",
    ),
    setting(
        "org_agenda_start_with_follow_mode",
        "boolean",
        "false",
        "org-agenda-start-with-follow-mode",
        "Start the agenda in follow mode, which shows the entry of the cursor's line in another window as the \
         cursor moves (`F` toggles it).",
    ),
    setting(
        "org_agenda_start_with_entry_text_mode",
        "boolean",
        "false",
        "org-agenda-start-with-entry-text-mode",
        "Start the agenda with some text of each entry under its line (`E` toggles it).",
    ),
    setting(
        "org_agenda_entry_text_maxlines",
        "integer",
        "5",
        "org-agenda-entry-text-maxlines",
        "The most lines of an entry's text `E` shows.",
    ),
    setting(
        "org_agenda_entry_text_leaders",
        "string",
        r#""    > ""#,
        "org-agenda-entry-text-leaders",
        "What each line of an entry's text starts with in the agenda.",
    ),
    setting(
        "org_agenda_entry_text_exclude_regexps",
        "string[]",
        "[]",
        "org-agenda-entry-text-exclude-regexps",
        "Emacs regexps whose matches are left out of the entry text `E` shows.",
    ),
    setting(
        "org_agenda_block_separator",
        "string | false",
        r#""=""#,
        "org-agenda-block-separator",
        "What goes between the blocks of a block agenda: one character repeated across 79 columns, a longer text as it is, or `false` for nothing. In a block's `settings`, the separator above that block.",
    ),
    setting(
        "org_agenda_compact_blocks",
        "boolean",
        "false",
        "org-agenda-compact-blocks",
        "Leave out the separators between blocks and the header over agenda blocks.",
    ),
    setting(
        "agenda_hide_empty_blocks",
        "boolean",
        "false",
        "",
        "Leave out the blocks of a block agenda that show no entries. Not in Org.",
    ),
    setting(
        "org_super_agenda_mode",
        "boolean",
        "false",
        "org-super-agenda-mode",
        "org-super-agenda: show the lines of the agenda's views in the groups of `org_super_agenda_groups`, \
         each under a header. Off, the groups are not used.",
    ),
    setting(
        "org_super_agenda_groups",
        "object[]",
        "[]",
        "org-super-agenda-groups",
        "org-super-agenda's groups, in turn taking the lines they select: each an object of selectors and \
         their arguments in Emacs's order, `{ name = \"Today\", time-grid = true }`, or a list of them in turn; \
         `true` is `t`, `false` is `nil` (and for `name`, `none`: no header). The lines no group takes come \
         last, under `org_super_agenda_unmatched_name`. In a custom command's `settings`, the groups of that \
         command or block.",
    ),
    setting(
        "org_super_agenda_unmatched_name",
        "string",
        r#""Other items""#,
        "org-super-agenda-unmatched-name",
        "The header of the lines no group takes.",
    ),
    setting(
        "org_super_agenda_unmatched_order",
        "integer",
        "99",
        "org-super-agenda-unmatched-order",
        "Where the lines no group takes come among the groups: groups show lowest `order` first.",
    ),
    setting(
        "org_super_agenda_header_separator",
        "string",
        r#""\n""#,
        "org-super-agenda-header-separator",
        "What goes before each header: text, or one character repeated across the window, then a newline.",
    ),
    setting(
        "org_super_agenda_header_prefix",
        "string",
        r#"" ""#,
        "org-super-agenda-header-prefix",
        "What each header starts with.",
    ),
    setting(
        "org_super_agenda_final_group_separator",
        "string",
        r#""""#,
        "org-super-agenda-final-group-separator",
        "What goes after the last group: text put at the end of its last line, or one character repeated \
         across the window on a line of its own.",
    ),
    setting(
        "org_super_agenda_date_format",
        "string",
        r#""%e %B %Y""#,
        "org-super-agenda-date-format",
        "The headers of the groups of `auto-planning` and `auto-ts`, as `format-time-string` writes dates.",
    ),
    setting(
        "org_super_agenda_hide_empty_groups",
        "boolean",
        "false",
        "org-super-agenda-hide-empty-groups",
        "Leave out the headers of groups whose lines the agenda's filters all hide.",
    ),
    setting(
        "org_super_agenda_keep_order",
        "boolean",
        "false",
        "org-super-agenda-keep-order",
        "Sort the lines of each group again, as the view sorts its lines: a group of several selectors puts \
         the lines of its first selector first.",
    ),
    setting(
        "org_super_agenda_group_property_name",
        "string",
        r#""agenda-group""#,
        "org-super-agenda-group-property-name",
        "The property `auto-group` groups lines by.",
    ),
    setting(
        "org_super_agenda_properties_inherit",
        "boolean",
        "true",
        "org-super-agenda-properties-inherit",
        "Look for the properties of `property`, `auto-property` and `auto-group` in the entries above too.",
    ),
    setting(
        "org_roam_enabled",
        "boolean",
        "false",
        "",
        "org-roam: the files of `org_roam_directory` are a network of notes, whose nodes (files and entries \
         with an `ID`) are found, linked to and made with the `org.roam.*` commands. Off, those commands only \
         say so.",
    ),
    setting(
        "org_roam_directory",
        "string",
        r#""~/org-roam/""#,
        "org-roam-directory",
        "The directory of org-roam's files, in all its subdirectories. `~/` is the home directory.",
    ),
    setting(
        "org_roam_file_extensions",
        "string[]",
        r#"["org"]"#,
        "org-roam-file-extensions",
        "The extensions of org-roam's files; each may be followed by `.gpg` or `.age`, which the server \
         cannot read.",
    ),
    setting(
        "org_roam_file_exclude_regexp",
        "string or string[]",
        r#"["data/"]"#,
        "org-roam-file-exclude-regexp",
        "Emacs regexps for the files that are not org-roam's, matched against their names relative to \
         `org_roam_directory`; the default leaves out attachments.",
    ),
    setting(
        "org_roam_node_display_template",
        "string",
        r#""${title}""#,
        "org-roam-node-display-template",
        "How a node is offered: `${field}` for its `title`, `tags`, `todo`, `file`, `olp`, `aliases` and \
         the other fields of `org-roam-node`, `${field:N}` in N columns, `${field:*}` in what is left of the \
         window's 79 columns.",
    ),
    setting(
        "org_roam_link_auto_replace",
        "boolean",
        "true",
        "org-roam-link-auto-replace",
        "Turn the `roam:` links of org-roam's files into `id:` links to their nodes as the files are saved, \
         and as a link is followed.",
    ),
    setting(
        "org_roam_completion_everywhere",
        "boolean",
        "false",
        "org-roam-completion-everywhere",
        "In org-roam's files, offer the titles of the nodes for the word at the cursor, not only in a link; \
         the one picked becomes a link to its node.",
    ),
    setting(
        "org_roam_mode_sections",
        "string[]",
        r#"["backlinks", "reflinks"]"#,
        "org-roam-mode-sections",
        "The sections of the org-roam buffer, in turn: `backlinks` (the nodes that link to the node), \
         `reflinks` (those that link to or cite its references) and `unlinked-references` (the places that \
         name it without a link, found by the server itself).",
    ),
    setting(
        "org_roam_capture_templates",
        "object[]",
        r##"[{ "key": "d", "description": "default", "type": "plain", "template": "%?", "target": ["file+head", "%<%Y%m%d%H%M%S>-${slug}.org", "#+title: ${title}\n"], "unnarrowed": true }]"##,
        "org-roam-capture-templates",
        "The templates of new nodes, as `org_capture_templates` writes them, with org-roam's targets: \
         `file` and a path, `file+head`, a path and the text a new file starts with, `file+olp`, a path and the \
         headlines of an outline path, `file+head+olp`, both, and `node` and the title, alias or `ID` of a node. \
         The path, relative to `org_roam_directory`, and the texts may have `${title}`, `${slug}`, `${id}` and \
         `${name}`, which asks for `name` (`${name=default}` starts the answer with `default`), besides the \
         escapes of `org_capture_templates`. `unnarrowed` shows the whole file in the capture buffer.",
    ),
    setting(
        "org_roam_dailies_directory",
        "string",
        r#""daily/""#,
        "org-roam-dailies-directory",
        "Where the daily notes are, relative to `org_roam_directory`.",
    ),
    setting(
        "org_roam_dailies_capture_templates",
        "object[]",
        r##"[{ "key": "d", "description": "default", "type": "entry", "template": "* %?", "target": ["file+head", "%<%Y-%m-%d>.org", "#+title: %<%Y-%m-%d>\n"] }]"##,
        "org-roam-dailies-capture-templates",
        "The templates of the daily notes, as `org_roam_capture_templates` writes them, with paths relative to \
         `org_roam_dailies_directory`; `%<…>` is the note's day.",
    ),
    setting(
        "org_journal_enabled",
        "boolean",
        "false",
        "",
        "org-journal: a journal in `org_journal_dir`, a file a day (or week, month, year), written with the \
         `org.journal.*` commands. Off, those commands only say so.",
    ),
    setting(
        "org_journal_dir",
        "string",
        r#""~/Documents/journal/""#,
        "org-journal-dir",
        "The directory of the journal's files. `~/` is the home directory.",
    ),
    setting(
        "org_journal_file_type",
        "string",
        r#""daily""#,
        "org-journal-file-type",
        "A file a day (`daily`), week (`weekly`), month (`monthly`) or year (`yearly`); but daily, each entry \
         has its date in a `CREATED` property.",
    ),
    setting(
        "org_journal_start_on_weekday",
        "number",
        "1",
        "org-journal-start-on-weekday",
        "The day a weekly file starts on: 1 is Monday, 7 Sunday.",
    ),
    setting(
        "org_journal_file_format",
        "string",
        r#""%Y%m%d""#,
        "org-journal-file-format",
        "The names of the files, as `format-time-string` writes the date their entries start on: with `%Y`, \
         `%m` and `%d` (a yearly file needs only `%Y`, a monthly one `%Y` and `%m`). A name without `.org` is \
         an Org file to org-journal, but not to most editors.",
    ),
    setting(
        "org_journal_date_format",
        "string",
        r#""%A, %x""#,
        "org-journal-date-format",
        "The date of an entry, after `org_journal_date_prefix`, as `format-time-string` writes it, in the C \
         locale: `%x` is `10/09/26`.",
    ),
    setting(
        "org_journal_date_prefix",
        "string",
        r#""* ""#,
        "org-journal-date-prefix",
        "What comes before the date of an entry: a headline, or `#+title: ` in a daily file.",
    ),
    setting(
        "org_journal_time_format",
        "string",
        r#""%R ""#,
        "org-journal-time-format",
        "The time of a new entry of the day, as `format-time-string` writes it; empty for none.",
    ),
    setting(
        "org_journal_time_format_post_midnight",
        "string",
        r#""""#,
        "org-journal-time-format-post-midnight",
        "The time of an entry made after midnight while it is still yesterday (`org_extend_today_until`); \
         empty for `org_journal_time_format`.",
    ),
    setting(
        "org_journal_time_prefix",
        "string",
        r#""** ""#,
        "org-journal-time-prefix",
        "What comes before the time of an entry: its stars give its level.",
    ),
    setting(
        "org_journal_hide_entries_p",
        "boolean",
        "true",
        "org-journal-hide-entries-p",
        "Fold all but the new entry.",
    ),
    setting(
        "org_journal_carryover_items",
        "string",
        r#""TODO=\"TODO\"""#,
        "org-journal-carryover-items",
        "The items of the entry before that move to today's, with the headlines above them: a match of the \
         tags view (`m`); empty for none.",
    ),
    setting(
        "org_journal_skip_carryover_drawers",
        "string[]",
        "[]",
        "org-journal-skip-carryover-drawers",
        "Drawers left out of the items carried over.",
    ),
    setting(
        "org_journal_carryover_delete_empty_journal",
        "string",
        r#""never""#,
        "org-journal-carryover-delete-empty-journal",
        "What happens to an entry or file left empty by the carryover: `never` deleted, `always`, or `ask`.",
    ),
    setting(
        "org_journal_file_header",
        "string",
        r#""""#,
        "org-journal-file-header",
        "What a new file starts with, as `format-time-string` writes it: `#+title: Journal %Y`.",
    ),
    setting(
        "org_journal_created_property_timestamp_format",
        "string",
        r#""%Y%m%d""#,
        "org-journal-created-property-timestamp-format",
        "The date in the `CREATED` property of an entry of a file that is not daily.",
    ),
    setting(
        "org_journal_scheduled_string",
        "string",
        r#""""#,
        "org-journal-scheduled-string",
        "What comes before the timestamp of an entry for a day to come (`org.journal.new.scheduled.entry`).",
    ),
    setting(
        "org_journal_enable_encryption",
        "boolean",
        "false",
        "org-journal-enable-encryption",
        "Tag the date of a new entry with `crypt`, for `org-crypt` to encrypt (`org.crypt.encrypt.all`, or as \
         the file is saved with `org-crypt-use-before-save-magic` in the Emacs configuration).",
    ),
    setting(
        "org_journal_enable_agenda_integration",
        "boolean",
        "false",
        "org-journal-enable-agenda-integration",
        "Add the journal's files of today and the days to come to the agenda's files.",
    ),
    setting(
        "org_journal_search_result_date_format",
        "string",
        r#""%A, %x""#,
        "org-journal-search-result-date-format",
        "The date of each result of a search of the journal, as `format-time-string` writes it.",
    ),
    setting(
        "org_journal_search_results_order_by",
        "string",
        r#""asc""#,
        "org-journal-search-results-order-by",
        "The order of the results of a search of the journal: `asc`, the first day first, or `desc`, the last.",
    ),
    setting(
        "org_extend_today_until",
        "number",
        "0",
        "org-extend-today-until",
        "The hour before which it is still yesterday: for the agenda's today, the date prompt with nothing to \
         start from, the times capture templates put in and the day of a datetree, `.+` and `++` repeaters, \
         clock report ranges and org-journal's new entries.",
    ),
    setting(
        "org_use_effective_time",
        "boolean",
        "false",
        "org-use-effective-time",
        "Before the hour `org_extend_today_until`, a TODO change records 23:59 of the day before: in \
         `CLOSED:`, `LAST_REPEAT` and the notes' timestamps. `org.todo.yesterday` does so at any hour.",
    ),
    setting(
        "org_use_last_clock_out_time_as_effective_time",
        "boolean",
        "false",
        "org-use-last-clock-out-time-as-effective-time",
        "A TODO change records the end of the first closed clock of the subtree instead of the time now, \
         when there is one; this comes before `org_use_effective_time`.",
    ),
    setting(
        "org_agenda_search_view_always_boolean",
        "boolean",
        "false",
        "org-agenda-search-view-always-boolean",
        "Read every search string of the search view (`s`) as words that must (`+word`, `word`) or must not \
         (`-word`) occur, regexps in braces (`{regexp}`) and `\"quoted phrases\"`. Off, it is a phrase whose spaces match any \
         whitespace, unless its first word starts with `+`, `-` or `{`.",
    ),
    setting(
        "org_agenda_search_view_force_full_words",
        "boolean",
        "false",
        "org-agenda-search-view-force-full-words",
        "The words of a Boolean search match whole words only, as a search string starting with `:` asks.",
    ),
    setting(
        "org_agenda_search_view_max_outline_level",
        "number",
        "0",
        "org-agenda-search-view-max-outline-level",
        "The search view shows headlines down to this level, a match below one counting for the headline \
         above: 0 for every level.",
    ),
    setting(
        "org_agenda_text_search_extra_files",
        "string[]",
        "[]",
        "org-agenda-text-search-extra-files",
        "Files the search view (`s`) searches besides the agenda files, after them, as `org_agenda_files` \
         names them. `agenda-archives` first adds the files the agenda files archive to. A file listed twice \
         is searched at its last place.",
    ),
    setting(
        "org_agenda_prefix_format",
        "{ agenda?, todo?, tags?, search? }",
        r#"{ "agenda": " %i %-12:c%?-12t% s", "todo": " %i %-12:c", "tags": " %i %-12:c", "search": " %i %-12:c" }"#,
        "org-agenda-prefix-format",
        "What comes before the headline on a line of the agenda (`agenda`), the TODO list (`todo`), tags \
         searches (`tags`) and text searches (`search`): `%c` the category, `%t` the time, `%s` the \
         leader (`Scheduled: `), `%e` the effort, `%l` a blank per level, `%T` the last tag, `%b` the \
         headlines above (breadcrumbs), `%i` the icon (none). A width pads (`%-12c` on the right, `%12c` on \
         the left), `.N` cuts, `%?` leaves out an empty field, and a character before the letter follows a \
         value that is not empty (`%-12:c`). `%(…)` is Emacs Lisp, run by Emacs (the `emacs` option) on the \
         entry's headline with `category`, `extra`, `level` … bound, as Org runs it: \
         `%(org-entry-get nil \"X\")`, or a function of `emacs_config`. Without Emacs it is empty.",
    ),
    setting(
        "org_agenda_todo_keyword_format",
        "string",
        r#""%-1s""#,
        "org-agenda-todo-keyword-format",
        "How a TODO keyword is written in the agenda: `%-4s` pads it to four columns, so that the titles line \
         up; an empty format leaves keywords out.",
    ),
    setting(
        "org_agenda_scheduled_leaders",
        "string[]",
        r#"["Scheduled: ", "Sched.%2dx: "]"#,
        "org-agenda-scheduled-leaders",
        "The leader of a scheduled entry on its day, and on today when its day has passed (`%d`: the days \
         since).",
    ),
    setting(
        "org_agenda_deadline_leaders",
        "string[]",
        r#"["Deadline:  ", "In %3d d.: ", "%2d d. ago: "]"#,
        "org-agenda-deadline-leaders",
        "The leader of a deadline on its day, on today before it (`%d`: the days left), and on today after it \
         (`%d`: the days since).",
    ),
    setting(
        "org_agenda_deadline_faces",
        "[part, face][]",
        r#"[[1.0, "org-imminent-deadline"], [0.5, "org-upcoming-deadline"], [0.0, "org-upcoming-distant-deadline"]]"#,
        "org-agenda-deadline-faces",
        "The look of a deadline in the agenda by the part of its warning time that has passed: the first \
         whose part is no more than that, none past the last. The faces are Org's: `org-imminent-deadline` (or \
         `org-warning`), `org-upcoming-deadline`, `org-upcoming-distant-deadline`; any other name is the plain \
         look.",
    ),
    setting(
        "org_agenda_timerange_leaders",
        "string[]",
        r#"["", "(%d/%d): "]"#,
        "org-agenda-timerange-leaders",
        "The leader of a time range within a day, and of a range over days (`%d`: which day of it, `%d`: how \
         many days).",
    ),
    setting(
        "org_agenda_inactive_leader",
        "string",
        r#""[""#,
        "org-agenda-inactive-leader",
        "What goes before an inactive timestamp's line, when the agenda shows them (`[`, `v [`).",
    ),
    setting(
        "org_agenda_remove_tags",
        r#"boolean, "prefix""#,
        "false",
        "org-agenda-remove-tags",
        "Leave the tags out of the agenda's lines; `prefix` only when the prefix shows a tag (`%T`).",
    ),
    setting(
        "org_stuck_projects",
        "[string, string[], string[], string]",
        r#"["+LEVEL=2/-DONE", ["TODO", "NEXT", "NEXTACTION"], [], ""]"#,
        "org-stuck-projects",
        "What the stuck projects view (`#`) lists: the projects the match string finds whose subtree has no entry \
         with one of the TODO keywords (`*` for any not done) or tags (`*` for any), and no match of the regexp.",
    ),
    setting(
        "org_agenda_breadcrumbs_separator",
        "string",
        r#""->""#,
        "org-agenda-breadcrumbs-separator",
        "What goes between the headlines of `%b`.",
    ),
    Setting {
        default: None,
        unset: "as `Wednesday   7 October 2026`, the week's number added on Mondays",
        ..setting(
            "org_agenda_format_date",
            "string",
            "",
            "org-agenda-format-date",
            "How the agenda writes a day, as `format-time-string` does: `%A %d %B %Y`, `%a %Y-%m-%d`.",
        )
    },
    Setting {
        default: None,
        unset: "today",
        ..setting(
            "org_agenda_start_day",
            "string",
            "",
            "org-agenda-start-day",
            "The day the agenda starts from, as the date prompt reads it: `-1d`, `+2d`, `2026-10-01`.",
        )
    },
    setting(
        "org_agenda_custom_commands_contexts",
        "object[]",
        "[]",
        "org-agenda-custom-commands-contexts",
        "Where the commands of `org_agenda_custom_commands` are offered, as for \
         `org_capture_templates_contexts`.",
    ),
    setting(
        "org_agenda_custom_commands",
        "object[]",
        "[]",
        "org-agenda-custom-commands",
        "Commands of the agenda menu. Each has a `key` and a `description`, and either a `type` (`agenda`, \
         `alltodo`, `todo`, `tags` or `tags-todo`) with a `match` (keywords such as `NEXT|WAIT` for `todo`, \
         a match string such as `work-home` or `PRIORITY=\"A\"` for `tags`), or `blocks`, a list of such \
         views shown one after the other. `header` replaces a view's header (empty: none), and `settings` \
         holds agenda options for the command or a block (`org_agenda_span`, `org_agenda_start_day` …). A command with only a key \
         and a description groups the commands whose keys start with its key.",
    ),
    Setting {
        default: None,
        unset: "from the client's capabilities",
        integration: true,
        ..setting(
            "virtual_documents",
            "boolean",
            "",
            "",
            "Whether the client fetches the agenda with `workspace/textDocumentContent`. A client that \
             announces that capability need not set this.",
        )
    },
    Setting {
        default: None,
        unset: "from the client's capabilities",
        integration: true,
        ..setting(
            "show_document",
            "boolean",
            "",
            "",
            "Whether the client shows the documents the server asks it to show (`window/showDocument`): the \
             agenda, capture buffers, an entry's file. A client that announces that capability need not set \
             this; the Vim plugin does, for the LSP clients that leave the request to it.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "org_client",
            "boolean",
            "false",
            "",
            "The client has org-lsp's editor code: it knows the server's own token types and does what a \
             command's result asks. A client that can announce it does so with `experimental.orgClient`; the \
             Vim, Sublime Text and Zed plugins set this. Without it, the server sends the standard token types \
             of LSP, which editors color without being told, `message_prompts` is on, and so are more code actions \
             when the client shows message requests (`window.showMessage`) or `message_prompts` is set.",
        )
    },
    Setting {
        default: None,
        unset: "true for a client without org-lsp's editor code (`org_client`)",
        integration: true,
        ..setting(
            "message_prompts",
            "boolean",
            "",
            "",
            "Ask what commands ask for with `window/showMessageRequest` (a key, one of a list) or, for text and \
             dates, through a rename at the cursor (`textDocument/rename`, whose new name is the answer), and say \
             what they have to say with `window/showMessage`, for a client that does nothing with a command's \
             result.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "line_highlights",
            "boolean",
            "false",
            "",
            "Whether the client highlights lines of the agenda from edge to edge as the server sends them \
             (`org-lsp/lineHighlights`): the entry the clock runs on. As a client announcing \
             `experimental.orgLineHighlights` does: for the Vim plugin and the Sublime Text package.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "images",
            "boolean",
            "false",
            "",
            "Whether the client shows images in place of links to image files and of LaTeX fragments \
             (`org.link.preview`, `org.latex.preview`, `org-lsp/images`). As a client announcing \
             `experimental.orgImages` does: for the Sublime Text package.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "view_changed",
            "boolean",
            "false",
            "",
            "Whether the server says when it has written the agenda's or the date prompt's file anew \
             (`org-lsp/viewChanged` `{ uri }`), for a \
             client that does not see the file change on disk and reads it again when told: the Vim plugin.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "date_prompt",
            "boolean",
            "false",
            "",
            "Whether the client shows the date prompt's calendar, for a client that cannot announce \
             `experimental.orgDatePrompt`: the Vim plugin. Without documents fetched from the server, the calendar \
             is a file the server writes, which the client reads again when told (`view_changed`).",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "virtual_indent",
            "boolean",
            "false",
            "",
            "Whether the client shows `org-indent-mode`'s prefixes and hidden stars that the server sends \
             (`org-lsp/virtualIndent`), as a client announcing `experimental.orgVirtualIndent` does: for the \
             Vim plugin and the Sublime Text package, whose LSP clients announce nothing of the kind. A client \
             that shows neither gets the prefixes as inlay hints instead.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "command_actions",
            "boolean",
            "false",
            "",
            "Offer more commands as code actions: the agenda, capture, refile, a TODO state or a priority picked \
             from a list, a new heading or item (M-RET and its neighbours), for a client that cannot run a \
             command from a key. The agenda's commands go through a local port instead: the server writes a \
             script (`command_script`) that runs `SCRIPT COMMAND [ROW COLUMN]` in the agenda, where the cursor \
             is when no row is given (the server follows the cursor through the document highlights the client \
             asks for), for the editor's keys to call; with no agenda open, the commands that need none (the \
             agenda, the TODO list, the agenda menu, capture). With org-lsp's editor code (`org_client`), the \
             agenda's commands are not code actions and its file is beside the script, the same one for the same \
             folders, and the server takes over the one the editor opens again when it starts. The Zed extension \
             sets it; a client without the editor code needs none of it.",
        )
    },
    Setting {
        integration: true,
        ..setting(
            "command_script",
            "string",
            r#""~/.local/state/org-lsp/agenda-command""#,
            "",
            "Where the server writes the script of `command_actions`. Of several servers, the one whose agenda \
             the cursor was last in has it, and a server that starts takes it when no other answers there. It \
             stays when the server stops: run then, it opens the Org file last opened, in its folder, with the \
             editor's command line (`zed`), and sends the command to the server that starts for it.",
        )
    },
];

/// The strategies of `org-agenda-sorting-strategy`.
macro_rules! strategy {
    () => {
        r#"{ "enum": ["time-up", "time-down", "timestamp-up", "timestamp-down", "scheduled-up", "scheduled-down",
            "deadline-up", "deadline-down", "tsia-up", "tsia-down", "ts-up", "ts-down", "stats-up", "stats-down",
            "priority-up", "priority-down", "urgency-up", "urgency-down", "effort-up", "effort-down", "category-up",
            "category-down", "category-keep", "tag-up", "tag-down", "todo-state-up", "todo-state-down", "habit-up",
            "habit-down", "alpha-up", "alpha-down", "user-defined-up", "user-defined-down"] }"#
    };
}

/// The values each option takes, as JSON Schema: what editors complete and
/// check the options against (VS Code's settings, the schema of the shared
/// file, Neovim's type annotations). An object that is a type of its own
/// names it in `x-lua-class`.
const SCHEMAS: &[(&str, &str)] = &[
    ("config_file", r#"{ "anyOf": [{ "const": false }, { "type": "string" }] }"#),
    ("org_agenda_files", r#"{ "type": "array", "items": { "type": "string" } }"#),
    (
        "org_todo_keywords",
        r#"{ "anyOf": [
            { "type": "array", "items": { "type": "string" } },
            { "type": "array", "items": { "type": "array", "items": { "type": "string" } } }
        ] }"#,
    ),
    ("org_todo_keyword_faces", r#"{ "type": "object", "additionalProperties": { "type": "string" } }"#),
    ("org_priority_faces", r#"{ "type": "object", "additionalProperties": { "type": "string" } }"#),
    ("org_tag_faces", r#"{ "type": "object", "additionalProperties": { "type": "string" } }"#),
    ("org_log_done", r#"{ "enum": [false, "time", "note"] }"#),
    ("org_log_repeat", r#"{ "enum": [false, "time", "note"] }"#),
    ("org_agenda_dim_blocked_tasks", r#"{ "enum": [true, false, "invisible"] }"#),
    ("org_list_use_circular_motion", BOOLEAN),
    (
        "org_M_RET_may_split_line",
        r#"{ "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "headline": { "type": "boolean" }, "item": { "type": "boolean" }, "table": { "type": "boolean" }, "default": { "type": "boolean" } }, "additionalProperties": false }] }"#,
    ),
    (
        "org_blank_before_new_entry",
        r#"{ "type": "object", "properties": { "heading": { "enum": [true, false, "auto"] }, "plain_list_item": { "enum": [true, false, "auto"] } }, "additionalProperties": false }"#,
    ),
    ("org_insert_heading_respect_content", BOOLEAN),
    (
        "org_special_ctrl_a_e",
        r#"{ "anyOf": [{ "enum": [true, false, "reversed"] }, { "type": "array", "items": { "enum": [true, false, "reversed"] }, "minItems": 2, "maxItems": 2 }] }"#,
    ),
    ("org_special_ctrl_k", BOOLEAN),
    ("org_sparse_tree_open_archived_trees", BOOLEAN),
    ("org_yank_adjusted_subtrees", BOOLEAN),
    ("org_allow_promoting_top_level_subtree", BOOLEAN),
    ("org_list_two_spaces_after_bullet_regexp", r#"{ "anyOf": [{ "const": false }, { "type": "string" }] }"#),
    (
        "org_list_demote_modify_bullet",
        r#"{ "type": "object", "additionalProperties": { "enum": ["-", "+", "*", "1.", "1)"] } }"#,
    ),
    ("org_list_indent_offset", r#"{ "type": "integer", "minimum": 0 }"#),
    (
        "org_list_automatic_rules",
        r#"{ "type": "object", "properties": { "checkbox": { "type": "boolean" }, "indent": { "type": "boolean" } }, "additionalProperties": false }"#,
    ),
    ("org_checkbox_hierarchical_statistics", BOOLEAN),
    (
        "org_table_formula_constants",
        r#"{ "type": "object", "additionalProperties": { "type": ["string", "number"] } }"#,
    ),
    ("org_table_relative_ref_may_cross_hline", r#"{ "enum": [true, false, "error"] }"#),
    ("org_table_formula_create_columns", BOOLEAN),
    ("org_table_duration_custom_format", r#"{ "enum": ["seconds", "minutes", "hours", "days", "hh:mm"] }"#),
    ("org_table_duration_hour_zero_padding", BOOLEAN),
    (
        "org_calc_default_modes",
        r#"{ "type": "object", "properties": { "float_format": { "type": "array", "minItems": 2, "maxItems": 2, "items": [{ "enum": ["float", "fix", "sci"] }, { "type": "integer", "minimum": 0 }] }, "angle_mode": { "enum": ["deg", "rad"] } }, "additionalProperties": false }"#,
    ),
    ("org_table_formula_evaluate_inline", BOOLEAN),
    ("org_table_allow_automatic_line_recalculation", BOOLEAN),
    ("org_table_number_regexp", STRING),
    ("org_table_number_fraction", r#"{ "type": "number", "minimum": 0 }"#),
    ("org_table_automatic_realign", BOOLEAN),
    ("org_table_tab_jumps_over_hlines", BOOLEAN),
    ("org_table_shrunk_column_indicator", STRING),
    ("org_enforce_todo_dependencies", BOOLEAN),
    ("org_enforce_todo_checkbox_dependencies", BOOLEAN),
    ("org_log_refile", r#"{ "enum": [false, "time", "note"] }"#),
    ("org_log_reschedule", r#"{ "enum": [false, "time", "note"] }"#),
    ("org_log_redeadline", r#"{ "enum": [false, "time", "note"] }"#),
    ("org_log_into_drawer", r#"{ "type": ["boolean", "string"] }"#),
    ("org_log_done_with_time", BOOLEAN),
    (
        "org_log_note_headings",
        r#"{ "type": "object", "properties": { "done": { "type": "string" }, "state": { "type": "string" }, "note": { "type": "string" }, "reschedule": { "type": "string" }, "delschedule": { "type": "string" }, "redeadline": { "type": "string" }, "deldeadline": { "type": "string" }, "refile": { "type": "string" }, "clock_out": { "type": "string" } }, "additionalProperties": false }"#,
    ),
    ("org_log_states_order_reversed", BOOLEAN),
    ("org_log_note_clock_out", BOOLEAN),
    (
        "org_provide_todo_statistics",
        r#"{ "anyOf": [{ "type": "boolean" }, { "const": "all-headlines" }, { "type": "array", "items": { "type": "string" } }, { "type": "array", "items": { "type": "array", "items": { "type": "string" } }, "minItems": 2, "maxItems": 2 }] }"#,
    ),
    ("org_hierarchical_todo_statistics", BOOLEAN),
    ("org_closed_keep_when_no_todo", BOOLEAN),
    (
        "org_todo_state_tags_triggers",
        r#"{ "type": "object", "additionalProperties": { "type": "array", "items": { "type": "string", "pattern": "^[+-]" } } }"#,
    ),
    ("org_log_state_notes_insert_after_drawers", BOOLEAN),
    ("org_tags_column", r#"{ "type": "integer" }"#),
    ("org_use_tag_inheritance", r#"{ "type": ["boolean", "array", "string"], "items": { "type": "string" } }"#),
    ("org_tags_exclude_from_inheritance", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_use_property_inheritance", r#"{ "type": ["boolean", "array", "string"], "items": { "type": "string" } }"#),
    ("org_global_properties", r#"{ "type": "object", "additionalProperties": { "type": "string" } }"#),
    ("org_complete_tags_always_offer_all_agenda_tags", BOOLEAN),
    ("org_cite_global_bibliography", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_link_abbrev_alist", r#"{ "type": "object", "additionalProperties": { "type": "string" } }"#),
    (
        "org_file_apps",
        r#"{ "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }"#,
    ),
    ("org_tag_alist", r#"{ "type": ["string", "array"], "items": { "type": "string" } }"#),
    ("org_tag_persistent_alist", r#"{ "type": ["string", "array"], "items": { "type": "string" } }"#),
    ("org_group_tags", BOOLEAN),
    ("org_use_fast_todo_selection", r#"{ "enum": ["auto", "expert", true, false] }"#),
    ("org_use_fast_tag_selection", r#"{ "enum": [true, false, "auto"] }"#),
    ("org_fast_tag_selection_single_key", BOOLEAN),
    ("org_tags_sort_function", r#"{ "enum": ["string<", "string>"] }"#),
    ("org_auto_align_tags", BOOLEAN),
    ("org_clock_into_drawer", r#"{ "type": ["boolean", "string"] }"#),
    ("org_clock_out_remove_zero_time_clocks", BOOLEAN),
    ("org_clock_out_when_done", BOOLEAN),
    (
        "org_clock_display_default_range",
        r#"{ "enum": ["today", "yesterday", "thisweek", "lastweek", "thismonth", "lastmonth", "thisyear", "lastyear", "untilnow"] }"#,
    ),
    ("org_clock_idle_time", r#"{ "type": "integer", "minimum": 1 }"#),
    ("org_clock_history_length", r#"{ "type": "integer", "minimum": 1 }"#),
    ("org_clock_clocktable_default_properties", STRING),
    ("org_clocktable_defaults", STRING),
    (
        "org_clock_clocktable_language_setup",
        r#"{ "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 1 } }"#,
    ),
    ("org_clock_report_include_clocking_task", BOOLEAN),
    ("org_clock_rounding_minutes", r#"{ "type": "integer" }"#),
    ("org_clock_continuously", BOOLEAN),
    ("org_clock_total_time_cell_format", STRING),
    ("org_clock_file_time_cell_format", STRING),
    ("org_clock_in_switch_to_state", STRING),
    ("org_clock_out_switch_to_state", STRING),
    (
        "org_id_link_to_org_use_id",
        r#"{ "enum": [true, false, "create-if-interactive", "create-if-interactive-and-no-custom-id", "use-existing"] }"#,
    ),
    ("org_id_method", r#"{ "enum": ["uuid", "uuidgen", "org", "ts"] }"#),
    ("org_id_prefix", r#"{ "anyOf": [{ "const": false }, { "type": "string" }] }"#),
    ("org_id_ts_format", STRING),
    ("org_id_include_domain", BOOLEAN),
    ("org_link_search_must_match_exact_headline", r#"{ "enum": [true, false, "query-to-create"] }"#),
    ("org_link_doi_server_url", STRING),
    ("org_link_context_for_files", BOOLEAN),
    ("org_lint", BOOLEAN),
    ("org_lint_ignore", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_id_link_use_context", BOOLEAN),
    ("org_id_link_consider_parent_id", BOOLEAN),
    ("org_link_file_path_type", r#"{ "enum": ["adaptive", "absolute", "noabbrev", "relative"] }"#),
    ("org_link_keep_stored_after_insertion", BOOLEAN),
    ("org_todo_repeat_to_state", r#"{ "anyOf": [{ "type": "string" }, { "const": true }] }"#),
    ("org_priority_highest", PRIORITY),
    ("org_priority_lowest", PRIORITY),
    ("org_priority_default", PRIORITY),
    ("org_priority_start_cycle_with_default", BOOLEAN),
    ("org_time_stamp_rounding_minutes", r#"{ "type": "integer", "minimum": 0 }"#),
    ("org_read_date_prefer_future", r#"{ "enum": [true, false, "time"] }"#),
    ("org_agenda_jump_prefer_future", r#"{ "enum": [true, false, "time"] }"#),
    ("org_read_date_force_compatible_dates", BOOLEAN),
    ("org_refile_targets", REFILE_TARGETS),
    ("org_id_completion_targets", REFILE_TARGETS),
    ("org_id_track_globally", BOOLEAN),
    ("org_id_search_archives", BOOLEAN),
    ("org_id_extra_files", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_id_locations_file", STRING),
    ("org_refile_use_outline_path", r#"{ "enum": [true, false, "file", "full-file-path", "title"] }"#),
    ("org_refile_allow_creating_parent_nodes", r#"{ "enum": [true, false, "confirm"] }"#),
    ("org_refile_target_verify_function", STRING),
    ("org_reverse_note_order", BOOLEAN),
    ("org_archive_location", STRING),
    ("org_archive_reversed_order", BOOLEAN),
    ("org_archive_mark_done", r#"{ "type": ["boolean", "string"] }"#),
    ("org_archive_subtree_add_inherited_tags", r#"{ "enum": [true, false, "infile"] }"#),
    (
        "org_archive_save_context_info",
        r#"{ "type": "array", "items": { "enum": ["time", "file", "olpath", "category", "todo", "itags", "ltags", "olid"] } }"#,
    ),
    ("org_archive_sibling_heading", STRING),
    ("org_archive_file_header_format", r#"{ "anyOf": [{ "type": "string" }, { "const": false }] }"#),
    ("org_archive_default_command", r#"{ "enum": ["subtree", "sibling", "tag"] }"#),
    ("org_capture_templates_contexts", CONTEXTS),
    ("org_agenda_custom_commands_contexts", CONTEXTS),
    (
        "org_capture_templates",
        r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.CaptureTemplate", "required": ["key", "description"], "additionalProperties": false, "properties": {
            "key": { "type": "string", "description": "The keys that pick the template" },
            "description": { "type": "string", "description": "What the menu shows" },
            "type": { "enum": ["entry", "plain", "item", "checkitem", "table-line"], "description": "An Org entry (the default), plain text, a list item, a checkbox item or a table line" },
            "target": { "type": "array", "items": { "type": "string" },
                "description": "`file` and a path; `file+headline`, a path and a headline; `file+olp`, a path and the headlines of an outline path; `file+regexp`, a path and a regular expression; `file+olp+datetree`, a path and maybe an outline path; `id` and an ID; `clock`; `file+function`, a path and an Emacs Lisp function; or `function` and an Emacs Lisp function that visits the file" },
            "template": { "anyOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }], "description": "The template, or its lines" },
            "prepend": { "type": "boolean", "description": "First under the target, not last" },
            "empty_lines": { "type": "integer", "minimum": 0, "description": "Blank lines before and after" },
            "empty_lines_before": { "type": "integer", "minimum": 0, "description": "Blank lines before" },
            "empty_lines_after": { "type": "integer", "minimum": 0, "description": "Blank lines after" },
            "immediate_finish": { "type": "boolean", "description": "Capture without a capture buffer" },
            "tree_type": { "anyOf": [{ "enum": ["day", "week", "month"] }, { "type": "array", "items": { "enum": ["year", "quarter", "month", "week", "day"] } }], "description": "The levels of a date tree" },
            "time_prompt": { "type": "boolean", "description": "Ask for the date of the date tree entry" },
            "table_line_pos": { "type": "string", "description": "Where a table line goes: `II-1` is above the second rule" },
            "clock_in": { "type": "boolean", "description": "Clock the captured entry while the capture goes on" },
            "clock_keep": { "type": "boolean", "description": "Keep the clock running when the capture is filed" },
            "clock_resume": { "type": "boolean", "description": "Start the clock the capture stopped again" },
            "jump_to_captured": { "type": "boolean", "description": "Show the captured entry in its file" },
            "unnarrowed": { "type": "boolean", "description": "Show the whole target file in the capture buffer" }
        } } }"#,
    ),
    ("org_directory", STRING),
    ("org_default_notes_file", STRING),
    ("org_link_descriptive", BOOLEAN),
    ("org_hide_emphasis_markers", BOOLEAN),
    ("org_fontify_emphasized_text", BOOLEAN),
    (
        "org_emphasis_alist",
        r#"{ "type": "object", "propertyNames": { "enum": ["*", "/", "_", "=", "~", "+"] }, "additionalProperties": { "anyOf": [{ "const": false }, { "type": "string" }, { "type": "array", "items": { "type": "string" } }] } }"#,
    ),
    (
        "org_hidden_keywords",
        r#"{ "type": "array", "items": { "enum": ["title", "subtitle", "author", "email", "date"] } }"#,
    ),
    ("org_hide_macro_markers", BOOLEAN),
    ("org_display_custom_times", BOOLEAN),
    (
        "org_timestamp_custom_formats",
        r#"{ "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }"#,
    ),
    ("org_pretty_entities", BOOLEAN),
    ("org_pretty_entities_include_sub_superscripts", BOOLEAN),
    ("org_use_sub_superscripts", r#"{ "enum": [true, false, "{}"] }"#),
    ("return_uses_meta_return", BOOLEAN),
    ("org_return_follows_link", BOOLEAN),
    ("org_goto_max_level", INTEGER),
    ("calendar_months", r#"{ "enum": [1, 3] }"#),
    ("calendar_week_start_day", r#"{ "type": "integer", "minimum": 0, "maximum": 6 }"#),
    ("emacs", STRING),
    ("emacs_config_file", STRING),
    ("emacs_config", STRING),
    ("auto_save", BOOLEAN),
    ("org_cycle_separator_lines", r#"{ "type": "integer" }"#),
    ("org_cycle_include_plain_lists", r#"{ "enum": [true, false, "integrate"] }"#),
    ("org_cycle_max_level", r#"{ "type": "integer", "minimum": 1 }"#),
    ("org_cycle_global_at_bob", BOOLEAN),
    ("org_cycle_level_after_item_entry_creation", BOOLEAN),
    (
        "org_startup_folded",
        r#"{ "enum": ["overview", "fold", "content", "show2levels", "show3levels", "show4levels", "show5levels", "showeverything", "nofold", "showall"] }"#,
    ),
    ("org_startup_shrink_all_tables", BOOLEAN),
    ("org_cycle_hide_block_startup", BOOLEAN),
    ("org_cycle_hide_drawer_startup", BOOLEAN),
    ("org_cycle_open_archived_trees", BOOLEAN),
    ("org_adapt_indentation", r#"{ "enum": [true, false, "headline-data"] }"#),
    ("org_startup_with_link_previews", BOOLEAN),
    (
        "org_image_actual_width",
        r#"{ "anyOf": [{ "type": "boolean" }, { "type": "number" }, { "type": "array", "items": { "type": "number" }, "maxItems": 1 }] }"#,
    ),
    ("org_image_max_width", r#"{ "anyOf": [{ "enum": ["fill-column", "window", null] }, { "type": "number" }] }"#),
    ("org_startup_with_latex_preview", BOOLEAN),
    ("org_startup_indented", BOOLEAN),
    ("org_hide_leading_stars", BOOLEAN),
    (
        "org_duration_format",
        r#"{ "anyOf": [{ "enum": ["h:mm", "h:mm:ss"] }, { "type": "array", "items": { "anyOf": [{ "const": "compact" }, { "type": "array", "minItems": 2, "maxItems": 2 }] } }] }"#,
    ),
    (
        "org_duration_units",
        r#"{ "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "number" }], "minItems": 2, "maxItems": 2 } }"#,
    ),
    ("org_fontify_done_headline", BOOLEAN),
    ("org_fontify_todo_headline", BOOLEAN),
    ("org_fontify_quote_and_verse_blocks", BOOLEAN),
    ("org_ellipsis", STRING),
    ("org_bullets_mode", BOOLEAN),
    ("org_bullets_bullet_list", r#"{ "type": "array", "items": { "type": "string" }, "minItems": 1 }"#),
    ("org_bullets_list_bullet", r#"{ "anyOf": [{ "const": false }, { "type": "string" }] }"#),
    (
        "org_bullets_checkboxes",
        r#"{ "anyOf": [{ "const": false }, { "type": "object", "properties": { "todo": { "type": "string" }, "half": { "type": "string" }, "done": { "type": "string" } }, "additionalProperties": false }] }"#,
    ),
    ("org_src_preserve_indentation", BOOLEAN),
    ("org_src_content_indentation", r#"{ "type": "integer", "minimum": 0 }"#),
    (
        "org_agenda_span",
        r#"{ "anyOf": [{ "enum": ["day", "week", "fortnight", "month", "year"] }, { "type": "integer", "minimum": 1 }] }"#,
    ),
    (
        "org_agenda_start_on_weekday",
        r#"{ "anyOf": [{ "type": "integer", "minimum": 0, "maximum": 6 }, { "const": false }] }"#,
    ),
    ("org_deadline_warning_days", INTEGER),
    ("org_scheduled_delay_days", INTEGER),
    ("org_scheduled_past_days", INTEGER),
    ("org_deadline_past_days", INTEGER),
    ("org_modules", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_habit_graph_column", r#"{ "type": "integer", "minimum": 0 }"#),
    ("org_habit_preceding_days", r#"{ "type": "integer", "minimum": 0 }"#),
    ("org_habit_following_days", r#"{ "type": "integer", "minimum": 0 }"#),
    ("org_habit_show_habits", BOOLEAN),
    ("org_habit_show_habits_only_for_today", BOOLEAN),
    ("org_habit_show_all_today", BOOLEAN),
    ("org_habit_today_glyph", STRING),
    ("org_habit_completed_glyph", STRING),
    ("org_habit_show_done_always_green", BOOLEAN),
    ("org_habit_scheduled_past_days", INTEGER),
    ("org_agenda_skip_scheduled_if_done", BOOLEAN),
    ("org_agenda_skip_deadline_if_done", BOOLEAN),
    ("org_agenda_skip_timestamp_if_done", BOOLEAN),
    ("org_agenda_time_leading_zero", BOOLEAN),
    ("org_agenda_timegrid_use_ampm", BOOLEAN),
    ("org_agenda_search_headline_for_time", BOOLEAN),
    ("org_agenda_remove_times_when_in_prefix", r#"{ "enum": [true, false, "beg"] }"#),
    ("org_agenda_default_appointment_duration", INTEGER),
    ("org_agenda_remove_timeranges_from_blocks", BOOLEAN),
    ("org_agenda_diary_sexp_prefix", STRING),
    (
        "org_agenda_show_inherited_tags",
        r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["always"] }, { "type": "array", "items": { "enum": ["agenda", "todo", "search"] } }] }"#,
    ),
    (
        "org_agenda_use_tag_inheritance",
        r#"{ "anyOf": [{ "type": "boolean" }, { "type": "array", "items": { "enum": ["agenda", "todo", "search"] } }] }"#,
    ),
    ("org_agenda_hide_tags_regexp", STRING),
    ("org_agenda_include_deadlines", BOOLEAN),
    ("org_agenda_skip_additional_timestamps_same_entry", BOOLEAN),
    ("org_agenda_fontify_priorities", r#"{ "enum": [true, false, "cookies"] }"#),
    (
        "org_agenda_prefer_last_repeat",
        r#"{ "anyOf": [{ "type": "boolean" }, { "type": "array", "items": { "type": "string" } }] }"#,
    ),
    (
        "org_agenda_skip_deadline_prewarning_if_scheduled",
        r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["pre-scheduled"] }, { "type": "integer" }] }"#,
    ),
    (
        "org_agenda_skip_scheduled_delay_if_deadline",
        r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["post-deadline"] }, { "type": "integer" }] }"#,
    ),
    (
        "org_agenda_skip_scheduled_if_deadline_is_shown",
        r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["not-today", "repeated-after-deadline"] }] }"#,
    ),
    ("org_agenda_skip_scheduled_repeats_after_deadline", BOOLEAN),
    ("org_agenda_skip_comment_trees", BOOLEAN),
    ("org_agenda_skip_archived_trees", BOOLEAN),
    ("org_agenda_todo_list_sublevels", BOOLEAN),
    ("org_agenda_todo_ignore_with_date", BOOLEAN),
    ("org_agenda_todo_ignore_scheduled", TODO_IGNORE),
    ("org_agenda_todo_ignore_deadlines", TODO_IGNORE_DEADLINES),
    ("org_agenda_todo_ignore_timestamp", TODO_IGNORE),
    ("org_agenda_todo_ignore_time_comparison_use_seconds", BOOLEAN),
    ("org_tags_match_list_sublevels", r#"{ "enum": [true, false, "indented"] }"#),
    ("org_agenda_tags_todo_honor_ignore_options", BOOLEAN),
    (
        "org_agenda_sorting_strategy",
        concat!(
            r#"{ "anyOf": [{ "type": "array", "items": "#,
            strategy!(),
            r#" }, { "type": "object", "additionalProperties": false, "properties": { "agenda": { "type": "array", "items": "#,
            strategy!(),
            r#" }, "todo": { "type": "array", "items": "#,
            strategy!(),
            r#" }, "tags": { "type": "array", "items": "#,
            strategy!(),
            r#" }, "search": { "type": "array", "items": "#,
            strategy!(),
            r#" } } }] }"#
        ),
    ),
    ("org_agenda_sort_notime_is_late", BOOLEAN),
    ("org_agenda_sort_noeffort_is_high", BOOLEAN),
    ("org_agenda_max_entries", AGENDA_LIMIT),
    ("org_agenda_max_todos", AGENDA_LIMIT),
    ("org_agenda_max_tags", AGENDA_LIMIT),
    ("org_agenda_max_effort", AGENDA_LIMIT),
    ("org_agenda_show_future_repeats", r#"{ "enum": [true, false, "next"] }"#),
    ("org_agenda_show_all_dates", BOOLEAN),
    ("org_agenda_tags_column", r#"{ "anyOf": [{ "type": "integer" }, { "const": "auto" }] }"#),
    ("org_agenda_use_time_grid", BOOLEAN),
    ("org_agenda_current_time_string", STRING),
    (
        "org_agenda_time_grid",
        r#"{ "type": "array", "minItems": 2, "maxItems": 4, "items": [
            { "type": "array", "items": { "enum": ["daily", "weekly", "today", "require-timed", "remove-match"] } },
            { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 2400 } },
            { "type": "string" },
            { "type": "string" }
        ] }"#,
    ),
    (
        "org_agenda_prefix_format",
        r#"{ "type": "object", "additionalProperties": false, "properties": {
            "agenda": { "type": "string" }, "todo": { "type": "string" }, "tags": { "type": "string" }, "search": { "type": "string" }
        } }"#,
    ),
    ("org_agenda_show_current_time_in_grid", BOOLEAN),
    ("org_agenda_weekend_days", r#"{ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 6 } }"#),
    ("org_agenda_start_day", STRING),
    ("org_agenda_start_with_log_mode", BOOLEAN),
    ("org_agenda_start_with_clockreport_mode", BOOLEAN),
    ("org_agenda_clockreport_parameter_plist", STRING),
    ("org_agenda_clock_report_header", STRING),
    ("org_agenda_start_with_archives_mode", r#"{ "enum": ["trees", "files", false] }"#),
    ("org_agenda_log_mode_items", r#"{ "type": "array", "items": { "enum": ["closed", "clock", "state"] } }"#),
    ("org_agenda_log_mode_add_notes", BOOLEAN),
    ("org_agenda_move_date_from_past_immediately_to_today", BOOLEAN),
    ("org_agenda_persistent_marks", BOOLEAN),
    ("org_agenda_start_with_follow_mode", BOOLEAN),
    ("org_agenda_show_outline_path", r#"{ "enum": [true, false, "title"] }"#),
    ("org_agenda_start_with_entry_text_mode", BOOLEAN),
    ("org_agenda_entry_text_maxlines", r#"{ "type": "integer", "minimum": 1 }"#),
    ("org_agenda_entry_text_leaders", STRING),
    ("org_agenda_entry_text_exclude_regexps", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_agenda_block_separator", r#"{ "anyOf": [{ "type": "string" }, { "const": false }] }"#),
    ("org_agenda_compact_blocks", BOOLEAN),
    ("agenda_hide_empty_blocks", BOOLEAN),
    ("org_super_agenda_mode", BOOLEAN),
    (
        "org_super_agenda_groups",
        r#"{ "type": "array", "items": { "anyOf": [{ "type": "object" }, { "type": "array" }] } }"#,
    ),
    ("org_super_agenda_unmatched_name", STRING),
    ("org_super_agenda_unmatched_order", r#"{ "type": "integer" }"#),
    ("org_super_agenda_header_separator", STRING),
    ("org_super_agenda_header_prefix", STRING),
    ("org_super_agenda_final_group_separator", STRING),
    ("org_super_agenda_date_format", STRING),
    ("org_super_agenda_hide_empty_groups", BOOLEAN),
    ("org_super_agenda_keep_order", BOOLEAN),
    ("org_super_agenda_group_property_name", STRING),
    ("org_super_agenda_properties_inherit", BOOLEAN),
    ("org_roam_enabled", BOOLEAN),
    ("org_roam_directory", STRING),
    ("org_roam_file_extensions", r#"{ "type": "array", "items": { "type": "string" } }"#),
    (
        "org_roam_file_exclude_regexp",
        r#"{ "anyOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }] }"#,
    ),
    ("org_roam_node_display_template", STRING),
    ("org_roam_link_auto_replace", BOOLEAN),
    ("org_roam_completion_everywhere", BOOLEAN),
    ("org_journal_enabled", BOOLEAN),
    ("org_journal_dir", STRING),
    ("org_journal_file_type", r#"{ "enum": ["daily", "weekly", "monthly", "yearly"] }"#),
    ("org_journal_start_on_weekday", r#"{ "type": "integer", "minimum": 1, "maximum": 7 }"#),
    ("org_journal_file_format", STRING),
    ("org_journal_date_format", STRING),
    ("org_journal_date_prefix", STRING),
    ("org_journal_time_format", STRING),
    ("org_journal_time_format_post_midnight", STRING),
    ("org_journal_time_prefix", STRING),
    ("org_journal_hide_entries_p", BOOLEAN),
    ("org_journal_carryover_items", STRING),
    ("org_journal_skip_carryover_drawers", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_journal_carryover_delete_empty_journal", r#"{ "enum": ["never", "always", "ask"] }"#),
    ("org_journal_file_header", STRING),
    ("org_journal_created_property_timestamp_format", STRING),
    ("org_journal_scheduled_string", STRING),
    ("org_journal_enable_encryption", BOOLEAN),
    ("org_journal_enable_agenda_integration", BOOLEAN),
    ("org_journal_search_result_date_format", STRING),
    ("org_journal_search_results_order_by", r#"{ "enum": ["asc", "desc"] }"#),
    ("org_extend_today_until", r#"{ "type": "integer", "minimum": 0, "maximum": 23 }"#),
    (
        "org_roam_mode_sections",
        r#"{ "type": "array", "items": { "enum": ["backlinks", "reflinks", "unlinked-references"] } }"#,
    ),
    ("org_roam_capture_templates", ROAM_TEMPLATES),
    ("org_roam_dailies_capture_templates", ROAM_TEMPLATES),
    ("org_roam_dailies_directory", STRING),
    ("org_use_effective_time", BOOLEAN),
    ("org_use_last_clock_out_time_as_effective_time", BOOLEAN),
    ("org_agenda_search_view_always_boolean", BOOLEAN),
    ("org_agenda_search_view_force_full_words", BOOLEAN),
    ("org_agenda_search_view_max_outline_level", r#"{ "type": "integer", "minimum": 0 }"#),
    ("org_agenda_text_search_extra_files", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("org_agenda_todo_keyword_format", STRING),
    (
        "org_agenda_scheduled_leaders",
        r#"{ "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }"#,
    ),
    (
        "org_agenda_deadline_leaders",
        r#"{ "type": "array", "items": { "type": "string" }, "minItems": 3, "maxItems": 3 }"#,
    ),
    (
        "org_agenda_deadline_faces",
        r#"{ "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "number" }, { "type": "string" }], "minItems": 2, "maxItems": 2 } }"#,
    ),
    (
        "org_agenda_timerange_leaders",
        r#"{ "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 }"#,
    ),
    ("org_agenda_inactive_leader", STRING),
    ("org_agenda_remove_tags", r#"{ "enum": [true, false, "prefix"] }"#),
    ("org_agenda_breadcrumbs_separator", STRING),
    (
        "org_stuck_projects",
        r#"{ "type": "array", "prefixItems": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }, { "type": "array", "items": { "type": "string" } }, { "type": "string" }], "minItems": 4, "maxItems": 4 }"#,
    ),
    ("org_agenda_format_date", STRING),
    (
        "org_agenda_custom_commands",
        r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.AgendaCommand", "required": ["key", "description"], "additionalProperties": false, "properties": {
            "key": { "type": "string", "description": "The keys that pick the command in the agenda menu" },
            "description": { "type": "string", "description": "What the menu shows" },
            "type": { "enum": ["agenda", "alltodo", "todo", "tags", "tags-todo", "search", "stuck"], "description": "The view" },
            "match": { "type": "string", "description": "For todo, keywords (NEXT|WAIT); for tags, a match string (work-home, PRIORITY=\"A\")" },
            "header": { "type": "string", "description": "Replaces the view's header; empty for none" },
            "settings": { "type": "object", "description": "Agenda options for this command: org_agenda_span, org_agenda_start_day…" },
            "blocks": { "type": "array", "description": "Views shown one after the other", "items": { "type": "object", "x-lua-class": "org_lsp.AgendaBlock", "required": ["type"], "additionalProperties": false, "properties": {
                "type": { "enum": ["agenda", "alltodo", "todo", "tags", "tags-todo", "search", "stuck"], "description": "The view" },
                "match": { "type": "string", "description": "For todo, keywords; for tags, a match string" },
                "header": { "type": "string", "description": "Replaces the view's header; empty for none" },
                "settings": { "type": "object", "description": "Agenda options for this block: org_agenda_span, org_agenda_start_day…" }
            } } }
        } } }"#,
    ),
    ("virtual_documents", BOOLEAN),
    ("show_document", BOOLEAN),
    ("org_client", BOOLEAN),
    ("message_prompts", BOOLEAN),
    ("line_highlights", BOOLEAN),
    ("images", BOOLEAN),
    ("view_changed", BOOLEAN),
    ("date_prompt", BOOLEAN),
    ("virtual_indent", BOOLEAN),
    ("command_actions", BOOLEAN),
    ("command_script", STRING),
];

/// `org_refile_targets` and `org_id_completion_targets`.
const REFILE_TARGETS: &str = r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.RefileTarget", "additionalProperties": false, "properties": {
    "files": { "anyOf": [{ "enum": ["current", "agenda", "org-id-files"] }, { "type": "string" }, { "type": "array", "items": { "type": "string" } }],
        "description": "`current` (the entry's own file), `agenda`, `org-id-files` (the files with IDs), a path or a list of paths" },
    "level": { "type": "integer", "minimum": 1, "description": "The headlines of this level" },
    "maxlevel": { "type": "integer", "minimum": 1, "description": "The headlines up to this level" },
    "tag": { "type": "string", "description": "The headlines with this tag" },
    "todo": { "type": "string", "description": "The headlines with this TODO keyword" },
    "regexp": { "type": "string", "description": "The headlines that match this regular expression" },
    "all": { "type": "boolean", "description": "Every headline" }
} } }"#;

/// `org_roam_capture_templates` and `org_roam_dailies_capture_templates`.
const ROAM_TEMPLATES: &str = r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.RoamCaptureTemplate", "required": ["key", "description"], "additionalProperties": false, "properties": {
    "key": { "type": "string", "description": "The keys that pick the template" },
    "description": { "type": "string", "description": "What the menu shows" },
    "type": { "enum": ["entry", "plain", "item", "checkitem", "table-line"], "description": "An Org entry (the default), plain text, a list item, a checkbox item or a table line" },
    "target": { "type": "array", "items": { "type": "string" },
        "description": "`file` and a path; `file+head`, a path and the head of a new file; `file+olp`, a path and an outline path; `file+head+olp`, a path, a head and an outline path; or `node` and a node's title, alias or ID" },
    "template": { "anyOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }], "description": "The template, or its lines" },
    "prepend": { "type": "boolean", "description": "First under the target, not last" },
    "empty_lines": { "type": "integer", "minimum": 0, "description": "Blank lines before and after" },
    "empty_lines_before": { "type": "integer", "minimum": 0, "description": "Blank lines before" },
    "empty_lines_after": { "type": "integer", "minimum": 0, "description": "Blank lines after" },
    "unnarrowed": { "type": "boolean", "description": "Show the whole file in the capture buffer" }
} } }"#;

const BOOLEAN: &str = r#"{ "type": "boolean" }"#;
/// A limit of `org-agenda-max-…`: one for all views, or one for each kind.
const TODO_IGNORE: &str =
    r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["future", "past", "all"] }, { "type": "integer" }] }"#;
const TODO_IGNORE_DEADLINES: &str = r#"{ "anyOf": [{ "type": "boolean" }, { "enum": ["near", "far", "all", "future", "past"] }, { "type": "integer" }] }"#;
const AGENDA_LIMIT: &str = r#"{ "anyOf": [{ "type": "integer" }, { "type": "object", "additionalProperties": false, "properties": {
    "agenda": { "type": "integer" }, "todo": { "type": "integer" }, "tags": { "type": "integer" }, "search": { "type": "integer" }
} }] }"#;
/// `org-capture-templates-contexts`, `org-agenda-custom-commands-contexts`
const CONTEXTS: &str = r#"{ "type": "array", "items": { "type": "object", "required": ["key", "contexts"], "additionalProperties": false, "properties": {
            "key": { "type": "string", "description": "The keys the context is about" },
            "use": { "type": "string", "description": "The keys whose template or command these keys stand for where the context holds" },
            "contexts": { "type": "array", "items": { "type": "object", "minProperties": 1, "maxProperties": 1, "additionalProperties": false, "properties": {
                "in_file": { "type": "string" }, "in_mode": { "type": "string" }, "in_buffer": { "type": "string" },
                "not_in_file": { "type": "string" }, "not_in_mode": { "type": "string" }, "not_in_buffer": { "type": "string" } } },
                "description": "Rules, each a regexp on the file, the mode or the buffer's name; the context holds where one of them does" } } } }"#;
const INTEGER: &str = r#"{ "type": "integer" }"#;
const STRING: &str = r#"{ "type": "string" }"#;
const PRIORITY: &str = r#"{ "anyOf": [{ "type": "string", "pattern": "^[A-Za-z]$" }, { "type": "integer", "minimum": 0, "maximum": 64 }] }"#;

/// The schema of the option `name`.
pub fn schema(name: &str) -> Option<Value> {
    let (_, schema) = SCHEMAS.iter().find(|(known, _)| *known == name)?;
    serde_json::from_str(schema).ok()
}

/// The table as JSON: a list of `{ "name", "type", "default"?, "unset"?,
/// "org"?, "doc", "schema", "integration"?, "editorOnly"? }`.
pub fn manifest() -> Value {
    Value::Array(
        SETTINGS
            .iter()
            .map(|setting| {
                let mut entry = json!({ "name": setting.name, "type": setting.kind, "doc": setting.doc });
                if let Some(schema) = schema(setting.name) {
                    entry["schema"] = schema;
                }
                match setting.default {
                    Some(default) => entry["default"] = serde_json::from_str(default).expect("a default is JSON"),
                    None => entry["unset"] = json!(setting.unset),
                }
                if !setting.org.is_empty() {
                    entry["org"] = json!(setting.org);
                }
                if setting.integration {
                    entry["integration"] = json!(true);
                }
                if setting.editor_only {
                    entry["editorOnly"] = json!(true);
                }
                entry
            })
            .collect(),
    )
}

/// The defaults as the options object a client would pass.
pub fn defaults() -> Value {
    let mut options = json!({});
    for setting in SETTINGS {
        let Some(default) = setting.default else { continue };
        let mut target = &mut options;
        let mut parts = setting.name.split('.').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                target[part] = serde_json::from_str(default).expect("a default is JSON");
            } else {
                if target.get(part).is_none() {
                    target[part] = json!({});
                }
                target = &mut target[part];
            }
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Options;

    /// Everything the server makes of its options.
    fn resolved(options: &Options) -> String {
        format!(
            "{:?}",
            (
                &options.agenda_files,
                options.todo_keywords(),
                options.agenda_config(),
                options.edit_settings(),
                options.read_date_settings(),
                options.refile_targets(),
                options.refile_outline_path(),
                (options.refile_settings(), options.archive_settings(), options.archive_default_command()),
                options.capture_templates(),
                options.org_directory(),
                options.default_notes_file(),
                (options.separator_lines(), options.roam()),
            )
        )
    }

    #[test]
    fn the_defaults_written_here_are_the_ones_that_apply() {
        let none: Options = serde_json::from_str("{}").unwrap();
        let written: Options = serde_json::from_value(defaults()).unwrap();
        assert_eq!(resolved(&written), resolved(&none));
    }

    #[test]
    fn the_defaults_of_org_roam_are_the_ones_that_apply() {
        let on = |mut value: Value| {
            value["org_roam_enabled"] = json!(true);
            let options: Options = serde_json::from_value(value).unwrap();
            format!("{:?}", options.roam())
        };
        assert_eq!(on(defaults()), on(json!({})));
    }

    /// The names of every option `Options` reads.
    fn names(value: &Value, out: &mut Vec<String>) {
        out.extend(value.as_object().unwrap().keys().cloned());
    }

    #[test]
    fn every_option_is_listed_and_every_listed_one_exists() {
        let mut read = Vec::new();
        names(&serde_json::to_value(Options::default()).unwrap(), &mut read);
        // A fixed clock, for tests only.
        read.retain(|name| name != "clock");
        read.sort();
        let mut listed: Vec<String> = SETTINGS.iter().map(|setting| setting.name.to_string()).collect();
        listed.sort();
        assert_eq!(listed, read);
    }

    /// Whether `value` is what `schema` allows, for the parts of JSON
    /// Schema the table uses.
    fn allows(schema: &Value, value: &Value) -> bool {
        if let Some(choices) = schema.get("anyOf").and_then(Value::as_array) {
            return choices.iter().any(|choice| allows(choice, value));
        }
        if let Some(constant) = schema.get("const") {
            return constant == value;
        }
        if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
            return choices.contains(value);
        }
        let of_type = |name: &str| match name {
            "boolean" => value.is_boolean(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "array" => value.as_array().is_some_and(|items| match schema.get("items") {
                // A list of schemas: one for each item in turn.
                Some(Value::Array(schemas)) => {
                    items.len() <= schemas.len() && items.iter().zip(schemas).all(|(item, schema)| allows(schema, item))
                }
                Some(each) => items.iter().all(|item| allows(each, item)),
                None => true,
            }),
            "object" => value.as_object().is_some_and(|fields| {
                fields.iter().all(|(key, field)| {
                    schema.pointer(&format!("/properties/{key}")).is_some_and(|property| allows(property, field))
                })
            }),
            _ => false,
        };
        match &schema["type"] {
            Value::String(name) => of_type(name),
            Value::Array(names) => names.iter().filter_map(Value::as_str).any(of_type),
            _ => false,
        }
    }

    #[test]
    fn every_option_has_a_schema_that_allows_its_default() {
        for setting in SETTINGS {
            let schema = schema(setting.name).unwrap_or_else(|| panic!("no schema for `{}`", setting.name));
            if let Some(default) = setting.default {
                let default: Value = serde_json::from_str(default).unwrap();
                assert!(allows(&schema, &default), "the default of `{}` is not what its schema allows", setting.name);
            }
        }
        assert_eq!(SCHEMAS.len(), SETTINGS.len(), "a schema for an option that is not there");
        // And an example of each kind of value the schemas catch.
        assert!(!allows(&schema("org_log_done").unwrap(), &json!("yes")));
        assert!(!allows(&schema("org_capture_templates").unwrap(), &json!([{ "key": "t", "taget": [] }])));
        assert!(allows(&schema("org_agenda_span").unwrap(), &json!(3)));
    }

    #[test]
    fn names_are_unique_and_documented() {
        let mut names: Vec<&str> = SETTINGS.iter().map(|setting| setting.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SETTINGS.len());
        for setting in SETTINGS {
            assert!(!setting.doc.is_empty() && (setting.default.is_some() || !setting.unset.is_empty()));
        }
    }
}
