# Configuring Neovim

The plugin works without configuration. Everything you set is merged into
the defaults, so you only write what you want to change.

## Where it goes

Either set `vim.g.org_lsp` before the plugin loads, which suits lazy.nvim's
`init`:

```lua
{
  'kristijanhusak/org-lsp',
  build = 'cargo build --release -p org-lsp',
  init = function()
    vim.g.org_lsp = { settings = { agendaFiles = { '~/org' } } }
  end,
}
```

or call `setup()` at any time:

```lua
require('org-lsp').setup({ settings = { agendaFiles = { '~/org' } } })
```

Calling `setup()` more than once merges each call into what was there.

## The plugin's options

These are the defaults:

```lua
require('org-lsp').setup({
  -- The server command. Unset, the plugin uses the server it built
  -- (target/release/org-lsp in the plugin's directory), or else
  -- `org-lsp` from PATH.
  cmd = nil,

  -- The server's options, passed to it as they are. See below.
  settings = {},

  -- How a newly opened file is folded: 'showeverything' or 'overview'.
  startup_folded = 'showeverything',

  -- Fold with the server's folding ranges.
  folding = true,

  -- Turn off Vim's regex syntax for Org once the server's highlighting
  -- arrives.
  replace_syntax = true,

  -- How the agenda and capture buffers open; see below.
  win_split_mode = 'split',

  -- Keys by scope; see keys.md. `false` sets no keys at all.
  mappings = {
    global = { ['org.agenda.dispatch'] = '<Leader>oa', ['org.capture'] = '<Leader>oc' },
    org = { cycle = '<Tab>', global_cycle = '<S-Tab>' },
    agenda = { quit = 'q' },
  },
})
```

`mappings` lists only the keys the plugin needs before the server has
started. The other keys come from the server, and
[keys.md](keys.md) lists them all and shows how to change them.

## Where the agenda and capture buffers open

`win_split_mode` says how a window is opened for the agenda and for a
capture buffer. One that is already shown is used again.

| Value | Opens it |
| --- | --- |
| `'split'` | In a horizontal split (the default) |
| `'vsplit'` | In a vertical split |
| `'float'` | In a floating window in the middle of the editor |
| `'current'` | In the current window |
| another Ex command | With that command, such as `'botright split'`, `'topleft vsplit'` or `'tabnew'` |
| a function | With the function, which is given the buffer and returns the window it opened |

```lua
require('org-lsp').setup({ win_split_mode = 'float' })

-- A float of your own size.
require('org-lsp').setup({
  win_split_mode = function(bufnr)
    return vim.api.nvim_open_win(bufnr, true, {
      relative = 'editor', row = 2, col = 4, width = vim.o.columns - 8, height = vim.o.lines - 6, border = 'single',
    })
  end,
})
```

Closing the agenda (`q`) or finishing a capture closes the window that was
opened for it; with `'current'`, the window goes back to the buffer it
showed before. Going to an entry from the agenda opens the entry's file in
the agenda's window, or, from a floating window, in the window under it.
The date prompt's calendar always opens at the bottom.

## The server's options

`settings` goes to the server as it is. Leave out an option and
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

