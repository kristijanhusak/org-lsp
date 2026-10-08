# Configuring Helix

Helix already knows the `org` language; it only needs to be told to use
the server. Add this to `~/.config/helix/languages.toml` (or a project's
`.helix/languages.toml`), with `org-lsp` on `PATH`:

```toml
[language-server.org-lsp]
command = "org-lsp"

[[language]]
name = "org"
language-servers = ["org-lsp"]
```

`editors/helix/languages.toml` in this repository has the same.

Files with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`)
show indented by level, without being changed, through inlay hints of
blanks before their lines, when Helix shows inlay hints:

```toml
[editor.lsp]
display-inlay-hints = true
```

## The server's options

The server's options go in a `config` table under the server, which Helix
passes to it as they are. Leave out an option and
the server takes it from the [shared configuration file](../configuration.md),
if you have one, or else uses its default, which is Org's. The editor's
options go on top of the file's: an object such as
`org_agenda_prefix_format` is merged key by key, and anything else
replaces the file's. `config_file`
says which file to read, or `false` for none.

These are all the options with their defaults; you would never write all of
them out:

<!-- The block below is written by scripts/generate-docs.mjs from
     `org-lsp settings`. Edit server/org-lsp/src/settings.rs, not the block. -->

<!-- BEGIN generated: options -->

```toml
[language-server.org-lsp.config]
# The shared configuration file, whose options this editor's go on top of:
# `false` for none, or the path of another file (JSON if it ends in `.json`,
# else TOML). Unset, it is `$ORG_LSP_CONFIG`, else `config.toml` or
# `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`;
# `%APPDATA%\org-lsp` on Windows).
# config_file = …  (unset: the shared file, `~/.config/org-lsp/config.toml` or `config.json`)

# Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built
# from. `~/` is the home directory, and a directory stands for the Org files
# directly in it. When empty, the agenda is built from the Org files under the
# workspace folders (hidden directories, `node_modules` and `target` left
# out), or else from the Org files that are open. (org-agenda-files)
org_agenda_files = []

# The TODO sequence, written like Org's `#+TODO:` line. `|` separates the
# states that count as done; without it the last keyword is the only done
# state. `TODO(t)` gives a keyword a key for fast selection, and `WAIT(w@/!)`
# makes it log a note (`@`) or the time (`!`) on entering and leaving it. A
# file's own `#+TODO:` lines take precedence. (org-todo-keywords)
org_todo_keywords = ["TODO", "|", "DONE"]

# Write `CLOSED:` with the time when an entry is done, and with `note` ask for
# a note too. A file's `#+STARTUP:` words `logdone`, `lognotedone` and
# `nologdone` change it there; an entry's `LOGGING` property (inherited) says
# what its state changes log instead: those words, `logrepeat` and its
# relatives, and keywords with what they log (`WAIT(@/!)`), or `nil` for
# nothing. (org-log-done)
org_log_done = false

# Record when a repeating entry is done and moves to its next date.
# `#+STARTUP:` `logrepeat`, `lognoterepeat` and `nologrepeat` change it in a
# file, and the `LOGGING` property in an entry. (org-log-repeat)
org_log_repeat = "time"

# An entry cannot be done while a child is not, or, under a parent with the
# `ORDERED` property, while an earlier sibling is not: the change is refused
# with what blocks it. (org-enforce-todo-dependencies)
org_enforce_todo_dependencies = false

# An entry cannot be done while it has an unchecked box.
# (org-enforce-todo-checkbox-dependencies)
org_enforce_todo_checkbox_dependencies = false

# Record when an entry is refiled (`Refiled on [2026-05-18 Mon 10:30]`), with
# `note` asking for a note too. `#+STARTUP:` `logrefile`, `lognoterefile` and
# `nologrefile` change it in the file refiled to. (org-log-refile)
org_log_refile = false

