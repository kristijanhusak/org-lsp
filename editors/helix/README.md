# Org for Helix

Helix knows the `org` language already; [languages.toml](languages.toml)
attaches the server to it. Add it to `~/.config/helix/languages.toml`,
with `org-lsp` on your PATH (a binary from the
[nightly release](https://github.com/kristijanhusak/org-lsp/releases/tag/nightly),
or `cargo install --path server/org-lsp` in a checkout).

Helix has no plugins, so what org-lsp does comes as standard LSP: the
commands are code actions (`Space a`), and Helix can neither open the
server's documents (the agenda, the capture buffer) nor ask for text on
its behalf.

- [docs/helix/configuration.md](../../docs/helix/configuration.md): the
  server's options
- [docs/helix/keys.md](../../docs/helix/keys.md): the commands
- [TESTING.md](TESTING.md): what to try by hand

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Helix | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 83% | ❌ | Helix does not use semantic tokens: it colors Org with its own tree-sitter grammar. |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 67% | ❌ | Helix has no folding. |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 73% | ❓ | New headings (`M-RET` and its relatives), moving between headings and `org-goto` are left out: they carry the cursor for the editor to move, which Helix's code actions cannot do. |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 73% | ❓ | `RET` stays Helix's own, new items and cycled bullets are left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and those are not among them. |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 83% | ❓ | A note for a state change (`@`) cannot be typed: Helix cannot ask for text on a server's behalf. |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ❓ | Setting tags is left out: it needs keys pressed in turn, or typed tags, and Helix cannot ask for text on a server's behalf. Completion and renaming work. |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 67% | ❓ | Setting a property or the effort is left out: they are typed, and Helix cannot ask for text on a server's behalf. |
| [Dates and times](../../docs/parity.md#dates-and-times) | 86% | ❓ | Scheduling, deadlines and new timestamps are left out: they ask for a date, and Helix cannot ask for text on a server's behalf. Changing a timestamp works. |
| [Agenda](../../docs/parity.md#agenda) | 74% | ❌ | Helix cannot open a document on a server's request. `org-lsp agenda FILE...` prints the agenda in a terminal. |
| [Clocking](../../docs/parity.md#clocking) | 88% | ❓ | No clock in the status line: Helix has no place there for a server's text. |
| [Capture](../../docs/parity.md#capture) | 50% | ❌ | Helix cannot open the capture buffer, and Helix cannot ask for text on a server's behalf. |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 55% | ❓ | Refiling is left out: it asks for a target from a list, which Helix cannot show for a server. Archiving works as code actions. |
| [Links](../../docs/parity.md#links) | 67% | ❓ | Storing and inserting links are left out: they ask for text, and Helix cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition. |
| [Footnotes](../../docs/parity.md#footnotes) | 50% | ✅ |  |
| [Tables](../../docs/parity.md#tables) | 44% | ❓ | Moving between fields with `TAB` is left out: Helix cannot bind a key to a server's command (the commands are code actions, `Space a`), and it needs one. The other edits are code actions in a table. |
| [Table formulas](../../docs/parity.md#table-formulas) | 50% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 55% | ❓ | Only tangling and the block at the cursor are offered (code actions in a block): the file's and the subtree's evaluation and removing results need keys, which Helix cannot give a server. |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 56% | ❌ | Not offered as a code action yet. |
| [Citations](../../docs/parity.md#citations) | 100% | ❓ | Inserting citations is not offered as a code action yet. Completion, hover and following work. |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ❓ | Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed. |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 75% | ❌ | A note is typed text: Helix cannot ask for text on a server's behalf. |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 25% | ✅ |  |
| [Timers](../../docs/parity.md#timers) | 0% | — |  |
| [Attachments](../../docs/parity.md#attachments) | 0% | — |  |
| [Encryption](../../docs/parity.md#encryption) | 0% | — |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 0% | — |  |
| **All of Org** | **61%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->
