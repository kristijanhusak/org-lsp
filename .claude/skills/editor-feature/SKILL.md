---
name: editor-feature
description: Roll a server feature out to every editor org-lsp supports (Neovim, Vim, VS Code, Sublime Text, Zed, Helix) with its protocol, docs, parity table and tests. Use when a feature needs editor code (something shown, a notification handled, a new client capability) or when bringing an editor up to parity with the others.
---

# Rolling a feature out to every editor

Most features need no editor code: a command and its keys reach every
editor through the server's manifest and the generators. Editor code is
for what LSP cannot express (images, virtual indentation, column view, a
date prompt). Keep it glue: the server decides, the editor shows.

## The server side first

1. **Capability.** The client announces it with
   `capabilities.experimental.orgNAME: true`; read it in `server.rs`
   (`Client`). For clients whose LSP plugin cannot announce one (the
   Vim plugin, Sublime Text's LSP), add an option of the same meaning in
   `settings.rs` (an "integration" setting) and `config.rs`, and accept
   either.
2. **What travels.** A command's answer for what follows from the command
   (`{ "preview": … }`), a notification `org-lsp/NAME` for what comes later
   or from elsewhere. Describe both in `docs/protocol.md`.
3. Without the capability the server says why (`This editor cannot show
   images`) or falls back (inlay hints instead of virtual indentation).

## Each editor

Do them in this order; each has tests that run against the real server.

| Editor | Where | Tests |
| --- | --- | --- |
| Neovim | `lua/org-lsp/` (handlers in `view.lua`, attach in `init.lua`) | `tests/nvim/*.lua` via `scripts/test-nvim.sh` |
| Vim | `editors/vim/autoload/org_lsp.vim`, and each adapter in `editors/vim/autoload/org_lsp/adapter/` (yegappan/lsp, vim-lsp, coc.nvim, ALE) | `editors/vim/test/test.vim` via `editors/vim/test/run.sh` |
| VS Code | `editors/vscode/src/extension.ts`: capability in `registerFeature`, notifications after `client.start()`, results in the `executeCommand` middleware's `handle`; an `Api` getter for tests | `editors/vscode/test/extension.test.cjs` |
| Sublime Text | `editors/sublime/LSP-org/plugin.py`: options in `on_pre_start`, notifications as `m_org_lsp_NAME`, results in `OrgLspRunCommand.done` | `editors/sublime/test/test_plugin.py` (stand-ins for Sublime and LSP) |
| Zed, Helix | `editors/zed`, `editors/helix`: usually nothing can be done; say so | |

Match Emacs's behaviour and messages in each, and match the other editors:
the same toggling, the same options, the same keys (Vim and Neovim share
theirs). Where an editor cannot do something, do the nearest thing and
write the limit down.

## Docs and parity

- `scripts/features.mjs`: each editor's support (`yes`, `part` with what is
  missing and why, `no` with why) and the manual checks (`only:` the
  editors they apply to). It writes `docs/parity.md`, the README tables and
  `editors/*/TESTING.md`.
- Each editor's `docs/<editor>/configuration.md` for new settings; VS Code's
  own settings go in the hand-written `configuration` section of
  `editors/vscode/package.json` too, and new Sublime settings in
  `editors/sublime/generate.mjs` (the list of user values it keeps).
- `scripts/regen.sh`, then `scripts/check.sh` and `scripts/test-all.sh`.

## Seeing it

Tests prove the protocol; look at the result too when it is visual. VS
Code: run a throwaway mocha suite through a copy of
`editors/vscode/test/run.mjs` under `Xvfb :N` and capture the screen with
`import -display :N -window root shot.png` (ImageMagick). Delete the
throwaway files before committing.
