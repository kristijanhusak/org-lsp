# Keys in VS Code

The setting `org-lsp.keys` picks which keys the extension binds:

- **`emacs`** (the default): Emacs's keys, in Org files (`Ctrl+C Ctrl+T`,
  `Tab`, `Shift+arrows`, …), in capture buffers, and Org's single letters
  in the agenda and the date prompt, which are read-only. In Org files
  `Ctrl+C` then waits for a second key instead of copying; `Ctrl+Insert`
  and the context menu still copy.
- **`vim`**: the Neovim plugin's keys, for VSCodeVim. In Org files, VS Code
  binds only the keys that take nothing from Vim (`Tab`, `Shift+Tab`,
  `Ctrl+Space`, `Ctrl+A`, `Ctrl+X`, `Shift+Up`/`Down`), in Normal mode;
  the keys with the leader go in VSCodeVim's own settings, below. Keys
  such as `cit` would take `c` from Vim, so they are not bound.
- **`custom`**: no keys at all. Bind the commands yourself, as shown under
  *Changing them* below.

`Tab` and `Shift+Tab` cycle the visibility of a subtree and of the whole
file, as Org's `TAB` and `S-TAB` do: folded, its children, all of it;
the top level, every headline, everything. Off a headline, block or
drawer, `Tab` is VS Code's own. The commands are also in the Command
Palette under **Org**, and the ones that change the text at the cursor
under **Quick Fix…** (`Ctrl+.`).

The bindings are generated from the server's own description of its
commands (`npm run generate` in `editors/vscode`). **Preferences: Open
Keyboard Shortcuts** lists them; search for `org.`.

The other options are in [configuration.md](configuration.md).

## The keys

<!-- The tables below are written by scripts/generate-docs.mjs from the
     server's manifest and editors/vscode/package.json. Edit the server's
     table in server/org-lsp/src/commands.rs, not the tables. -->

<!-- BEGIN generated: keys -->

### Emacs (`"org-lsp.keys": "emacs"`, the default)

#### In Org files

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C Ctrl+X Ctrl+I` | `org.clock.in` | Clock in |
| `Ctrl+C Ctrl+X Ctrl+O` | `org.clock.out` | Clock out |
| `Ctrl+C Ctrl+X Ctrl+Q` | `org.clock.cancel` | Cancel the clock |
| `Ctrl+C Ctrl+X Ctrl+J` | `org.clock.goto` | Go to the clocked entry |
| `Ctrl+C Ctrl+T` | `org.todo` | Change the TODO state |
| `Ctrl+C ,` | `org.priority` | Set the priority |
| `Ctrl+C Ctrl+C` | `org.checkbox.toggle` | Toggle the checkbox |
| `Ctrl+C Ctrl+W` | `org.refile` | Refile the entry |
| `Shift+Up`¹ | `org.shift.up` | Increase the timestamp part or raise the priority |
| `Shift+Down`¹ | `org.shift.down` | Decrease the timestamp part or lower the priority |
| `Shift+Left`¹ | `org.shift.left` | Move the date a day earlier or cycle the TODO state backward |
| `Shift+Right`¹ | `org.shift.right` | Move the date a day later or cycle the TODO state forward |
| `Ctrl+C Ctrl+S` | `org.schedule` | Schedule the entry |
| `Ctrl+C Ctrl+D` | `org.deadline` | Set the deadline of the entry |
| `Ctrl+C .` | `org.timestamp.insert` | Insert or change a timestamp |
| `Ctrl+C !` | `org.timestamp.insert.inactive` | Insert or change an inactive timestamp |
| `Tab`¹ | `org-lsp.cycle` | Cycle the visibility of the subtree, block or drawer at the cursor |
| `Shift+Tab` | `org-lsp.cycleGlobal` | Cycle the visibility of the whole file |
| `Ctrl+C a` | `org.agenda.dispatch` | Agenda commands |
| `Ctrl+C c` | `org.capture` | Capture |

#### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C Ctrl+C` | `org.capture.finish` | Capture: finish |
| `Ctrl+C Ctrl+K` | `org.capture.abort` | Capture: abort |
| `Ctrl+C Ctrl+W` | `org.capture.refile` | Capture: refile the entry |

#### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C a` | `org.agenda.dispatch` | Agenda commands |
| `Ctrl+C c` | `org.capture` | Capture |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `j` | `org.agenda.goto.date` | Agenda: go to a date |
| `d`, `v d` | `org.agenda.day` | Agenda: day view |
| `w`, `v w` | `org.agenda.week` | Agenda: week view |
| `v t` | `org.agenda.fortnight` | Agenda: fortnight view |
| `v m` | `org.agenda.month` | Agenda: month view |
| `v y` | `org.agenda.year` | Agenda: year view |
| `r`, `g` | `org.agenda.redo` | Agenda: rebuild |
| `Enter`, `Tab` | `org.agenda.goto` | Agenda: go to the entry |
| `s`, `Ctrl+X Ctrl+S` | `org.agenda.save` | Save all Org files |
| `l`, `v l` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `Shift+R`, `v Shift+R` | `org.agenda.clockreport` | Agenda: the clock report |
| `Shift+I` | `org.clock.in` | Clock in |
| `Shift+O` | `org.clock.out` | Clock out |
| `Shift+X` | `org.clock.cancel` | Cancel the clock |
| `Shift+J` | `org.clock.goto` | Go to the clocked entry |
| `t` | `org.todo` | Change the TODO state |
| `+`, `Shift+Up` | `org.priority.up` | Raise the priority |
| `-`, `Shift+Down` | `org.priority.down` | Lower the priority |
| `,` | `org.priority` | Set the priority |
| `Ctrl+C Ctrl+W` | `org.refile` | Refile the entry |
| `Shift+Right` | `org.date.later` | Move the date one day later |
| `Shift+Left` | `org.date.earlier` | Move the date one day earlier |
| `Ctrl+C Ctrl+S` | `org.schedule` | Schedule the entry |
| `Ctrl+C Ctrl+D` | `org.deadline` | Set the deadline of the entry |

#### In the date prompt

| Keys | Command | What it does |
| --- | --- | --- |
| `Shift+Right` | `org.calendar.day.later` | Date prompt: one day later |
| `Shift+Left` | `org.calendar.day.earlier` | Date prompt: one day earlier |
| `Shift+Down` | `org.calendar.week.later` | Date prompt: one week later |
| `Shift+Up` | `org.calendar.week.earlier` | Date prompt: one week earlier |
| `Alt+Shift+Right` | `org.calendar.month.later` | Date prompt: one month later |
| `Alt+Shift+Left` | `org.calendar.month.earlier` | Date prompt: one month earlier |
| `Alt+Shift+Down` | `org.calendar.year.later` | Date prompt: one year later |
| `Alt+Shift+Up` | `org.calendar.year.earlier` | Date prompt: one year earlier |
| `>` | `org.calendar.scroll.later` | Date prompt: show the next months |
| `<` | `org.calendar.scroll.earlier` | Date prompt: show the previous months |
| `.` | `org.calendar.today` | Date prompt: go to today |
| `Enter` | `org.calendar.select` | Date prompt: take the day at the cursor |
| `i` | `org.calendar.type` | Date prompt: type the date |
| `t` | `org.calendar.time` | Date prompt: pick the time, or go back to the day |
| `q`, `Ctrl+G` | `org.calendar.cancel` | Date prompt: cancel |

### Vim (`"org-lsp.keys": "vim"`)

#### In Org files

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+Space` | `org.checkbox.toggle` | Toggle the checkbox |
| `Ctrl+A`¹ | `org.timestamp.up` | Increase the part of the timestamp at the cursor |
| `Ctrl+X`¹ | `org.timestamp.down` | Decrease the part of the timestamp at the cursor |
| `Shift+Up`¹ | `org.date.later` | Move the date one day later |
| `Shift+Down`¹ | `org.date.earlier` | Move the date one day earlier |
| `Tab` | `org-lsp.cycle` | Cycle the visibility of the subtree, block or drawer at the cursor |
| `Shift+Tab` | `org-lsp.cycleGlobal` | Cycle the visibility of the whole file |

#### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C` | `org.capture.finish` | Capture: finish |

#### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
| `f` | `org.agenda.later` | Agenda: go forward in time |
| `b` | `org.agenda.earlier` | Agenda: go backward in time |
| `.` | `org.agenda.today` | Agenda: go to today |
| `Shift+J` | `org.agenda.goto.date` | Agenda: go to a date |
| `v d` | `org.agenda.day` | Agenda: day view |
| `v w` | `org.agenda.week` | Agenda: week view |
| `v t` | `org.agenda.fortnight` | Agenda: fortnight view |
| `v m` | `org.agenda.month` | Agenda: month view |
| `v y` | `org.agenda.year` | Agenda: year view |
| `r` | `org.agenda.redo` | Agenda: rebuild |
| `Enter`, `Tab` | `org.agenda.goto` | Agenda: go to the entry |
| `v l` | `org.agenda.log` | Agenda: log mode, what was closed and clocked |
| `Shift+R` | `org.agenda.clockreport` | Agenda: the clock report |
| `Shift+I` | `org.clock.in` | Clock in |
| `Shift+O` | `org.clock.out` | Clock out |
| `Shift+X` | `org.clock.cancel` | Cancel the clock |
| `t` | `org.todo` | Change the TODO state |
| `+` | `org.priority.up` | Raise the priority |
| `-` | `org.priority.down` | Lower the priority |
| `Shift+Right` | `org.date.later` | Move the date one day later |
| `Shift+Left` | `org.date.earlier` | Move the date one day earlier |

#### In the date prompt

| Keys | Command | What it does |
| --- | --- | --- |
| `l`, `Shift+Right` | `org.calendar.day.later` | Date prompt: one day later |
| `h`, `Shift+Left` | `org.calendar.day.earlier` | Date prompt: one day earlier |
| `j`, `Shift+Down` | `org.calendar.week.later` | Date prompt: one week later |
| `k`, `Shift+Up` | `org.calendar.week.earlier` | Date prompt: one week earlier |
| `Alt+Shift+Right` | `org.calendar.month.later` | Date prompt: one month later |
| `Alt+Shift+Left` | `org.calendar.month.earlier` | Date prompt: one month earlier |
| `Alt+Shift+Down` | `org.calendar.year.later` | Date prompt: one year later |
| `Alt+Shift+Up` | `org.calendar.year.earlier` | Date prompt: one year earlier |
| `>` | `org.calendar.scroll.later` | Date prompt: show the next months |
| `<` | `org.calendar.scroll.earlier` | Date prompt: show the previous months |
| `.` | `org.calendar.today` | Date prompt: go to today |
| `Enter` | `org.calendar.select` | Date prompt: take the day at the cursor |
| `i` | `org.calendar.type` | Date prompt: type the date |
| `t` | `org.calendar.time` | Date prompt: pick the time, or go back to the day |
| `q`, `Escape` | `org.calendar.cancel` | Date prompt: cancel |

¹ Where the command does not apply, the key does what it does without
the extension: `Shift+Up` off a timestamp or headline selects, `Tab` off
a headline, block or drawer indents.

#### The leader keys, for VSCodeVim

VS Code cannot bind keys that start with the leader without taking
them from Vim. Add them to VSCodeVim's settings instead (they apply in
every file; outside Org files the commands say they do not apply):

```json
"vim.normalModeKeyBindingsNonRecursive": [
  { "before": ["<leader>", "o", "a"], "commands": ["org.agenda.dispatch"] },
  { "before": ["<leader>", "o", "x", "i"], "commands": ["org.clock.in"] },
  { "before": ["<leader>", "o", "x", "o"], "commands": ["org.clock.out"] },
  { "before": ["<leader>", "o", "x", "q"], "commands": ["org.clock.cancel"] },
  { "before": ["<leader>", "o", "x", "j"], "commands": ["org.clock.goto"] },
  { "before": ["<leader>", "o", ","], "commands": ["org.priority"] },
  { "before": ["<leader>", "o", "r"], "commands": ["org.refile"] },
  { "before": ["<leader>", "o", "c"], "commands": ["org.capture"] },
  { "before": ["<leader>", "o", "i", "s"], "commands": ["org.schedule"] },
  { "before": ["<leader>", "o", "i", "d"], "commands": ["org.deadline"] },
  { "before": ["<leader>", "o", "i", "."], "commands": ["org.timestamp.insert"] },
  { "before": ["<leader>", "o", "i", "!"], "commands": ["org.timestamp.insert.inactive"] }
]
```