# Record when a scheduled date changes or is removed (`Rescheduled from
# "[2026-05-18 Mon]" on …`), with `note` asking for a note too. `#+STARTUP:`
# `logreschedule`, `lognotereschedule` and `nologreschedule` change it in a
# file. (org-log-reschedule)
org_log_reschedule = false

# Record when a deadline changes or is removed (`New deadline from …`), with
# `note` asking for a note too. `#+STARTUP:` `logredeadline`,
# `lognoteredeadline` and `nologredeadline` change it in a file.
# (org-log-redeadline)
org_log_redeadline = false

# Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name
# of a drawer. `#+STARTUP:` `logdrawer` and `nologdrawer` change it in a file;
# an entry's `LOG_INTO_DRAWER` property (inherited) says which drawer for it
# (`t` for `LOGBOOK`, `nil` for none). (org-log-into-drawer)
org_log_into_drawer = false

# Where tags are aligned after a change to a headline. Negative: they end at
# that column. (org-tags-column)
org_tags_column = -77

# Which tags entries inherit from their parents and from `#+FILETAGS:`: `true`
# for all, `false` for none, a list of tags, or a regexp (Emacs syntax) tags
# must match. Inherited tags count in tag searches and show in the agenda.
# (org-use-tag-inheritance)
org_use_tag_inheritance = true

# Tags never inherited, when `org_use_tag_inheritance` is `true` or a regexp.
# (org-tags-exclude-from-inheritance)
org_tags_exclude_from_inheritance = []

# Which properties an entry without them takes from its parents, the file's
# property drawer and `#+PROPERTY:` lines, in property searches
# (`OWNER="alice"`): `true` for all, `false` for none, a list of names, or a
# regexp (Emacs syntax). `CATEGORY` and `ARCHIVE` are always inherited.
# (org-use-property-inheritance)
org_use_property_inheritance = false

# Properties every entry inherits, after those of `#+PROPERTY:` lines: chiefly
# the allowed values of a property, which setting it offers and `S-<right>`
# and `S-<left>` go through on its line (`{ "Effort_ALL": "0:10 0:30 1:00
# 2:00" }`; `:ETC` allows others too). (org-global-properties)
org_global_properties = {  }

# Where `CLOCK:` lines go: `true` for the drawer of `org_log_into_drawer`
# (`LOGBOOK` when that is not a name), the name of a drawer, or `false` for
# the entry itself. (org-clock-into-drawer)
org_clock_into_drawer = true

# Clocking out after less than a minute removes the `CLOCK:` line.
# (org-clock-out-remove-zero-time-clocks)
org_clock_out_remove_zero_time_clocks = false

# Marking the clocked entry done stops the clock. (org-clock-out-when-done)
org_clock_out_when_done = true

# The span whose clocks `org.clock.display` (`C-c C-x C-d`) adds up.
# (org-clock-display-default-range)
org_clock_display_default_range = "thisyear"

# After how many minutes without a message from the editor a clock that runs
# asks what to do with the idle time: keep it, subtract it (and clock out), or
# cancel the clock. (org-clock-idle-time)
# org_clock_idle_time = …  (unset: none)

# How many entries clocked into the clock history keeps, for `org.clock.in`
# with a count and `org.clock.in.last`. (org-clock-history-length)
org_clock_history_length = 5

# The parameters a new clock table gets on its `#+BEGIN:` line from
# `org.clock.report` (`C-c C-x C-r`), after `:scope`, as Emacs Lisp reads them
# (`:maxlevel 3 :link t`). (org-clock-clocktable-default-properties)
org_clock_clocktable_default_properties = ":maxlevel 2"

# How a clock table writes its total time and the words before it; `%s` is the
# text. (org-clock-total-time-cell-format)
org_clock_total_time_cell_format = "*%s*"

# How a clock table over several files writes the words before a file's time;
# `%s` is the text. (org-clock-file-time-cell-format)
org_clock_file_time_cell_format = "*%s*"

# The TODO state an entry is put in when the clock starts on it (`NEXT`),
# unless it has it already. (org-clock-in-switch-to-state)
# org_clock_in_switch_to_state = …  (unset: none)

