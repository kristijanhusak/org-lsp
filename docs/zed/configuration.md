# Configuring Zed

The extension's settings go in Zed's settings (**zed: open settings**).

## Zed's settings for Org

```json
{
  "languages": {
    "Org": { "semantic_tokens": "combined", "inlay_hints": { "enabled": true } }
  }
}
```

- `semantic_tokens`: Zed asks a language server for semantic tokens only
  when a language's `semantic_tokens` says so. `combined` colors Org
  files with the tree-sitter grammar and the server's tokens on top;
  `full` uses the server's tokens alone, without the grammar's colors.
  Source and export blocks (`#+begin_src python`, `#+begin_export html`)
  show their code in its language's colors, through the grammar's
  injections, with `combined` (with `full` there is no grammar to inject
  into). The grammar has no node for inline source blocks
  (`src_python{…}`), so their code is not colored.
- `inlay_hints`: files with `org-indent-mode` (`org_startup_indented`,
  `#+STARTUP: indent`) show indented by level, without being changed,
  through inlay hints of blanks before their lines. The leading stars
  still show; Zed cannot hide them. Pretty entities (`\alpha` as `α`) and
  the clock's times are inlay hints too.

## The server

The extension starts `org-lsp` from your PATH (`cargo install --path
server/org-lsp` in a checkout puts it in `~/.cargo/bin`), or else
downloads the nightly build for your platform once. `binary.path` names a
server binary of your own:

```json
{ "lsp": { "org-lsp": { "binary": { "path": "/path/to/org-lsp/target/release/org-lsp" } } } }
```

## The server's options

`lsp.org-lsp.initialization_options` passes the server's options. Below,
every option with its default; an option that is unset is a comment. You
only write the ones you want to change. The shared configuration file
([docs/configuration.md](../configuration.md)) has the same options, for
every editor at once; `initialization_options` goes on top of it.

<!-- The block below is written by scripts/generate-docs.mjs from the
     server's table of options. Edit server/org-lsp/src/settings.rs, not
     the block. -->

<!-- BEGIN generated: options -->

