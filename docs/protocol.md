# Protocol

org-lsp uses standard LSP. This page says what it offers and lists the
names an editor integration deals with: options, commands, the manifest,
the agenda document and the semantic tokens.

## Capabilities

| Request | Notes |
| --- | --- |
| `textDocument/didOpen`, `didChange`, `didSave`, `didClose` | Incremental sync. A save rebuilds an open agenda. |
| `textDocument/semanticTokens/full`, `/range` | For Org files and the agenda. See [Semantic tokens](#semantic-tokens). No deltas. |
| `textDocument/foldingRange` | Subtrees, drawers, blocks, LaTeX environments and multi-line list items. Whole lines only. A subtree takes the blank lines before the next headline along, as Emacs hides them; see `org_cycle_separator_lines`. |
| `textDocument/documentSymbol` | The outline of headlines. Hierarchical when the client supports it, flat otherwise. |
| `textDocument/codeAction` | The commands that apply at the cursor. See [Commands](#commands). |
| `workspace/executeCommand` | Every command. See [Commands](#commands). |
| `textDocument/definition` | In the agenda: from a line to its entry. In an Org file: where a link or a footnote leads; on a citation key, its entry in the bibliography (`org-cite-basic-goto`: the start of its line, or for a CSL-JSON file its opening brace; the start of the file when Org finds no entry there), elsewhere in a citation the entries of all its keys. |
| `textDocument/hover` | In the agenda: the line's entry, its headline and its text up to the next heading, each drawer folded to its first line as Emacs shows a folded drawer (`:PROPERTIES:...`), as Org in a fenced block of Markdown. In an Org file, over a citation key, its entry as Org's help echo shows it (`oc-basic`'s `plain` style: `Knuth. The {\TeX}book, Addison-Wesley, 1984.`), or for a key no bibliography file has, `Suggestions:` and the keys at most two edits away from it. Elsewhere none. An editor that shows the entry in an Org buffer of its own can have it highlighted by opening that buffer as an Org document with a URI that is no file (Neovim: `org-preview:/1`); such a document is no file of an agenda built from the open files. |
| `org-lsp/preview` | The same text of an agenda line's entry with its semantic tokens, for an editor that shows it in a popup of its own: `{ "text", "data" }` (`data` as in `SemanticTokens`), or null. Params: `TextDocumentPositionParams`. |
| `textDocument/completion` | In a headline: TODO keywords for the first word after the stars, and the file's tags after a `:` in the headline's tags (`pcomplete`). `:` triggers it. After `@` in a citation (`[cite:@…`), the keys of the bibliography, each with its authors, year and title as Org's completion table has them (`detail`). |
| `workspace/textDocumentContent` | The text of the agenda. See [The agenda document](#the-agenda-document). |
| `textDocument/onTypeFormatting` | After a line break: `org-indent-line` on the new line (Org's `RET`), following `org_adapt_indentation`. No edit when the line is indented right, or when Org leaves such a line as it is (headlines, the first line of a list item, the code of a source block). |
| `textDocument/rangeFormatting` | `org-indent-region` over the lines of the range (text, drawers and blocks re-indented, node properties aligned as `org-property-format` has them), then, as `textDocument/formatting` does for the whole file, every table with a line in the range aligned whole (`org-table-align`) and the tags of the headlines in the range put in their column (`org-align-tags`). A range that ends at the start of a line leaves that line out. One edit per changed line. |

The server sends `textDocument/publishDiagnostics` for the citation keys of a document that no file of its bibliography has (`oc-basic` shows them with the `error` face): severity Error, `Unknown citation key: @key`, with the keys at most two edits away (`org-cite-basic--close-keys`) in the message and in `data.citationKeys`, from which `textDocument/codeAction` offers quick fixes (`Replace with @key`). The bibliography is the files of the document's `#+BIBLIOGRAPHY:` lines (relative to it) and of `org_cite_global_bibliography`: BibTeX files (`.bib`, read with the `biblatex` dialect, and `.bibtex`), parsed as `bibtex.el` parses them (`@String` expansion, `#`, field names in lower case), and CSL-JSON files (`.json`). A file Org cannot read (malformed, a duplicate key) adds nothing; Emacs then fails for the whole bibliography.

The server sends these requests to the client:

| Request | When |
| --- | --- |
| `workspace/applyEdit` | A command changes an Org file. |
| `window/showDocument` | To show the agenda, a capture buffer, or the file of an agenda entry. The editors open the agenda and capture buffers in a window of their own (a split by default), and an entry's file where the agenda was. With `"orgOtherWindow": true` (not LSP's; `org.agenda.goto.other`, TAB in the agenda) the editors of this project show the file in another window: one that shows it, else an empty one, else a new one. |
| `workspace/textDocumentContent/refresh` | The agenda has new text. |

And this notification:

| Notification | When |
| --- | --- |
| `org-lsp/save` | `{ "uris": [uri…] }`: with `auto_save`, the files a command changed other than the one it was run in (an entry's file changed from the agenda, the target of a capture or a refile), once the editor has applied the change. The editor saves those with changes. |
| `org-lsp/virtualIndent` | For a client that announces `experimental.orgVirtualIndent: true`: how the lines of a file with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`) or hidden leading stars (`org_hide_leading_stars`, `#+STARTUP: hidestars`) show. `{ "uri", "version", "lines" }`, each line `[line, prefix, wrap, hiddenStars]`: the columns shown before the line (`line-prefix`), before the lines it wraps into (`wrap-prefix`), and how many of a headline's leading stars show as blanks. Lines with nothing to show are left out. Sent after the file opens and after every change while it shows so, and once with no lines when it stops. Only for the version given. |
| `org-lsp/lineHighlights` | For a client that announces `experimental.orgLineHighlights: true` (or sets the option `line_highlights`): the lines of the agenda to highlight from edge to edge, as Emacs's overlays cover them, past the end of their text. `{ "uri", "lines" }`, each line `{ "line", "highlight" }`; `highlight` names the token type whose look it takes (`agendaClocking`: the entry the clock runs on, `org-agenda-clocking`). Sent once the client shows the agenda's current text, after each new text it shows, and when the lines change (clocking in or out), with no lines when none is highlighted. The text of those lines also has the token type, for clients that show only tokens. |
| `org-lsp/clock` | The clock that runs, for a status line: `null` when none does, or `{ "heading", "minutes", "effort", "text", "uri", "line" }`, the entry's headline, the minutes clocked on it so far (since `LAST_REPEAT` if it has one), its effort in minutes or `null`, the text of Emacs's mode line (`[0:25/1:00] (Write the report)`), and where the entry is. Sent when the clock starts or stops, and when the server starts with a clock running in the files. The editor counts the minutes on from there. |

And from the client, these notifications (also taken as requests, for
clients that send none of their own):

| Notification | When |
| --- | --- |
| `org-lsp/agendaCursor` | `{ "textDocument", "position" }`: the line of the agenda the cursor is on, each time it changes. In follow mode (`org.agenda.follow`) the server answers with `window/showDocument` for the entry of that line, `takeFocus: false` and `orgOtherWindow: true`: the client shows it in another window and keeps the cursor in the agenda. |
| `org-lsp/agendaWidth` | `{ "width": N }`: how many columns of text the window that shows the agenda has, when it opens and whenever its size changes. With `org_agenda_tags_column` `"auto"` the tags end at that column, and the server builds the agenda again when it changes; without it they end at column 80. |

**Position encoding.** The server picks UTF-8 if the client offers it, then
UTF-32, and falls back to UTF-16.

**Line endings.** `\n`, `\r\n` and a lone `\r` all end a line for positions.

## Options

Passed as `initializationOptions`, on top of the shared configuration file
the server reads itself (`~/.config/org-lsp/config.toml` or `config.json`; see
`docs/configuration.md`): an object such as `org_agenda_prefix_format` is
merged key by key, anything else replaces the file's. The names are Org's
variable names with `_` for `-` (`org_agenda_files`; `org-agenda-files` is
read too), and the values Org's, as JSON; options of the server's own have
plain names (`auto_save`). Defaults are Org's. The names options had before
are read and named in a message; unknown options are ignored, and named in
a message. `org-lsp settings` prints this
table as JSON, and the table below is written from it by
`scripts/generate-docs.mjs`; the configuration pages of each editor
(`docs/neovim/configuration.md` and the like) show the same defaults in
the editor's own syntax.

<!-- BEGIN generated: options -->

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `config_file` | false, or a path | the shared file, `~/.config/org-lsp/config.toml` or `config.json` | The shared configuration file, whose options this editor's go on top of: `false` for none, or the path of another file (JSON if it ends in `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, else `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`; `%APPDATA%\org-lsp` on Windows). |
| `org_agenda_files` | string[] | `[]` | Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built from. `~/` is the home directory, and a directory stands for the Org files directly in it. When empty, the agenda is built from the Org files under the workspace folders (hidden directories, `node_modules` and `target` left out), or else from the Org files that are open. (`org-agenda-files`) |
| `org_todo_keywords` | string[] | `["TODO", "\|", "DONE"]` | The TODO sequence, written like Org's `#+TODO:` line. `\|` separates the states that count as done; without it the last keyword is the only done state. `TODO(t)` gives a keyword a key for fast selection, and `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering and leaving it. A file's own `#+TODO:` lines take precedence. (`org-todo-keywords`) |
| `org_log_done` | false, "time", "note" | `false` | Write `CLOSED:` with the time when an entry is done, and with `note` ask for a note too. A file's `#+STARTUP:` words `logdone`, `lognotedone` and `nologdone` change it there; an entry's `LOGGING` property (inherited) says what its state changes log instead: those words, `logrepeat` and its relatives, and keywords with what they log (`WAIT(@/!)`), or `nil` for nothing. (`org-log-done`) |
| `org_log_repeat` | false, "time", "note" | `"time"` | Record when a repeating entry is done and moves to its next date. `#+STARTUP:` `logrepeat`, `lognoterepeat` and `nologrepeat` change it in a file, and the `LOGGING` property in an entry. (`org-log-repeat`) |
| `org_log_into_drawer` | boolean, string | `false` | Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name of a drawer. `#+STARTUP:` `logdrawer` and `nologdrawer` change it in a file; an entry's `LOG_INTO_DRAWER` property (inherited) says which drawer for it (`t` for `LOGBOOK`, `nil` for none). (`org-log-into-drawer`) |
| `org_tags_column` | number | `-77` | Where tags are aligned after a change to a headline. Negative: they end at that column. (`org-tags-column`) |
| `org_use_tag_inheritance` | boolean, string[], string | `true` | Which tags entries inherit from their parents and from `#+FILETAGS:`: `true` for all, `false` for none, a list of tags, or a regexp (Emacs syntax) tags must match. Inherited tags count in tag searches and show in the agenda. (`org-use-tag-inheritance`) |
| `org_tags_exclude_from_inheritance` | string[] | `[]` | Tags never inherited, when `org_use_tag_inheritance` is `true` or a regexp. (`org-tags-exclude-from-inheritance`) |
| `org_use_property_inheritance` | boolean, string[], string | `false` | Which properties an entry without them takes from its parents, the file's property drawer and `#+PROPERTY:` lines, in property searches (`OWNER="alice"`): `true` for all, `false` for none, a list of names, or a regexp (Emacs syntax). `CATEGORY` and `ARCHIVE` are always inherited. (`org-use-property-inheritance`) |
| `org_global_properties` | { [name]: string } | `{  }` | Properties every entry inherits, after those of `#+PROPERTY:` lines: chiefly the allowed values of a property, which setting it offers and `S-<right>` and `S-<left>` go through on its line (`{ "Effort_ALL": "0:10 0:30 1:00 2:00" }`; `:ETC` allows others too). (`org-global-properties`) |
| `org_clock_into_drawer` | boolean, string | `true` | Where `CLOCK:` lines go: `true` for the drawer of `org_log_into_drawer` (`LOGBOOK` when that is not a name), the name of a drawer, or `false` for the entry itself. (`org-clock-into-drawer`) |
| `org_clock_out_remove_zero_time_clocks` | boolean | `false` | Clocking out after less than a minute removes the `CLOCK:` line. (`org-clock-out-remove-zero-time-clocks`) |
| `org_clock_out_when_done` | boolean | `true` | Marking the clocked entry done stops the clock. (`org-clock-out-when-done`) |
| `org_complete_tags_always_offer_all_agenda_tags` | boolean | `false` | Completing a tag offers the tags of every agenda file, after those of the file. Off, only the file's own. Setting tags offers them too. (`org-complete-tags-always-offer-all-agenda-tags`) |
| `org_cite_global_bibliography` | string[] | `[]` | Bibliography files for the citations of every document (`[cite:@key]`), after those its `#+BIBLIOGRAPHY:` lines name: BibTeX (`.bib`, `.bibtex`) or CSL-JSON (`.json`). Their entries are completed after `@`, shown over a key, and where following a key goes; a key none has is marked. (`org-cite-global-bibliography`) |
| `org_tag_alist` | string, string[] | `""` | The tags offered when setting tags in a file without a `#+TAGS:` line, written like one: `"work(w) home(h) { @office(o) @home(m) } [ GTD : Control Persp ]"` gives `work` the key `w`, makes `@office` and `@home` exclusive, and groups `Control` and `Persp` under `GTD`; `\n` starts a new line. A list of these words does as well. Unset, the tags used in the file are offered. (`org-tag-alist`) |
| `org_tag_persistent_alist` | string, string[] | `""` | Tags offered in every file, before those of `#+TAGS:` or `org_tag_alist`, written the same way. (`org-tag-persistent-alist`) |
| `org_group_tags` | boolean | `true` | A group tag (`[ GTD : Control Persp ]`, `{ Context : @home @office }` in `#+TAGS:` or `org_tag_alist`) in the match string of a tags view stands for itself and the tags of its group. (`org-group-tags`) |
| `org_use_fast_tag_selection` | true, false, "auto" | `"auto"` | Pick tags by key, each key turning its tag on or off until RET: `auto` when some tag has a key, `true` always (keys are given to the tags without one), `false` never: the tags are typed instead. (`org-use-fast-tag-selection`) |
| `org_fast_tag_selection_single_key` | boolean | `false` | In fast tag selection, the first key that changes the tags also ends it. (`org-fast-tag-selection-single-key`) |
| `org_tags_sort_function` | "string<", "string>" | the order they were picked in | Sort the tags of a headline when they are set: alphabetically, or the other way. (`org-tags-sort-function`) |
| `org_auto_align_tags` | boolean | `true` | Align the tags of a headline to `org_tags_column` when they are set. (`org-auto-align-tags`) |
| `org_todo_repeat_to_state` | string, true | the first keyword of the sequence | The state a repeating entry returns to when it is done: a keyword, or `true` for the state it had. (`org-todo-repeat-to-state`) |
| `org_priority_highest` | letter, or 0 to 64 | `"A"` | The highest priority. (`org-priority-highest`) |
| `org_priority_lowest` | letter, or 0 to 64 | `"C"` | The lowest priority. (`org-priority-lowest`) |
| `org_priority_default` | letter, or 0 to 64 | `"B"` | The priority of an entry without a cookie. A file's `#+PRIORITIES:` line takes precedence over the three priority options. (`org-priority-default`) |
| `org_priority_start_cycle_with_default` | boolean | `true` | Raising or lowering the priority of an entry that has none starts at the default. (`org-priority-start-cycle-with-default`) |
| `org_time_stamp_rounding_minutes` | number | `5` | The step in which the minutes of a timestamp change (the second number of Org's option). (`org-time-stamp-rounding-minutes`) |
| `org_read_date_prefer_future` | boolean, "time" | `true` | A date typed without a year or month is the next one to come. With `time`, a time that has passed today is tomorrow's. (`org-read-date-prefer-future`) |
| `org_read_date_force_compatible_dates` | boolean | `true` | A typed year is kept within 1970 to 2037. (`org-read-date-force-compatible-dates`) |
| `org_refile_targets` | object[] | `[{ "files": "current", "level": 1 }]` | Where entries can be refiled to. Each entry has `files`: `current` (the entry's own file), `agenda`, a path or a list of paths; and one of `level`, `maxlevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's syntax) or `all` (true), which says which headlines there are targets. (`org-refile-targets`) |
| `org_refile_use_outline_path` | boolean, "file", "full-file-path", "title" | `false` | Name targets by their outline path (`Projects/Alpha/`). The three names start the path with the file, and offer the file itself, for its top level. (`org-refile-use-outline-path`) |
| `org_reverse_note_order` | boolean | `false` | A refiled entry goes first under its target, not last. (`org-reverse-note-order`) |
| `org_archive_location` | string | `"%s_archive::"` | Where archived subtrees go: a file, `::`, and a headline under which they go (they go to the end of the file without one). `%s` is the name of the file the subtree is in; an empty file part is that file. `datetree/` as the headline files them under year, month and day, by when they were closed. An `ARCHIVE` property of the entry or a parent, or a `#+ARCHIVE:` line, says otherwise. (`org-archive-location`) |
| `org_archive_reversed_order` | boolean | `false` | An archived subtree goes first under its archive headline, not last. (`org-archive-reversed-order`) |
| `org_archive_mark_done` | boolean, string | `false` | Mark archived entries done: `true` for the first done keyword, or the keyword to use. (`org-archive-mark-done`) |
| `org_archive_subtree_add_inherited_tags` | boolean, "infile" | `"infile"` | Give an archived subtree the tags it inherited: always, never, or when it stays in its file. (`org-archive-subtree-add-inherited-tags`) |
| `org_archive_save_context_info` | string[] | `["time", "file", "olpath", "category", "todo", "itags"]` | What an archived entry keeps as `ARCHIVE_…` properties: `time`, `file`, `olpath` (its outline path), `category`, `todo`, `itags` (inherited tags), `ltags` (its own tags) and `olid` (its parent's ID). (`org-archive-save-context-info`) |
| `org_archive_sibling_heading` | string | `"Archive"` | The headline of the sibling `org.archive.sibling` archives under. (`org-archive-sibling-heading`) |
| `org_archive_file_header_format` | string, false | `"\nArchived entries from file %s\n\n"` | What a new archive file starts with; `%s` is the path of the file the subtree came from. (`org-archive-file-header-format`) |
| `org_archive_default_command` | "subtree", "sibling", "tag" | `"subtree"` | What `org.archive.default` does: archive the subtree, archive it under its sibling, or tag it. (`org-archive-default-command`) |
| `org_capture_templates` | object[] | `[{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }]` | Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the default, or `plain`), a `target` and a `template` (text, or a list of lines), and may have `prepend`, `empty_lines`, `empty_lines_before`, `empty_lines_after` and `immediate_finish`. The target is a list: `file` and a path; `file+headline`, a path and a headline; or `file+olp`, a path and the headlines of an outline path. An empty path is `org_default_notes_file`, and without a target the entry goes at the end of `org_default_notes_file`. A template with only a key and a description groups the templates whose keys start with its key. (`org-capture-templates`) |
| `org_directory` | string | `"~/org"` | Where the relative paths of capture targets are. (`org-directory`) |
| `org_default_notes_file` | string | `"~/.notes"` | The target of a capture template whose file is empty. (`org-default-notes-file`) |
| `org_link_descriptive` | boolean | `true` | Show a link as its description (or its target, when it has none) and hide the rest: the brackets, and the target of a link with a description. The text is marked `hidden` in the highlighting, and editors that can conceal text conceal it. (`org-link-descriptive`) |
| `org_hide_emphasis_markers` | boolean | `false` | Hide the markers around `*bold*`, `/italic/`, `_underline_`, `+strike-through+`, `=verbatim=` and `~code~`, as for links. (`org-hide-emphasis-markers`) |
| `return_uses_meta_return` | boolean | `false` | RET on the first line of a list item makes the next item, numbered and with a checkbox as M-RET (`org-meta-return`) makes it, the items after it renumbered, as nvim-orgmode's `org_return_uses_meta_return` does. Off, RET is Org's: a new line in the item. `org.return.item` (`<S-CR>`, Shift+Enter, with the vim keys) always does it. |
| `auto_save` | boolean | `true` | Save the files a command changes other than the one it was run in: an entry's file changed from the agenda, the target of a capture or of a refile. The editor saves them once it has made the change. Off, they are left with unsaved changes, as Emacs leaves them. |
| `org_cycle_separator_lines` | number | `2` | With at least this many blank lines after a subtree, the last one stays outside its fold. Negative: all of them stay outside. `0`: none does. (`org-cycle-separator-lines`) |
| `org_adapt_indentation` | boolean \| "headline-data" | `false` | Indent the lines of an entry past its headline's stars: `true` for all of them, `"headline-data"` for the planning line, drawers and clock lines right below the headline. Applies to new lines, to re-indenting (range formatting) and to what commands write. (`org-adapt-indentation`) |
| `org_startup_indented` | boolean | `false` | Show files indented by level without changing them (`org-indent-mode`): the text under a headline lines up with its title, and leading stars are hidden. `#+STARTUP: indent` and `noindent` decide for a file. It turns `org_adapt_indentation` off in such files, unless that is `"headline-data"`. Editors that can show it: Neovim, and VS Code (not on wrapped lines). (`org-startup-indented`) |
| `org_hide_leading_stars` | boolean | `false` | Hide all the stars of a headline but the last; they keep their place. `#+STARTUP: hidestars` and `showstars` decide for a file. (`org-hide-leading-stars`) |
| `org_src_preserve_indentation` | boolean | `false` | Leave the code of source blocks as it is indented when re-indenting. (`org-src-preserve-indentation`) |
| `org_edit_src_content_indentation` | number | `2` | How far code is indented past its source block when re-indenting. (`org-edit-src-content-indentation`) |
| `org_agenda_span` | "day", "week", "fortnight", "month", "year", or a number of days | `"week"` | How much time the agenda shows. (`org-agenda-span`) |
| `org_agenda_start_on_weekday` | 0 to 6 (0 is Sunday), or false | `1` | The day a week or fortnight starts on; `false` starts it today. (`org-agenda-start-on-weekday`) |
| `org_deadline_warning_days` | number | `14` | How many days before a deadline it shows up. (`org-deadline-warning-days`) |
| `org_scheduled_delay_days` | number | `0` | How many days after its date a scheduled entry shows up. (`org-scheduled-delay-days`) |
| `org_scheduled_past_days` | number | `10000` | How many days a scheduled entry that is not done keeps showing up. (`org-scheduled-past-days`) |
| `org_deadline_past_days` | number | `10000` | How many days a missed deadline keeps showing up. (`org-deadline-past-days`) |
| `org_agenda_skip_scheduled_if_done` | boolean | `false` | Leave out scheduled entries that are done. (`org-agenda-skip-scheduled-if-done`) |
| `org_agenda_skip_deadline_if_done` | boolean | `false` | Leave out deadlines of entries that are done. (`org-agenda-skip-deadline-if-done`) |
| `org_agenda_skip_timestamp_if_done` | boolean | `false` | Leave out timestamps of entries that are done. (`org-agenda-skip-timestamp-if-done`) |
| `org_agenda_show_future_repeats` | boolean, "next" | `true` | Show the future dates of repeating entries: all of them, none, or only the next. (`org-agenda-show-future-repeats`) |
| `org_agenda_show_all_dates` | boolean | `true` | Show days without entries. (`org-agenda-show-all-dates`) |
| `org_agenda_tags_column` | number, "auto" | `"auto"` | Where tags are aligned in the agenda: a column (negative: where they end), or `auto` for the right edge of the window. With `auto`, the editors that tell the server how wide the agenda's window is (Neovim, Vim, Sublime Text) have the tags there, and the agenda follows the window as it is resized; the others (VS Code, Zed) have them end at column 80. (`org-agenda-tags-column`) |
| `org_agenda_use_time_grid` | boolean | `true` | Draw the time grid. (`org-agenda-use-time-grid`) |
| `org_agenda_time_grid` | [string[], number[], string, string] | `[["daily", "today", "require-timed"], [800, 1000, 1200, 1400, 1600, 1800, 2000], "......", "----------------"]` | The time grid, as Org writes it: the days that have it (`daily`: a one-day agenda; `weekly`: every day of a longer one; `today`: today; `require-timed`: only days with an entry that has a time; `remove-match`: no grid line at a time an entry already has), the times of its lines (`800` for 8:00), what follows a time that has no end time, and the text of a grid line. (`org-agenda-time-grid`) |
| `org_agenda_current_time_string` | string | `"now - - - - - - - - - - - - - - - - - - - - - - - - -"` | The text of the line for the current time. (`org-agenda-current-time-string`) |
| `org_agenda_show_current_time_in_grid` | boolean | `true` | Show the current time in the grid. (`org-agenda-show-current-time-in-grid`) |
| `org_agenda_weekend_days` | number[] | `[6, 0]` | The days drawn as the weekend (0 is Sunday). (`org-agenda-weekend-days`) |
| `org_agenda_start_with_log_mode` | boolean | `false` | Start the agenda in log mode, which shows what was closed and clocked each day (`l` toggles it). (`org-agenda-start-with-log-mode`) |
| `org_agenda_start_with_clockreport_mode` | boolean | `false` | Start the agenda with the clock report under it (`R` toggles it). (`org-agenda-start-with-clockreport-mode`) |
| `org_agenda_start_with_archives_mode` | "trees" \| "files" \| false | `false` | Start the agenda with entries in archived trees (`trees`; `v a` toggles it), or those and the files the agenda files archive to (`files`; `v A`). (`org-agenda-start-with-archives-mode`) |
| `org_agenda_log_mode_items` | string[] | `["closed", "clock"]` | What log mode shows: `closed` entries, `clock` lines and `state` changes. (`org-agenda-log-mode-items`) |
| `org_agenda_start_with_follow_mode` | boolean | `false` | Start the agenda in follow mode, which shows the entry of the cursor's line in another window as the cursor moves (`F` toggles it). (`org-agenda-start-with-follow-mode`) |
| `org_agenda_start_with_entry_text_mode` | boolean | `false` | Start the agenda with some text of each entry under its line (`E` toggles it). (`org-agenda-start-with-entry-text-mode`) |
| `org_agenda_entry_text_maxlines` | integer | `5` | The most lines of an entry's text `E` shows. (`org-agenda-entry-text-maxlines`) |
| `org_agenda_entry_text_leaders` | string | `"    > "` | What each line of an entry's text starts with in the agenda. (`org-agenda-entry-text-leaders`) |
| `org_agenda_entry_text_exclude_regexps` | string[] | `[]` | Emacs regexps whose matches are left out of the entry text `E` shows. (`org-agenda-entry-text-exclude-regexps`) |
| `org_agenda_block_separator` | string \| false | `"="` | What goes between the blocks of a block agenda: one character repeated across 79 columns, a longer text as it is, or `false` for nothing. In a block's `settings`, the separator above that block. (`org-agenda-block-separator`) |
| `org_agenda_compact_blocks` | boolean | `false` | Leave out the separators between blocks and the header over agenda blocks. (`org-agenda-compact-blocks`) |
| `agenda_hide_empty_blocks` | boolean | `false` | Leave out the blocks of a block agenda that show no entries. Not in Org. |
| `org_agenda_search_view_always_boolean` | boolean | `false` | Read every search string of the search view (`s`) as words that must (`+word`, `word`) or must not (`-word`) occur, `{regexp}`s and `"quoted phrases"`. Off, it is a phrase whose spaces match any whitespace, unless its first word starts with `+`, `-` or `{`. (`org-agenda-search-view-always-boolean`) |
| `org_agenda_search_view_force_full_words` | boolean | `false` | The words of a Boolean search match whole words only, as a search string starting with `:` asks. (`org-agenda-search-view-force-full-words`) |
| `org_agenda_search_view_max_outline_level` | number | `0` | The search view shows headlines down to this level, a match below one counting for the headline above: 0 for every level. (`org-agenda-search-view-max-outline-level`) |
| `org_agenda_text_search_extra_files` | string[] | `[]` | Files the search view (`s`) searches besides the agenda files, after them, as `org_agenda_files` names them. `agenda-archives` first adds the files the agenda files archive to. A file listed twice is searched at its last place. (`org-agenda-text-search-extra-files`) |
| `org_agenda_prefix_format` | { agenda?, todo?, tags?, search? } | `{ "agenda": " %i %-12:c%?-12t% s", "search": " %i %-12:c", "tags": " %i %-12:c", "todo": " %i %-12:c" }` | What comes before the headline on a line of the agenda (`agenda`), the TODO list (`todo`), tags searches (`tags`) and text searches (`search`): `%c` the category, `%t` the time, `%s` the leader (`Scheduled: `), `%e` the effort, `%l` a blank per level, `%T` the last tag, `%b` the headlines above (breadcrumbs), `%i` the icon (none). A width pads (`%-12c` on the right, `%12c` on the left), `.N` cuts, `%?` leaves out an empty field, and a character before the letter follows a value that is not empty (`%-12:c`). `%(…)` (Emacs Lisp) is left empty. (`org-agenda-prefix-format`) |
| `org_agenda_todo_keyword_format` | string | `"%-1s"` | How a TODO keyword is written in the agenda: `%-4s` pads it to four columns, so that the titles line up; an empty format leaves keywords out. (`org-agenda-todo-keyword-format`) |
| `org_agenda_scheduled_leaders` | string[] | `["Scheduled: ", "Sched.%2dx: "]` | The leader of a scheduled entry on its day, and on today when its day has passed (`%d`: the days since). (`org-agenda-scheduled-leaders`) |
| `org_agenda_deadline_leaders` | string[] | `["Deadline:  ", "In %3d d.: ", "%2d d. ago: "]` | The leader of a deadline on its day, on today before it (`%d`: the days left), and on today after it (`%d`: the days since). (`org-agenda-deadline-leaders`) |
| `org_agenda_timerange_leaders` | string[] | `["", "(%d/%d): "]` | The leader of a time range within a day, and of a range over days (`%d`: which day of it, `%d`: how many days). (`org-agenda-timerange-leaders`) |
| `org_agenda_remove_tags` | boolean, "prefix" | `false` | Leave the tags out of the agenda's lines; `prefix` only when the prefix shows a tag (`%T`). (`org-agenda-remove-tags`) |
| `org_stuck_projects` | [string, string[], string[], string] | `["+LEVEL=2/-DONE", ["TODO", "NEXT", "NEXTACTION"], [], ""]` | What the stuck projects view (`#`) lists: the projects the match string finds whose subtree has no entry with one of the TODO keywords (`*` for any not done) or tags (`*` for any), and no match of the regexp. (`org-stuck-projects`) |
| `org_agenda_breadcrumbs_separator` | string | `"->"` | What goes between the headlines of `%b`. (`org-agenda-breadcrumbs-separator`) |
| `org_agenda_format_date` | string | as `Wednesday   7 October 2026`, the week's number added on Mondays | How the agenda writes a day, as `format-time-string` does: `%A %d %B %Y`, `%a %Y-%m-%d`. (`org-agenda-format-date`) |
| `org_agenda_start_day` | string | today | The day the agenda starts from, as the date prompt reads it: `-1d`, `+2d`, `2026-10-01`. (`org-agenda-start-day`) |
| `org_agenda_custom_commands` | object[] | `[]` | Commands of the agenda menu. Each has a `key` and a `description`, and either a `type` (`agenda`, `alltodo`, `todo`, `tags` or `tags-todo`) with a `match` (keywords such as `NEXT\|WAIT` for `todo`, a match string such as `work-home` or `PRIORITY="A"` for `tags`), or `blocks`, a list of such views shown one after the other. `header` replaces a view's header (empty: none), and `settings` holds agenda options for the command or a block (`org_agenda_span`, `org_agenda_start_day`…). A command with only a key and a description groups the commands whose keys start with its key. (`org-agenda-custom-commands`) |
| `virtual_documents` | boolean | from the client's capabilities | Whether the client fetches the agenda with `workspace/textDocumentContent`. A client that announces that capability need not set this. |
| `show_document` | boolean | from the client's capabilities | Whether the client shows the documents the server asks it to show (`window/showDocument`): the agenda, capture buffers, an entry's file. A client that announces that capability need not set this; the Vim plugin does, for the LSP clients that leave the request to it. |
| `message_prompts` | boolean | `false` | Ask what commands ask for (a key, one of a list) with `window/showMessageRequest`, and say what they have to say with `window/showMessage`, for a client that does nothing with a command's result. The Zed extension sets it. |
| `line_highlights` | boolean | `false` | Whether the client highlights lines of the agenda from edge to edge as the server sends them (`org-lsp/lineHighlights`): the entry the clock runs on. As a client announcing `experimental.orgLineHighlights` does: for the Vim plugin and the Sublime Text package. |
| `virtual_indent` | boolean | `false` | Whether the client shows `org-indent-mode`'s prefixes and hidden stars that the server sends (`org-lsp/virtualIndent`), as a client announcing `experimental.orgVirtualIndent` does: for the Vim plugin and the Sublime Text package, whose LSP clients announce nothing of the kind. A client that shows neither gets the prefixes as inlay hints instead. |
| `command_actions` | boolean | `false` | Offer more commands as code actions: the agenda, capture, refile, a TODO state or a priority picked from a list, a new heading or item (M-RET and its neighbours), for a client that cannot run a command from a key. The agenda's commands go through a local port instead: the server writes a script (`command_script`) that runs `SCRIPT COMMAND [ROW COLUMN]` in the agenda, where the cursor is when no row is given (the server follows the cursor through the document highlights the client asks for), for the editor's keys to call. The Zed extension sets it. |
| `command_script` | string | `"~/.local/state/org-lsp/agenda-command"` | Where the server writes the script of `command_actions`. Of several servers with an agenda, the one whose agenda the cursor was last in has it. |

<!-- END generated -->

A document that has `#+TODO:`, `#+SEQ_TODO:` or `#+TYP_TODO:` lines uses
those keywords instead of `org_todo_keywords`, and one with a `#+PRIORITIES:`
line uses its priorities instead of the three `org_priority_…` options; the
agenda sorts each entry by the priorities of its own file.

Without `org_agenda_files`, the agenda is built from the Org files under the
workspace folders (hidden directories, `node_modules` and `target`
excluded). Without workspace folders too, it is built from the Org files
that are open. A file that is open in the editor is always read from the
editor, not from disk.

## Commands

Commands run through `workspace/executeCommand`. The one argument says
where the cursor is, in a document the server knows:

```json
{
  "command": "org.todo.next",
  "arguments": [{ "textDocument": { "uri": "file:///home/me/org/work.org" }, "position": { "line": 3, "character": 0 } }]
}
```

This is `TextDocumentPositionParams`, with one optional field more:
`count`, a number the user typed before the key (Emacs's prefix argument,
Vim's count). The commands that move something by steps take it as the
number of steps; the others ignore it. Commands that open a view work
without the argument.

A command's result may carry `"message"`: what Emacs says in the echo area
when it is done, for the editor to show the same way (Neovim's messages, VS
Code's status bar). `TODO state changed to DONE` (from the agenda, where
the entry is out of sight), `TODO state was already TODO`, `Priority of
current item set to B`, `Priority removed`, `Scheduled to <2026-05-22
Fri>`, `Deadline on <…>`, `Timestamp updated`, `Refile to "Projects" in
file /home/me/org/work.org: done`, `Switched to day view`, `Capture
process aborted and target buffer cleaned up`.

A command that runs as a key is typed returns its edit instead of sending
it: `org.return` answers `{ "edits": [TextEdit…], "cursor": Position }`,
and the editor applies the edits to the document at hand and puts the
cursor there before the next key is handled. (A `workspace/applyEdit`
from the server, and a cursor moved after it, would race with what is
typed next.) `org.meta.return` and its neighbours answer the same way.
With `"apply": true` in the context the server applies the edit itself
and moves the cursor with `window/showDocument`; the code actions of
`command_actions` run them so. With `"below": true`, `org.meta.return`
and its neighbours put the new heading, item or table row below the
cursor's line, as nvim-orgmode's `<Leader><CR>` does: never above it and
splitting nothing, as Emacs does from the end of the line with
`org-M-RET-may-split-line` nil (in a table, `org-table-insert-row` with
its argument).

A command that cannot run answers with error code -32803
(`RequestFailed`) and a message meant for the user, such as
`No agenda is open`. When the command means nothing at the cursor (a
timestamp key off a timestamp, a headline command before the first
headline), the error's `data` is `{ "notApplicable": true }`. An editor
that bound the command to a key with a meaning of its own (see `native`
in [The manifest](#the-manifest)) then lets the key do that instead:
Vim's `<C-a>` on a timestamp changes the date and elsewhere increments a
number.

| Command | Does |
| --- | --- |
| `org.agenda.dispatch` | `org-agenda` (`C-c a`): asks for a view by its key, as Emacs's menu does (input kind `key`; `a` for the agenda, `t` for the TODO list, `m` / `M` for a tags search, `s` / `S` for a search, `#` for the stuck projects (`org-agenda-list-stuck-projects`, as `org_stuck_projects` says what a project is and what makes it not stuck), and the commands of `org_agenda_custom_commands`, of the types `agenda`, `alltodo`, `todo`, `tags`, `tags-todo`, `search` and `stuck`), and opens it. Group tags in a match string (`org_group_tags`) stand for the tags of their group, from the `#+TAGS:` lines of the agenda files (or `org_tag_alist`). A prefix key of custom commands asks again, with the keys so far in the input's `context` (`keys`); `m` and `M` then ask for the match string, `s` and `S` for the search string (input kind `text`, `ask` in the context). |
| `org.return` | `org-return` (RET): in a table, aligns it and goes to the same column of the next row, adding a row when there is none or the next is a rule; on a headline, puts the rest of the title on a new line with the tags kept in their column; in a list item, keeps the rest of the line in the item; else a new line. The new line is indented as `electric-indent-mode` indents in Org (`org-indent-line`), the blanks left at the end of the line removed. Returns `{ "edits", "cursor" }` (see above); in a source block's code, `notApplicable`. |
| `org.meta.return` | `org-meta-return` (M-RET): in a table, splits the field at the cursor, what follows it going in front of the field one row down (`org-table-wrap-region`); in a list, a new item (`org-insert-item`), what follows the cursor in it, before the item when the cursor is before its text, the bullets numbered again; else a new heading (`org-insert-heading`): above a headline from its start, the rest of the title in it from within the title, after the headline elsewhere on it, and a line of text turned into a heading from its start. Empty lines before the new heading as before the one the cursor is in (`org-blank-before-new-entry`'s `auto`). Returns `{ "edits", "cursor" }`. |
| `org.meta.left`, `org.meta.right`, `org.shiftmeta.left`, `org.shiftmeta.right`, `org.meta.up`, `org.meta.down` | Org's meta keys (`M-<left>`, `M-<right>`, `M-S-<left>`, `M-S-<right>`, `M-<up>`, `M-<down>`): on a headline, `org-do-promote` and `org-do-demote` (the headline, its tags aligned again, its text with `org-adapt-indentation`), `org-promote-subtree` and `org-demote-subtree`, `org-move-subtree-up` and `org-move-subtree-down` (past `count` siblings); `M-<right>` on a drawer or a block indents it (`org-indent-drawer`, `org-indent-block`). On a list item, `org-outdent-item` and `org-indent-item` (the item alone; on the first item of a list the whole list moves a column, with `M-S-` only), `org-outdent-item-tree` and `org-indent-item-tree` (with its children), `org-move-item-up` and `org-move-item-down` (with its children, the cursor going along): bullets, numbers and checkboxes put right after, the cookies counted again. In an Org table, `org-table-move-column` left and right, `org-table-delete-column` and `org-table-insert-column`, `org-table-move-row` up and down (see `org.table.next.field`). Returns `{ "edits", "cursor" }`; elsewhere `notApplicable`. Also code actions on a headline, an item or a table, where they would change something. |
| `org.table.next.field`, `org.table.previous.field`, `org.shiftmeta.up`, `org.shiftmeta.down`, `org.table.insert.row.below`, `org.table.hline`, `org.table.hline.above`, `org.table.hline.move`, `org.table.blank.field` | The commands of `org-table.el` in an Org table, as Emacs runs them: `org-table-next-field` (`TAB`: the table aligned, then the next field, over a rule to the next row, a new row after the last field) and `org-table-previous-field` (`S-TAB`), which the editors send for TAB and S-TAB on a table line, where they would cycle folds elsewhere; `org-table-kill-row` (`M-S-<up>`, the row in `copy` for the editor's register or clipboard), `org-table-insert-row` above (`M-S-<down>`) and below (its prefix argument); `org-table-insert-hline` below (`C-c -`) and above, `org-table-hline-and-move` (`C-c RET`), `org-table-blank-field` (`C-c SPC` in `orgtbl-mode`; Org mode 9.8 binds it to no key). Columns count as Org shows them: the hidden parts of links take none. The `#+TBLFM:` lines under the table follow the rows and columns that move, go or come (`org-table-fix-formulas`): `$INVALID` for a deleted column, the formulas of a deleted row or column gone. A field that is a formula (`=…`) or a line marked `#`, which Org evaluates on `TAB`, is refused with a message. Returns `{ "edits", "cursor" }`; out of a table `notApplicable`. |
| `org.cut.subtree`, `org.copy.subtree`, `org.paste.subtree` | `org-cut-subtree` (`C-c C-x C-w`), `org-copy-subtree` (`C-c C-x M-w`) and `org-paste-subtree` (`C-c C-x C-y`). Cutting and copying take the subtree at the cursor and `count - 1` siblings after it; the result has the text in `copy`, for the editor's clipboard or register, and cutting also `{ "edits", "cursor" }`. Pasting reads `clipboard` from the context (the command has `"clipboard": true` in the manifest), or else the server's last cut or copy: before the headline at the cursor, else before the next one, its levels shifted to the deeper of the headlines around it, or to `count`; "The clipboard is not a (set of) tree(s)" otherwise. Before the first headline cutting and copying are `notApplicable`. Code actions too (`commandActions`), with the server's own clipboard. |
| `org.sort` | `org-sort` (`C-c ^`) on entries, as `org-sort-entries` does: the children of the entry at the cursor, or the top-level entries before the first headline. Asks for the sorting type (input kind `key`): `a`lpha, `n`umeric, `p`riority, p`r`operty, todo `o`rder, `t`ime, `s`cheduled, `d`eadline, `c`reated, cloc`k`ing, a capital for the reverse order; for `r` and `R` then the property (kind `choice`, with `answers` in its context). Text compares by character code, as in Emacs's C locale; an entry without the date sorts by now. Returns `{ "edits", "cursor" }`, the cursor on the parent (or the first top-level headline). On a list item, `org-sort-list`. In a table, `org-table-sort-lines`: the rows between the rules around the cursor (or all of them), by the field of the cursor's column, `a`lphabetic, `n`umeric or `t`ime (a timestamp, a duration, a `H:MM`), a capital for the reverse order; before the first column it first asks which (kind `text`, step `column`, then the type with the column in `answers`). |
| `org.sparse.tree` | `org-sparse-tree` (`C-c /`): asks what to show (input kind `key`): `r` a regexp (`org-occur`, Emacs's regexp syntax, case folded), `t` TODO entries, `T` entries with these keywords (`KWD1\|KWD2`), `m` a tags and properties match, `p` a property and its value, `d` deadlines due within `org_deadline_warning_days`, `b` and `a` scheduled and deadline dates before or after a date, `D` between two; then what that needs (kinds `text`, `choice` and `date`, the answers so far in `answers`, a line each). Returns `{ "sparse": { "lines", "highlights" } }` and Org's message: the editor folds everything, then opens the folds around each line, as `org-occur` hides everything but the matches and the headlines above them, and highlights the ranges until the text changes. An editor's folds show the siblings of the headlines above as closed folds, where Org hides them. |
| `org.insert.todo.heading` | `org-insert-todo-heading` (M-S-RET): in a list, a new item with a checkbox, the cookies that count them updated; else a new heading with the keyword of the heading before it at its level, unless that one is done or has none, then the first keyword. Returns `{ "edits", "cursor" }`. |
| `org.insert.heading.after`, `org.insert.todo.heading.after` | `org-insert-heading-respect-content` (C-RET) and `org-insert-todo-heading-respect-content` (C-S-RET): a heading, or a TODO heading, at the level of the one the cursor is in, after its subtree. Returns `{ "edits", "cursor" }`. |
| `org.agenda.list` | Opens the agenda for the current week or day. |
| `org.agenda.todos` | Opens the list of all TODO entries. |
| `org.agenda.later`, `org.agenda.earlier` | Moves the agenda one span forward or backward. |
| `org.agenda.today` | Moves the agenda to today. |
| `org.agenda.query.add`, `org.agenda.query.subtract`, `org.agenda.query.add.regexp`, `org.agenda.query.subtract.regexp` | `org-agenda-manipulate-query` (`[`, `]`, `{`, `}`) in a search view: asks for its search string again (input kind `text`) with ` +`, ` -`, ` +{}` or ` -{}` after it (a space when it was not Boolean), and searches for the answer. In an agenda (where Emacs would show inactive timestamps), a TODO list or a tags view it refuses. |
| `org.agenda.goto.date` | Asks for a date and shows the agenda from that day, over as many days as its span (`org-agenda-goto-date`); a week or a fortnight still starts on the first day of the week. |
| `org.agenda.day`, `.week`, `.fortnight`, `.month`, `.year` | Changes the span, keeping the day under the cursor in view. |
| `org.agenda.redo` | Rebuilds the agenda. |
| `org.agenda.goto` | Shows the entry of the agenda line under the cursor (`org-agenda-switch-to`, RET). |
| `org.agenda.goto.other` | The same in another window (`org-agenda-goto`, TAB): see `window/showDocument` above. |
| `org.agenda.save` | `org-save-all-org-buffers`: returns `{ "save": [uri…] }`, the Org files the editor has open; the editor saves those with changes. |
| `org.agenda.log`, `org.agenda.clockreport` | Turn log mode (`org-agenda-log-mode`: what was closed and clocked each day) or the clock report under the agenda (`org-agenda-clockreport-mode`) on or off. |
| `org.agenda.follow` | `org-agenda-follow-mode` (`F`): toggles follow mode, in which the entry of the cursor's line shows in another window (as `org-agenda-show` shows it, the agenda keeping the focus) at once and as the client reports the cursor on other lines (`org-lsp/agendaCursor`). `org_agenda_start_with_follow_mode` starts with it on. |
| `org.agenda.entry.text` | `org-agenda-entry-text-mode` (`E`, `v E`): shows some text of each entry under its line, as `org-agenda-get-some-entry-text` takes it (up to the next headline, without drawers, planning and clock lines, and what `org_agenda_entry_text_exclude_regexps` matches; the common indentation removed; each line after `org_agenda_entry_text_leaders`), at most `org_agenda_entry_text_maxlines` lines, or as many as a count says (which turns it on). Again turns it off. The lines belong to the entry above them, as commands and hovers there see it. Refused while tags, categories, a regexp or a top headline filter the view; setting a filter turns it off. |
| `org.agenda.filter` | `org-agenda-filter` (`/`): asks for a filter string (input kind `text`, completed from the tags and categories of the view) such as `+work-home<1:00/report/`: `+` or `-` and a category or tag, `<`, `>` or `=` and an effort, a regexp between slashes. A count negates it; starting with `+` keeps the filters there are. The lines that do not pass are hidden until the filters change or another view opens. The result's `message` says what filters there are, as Emacs's mode line does. |
| `org.agenda.filter.tag` | `org-agenda-filter-by-tag` (`\`): asks for a tag by its key in the agenda files' tag tables (input kind `key`, `step` in the context), `TAB` for one typed, `SPC` for any tag, `.` for the line's tags, `\` for none, and `-` / `+` to exclude or keep. A count excludes. Group tags stand for their groups. |
| `org.agenda.filter.category`, `org.agenda.filter.top.headline` | `org-agenda-filter-by-category` (`<`) and `org-agenda-filter-by-top-headline` (`^`): keep the lines of the cursor's line's category or top-level headline, or with a count hide them; again removes the filter. |
| `org.agenda.filter.regexp` | `org-agenda-filter-by-regexp` (`=`): asks for a regexp and keeps the lines matching it (with a count, hides them); again removes the filter. |
| `org.agenda.filter.effort` | `org-agenda-filter-by-effort` (`_`): asks for `<`, `>` or `=` and then an effort by its digit, from `Effort_ALL` of `org_global_properties` (input kind `key`); `_` removes the filter. Entries without an effort count as more than any. |
| `org.agenda.filter.remove` | `org-agenda-filter-remove-all` (`\|`): shows every line again. |
| `org.agenda.archives`, `org.agenda.archives.files` | `org-agenda-archives-mode` (`v a`, `v A`): show the entries of archived trees too, which the agenda leaves out otherwise; with `.files`, also the files the agenda files archive to (their `#+ARCHIVE:` or `org_archive_location`, and their `ARCHIVE` properties) that exist. Each turns its mode off when it is on. |
| `org.clock.in`, `org.clock.out`, `org.clock.cancel` | `org-clock-in`, `org-clock-out`, `org-clock-cancel`, on the entry at the cursor or of an agenda line. Clocking in stops a clock that runs elsewhere, in the same edit; the clock is a `CLOCK:` line in the `LOGBOOK` drawer, as `org_clock_into_drawer` says. |
| `org.clock.goto` | `org-clock-goto`: shows the entry the clock runs on, or the one clocked last. |
| `org.todo` | `org-todo`: when the keywords have keys (`TODO(t)`), asks for one (`org-fast-todo-selection`) and sets the state it selects, a space clearing it; without keys, the next state. In an Org file or on an agenda line. |
| `org.tags.set` | `org-set-tags-command` (`C-c C-q`; `org-agenda-set-tags` on an agenda line): the tags of the headline, from the file's `#+TAGS:` lines, or `org_tag_alist`, with `org_tag_persistent_alist` (or else the tags used in the file, and with `org_complete_tags_always_offer_all_agenda_tags` the agenda files'). When a tag has a key (`work(w)`), or `org_use_fast_tag_selection` is `true`, they are picked as `org-fast-tag-selection` picks them: an input of kind `key` whose choices are the tags with their keys (`(on)` for those set, `(inherited)`), RET to accept, SPC to clear them all, TAB to type one, `!` to turn exclusive groups (`{ @office @home }`) off and on; each key turns its tag on or off, the others of its group off, and asks again with the tags so far in the prompt and in the input's `context` (`answers`). Otherwise the tags are typed (kind `text`, the current ones as `:a:b:`). The tags are written as `org-set-tags` writes them, sorted by `org_tags_sort_function` and aligned to `org_tags_column`. |
| `org.todo.next`, `org.todo.previous` | Changes the TODO state of the entry at the cursor, in an Org file or on an agenda line. |
| `org.priority.up`, `org.priority.down` | Raises or lowers the priority of the entry at the cursor, in an Org file or on an agenda line. |
| `org.capture` | `org-capture`: asks for a template's key (input kind `key`, groups one key at a time), then for what its `%^{…}` and `%^t` escapes want (kinds `text` and `date`), with `context` carrying the keys and answers so far. It then writes the filled template, as it will stand in its target, to a capture buffer (a file named `CAPTURE-<target>` in a directory of the server's own) and shows it with the cursor where `%?` was. |
| `org.capture.finish` | `org-capture-finalize` (`C-c C-c` in the capture buffer): the buffer's text goes into the target file, and the result `{ "close": uri }` asks the editor to close the buffer (saving it first). A target that does not exist yet is written by the server. The argument may carry the buffer's text as `text` (also for `org.capture.refile`, and for a note buffer): a client that tells the server of changes only a while after they are made (coc.nvim) sends it, and it is used when the command runs in that buffer. |
| `org.capture.abort` | `org-capture-kill` (`C-c C-k`): closes the capture buffer; nothing changes. |
| `org.capture.refile` | `org-capture-refile` (`C-c C-w`): the entry goes to its target and from there under a refile target picked as for `org.refile`. |
| `org.refile` | `org-refile` (`org-agenda-refile` on an agenda line): offers the targets (an input of kind `choice`), named as Emacs names them (`Projects`, `Home (other.org)`, `Projects/Alpha/`), and moves the subtree under the one picked, as its last child, promoted or demoted to fit, into another file if need be (one `workspace/applyEdit` over both). |
| `org.archive.subtree` | `org-archive-subtree` (`org-agenda-archive` on an agenda line): moves the subtree to its archive location: the `ARCHIVE` property of the entry or a parent, `#+PROPERTY: ARCHIVE`, the first `#+ARCHIVE:` line, or `org_archive_location`. Under the location's headline (made when missing, `datetree/` for year, month and day headlines by the entry's `CLOSED:` date or today), or at the end of the file; with the properties `org_archive_save_context_info` names, inherited tags as `org_archive_subtree_add_inherited_tags` says, done as `org_archive_mark_done` says, and the parent's statistics cookies recounted. A new archive file starts with `org_archive_file_header_format`. An archive file no editor has open is written by the server, as Emacs saves it; one that is open gets an edit. |
| `org.archive.sibling` | `org-archive-to-archive-sibling`: moves the subtree under its sibling `org_archive_sibling_heading` tagged `:ARCHIVE:`, made at the end of the parent's subtree when there is none, with an `ARCHIVE_TIME` property; the parent's TODO statistics are recounted. |
| `org.archive.tag` | `org-toggle-archive-tag`: sets or removes the `ARCHIVE` tag of the headline. Archived subtrees are left out of the agenda. |
| `org.archive.default`, `org.archive.confirm` | `org-archive-subtree-default`: one of the three, as `org_archive_default_command` says; `org.archive.confirm` (`a` in the agenda) asks first (input kind `key`, `y` or `n`). |
| `org.archive.done`, `org.archive.tag.done` | `org-archive-subtree` and `org-toggle-archive-tag` with `C-u`: each subtree without open TODO items (the children of the headline at the cursor, or the top-level ones before the first headline) is archived or tagged when the answer to its question is yes. One question at a time, input kind `key` (`y` or `n`), with the answers so far in the input's `context` (`answers`, a string of `y` and `n`). |
| `org.priority` | Asks for a priority (`Priority A-C, SPC to remove`, an input of kind `priority`) and sets it, or removes it for a space; in an Org file or on an agenda line. |
| `org.table.recalculate`, `org.table.recalculate.line`, `org.table.iterate` | `org-table-recalculate` (`C-u C-c *`, `C-c *`) and `org-table-iterate` (`C-u C-u C-c *`): apply the formulas of the `#+TBLFM:` line under the table at the cursor, to every line, to the cursor's line (with every field formula), or again until the table no longer changes (at most ten times; the result's `message` says how many it took). On a `#+TBLFM:` line, that line's formulas apply to every line, as `C-c C-c` there applies them (`org.checkbox.toggle` does that there too). Formulas are read as Org reads them and evaluated as Calc evaluates them, results written as Calc writes them; Emacs Lisp formulas, remote references, units, `;F`, `;S` and `;e` are refused with a message. `org.table.recalculate` is also a code action in a table whose formulas would change it. |
| `org.timestamp.up`, `org.timestamp.down` | Changes the part of the timestamp the cursor is on. Org files only. |
| `org.date.later`, `org.date.earlier` | Moves a date by days: in an Org file the timestamp at the cursor, on an agenda line the date the line is listed for. |
| `org.shift.up`, `.down`, `.left`, `.right` | Org's shifted cursor keys in an Org file: on a timestamp they change it, on a headline they change its priority (up, down) or TODO state (left, right). On a list item, left and right cycle the bullets of its list (`org-cycle-list-bullet`: `-`, `+`, `*` when indented, `1.`, `1)`, no numbers for a description list), up and down go to the previous or next item (`org-previous-item`, `org-next-item`); on a property line, left and right go to the previous or next of its allowed values (`org-property-next-allowed-value`: those of `NAME_ALL`, or `[ ]` and `[X]`); these return `{ "edits", "cursor" }`. |
| `org.property.set` | `org-set-property` (`C-c C-x p`; `org-agenda-set-property` on an agenda line): asks for the name (kind `text`, completing the file's property names and those Org uses, the one on the cursor's property line or the one set last by default), then the value: one of its allowed values (kind `choice`), which `NAME_ALL` gives (inherited, from `#+PROPERTY:` lines and `org_global_properties` too; `:ETC` allows others, kind `text` then), or else any (kind `text`, completing the values the file has, the entry's own by default). The steps come back with `step` in the context (`name`, then `value` with the name in `answers`). The value goes in the entry's property drawer, made when there is none (before the first headline, the file's), aligned as `org-indent-line` aligns it; `TODO` and `PRIORITY` change the keyword and the priority instead. `SCHEDULED`, `DEADLINE` and Org's other special properties are refused. |
| `org.property.delete`, `org.property.delete.globally` | `org-delete-property`, which asks which of the entry's properties (kind `choice`), and `org-delete-property-globally`, which asks for a name (kind `text`) and takes it out of every entry of the file; a drawer left empty goes too. |
| `org.effort.set`, `org.effort.increase` | `org-set-effort` (`C-c C-x e`; `e` in the agenda): one of `Effort_ALL` (kind `choice`) or any duration Org reads (`1:30`, `2h`, `1d 3:00`); `org-inc-effort` (`C-c C-x E`): the allowed value after the entry's. |
| `org.ordered.toggle` | `org-toggle-ordered-property` (`C-c C-x o`): sets `ORDERED` to `t`, or takes it out. |
| `org.cite.insert`, `org.cite.insert.prefix` | `org-cite-insert` (`C-c C-x @`) with Org's `basic` processor, and with the prefix argument (`C-u C-c C-x @`). Elsewhere than on a citation: keys one at a time (kind `choice`, step `keys`, the keys picked so far in `answers` separated by `;`, each choice labelled with its line of Org's completion table, and `(done)` once one is picked), then with the prefix argument a style (step `new-style`; `(default)` or one of `oc-basic`'s styles); the citation goes in at the cursor with the keys last picked first, as Org puts them, and the cursor after it. On a citation's style, another style (step `style`); on its global prefix or suffix, a key (step `key`) before the first reference or after the last; on a reference, a key before it (at its prefix or the `@`), after it (at its suffix), or in place of its key. With the prefix argument on a citation, it goes with the blanks around it as `org-cite-delete-citation` takes them, or on a reference that reference. Refused where Org allows no citation (`Cannot insert a citation here`: a keyword line, a block other than a verse block, a drawer's lines, a link, a timestamp, a headline's stars or tags, …) and without a bibliography (`No bibliography set`). |
| `org.checkbox.toggle` | `C-c C-c` in a property drawer is `org-property-action`: asks for `s` (set), `d` (delete), `D` (delete globally) or `c` (compute, refused: org-lsp has no column view), as kind `key` with `step` `action`, then goes on as those commands. `C-c C-c` on a list item (`org-ctrl-c-ctrl-c`): toggles its checkbox if it has one, and puts the list right: indentation, bullets, numbers, the boxes of parents (`ORDERED` entries unchecking what follows an unchecked box). At the start of a list's first line only that item's box is toggled; on a statistics cookie the cookies are counted again. On a headline, `org-toggle-checkbox`: the boxes of the items below it. On a `#+TBLFM:` line, `org.table.recalculate`; in a table, `org-table-align` (at the very start of the table, where Emacs finds the table itself, `org.table.recalculate`). The cookies that count checkboxes follow. |
| `org.schedule`, `org.deadline` | Asks for a date and sets the entry's `SCHEDULED:` or `DEADLINE:`, in an Org file or on an agenda line (`org-agenda-schedule`, `org-agenda-deadline`). |
| `org.timestamp.insert`, `org.timestamp.insert.inactive` | Asks for a date and inserts a timestamp at the cursor, or changes the one there. Org files only. |
| `org.date.preview` | Not for users: what an answer to a date prompt means; see below. Not in the manifest. |
| `org.calendar.day.later`, `.day.earlier`, `.week.later`, `.week.earlier`, `.month.later`, `.month.earlier`, `.year.later`, `.year.earlier` | In the date prompt: move the selected day. |
| `org.calendar.scroll.later`, `org.calendar.scroll.earlier` | In the date prompt: show the next or previous months. |
| `org.calendar.today`, `org.calendar.select`, `org.calendar.type`, `org.calendar.time`, `org.calendar.cancel` | In the date prompt: go to today; take the day at the cursor; type the answer; pick a time, or go back to the day; give up. See [The date prompt document](#the-date-prompt-document). |

**TODO state changes** do what Org's `org-todo` does. The cycle runs
through "no keyword" between the last and first states. The tags of the
headline return to their column, and statistics cookies (`[1/3]`, `[33%]`)
in the parent headline are recounted. `CLOSED:` and state notes are
written as `org_log_done` and the keywords ask; where Org would prompt for the
text of a note, the note is written without text. An entry with a repeater
that is marked done moves to its next date and is open again, with
`LAST_REPEAT` and a state note recorded.

The entry is the innermost headline whose subtree contains the cursor, so
the cursor may be anywhere in the entry's body.

**Priorities** change as `org-priority-up` and `-down` do them, within
the file's priorities. Past the highest or lowest the cookie is removed;
the same key pressed again goes on from the other end. "Pressed again"
means the same command on the same entry with no other command in
between; the server cannot see the cursor move, as Emacs can.

**Timestamps** change as `org-timestamp-up` and `-down` do them
(`S-<up>`, `S-<down>` in Emacs): the year, month, day, hour or minute
the cursor is on, the end of a time range together with its start, a
repeater or warning period, or, on a bracket, active against inactive.
Minutes move in steps of `org_time_stamp_rounding_minutes` unless a count is
given. On a `CLOCK:` line the duration is recomputed. In a file,
`org.date.later` and `-earlier` are `org-timestamp-up-day` and
`-down-day`. On an agenda line they are `org-agenda-date-later` and
`-earlier`: the first step forward takes a date that has passed to
today, and a date range moves as a whole.

**Commands that ask for something.** In the manifest such a command has
`"input": { "kind", "prompt" }`. Run without an answer, it changes
nothing and returns what to ask:

```json
{ "input": { "kind": "date", "prompt": "Schedule", "value": "10:00" } }
```

`value` is the text the prompt starts with, as Emacs fills it in: the
time of the date being changed (`10:00+1:30` for a range), or nothing.
The editor asks with its own input box and runs the command again with
the answer as `input` in the argument:

```json
{ "textDocument": { "uri": "…" }, "position": { … }, "input": "fri 10:00" }
```

An input may say more:

- `"choices": [{ "key": "t", "label": "TODO" }, …]` with kind `key`: the
  answer is one key, as Emacs reads it at its fast selection prompts. The
  prompt is a title (`Select a TODO state`); the editor shows the choices,
  as Emacs shows them in a window of their own. It can list them and read
  a single key press (Neovim) or offer them as a list (VS Code). A space
  is shown as `SPC`.
- `"completions": ["Owner", …]` with kind `text`: what may be typed,
  offered as it is typed (Emacs's `completing-read`); anything else may
  be typed too. An empty answer stands for the default the prompt shows
  in brackets (`Property [Owner]`).
- `"context": { … }`: fields to send back with the answer. A TODO state
  change that records a note (`@`, or `org-log-done` set to `note`) is
  not made until the note's text comes: it asks with kind `note`
  (`Note for state change from "TODO" to "WAIT"`) and `"context": {
  "state": "WAIT" }`, and the answer then sets that state and writes the
  note under the heading line, as `org-store-log-note` does. A client
  that announces `experimental.orgDatePrompt` (one of this project's
  integrations) gets a note buffer instead, as Emacs's `*Org Note*`: a
  file named like a capture buffer (`CAPTURE-Note.org`), shown with
  `window/showDocument`, which starts with `# Insert note for state
  change from "TODO" to "WAIT".` and has the cursor on its third line.
  `org.capture.finish` there makes the change with the text below the
  comment lines as the note, and `org.capture.abort` makes it without
  any (Emacs's `C-c C-k`); both return `{ "close": uri }`.

An answer may lead to another question; the editor asks again until the
command asks for nothing more.

A `date` answer is read as Org's date prompt reads it (`org-read-date`):
`+2d`, `fri`, `2/15`, `feb 15 3pm`, `w12`, `10:00-11:30`, `++1w`, and so
on, against the date being changed or else today. While it is typed, an
editor can show what it means with `org.date.preview`, whose argument is
the same plus `"command"`, the command being answered; it returns
`{ "timestamp": "<2026-05-22 Fri 10:00>" }`, or fails while the answer
means nothing.

**Scheduling** works as `org-schedule` and `org-deadline` do: the
timestamp goes on the planning line under the headline, which is made
when there is none; a `CLOSED:` timestamp is taken out; the repeater of
the old date stays. **Timestamps** work as `org-timestamp` does: on a
timestamp the command changes it (keeping its repeater), elsewhere it
inserts one (`org.timestamp.change` only changes one, active or inactive
as it is, and refuses elsewhere); right after inserting one, the same command adds the end of
a date range (`<…>--<…>`). After a timestamp is inserted, the server
moves the cursor after it with `window/showDocument`, as Emacs leaves
it. Changes are not logged (`org-log-reschedule` and the like) yet.

**Code actions.** The commands that apply at the cursor are also offered
as code actions, with the command name as the action's kind. In an Org
file the actions carry their edit (versioned when the client supports
`documentChanges`), except clocking (`org.clock.in`, `org.clock.out`,
`org.clock.cancel`), which may change another file too and carries its
command; in the agenda they carry the command to run. A client
can therefore also bind a key by requesting code actions with
`context.only` set to a kind, which matches hierarchically: `org` selects
every action.

**Edits to files that are not open.** A command run from the agenda edits
the entry's file with `workspace/applyEdit`. Editors apply such an edit to
a buffer and leave saving to the user, as Emacs does. The agenda follows
the edit at once and again when the file is saved.

**Reporting edits at once.** The server builds on the text it was last
told about. A key held down sends the next command before an editor that
batches its `didChange` notifications (Neovim waits 150 ms) has reported
the previous edit, and that press is lost. An integration should send the
change as soon as it has applied a `workspace/applyEdit`.

## The manifest

What an editor integration has to know beyond LSP's own fields, the server
announces itself, so that integrations carry no tables about Org. The
manifest is `capabilities.experimental.org` in the `initialize` result, and
`org-lsp manifest` prints it for integrations that are generated ahead of
time.

```json
{
  "commands": [
    {
      "command": "org.todo.next",
      "title": "Cycle TODO state forward",
      "edits": true,
      "keys": [
        { "key": "C-c C-t", "in": "org", "preset": "emacs" },
        { "key": "cit", "in": "org", "preset": "vim" },
        { "key": "t", "in": "agenda", "preset": "vim" }
      ]
    }
  ],
  "agenda": {
    "scheme": "org-agenda",
    "uri": "org-agenda:/Org%20Agenda",
    "fileName": "Agenda.org-agenda",
    "languageId": "org-agenda"
  },
  "datePrompt": { "scheme": "org-agenda", "uri": "org-agenda:/Org%20Date", "languageId": "org-date" },
  "capture": { "prefix": "CAPTURE-" },
  "tokenTypes": [{ "id": "headline", "description": "The title text of a headline", "standard": false }],
  "tokenModifiers": [{ "id": "level1", "description": "Headline level 1" }],
  "highlights": [
    { "token": "headline.level1", "vim": ["@org.headline.level1", "Title"], "textmate": ["markup.heading.1.org"] }
  ]
}
```

- **`commands`**: every command with its title and default keys. `edits`
  marks commands that change an Org file, for editors that can repeat the
  last change. A key applies `in` Org files (`org`), the agenda (`agenda`)
  or everywhere (`global`). There are two presets: `emacs` has Org's own
  keys in Emacs notation (`C-c C-t`, `RET`, `v d`), `vim` has
  nvim-orgmode's defaults in Vim notation (`cit`, `<CR>`, `vd`). A key
  marked `"native": true` keeps its own meaning where the command does
  not apply, such as `<C-a>` or `S-<up>`; see `notApplicable` under
  [Commands](#commands). A `vim` key with `"mode": "insert"` is typed in
  insert mode (`<CR>` of `org.return`); the others in normal mode. One with
  `"mode": "append"` (`<Leader><CR>` of `org.meta.return`) runs with
  `"below": true` in the context, and leaves the editor in insert mode
  where the command put the cursor.
- **`agenda`**: how the agenda document is named; see below.
- **`tokenTypes`**, **`tokenModifiers`**: the semantic token legend with
  descriptions. `standard` types are LSP's own.
- **`highlights`**: how tokens should look by default. `token` is a type, a
  `type.modifier`, or `*.modifier` for any type. `vim` lists highlight
  groups, the first defined one to be used: the group nvim-orgmode
  colorschemes define, then a standard group. `textmate` lists scopes for
  editors whose themes match scopes, most specific first.

## The agenda document

The agenda is one document that the server owns, as it is one buffer in
Emacs: a new view replaces the old one. Its language identifier is
`org-agenda`. The server remembers which entry each line stands for, so
semantic tokens, go-to-definition, code actions and commands work on it
like on any document.

A client that announces `workspace.textDocumentContent` (LSP 3.18) is
shown `org-agenda:/Org%20Agenda` and fetches its text with
`workspace/textDocumentContent`. Other clients are shown a file,
`Agenda.org-agenda` in a temporary directory, which the server rewrites
and removes when it stops.

Opening a view goes like this:

1. The client runs `org.agenda.list`.
2. The server sends `window/showDocument` with the agenda's URI and, as
   the selection, the line of today's date.
3. The client opens the document, which makes it fetch the text.

When the view changes while the client has it open:

1. The server sends `workspace/textDocumentContent/refresh`.
2. The client fetches the text again and, since the document is open,
   reports the change with `textDocument/didChange`.
3. If the cursor is to move (the view went to another week, say), the
   server sends `window/showDocument` with the new line only now, when the
   client's copy matches. Sent earlier, the line number would be applied to
   the old text.

For step 3 the client must sync the agenda document like any other:
`didOpen` when it shows it, `didChange` when it replaces the text,
`didClose` when it closes it. After `didClose` the server stops rebuilding
the view.

**The cursor follows an entry.** A change of priority or date can move an
entry to another line, or another day. The server then moves the cursor
to the entry's new line the same way, once the client shows the new text,
so that the next press of the key reaches the same entry. Emacs instead
leaves the line where it is until the agenda is rebuilt. A TODO change
leaves the cursor where it is, as in Emacs.

**Lines of a view the client still shows.** A key pressed before the
client has fetched the new text names a line of the old one. The server
keeps the old view until the client's copy has changed, and resolves the
line against it. Likewise, while the cursor is on its way to an entry's
new line, a command from the line it is leaving is meant for the entry.

## The date prompt document

Emacs shows a calendar while it asks for a date, and the server does the
same for a client that announces `experimental.orgDatePrompt: true` in
its capabilities (and supports `workspace/textDocumentContent` and
`window/showDocument`). The prompt is a second document of the agenda's
scheme, `org-agenda:/Org%20Date` with the language `org-date`, as the
manifest's `datePrompt` names it:

```text
          April 2026                May 2026                June 2026
     Su Mo Tu We Th Fr Sa     Su Mo Tu We Th Fr Sa     Su Mo Tu We Th Fr Sa
               1  2  3  4                     1  2         1  2  3  4  5  6
      …

SCHEDULED Date+time [2026-05-20]: 10:00    => <2026-05-20 Wed 10:00>
```

The three months are drawn as Emacs's calendar draws them; the last line
is Emacs's minibuffer: the prompt, what was typed, and what that means.
The selected day has the `calendarSelected` token, and the server puts
the cursor on it.

1. A command that asks for a date (`org.schedule` …) opens the prompt with
   `window/showDocument` and returns nothing. The client shows it as it
   likes (Neovim: a window at the bottom, VS Code: beside the file).
2. Its keys are the commands' keys `in` the `calendar` scope: they move the
   selected day (from the day under the cursor), take the day at the
   cursor (`org.calendar.select`: what was typed and that day), ask for the
   answer (`org.calendar.type`, which returns `{ "input": … }` like any
   command; that input box is the minibuffer), or give up
   (`org.calendar.cancel`). While the answer is typed, `org.date.preview`
   also shows it in the minibuffer line. `org.calendar.time` picks a
   time, which Emacs's calendar cannot: a `Time: 10:00` line appears above
   the minibuffer, and until `org.calendar.time` again, the day commands
   choose the hour (`.day.earlier`) or the minutes (`.day.later`) and the
   week commands move it (`.week.earlier` up, `.week.later` down; the
   minutes by `org_time_stamp_rounding_minutes`). The time takes the place
   of a typed one, and stays when the keys go back to days.
3. Answered or picked, the command runs with the answer and the day moved
   to (if it moved), as Emacs reads its prompt; the result is
   `{ "close": "org-agenda:/Org%20Date" }`, and the client closes that
   document. Cancelling returns the same.

When the selected day moves, the server sends
`workspace/textDocumentContent/refresh`, and once the client shows the new
text, `window/showDocument` with the cursor on the day, as for the agenda.
Other clients keep the plain input box.

## Semantic tokens

Tokens never overlap and never span lines. Nested markup is flattened into
modifiers: in `*bold /both/*` the inner text carries `bold` and `italic`.
Emphasis markers are part of the token they delimit. The manifest has the
full legend with descriptions; this is an overview.

### In Org files

| Type | Covers |
| --- | --- |
| `headline` | Headline title text. Carries a `levelN` modifier. |
| `stars` | The stars of a headline. Carries a `levelN` modifier. |
| `todoKeyword` | A TODO keyword; `done` when it is a done state. |
| `priority` | A priority cookie, `[#A]`. |
| `tag` | The tags of a headline, `:a:b:`, as one token. |
| `planning` | `SCHEDULED:`, `DEADLINE:`, `CLOSED:`, `CLOCK:` and a headline's `COMMENT`. |
| `timestamp` | A timestamp or timestamp range; `inactive` for `[…]`. |
| `drawer` | The `:NAME:` and `:END:` lines of a drawer. |
| `property` | The `:KEY:` of a property. |
| `keyword` | The `#+KEY:` of a keyword line. |
| `block` | The `#+BEGIN…` line, with its parameters, and the `#+END…` line. |
| `comment` | Comment lines and the contents of comment blocks. |
| `bullet` | A list bullet or counter. |
| `checkbox` | A checkbox; `checked` or `partial`. |
| `link` | A link of any form, and citations. |
| `code` | `~code~`, and inline source blocks but for their code (`src_python{…}`), which is left to the editor to color in its language. |
| `verbatim` | `=verbatim=` and fixed-width lines. |
| `text` | Plain text that carries emphasis modifiers. |
| `tableDelimiter` | Table borders and rule lines. |
| `footnote` | Footnote references and definition labels. |
| `cookie` | A statistics cookie, `[1/3]`. |
| `target` | `<<target>>` and `<<<radio target>>>`. |
| `latex` | LaTeX fragments and environments. |
| `entity` | An entity such as `\alpha`. |
| `macro` | `{{{macro}}}`. |

Plain text gets no token, and neither do the contents of source and
example blocks.

### In the agenda

Keywords, priorities, tags and links inside an agenda line have the types
above. The rest of a line has one of these, which follow Org's agenda
faces:

| Type | Covers | Modifiers |
| --- | --- | --- |
| `agendaStructure` | The title line of a view. | |
| `agendaDate` | A date line. | `today`, `weekend` |
| `agendaScheduled` | A scheduled entry. | `today`; `past` when scheduled before today |
| `agendaDeadline` | An entry with a deadline. | `imminent` when due or overdue, `distant` when far away |
| `agendaEvent` | An entry with a plain timestamp. | `range` inside a date range |
| `agendaDone` | An entry that is done. | |
| `agendaTimeGrid` | A line of the time grid. | `current` for the current time |

### In the date prompt

| Type | Covers | Modifiers |
| --- | --- | --- |
| `calendarHeader` | The month and year above a month. | |
| `calendarWeekday` | The name of a weekday above its column. | `weekend` |
| `calendarSelected` | The selected day. | |
| `calendarPrompt` | The prompt in the minibuffer line. | |
| `timestamp` | What the answer means. | |

### Modifiers

| Modifier | On |
| --- | --- |
| `level1` … `level8` | `headline` and `stars`. Levels above eight wrap around. |
| `done` | `todoKeyword` |
| `inactive` | `timestamp` |
| `checked`, `partial` | `checkbox` |
| `bold`, `italic`, `underline`, `strikethrough` | `text` and `headline` |
| `today`, `weekend`, `past`, `imminent`, `distant`, `range`, `current` | the agenda types, as listed above |
| `archived` | A headline with `:ARCHIVE:` in it, all of it but the stars (`org-archived`, a shadow over its other colors). |
| `hidden` | Text Org hides: the brackets of a link, and its target when it has a description (`org_link_descriptive`); emphasis markers (`org_hide_emphasis_markers`). It comes as tokens of its own, so an editor that can conceal text conceals the tokens with this modifier: Neovim with extmarks in the semantic tokens' namespace, VS Code with decorations. |

## Writing an editor integration

An integration should stay free of Org logic. Its whole job:

1. Start `org-lsp` for `org` files, and on demand for the agenda.
2. Apply the manifest: link the tokens to the editor's highlight names and
   bind the keys of its preset to the commands.
3. Run commands with the cursor position as their argument.
4. Show the agenda document and keep it in sync, as described above;
   likewise the date prompt document, if it announces it, and close it
   when a command returns `close`.
5. Open and close folds; the server only says where they are.

`lua/org-lsp/` does this at run time. `editors/vscode/` cannot, because
VS Code wants commands, keys and token colors declared in `package.json`:
there `scripts/generate.mjs` writes them from `org-lsp manifest`.

Without an integration, an editor still gets what its LSP client supports
on its own: highlighting, folding, the outline, and TODO changes as code
actions. Commands need the cursor position as an argument, which generic
clients do not send, and the agenda needs `window/showDocument`.