# The TODO state an entry is put in when the clock stops on it.
# (org-clock-out-switch-to-state)
# org_clock_out_switch_to_state = …  (unset: none)

# Org's linter checks the documents and reports what it finds as diagnostics:
# misplaced headings and planning lines, links to nowhere, duplicate names,
# footnotes without definitions… A checker Org trusts little reports
# information, the others warnings. (org-lint)
org_lint = true

# The `org-lint` checkers to leave out, by name, as a diagnostic's code shows
# it (`"misplaced-heading"`). (org-lint)
org_lint_ignore = []

# Whether `org.link.store` (`C-c l`) links to an entry by its ID: `true` makes
# one when the entry has none, `"create-if-interactive"` only from the command
# (not for capture's `%a`), `"create-if-interactive-and-no-custom-id"`
# likewise unless the entry has a `CUSTOM_ID`, `"use-existing"` only when it
# has one already. Off, the link searches for the headline.
# (org-id-link-to-org-use-id)
org_id_link_to_org_use_id = false

# Links to a place in a file search for it (`file:notes.org::*Heading`); `C-u`
# turns it around for one link. (org-link-context-for-files)
org_link_context_for_files = true

# ID links to a place below the entry's headline search for it as well
# (`id:…::name`). (org-id-link-use-context)
org_id_link_use_context = true

# An ID link may use the ID of a parent entry, with a search for the place.
# (org-id-link-consider-parent-id)
org_id_link_consider_parent_id = false

# How links to files write the path: `"adaptive"` relative below the file's
# directory and absolute elsewhere, `"absolute"` (with `~`), `"noabbrev"`
# (without), `"relative"`. (org-link-file-path-type)
org_link_file_path_type = "adaptive"

# Stored links stay stored once inserted.
# (org-link-keep-stored-after-insertion)
org_link_keep_stored_after_insertion = false

# Completing a tag offers the tags of every agenda file, after those of the
# file. Off, only the file's own. Setting tags offers them too.
# (org-complete-tags-always-offer-all-agenda-tags)
org_complete_tags_always_offer_all_agenda_tags = false

# Bibliography files for the citations of every document (`[cite:@key]`),
# after those its `#+BIBLIOGRAPHY:` lines name: BibTeX (`.bib`, `.bibtex`) or
# CSL-JSON (`.json`). Their entries are completed after `@`, shown over a key,
# and where following a key goes; a key none has is marked.
# (org-cite-global-bibliography)
org_cite_global_bibliography = []

# Link abbreviations: `[[gh:org-lsp]]` with `{ "gh": "https://github.com/%s"
# }` follows `https://github.com/org-lsp`. `%s` stands for what follows the
# colon, `%h` for it URL-encoded; without either it goes at the end. A file's
# `#+LINK:` lines come first. A function (`%(…)`) is not supported.
# (org-link-abbrev-alist)
org_link_abbrev_alist = {  }

# The tags offered when setting tags in a file without a `#+TAGS:` line,
# written like one: `"work(w) home(h) { @office(o) @home(m) } [ GTD : Control
# Persp ]"` gives `work` the key `w`, makes `@office` and `@home` exclusive,
# and groups `Control` and `Persp` under `GTD`; `\n` starts a new line. A list
# of these words does as well. Unset, the tags used in the file are offered.
# (org-tag-alist)
org_tag_alist = ""

# Tags offered in every file, before those of `#+TAGS:` or `org_tag_alist`,
# written the same way. (org-tag-persistent-alist)
org_tag_persistent_alist = ""

# A group tag (`[ GTD : Control Persp ]`, `{ Context : @home @office }` in
# `#+TAGS:` or `org_tag_alist`) in the match string of a tags view stands for
# itself and the tags of its group. (org-group-tags)
org_group_tags = true

# Pick tags by key, each key turning its tag on or off until RET: `auto` when
# some tag has a key, `true` always (keys are given to the tags without one),
# `false` never: the tags are typed instead. (org-use-fast-tag-selection)
org_use_fast_tag_selection = "auto"

