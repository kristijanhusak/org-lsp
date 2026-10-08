# Org for Zed

A Zed extension that runs Org through the org-lsp server.

## Install

1. Install Rust with [rustup](https://rustup.rs): Zed builds dev extensions
   with it.
2. In Zed, run **zed: install dev extension** and pick this folder
   (`editors/zed` of a checkout). Zed builds the extension and installs it.
3. Add to your settings (**zed: open settings**):

   ```json
   {
     "languages": {
       "Org": { "semantic_tokens": "combined", "inlay_hints": { "enabled": true } }
     },
     "lsp": {
       "org-lsp": {
         "initialization_options": { "org_agenda_files": ["~/org"] }
       }
     }
   }
   ```

The extension starts `org-lsp` from your PATH (`cargo install --path
server/org-lsp` in a checkout puts it in `~/.cargo/bin`), or else
downloads the nightly build for your platform once. The download needs
the repository to be public: GitHub answers 404 to Zed for a private
one. `lsp.org-lsp.binary.path` names a server binary of your own:

```json
{ "lsp": { "org-lsp": { "binary": { "path": "/path/to/org-lsp/target/release/org-lsp" } } } }
```

Zed asks a language server for semantic tokens only when a language's
`semantic_tokens` says so. `combined` colors Org files with the
tree-sitter grammar and the server's tokens on top; `full` uses the
server's tokens alone, without the grammar's colors.

Source and export blocks (`#+begin_src python`, `#+begin_export html`)
show their code in its language's colors, through the grammar's
injections, with `combined` (with `full` there is no grammar to inject
into). The grammar has no node for inline source blocks
(`src_python{…}`), so their code is not colored.

Files with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`)
show indented by level, without being changed, through inlay hints of
blanks before their lines, when Zed shows inlay hints for Org:

```json
{ "languages": { "Org": { "inlay_hints": { "enabled": true } } } }
```

The leading stars still show; Zed cannot hide them.

`initialization_options` are the server's options
([docs/configuration.md](../../docs/configuration.md)); the shared
`~/.config/org-lsp/config.toml` is read too.

## Commands

Zed cannot run a language server's command from a key, so the commands
are in the **code actions** (**editor: toggle code actions**, `ctrl-.` /
`cmd-.`), which list what applies where the cursor is:

- In an Org file: change the TODO state (the next one, the previous one,
  or one picked from a list), the priority, the timestamp under the
  cursor; toggle a checkbox; a new heading, item or table row (as M-RET,
  M-S-RET, C-RET and C-S-RET make them), the cursor then in it; clock in
  and out; refile; archive; open the agenda (this week, the TODO list, or
  the agenda menu); capture.
- In a capture buffer: file the entry, give up, or refile it.

The agenda is a read-only file the server writes (`Agenda.org-agenda`, in
a directory of its own under the system's temporary one), opened as Org;
the server writes it again when it changes. Its commands have keys
instead.

### The agenda's keys

The agenda has the same keys in every editor: Emacs's agenda keys, with
`J` for a date and the views under `v` (`v d`, `v w`), so that Vim mode
keeps its motions. An extension cannot bind keys, and a key of Zed's
cannot run a language server's command, but it can run a task. Two files
set that up, once:

1. [tasks.json](tasks.json): a task for each agenda command (*Org agenda:
   …*). Paste its entries into your tasks (**zed: open tasks**,
   `~/.config/zed/tasks.json`).
2. [keymap.json](keymap.json): binds the agenda's keys to those tasks.
   Paste its block into your keymap (**zed: open keymap**).

Then in the agenda `f` / `b` move it through time, `.` goes to today,
`v d` / `v w` change the view, `t` changes the TODO state, `ctrl-c
ctrl-s` schedules, and so on, at once; `enter` goes to the entry of the
line (`tab` in a split), and `q` closes the agenda. A task's terminal
stays out of sight unless the command fails.

The tasks are yours rather than the extension's because Zed offers an
extension's tasks only in files of the project's folders, and the agenda
is a file of the server's own, elsewhere: a key bound to one would open
the task picker instead. Each task runs
`~/.local/state/org-lsp/agenda-command`, a script the server writes,
which runs the command in the agenda where the cursor is: the server
follows the cursor through the document highlights Zed asks for when it
moves (after `lsp_highlight_debounce`, 75 milliseconds). With agendas
open in several windows, the keys act on the one the cursor was last in.
On Windows, where the script is `agenda-command.cmd` under your user
folder and the shell is PowerShell, make each task's command
`& "$HOME/.local/state/org-lsp/agenda-command.cmd" org.agenda.later`
(with its own command at the end).

What a command asks for is asked with buttons: a TODO state, a priority,
a refile target, a capture template, yes or no. Zed has no way to ask for
typed text, so the commands that want a date (schedule, deadline) or text
(a template's `%^{…}` prompts, a tags search) say so and do nothing; set
those dates in the file itself, where completion helps.

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Zed | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 92% | ❓ | Links, emphasis markers and pretty entities are not concealed: Zed lets no server hide text. The colors need `"semantic_tokens": "combined"`; code blocks take their language's colors, inline code does not. |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 75% | ❓ | Zed folds with its own folding: Org's cycling and sparse trees need folds an extension can set, which Zed does not offer. |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ❓ | Moving between headings, `org-goto` and the commands run by Emacs (`C-c *`, `C-c ;`, cloning) are left out: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them. |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 100% | ❓ | `RET` stays Zed's own, bullets are not cycled and lines not turned into items: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them. |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ❓ | A note for a state change (`@`) cannot be typed: Zed cannot ask for text on a server's behalf. States are picked with buttons. |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ❓ | Setting tags is left out: it needs keys pressed in turn, or typed tags, and Zed cannot ask for text on a server's behalf. Completion and renaming work. |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 83% | ❓ | Setting a property or the effort is left out: they are typed, and Zed cannot ask for text on a server's behalf. |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ❓ | Scheduling, deadlines and new timestamps are left out: they ask for a date, and Zed cannot ask for text on a server's behalf. Changing a timestamp works. |
| [Agenda](../../docs/parity.md#agenda) | 95% | ❓ | The agenda is a file, its commands code actions (or keys through Zed's tasks); searches, going to a date and dates asked for are left out (Zed cannot ask for text on a server's behalf), and follow mode (Zed reports no cursor moves to an extension). |
| [Clocking](../../docs/parity.md#clocking) | 100% | ❓ | No clock in the status line: Zed has no place there for an extension's text. |
| [Capture](../../docs/parity.md#capture) | 100% | ❓ | Templates with prompts are left out, and there is no capture buffer to type in: Zed cannot ask for text on a server's behalf. |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 100% | ✅ |  |
| [Links](../../docs/parity.md#links) | 100% | ❓ | Storing and inserting links are left out: they ask for text, and Zed cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition, for links inside Org files and to files. |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ❓ | Making footnotes and their menu are left out: they are not among Zed's code actions. Following them works. |
| [Tables](../../docs/parity.md#tables) | 88% | ❓ | Moving between fields with `TAB`, copying down, summing, field information and creating a table are left out: Zed cannot bind a key to a server's command (the commands are code actions), and those are not among them. The other edits are code actions in a table. |
| [Table formulas](../../docs/parity.md#table-formulas) | 92% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 91% | ❓ | Only tangling, the block at the cursor and editing it in a file of its own are offered (code actions in a block): the file's and the subtree's evaluation, removing results and moving between blocks need keys, which Zed cannot give a server. |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 67% | ❌ | Not offered as a code action yet; its menu would come as buttons. |
| [Citations](../../docs/parity.md#citations) | 100% | ❓ | Inserting citations is not offered as a code action yet. Completion, hover and following work. |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ❓ | Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed. |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ❌ | A note is typed text: Zed cannot ask for text on a server's behalf. |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ❓ | Structure templates, emphasis and drawers are not among Zed's code actions. |
| [Timers](../../docs/parity.md#timers) | 80% | ❌ | Timers are left out: Zed cannot bind a key to a server's command (the commands are code actions), and they are not among them. |
| [Attachments](../../docs/parity.md#attachments) | 100% | ❓ | `attachment:` links open; the dispatcher (`C-c C-a`) is left out: Zed cannot bind a key to a server's command (the commands are code actions), and it is not among them. |
| [Encryption](../../docs/parity.md#encryption) | 100% | ✅ |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 0% | — |  |
| **All of Org** | **91%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->

Bind `editor::ToggleCodeActions` to a key if the code actions menu is too
far away; the keymap and tasks above give the agenda keys of its own.

## Development

`node editors/zed/generate.mjs` writes the semantic token rules
(`languages/org/semantic_token_rules.json`), `tasks.json` and
`keymap.json` from the server's manifest; `--check` fails if they are out of date. `cargo build --target
wasm32-wasip2` in this folder builds the extension.
