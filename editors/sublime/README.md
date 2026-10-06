# Org for Sublime Text (LSP-org)

`LSP-org` is a package for [sublimelsp/LSP](https://github.com/sublimelsp/LSP)
that runs Org through the org-lsp server.

## Install

From a checkout of this repository:

```sh
scripts/install-sublime.sh          # links editors/sublime/LSP-org into Packages
scripts/install-sublime.sh --copy   # or copies it there
```

It finds Sublime Text's `Packages` folder (set `SUBLIME_PACKAGES` when it
cannot), puts the package there as `LSP-org`, and adds
[LSP](https://github.com/sublimelsp/LSP) to Package Control's packages, so
that it is installed when Sublime Text starts. Then restart Sublime Text
and open an `.org` file.

By hand: install LSP with Package Control, and copy or link
`editors/sublime/LSP-org` into the `Packages` folder (*Preferences →
Browse Packages…*), keeping the name `LSP-org`.

**The server.** Nothing to do: the package starts the first of

1. the `command` set in the settings, when it is not the default
   `["org-lsp"]`;
2. `org-lsp` on PATH, or in `~/.cargo/bin`, `~/.local/bin`,
   `/usr/local/bin` or `/opt/homebrew/bin` (Sublime Text started from a
   desktop launcher often has a shorter PATH than a terminal);
3. the build in the checkout the package is linked from
   (`target/release/org-lsp`, then `target/debug/org-lsp`);
4. the nightly build for your platform, downloaded once into Sublime
   Text's `Package Storage/LSP-org`.

*Org: Download the latest server* fetches the nightly build again and
restarts the server.

**Highlighting.** The package's syntaxes color Org files and the agenda
by their patterns: headlines by level, TODO keywords, priorities, tags,
planning lines and timestamps, drawers and properties, lists and
checkboxes, links, emphasis, tables, blocks and comments, and the code of
source blocks in its language's syntax for the languages Sublime Text
ships (`#+begin_src python`); in the agenda
the days, schedules, deadlines and the time grid. The server's colors
(semantic tokens) go further, with the TODO keywords of your files, the
agenda's today, weekends and overdue items, the clocked entry. LSP draws
them with `"semantic_highlighting": true` (*Preferences → Package
Settings → LSP → Settings*) and a color scheme that allows them: LSP
lets Sublime Text's own schemes do so, and another needs the rule
*UI: Customize Color Scheme* adds, as
[LSP's docs](https://lsp.sublimetext.io/features/#semantic-highlighting)
say:

```json
{ "rules": [{ "scope": "meta.semantic-token", "background": "#00000101" }] }
```

## Settings

*Preferences → Package Settings → LSP → Servers → LSP-org*, or
*Preferences: LSP-org Settings* in the Command Palette:

| Setting | Meaning |
| --- | --- |
| `command` | The server command; by default the package finds or downloads it (see above). |
| `initializationOptions` | The server's options, such as `{"agendaFiles": ["~/org"]}`; see [docs/configuration.md](../../docs/configuration.md). The shared `~/.config/org-lsp/config.toml` is read too. |
| `org_keys` | The keys: `emacs` (the default), `vim` (for Vintage and NeoVintageous, in command mode) or `custom` (none). |
| `semantic_tokens` | The scopes of the server's tokens; generated. |

## Keys

With `emacs`, Org files get Emacs's keys: `Ctrl+C Ctrl+T` changes the TODO
state, `Shift+arrows` change the priority, the timestamp under the cursor
or the TODO state, `Ctrl+C Ctrl+S` schedules, `Ctrl+C Ctrl+W` refiles,
`Ctrl+C c` captures, `Ctrl+C a` opens the agenda menu, `Tab` and
`Shift+Tab` cycle folds as Org's TAB and S-TAB do. In a capture buffer,
`Ctrl+C Ctrl+C` files the entry. Where a command does not apply,
`Shift+arrows` select and `Tab` indents as usual.

With `vim`, the Neovim plugin's keys that do not start with a plain letter
or the leader are bound in command mode (`Tab`, `Ctrl+Space`, `Ctrl+A`,
…). Bind the others yourself.

The agenda has the same keys with either preset, and in every editor:
Emacs's agenda keys (`f`, `b`, `.`, `t`, `+`, `Enter`, `Ctrl+C Ctrl+S`,
…), with `Shift+J` for a date and the views under `v` (`v d`, `v w`), so
that Vintage and NeoVintageous keep their motions; `q` closes the agenda
and `g ?` lists its keys.

`Default.sublime-keymap` has every binding, and the Command Palette lists
every command under *Org:*. *Org: Show the keys* lists the keys bound in
the view at hand. To bind one yourself:

```json
{ "keys": ["alt+t"], "command": "org_lsp_run", "args": {"command": "org.todo"},
  "context": [{"key": "selector", "operand": "text.org"}] }
```

## What it does not do

Dates are typed (`fri 10:00`, `+2d`), with no calendar; the agenda is the
file the server writes, opened read-only; nothing is concealed.

The package has not been tried in Sublime Text itself here: its plugin is
tested against stand-ins for Sublime Text and LSP
(`python3 editors/sublime/test/test_plugin.py`), and its keys and settings
are generated from the server (`node editors/sublime/generate.mjs`).