### Every command

| Command | In the Command Palette |
| --- | --- |
| `org.agenda.dispatch` | Org: Agenda commands |
| `org.agenda.list` | Org: Agenda for the current week or day |
| `org.agenda.todos` | Org: List of all TODO entries |
| `org.agenda.later` | Org: Agenda: go forward in time |
| `org.agenda.earlier` | Org: Agenda: go backward in time |
| `org.agenda.today` | Org: Agenda: go to today |
| `org.agenda.goto.date` | Org: Agenda: go to a date |
| `org.agenda.day` | Org: Agenda: day view |
| `org.agenda.week` | Org: Agenda: week view |
| `org.agenda.fortnight` | Org: Agenda: fortnight view |
| `org.agenda.month` | Org: Agenda: month view |
| `org.agenda.year` | Org: Agenda: year view |
| `org.agenda.redo` | Org: Agenda: rebuild |
| `org.agenda.goto` | Org: Agenda: go to the entry |
| `org.agenda.save` | Org: Save all Org files |
| `org.agenda.log` | Org: Agenda: log mode, what was closed and clocked |
| `org.agenda.clockreport` | Org: Agenda: the clock report |
| `org.clock.in` | Org: Clock in |
| `org.clock.out` | Org: Clock out |
| `org.clock.cancel` | Org: Cancel the clock |
| `org.clock.goto` | Org: Go to the clocked entry |
| `org.todo` | Org: Change the TODO state |
| `org.todo.next` | Org: Cycle TODO state forward |
| `org.todo.previous` | Org: Cycle TODO state backward |
| `org.priority.up` | Org: Raise the priority |
| `org.priority.down` | Org: Lower the priority |
| `org.priority` | Org: Set the priority |
| `org.checkbox.toggle` | Org: Toggle the checkbox |
| `org.refile` | Org: Refile the entry |
| `org.capture` | Org: Capture |
| `org.capture.finish` | Org: Capture: finish |
| `org.capture.abort` | Org: Capture: abort |
| `org.capture.refile` | Org: Capture: refile the entry |
| `org.timestamp.up` | Org: Increase the part of the timestamp at the cursor |
| `org.timestamp.down` | Org: Decrease the part of the timestamp at the cursor |
| `org.date.later` | Org: Move the date one day later |
| `org.date.earlier` | Org: Move the date one day earlier |
| `org.shift.up` | Org: Increase the timestamp part or raise the priority |
| `org.shift.down` | Org: Decrease the timestamp part or lower the priority |
| `org.shift.left` | Org: Move the date a day earlier or cycle the TODO state backward |
| `org.shift.right` | Org: Move the date a day later or cycle the TODO state forward |
| `org.schedule` | Org: Schedule the entry |
| `org.deadline` | Org: Set the deadline of the entry |
| `org.timestamp.insert` | Org: Insert or change a timestamp |
| `org.timestamp.insert.inactive` | Org: Insert or change an inactive timestamp |
| `org.timestamp.change` | Org: Change the timestamp at the cursor |
| `org.calendar.day.later` | Org: Date prompt: one day later |
| `org.calendar.day.earlier` | Org: Date prompt: one day earlier |
| `org.calendar.week.later` | Org: Date prompt: one week later |
| `org.calendar.week.earlier` | Org: Date prompt: one week earlier |
| `org.calendar.month.later` | Org: Date prompt: one month later |
| `org.calendar.month.earlier` | Org: Date prompt: one month earlier |
| `org.calendar.year.later` | Org: Date prompt: one year later |
| `org.calendar.year.earlier` | Org: Date prompt: one year earlier |
| `org.calendar.scroll.later` | Org: Date prompt: show the next months |
| `org.calendar.scroll.earlier` | Org: Date prompt: show the previous months |
| `org.calendar.today` | Org: Date prompt: go to today |
| `org.calendar.select` | Org: Date prompt: take the day at the cursor |
| `org.calendar.type` | Org: Date prompt: type the date |
| `org.calendar.time` | Org: Date prompt: pick the time, or go back to the day |
| `org.calendar.cancel` | Org: Date prompt: cancel |
| `org-lsp.cycle` | Org: Cycle the visibility of the subtree, block or drawer at the cursor |
| `org-lsp.cycleGlobal` | Org: Cycle the visibility of the whole file |

