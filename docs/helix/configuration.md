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

Files with `org-indent-mode` (`startupIndented`, `#+STARTUP: indent`)
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

# Which tags entries inherit from their parents and from `#+FILETAGS:`: `true`
# for all, `false` for none, a list of tags, or a regexp (Emacs syntax) tags
# must match. Inherited tags count in tag searches and show in the agenda.
# (org-use-tag-inheritance)
useTagInheritance = true

# Tags never inherited, when `useTagInheritance` is `true` or a regexp.
# (org-tags-exclude-from-inheritance)
tagsExcludeFromInheritance = []

# Which properties an entry without them takes from its parents, the file's
# property drawer and `#+PROPERTY:` lines, in property searches
# (`OWNER="alice"`): `true` for all, `false` for none, a list of names, or a
# regexp (Emacs syntax). `CATEGORY` and `ARCHIVE` are always inherited.
# (org-use-property-inheritance)
usePropertyInheritance = false

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

# Where archived subtrees go: a file, `::`, and a headline under which they go
# (they go to the end of the file without one). `%s` is the name of the file
# the subtree is in; an empty file part is that file. `datetree/` as the
# headline files them under year, month and day, by when they were closed. An
# `ARCHIVE` property of the entry or a parent, or a `#+ARCHIVE:` line, says
# otherwise. (org-archive-location)
archiveLocation = "%s_archive::"

# An archived subtree goes first under its archive headline, not last.
# (org-archive-reversed-order)
archiveReversedOrder = false

# Mark archived entries done: `true` for the first done keyword, or the
# keyword to use. (org-archive-mark-done)
archiveMarkDone = false

# Give an archived subtree the tags it inherited: always, never, or when it
# stays in its file. (org-archive-subtree-add-inherited-tags)
archiveSubtreeAddInheritedTags = "infile"

# What an archived entry keeps as `ARCHIVE_…` properties: `time`, `file`,
# `olpath` (its outline path), `category`, `todo`, `itags` (inherited tags),
# `ltags` (its own tags) and `olid` (its parent's ID).
# (org-archive-save-context-info)
archiveSaveContextInfo = ["time", "file", "olpath", "category", "todo", "itags"]

# The headline of the sibling `org.archive.sibling` archives under.
# (org-archive-sibling-heading)
archiveSiblingHeading = "Archive"

# What a new archive file starts with; `%s` is the path of the file the
# subtree came from. (org-archive-file-header-format)
archiveFileHeaderFormat = "\nArchived entries from file %s\n\n"

# What `org.archive.default` does: archive the subtree, archive it under its
# sibling, or tag it. (org-archive-default-command)
archiveDefaultCommand = "subtree"

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

# RET on the first line of a list item makes the next item, numbered and with
# a checkbox as M-RET (`org-meta-return`) makes it, the items after it
# renumbered, as nvim-orgmode's `org_return_uses_meta_return` does. Off, RET
# is Org's: a new line in the item.
returnUsesMetaReturn = false

# Save the files a command changes other than the one it was run in: an
# entry's file changed from the agenda, the target of a capture or of a
# refile. The editor saves them once it has made the change. Off, they are
# left with unsaved changes, as Emacs leaves them.
autoSave = true

# With at least this many blank lines after a subtree, the last one stays
# outside its fold. Negative: all of them stay outside. `0`: none does.
# (org-cycle-separator-lines)
cycleSeparatorLines = 2

# Indent the lines of an entry past its headline's stars: `true` for all of
# them, `"headline-data"` for the planning line, drawers and clock lines right
# below the headline. Applies to new lines, to re-indenting (range formatting)
# and to what commands write. (org-adapt-indentation)
adaptIndentation = false

# Show files indented by level without changing them (`org-indent-mode`): the
# text under a headline lines up with its title, and leading stars are hidden.
# `#+STARTUP: indent` and `noindent` decide for a file. It turns
# `adaptIndentation` off in such files, unless that is `"headline-data"`.
# Editors that can show it: Neovim, and VS Code (not on wrapped lines).
# (org-startup-indented)
startupIndented = false

# Hide all the stars of a headline but the last; they keep their place.
# `#+STARTUP: hidestars` and `showstars` decide for a file.
# (org-hide-leading-stars)
hideLeadingStars = false

# Leave the code of source blocks as it is indented when re-indenting.
# (org-src-preserve-indentation)
srcPreserveIndentation = false

# How far code is indented past its source block when re-indenting.
# (org-edit-src-content-indentation)
editSrcContentIndentation = 2

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

# Where tags are aligned in the agenda: a column (negative: where they end),
# or `auto` for the right edge of the window. With `auto`, the editors that
# tell the server how wide the agenda's window is (Neovim, Vim, Sublime Text)
# have the tags there, and the agenda follows the window as it is resized; the
# others (VS Code, Zed) have them end at column 80. (org-agenda-tags-column)
tagsColumn = "auto"

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

# Start the agenda with entries in archived trees (`trees`; `v a` toggles it),
# or those and the files the agenda files archive to (`files`; `v A`).
# (org-agenda-start-with-archives-mode)
startWithArchivesMode = false

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

# How a TODO keyword is written in the agenda: `%-4s` pads it to four columns,
# so that the titles line up; an empty format leaves keywords out.
# (org-agenda-todo-keyword-format)
todoKeywordFormat = "%-1s"

# The leader of a scheduled entry on its day, and on today when its day has
# passed (`%d`: the days since). (org-agenda-scheduled-leaders)
scheduledLeaders = ["Scheduled: ", "Sched.%2dx: "]

# The leader of a deadline on its day, on today before it (`%d`: the days
# left), and on today after it (`%d`: the days since).
# (org-agenda-deadline-leaders)
deadlineLeaders = ["Deadline:  ", "In %3d d.: ", "%2d d. ago: "]

# The leader of a time range within a day, and of a range over days (`%d`:
# which day of it, `%d`: how many days). (org-agenda-timerange-leaders)
timerangeLeaders = ["", "(%d/%d): "]

# Leave the tags out of the agenda's lines; `prefix` only when the prefix
# shows a tag (`%T`). (org-agenda-remove-tags)
removeTags = false

# What goes between the headlines of `%b`. (org-agenda-breadcrumbs-separator)
breadcrumbsSeparator = "->"

# How the agenda writes a day, as `format-time-string` does: `%A %d %B %Y`,
# `%a %Y-%m-%d`. (org-agenda-format-date)
# formatDate = …  (unset: as `Wednesday   7 October 2026`, the week's number added on Mondays)

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

[language-server.org-lsp.config.agenda.prefixFormat]
# What comes before the headline on a line of the agenda: `%c` the category,
# `%t` the time, `%s` the leader (`Scheduled: `), `%e` the effort, `%l` a
# blank per level, `%T` the last tag, `%b` the headlines above (breadcrumbs),
# `%i` the icon (none). A width pads (`%-12c` on the right, `%12c` on the
# left), `.N` cuts, `%?` leaves out an empty field, and a character before the
# letter follows a value that is not empty (`%-12:c`). `%(…)` (Emacs Lisp) is
# left empty. (org-agenda-prefix-format)
agenda = " %i %-12:c%?-12t% s"

# The same, for the TODO list. (org-agenda-prefix-format)
todo = " %i %-12:c"

# The same, for tags searches. (org-agenda-prefix-format)
tags = " %i %-12:c"

# The same, for text searches, which the agenda does not have yet.
# (org-agenda-prefix-format)
search = " %i %-12:c"
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
