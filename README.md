# org-lsp

A language server for [Org mode](https://orgmode.org) documents, so that Org
works in any editor with an LSP client instead of in one editor at a time.

The server owns everything about Org: parsing, highlighting, structure and
edits. An editor needs only a thin shim (start the server, map highlight
groups, bind keys), and editors without a plugin system still get whatever
standard LSP can carry.

**Status: first milestone.** The parser is complete enough to match Emacs on
the test corpus; the server exposes a first slice of features on top of it.

## What works

| Feature | How it is exposed | Neovim | VS Code | Helix |
| --- | --- | --- | --- | --- |
| Highlighting | semantic tokens | yes | yes¹ | no² |
| Folding | folding ranges | yes, with `<Tab>` / `<S-Tab>` cycling | yes¹ | no³ |
| Outline | document symbols | yes | yes¹ | yes |
| TODO cycling | code actions `org.todo.next` / `org.todo.previous` | yes, `cit` / `ciT`, repeatable with `.` | yes¹, commands and `Ctrl+.` | yes, `Space a` |

¹ The extension builds, packages and passes a protocol smoke test, but has
not been run inside VS Code yet.
² Helix does not consume semantic tokens; it highlights Org with its own grammar.
³ Helix has no folding.

Tested with Neovim 0.11.0 and 0.12.5 and Helix 25.07.1.

## Install

The server is a single binary, `org-lsp`. There are no prebuilt binaries yet,
so each route below builds it with [cargo](https://rustup.rs).

### Neovim (0.11 or newer)

Install the repository as a plugin and build the server. With lazy.nvim:

```lua
{ 'kristijanhusak/org-lsp', build = 'cargo build --release -p org-lsp' }
```

With any other plugin manager, install the repository and run `:OrgLspBuild`
once. Nothing else is required: opening an `.org` file starts the server.
`:checkhealth org-lsp` reports what the plugin found.

The plugin sets buffer-local keys in Org files, so it is meant to be used
instead of nvim-orgmode for those buffers, not next to it.

Options, with their defaults:

```lua
require('org-lsp').setup({
  cmd = nil,                          -- server command; found automatically
  todo_keywords = nil,                -- e.g. { 'TODO', 'NEXT', '|', 'DONE' }
  startup_folded = 'showeverything',  -- or 'overview'
  folding = true,                     -- fold with the server's ranges
  replace_syntax = true,              -- turn off regex syntax once tokens arrive
  mappings = {                        -- false disables all of them
    todo_next = 'cit',
    todo_previous = 'ciT',
    cycle = '<Tab>',
    global_cycle = '<S-Tab>',
  },
})
```

Highlight groups are documented in `lua/org-lsp/highlights.lua`. Colorschemes
that style nvim-orgmode's `@org.*` groups apply here too.

### VS Code

```sh
cargo build --release -p org-lsp
cd editors/vscode
npm install
npm run bundle-server
npm run package        # writes org-lsp-0.1.0.vsix
code --install-extension org-lsp-0.1.0.vsix
```

See `editors/vscode/README.md` for the commands and settings.

### Helix

```sh
cargo install --path server/org-lsp
```

Then add the contents of `editors/helix/languages.toml` to your
`languages.toml`.

### Other editors

Point the editor's LSP client at `org-lsp` for the `org` file type. The
server talks over standard input and output. `docs/protocol.md` lists the
token legend, the code action kinds and the initialization options.

## Layout

```
plugin/, lua/        Neovim shim (at the root so plugin managers find it)
server/org-syntax/   the Org parser, a library with no LSP in it
server/org-lsp/      the language server
editors/vscode/      VS Code extension
editors/helix/       Helix configuration
docs/                protocol and parser notes
tests/fixtures/      Org files with reference dumps from Emacs
tests/nvim/          Neovim end-to-end test
scripts/             fixture, fuzzing and test helpers
```

## Development

```sh
cargo test --workspace          # parser, server and protocol tests
scripts/test-nvim.sh            # Neovim shim against the real server
cd editors/vscode && npm run check && npm run smoke
```

The parser is checked against Emacs. Every file in `tests/fixtures/` has a
`.elements` dump produced by `org-element-parse-buffer`, and the parser must
find the same elements and objects at the same positions. After changing a
fixture, regenerate the dumps (needs Emacs):

```sh
scripts/update-fixtures.sh
```

`scripts/fuzz-against-emacs.py` generates random documents and compares both
parsers on them. See `docs/parser.md`.

## Not done yet

- Everything beyond the four features above: agenda, capture, refile, clocking,
  tables, completion, links, export, and so on.
- TODO cycling only changes the keyword. It does not log `CLOSED:`, shift
  repeaters or realign tags.
- No prebuilt binaries, and no Vim or Zed integration.
- Source blocks get no highlighting of their own; that needs editor support
  for embedded languages.