# In fast tag selection, the first key that changes the tags also ends it.
# (org-fast-tag-selection-single-key)
org_fast_tag_selection_single_key = false

# Sort the tags of a headline when they are set: alphabetically, or the other
# way. (org-tags-sort-function)
# org_tags_sort_function = …  (unset: the order they were picked in)

# Align the tags of a headline to `org_tags_column` when they are set.
# (org-auto-align-tags)
org_auto_align_tags = true

# The state a repeating entry returns to when it is done: a keyword, or `true`
# for the state it had. (org-todo-repeat-to-state)
# org_todo_repeat_to_state = …  (unset: the first keyword of the sequence)

# The highest priority. (org-priority-highest)
org_priority_highest = "A"

# The lowest priority. (org-priority-lowest)
org_priority_lowest = "C"

# The priority of an entry without a cookie. A file's `#+PRIORITIES:` line
# takes precedence over the three priority options. (org-priority-default)
org_priority_default = "B"

# Raising or lowering the priority of an entry that has none starts at the
# default. (org-priority-start-cycle-with-default)
org_priority_start_cycle_with_default = true

# The step in which the minutes of a timestamp change (the second number of
# Org's option). (org-time-stamp-rounding-minutes)
org_time_stamp_rounding_minutes = 5

# A date typed without a year or month is the next one to come. With `time`, a
# time that has passed today is tomorrow's. (org-read-date-prefer-future)
org_read_date_prefer_future = true

# A typed year is kept within 1970 to 2037.
# (org-read-date-force-compatible-dates)
org_read_date_force_compatible_dates = true

# Where entries can be refiled to. Each entry has `files`: `current` (the
# entry's own file), `agenda`, a path or a list of paths; and one of `level`,
# `maxlevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's syntax) or
# `all` (true), which says which headlines there are targets.
# (org-refile-targets)
org_refile_targets = [{ files = "current", level = 1 }]

# Name targets by their outline path (`Projects/Alpha/`). The three names
# start the path with the file, and offer the file itself, for its top level.
# (org-refile-use-outline-path)
org_refile_use_outline_path = false

# A refiled entry goes first under its target, not last.
# (org-reverse-note-order)
org_reverse_note_order = false

# Where archived subtrees go: a file, `::`, and a headline under which they go
# (they go to the end of the file without one). `%s` is the name of the file
# the subtree is in; an empty file part is that file. `datetree/` as the
# headline files them under year, month and day, by when they were closed. An
# `ARCHIVE` property of the entry or a parent, or a `#+ARCHIVE:` line, says
# otherwise. (org-archive-location)
org_archive_location = "%s_archive::"

# An archived subtree goes first under its archive headline, not last.
# (org-archive-reversed-order)
org_archive_reversed_order = false

# Mark archived entries done: `true` for the first done keyword, or the
# keyword to use. (org-archive-mark-done)
org_archive_mark_done = false

# Give an archived subtree the tags it inherited: always, never, or when it
# stays in its file. (org-archive-subtree-add-inherited-tags)
org_archive_subtree_add_inherited_tags = "infile"

# What an archived entry keeps as `ARCHIVE_…` properties: `time`, `file`,
# `olpath` (its outline path), `category`, `todo`, `itags` (inherited tags),
# `ltags` (its own tags) and `olid` (its parent's ID).
# (org-archive-save-context-info)
org_archive_save_context_info = ["time", "file", "olpath", "category", "todo", "itags"]

# The headline of the sibling `org.archive.sibling` archives under.
# (org-archive-sibling-heading)
org_archive_sibling_heading = "Archive"

# What a new archive file starts with; `%s` is the path of the file the
# subtree came from. (org-archive-file-header-format)
org_archive_file_header_format = "\nArchived entries from file %s\n\n"

# What `org.archive.default` does: archive the subtree, archive it under its
# sibling, or tag it. (org-archive-default-command)
org_archive_default_command = "subtree"

# Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the
# default, `plain`, `item`, `checkitem` or `table-line`), a `target` and a
# `template` (text, or a list of lines), and may have `prepend`,
# `empty_lines`, `empty_lines_before`, `empty_lines_after`,
# `immediate_finish`, `tree_type`, `time_prompt`, `table_line_pos`,
# `clock_in`, `clock_keep`, `clock_resume` and `jump_to_captured`. The target
# is a list: `file` and a path; `file+headline`, a path and a headline;
# `file+olp`, a path and the headlines of an outline path; `file+regexp`, a
# path and a regular expression; `file+olp+datetree`, a path and the headlines
# of an outline path, maybe none, for a date tree under them; `id` and an ID;
# or `clock`, the entry clocked now. An empty path is
# `org_default_notes_file`, and without a target the entry goes at the end of
# `org_default_notes_file`. A template with only a key and a description
# groups the templates whose keys start with its key. (org-capture-templates)
org_capture_templates = [{ key = "t", description = "Task", target = ["file+headline", "", "Tasks"], template = "* TODO %?\n  %u\n  %a" }]

# Where the relative paths of capture targets are. (org-directory)
org_directory = "~/org"

# The target of a capture template whose file is empty.
# (org-default-notes-file)
org_default_notes_file = "~/.notes"

# Show a link as its description (or its target, when it has none) and hide
# the rest: the brackets, and the target of a link with a description. The
# text is marked `hidden` in the highlighting, and editors that can conceal
# text conceal it. (org-link-descriptive)
org_link_descriptive = true

# Hide the markers around `*bold*`, `/italic/`, `_underline_`,
# `+strike-through+`, `=verbatim=` and `~code~`, as for links.
# (org-hide-emphasis-markers)
org_hide_emphasis_markers = false

# RET on the first line of a list item makes the next item, numbered and with
# a checkbox as M-RET (`org-meta-return`) makes it, the items after it
# renumbered, as nvim-orgmode's `org_return_uses_meta_return` does. Off, RET
# is Org's: a new line in the item. `org.return.item` (`<S-CR>`, Shift+Enter,
# with the vim keys) always does it.
return_uses_meta_return = false

# The deepest headings `org.goto` (`C-c C-j`) offers. (org-goto-max-level)
org_goto_max_level = 5

# How many months the calendar of the date prompt shows: 3 side by side, as
# Emacs's calendar shows them, the one with the selected day in the middle; or
# 1, that month alone, for a narrow window.
calendar_months = 3

# The Emacs program, for what only Org itself does: export
# (`org.export.dispatch`). The server starts it (`emacs --batch -Q`) when that
# is first asked for and keeps it running, so that Org loads once. Without
# Emacs, everything else works as before.
emacs = "emacs"

# A file of Emacs Lisp that Emacs loads before Org, for the variables export
# reads (`org-export-backends`, `org-html-…`, `org-latex-…`,
# `org-publish-project-alist`): a file of its own, or your init file. `~/` is
# the home directory.
# emacs_config_file = …  (unset: none)

# Emacs Lisp that Emacs evaluates after `emacs_config_file` and before loading
# Org, for a few settings without a file: `(setq org-export-with-toc nil)`, or
# `(add-to-list 'load-path "~/src/org-mode/lisp")` for another Org. A
# yes-or-no question Emacs asks while it works (`org-confirm-babel-evaluate`)
# is asked in the editor.
# emacs_config = …  (unset: none)

# Save the files a command changes other than the one it was run in: an
# entry's file changed from the agenda, the target of a capture or of a
# refile. The editor saves them once it has made the change. Off, they are
# left with unsaved changes, as Emacs leaves them.
auto_save = true

# With at least this many blank lines after a subtree, the last one stays
# outside its fold. Negative: all of them stay outside. `0`: none does.
# (org-cycle-separator-lines)
org_cycle_separator_lines = 2

