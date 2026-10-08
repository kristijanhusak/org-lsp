# Org (org-lsp) for VS Code

Org mode support through the org-lsp language server: highlighting
(the code of source and export blocks and of inline source blocks in its
own language's colors, for the languages VS Code knows), folding, the
outline, TODO states, priorities, timestamps and the agenda.

The extension is a thin client. It starts the server and tells it where
the cursor is when a command runs. The commands, their keys and the token
colors in `package.json` are generated from the server's own description
of itself (`npm run generate`), so nothing about Org is listed here by
hand.

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | VS Code | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 92% | ✅ |  |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 67% | ✅ |  |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ✅ |  |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 91% | ✅ |  |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ✅ |  |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ✅ |  |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 83% | ✅ |  |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ✅ |  |
| [Agenda](../../docs/parity.md#agenda) | 95% | ❓ | Tags are at column 80, not at the window's edge (`org_agenda_tags_column` `"auto"`): VS Code does not tell an extension how many columns an editor shows. |
| [Clocking](../../docs/parity.md#clocking) | 100% | ✅ |  |
| [Capture](../../docs/parity.md#capture) | 100% | ✅ |  |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 100% | ✅ |  |
| [Links](../../docs/parity.md#links) | 100% | ✅ |  |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ✅ |  |
| [Tables](../../docs/parity.md#tables) | 82% | ✅ |  |
| [Table formulas](../../docs/parity.md#table-formulas) | 83% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 82% | ✅ |  |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 67% | ✅ |  |
| [Citations](../../docs/parity.md#citations) | 100% | ✅ |  |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ✅ |  |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ✅ |  |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ✅ |  |
| [Timers](../../docs/parity.md#timers) | 80% | ✅ |  |
| [Attachments](../../docs/parity.md#attachments) | 100% | ✅ |  |
| [Encryption](../../docs/parity.md#encryption) | 0% | — |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 0% | — |  |
| **All of Org** | **86%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->

## Commands

All commands are in the Command Palette under **Org**.

| Command | What it does |
| --- | --- |
| `Org: Cycle the visibility of the subtree, block or drawer at the cursor` | Org's `TAB` (`Tab`): folded, its children, all of it |
| `Org: Cycle the visibility of the whole file` | Org's `S-TAB` (`Shift+Tab`): the top level, every headline, everything |
| `Org: Show the keys` | Lists the keys bound in the editor at hand, with the preset `org-lsp.keys` picks; picking one runs it |
| `Org: Agenda for the current week or day` | Opens the agenda |
| `Org: List of all TODO entries` | Opens the TODO list |
| `Org: Agenda commands` | Asks which of the two to open |
| `Org: Change the TODO state` | Org's `C-c C-t` (`t` in the agenda): with keys on the keywords (`TODO(t)`), pick one from a list; else the next state. A state with `@` asks for its note |
| `Org: Cycle TODO state forward` / `backward` | Changes the entry under the cursor, in an Org file or on an agenda line |
| `Org: Raise the priority` / `Lower the priority` | The same, for the priority |
| `Org: Capture` | Org's `org-capture`: pick a template, answer its prompts, and edit the entry in a capture buffer; `Ctrl+C Ctrl+C` there files it, `Ctrl+C Ctrl+K` gives up, `Ctrl+C Ctrl+W` refiles it. Templates are `org-lsp.settings.captureTemplates` |
| `Org: Refile the entry` | Org's `C-c C-w` (also in the agenda): pick a target from a list; see `org_refile_targets` |
| `Org: Set the priority` | Asks for it, as Org's `C-c ,` does (`,` in the agenda); a space removes it |
| `Org: Increase` / `Decrease the part of the timestamp at the cursor` | The year, month, day, hour or minute under the cursor, as Org's `S-<up>` / `S-<down>` |
| `Org: Move the date one day later` / `earlier` | The timestamp under the cursor, or the date of an agenda line |
| `Org: Increase the timestamp part or raise the priority`, and the three like it | Org's `S-<up>`, `S-<down>`, `S-<left>`, `S-<right>`: what they do depends on where the cursor is |
| `Org: Schedule the entry`, `Org: Set the deadline of the entry` | Ask for a date and set it, in an Org file or on an agenda line (`Ctrl+C Ctrl+S`, `Ctrl+C Ctrl+D` there) |
| `Org: Insert or change a timestamp` / `an inactive timestamp` | Ask for a date and insert it at the cursor, or change the timestamp there |
| `Org: Open the link or thing at the cursor` | Org's `C-c C-o` (also in the agenda): follows a link to a headline, target, ID or file, opens a URL in the browser, a timestamp or tag in the agenda; on a headline, picks one of the entry's links |
| `Org: Add a note to the entry` | Org's `C-c C-z` (`z` in the agenda): asks for the note and files it under the heading, as `org-add-note` does |
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

Emacs's keys are bound in Org files (`Ctrl+C Ctrl+T`, `Ctrl+C Ctrl+S`,
`Shift+arrows`, `Tab` and `Shift+Tab` to cycle folds, …), in capture
buffers (`Ctrl+C Ctrl+C`), and the date prompt. The agenda has the same
keys in every editor and with every `org-lsp.keys`: Emacs's agenda keys
(`f`, `b`, `t`, `+`, `Enter`, `Ctrl+C Ctrl+S`, …), with `Shift+J` for a
date and the views under `v` (`v d`, `v w`), so that VSCodeVim keeps its
motions; `q` closes the agenda and `g ?` lists its keys. In Org files `Ctrl+C` waits
for a second key instead of copying (`Ctrl+Insert` still copies). After a
change of priority or date in the agenda, the cursor goes with the entry
to its new line.

`org-lsp.keys` picks other keys: `vim` binds the Neovim plugin's keys for
VSCodeVim (those VS Code can bind without taking them from Vim, in Normal
mode), and `custom` binds none, for keys of your own in
`keybindings.json`:

```json
{ "key": "alt+t", "command": "org.todo", "when": "editorTextFocus && editorLangId == 'org'" },
{ "key": "alt+up", "command": "org.shift.up", "when": "editorTextFocus && editorLangId == 'org'" }
```

[docs/vscode/keys.md](https://github.com/kristijanhusak/org-lsp/blob/main/docs/vscode/keys.md) lists every key and
command, and how to add, change and remove keys.

## Settings

| Setting | Meaning |
| --- | --- |
| `org-lsp.settings` | The server's options, for example `{ "org_agenda_files": ["~/org"], "org_log_done": "time" }`. [docs/vscode/configuration.md](https://github.com/kristijanhusak/org-lsp/blob/main/docs/vscode/configuration.md) lists them with their defaults. |
| `org-lsp.server.path` | Path to the server binary; see below. |
| `org-lsp.conceal` | Hide link brackets and targets, and emphasis markers with `org_hide_emphasis_markers`, except on the line with the cursor (default on). |
| `org-lsp.keys` | Which keys are bound: `emacs` (the default), `vim` (for VSCodeVim) or `custom` (none). |
| `org-lsp.winSplitMode` | Where the agenda and capture buffers open: `split` (below, the default), `vsplit` (beside), `float` (a window of its own) or `current`. |

Without `org_agenda_files`, the agenda is built from the Org files in the
workspace folders.

A change made from the agenda edits the entry's file and saves it
(`auto_save` in `org-lsp.settings`; off, it is left unsaved, as in Emacs);
the agenda follows at once.

## The server binary

The extension looks for the server in this order:

1. the `org-lsp.server.path` setting,
2. `server/org-lsp` inside the extension (present in packaged builds),
3. `org-lsp` on `PATH`.

## Building

`scripts/install-vscode.sh --source` (in the repository root) does all of
this and installs the result. By hand:

```sh
cargo build --release -p org-lsp   # in the repository root
npm install
npm run generate                   # package.json from the server's manifest
npm run bundle-server              # copies the binary into server/
npm run package                    # writes org-lsp-<version>.vsix
```

`npm test` runs the extension inside VS Code against the server
(`xvfb-run -a npm test` on a machine without a display).
