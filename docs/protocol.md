# Protocol

org-lsp uses standard LSP. This page says what it offers and lists the
names an editor integration deals with: options, commands, the manifest,
the agenda document and the semantic tokens.

## Capabilities

| Request | Notes |
| --- | --- |
| `textDocument/didOpen`, `didChange`, `didSave`, `didClose` | Incremental sync. A save rebuilds an open agenda. |
| `textDocument/semanticTokens/full`, `/range` | For Org files and the agenda. See [Semantic tokens](#semantic-tokens). No deltas. |
| `textDocument/foldingRange` | Subtrees, drawers, blocks, LaTeX environments and multi-line list items. Whole lines only. A subtree takes the blank lines before the next headline along, as Emacs hides them; see `cycleSeparatorLines`. |
| `textDocument/documentSymbol` | The outline of headlines. Hierarchical when the client supports it, flat otherwise. |
| `textDocument/codeAction` | The commands that apply at the cursor. See [Commands](#commands). |
| `workspace/executeCommand` | Every command. See [Commands](#commands). |
| `textDocument/definition` | In the agenda: from a line to its entry. |
| `textDocument/completion` | In a headline: TODO keywords for the first word after the stars, and the file's tags after a `:` in the headline's tags (`pcomplete`). `:` triggers it. |
| `workspace/textDocumentContent` | The text of the agenda. See [The agenda document](#the-agenda-document). |

The server sends these requests to the client:

| Request | When |
| --- | --- |
| `workspace/applyEdit` | A command changes an Org file. |
| `window/showDocument` | To show the agenda, a capture buffer, or the file of an agenda entry. The editors open the agenda and capture buffers in a window of their own (a split by default), and an entry's file where the agenda was. |
| `workspace/textDocumentContent/refresh` | The agenda has new text. |

And this notification:

| Notification | When |
| --- | --- |
| `org-lsp/save` | `{ "uris": [uri…] }`: with `autoSave`, the files a command changed other than the one it was run in (an entry's file changed from the agenda, the target of a capture or a refile), once the editor has applied the change. The editor saves those with changes. |

**Position encoding.** The server picks UTF-8 if the client offers it, then
UTF-32, and falls back to UTF-16.

**Line endings.** `\n`, `\r\n` and a lone `\r` all end a line for positions.

## Options

Passed as `initializationOptions`, on top of the shared configuration file
the server reads itself (`~/.config/org-lsp/config.toml` or `config.json`; see
`docs/configuration.md`): an object such as `agenda` is merged option by
option, anything else replaces the file's. The names are Org's variable names in
camel case; the agenda's lose their `org-agenda-` prefix and live under
`agenda` (`agenda.span` below is `{ "agenda": { "span": … } }`). Defaults
are Org's. Unknown options are ignored. `org-lsp settings` prints this
table as JSON, and the table below is written from it by
`scripts/generate-docs.mjs`; the configuration pages of each editor
(`docs/neovim/configuration.md` and the like) show the same defaults in
the editor's own syntax.

<!-- BEGIN generated: options -->

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `configFile` | false, or a path | the shared file, `~/.config/org-lsp/config.toml` or `config.json` | The shared configuration file, whose options this editor's go on top of: `false` for none, or the path of another file (JSON if it ends in `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, else `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`; `%APPDATA%\org-lsp` on Windows). |
| `agendaFiles` | string[] | `[]` | Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built from. `~/` is the home directory, and a directory stands for the Org files directly in it. When empty, the agenda is built from the Org files under the workspace folders (hidden directories, `node_modules` and `target` left out), or else from the Org files that are open. (`org-agenda-files`) |
| `todoKeywords` | string[] | `["TODO", "\|", "DONE"]` | The TODO sequence, written like Org's `#+TODO:` line. `\|` separates the states that count as done; without it the last keyword is the only done state. `TODO(t)` gives a keyword a key for fast selection, and `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering and leaving it. A file's own `#+TODO:` lines take precedence. (`org-todo-keywords`) |
| `logDone` | false, "time", "note" | `false` | Write `CLOSED:` with the time when an entry is done, and with `note` ask for a note too. (`org-log-done`) |
| `logRepeat` | false, "time", "note" | `"time"` | Record when a repeating entry is done and moves to its next date. (`org-log-repeat`) |
| `logIntoDrawer` | boolean, string | `false` | Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name of a drawer. (`org-log-into-drawer`) |
| `tagsColumn` | number | `-77` | Where tags are aligned after a change to a headline. Negative: they end at that column. (`org-tags-column`) |
| `todoRepeatToState` | string, true | the first keyword of the sequence | The state a repeating entry returns to when it is done: a keyword, or `true` for the state it had. (`org-todo-repeat-to-state`) |
| `priorityHighest` | letter, or 0 to 64 | `"A"` | The highest priority. (`org-priority-highest`) |
| `priorityLowest` | letter, or 0 to 64 | `"C"` | The lowest priority. (`org-priority-lowest`) |
| `priorityDefault` | letter, or 0 to 64 | `"B"` | The priority of an entry without a cookie. A file's `#+PRIORITIES:` line takes precedence over the three priority options. (`org-priority-default`) |
| `priorityStartCycleWithDefault` | boolean | `true` | Raising or lowering the priority of an entry that has none starts at the default. (`org-priority-start-cycle-with-default`) |
| `timestampRoundingMinutes` | number | `5` | The step in which the minutes of a timestamp change (the second number of Org's option). (`org-time-stamp-rounding-minutes`) |
| `readDatePreferFuture` | boolean, "time" | `true` | A date typed without a year or month is the next one to come. With `time`, a time that has passed today is tomorrow's. (`org-read-date-prefer-future`) |
| `readDateForceCompatibleDates` | boolean | `true` | A typed year is kept within 1970 to 2037. (`org-read-date-force-compatible-dates`) |
| `refileTargets` | object[] | `[{ "files": "current", "level": 1 }]` | Where entries can be refiled to. Each entry has `files`: `current` (the entry's own file), `agenda`, a path or a list of paths; and one of `level`, `maxLevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's syntax) or `all` (true), which says which headlines there are targets. (`org-refile-targets`) |
| `refileUseOutlinePath` | boolean, "file", "full-file-path", "title" | `false` | Name targets by their outline path (`Projects/Alpha/`). The three names start the path with the file, and offer the file itself, for its top level. (`org-refile-use-outline-path`) |
| `reverseNoteOrder` | boolean | `false` | A refiled entry goes first under its target, not last. (`org-reverse-note-order`) |
| `captureTemplates` | object[] | `[{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }]` | Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the default, or `plain`), a `target` and a `template` (text, or a list of lines), and may have `prepend`, `emptyLines`, `emptyLinesBefore`, `emptyLinesAfter` and `immediateFinish`. The target is a list: `file` and a path; `file+headline`, a path and a headline; or `file+olp`, a path and the headlines of an outline path. An empty path is `defaultNotesFile`, and without a target the entry goes at the end of `defaultNotesFile`. A template with only a key and a description groups the templates whose keys start with its key. (`org-capture-templates`) |
| `orgDirectory` | string | `"~/org"` | Where the relative paths of capture targets are. (`org-directory`) |
| `defaultNotesFile` | string | `"~/.notes"` | The target of a capture template whose file is empty. (`org-default-notes-file`) |
| `autoSave` | boolean | `true` | Save the files a command changes other than the one it was run in: an entry's file changed from the agenda, the target of a capture or of a refile. The editor saves them once it has made the change. Off, they are left with unsaved changes, as Emacs leaves them. |
| `cycleSeparatorLines` | number | `2` | With at least this many blank lines after a subtree, the last one stays outside its fold. Negative: all of them stay outside. `0`: none does. (`org-cycle-separator-lines`) |
| `agenda.span` | "day", "week", "fortnight", "month", "year", or a number of days | `"week"` | How much time the agenda shows. (`org-agenda-span`) |
| `agenda.startOnWeekday` | 0 to 6 (0 is Sunday), or false | `1` | The day a week or fortnight starts on; `false` starts it today. (`org-agenda-start-on-weekday`) |
| `agenda.deadlineWarningDays` | number | `14` | How many days before a deadline it shows up. (`org-deadline-warning-days`) |
| `agenda.scheduledDelayDays` | number | `0` | How many days after its date a scheduled entry shows up. (`org-scheduled-delay-days`) |
| `agenda.scheduledPastDays` | number | `10000` | How many days a scheduled entry that is not done keeps showing up. (`org-scheduled-past-days`) |
| `agenda.deadlinePastDays` | number | `10000` | How many days a missed deadline keeps showing up. (`org-deadline-past-days`) |
| `agenda.skipScheduledIfDone` | boolean | `false` | Leave out scheduled entries that are done. (`org-agenda-skip-scheduled-if-done`) |
| `agenda.skipDeadlineIfDone` | boolean | `false` | Leave out deadlines of entries that are done. (`org-agenda-skip-deadline-if-done`) |
| `agenda.skipTimestampIfDone` | boolean | `false` | Leave out timestamps of entries that are done. (`org-agenda-skip-timestamp-if-done`) |
| `agenda.showFutureRepeats` | boolean, "next" | `true` | Show the future dates of repeating entries: all of them, none, or only the next. (`org-agenda-show-future-repeats`) |
| `agenda.showAllDates` | boolean | `true` | Show days without entries. (`org-agenda-show-all-dates`) |
| `agenda.tagsColumn` | number | `-80` | Where tags are aligned in the agenda. Org's own default follows the window width, which a server does not know. (`org-agenda-tags-column`) |
| `agenda.useTimeGrid` | boolean | `true` | Draw the time grid. (`org-agenda-use-time-grid`) |
| `agenda.timeGrid.daily` | boolean | `true` | Draw the grid in a one-day agenda. (`org-agenda-time-grid`) |
| `agenda.timeGrid.today` | boolean | `true` | Draw the grid for today. (`org-agenda-time-grid`) |
| `agenda.timeGrid.weekly` | boolean | `false` | Draw the grid for every day. (`org-agenda-time-grid`) |
| `agenda.timeGrid.requireTimed` | boolean | `true` | Draw the grid only on days with an entry that has a time. (`org-agenda-time-grid`) |
| `agenda.timeGrid.removeMatch` | boolean | `false` | Leave out grid lines at a time an entry already has. (`org-agenda-time-grid`) |
| `agenda.timeGrid.times` | number[] | `[800, 1000, 1200, 1400, 1600, 1800, 2000]` | The times of the grid lines, as `800` for 8:00. (`org-agenda-time-grid`) |
| `agenda.timeGrid.trailer` | string | `"......"` | What follows a time that has no end time. (`org-agenda-time-grid`) |
| `agenda.timeGrid.line` | string | `"----------------"` | The text of a grid line. (`org-agenda-time-grid`) |
| `agenda.timeGrid.now` | string | `"now - - - - - - - - - - - - - - - - - - - - - - - - -"` | The text of the line for the current time. (`org-agenda-current-time-string`) |
| `agenda.showCurrentTimeInGrid` | boolean | `true` | Show the current time in the grid. (`org-agenda-show-current-time-in-grid`) |
| `agenda.weekendDays` | number[] | `[6, 0]` | The days drawn as the weekend (0 is Sunday). (`org-agenda-weekend-days`) |
| `virtualDocuments` | boolean | from the client's capabilities | Whether the client fetches the agenda with `workspace/textDocumentContent`. A client that announces that capability need not set this. |

<!-- END generated -->

A document that has `#+TODO:`, `#+SEQ_TODO:` or `#+TYP_TODO:` lines uses
those keywords instead of `todoKeywords`, and one with a `#+PRIORITIES:`
line uses its priorities instead of the three `priority…` options; the
agenda sorts each entry by the priorities of its own file.

Without `agendaFiles`, the agenda is built from the Org files under the
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
| `org.agenda.dispatch` | `org-agenda` (`C-c a`): asks for a view by its key, as Emacs's menu does (input kind `key`; `a` for the agenda, `t` for the TODO list), and opens it. |
| `org.agenda.list` | Opens the agenda for the current week or day. |
| `org.agenda.todos` | Opens the list of all TODO entries. |
| `org.agenda.later`, `org.agenda.earlier` | Moves the agenda one span forward or backward. |
| `org.agenda.today` | Moves the agenda to today. |
| `org.agenda.goto.date` | Asks for a date and shows the agenda from that day, over as many days as its span (`org-agenda-goto-date`); a week or a fortnight still starts on the first day of the week. |
| `org.agenda.day`, `.week`, `.fortnight`, `.month`, `.year` | Changes the span, keeping the day under the cursor in view. |
| `org.agenda.redo` | Rebuilds the agenda. |
| `org.agenda.goto` | Shows the entry of the agenda line under the cursor. |
| `org.agenda.save` | `org-save-all-org-buffers`: returns `{ "save": [uri…] }`, the Org files the editor has open; the editor saves those with changes. |
| `org.todo` | `org-todo`: when the keywords have keys (`TODO(t)`), asks for one (`org-fast-todo-selection`) and sets the state it selects, a space clearing it; without keys, the next state. In an Org file or on an agenda line. |
| `org.todo.next`, `org.todo.previous` | Changes the TODO state of the entry at the cursor, in an Org file or on an agenda line. |
| `org.priority.up`, `org.priority.down` | Raises or lowers the priority of the entry at the cursor, in an Org file or on an agenda line. |
| `org.capture` | `org-capture`: asks for a template's key (input kind `key`, groups one key at a time), then for what its `%^{…}` and `%^t` escapes want (kinds `text` and `date`), with `context` carrying the keys and answers so far. It then writes the filled template, as it will stand in its target, to a capture buffer (a file named `CAPTURE-<target>` in a directory of the server's own) and shows it with the cursor where `%?` was. |
| `org.capture.finish` | `org-capture-finalize` (`C-c C-c` in the capture buffer): the buffer's text goes into the target file, and the result `{ "close": uri }` asks the editor to close the buffer (saving it first). A target that does not exist yet is written by the server. |
| `org.capture.abort` | `org-capture-kill` (`C-c C-k`): closes the capture buffer; nothing changes. |
| `org.capture.refile` | `org-capture-refile` (`C-c C-w`): the entry goes to its target and from there under a refile target picked as for `org.refile`. |
| `org.refile` | `org-refile` (`org-agenda-refile` on an agenda line): offers the targets (an input of kind `choice`), named as Emacs names them (`Projects`, `Home (other.org)`, `Projects/Alpha/`), and moves the subtree under the one picked, as its last child, promoted or demoted to fit, into another file if need be (one `workspace/applyEdit` over both). |
| `org.priority` | Asks for a priority (`Priority A-C, SPC to remove`, an input of kind `priority`) and sets it, or removes it for a space; in an Org file or on an agenda line. |
| `org.timestamp.up`, `org.timestamp.down` | Changes the part of the timestamp the cursor is on. Org files only. |
| `org.date.later`, `org.date.earlier` | Moves a date by days: in an Org file the timestamp at the cursor, on an agenda line the date the line is listed for. |
| `org.shift.up`, `.down`, `.left`, `.right` | Org's shifted cursor keys in an Org file: on a timestamp they change it, on a headline they change its priority (up, down) or TODO state (left, right). |
| `org.schedule`, `org.deadline` | Asks for a date and sets the entry's `SCHEDULED:` or `DEADLINE:`, in an Org file or on an agenda line (`org-agenda-schedule`, `org-agenda-deadline`). |
| `org.timestamp.insert`, `org.timestamp.insert.inactive` | Asks for a date and inserts a timestamp at the cursor, or changes the one there. Org files only. |
| `org.date.preview` | Not for users: what an answer to a date prompt means; see below. Not in the manifest. |
| `org.calendar.day.later`, `.day.earlier`, `.week.later`, `.week.earlier`, `.month.later`, `.month.earlier`, `.year.later`, `.year.earlier` | In the date prompt: move the selected day. |
| `org.calendar.scroll.later`, `org.calendar.scroll.earlier` | In the date prompt: show the next or previous months. |
| `org.calendar.today`, `org.calendar.select`, `org.calendar.type`, `org.calendar.cancel` | In the date prompt: go to today; take the day at the cursor; type the answer; give up. See [The date prompt document](#the-date-prompt-document). |

**TODO state changes** do what Org's `org-todo` does. The cycle runs
through "no keyword" between the last and first states. The tags of the
headline return to their column, and statistics cookies (`[1/3]`, `[33%]`)
in the parent headline are recounted. `CLOSED:` and state notes are
written as `logDone` and the keywords ask; where Org would prompt for the
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
Minutes move in steps of `timestampRoundingMinutes` unless a count is
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
- `"context": { … }`: fields to send back with the answer. A TODO state
  change that records a note (`@`, or `org-log-done` set to `note`) is
  not made until the note's text comes: it asks with kind `note`
  (`Note for state change from "TODO" to "WAIT"`) and `"context": {
  "state": "WAIT" }`, and the answer then sets that state and writes the
  note under the heading line, as `org-store-log-note` does.

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
`documentChanges`); in the agenda they carry the command to run. A client
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
  [Commands](#commands).
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
   also shows it in the minibuffer line.
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
| `code` | `~code~` and inline source blocks. |
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
