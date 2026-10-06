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
       "Org": { "semantic_tokens": "combined" },
       "Org Agenda": { "semantic_tokens": "combined" }
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
- In the agenda: move it through time (later, earlier, today, day,
  week), rebuild it, log mode and the clock report; on an entry's line
  also its state, priority and dates, clocking, refile, archive, and going
  to the entry.
- In a capture buffer: file the entry, give up, or refile it.

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
keys of their own for the commands (bind `editor::ToggleCodeActions` to a
key if the menu is too far away).

## Development

`node editors/zed/generate.mjs` writes the semantic token rules
(`languages/*/semantic_token_rules.json`) from the server's manifest;
`--check` fails if they are out of date. `cargo build --target
wasm32-wasip2` in this folder builds the extension.
