# Org for Sublime Text (LSP-org)

`LSP-org` is a package for [sublimelsp/LSP](https://github.com/sublimelsp/LSP)
that runs Org through the org-lsp server.

## Install

1. Install the LSP package (Package Control: *LSP*).
2. Copy or link `editors/sublime/LSP-org` into Sublime Text's `Packages`
   folder (*Preferences → Browse Packages…*), keeping the name `LSP-org`.
3. Put `org-lsp` on your PATH, or set `command` in
   *Preferences → Package Settings → LSP → Servers → LSP-org*.
4. Turn on LSP's `"semantic_highlighting": true` for the server's
   highlighting.

## Settings

`LSP-org.sublime-settings`:

| Setting | Meaning |
| --- | --- |
| `command` | The server command. |
| `initializationOptions` | The server's options, such as `{"agendaFiles": ["~/org"]}`; see [docs/configuration.md](../../docs/configuration.md). The shared `~/.config/org-lsp/config.toml` is read too. |
| `org_keys` | The keys: `emacs` (the default), `vim` (for Vintage and NeoVintageous, in command mode) or `custom` (none). |
| `semantic_tokens` | The scopes of the server's tokens; generated. |

## Keys

With `emacs`, Org files get Emacs's keys: `Ctrl+C Ctrl+T` changes the TODO
state, `Shift+arrows` change the priority, the timestamp under the cursor
or the TODO state, `Ctrl+C Ctrl+S` schedules, `Ctrl+C Ctrl+W` refiles,
`Ctrl+C c` captures, `Ctrl+C a` opens the agenda menu, `Tab` and
`Shift+Tab` cycle folds as Org's TAB and S-TAB do. In the agenda Org's
single letters work (`f`, `b`, `t`, `+`, `Enter`). In a capture buffer,
`Ctrl+C Ctrl+C` files the entry. Where a command does not apply,
`Shift+arrows` select and `Tab` indents as usual.

With `vim`, the Neovim plugin's keys that do not start with a plain letter
or the leader are bound in command mode (`Tab`, `Ctrl+Space`, `Ctrl+A`,
…), and the agenda's keys. Bind the others yourself.

`Default.sublime-keymap` has every binding, and the Command Palette lists
every command under *Org:*. To bind one yourself:

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