# Indent the lines of an entry past its headline's stars: `true` for all of
# them, `"headline-data"` for the planning line, drawers and clock lines right
# below the headline. Applies to new lines, to re-indenting (range formatting)
# and to what commands write. (org-adapt-indentation)
org_adapt_indentation = false

# Show files indented by level without changing them (`org-indent-mode`): the
# text under a headline lines up with its title, and leading stars are hidden.
# `#+STARTUP: indent` and `noindent` decide for a file. It turns
# `org_adapt_indentation` off in such files, unless that is `"headline-data"`.
# Editors that can show it: Neovim, and VS Code (not on wrapped lines).
# (org-startup-indented)
org_startup_indented = false

# Hide all the stars of a headline but the last; they keep their place.
# `#+STARTUP: hidestars` and `showstars` decide for a file.
# (org-hide-leading-stars)
org_hide_leading_stars = false

# Leave the code of source blocks as it is indented when re-indenting.
# (org-src-preserve-indentation)
org_src_preserve_indentation = false

# How far code is indented past its source block when re-indenting.
# (org-edit-src-content-indentation)
org_edit_src_content_indentation = 2

# How much time the agenda shows. (org-agenda-span)
org_agenda_span = "week"

# The day a week or fortnight starts on; `false` starts it today.
# (org-agenda-start-on-weekday)
org_agenda_start_on_weekday = 1

# How many days before a deadline it shows up. (org-deadline-warning-days)
org_deadline_warning_days = 14

# How many days after its date a scheduled entry shows up.
# (org-scheduled-delay-days)
org_scheduled_delay_days = 0

# How many days a scheduled entry that is not done keeps showing up.
# (org-scheduled-past-days)
org_scheduled_past_days = 10000

# How many days a missed deadline keeps showing up. (org-deadline-past-days)
org_deadline_past_days = 10000

# Leave out scheduled entries that are done.
# (org-agenda-skip-scheduled-if-done)
org_agenda_skip_scheduled_if_done = false

# Leave out deadlines of entries that are done.
# (org-agenda-skip-deadline-if-done)
org_agenda_skip_deadline_if_done = false

# Leave out timestamps of entries that are done.
# (org-agenda-skip-timestamp-if-done)
org_agenda_skip_timestamp_if_done = false

# Show the future dates of repeating entries: all of them, none, or only the
# next. (org-agenda-show-future-repeats)
org_agenda_show_future_repeats = true

# Show days without entries. (org-agenda-show-all-dates)
org_agenda_show_all_dates = true

# Where tags are aligned in the agenda: a column (negative: where they end),
# or `auto` for the right edge of the window. With `auto`, the editors that
# tell the server how wide the agenda's window is (Neovim, Vim, Sublime Text)
# have the tags there, and the agenda follows the window as it is resized; the
# others (VS Code, Zed) have them end at column 80. (org-agenda-tags-column)
org_agenda_tags_column = "auto"

# Draw the time grid. (org-agenda-use-time-grid)
org_agenda_use_time_grid = true

# The time grid, as Org writes it: the days that have it (`daily`: a one-day
# agenda; `weekly`: every day of a longer one; `today`: today;
# `require-timed`: only days with an entry that has a time; `remove-match`: no
# grid line at a time an entry already has), the times of its lines (`800` for
# 8:00), what follows a time that has no end time, and the text of a grid
# line. (org-agenda-time-grid)
org_agenda_time_grid = [["daily", "today", "require-timed"], [800, 1000, 1200, 1400, 1600, 1800, 2000], "......", "----------------"]

# The text of the line for the current time. (org-agenda-current-time-string)
org_agenda_current_time_string = "now - - - - - - - - - - - - - - - - - - - - - - - - -"

# Show the current time in the grid. (org-agenda-show-current-time-in-grid)
org_agenda_show_current_time_in_grid = true

# The days drawn as the weekend (0 is Sunday). (org-agenda-weekend-days)
org_agenda_weekend_days = [6, 0]

