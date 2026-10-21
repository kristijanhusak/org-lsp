# End-to-end testing round, 2026-10-21

The log of the round asked for on 2026-10-21: every feature in every
editor, as end to end as possible, the new Emacs work (`emacs_options`,
`docs/emacs-options.org`), and LSP alone with no editor code. It is
committed on `claude/elegant-meitner-ggxbid` as it grows, so that the round
can be picked up where it stopped.

Base: `main` at bad79cc (#365).

## How to pick it up

1. Read the plan and the state of each part below.
2. The issues are in `issues.md`, one heading each, with what was done,
   how to reproduce it, and its state (open, fixing, fixed in #N, not a
   bug). Notes of each tester are in `notes-<part>.md`.
3. An issue marked open is fixed on its own branch in a worktree, then
   merged as one PR per area, as `CLAUDE.md` says.

## Plan

| Part | What | State |
| --- | --- | --- |
| lsp | A plain LSP client (no `orgClient`, no `experimental`), and one announcing everything, driving every command of the manifest over stdio | tested: lsp-1 to lsp-19 (notes-lsp.md); fixing |
| emacs | `emacs_options`, `emacs_config`, everything that goes through Emacs (export, Babel, attachments, crypt, column view, plot, timers, formula editor, appointments, tangling fallbacks), with and without Emacs | tested: emacs-1 to emacs-19 (notes-emacs.md); fixing |
| neovim | The Neovim suites (0.12.0, stable, nightly), then `editors/neovim/TESTING.org` by script, `vim.pack` install | tested: neovim-1 to neovim-20 (notes-neovim.md); fixing |
| vim-helix | Vim with its four clients, Helix | tested: vim-1 to vim-21, helix-1 to helix-8 (notes-vim-helix.md); fixing |
| vscode-zed-sublime | VS Code (VSCodium), Zed, Sublime Text's tests | tested: vscode-1, zed-1, sublime-1 to sublime-4 (notes-vscode-zed-sublime.md); fixing |

## Rounds of fixes

- emacs-1 to emacs-19, with lsp-9, lsp-10, lsp-19 (the same bridge): merged in #366 (notes-fix-emacs.md). Left: the rest of emacs-11/lsp-10 (the synchronous Emacs calls answered later, plan in notes-fix-emacs.md and docs/protocol.org) and fix-1, for an agent once disk allows.
- lsp-1 to lsp-18 but lsp-9, lsp-10: merged in #367 (notes-fix-lsp.md).
- neovim-*, vim-*, helix-*, and Zed's Save All: merged in #368 (notes-fix-editors.md).
- vscode-1, zed-1, sublime-1 to sublime-4, fix-1, and the rest of emacs-11/lsp-10: done (notes-fix-rest.md), on branch worktree-agent-a92ff95f01afdaa8f; merged after the editors' PR.

All five parts are tested; what is left is fixing, then one PR per fix agent.

## 10:10 UTC: the usage limit stopped the three fix agents

Their work so far is saved in `wip/` (the editors fixer's uncommitted
changes as a diff on the commit in `fix-editors-base.txt`; the other two as
`git format-patch` series) and they were resumed from where they stopped.