```lua
require('org-lsp').setup({
  settings = {
    -- The shared configuration file, whose options this editor's go on top
    -- of: `false` for none, or the path of another file (JSON if it ends in
    -- `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, else `config.toml`
    -- or `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`;
    -- `%APPDATA%\org-lsp` on Windows).
    -- configFile = nil, -- unset: the shared file, `~/.config/org-lsp/config.toml` or `config.json`

    -- Files, directories and glob patterns (`*`, `?`, `**`) the agenda is
    -- built from. `~/` is the home directory, and a directory stands for the
    -- Org files directly in it. When empty, the agenda is built from the Org
    -- files under the workspace folders (hidden directories, `node_modules`
    -- and `target` left out), or else from the Org files that are open.
    -- (org-agenda-files)
    agendaFiles = {},

    -- The TODO sequence, written like Org's `#+TODO:` line. `|` separates the
    -- states that count as done; without it the last keyword is the only done
    -- state. `TODO(t)` gives a keyword a key for fast selection, and
    -- `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering
    -- and leaving it. A file's own `#+TODO:` lines take precedence.
    -- (org-todo-keywords)
    todoKeywords = { 'TODO', '|', 'DONE' },

    -- Write `CLOSED:` with the time when an entry is done, and with `note`
    -- ask for a note too. (org-log-done)
    logDone = false,

    -- Record when a repeating entry is done and moves to its next date.
    -- (org-log-repeat)
    logRepeat = 'time',

    -- Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the
    -- name of a drawer. (org-log-into-drawer)
    logIntoDrawer = false,

    -- Where tags are aligned after a change to a headline. Negative: they end
    -- at that column. (org-tags-column)
    tagsColumn = -77,

    -- The state a repeating entry returns to when it is done: a keyword, or
    -- `true` for the state it had. (org-todo-repeat-to-state)
    -- todoRepeatToState = nil, -- unset: the first keyword of the sequence

    -- The highest priority. (org-priority-highest)
    priorityHighest = 'A',

    -- The lowest priority. (org-priority-lowest)
    priorityLowest = 'C',

    -- The priority of an entry without a cookie. A file's `#+PRIORITIES:`
    -- line takes precedence over the three priority options.
    -- (org-priority-default)
    priorityDefault = 'B',

    -- Raising or lowering the priority of an entry that has none starts at
    -- the default. (org-priority-start-cycle-with-default)
    priorityStartCycleWithDefault = true,

    -- The step in which the minutes of a timestamp change (the second number
    -- of Org's option). (org-time-stamp-rounding-minutes)
    timestampRoundingMinutes = 5,

    -- A date typed without a year or month is the next one to come. With
    -- `time`, a time that has passed today is tomorrow's.
    -- (org-read-date-prefer-future)
    readDatePreferFuture = true,

    -- A typed year is kept within 1970 to 2037.
    -- (org-read-date-force-compatible-dates)
    readDateForceCompatibleDates = true,

    -- Where entries can be refiled to. Each entry has `files`: `current` (the
    -- entry's own file), `agenda`, a path or a list of paths; and one of
    -- `level`, `maxLevel`, `tag`, `todo`, `regexp` (the Rust `regex` crate's
    -- syntax) or `all` (true), which says which headlines there are targets.
    -- (org-refile-targets)
    refileTargets = { { files = 'current', level = 1 } },

    -- Name targets by their outline path (`Projects/Alpha/`). The three names
    -- start the path with the file, and offer the file itself, for its top
    -- level. (org-refile-use-outline-path)
    refileUseOutlinePath = false,

    -- A refiled entry goes first under its target, not last.
    -- (org-reverse-note-order)
    reverseNoteOrder = false,

    -- Capture templates. Each has a `key`, a `description`, a `type`
    -- (`entry`, the default, or `plain`), a `target` and a `template` (text,
    -- or a list of lines), and may have `prepend`, `emptyLines`,
    -- `emptyLinesBefore`, `emptyLinesAfter` and `immediateFinish`. The target
    -- is a list: `file` and a path; `file+headline`, a path and a headline;
    -- or `file+olp`, a path and the headlines of an outline path. An empty
    -- path is `defaultNotesFile`, and without a target the entry goes at the
    -- end of `defaultNotesFile`. A template with only a key and a description
    -- groups the templates whose keys start with its key.
    -- (org-capture-templates)
    captureTemplates = { { key = 't', description = 'Task', target = { 'file+headline', '', 'Tasks' }, template = '* TODO %?\n  %u\n  %a' } },

    -- Where the relative paths of capture targets are. (org-directory)
    orgDirectory = '~/org',

    -- The target of a capture template whose file is empty.
    -- (org-default-notes-file)
    defaultNotesFile = '~/.notes',

    -- Save the files a command changes other than the one it was run in: an
    -- entry's file changed from the agenda, the target of a capture or of a
    -- refile. The editor saves them once it has made the change. Off, they
    -- are left with unsaved changes, as Emacs leaves them.
    autoSave = true,

    -- With at least this many blank lines after a subtree, the last one stays
    -- outside its fold. Negative: all of them stay outside. `0`: none does.
    -- (org-cycle-separator-lines)
    cycleSeparatorLines = 2,

    agenda = {
      -- How much time the agenda shows. (org-agenda-span)
      span = 'week',

      -- The day a week or fortnight starts on; `false` starts it today.
      -- (org-agenda-start-on-weekday)
      startOnWeekday = 1,

      -- How many days before a deadline it shows up.
      -- (org-deadline-warning-days)
      deadlineWarningDays = 14,

      -- How many days after its date a scheduled entry shows up.
      -- (org-scheduled-delay-days)
      scheduledDelayDays = 0,

      -- How many days a scheduled entry that is not done keeps showing up.
      -- (org-scheduled-past-days)
      scheduledPastDays = 10000,

      -- How many days a missed deadline keeps showing up.
      -- (org-deadline-past-days)
      deadlinePastDays = 10000,

      -- Leave out scheduled entries that are done.
      -- (org-agenda-skip-scheduled-if-done)
      skipScheduledIfDone = false,

      -- Leave out deadlines of entries that are done.
      -- (org-agenda-skip-deadline-if-done)
      skipDeadlineIfDone = false,

      -- Leave out timestamps of entries that are done.
      -- (org-agenda-skip-timestamp-if-done)
      skipTimestampIfDone = false,

      -- Show the future dates of repeating entries: all of them, none, or
      -- only the next. (org-agenda-show-future-repeats)
      showFutureRepeats = true,

      -- Show days without entries. (org-agenda-show-all-dates)
      showAllDates = true,

      -- Where tags are aligned in the agenda. Org's own default follows the
      -- window width, which a server does not know. (org-agenda-tags-column)
      tagsColumn = -80,

      -- Draw the time grid. (org-agenda-use-time-grid)
      useTimeGrid = true,

      timeGrid = {
        -- Draw the grid in a one-day agenda. (org-agenda-time-grid)
        daily = true,

        -- Draw the grid for today. (org-agenda-time-grid)
        today = true,

        -- Draw the grid for every day. (org-agenda-time-grid)
        weekly = false,

        -- Draw the grid only on days with an entry that has a time.
        -- (org-agenda-time-grid)
        requireTimed = true,

        -- Leave out grid lines at a time an entry already has.
        -- (org-agenda-time-grid)
        removeMatch = false,

        -- The times of the grid lines, as `800` for 8:00.
        -- (org-agenda-time-grid)
        times = { 800, 1000, 1200, 1400, 1600, 1800, 2000 },

        -- What follows a time that has no end time. (org-agenda-time-grid)
        trailer = '......',

        -- The text of a grid line. (org-agenda-time-grid)
        line = '----------------',

        -- The text of the line for the current time.
        -- (org-agenda-current-time-string)
        now = 'now - - - - - - - - - - - - - - - - - - - - - - - - -',
      },

      -- Show the current time in the grid.
      -- (org-agenda-show-current-time-in-grid)
      showCurrentTimeInGrid = true,

      -- The days drawn as the weekend (0 is Sunday).
      -- (org-agenda-weekend-days)
      weekendDays = { 6, 0 },
    },
  },
})
```

<!-- END generated -->

## Changing them

Set only what you want different:

```lua
require('org-lsp').setup({
  settings = {
    agendaFiles = { '~/org', '~/notes/**/*.org' },
    todoKeywords = { 'TODO(t)', 'NEXT(n)', 'WAIT(w@/!)', '|', 'DONE(d!)', 'CANCELLED(c@)' },
    logDone = 'time',
    logIntoDrawer = true,
    refileTargets = { { files = 'agenda', maxLevel = 2 } },
    refileUseOutlinePath = 'file',
    captureTemplates = {
      { key = 't', description = 'Task', target = { 'file+headline', '~/org/inbox.org', 'Tasks' }, template = '* TODO %?\n  %U' },
      { key = 'n', description = 'Note', target = { 'file', '~/org/notes.org' }, template = { '* %^{Title}', '%U', '', '%?' } },
    },
    agenda = { span = 'day', startOnWeekday = 0 },
  },
})
```

A list replaces the default list whole: the `captureTemplates` above are
the only templates, and Org's default one is gone.

A file can set some of these for itself, as in Emacs: `#+TODO:` lines
replace `todoKeywords` and `#+PRIORITIES:` replaces the three `priority…`
options in that file.

The server reads its options when it starts. After changing `settings`,
restart it with `:lsp restart` (Neovim 0.12) or restart Neovim. An option
with a value of the wrong type is ignored, and the server says so in
Neovim's LSP log (`:checkhealth vim.lsp` shows where it is); the other
options still apply.

`:checkhealth org-lsp` shows which server the plugin found.

## Highlighting

The server's highlighting arrives as semantic tokens, which Neovim shows
with the groups `@lsp.type.<token>.org` and
`@lsp.typemod.<token>.<modifier>.org`; the tokens and modifiers are listed
in `docs/protocol.md`. The plugin links them to nvim-orgmode's `@org.*`
groups by default, so a colorscheme that styles those applies here too.
Like any group, they can be set in your configuration:

```lua
vim.api.nvim_set_hl(0, '@lsp.type.todoKeyword.org', { fg = '#ff6c6b', bold = true })
vim.api.nvim_set_hl(0, '@lsp.typemod.todoKeyword.done.org', { fg = '#98be65', bold = true })
```