# Start the agenda in log mode, which shows what was closed and clocked each
# day (`l` toggles it). (org-agenda-start-with-log-mode)
org_agenda_start_with_log_mode = false

# Start the agenda with the clock report under it (`R` toggles it).
# (org-agenda-start-with-clockreport-mode)
org_agenda_start_with_clockreport_mode = false

# Start the agenda with entries in archived trees (`trees`; `v a` toggles it),
# or those and the files the agenda files archive to (`files`; `v A`).
# (org-agenda-start-with-archives-mode)
org_agenda_start_with_archives_mode = false

# What log mode shows: `closed` entries, `clock` lines and `state` changes.
# (org-agenda-log-mode-items)
org_agenda_log_mode_items = ["closed", "clock"]

# Keep the marks of a bulk action (`B`) after it (`p` in its menu toggles this
# for one action). (org-agenda-persistent-marks)
org_agenda_persistent_marks = false

# Start the agenda in follow mode, which shows the entry of the cursor's line
# in another window as the cursor moves (`F` toggles it).
# (org-agenda-start-with-follow-mode)
org_agenda_start_with_follow_mode = false

# Start the agenda with some text of each entry under its line (`E` toggles
# it). (org-agenda-start-with-entry-text-mode)
org_agenda_start_with_entry_text_mode = false

# The most lines of an entry's text `E` shows.
# (org-agenda-entry-text-maxlines)
org_agenda_entry_text_maxlines = 5

# What each line of an entry's text starts with in the agenda.
# (org-agenda-entry-text-leaders)
org_agenda_entry_text_leaders = "    > "

# Emacs regexps whose matches are left out of the entry text `E` shows.
# (org-agenda-entry-text-exclude-regexps)
org_agenda_entry_text_exclude_regexps = []

# What goes between the blocks of a block agenda: one character repeated
# across 79 columns, a longer text as it is, or `false` for nothing. In a
# block's `settings`, the separator above that block.
# (org-agenda-block-separator)
org_agenda_block_separator = "="

# Leave out the separators between blocks and the header over agenda blocks.
# (org-agenda-compact-blocks)
org_agenda_compact_blocks = false

# Leave out the blocks of a block agenda that show no entries. Not in Org.
agenda_hide_empty_blocks = false

# Read every search string of the search view (`s`) as words that must
# (`+word`, `word`) or must not (`-word`) occur, `{regexp}`s and `"quoted
# phrases"`. Off, it is a phrase whose spaces match any whitespace, unless its
# first word starts with `+`, `-` or `{`.
# (org-agenda-search-view-always-boolean)
org_agenda_search_view_always_boolean = false

# The words of a Boolean search match whole words only, as a search string
# starting with `:` asks. (org-agenda-search-view-force-full-words)
org_agenda_search_view_force_full_words = false

# The search view shows headlines down to this level, a match below one
# counting for the headline above: 0 for every level.
# (org-agenda-search-view-max-outline-level)
org_agenda_search_view_max_outline_level = 0

# Files the search view (`s`) searches besides the agenda files, after them,
# as `org_agenda_files` names them. `agenda-archives` first adds the files the
# agenda files archive to. A file listed twice is searched at its last place.
# (org-agenda-text-search-extra-files)
org_agenda_text_search_extra_files = []

# What comes before the headline on a line of the agenda (`agenda`), the TODO
# list (`todo`), tags searches (`tags`) and text searches (`search`): `%c` the
# category, `%t` the time, `%s` the leader (`Scheduled: `), `%e` the effort,
# `%l` a blank per level, `%T` the last tag, `%b` the headlines above
# (breadcrumbs), `%i` the icon (none). A width pads (`%-12c` on the right,
# `%12c` on the left), `.N` cuts, `%?` leaves out an empty field, and a
# character before the letter follows a value that is not empty (`%-12:c`).
# `%(…)` (Emacs Lisp) is left empty. (org-agenda-prefix-format)
org_agenda_prefix_format = { agenda = " %i %-12:c%?-12t% s", search = " %i %-12:c", tags = " %i %-12:c", todo = " %i %-12:c" }

