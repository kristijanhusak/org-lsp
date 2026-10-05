# Keys in VS Code

The extension binds Emacs's keys, but only where they cost the editor
little:

- **In the agenda and the date prompt.** These are read-only, so Org's
  single letters are free there.
- **In a capture buffer.** A capture buffer is short-lived, and
  `Ctrl+C Ctrl+C` is how it is finished. `Ctrl+C` then waits for a second
  key, so it does not copy in a capture buffer.

Org files get **no keys**. Emacs's keys there (`C-c C-t`, `S-<right>`)
would take over copying and selecting. The commands are in the Command
Palette under **Org**. The ones that change the text at the cursor are also
under **Quick Fix…** (`Ctrl+.`). You can bind any of them to keys of your
own, as shown under *Changing them* below.

The bindings are generated from the server's own description of its
commands (`npm run generate` in `editors/vscode`). **Preferences: Open
Keyboard Shortcuts** lists them; search for `org.`.

The other options are in [configuration.md](configuration.md).

## The keys

<!-- The tables below are written by scripts/generate-docs.mjs from the
     server's manifest and editors/vscode/package.json. Edit the server's
     table in server/org-lsp/src/commands.rs, not the tables. -->

<!-- BEGIN generated: keys -->

### In a capture buffer

| Keys | Command | What it does |
| --- | --- | --- |
| `Ctrl+C Ctrl+C` | `org.capture.finish` | Capture: finish |
| `Ctrl+C Ctrl+K` | `org.capture.abort` | Capture: abort |
| `Ctrl+C Ctrl+W` | `org.capture.refile` | Capture: refile the entry |

### In the agenda

| Keys | Command | What it does |
| --- | --- | --- |
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

### In the date prompt

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
`C-c a` and `C-c c` do. Emacs's own `Ctrl+C` chords work too
(`"ctrl+c ctrl+t"`), but then `Ctrl+C` no longer copies where they apply.

To **remove** a default key, add the same key and command with a `-` in
front of the command. To **change** one, remove it and add the new key:

```json
[
  { "key": "j", "command": "-org.agenda.goto.date", "when": "editorTextFocus && editorLangId == 'org-agenda'" },
  { "key": "shift+j", "command": "org.agenda.goto.date", "when": "editorTextFocus && editorLangId == 'org-agenda'" }
]
```

The same can be done in **Preferences: Open Keyboard Shortcuts**: search
for `org.`, then right-click a binding to change or remove it.
