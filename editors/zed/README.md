# Org for Zed

A Zed extension that runs Org through the org-lsp server.

## Install

1. Install Rust with [rustup](https://rustup.rs): Zed builds dev extensions
   with it.
2. In Zed, run **zed: install dev extension** and pick this folder
   (`editors/zed` of a checkout). Zed builds the extension and installs it.
3. Add to your settings (**zed: open settings**):

   ```json
   {
     "languages": {
       "Org": { "semantic_tokens": "combined", "inlay_hints": { "enabled": true } }
     },
     "lsp": {
       "org-lsp": {
         "initialization_options": { "org_agenda_files": ["~/org"] }
       }
     }
   }
   ```

The extension starts `org-lsp` from your PATH (`cargo install --path
server/org-lsp` in a checkout puts it in `~/.cargo/bin`), or else
downloads the nightly build for your platform once. The download needs
the repository to be public: GitHub answers 404 to Zed for a private
one; [docs/zed/configuration.md](../../docs/zed/configuration.md) says
how to use a server binary of your own.

## Keys and settings

- [docs/zed/keys.md](../../docs/zed/keys.md): the commands in the code
  actions, and the agenda's keys through Zed's tasks.
- [docs/zed/configuration.md](../../docs/zed/configuration.md): Zed's
  settings for Org and the server's options, with their defaults.

## What works

<!-- BEGIN generated: features -->

| Feature | Parity | Zed | What does not work, and why |
| --- | --- | --- | --- |
| [Highlighting](../../docs/parity.md#highlighting) | 100% | ❓ | Links, emphasis markers, pretty entities and sub- and superscripts are not concealed: Zed lets no server hide text. The colors need `"semantic_tokens": "combined"`; code blocks take their language's colors, inline code does not. |
| [Folding and visibility](../../docs/parity.md#folding-and-visibility) | 100% | ❓ | Zed folds with its own folding: Org's cycling and sparse trees need folds an extension can set, which Zed does not offer. |
| [Headings and structure](../../docs/parity.md#headings-and-structure) | 100% | ❓ | Cloning is left out: it asks for text, and Zed cannot ask for text on a server's behalf. A command on a selection acts on the cursor's line: Zed tells a server no selection. The rest goes through the keys of keymap.json (Zed's tasks). |
| [Lists and checkboxes](../../docs/parity.md#lists-and-checkboxes) | 100% | ❓ | `RET` stays Zed's own, and so do Shift and Alt with the arrows, which cycle bullets and move items in Org. |
| [TODO keywords](../../docs/parity.md#todo-keywords) | 100% | ❓ | A note for a state change (`@`) cannot be typed: Zed cannot ask for text on a server's behalf. States are picked with buttons. Faces of particular keywords and priorities are not shown: Zed colors tokens by its theme only. |
| [Priorities](../../docs/parity.md#priorities) | 100% | ✅ |  |
| [Tags](../../docs/parity.md#tags) | 100% | ❓ | Setting tags is left out: it needs keys pressed in turn, or typed tags, and Zed cannot ask for text on a server's behalf. Completion and renaming work. |
| [Properties and column view](../../docs/parity.md#properties-and-column-view) | 100% | ❓ | Setting a property or the effort is left out: they are typed, and Zed cannot ask for text on a server's behalf. Column view is left out: it draws over the headlines. |
| [Dates and times](../../docs/parity.md#dates-and-times) | 100% | ❓ | Scheduling, deadlines and new timestamps are left out: they ask for a date, and Zed cannot ask for text on a server's behalf. Changing a timestamp works. |
| [Agenda](../../docs/parity.md#agenda) | 100% | ❓ | The agenda is a file, its commands code actions (or keys through Zed's tasks); searches, going to a date and dates asked for are left out (Zed cannot ask for text on a server's behalf), and follow mode (Zed reports no cursor moves to an extension). Column view in the agenda is left out: it draws over the lines. |
| [Clocking](../../docs/parity.md#clocking) | 100% | ❓ | No clock in the status line: Zed has no place there for an extension's text. |
| [Capture](../../docs/parity.md#capture) | 100% | ❓ | Templates with prompts are left out: Zed cannot ask for text on a server's behalf. The capture buffer's tab stays open once the entry is filed: Zed lets no server close a tab. |
| [Refile and archive](../../docs/parity.md#refile-and-archive) | 100% | ✅ |  |
| [Links](../../docs/parity.md#links) | 100% | ❓ | Storing and inserting links are left out: they ask for text, and Zed cannot ask for text on a server's behalf; moving between links needs a key. Following works through go to definition, for links inside Org files and to files. |
| [Footnotes](../../docs/parity.md#footnotes) | 100% | ❓ | Making footnotes and their menu are left out: they are not among Zed's code actions. Following them works. |
| [Tables](../../docs/parity.md#tables) | 94% | ❓ | Moving between fields with `TAB` and copying down (`S-RET`) are left out: those keys stay Zed's own. Creating a table is left out: it asks for its size, and Zed cannot ask for text on a server's behalf. A shrunk column shows its whole text and `…`: Zed lets no server hide text. |
| [Table formulas](../../docs/parity.md#table-formulas) | 100% | ✅ |  |
| [Code blocks (Babel)](../../docs/parity.md#code-blocks-babel) | 100% | ❓ | Only tangling, the block at the cursor and editing it in a file of its own are offered (code actions in a block): the file's and the subtree's evaluation, removing results and moving between blocks need keys, which Zed cannot give a server. |
| [Export and publishing](../../docs/parity.md#export-and-publishing) | 100% | ❌ | Not offered as a code action yet; its menu would come as buttons. |
| [Citations](../../docs/parity.md#citations) | 100% | ❓ | Inserting citations is not offered as a code action yet. Completion, hover and following work. |
| [Diagnostics (org-lint)](../../docs/parity.md#diagnostics-org-lint) | 97% | ✅ |  |
| [Indentation and formatting](../../docs/parity.md#indentation-and-formatting) | 83% | ❓ | Virtual indentation shows as inlay hints, the leading stars still showing; new lines are not indented as they are typed. |
| [Notes and logging](../../docs/parity.md#notes-and-logging) | 100% | ❌ | A note is typed text: Zed cannot ask for text on a server's behalf. |
| [Completion and editing helpers](../../docs/parity.md#completion-and-editing-helpers) | 100% | ❓ | Structure templates, emphasis and drawers are not among Zed's code actions. |
| [Timers](../../docs/parity.md#timers) | 100% | ❓ | A countdown that asks for its length is left out: Zed cannot ask for text on a server's behalf. No timer in the status line: Zed has no place there for an extension's text. |
| [Attachments](../../docs/parity.md#attachments) | 100% | ❓ | `attachment:` links open; attaching a file is left out: it asks for the file, and Zed cannot ask for text on a server's behalf. |
| [Encryption](../../docs/parity.md#encryption) | 100% | ✅ |  |
| [Inline images and LaTeX previews](../../docs/parity.md#inline-images-and-latex-previews) | 100% | ❌ | Zed lets no extension show images in a file. |
| **All of Org** | **99%** |  |  |

✅ all that org-lsp does works there, ❓ some does not (the last column says what
and why), ❌ none does, — nothing to support yet.
Parity is the share of what Emacs Org does in that area that org-lsp
does; [docs/parity.md](../../docs/parity.md) lists both.

[TESTING.md](TESTING.md) lists what to try by hand.

<!-- END generated -->

## Development

`node editors/zed/generate.mjs` writes the semantic token rules
(`languages/org/semantic_token_rules.json`), `tasks.json` and
`keymap.json` from the server's manifest; `--check` fails if they are out of date. `cargo build --target
wasm32-wasip2` in this folder builds the extension.