# How a TODO keyword is written in the agenda: `%-4s` pads it to four columns,
# so that the titles line up; an empty format leaves keywords out.
# (org-agenda-todo-keyword-format)
org_agenda_todo_keyword_format = "%-1s"

# The leader of a scheduled entry on its day, and on today when its day has
# passed (`%d`: the days since). (org-agenda-scheduled-leaders)
org_agenda_scheduled_leaders = ["Scheduled: ", "Sched.%2dx: "]

# The leader of a deadline on its day, on today before it (`%d`: the days
# left), and on today after it (`%d`: the days since).
# (org-agenda-deadline-leaders)
org_agenda_deadline_leaders = ["Deadline:  ", "In %3d d.: ", "%2d d. ago: "]

# The leader of a time range within a day, and of a range over days (`%d`:
# which day of it, `%d`: how many days). (org-agenda-timerange-leaders)
org_agenda_timerange_leaders = ["", "(%d/%d): "]

# Leave the tags out of the agenda's lines; `prefix` only when the prefix
# shows a tag (`%T`). (org-agenda-remove-tags)
org_agenda_remove_tags = false

# What the stuck projects view (`#`) lists: the projects the match string
# finds whose subtree has no entry with one of the TODO keywords (`*` for any
# not done) or tags (`*` for any), and no match of the regexp.
# (org-stuck-projects)
org_stuck_projects = ["+LEVEL=2/-DONE", ["TODO", "NEXT", "NEXTACTION"], [], ""]

# What goes between the headlines of `%b`. (org-agenda-breadcrumbs-separator)
org_agenda_breadcrumbs_separator = "->"

# How the agenda writes a day, as `format-time-string` does: `%A %d %B %Y`,
# `%a %Y-%m-%d`. (org-agenda-format-date)
# org_agenda_format_date = …  (unset: as `Wednesday   7 October 2026`, the week's number added on Mondays)

# The day the agenda starts from, as the date prompt reads it: `-1d`, `+2d`,
# `2026-10-01`. (org-agenda-start-day)
# org_agenda_start_day = …  (unset: today)

# Commands of the agenda menu. Each has a `key` and a `description`, and
# either a `type` (`agenda`, `alltodo`, `todo`, `tags` or `tags-todo`) with a
# `match` (keywords such as `NEXT|WAIT` for `todo`, a match string such as
# `work-home` or `PRIORITY="A"` for `tags`), or `blocks`, a list of such views
# shown one after the other. `header` replaces a view's header (empty: none),
# and `settings` holds agenda options for the command or a block
# (`org_agenda_span`, `org_agenda_start_day`…). A command with only a key and
# a description groups the commands whose keys start with its key.
# (org-agenda-custom-commands)
org_agenda_custom_commands = []
```

<!-- END generated -->

Helix uses only some of them: it has no agenda view, and no way to ask
for a date, a choice or text on the server's behalf, so the agenda,
refile, capture and date prompt options do nothing there. `org-lsp
agenda` prints the agenda in a terminal and takes the agenda options as
`--options '{"span": "day"}'`.

## Changing them

Set only what you want different:

```toml
[language-server.org-lsp]
command = "org-lsp"

[language-server.org-lsp.config]
org_todo_keywords = ["TODO(t)", "NEXT(n)", "WAIT(w@/!)", "|", "DONE(d!)", "CANCELLED(c@)"]
org_log_done = "time"
org_log_into_drawer = true
org_priority_lowest = "E"
```

A list replaces the default list whole.

A file can set some of these for itself, as in Emacs: `#+TODO:` lines
replace `org_todo_keywords` and `#+PRIORITIES:` replaces the three `org_priority_…`
options in that file.

The server reads its options when it starts: run `:lsp-restart` after a
change. An option with a value of the wrong type is ignored, and the server
says so in Helix's log (`:log-open`); the other options still apply.