<!-- END generated -->

## How they behave

VS Code has no count, so each command moves by one step. Questions are
asked in VS Code's own way:

- **A date** (scheduling, a deadline, `j` in the agenda) opens Emacs's
  calendar beside the file, with the keys under *In the date prompt*.
  `i` there asks for the date in an input box, with the timestamp the
  answer makes shown as you type. `t` picks a time: a `Time:` line shows
  it, `Shift+Left` and `Shift+Right` choose the hour or the minutes,
  `Shift+Up` and `Shift+Down` move it up or down (the minutes by
  `timestampRoundingMinutes` in `org-lsp.settings`), and `t` again goes
  back to the days, keeping the time.
- **One key**, such as a TODO keyword with a key or a capture template, is
  picked from a list that shows each choice's key (`[t] TODO`). Type to
  narrow the list, then press `Enter`.
- **A choice from a list**, such as a refile target, is picked from a
  list.
- **Text**, such as a capture prompt, is typed in an input box.
- **A state note** (a keyword with `@`, as `WAIT(w@)`) is written in a note
  buffer that opens like a capture buffer, as Emacs's `*Org Note*`:
  `Ctrl+C Ctrl+C` stores it, `Ctrl+C Ctrl+K` changes the state without it.

What Emacs says when a command is done (`Priority of current item set to
B`, `Scheduled to <2026-05-22 Fri>`) shows in the status bar for a few
seconds; a command that cannot run says why in a warning.

## Changing them

Open **Preferences: Open Keyboard Shortcuts (JSON)** and add entries to
`keybindings.json`. Each binding needs a `when` clause that says where it
applies:

| Where | `when` |
| --- | --- |
| Org files | `editorTextFocus && editorLangId == 'org'` |
| The agenda | `editorTextFocus && editorLangId == 'org-agenda'` |
| The date prompt | `editorTextFocus && editorLangId == 'org-date'` |
| A capture buffer | `editorTextFocus && editorLangId == 'org' && resourceFilename =~ /^CAPTURE-/` |

To **add** a key, bind it to the command's name, from the second column of
the tables above:

```json
[
  { "key": "ctrl+alt+a", "command": "org.agenda.dispatch" },
  { "key": "ctrl+alt+c", "command": "org.capture" },
  { "key": "alt+t", "command": "org.todo", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+up", "command": "org.shift.up", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+down", "command": "org.shift.down", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+left", "command": "org.shift.left", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+right", "command": "org.shift.right", "when": "editorTextFocus && editorLangId == 'org'" },
  { "key": "alt+s", "command": "org.schedule", "when": "editorTextFocus && editorLangId == 'org'" }
]
```

The first two have no `when`, so they work anywhere, as Emacs's
`C-c a` and `C-c c` do. With `"org-lsp.keys": "custom"` these are the only
keys there are.

To **remove** a default key, add the same key and command with a `-` in
front of the command. To **change** one, remove it and add the new key:

```json
[
  { "key": "j", "command": "-org.agenda.goto.date" },
  { "key": "shift+j", "command": "org.agenda.goto.date", "when": "editorTextFocus && editorLangId == 'org-agenda'" }
]
```

The same can be done in **Preferences: Open Keyboard Shortcuts**: search
for `org.`, then right-click a binding to change or remove it.
