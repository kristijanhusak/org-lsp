# Org (org-lsp) for VS Code

Org mode support through the org-lsp language server: highlighting,
folding, the outline, TODO states, priorities, timestamps and the agenda.

The extension is a thin client. It starts the server and tells it where
the cursor is when a command runs. The commands, their keys and the token
colors in `package.json` are generated from the server's own description
of itself (`npm run generate`), so nothing about Org is listed here by
hand.

## Commands

All commands are in the Command Palette under **Org**.

| Command | What it does |
| --- | --- |
| `Org: Agenda for the current week or day` | Opens the agenda |
| `Org: List of all TODO entries` | Opens the TODO list |
| `Org: Agenda commands` | Asks which of the two to open |
| `Org: Change the TODO state` | Org's `C-c C-t` (`t` in the agenda): with keys on the keywords (`TODO(t)`), pick one from a list; else the next state. A state with `@` asks for its note |
| `Org: Cycle TODO state forward` / `backward` | Changes the entry under the cursor, in an Org file or on an agenda line |
| `Org: Raise the priority` / `Lower the priority` | The same, for the priority |
| `Org: Refile the entry` | Org's `C-c C-w` (also in the agenda): pick a target from a list; see `refileTargets` |
| `Org: Set the priority` | Asks for it, as Org's `C-c ,` does (`,` in the agenda); a space removes it |
| `Org: Increase` / `Decrease the part of the timestamp at the cursor` | The year, month, day, hour or minute under the cursor, as Org's `S-<up>` / `S-<down>` |
| `Org: Move the date one day later` / `earlier` | The timestamp under the cursor, or the date of an agenda line |
| `Org: Increase the timestamp part or raise the priority`, and the three like it | Org's `S-<up>`, `S-<down>`, `S-<left>`, `S-<right>`: what they do depends on where the cursor is |
| `Org: Schedule the entry`, `Org: Set the deadline of the entry` | Ask for a date and set it, in an Org file or on an agenda line (`Ctrl+C Ctrl+S`, `Ctrl+C Ctrl+D` there) |
| `Org: Insert or change a timestamp` / `an inactive timestamp` | Ask for a date and insert it at the cursor, or change the timestamp there |
| `Org: Agenda: …` | Move through time, go to a date (`j`), change the span, rebuild, go to the entry |

The changes that apply at the cursor are also listed under **Quick Fix…**
(`Ctrl+.`). VS Code has no count to pass: each command moves by one step.

A date is typed as at Org's prompt: `fri 10:00`, `+2d`, `2/15`, `++1w`,
`10:00-11:30`. Under the input box the extension shows the timestamp the
answer makes, as you type.

As in Emacs, a date command shows a calendar first, beside the file, with
the answer and what it means on its last line. Emacs's keys work there:
`Shift+arrows` by day and week, `Alt+Shift+Left`/`Right` by month,
`Alt+Shift+Up`/`Down` by year, `<` `>` for other months, `.` for today,
`Enter` to take the day under the cursor, `i` to type the answer (read
with the day you moved to, if you did), `q` or `Ctrl+G` to give up.

## Keys

The agenda is read-only, so it has Org's single-letter keys:

| Key | In the agenda |
| --- | --- |
| `f` / `b` | Later / earlier |
| `.` | Today |
| `d`, `w`, `v d`, `v w`, `v t`, `v m`, `v y` | Day, week, fortnight, month, year |
| `r` or `g` | Rebuild |
| `t` | Next TODO state of the entry |
| `+` / `-`, `Shift+Up` / `Shift+Down` | Raise / lower the entry's priority |
| `Shift+Right` / `Shift+Left` | Move the entry's date a day later / earlier |
| `Enter` or `Tab` | Go to the entry |
| `s` or `Ctrl+X Ctrl+S` | Save all Org files with changes |

After a change of priority or date, the cursor goes with the entry to
its new line.

No keys are bound in Org files: Emacs's keys there (`C-c C-t`,
`S-<right>`) would take over copying and selecting. Add your own in
`keybindings.json`:

```json
{ "key": "alt+t", "command": "org.todo.next", "when": "editorTextFocus && editorLangId == 'org'" },
{ "key": "alt+up", "command": "org.shift.up", "when": "editorTextFocus && editorLangId == 'org'" }
```

## Settings

| Setting | Meaning |
| --- | --- |
| `org-lsp.settings` | The server's options, for example `{ "agendaFiles": ["~/org"], "logDone": "time" }`. They are listed in the server's `docs/protocol.md`. |
| `org-lsp.server.path` | Path to the server binary; see below. |

Without `agendaFiles`, the agenda is built from the Org files in the
workspace folders.

A change made from the agenda edits the entry's file and leaves it
unsaved, like any edit; the agenda follows at once.

## The server binary

The extension looks for the server in this order:

1. the `org-lsp.server.path` setting,
2. `server/org-lsp` inside the extension (present in packaged builds),
3. `org-lsp` on `PATH`.

## Building

```sh
cargo build --release -p org-lsp   # in the repository root
npm install
npm run generate                   # package.json from the server's manifest
npm run bundle-server              # copies the binary into server/
npm run package                    # writes org-lsp-<version>.vsix
```

`npm test` runs the extension inside VS Code against the server
(`xvfb-run -a npm test` on a machine without a display).
