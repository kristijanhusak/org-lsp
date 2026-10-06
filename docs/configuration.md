# The shared configuration file

The server's options (TODO keywords, agenda files, capture templates and the
rest) can live in one file that the server reads itself, whichever editor
started it. Set them once there, and Neovim, VS Code, Helix and any other
editor use the same ones. Each editor can still change any of them for
itself.

## Where it is

The server reads the first of these that is set:

1. the editor's `configFile` option, if it gives one (see
   [Leaving it out](#leaving-it-out-or-using-another-file));
2. the file named by `$ORG_LSP_CONFIG`;
3. `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp`;
4. `config.toml` or `config.json` in `~/.config/org-lsp` (on Windows,
   `%APPDATA%\org-lsp`).

Write it in whichever of the two you prefer. With both there, the server
reads `config.toml` and says that it leaves `config.json` out. A file
named by `configFile` or `$ORG_LSP_CONFIG` is JSON if its name ends in
`.json`, and TOML otherwise.

There is no need for the file. Without it, the options are the editor's,
and Org's defaults for the rest.

`org-lsp config` shows which file the server reads, anything wrong with it,
and the options it comes to:

```sh
$ org-lsp config
Configuration file: /home/me/.config/org-lsp/config.toml
Options:
{
  "agendaFiles": [
    "~/org"
  ],
  "logDone": "time"
}
```

With `--options`, it puts an editor's options on top, as the server would:
`org-lsp config --options '{"agenda": {"span": "day"}}'`.

## What goes in it

The file is [TOML](https://toml.io) (`config.toml`) or JSON
(`config.json`). Its options are the ones the editors set, with the same
names. In TOML:

```toml
agendaFiles = ["~/org"]
todoKeywords = ["TODO(t)", "NEXT(n)", "WAIT(w@/!)", "|", "DONE(d!)", "CANCELLED(c@)"]
logDone = "time"
logIntoDrawer = true
refileTargets = [{ files = "agenda", maxLevel = 2 }]

[agenda]
span = "day"
startOnWeekday = 0

[[captureTemplates]]
key = "t"
description = "Task"
target = ["file+headline", "~/org/inbox.org", "Tasks"]
template = "* TODO %?\n  %U"

[[captureTemplates]]
key = "n"
description = "Note"
target = ["file", "~/org/notes.org"]
template = ["* %^{Title}", "%U", "", "%?"]
```

The same in JSON, which may have comments and trailing commas, as VS Code's
`settings.json` does, so options can be copied from there as they are:

```jsonc
{
  "agendaFiles": ["~/org"],
  "todoKeywords": ["TODO(t)", "NEXT(n)", "WAIT(w@/!)", "|", "DONE(d!)", "CANCELLED(c@)"],
  "logDone": "time",
  "logIntoDrawer": true,
  "refileTargets": [{ "files": "agenda", "maxLevel": 2 }],
  "agenda": { "span": "day", "startOnWeekday": 0 },
  "captureTemplates": [
    { "key": "t", "description": "Task", "target": ["file+headline", "~/org/inbox.org", "Tasks"], "template": "* TODO %?\n  %U" },
    // Notes, one file of their own.
    { "key": "n", "description": "Note", "target": ["file", "~/org/notes.org"], "template": ["* %^{Title}", "%U", "", "%?"] },
  ],
}
```

TOML is easier to write by hand: templates can span lines, and there are
no braces to balance. JSON is easier to copy settings into from VS Code.

The full list, with every option at its default, is at the end of this
page, in both.

### Completion and checking

[`config.schema.json`](config.schema.json) is the JSON Schema of the file,
written from the server's own table of options. An editor that knows JSON
Schema completes and checks the options with it, and flags a misspelt
name. In JSON, name it with `$schema`:

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/docs/config.schema.json",
  "logDone": "time"
}
```

In TOML, editors that use [Taplo](https://taplo.tamasfe.dev) (VS Code's
Even Better TOML, Helix, Neovim's `taplo` server) read a `#:schema`
comment at the top of the file:

```toml
#:schema https://raw.githubusercontent.com/kristijanhusak/org-lsp/main/docs/config.schema.json
logDone = "time"
```

## Custom agenda commands

`agenda.customCommands` is Emacs's `org-agenda-custom-commands`: views
of your own in the agenda menu (`C-c a`, `<Leader>oa`), next to `a`
(the agenda), `t` (the TODO list) and `m` / `M` (a tags search, asked
for). A command is one view, or a block agenda: several views one after
the other, with a line of `=` between them.

```toml
[[agenda.customCommands]]
key = "n"
description = "Next actions"
type = "todo"
match = "NEXT|WAIT"

[[agenda.customCommands]]
key = "w"
description = "Work, open"
type = "tags-todo"
match = "work-email"

# A prefix key: the menu then offers the commands under it.
[[agenda.customCommands]]
key = "d"
description = "Daily"

[[agenda.customCommands]]
key = "dd"
description = "Today, then what is waiting"
blocks = [
  { type = "agenda", settings = { span = "day" } },
  { type = "tags-todo", match = "PRIORITY=\"A\"", header = "Urgent" },
  { type = "todo", match = "WAIT", header = "Waiting for" },
]
```

The views:

| `type` | Shows | `match` |
| --- | --- | --- |
| `agenda` | the day or week agenda | |
| `alltodo` | every open TODO entry | |
| `todo` | entries with these keywords | `NEXT\|WAIT` |
| `tags` | entries the match string matches | `work-home` |
| `tags-todo` | the same, open TODO entries only | `work-home` |

A match string is Org's: tags joined with `+` (and), `-` (not) and `|`
(or), as in `work+urgent-home|@phone`; a regexp in braces, `{^proj}`;
properties, as in `OWNER="alice"`, `EFFORT>2`, `PRIORITY="A"`, `LEVEL>1`,
`TODO="NEXT"`, `SCHEDULED<"<today>"` (a `*` after the operator, as in
`EFFORT<*3`, also requires the property); and, after a slash, keywords:
`work/NEXT|WAIT`, or `work/!` for any open one.

`header` replaces a view's header, and an empty one leaves it out.
`settings` holds agenda options, as under `agenda`, for a command or a
block: `span`, `startDay` (`-1d`, `2026-10-01`), `deadlineWarningDays`,
`useTimeGrid` and the others. A block's settings go over its command's,
which go over `agenda`'s.

In a block agenda, `f`, `b`, `.`, `j` and the span keys move the agenda
block the cursor is in, as in Emacs.

## An editor's options on top

An editor's own options for the server (Neovim's `settings`, VS Code's
`org-lsp.settings`, Helix's `config`) go on top of the file's:

- an option the editor sets replaces the file's;
- an object, such as `agenda`, is merged option by option, so an editor can
  change the span and keep the file's other agenda options;
- a list, such as `captureTemplates` or `agendaFiles`, is replaced whole.

With the file above, this Neovim configuration shows a week in the agenda,
and keeps everything else from the file:

```lua
require('org-lsp').setup({ settings = { agenda = { span = 'week' } } })
```

A file's own `#+TODO:` and `#+PRIORITIES:` lines still apply to that file,
over both.

## Leaving it out, or using another file

`configFile` is an option of the editor's that says which file to read:

- `false`: none; the editor's options alone count;
- a path: that file instead (`~/` is the home directory). It must exist:
  the server says so when it cannot read it.

```lua
-- Neovim: no shared file.
require('org-lsp').setup({ settings = { configFile = false } })
```

```jsonc
// VS Code: the work configuration.
{ "org-lsp.settings": { "configFile": "~/work/org-lsp.toml" } }
```

## When something is wrong

The server never refuses to start because of its options:

- a file that is not valid TOML or JSON is left out whole, and the server
  shows a message naming the file and the line;
- an option with a value of the wrong type is left out on its own, with a
  message; the other options apply.

The messages also go to the editor's LSP log.

The server reads the file when it starts. After changing it, restart the
server: `:lsp restart` in Neovim, **Org: Restart Language Server** in
VS Code, `:lsp-restart` in Helix.

## Every option, at its default

<!-- The block below is written by scripts/generate-docs.mjs from
     `org-lsp settings`. Edit server/org-lsp/src/settings.rs, not the block. -->

<!-- BEGIN generated: options -->

```toml
# Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built
# from. `~/` is the home directory, and a directory stands for the Org files
# directly in it. When empty, the agenda is built from the Org files under the
# workspace folders (hidden directories, `node_modules` and `target` left
# out), or else from the Org files that are open. (org-agenda-files)
agendaFiles = []

# The TODO sequence, written like Org's `#+TODO:` line. `|` separates the
# states that count as done; without it the last keyword is the only done
# state. `TODO(t)` gives a keyword a key for fast selection, and `WAIT(w@/!)`
# makes it log a note (`@`) or the time (`!`) on entering and leaving it. A
# file's own `#+TODO:` lines take precedence. (org-todo-keywords)
todoKeywords = ["TODO", "|", "DONE"]

# Write `CLOSED:` with the time when an entry is done, and with `note` ask for
# a note too. (org-log-done)
logDone = false

# Record when a repeating entry is done and moves to its next date.
# (org-log-repeat)
logRepeat = "time"

# Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name
# of a drawer. (org-log-into-drawer)
logIntoDrawer = false

# Where tags are aligned after a change to a headline. Negative: they end at
# that column. (org-tags-column)
tagsColumn = -77

# Where `CLOCK:` lines go: `true` for the drawer of `logIntoDrawer` (`LOGBOOK`
# when that is not a name), the name of a drawer, or `false` for the entry
# itself. (org-clock-into-drawer)
clockIntoDrawer = true

# Clocking out after less than a minute removes the `CLOCK:` line.
# (org-clock-out-remove-zero-time-clocks)
clockOutRemoveZeroTimeClocks = false

# Marking the clocked entry done stops the clock. (org-clock-out-when-done)
clockOutWhenDone = true

# Completing a tag offers the tags of every agenda file, after those of the
# file. Off, only the file's own.
# (org-complete-tags-always-offer-all-agenda-tags)
completeTagsAlwaysOfferAllAgendaTags = false

# The state a repeating entry returns to when it is done: a keyword, or `true`
# for the state it had. (org-todo-repeat-to-state)
# todoRepeatToState = …  (unset: the first keyword of the sequence)

# The highest priority. (org-priority-highest)
priorityHighest = "A"

# The lowest priority. (org-priority-lowest)
priorityLowest = "C"

# The priority of an entry without a cookie. A file's `#+PRIORITIES:` line
# takes precedence over the three priority options. (org-priority-default)
priorityDefault = "B"

# Raising or lowering the priority of an entry that has none starts at the
# default. (org-priority-start-cycle-with-default)
priorityStartCycleWithDefault = true

# The step in which the minutes of a timestamp change (the second number of
# Org's option). (org-time-stamp-rounding-minutes)
timestampRoundingMinutes = 5

# A date typed without a year or month is the next one to come. With `time`, a
# time that has passed today is tomorrow's. (org-read-date-prefer-future)
readDatePreferFuture = true

# A typed year is kept within 1970 to 2037.
# (org-read-date-force-compatible-dates)
readDateForceCompatibleDates = true

# Where entries can be refiled to. Each entry has `files`: `current` (the
# entry's own file), `agenda`, a path or a list of paths; and one of `level`,
# `maxLevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's syntax) or
# `all` (true), which says which headlines there are targets.
# (org-refile-targets)
refileTargets = [{ files = "current", level = 1 }]

# Name targets by their outline path (`Projects/Alpha/`). The three names
# start the path with the file, and offer the file itself, for its top level.
# (org-refile-use-outline-path)
refileUseOutlinePath = false

# A refiled entry goes first under its target, not last.
# (org-reverse-note-order)
reverseNoteOrder = false

# Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the
# default, or `plain`), a `target` and a `template` (text, or a list of
# lines), and may have `prepend`, `emptyLines`, `emptyLinesBefore`,
# `emptyLinesAfter` and `immediateFinish`. The target is a list: `file` and a
# path; `file+headline`, a path and a headline; or `file+olp`, a path and the
# headlines of an outline path. An empty path is `defaultNotesFile`, and
# without a target the entry goes at the end of `defaultNotesFile`. A template
# with only a key and a description groups the templates whose keys start with
# its key. (org-capture-templates)
captureTemplates = [{ key = "t", description = "Task", target = ["file+headline", "", "Tasks"], template = "* TODO %?\n  %u\n  %a" }]

# Where the relative paths of capture targets are. (org-directory)
orgDirectory = "~/org"

# The target of a capture template whose file is empty.
# (org-default-notes-file)
defaultNotesFile = "~/.notes"

# Show a link as its description (or its target, when it has none) and hide
# the rest: the brackets, and the target of a link with a description. The
# text is marked `hidden` in the highlighting, and editors that can conceal
# text conceal it. (org-link-descriptive)
linkDescriptive = true

# Hide the markers around `*bold*`, `/italic/`, `_underline_`,
# `+strike-through+`, `=verbatim=` and `~code~`, as for links.
# (org-hide-emphasis-markers)
hideEmphasisMarkers = false

# Save the files a command changes other than the one it was run in: an
# entry's file changed from the agenda, the target of a capture or of a
# refile. The editor saves them once it has made the change. Off, they are
# left with unsaved changes, as Emacs leaves them.
autoSave = true

# With at least this many blank lines after a subtree, the last one stays
# outside its fold. Negative: all of them stay outside. `0`: none does.
# (org-cycle-separator-lines)
cycleSeparatorLines = 2

[agenda]
# How much time the agenda shows. (org-agenda-span)
span = "week"

# The day a week or fortnight starts on; `false` starts it today.
# (org-agenda-start-on-weekday)
startOnWeekday = 1

# How many days before a deadline it shows up. (org-deadline-warning-days)
deadlineWarningDays = 14

# How many days after its date a scheduled entry shows up.
# (org-scheduled-delay-days)
scheduledDelayDays = 0

# How many days a scheduled entry that is not done keeps showing up.
# (org-scheduled-past-days)
scheduledPastDays = 10000

# How many days a missed deadline keeps showing up. (org-deadline-past-days)
deadlinePastDays = 10000

# Leave out scheduled entries that are done.
# (org-agenda-skip-scheduled-if-done)
skipScheduledIfDone = false

# Leave out deadlines of entries that are done.
# (org-agenda-skip-deadline-if-done)
skipDeadlineIfDone = false

# Leave out timestamps of entries that are done.
# (org-agenda-skip-timestamp-if-done)
skipTimestampIfDone = false

# Show the future dates of repeating entries: all of them, none, or only the
# next. (org-agenda-show-future-repeats)
showFutureRepeats = true

# Show days without entries. (org-agenda-show-all-dates)
showAllDates = true

# Where tags are aligned in the agenda. Org's own default follows the window
# width, which a server does not know. (org-agenda-tags-column)
tagsColumn = -80

# Draw the time grid. (org-agenda-use-time-grid)
useTimeGrid = true

# Show the current time in the grid. (org-agenda-show-current-time-in-grid)
showCurrentTimeInGrid = true

# The days drawn as the weekend (0 is Sunday). (org-agenda-weekend-days)
weekendDays = [6, 0]

# Start the agenda in log mode, which shows what was closed and clocked each
# day (`l` toggles it). (org-agenda-start-with-log-mode)
startWithLogMode = false

# Start the agenda with the clock report under it (`R` toggles it).
# (org-agenda-start-with-clockreport-mode)
startWithClockreportMode = false

# What log mode shows: `closed` entries, `clock` lines and `state` changes.
# (org-agenda-log-mode-items)
logModeItems = ["closed", "clock"]

# What goes between the blocks of a block agenda: one character repeated
# across 79 columns, a longer text as it is, or `false` for nothing. In a
# block's `settings`, the separator above that block.
# (org-agenda-block-separator)
blockSeparator = "="

# Leave out the separators between blocks and the header over agenda blocks.
# (org-agenda-compact-blocks)
compactBlocks = false

# Leave out the blocks of a block agenda that show no entries. Not in Org.
hideEmptyBlocks = false

# The day the agenda starts from, as the date prompt reads it: `-1d`, `+2d`,
# `2026-10-01`. (org-agenda-start-day)
# startDay = …  (unset: today)

# Commands of the agenda menu. Each has a `key` and a `description`, and
# either a `type` (`agenda`, `alltodo`, `todo`, `tags` or `tags-todo`) with a
# `match` (keywords such as `NEXT|WAIT` for `todo`, a match string such as
# `work-home` or `PRIORITY="A"` for `tags`), or `blocks`, a list of such views
# shown one after the other. `header` replaces a view's header (empty: none),
# and `settings` holds agenda options for the command or a block (`span`,
# `startDay`…). A command with only a key and a description groups the
# commands whose keys start with its key. (org-agenda-custom-commands)
customCommands = []

[agenda.timeGrid]
# Draw the grid in a one-day agenda. (org-agenda-time-grid)
daily = true

# Draw the grid for today. (org-agenda-time-grid)
today = true

# Draw the grid for every day. (org-agenda-time-grid)
weekly = false

# Draw the grid only on days with an entry that has a time.
# (org-agenda-time-grid)
requireTimed = true

# Leave out grid lines at a time an entry already has. (org-agenda-time-grid)
removeMatch = false

# The times of the grid lines, as `800` for 8:00. (org-agenda-time-grid)
times = [800, 1000, 1200, 1400, 1600, 1800, 2000]

# What follows a time that has no end time. (org-agenda-time-grid)
trailer = "......"

# The text of a grid line. (org-agenda-time-grid)
line = "----------------"

# The text of the line for the current time. (org-agenda-current-time-string)
now = "now - - - - - - - - - - - - - - - - - - - - - - - - -"
```

<!-- END generated -->

<details>
<summary>The same as <code>config.json</code></summary>

<!-- BEGIN generated: options-json -->

```jsonc
{
  // Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built
  // from. `~/` is the home directory, and a directory stands for the Org
  // files directly in it. When empty, the agenda is built from the Org files
  // under the workspace folders (hidden directories, `node_modules` and
  // `target` left out), or else from the Org files that are open.
  // (org-agenda-files)
  "agendaFiles": [],

  // The TODO sequence, written like Org's `#+TODO:` line. `|` separates the
  // states that count as done; without it the last keyword is the only done
  // state. `TODO(t)` gives a keyword a key for fast selection, and
  // `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering and
  // leaving it. A file's own `#+TODO:` lines take precedence.
  // (org-todo-keywords)
  "todoKeywords": ["TODO", "|", "DONE"],

  // Write `CLOSED:` with the time when an entry is done, and with `note` ask
  // for a note too. (org-log-done)
  "logDone": false,

  // Record when a repeating entry is done and moves to its next date.
  // (org-log-repeat)
  "logRepeat": "time",

  // Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the
  // name of a drawer. (org-log-into-drawer)
  "logIntoDrawer": false,

  // Where tags are aligned after a change to a headline. Negative: they end
  // at that column. (org-tags-column)
  "tagsColumn": -77,

  // Where `CLOCK:` lines go: `true` for the drawer of `logIntoDrawer`
  // (`LOGBOOK` when that is not a name), the name of a drawer, or `false` for
  // the entry itself. (org-clock-into-drawer)
  "clockIntoDrawer": true,

  // Clocking out after less than a minute removes the `CLOCK:` line.
  // (org-clock-out-remove-zero-time-clocks)
  "clockOutRemoveZeroTimeClocks": false,

  // Marking the clocked entry done stops the clock. (org-clock-out-when-done)
  "clockOutWhenDone": true,

  // Completing a tag offers the tags of every agenda file, after those of the
  // file. Off, only the file's own.
  // (org-complete-tags-always-offer-all-agenda-tags)
  "completeTagsAlwaysOfferAllAgendaTags": false,

  // The state a repeating entry returns to when it is done: a keyword, or
  // `true` for the state it had. (org-todo-repeat-to-state)
  // "todoRepeatToState": unset: the first keyword of the sequence

  // The highest priority. (org-priority-highest)
  "priorityHighest": "A",

  // The lowest priority. (org-priority-lowest)
  "priorityLowest": "C",

  // The priority of an entry without a cookie. A file's `#+PRIORITIES:` line
  // takes precedence over the three priority options. (org-priority-default)
  "priorityDefault": "B",

  // Raising or lowering the priority of an entry that has none starts at the
  // default. (org-priority-start-cycle-with-default)
  "priorityStartCycleWithDefault": true,

  // The step in which the minutes of a timestamp change (the second number of
  // Org's option). (org-time-stamp-rounding-minutes)
  "timestampRoundingMinutes": 5,

  // A date typed without a year or month is the next one to come. With
  // `time`, a time that has passed today is tomorrow's.
  // (org-read-date-prefer-future)
  "readDatePreferFuture": true,

  // A typed year is kept within 1970 to 2037.
  // (org-read-date-force-compatible-dates)
  "readDateForceCompatibleDates": true,

  // Where entries can be refiled to. Each entry has `files`: `current` (the
  // entry's own file), `agenda`, a path or a list of paths; and one of
  // `level`, `maxLevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's
  // syntax) or `all` (true), which says which headlines there are targets.
  // (org-refile-targets)
  "refileTargets": [{ "files": "current", "level": 1 }],

  // Name targets by their outline path (`Projects/Alpha/`). The three names
  // start the path with the file, and offer the file itself, for its top
  // level. (org-refile-use-outline-path)
  "refileUseOutlinePath": false,

  // A refiled entry goes first under its target, not last.
  // (org-reverse-note-order)
  "reverseNoteOrder": false,

  // Capture templates. Each has a `key`, a `description`, a `type` (`entry`,
  // the default, or `plain`), a `target` and a `template` (text, or a list of
  // lines), and may have `prepend`, `emptyLines`, `emptyLinesBefore`,
  // `emptyLinesAfter` and `immediateFinish`. The target is a list: `file` and
  // a path; `file+headline`, a path and a headline; or `file+olp`, a path and
  // the headlines of an outline path. An empty path is `defaultNotesFile`,
  // and without a target the entry goes at the end of `defaultNotesFile`. A
  // template with only a key and a description groups the templates whose
  // keys start with its key. (org-capture-templates)
  "captureTemplates": [{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }],

  // Where the relative paths of capture targets are. (org-directory)
  "orgDirectory": "~/org",

  // The target of a capture template whose file is empty.
  // (org-default-notes-file)
  "defaultNotesFile": "~/.notes",

  // Show a link as its description (or its target, when it has none) and hide
  // the rest: the brackets, and the target of a link with a description. The
  // text is marked `hidden` in the highlighting, and editors that can conceal
  // text conceal it. (org-link-descriptive)
  "linkDescriptive": true,

  // Hide the markers around `*bold*`, `/italic/`, `_underline_`,
  // `+strike-through+`, `=verbatim=` and `~code~`, as for links.
  // (org-hide-emphasis-markers)
  "hideEmphasisMarkers": false,

  // Save the files a command changes other than the one it was run in: an
  // entry's file changed from the agenda, the target of a capture or of a
  // refile. The editor saves them once it has made the change. Off, they are
  // left with unsaved changes, as Emacs leaves them.
  "autoSave": true,

  // With at least this many blank lines after a subtree, the last one stays
  // outside its fold. Negative: all of them stay outside. `0`: none does.
  // (org-cycle-separator-lines)
  "cycleSeparatorLines": 2,

  "agenda": {
    // How much time the agenda shows. (org-agenda-span)
    "span": "week",

    // The day a week or fortnight starts on; `false` starts it today.
    // (org-agenda-start-on-weekday)
    "startOnWeekday": 1,

    // How many days before a deadline it shows up.
    // (org-deadline-warning-days)
    "deadlineWarningDays": 14,

    // How many days after its date a scheduled entry shows up.
    // (org-scheduled-delay-days)
    "scheduledDelayDays": 0,

    // How many days a scheduled entry that is not done keeps showing up.
    // (org-scheduled-past-days)
    "scheduledPastDays": 10000,

    // How many days a missed deadline keeps showing up.
    // (org-deadline-past-days)
    "deadlinePastDays": 10000,

    // Leave out scheduled entries that are done.
    // (org-agenda-skip-scheduled-if-done)
    "skipScheduledIfDone": false,

    // Leave out deadlines of entries that are done.
    // (org-agenda-skip-deadline-if-done)
    "skipDeadlineIfDone": false,

    // Leave out timestamps of entries that are done.
    // (org-agenda-skip-timestamp-if-done)
    "skipTimestampIfDone": false,

    // Show the future dates of repeating entries: all of them, none, or only
    // the next. (org-agenda-show-future-repeats)
    "showFutureRepeats": true,

    // Show days without entries. (org-agenda-show-all-dates)
    "showAllDates": true,

    // Where tags are aligned in the agenda. Org's own default follows the
    // window width, which a server does not know. (org-agenda-tags-column)
    "tagsColumn": -80,

    // Draw the time grid. (org-agenda-use-time-grid)
    "useTimeGrid": true,

    "timeGrid": {
      // Draw the grid in a one-day agenda. (org-agenda-time-grid)
      "daily": true,

      // Draw the grid for today. (org-agenda-time-grid)
      "today": true,

      // Draw the grid for every day. (org-agenda-time-grid)
      "weekly": false,

      // Draw the grid only on days with an entry that has a time.
      // (org-agenda-time-grid)
      "requireTimed": true,

      // Leave out grid lines at a time an entry already has.
      // (org-agenda-time-grid)
      "removeMatch": false,

      // The times of the grid lines, as `800` for 8:00.
      // (org-agenda-time-grid)
      "times": [800, 1000, 1200, 1400, 1600, 1800, 2000],

      // What follows a time that has no end time. (org-agenda-time-grid)
      "trailer": "......",

      // The text of a grid line. (org-agenda-time-grid)
      "line": "----------------",

      // The text of the line for the current time.
      // (org-agenda-current-time-string)
      "now": "now - - - - - - - - - - - - - - - - - - - - - - - - -"
    },

    // Show the current time in the grid.
    // (org-agenda-show-current-time-in-grid)
    "showCurrentTimeInGrid": true,

    // The days drawn as the weekend (0 is Sunday). (org-agenda-weekend-days)
    "weekendDays": [6, 0],

    // Start the agenda in log mode, which shows what was closed and clocked
    // each day (`l` toggles it). (org-agenda-start-with-log-mode)
    "startWithLogMode": false,

    // Start the agenda with the clock report under it (`R` toggles it).
    // (org-agenda-start-with-clockreport-mode)
    "startWithClockreportMode": false,

    // What log mode shows: `closed` entries, `clock` lines and `state`
    // changes. (org-agenda-log-mode-items)
    "logModeItems": ["closed", "clock"],

    // What goes between the blocks of a block agenda: one character repeated
    // across 79 columns, a longer text as it is, or `false` for nothing. In a
    // block's `settings`, the separator above that block.
    // (org-agenda-block-separator)
    "blockSeparator": "=",

    // Leave out the separators between blocks and the header over agenda
    // blocks. (org-agenda-compact-blocks)
    "compactBlocks": false,

    // Leave out the blocks of a block agenda that show no entries. Not in
    // Org.
    "hideEmptyBlocks": false,

    // The day the agenda starts from, as the date prompt reads it: `-1d`,
    // `+2d`, `2026-10-01`. (org-agenda-start-day)
    // "startDay": unset: today

    // Commands of the agenda menu. Each has a `key` and a `description`, and
    // either a `type` (`agenda`, `alltodo`, `todo`, `tags` or `tags-todo`)
    // with a `match` (keywords such as `NEXT|WAIT` for `todo`, a match string
    // such as `work-home` or `PRIORITY="A"` for `tags`), or `blocks`, a list
    // of such views shown one after the other. `header` replaces a view's
    // header (empty: none), and `settings` holds agenda options for the
    // command or a block (`span`, `startDay`…). A command with only a key and
    // a description groups the commands whose keys start with its key.
    // (org-agenda-custom-commands)
    "customCommands": []
  }
}
```

<!-- END generated -->

</details>