```jsonc
{
  "lsp": {
    "org-lsp": {
      "initialization_options": {
        // The shared configuration file, whose options this editor's go on
        // top of: `false` for none, or the path of another file (JSON if it
        // ends in `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, else
        // `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp`
        // (`~/.config/org-lsp`; `%APPDATA%\org-lsp` on Windows).
        // "config_file": unset: the shared file, `~/.config/org-lsp/config.toml` or `config.json`

        // Files, directories and glob patterns (`*`, `?`, `**`) the agenda is
        // built from. `~/` is the home directory, and a directory stands for
        // the Org files directly in it. When empty, the agenda is built from
        // the Org files under the workspace folders (hidden directories,
        // `node_modules` and `target` left out), or else from the Org files
        // that are open. (org-agenda-files)
        "org_agenda_files": [],

        // The TODO sequence, written like Org's `#+TODO:` line. `|` separates
        // the states that count as done; without it the last keyword is the
        // only done state. `TODO(t)` gives a keyword a key for fast
        // selection, and `WAIT(w@/!)` makes it log a note (`@`) or the time
        // (`!`) on entering and leaving it. Several sequences go in a list of
        // lists, `[["TODO", "|", "DONE"], ["REPORT", "|", "FIXED"]]`; cycling
        // stays in the sequence of the entry, and `C-S-<right>` switches to
        // the next. A list that starts with `type` is a type sequence
        // (`#+TYP_TODO:`): each of its keywords goes straight to done. A
        // file's own `#+TODO:` lines take precedence. (org-todo-keywords)
        "org_todo_keywords": ["TODO", "|", "DONE"],

        // Faces for particular TODO keywords: a color, `{ "WAIT": "orange"
        // }`, or attributes written as Org writes them, `{ "CANCELLED":
        // ":foreground gray :slant italic :strike-through t" }`
        // (`:foreground`, `:background`, `:weight bold`, `:slant italic`,
        // `:underline t`, `:strike-through t`). Shown in Org files and in the
        // agenda by Neovim, Vim and VS Code. (org-todo-keyword-faces)
        "org_todo_keyword_faces": {  },

        // Faces for particular priorities, by their letter, written as in
        // `org_todo_keyword_faces`: `{ "A": ":foreground red :weight bold"
        // }`. (org-priority-faces)
        "org_priority_faces": {  },

        // Write `CLOSED:` with the time when an entry is done, and with
        // `note` ask for a note too. A file's `#+STARTUP:` words `logdone`,
        // `lognotedone` and `nologdone` change it there; an entry's `LOGGING`
        // property (inherited) says what its state changes log instead: those
        // words, `logrepeat` and its relatives, and keywords with what they
        // log (`WAIT(@/!)`), or `nil` for nothing. (org-log-done)
        "org_log_done": false,

        // Record when a repeating entry is done and moves to its next date.
        // `#+STARTUP:` `logrepeat`, `lognoterepeat` and `nologrepeat` change
        // it in a file, and the `LOGGING` property in an entry.
        // (org-log-repeat)
        "org_log_repeat": "time",

        // Moving to the next item from the last goes to the first, and back;
        // moving the last item down puts it first, the first up puts it last.
        // (org-list-use-circular-motion)
        "org_list_use_circular_motion": false,

        // An entry cannot be done while a child is not, or, under a parent
        // with the `ORDERED` property, while an earlier sibling is not: the
        // change is refused with what blocks it.
        // (org-enforce-todo-dependencies)
        "org_enforce_todo_dependencies": false,

        // Dim the tasks of the agenda and the TODO list that their
        // dependencies keep from being done (with
        // `org_enforce_todo_dependencies` or
        // `org_enforce_todo_checkbox_dependencies`); `invisible` leaves them
        // out instead, except those blocked by checkboxes, which stay dimmed.
        // (org-agenda-dim-blocked-tasks)
        "org_agenda_dim_blocked_tasks": true,

        // An entry cannot be done while it has an unchecked box.
        // (org-enforce-todo-checkbox-dependencies)
        "org_enforce_todo_checkbox_dependencies": false,

        // Record when an entry is refiled (`Refiled on [2026-05-18 Mon
        // 10:30]`), with `note` asking for a note too. `#+STARTUP:`
        // `logrefile`, `lognoterefile` and `nologrefile` change it in the
        // file refiled to. (org-log-refile)
        "org_log_refile": false,

        // Record when a scheduled date changes or is removed (`Rescheduled
        // from "[2026-05-18 Mon]" on …`), with `note` asking for a note too.
        // `#+STARTUP:` `logreschedule`, `lognotereschedule` and
        // `nologreschedule` change it in a file. (org-log-reschedule)
        "org_log_reschedule": false,

        // Record when a deadline changes or is removed (`New deadline from
        // …`), with `note` asking for a note too. `#+STARTUP:`
        // `logredeadline`, `lognoteredeadline` and `nologredeadline` change
        // it in a file. (org-log-redeadline)
        "org_log_redeadline": false,

        // Put state changes and notes into a drawer: `true` for `LOGBOOK`, or
        // the name of a drawer. `#+STARTUP:` `logdrawer` and `nologdrawer`
        // change it in a file; an entry's `LOG_INTO_DRAWER` property
        // (inherited) says which drawer for it (`t` for `LOGBOOK`, `nil` for
        // none). (org-log-into-drawer)
        "org_log_into_drawer": false,

        // Where tags are aligned after a change to a headline. Negative: they
        // end at that column. (org-tags-column)
        "org_tags_column": -77,

        // Which tags entries inherit from their parents and from
        // `#+FILETAGS:`: `true` for all, `false` for none, a list of tags, or
        // a regexp (Emacs syntax) tags must match. Inherited tags count in
        // tag searches and show in the agenda. (org-use-tag-inheritance)
        "org_use_tag_inheritance": true,

        // Tags never inherited, when `org_use_tag_inheritance` is `true` or a
        // regexp. (org-tags-exclude-from-inheritance)
        "org_tags_exclude_from_inheritance": [],

        // Which properties an entry without them takes from its parents, the
        // file's property drawer and `#+PROPERTY:` lines, in property
        // searches (`OWNER="alice"`): `true` for all, `false` for none, a
        // list of names, or a regexp (Emacs syntax). `CATEGORY` and `ARCHIVE`
        // are always inherited. (org-use-property-inheritance)
        "org_use_property_inheritance": false,

        // Properties every entry inherits, after those of `#+PROPERTY:`
        // lines: chiefly the allowed values of a property, which setting it
        // offers and `S-<right>` and `S-<left>` go through on its line (`{
        // "Effort_ALL": "0:10 0:30 1:00 2:00" }`; `:ETC` allows others too).
        // (org-global-properties)
        "org_global_properties": {  },

        // Where `CLOCK:` lines go: `true` for the drawer of
        // `org_log_into_drawer` (`LOGBOOK` when that is not a name), the name
        // of a drawer, or `false` for the entry itself.
        // (org-clock-into-drawer)
        "org_clock_into_drawer": true,

        // Clocking out after less than a minute removes the `CLOCK:` line.
        // (org-clock-out-remove-zero-time-clocks)
        "org_clock_out_remove_zero_time_clocks": false,

        // Marking the clocked entry done stops the clock.
        // (org-clock-out-when-done)
        "org_clock_out_when_done": true,

        // The span whose clocks `org.clock.display` (`C-c C-x C-d`) adds up.
        // (org-clock-display-default-range)
        "org_clock_display_default_range": "thisyear",

        // After how many minutes without a message from the editor a clock
        // that runs asks what to do with the idle time: keep it, subtract it
        // (and clock out), or cancel the clock. (org-clock-idle-time)
        // "org_clock_idle_time": unset: none

        // How many entries clocked into the clock history keeps, for
        // `org.clock.in` with a count and `org.clock.in.last`.
        // (org-clock-history-length)
        "org_clock_history_length": 5,

        // The parameters a new clock table gets on its `#+BEGIN:` line from
        // `org.clock.report` (`C-c C-x C-r`), after `:scope`, as Emacs Lisp
        // reads them (`:maxlevel 3 :link t`).
        // (org-clock-clocktable-default-properties)
        "org_clock_clocktable_default_properties": ":maxlevel 2",

        // How a clock table writes its total time and the words before it;
        // `%s` is the text. (org-clock-total-time-cell-format)
        "org_clock_total_time_cell_format": "*%s*",

        // How a clock table over several files writes the words before a
        // file's time; `%s` is the text. (org-clock-file-time-cell-format)
        "org_clock_file_time_cell_format": "*%s*",

        // The TODO state an entry is put in when the clock starts on it
        // (`NEXT`), unless it has it already. (org-clock-in-switch-to-state)
        // "org_clock_in_switch_to_state": unset: none

        // The TODO state an entry is put in when the clock stops on it.
        // (org-clock-out-switch-to-state)
        // "org_clock_out_switch_to_state": unset: none

        // Org's linter checks the documents and reports what it finds as
        // diagnostics: misplaced headings and planning lines, links to
        // nowhere, duplicate names, footnotes without definitions… What it
        // finds is information, a hint for a checker Org trusts little: an
        // Org file is text, nothing in it is an error or a warning.
        // (org-lint)
        "org_lint": true,

        // The `org-lint` checkers to leave out, by name, as a diagnostic's
        // code shows it (`"misplaced-heading"`). (org-lint)
        "org_lint_ignore": [],

        // Whether `org.link.store` (`C-c l`) links to an entry by its ID:
        // `true` makes one when the entry has none, `"create-if-interactive"`
        // only from the command (not for capture's `%a`),
        // `"create-if-interactive-and-no-custom-id"` likewise unless the
        // entry has a `CUSTOM_ID`, `"use-existing"` only when it has one
        // already. Off, the link searches for the headline.
        // (org-id-link-to-org-use-id)
        "org_id_link_to_org_use_id": false,

        // Links to a place in a file search for it
        // (`file:notes.org::*Heading`); `C-u` turns it around for one link.
        // (org-link-context-for-files)
        "org_link_context_for_files": true,

        // ID links to a place below the entry's headline search for it as
        // well (`id:…::name`). (org-id-link-use-context)
        "org_id_link_use_context": true,

        // An ID link may use the ID of a parent entry, with a search for the
        // place. (org-id-link-consider-parent-id)
        "org_id_link_consider_parent_id": false,

        // How links to files write the path: `"adaptive"` relative below the
        // file's directory and absolute elsewhere, `"absolute"` (with `~`),
        // `"noabbrev"` (without), `"relative"`. (org-link-file-path-type)
        "org_link_file_path_type": "adaptive",

        // Stored links stay stored once inserted.
        // (org-link-keep-stored-after-insertion)
        "org_link_keep_stored_after_insertion": false,

        // Completing a tag offers the tags of every agenda file, after those
        // of the file. Off, only the file's own. Setting tags offers them
        // too. (org-complete-tags-always-offer-all-agenda-tags)
        "org_complete_tags_always_offer_all_agenda_tags": false,

        // Bibliography files for the citations of every document
        // (`[cite:@key]`), after those its `#+BIBLIOGRAPHY:` lines name:
        // BibTeX (`.bib`, `.bibtex`) or CSL-JSON (`.json`). Their entries are
        // completed after `@`, shown over a key, and where following a key
        // goes; a key none has is marked. (org-cite-global-bibliography)
        "org_cite_global_bibliography": [],

        // Link abbreviations: `[[gh:org-lsp]]` with `{ "gh":
        // "https://github.com/%s" }` follows `https://github.com/org-lsp`.
        // `%s` stands for what follows the colon, `%h` for it URL-encoded;
        // without either it goes at the end. A file's `#+LINK:` lines come
        // first. A function (`%(…)`) is not supported.
        // (org-link-abbrev-alist)
        "org_link_abbrev_alist": {  },

        // The tags offered when setting tags in a file without a `#+TAGS:`
        // line, written like one: `"work(w) home(h) { @office(o) @home(m) } [
        // GTD : Control Persp ]"` gives `work` the key `w`, makes `@office`
        // and `@home` exclusive, and groups `Control` and `Persp` under
        // `GTD`; `\n` starts a new line. A list of these words does as well.
        // Unset, the tags used in the file are offered. (org-tag-alist)
        "org_tag_alist": "",

        // Tags offered in every file, before those of `#+TAGS:` or
        // `org_tag_alist`, written the same way. (org-tag-persistent-alist)
        "org_tag_persistent_alist": "",

        // A group tag (`[ GTD : Control Persp ]`, `{ Context : @home @office
        // }` in `#+TAGS:` or `org_tag_alist`) in the match string of a tags
        // view stands for itself and the tags of its group. (org-group-tags)
        "org_group_tags": true,

        // Pick tags by key, each key turning its tag on or off until RET:
        // `auto` when some tag has a key, `true` always (keys are given to
        // the tags without one), `false` never: the tags are typed instead.
        // (org-use-fast-tag-selection)
        "org_use_fast_tag_selection": "auto",

        // In fast tag selection, the first key that changes the tags also
        // ends it. (org-fast-tag-selection-single-key)
        "org_fast_tag_selection_single_key": false,

        // Sort the tags of a headline when they are set: alphabetically, or
        // the other way. (org-tags-sort-function)
        // "org_tags_sort_function": unset: the order they were picked in

        // Align the tags of a headline to `org_tags_column` when they are
        // set. (org-auto-align-tags)
        "org_auto_align_tags": true,

        // The state a repeating entry returns to when it is done: a keyword,
        // or `true` for the state it had. (org-todo-repeat-to-state)
        // "org_todo_repeat_to_state": unset: the first keyword of the sequence

        // The highest priority. (org-priority-highest)
        "org_priority_highest": "A",

        // The lowest priority. (org-priority-lowest)
        "org_priority_lowest": "C",

        // The priority of an entry without a cookie. A file's `#+PRIORITIES:`
        // line takes precedence over the three priority options.
        // (org-priority-default)
        "org_priority_default": "B",

        // Raising or lowering the priority of an entry that has none starts
        // at the default. (org-priority-start-cycle-with-default)
        "org_priority_start_cycle_with_default": true,

        // The step in which the minutes of a timestamp change (the second
        // number of Org's option). (org-time-stamp-rounding-minutes)
        "org_time_stamp_rounding_minutes": 5,

        // A date typed without a year or month is the next one to come. With
        // `time`, a time that has passed today is tomorrow's.
        // (org-read-date-prefer-future)
        "org_read_date_prefer_future": true,

        // A typed year is kept within 1970 to 2037.
        // (org-read-date-force-compatible-dates)
        "org_read_date_force_compatible_dates": true,

        // Where entries can be refiled to. Each entry has `files`: `current`
        // (the entry's own file), `agenda`, a path or a list of paths; and
        // one of `level`, `maxlevel`, `tag`, `todo`, `regexp` (the Rust
        // `regex` crate's syntax) or `all` (true), which says which headlines
        // there are targets. (org-refile-targets)
        "org_refile_targets": [{ "files": "current", "level": 1 }],

        // Name targets by their outline path (`Projects/Alpha/`). The three
        // names start the path with the file, and offer the file itself, for
        // its top level. (org-refile-use-outline-path)
        "org_refile_use_outline_path": false,

        // A target that is not there yet can be named after an existing one,
        // `Projects/New`: it is made, as the last child of `Projects`, and
        // the entry goes under it. `confirm` asks first.
        // (org-refile-allow-creating-parent-nodes)
        "org_refile_allow_creating_parent_nodes": false,

        // The name of an Emacs Lisp function that decides which headlines are
        // refile targets: the server's Emacs calls it at the start of each,
        // and keeps those it says yes to. Where it leaves the point, the
        // search goes on: a function can skip a subtree.
        // (org-refile-target-verify-function)
        // "org_refile_target_verify_function": unset: none

        // A refiled entry goes first under its target, not last.
        // (org-reverse-note-order)
        "org_reverse_note_order": false,

        // Where archived subtrees go: a file, `::`, and a headline under
        // which they go (they go to the end of the file without one). `%s` is
        // the name of the file the subtree is in; an empty file part is that
        // file. `datetree/` as the headline files them under year, month and
        // day, by when they were closed. An `ARCHIVE` property of the entry
        // or a parent, or a `#+ARCHIVE:` line, says otherwise.
        // (org-archive-location)
        "org_archive_location": "%s_archive::",

        // An archived subtree goes first under its archive headline, not
        // last. (org-archive-reversed-order)
        "org_archive_reversed_order": false,

        // Mark archived entries done: `true` for the first done keyword, or
        // the keyword to use. (org-archive-mark-done)
        "org_archive_mark_done": false,

        // Give an archived subtree the tags it inherited: always, never, or
        // when it stays in its file. (org-archive-subtree-add-inherited-tags)
        "org_archive_subtree_add_inherited_tags": "infile",

        // What an archived entry keeps as `ARCHIVE_…` properties: `time`,
        // `file`, `olpath` (its outline path), `category`, `todo`, `itags`
        // (inherited tags), `ltags` (its own tags) and `olid` (its parent's
        // ID). (org-archive-save-context-info)
        "org_archive_save_context_info": ["time", "file", "olpath", "category", "todo", "itags"],

        // The headline of the sibling `org.archive.sibling` archives under.
        // (org-archive-sibling-heading)
        "org_archive_sibling_heading": "Archive",

        // What a new archive file starts with; `%s` is the path of the file
        // the subtree came from. (org-archive-file-header-format)
        "org_archive_file_header_format": "\nArchived entries from file %s\n\n",

        // What `org.archive.default` does: archive the subtree, archive it
        // under its sibling, or tag it. (org-archive-default-command)
        "org_archive_default_command": "subtree",

        // Where templates are offered. Each has the `key` of a template,
        // `contexts`, rules such as `{ "in_file": "work" }` (`in_file`,
        // `in_mode`, `in_buffer`, `not_in_file`, `not_in_mode` or
        // `not_in_buffer`, each with a regexp of the `regex` crate's syntax,
        // matched ignoring case), and maybe `use`, the key of the template it
        // stands for there. A template with contexts is offered only where
        // one of their rules holds. The mode is `org-mode` in an Org file and
        // `org-agenda-mode` in the agenda, where there is no file; the
        // buffer's name is the file's. (org-capture-templates-contexts)
        "org_capture_templates_contexts": [],

        // Capture templates. Each has a `key`, a `description`, a `type`
        // (`entry`, the default, `plain`, `item`, `checkitem` or
        // `table-line`), a `target` and a `template` (text, or a list of
        // lines), and may have `prepend`, `empty_lines`,
        // `empty_lines_before`, `empty_lines_after`, `immediate_finish`,
        // `tree_type`, `time_prompt`, `table_line_pos`, `clock_in`,
        // `clock_keep`, `clock_resume` and `jump_to_captured`. The target is
        // a list: `file` and a path; `file+headline`, a path and a headline;
        // `file+olp`, a path and the headlines of an outline path;
        // `file+regexp`, a path and a regular expression;
        // `file+olp+datetree`, a path and the headlines of an outline path,
        // maybe none, for a date tree under them; `id` and an ID; `clock`,
        // the entry clocked now; `file+function`, a path and the name of an
        // Emacs Lisp function that moves the point there, or `function` and
        // the name of one that visits the file too (run by the server's
        // Emacs). An empty path is `org_default_notes_file`, and without a
        // target the entry goes at the end of `org_default_notes_file`. A
        // template with only a key and a description groups the templates
        // whose keys start with its key. (org-capture-templates)
        "org_capture_templates": [{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }],

        // Where the relative paths of capture targets are. (org-directory)
        "org_directory": "~/org",

        // The target of a capture template whose file is empty.
        // (org-default-notes-file)
        "org_default_notes_file": "~/.notes",

        // Show a link as its description (or its target, when it has none)
        // and hide the rest: the brackets, and the target of a link with a
        // description. The text is marked `hidden` in the highlighting, and
        // editors that can conceal text conceal it. (org-link-descriptive)
        "org_link_descriptive": true,

        // Hide the markers around `*bold*`, `/italic/`, `_underline_`,
        // `+strike-through+`, `=verbatim=` and `~code~`, as for links.
        // (org-hide-emphasis-markers)
        "org_hide_emphasis_markers": false,

        // Show entities (`\alpha`, `\to`) as their character (`α`, `→`): the
        // entity is hidden as link brackets are, and its character is an
        // inlay hint in its place. (org-pretty-entities)
        "org_pretty_entities": false,

        // With `org_pretty_entities`, show sub- and superscripts (`x^2`,
        // `H_{2}O`, `a_{n}`) raised and lowered: in Unicode (`x²`, `H₂O`,
        // `aₙ`) where every character has a raised or lowered form, as an
        // inlay hint in place of the hidden text; the others are left as they
        // are. (org-pretty-entities-include-sub-superscripts)
        "org_pretty_entities_include_sub_superscripts": true,

        // Which sub- and superscripts `org_pretty_entities` shows: all, only
        // those in braces (`a_{b}`, `"{}"`), or none.
        // (org-use-sub-superscripts)
        "org_use_sub_superscripts": true,

        // RET on the first line of a list item makes the next item, numbered
        // and with a checkbox as M-RET (`org-meta-return`) makes it, the
        // items after it renumbered, as nvim-orgmode's
        // `org_return_uses_meta_return` does. Off, RET is Org's: a new line
        // in the item. `org.return.item` (`<S-CR>`, Shift+Enter, with the vim
        // keys) always does it.
        "return_uses_meta_return": false,

        // The deepest headings `org.goto` (`C-c C-j`) offers.
        // (org-goto-max-level)
        "org_goto_max_level": 5,

        // How many months the calendar of the date prompt shows: 3 side by
        // side, as Emacs's calendar shows them, the one with the selected day
        // in the middle; or 1, that month alone, for a narrow window.
        "calendar_months": 3,

        // The Emacs program, for what only Org itself does: export
        // (`org.export.dispatch`). The server starts it (`emacs --batch -Q`)
        // when that is first asked for and keeps it running, so that Org
        // loads once. Without Emacs, everything else works as before.
        "emacs": "emacs",

        // A file of Emacs Lisp that Emacs loads before Org, for the variables
        // export reads (`org-export-backends`, `org-html-…`, `org-latex-…`,
        // `org-publish-project-alist`): a file of its own, or your init file.
        // `~/` is the home directory.
        // "emacs_config_file": unset: none

        // Emacs Lisp that Emacs evaluates after `emacs_config_file` and
        // before loading Org, for a few settings without a file: `(setq
        // org-export-with-toc nil)`, or `(add-to-list 'load-path
        // "~/src/org-mode/lisp")` for another Org. A yes-or-no question Emacs
        // asks while it works (`org-confirm-babel-evaluate`) is asked in the
        // editor.
        // "emacs_config": unset: none

        // Save the files a command changes other than the one it was run in:
        // an entry's file changed from the agenda, the target of a capture or
        // of a refile. The editor saves them once it has made the change.
        // Off, they are left with unsaved changes, as Emacs leaves them.
        "auto_save": true,

        // With at least this many blank lines after a subtree, the last one
        // stays outside its fold. Negative: all of them stay outside. `0`:
        // none does. (org-cycle-separator-lines)
        "org_cycle_separator_lines": 2,

        // How files show when opened: `"overview"` (or `"fold"`) only the top
        // headlines, `"content"` all the headlines, `"show2levels"` to
        // `"show5levels"` the headlines down to that level,
        // `"showeverything"` (or `"nofold"`, `"showall"`) everything.
        // `#+STARTUP:` decides for a file. Unless everything shows, a
        // subtree's `VISIBILITY` property (`folded`, `children`, `content`,
        // `all`) decides for it, archived subtrees are folded, and drawers
        // too. Folds hide whole subtrees, so where Org shows a headline's
        // children but not its text, the text shows. (org-startup-folded)
        "org_startup_folded": "showeverything",

        // Shrink the columns of every table that have a width cookie (`<10>`)
        // when a file opens. `#+STARTUP: shrink` decides for a file. Shrunk
        // fields show their first characters and `…`, in editors that can
        // hide text. (org-startup-shrink-all-tables)
        "org_startup_shrink_all_tables": false,

        // Fold blocks when a file opens folded (`org_startup_folded`).
        // `#+STARTUP: hideblocks` and `nohideblocks` decide for a file.
        // (org-cycle-hide-block-startup)
        "org_cycle_hide_block_startup": false,

        // Fold drawers when a file opens folded (`org_startup_folded`).
        // (org-cycle-hide-drawer-startup)
        "org_cycle_hide_drawer_startup": true,

        // Leave archived subtrees open when a file opens folded
        // (`org_startup_folded`). (org-cycle-open-archived-trees)
        "org_cycle_open_archived_trees": false,

        // Indent the lines of an entry past its headline's stars: `true` for
        // all of them, `"headline-data"` for the planning line, drawers and
        // clock lines right below the headline. Applies to new lines, to
        // re-indenting (range formatting) and to what commands write.
        // (org-adapt-indentation)
        "org_adapt_indentation": false,

        // Show the images of links to image files in place when a file opens,
        // as `C-c C-x C-v` (`org.link.preview`) shows them. `#+STARTUP:
        // inlineimages` (or `linkpreviews`) and `noinlineimages` decide for a
        // file. Editors that can show images: Neovim from 0.13
        // (`vim.ui.img`), in a terminal with the Kitty graphics protocol.
        // (org-startup-with-link-previews)
        "org_startup_with_link_previews": false,

        // Show the LaTeX fragments as images when a file opens, as `C-c C-x
        // C-l` (`org.latex.preview`) shows them, made by Emacs. `#+STARTUP:
        // latexpreview` and `nolatexpreview` decide for a file.
        // (org-startup-with-latex-preview)
        "org_startup_with_latex_preview": false,

        // Show files indented by level without changing them
        // (`org-indent-mode`): the text under a headline lines up with its
        // title, and leading stars are hidden. `#+STARTUP: indent` and
        // `noindent` decide for a file. It turns `org_adapt_indentation` off
        // in such files, unless that is `"headline-data"`. Editors that can
        // show it: Neovim, and VS Code (not on wrapped lines).
        // (org-startup-indented)
        "org_startup_indented": false,

        // Hide all the stars of a headline but the last; they keep their
        // place. `#+STARTUP: hidestars` and `showstars` decide for a file.
        // (org-hide-leading-stars)
        "org_hide_leading_stars": false,

        // Leave the code of source blocks as it is indented when
        // re-indenting. (org-src-preserve-indentation)
        "org_src_preserve_indentation": false,

        // How far code is indented past its source block when re-indenting.
        // (org-edit-src-content-indentation)
        "org_edit_src_content_indentation": 2,

        // How much time the agenda shows. (org-agenda-span)
        "org_agenda_span": "week",

        // The day a week or fortnight starts on; `false` starts it today.
        // (org-agenda-start-on-weekday)
        "org_agenda_start_on_weekday": 1,

        // How many days before a deadline it shows up.
        // (org-deadline-warning-days)
        "org_deadline_warning_days": 14,

        // How many days after its date a scheduled entry shows up.
        // (org-scheduled-delay-days)
        "org_scheduled_delay_days": 0,

        // How many days a scheduled entry that is not done keeps showing up.
        // (org-scheduled-past-days)
        "org_scheduled_past_days": 10000,

        // Org modules to load. `org-habit` turns habits on: entries with the
        // `STYLE` property `habit` and a scheduled date with a repeater
        // (`.+2d`, `.+2d/4d` for a deadline too) show in the agenda with a
        // graph of the days they were done. Others are ignored. (org-modules)
        "org_modules": [],

        // The column the consistency graph of a habit starts at in the
        // agenda. (org-habit-graph-column)
        "org_habit_graph_column": 40,

        // How many days before today a habit's graph shows.
        // (org-habit-preceding-days)
        "org_habit_preceding_days": 21,

        // How many days after today a habit's graph shows.
        // (org-habit-following-days)
        "org_habit_following_days": 7,

        // Show habits in the agenda. (org-habit-show-habits)
        "org_habit_show_habits": true,

        // Show habits only on today's date, not on future days of the agenda.
        // (org-habit-show-habits-only-for-today)
        "org_habit_show_habits_only_for_today": true,

        // Show every habit today, even those not scheduled for today.
        // (org-habit-show-all-today)
        "org_habit_show_all_today": false,

        // The mark of today in a habit's graph. (org-habit-today-glyph)
        "org_habit_today_glyph": "!",

        // The mark of a day a habit was done in its graph.
        // (org-habit-completed-glyph)
        "org_habit_completed_glyph": "*",

        // A day a habit was done is green in its graph even when it was done
        // late. (org-habit-show-done-always-green)
        "org_habit_show_done_always_green": false,

        // How many days a habit that is not done keeps showing up.
        // (org-habit-scheduled-past-days)
        // "org_habit_scheduled_past_days": unset: `org_scheduled_past_days`

        // How many days a missed deadline keeps showing up.
        // (org-deadline-past-days)
        "org_deadline_past_days": 10000,

        // Leave out scheduled entries that are done.
        // (org-agenda-skip-scheduled-if-done)
        "org_agenda_skip_scheduled_if_done": false,

        // Leave out deadlines of entries that are done.
        // (org-agenda-skip-deadline-if-done)
        "org_agenda_skip_deadline_if_done": false,

        // Leave out timestamps of entries that are done.
        // (org-agenda-skip-timestamp-if-done)
        "org_agenda_skip_timestamp_if_done": false,

        // Show the future dates of repeating entries: all of them, none, or
        // only the next. (org-agenda-show-future-repeats)
        "org_agenda_show_future_repeats": true,

        // Show days without entries. (org-agenda-show-all-dates)
        "org_agenda_show_all_dates": true,

        // Where tags are aligned in the agenda: a column (negative: where
        // they end), or `auto` for the right edge of the window. With `auto`,
        // the editors that tell the server how wide the agenda's window is
        // (Neovim, Vim, Sublime Text) have the tags there, and the agenda
        // follows the window as it is resized; the others (VS Code, Zed) have
        // them end at column 80. (org-agenda-tags-column)
        "org_agenda_tags_column": "auto",

        // Draw the time grid. (org-agenda-use-time-grid)
        "org_agenda_use_time_grid": true,

        // The time grid, as Org writes it: the days that have it (`daily`: a
        // one-day agenda; `weekly`: every day of a longer one; `today`:
        // today; `require-timed`: only days with an entry that has a time;
        // `remove-match`: no grid line at a time an entry already has), the
        // times of its lines (`800` for 8:00), what follows a time that has
        // no end time, and the text of a grid line. (org-agenda-time-grid)
        "org_agenda_time_grid": [["daily", "today", "require-timed"], [800, 1000, 1200, 1400, 1600, 1800, 2000], "......", "----------------"],

        // The text of the line for the current time.
        // (org-agenda-current-time-string)
        "org_agenda_current_time_string": "now - - - - - - - - - - - - - - - - - - - - - - - - -",

        // Show the current time in the grid.
        // (org-agenda-show-current-time-in-grid)
        "org_agenda_show_current_time_in_grid": true,

        // The days drawn as the weekend (0 is Sunday).
        // (org-agenda-weekend-days)
        "org_agenda_weekend_days": [6, 0],

        // Start the agenda in log mode, which shows what was closed and
        // clocked each day (`l` toggles it). (org-agenda-start-with-log-mode)
        "org_agenda_start_with_log_mode": false,

        // Start the agenda with the clock report under it (`R` toggles it).
        // (org-agenda-start-with-clockreport-mode)
        "org_agenda_start_with_clockreport_mode": false,

        // Start the agenda with entries in archived trees (`trees`; `v a`
        // toggles it), or those and the files the agenda files archive to
        // (`files`; `v A`). (org-agenda-start-with-archives-mode)
        "org_agenda_start_with_archives_mode": false,

        // What log mode shows: `closed` entries, `clock` lines and `state`
        // changes. (org-agenda-log-mode-items)
        "org_agenda_log_mode_items": ["closed", "clock"],

        // Keep the marks of a bulk action (`B`) after it (`p` in its menu
        // toggles this for one action). (org-agenda-persistent-marks)
        "org_agenda_persistent_marks": false,

        // Start the agenda in follow mode, which shows the entry of the
        // cursor's line in another window as the cursor moves (`F` toggles
        // it). (org-agenda-start-with-follow-mode)
        "org_agenda_start_with_follow_mode": false,

        // Start the agenda with some text of each entry under its line (`E`
        // toggles it). (org-agenda-start-with-entry-text-mode)
        "org_agenda_start_with_entry_text_mode": false,

        // The most lines of an entry's text `E` shows.
        // (org-agenda-entry-text-maxlines)
        "org_agenda_entry_text_maxlines": 5,

        // What each line of an entry's text starts with in the agenda.
        // (org-agenda-entry-text-leaders)
        "org_agenda_entry_text_leaders": "    > ",

        // Emacs regexps whose matches are left out of the entry text `E`
        // shows. (org-agenda-entry-text-exclude-regexps)
        "org_agenda_entry_text_exclude_regexps": [],

        // What goes between the blocks of a block agenda: one character
        // repeated across 79 columns, a longer text as it is, or `false` for
        // nothing. In a block's `settings`, the separator above that block.
        // (org-agenda-block-separator)
        "org_agenda_block_separator": "=",

        // Leave out the separators between blocks and the header over agenda
        // blocks. (org-agenda-compact-blocks)
        "org_agenda_compact_blocks": false,

        // Leave out the blocks of a block agenda that show no entries. Not in
        // Org.
        "agenda_hide_empty_blocks": false,

        // Read every search string of the search view (`s`) as words that
        // must (`+word`, `word`) or must not (`-word`) occur, `{regexp}`s and
        // `"quoted phrases"`. Off, it is a phrase whose spaces match any
        // whitespace, unless its first word starts with `+`, `-` or `{`.
        // (org-agenda-search-view-always-boolean)
        "org_agenda_search_view_always_boolean": false,

        // The words of a Boolean search match whole words only, as a search
        // string starting with `:` asks.
        // (org-agenda-search-view-force-full-words)
        "org_agenda_search_view_force_full_words": false,

        // The search view shows headlines down to this level, a match below
        // one counting for the headline above: 0 for every level.
        // (org-agenda-search-view-max-outline-level)
        "org_agenda_search_view_max_outline_level": 0,

        // Files the search view (`s`) searches besides the agenda files,
        // after them, as `org_agenda_files` names them. `agenda-archives`
        // first adds the files the agenda files archive to. A file listed
        // twice is searched at its last place.
        // (org-agenda-text-search-extra-files)
        "org_agenda_text_search_extra_files": [],

        // What comes before the headline on a line of the agenda (`agenda`),
        // the TODO list (`todo`), tags searches (`tags`) and text searches
        // (`search`): `%c` the category, `%t` the time, `%s` the leader
        // (`Scheduled: `), `%e` the effort, `%l` a blank per level, `%T` the
        // last tag, `%b` the headlines above (breadcrumbs), `%i` the icon
        // (none). A width pads (`%-12c` on the right, `%12c` on the left),
        // `.N` cuts, `%?` leaves out an empty field, and a character before
        // the letter follows a value that is not empty (`%-12:c`). `%(…)`
        // (Emacs Lisp) is left empty. (org-agenda-prefix-format)
        "org_agenda_prefix_format": { "agenda": " %i %-12:c%?-12t% s", "search": " %i %-12:c", "tags": " %i %-12:c", "todo": " %i %-12:c" },

        // How a TODO keyword is written in the agenda: `%-4s` pads it to four
        // columns, so that the titles line up; an empty format leaves
        // keywords out. (org-agenda-todo-keyword-format)
        "org_agenda_todo_keyword_format": "%-1s",

        // The leader of a scheduled entry on its day, and on today when its
        // day has passed (`%d`: the days since).
        // (org-agenda-scheduled-leaders)
        "org_agenda_scheduled_leaders": ["Scheduled: ", "Sched.%2dx: "],

        // The leader of a deadline on its day, on today before it (`%d`: the
        // days left), and on today after it (`%d`: the days since).
        // (org-agenda-deadline-leaders)
        "org_agenda_deadline_leaders": ["Deadline:  ", "In %3d d.: ", "%2d d. ago: "],

        // The leader of a time range within a day, and of a range over days
        // (`%d`: which day of it, `%d`: how many days).
        // (org-agenda-timerange-leaders)
        "org_agenda_timerange_leaders": ["", "(%d/%d): "],

        // Leave the tags out of the agenda's lines; `prefix` only when the
        // prefix shows a tag (`%T`). (org-agenda-remove-tags)
        "org_agenda_remove_tags": false,

        // What the stuck projects view (`#`) lists: the projects the match
        // string finds whose subtree has no entry with one of the TODO
        // keywords (`*` for any not done) or tags (`*` for any), and no match
        // of the regexp. (org-stuck-projects)
        "org_stuck_projects": ["+LEVEL=2/-DONE", ["TODO", "NEXT", "NEXTACTION"], [], ""],

        // What goes between the headlines of `%b`.
        // (org-agenda-breadcrumbs-separator)
        "org_agenda_breadcrumbs_separator": "->",

        // How the agenda writes a day, as `format-time-string` does: `%A %d
        // %B %Y`, `%a %Y-%m-%d`. (org-agenda-format-date)
        // "org_agenda_format_date": unset: as `Wednesday   7 October 2026`, the week's number added on Mondays

        // The day the agenda starts from, as the date prompt reads it: `-1d`,
        // `+2d`, `2026-10-01`. (org-agenda-start-day)
        // "org_agenda_start_day": unset: today

        // Where the commands of `org_agenda_custom_commands` are offered, as
        // for `org_capture_templates_contexts`.
        // (org-agenda-custom-commands-contexts)
        "org_agenda_custom_commands_contexts": [],

        // Commands of the agenda menu. Each has a `key` and a `description`,
        // and either a `type` (`agenda`, `alltodo`, `todo`, `tags` or
        // `tags-todo`) with a `match` (keywords such as `NEXT|WAIT` for
        // `todo`, a match string such as `work-home` or `PRIORITY="A"` for
        // `tags`), or `blocks`, a list of such views shown one after the
        // other. `header` replaces a view's header (empty: none), and
        // `settings` holds agenda options for the command or a block
        // (`org_agenda_span`, `org_agenda_start_day`…). A command with only a
        // key and a description groups the commands whose keys start with its
        // key. (org-agenda-custom-commands)
        "org_agenda_custom_commands": []
      }
    }
  }
}
```

<!-- END generated -->

`org-lsp config` shows the options the server ends up with, after the
shared file and your settings. **editor: restart language server** makes
the server read changed settings.
