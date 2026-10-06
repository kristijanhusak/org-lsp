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
         "initialization_options": { "agendaFiles": ["~/org"] }
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

Files with `org-indent-mode` (`startupIndented`, `#+STARTUP: indent`)
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
cannot run a language server's command, but it can run a task. The
extension has a task for each agenda command (**task: spawn** lists them
as *Org agenda: …*), which runs it where the cursor is, through a script
the server writes next to the agenda. [keymap.json](keymap.json) binds
the keys to them: paste its block into your keymap (**zed: open
keymap**). Then in the agenda `f` / `b` move it through time, `.` goes
to today, `v d` / `v w` change the view, `t` changes the TODO state,
`ctrl-c ctrl-s` schedules, and so on; `enter` goes to the entry of the
line (`tab` in a split), and `q` closes the agenda. A task's terminal
stays out of sight unless the command fails.

What a command asks for is asked with buttons: a TODO state, a priority,
a refile target, a capture template, yes or no. Zed has no way to ask for
typed text, so the commands that want a date (schedule, deadline) or text
(a template's `%^{…}` prompts, a tags search) say so and do nothing; set
those dates in the file itself, where completion helps.

## What works

- Highlighting from the server, completion, the outline, formatting
  (**editor: format**: tables aligned, tags in their column; on save if
  `format_on_save` is on), and the server's indentation of a selection.
- The commands above.

Not there: the date prompt's calendar, the clock in the status bar, and
keys of their own for the commands in Org files (bind
`editor::ToggleCodeActions` to a key if the menu is too far away).

## Development

`node editors/zed/generate.mjs` writes the semantic token rules
(`languages/org/semantic_token_rules.json`), the agenda's tasks
(`languages/org/tasks.json`) and `keymap.json` from the server's
manifest; `--check` fails if they are out of date. `cargo build --target
wasm32-wasip2` in this folder builds the extension.
