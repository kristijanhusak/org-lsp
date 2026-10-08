# Org for Vim

The Vim plugin (this folder) runs Org on top of an LSP client:
yegappan/lsp, vim-lsp, coc.nvim or ALE. It needs Vim 9.1.

- [docs/vim/README.md](../../docs/vim/README.md): installing it, what
  each client needs
- [docs/vim/keys.md](../../docs/vim/keys.md): every key, and how to
  change it
- [docs/vim/configuration.md](../../docs/vim/configuration.md): the
  plugin's options and the server's, with their defaults
- [TESTING.md](TESTING.md): what to try by hand, once with each client

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Vim | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 100% | ❓ | Links, emphasis markers, pretty entities and sub- and superscripts are not concealed: Vim cannot hide text that has text properties. |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 100% | ✅ |  |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ✅ |  |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 100% | ✅ |  |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ✅ |  |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ✅ |  |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 100% | ❓ | Column view is left out: it draws over the headlines, which Vim cannot. |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ✅ |  |
| [Agenda](../../docs/parity.md#agenda) | 100% | ❓ | Column view in the agenda is left out: it draws over the lines, which Vim cannot. |
| [Clocking](../../docs/parity.md#clocking) | 100% | ✅ |  |
| [Capture](../../docs/parity.md#capture) | 100% | ✅ |  |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 100% | ✅ |  |
| [Links](../../docs/parity.md#links) | 100% | ✅ |  |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ✅ |  |
| [Tables](../../docs/parity.md#tables) | 94% | ❓ | A shrunk column shows its whole text and `…`: Vim cannot hide text that has text properties. |
| [Table formulas](../../docs/parity.md#table-formulas) | 100% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 100% | ✅ |  |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 100% | ✅ |  |
| [Citations](../../docs/parity.md#citations) | 100% | ✅ |  |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ❓ | New lines are not indented by the server (no `indentexpr`), and wrapped lines are not. |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ✅ |  |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ✅ |  |
| [Timers](../../docs/parity.md#timers) | 100% | ✅ |  |
| [Attachments](../../docs/parity.md#attachments) | 100% | ✅ |  |
| [Encryption](../../docs/parity.md#encryption) | 100% | ✅ |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 67% | ❌ | Vim cannot show images. |
| **All of Org** | **98%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->
