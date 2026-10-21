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
| neovim | The Neovim suites (0.12.0, stable, nightly), then `editors/neovim/TESTING.org` by script, `vim.pack` install | started |
| vim-helix | Vim with its four clients, Helix | started |
| vscode-zed-sublime | VS Code (VSCodium), Zed, Sublime Text's tests | waiting |

## Rounds of fixes

- emacs-1 to emacs-19, with lsp-9, lsp-10, lsp-19 (the same bridge): a fix agent in a worktree (notes-fix-emacs.md), one PR when done.
- lsp-1 to lsp-18 but lsp-9, lsp-10: a second fix agent (notes-fix-lsp.md), one PR when done.
