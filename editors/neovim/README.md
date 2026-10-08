# Org for Neovim

The Neovim plugin is the repository itself (`lua/`, `plugin/`): install
`kristijanhusak/org-lsp` with your plugin manager. The main
[README](../../README.md#neovim-011-or-newer) says how; Neovim 0.11 or newer.

- [docs/neovim/configuration.md](../../docs/neovim/configuration.md): the
  plugin's options and the server's
- [docs/neovim/keys.md](../../docs/neovim/keys.md): every key, by where it
  works
- [TESTING.md](TESTING.md): what to try by hand

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Neovim | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 92% | ✅ |  |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 67% | ✅ |  |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ✅ |  |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 91% | ✅ |  |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ✅ |  |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ✅ |  |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 67% | ✅ |  |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ✅ |  |
| [Agenda](../../docs/parity.md#agenda) | 81% | ✅ |  |
| [Clocking](../../docs/parity.md#clocking) | 100% | ✅ |  |
| [Capture](../../docs/parity.md#capture) | 82% | ✅ |  |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 82% | ✅ |  |
| [Links](../../docs/parity.md#links) | 91% | ✅ |  |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ✅ |  |
| [Tables](../../docs/parity.md#tables) | 65% | ✅ |  |
| [Table formulas](../../docs/parity.md#table-formulas) | 83% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 82% | ✅ |  |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 67% | ✅ |  |
| [Citations](../../docs/parity.md#citations) | 100% | ✅ |  |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ✅ |  |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ✅ |  |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ✅ | Completion needs an LSP completion plugin (or `vim.lsp.completion`). |
| [Timers](../../docs/parity.md#timers) | 60% | ✅ |  |
| [Attachments](../../docs/parity.md#attachments) | 0% | — |  |
| [Encryption](../../docs/parity.md#encryption) | 0% | — |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 0% | — |  |
| **All of Org** | **78%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->
