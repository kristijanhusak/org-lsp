# org-lsp

Org mode in any editor through one language server, as close to Emacs Org
mode as it can get. [PLAN.org](PLAN.org) has the goal and rules, where things
stand and how the work is checked; [docs/protocol.org](docs/protocol.org) is
what an editor integration has to do. Read both before larger work.

## Layout

- `server/org-syntax`: the parser, written from scratch (no tree-sitter).
- `server/org-core`: Org logic, ported from Emacs.
- `server/org-lsp`: the server. `commands.rs` declares the commands and
  their keys, `settings.rs` the options, `legend.rs` the tokens and how they
  look; `bridge.el` is the Emacs side of the bridge (`emacs.rs`), whose
  method `foo-bar` is the function `org-lsp-bridge-foo-bar`.
- Neovim at the root (`plugin/`, `lua/`); the other editors in `editors/`
  (`vim`, `vscode`, `sublime`, `zed`, `helix`).
- `site/`: the documentation website, written from `README.org` and
  `docs/` with `org-syntax` (no other tools); `.github/workflows/docs.yml`
  publishes it on GitHub Pages.
- `scripts/`: generators, Emacs reference scripts, fixtures and fuzzers.
- `tests/`: committed cases recorded from Emacs, and the Neovim tests.

## Commands

```sh
scripts/regen.sh      # write every generated file again
scripts/check.sh      # fmt, clippy, cargo test, generated files, VS Code types, Sublime tests
scripts/test-all.sh   # check.sh, then Neovim, Vim, VS Code end to end
scripts/screenshots.py  # docs/images: screenshots and animations of Neovim (NVIM)
scripts/site.sh       # the website into target/site; --serve shows it
```

Set `ORG_LISP_DIR` to the `lisp` directory of an Org 9.8.10 checkout: the
tests that run Emacs use it, and
`follows_links_whose_abbreviation_calls_a_function` fails against the Org
that Emacs ships with. `ORG_LSP_REQUIRE_EMACS=1` makes those tests fail
instead of skipping when Emacs is missing. `test-all.sh` also reads `NVIM`,
`CLIENTS` (Vim's LSP clients, see `editors/vim/test/run.sh`) and
`VSCODE_EXECUTABLE` (VSCodium works); the VS Code tests run under
`xvfb-run`. Recording the org-super-agenda cases (`tests/agenda/super-*`),
the org-roam ones (`tests/roam/`) and the org-journal ones
(`tests/journal/`) needs `ORG_LSP_EMACS_PACKAGES`: a directory holding
those packages and what they require (`scripts/agenda_reference.py`,
`scripts/roam_reference.py` and `scripts/journal_reference.py` list them).

## Generated files: edit the source

The server describes itself (`org-lsp manifest`, `org-lsp settings`), and
generators write what editors must be told ahead of time. Never edit their
output; edit the source and run `scripts/regen.sh`. `check.sh` fails when
they are out of date.

| Output | Source |
| --- | --- |
| `editors/vscode/package.json`: commands, keys, tokens, `org-lsp.settings` | `commands.rs`, `legend.rs`, `settings.rs` |
| `editors/sublime/LSP-org/*` keymap, commands, `semantic_tokens` | the same |
| `editors/zed/languages/org/*.json`, `keymap.json`, `tasks.json` | the same |
| Between `# BEGIN generated: NAME` and `# END generated` in the Org docs (key tables, option lists); `docs/config.schema.json`; `lua/org-lsp/types.lua` | `settings.rs` and the manifest |
| What works (`README.org`, `docs/<editor>.org`), `docs/parity.org`, `editors/*/TESTING.org` | `scripts/features.mjs` |
| `editors/vscode/README.md` (written when the extension is packaged, not kept) | `docs/vscode.org`, by `editors/vscode/scripts/readme.mjs` |

Written by hand: the `configuration` section of `package.json` (the
extension's own settings such as `org-lsp.images`), the user-facing values
in `LSP-org.sublime-settings` (`org_keys`, `org_messages`, `org_images`,
kept by the generator), and every doc outside the generated blocks.

## Rules

- **Emacs is the reference.** Port Org logic from the Org 9.8.10 sources,
  and check it against Emacs with recorded cases and differential fuzzing
  (`.claude/skills/port-from-emacs`). Write down any deliberate difference.
  Where porting would be a lot of code for little use, call Emacs through
  the bridge instead.
- **Say what Emacs says.** A command's messages have Emacs's exact text.
- **Diagnostics are hints or information**, never errors or warnings.
- **As much as possible in the server.** Editors get glue that loops over
  what the server announces. A feature that needs editor code follows
  `.claude/skills/editor-feature`: an `experimental.org*` client
  capability, plus an option of the same meaning for clients whose LSP
  plugin cannot announce one (Vim, Sublime Text).
- **Write like the code around you.** Comments say what the code is for,
  in plain sentences, with Emacs's names in backticks (`org-link-preview`).
- **One pull request per change**, merged by squashing; the docs, parity
  table and `TESTING.org` checks change in the same PR as the feature.
- **The docs are Org**, written as Org (not Markdown in Org's clothes):
  `~code~`, `/italic/`, `[[file:x.org::#id][text]]` links, description lists
  for references, `#+begin_src` blocks. Each editor has `docs/<editor>.org`
  (install, setup, a short example, what works) and `docs/keys/<editor>.org`;
  every option and setting is in `docs/configuration.org`, and the Emacs
  packages ported (org-super-agenda, org-roam, org-journal) are described
  in `docs/plugins.org`; `editors/*/README.org`
  only points to the editor's page. The website shows them as they are:
  a new page in `docs/` goes in the sidebar (`NAV` in `site/src/main.rs`),
  and `check.sh` fails on a link to a file or heading that is not there. A link to a heading in another file
  uses a `CUSTOM_ID` equal to GitHub's anchor of the heading, so it works in
  Emacs and on GitHub. CLAUDE.md and the skills stay Markdown: Claude Code
  reads them.

## Pitfalls

- Vim: LSP clients do not see a buffer reloaded from disk; the plugin
  sends the agenda's new text itself (`s:give_view`). coc.nvim misses edits
  made in the first ~300 ms after a buffer opens.
- Emacs in batch mode has no window system: code that asks for a display
  (`org--get-display-dpi`, `clear-image-cache`, `image-flush`) has to be
  stubbed in `bridge.el`.
- VS Code: a decoration cannot be taller than a line (it overlaps the next
  ones), and hover markdown is cut short past 100,000 characters (refer to
  files, never inline data). In tests, a document closed and opened again
  may be the same document, with no open event.
- Neovim `vim.ui.img`: never pass both width and height; give one and let
  the image keep its proportions.
- The Neovim tests run headless against the real server, on 0.11.0 and
  stable; the image tests put a stand-in in place of `vim.ui.img` (0.13),
  and the LaTeX test stand-ins for `latex` and `dvipng`.
