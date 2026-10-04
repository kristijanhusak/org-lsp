# Org (org-lsp) for VS Code

Org mode support through the org-lsp language server:
highlighting, folding, the outline, and TODO cycling.

The extension is a thin client. It starts the server, declares the `org`
language and the server's semantic tokens, and exposes two commands:

| Command | What it does |
| --- | --- |
| `Org: Next TODO State` | Cycles the entry under the cursor forward |
| `Org: Previous TODO State` | Cycles it backward |

Both are also listed under **Quick Fix…** (`Ctrl+.`). No keys are bound by
default; add your own in `keybindings.json`:

```json
{ "key": "alt+t", "command": "org-lsp.todoNext", "when": "editorTextFocus && editorLangId == org" }
```

## The server binary

The extension looks for the server in this order:

1. the `org-lsp.server.path` setting,
2. `server/org-lsp` inside the extension (present in packaged builds),
3. `org-lsp` on `PATH`.

## Building

```sh
cargo build --release -p org-lsp   # in the repository root
npm install
npm run bundle-server              # copies the binary into server/
npm run package                    # writes org-lsp-<version>.vsix
```
