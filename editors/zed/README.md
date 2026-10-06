# Org for Zed

A Zed extension that runs Org through the org-lsp server.

## Install

In Zed, run **zed: install dev extension** and pick this folder
(`editors/zed`); Zed builds it (with Rust from rustup) and installs it.
The extension starts `org-lsp` from your PATH, or else downloads the
nightly build for your platform once.

Zed asks language servers for semantic tokens only when told to; the
server's highlighting needs, in your settings:

```json
{
  "languages": {
    "Org": { "semantic_tokens": "full" }
  },
  "lsp": {
    "org-lsp": {
      "initialization_options": { "agendaFiles": ["~/org"] }
    }
  }
}
```

`initialization_options` are the server's options
([docs/configuration.md](../../docs/configuration.md)); the shared
`~/.config/org-lsp/config.toml` is read too. `lsp.org-lsp.binary.path`
names a server binary of your own.

## What works

- Highlighting from the server (with `semantic_tokens`); without it, the
  tree-sitter grammar (tree-sitter-org) marks headlines, tags, drawers,
  blocks and comments.
- The outline, completion, formatting (**editor: format**: tables aligned,
  tags in their column; on save if `format_on_save` is on), and the
  server's indentation of a selection.
- The editing commands in the code actions (**editor: toggle code
  actions**, `ctrl-.` / `cmd-.`): change the TODO state, the priority, the
  timestamp under the cursor, toggle a checkbox, clock in and out.

What does not: Zed cannot bind a key to one of the server's commands, or
ask for text on a server's behalf, so the agenda, capture, refile and
the commands that ask for a date are not available, nor is the clock in
the status bar.

## Development

`node editors/zed/generate.mjs` writes the semantic token rules
(`languages/*/semantic_token_rules.json`) from the server's manifest;
`--check` fails if they are out of date. `cargo build --target
wasm32-wasip2` in this folder builds the extension.
