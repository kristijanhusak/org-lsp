# Org for Sublime Text (LSP-org)

`LSP-org` is a package for [sublimelsp/LSP](https://github.com/sublimelsp/LSP)
that runs Org through the org-lsp server.

## Install

From a checkout of this repository:

```sh
scripts/install-sublime.sh          # links editors/sublime/LSP-org into Packages
scripts/install-sublime.sh --copy   # or copies it there
```

It finds Sublime Text's `Packages` folder (set `SUBLIME_PACKAGES` when it
cannot), puts the package there as `LSP-org`, and adds
[LSP](https://github.com/sublimelsp/LSP) to Package Control's packages, so
that it is installed when Sublime Text starts. Then restart Sublime Text
and open an `.org` file.

By hand: install LSP with Package Control, and copy or link
`editors/sublime/LSP-org` into the `Packages` folder (*Preferences →
Browse Packages…*), keeping the name `LSP-org`.

**The server.** Nothing to do: the package starts the first of

1. the `command` set in the settings, when it is not the default
   `["org-lsp"]`;
2. `org-lsp` on PATH, or in `~/.cargo/bin`, `~/.local/bin`,
   `/usr/local/bin` or `/opt/homebrew/bin` (Sublime Text started from a
   desktop launcher often has a shorter PATH than a terminal);
3. the build in the checkout the package is linked from
   (`target/release/org-lsp`, then `target/debug/org-lsp`);
4. the nightly build for your platform, downloaded once into Sublime
   Text's `Package Storage/LSP-org`.

*Org: Download the latest server* fetches the nightly build again and
restarts the server.

**Highlighting.** The package's syntaxes color Org files and the agenda
by their patterns: headlines by level, TODO keywords, priorities, tags,
planning lines and timestamps, drawers and properties, lists and
checkboxes, links, emphasis, tables, blocks and comments, and the code of
source and export blocks and of inline source blocks in its language's
syntax for the languages Sublime Text ships (`#+begin_src python`,
`#+begin_export html`, `src_python{…}` to the first `}`); in the agenda
the days, schedules, deadlines and the time grid. The server's colors
(semantic tokens) go further, with the TODO keywords of your files, the
agenda's today, weekends and overdue items, the clocked entry. LSP draws
them with `"semantic_highlighting": true` (*Preferences → Package
Settings → LSP → Settings*) and a color scheme that allows them: LSP
lets Sublime Text's own schemes do so, and another needs the rule
*UI: Customize Color Scheme* adds, as
[LSP's docs](https://lsp.sublimetext.io/features/#semantic-highlighting)
say:

```json
{ "rules": [{ "scope": "meta.semantic-token", "background": "#00000101" }] }
```

## Using it

- [docs/sublime/keys.md](../../docs/sublime/keys.md): every key of both
  presets (`emacs`, the default, and `vim`), and how to bind your own.
- [docs/sublime/configuration.md](../../docs/sublime/configuration.md):
  the package's settings and the server's options, with their defaults.

Files with `org-indent-mode` (`org_startup_indented`, `#+STARTUP: indent`)
show indented by level, without being changed: blanks before the lines,
as phantoms. The leading stars still show, and the lines a long line
wraps into are not indented.

The agenda and capture need no Org file open: from the Command Palette
(`Org: Agenda commands`, `Org: Capture`, …) in any view, the package starts the
server first, with a tab that says so and closes when the agenda opens.

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Sublime Text | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 100% | ❓ | Links, emphasis markers, pretty entities and sub- and superscripts are not concealed: Sublime Text cannot hide text in a view. The colors need LSP's `semantic_highlighting` on. |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 100% | ✅ |  |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ✅ |  |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 100% | ✅ |  |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ❓ | Faces of particular keywords and priorities are not shown: Sublime Text colors text by its color scheme only. |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ✅ |  |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 100% | ❓ | Column view is left out: it draws over the headlines, which Sublime Text cannot. |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ❓ | Dates are typed, without the calendar: the package asks with an input panel. |
| [Agenda](../../docs/parity.md#agenda) | 100% | ❓ | Column view in the agenda is left out: it draws over the lines, which Sublime Text cannot. |
| [Clocking](../../docs/parity.md#clocking) | 100% | ✅ |  |
| [Capture](../../docs/parity.md#capture) | 100% | ✅ |  |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 100% | ✅ |  |
| [Links](../../docs/parity.md#links) | 100% | ✅ |  |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ✅ |  |
| [Tables](../../docs/parity.md#tables) | 94% | ❓ | A shrunk column shows its whole text and `…`: Sublime Text cannot hide text in a view. |
| [Table formulas](../../docs/parity.md#table-formulas) | 100% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 100% | ✅ |  |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 100% | ✅ |  |
| [Citations](../../docs/parity.md#citations) | 100% | ✅ |  |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 100% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ❓ | Virtual indentation shows as phantoms, the leading stars still showing; new lines are not indented as they are typed. |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ✅ |  |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ✅ |  |
| [Timers](../../docs/parity.md#timers) | 100% | ✅ |  |
| [Attachments](../../docs/parity.md#attachments) | 100% | ✅ |  |
| [Encryption](../../docs/parity.md#encryption) | 100% | ✅ |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 100% | ❌ | Not shown yet. |
| **All of Org** | **99%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->

The package has not been tried in Sublime Text itself here: its plugin is
tested against stand-ins for Sublime Text and LSP
(`python3 editors/sublime/test/test_plugin.py`), and its keys and settings
are generated from the server (`node editors/sublime/generate.mjs`).
