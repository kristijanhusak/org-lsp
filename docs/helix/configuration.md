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

## The server's options

The server's options go in a `config` table under the server, which Helix
passes to it as they are. Leave out an option and
the server takes it from the [shared configuration file](../configuration.md),
if you have one, or else uses its default, which is Org's. The editor's
options go on top of the file's: an object such as `agenda` is merged
option by option, and anything else replaces the file's. `configFile`
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
# configFile = …  (unset: the shared file, `~/.config/org-lsp/config.toml` or `config.json`)

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

[language-server.org-lsp.config.agenda]
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

[language-server.org-lsp.config.agenda.timeGrid]
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
todoKeywords = ["TODO(t)", "NEXT(n)", "WAIT(w@/!)", "|", "DONE(d!)", "CANCELLED(c@)"]
logDone = "time"
logIntoDrawer = true
priorityLowest = "E"
```

A list replaces the default list whole.

A file can set some of these for itself, as in Emacs: `#+TODO:` lines
replace `todoKeywords` and `#+PRIORITIES:` replaces the three `priority…`
options in that file.

The server reads its options when it starts: run `:lsp-restart` after a
change. An option with a value of the wrong type is ignored, and the server
says so in Helix's log (`:log-open`); the other options still apply.
