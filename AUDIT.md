# Audit findings, round five (final)

The fifth and last review of org-lsp, after the fourth round's fixes landed
on `main` (PR #228). Earlier rounds are in this file's git history. This
round was cut short at the maintainer's request to save session quota, so
it is narrower than the others:

- **Done**: the editor, docs and website parts of #228 were verified, the
  regenerated docs were checked against the code, and the Neovim plugin's
  remaining logic (key mapping, agenda view, display, folds, clock,
  columns) had a first correctness review. `check.sh` passes on the merged
  tree.
- **Not done**: the server-side fixes of #228 (J1 to J5, 2.1 to 2.5, R6,
  A1, L2, L5, W1 and the 3b items) were **not re-verified** by running
  their triggers; they are listed below as "claimed fixed" with what the
  commit message says. The planned reviews of the org-lint checkers, the
  indentation command, `org-read-date` parsing, capture template expansion,
  and display, cycling and `RET` behaviour did not run.

No Emacs or Org sources were on this machine at any point; "what Emacs
does" comes from the reviewers' knowledge of Org 9.8 and the recorded cases
under `tests/`, except where a reviewer fetched a source file and says so.

Deliberately left as they are, per the maintainer (unchanged list):
diagnostics debounce, caching the workspace file list, Sublime
file-and-position paths, coc.nvim on Windows and the Neovim edit version
check, roam finalize and jump-to-captured after a refiled capture, the
encryption docs, tangle paths, the synchronous Enter/o/Tab waits, and the
`zed-extension` hash (the host answers 403 from this environment).

---

## 1. Status of the round-four findings

### Verified fixed this round (editor, docs, website)

| ID | Finding | Status |
| --- | --- | --- |
| 2.6 | VS Code `deactivate` outside the restart chain | Fixed: `deactivate` chains after `restarting` and sets a flag the restart checks (`extension.ts:669-690`, `2135-2140`); the promise is returned so VS Code waits |
| 2.7 | image `.part` names and pruning | Fixed: per-process `.part` name, `.part` files older than an hour pruned (a conversion is bounded to 10 s), a bad cached PNG removed and not remembered |
| 3c | site search highlighting, logo copied twice, empty `alt` | Fixed: one alternation over the plain text with every piece escaped (tested in node with metacharacters, entities, non-ASCII case folding); logo resolved to the site's own copy; screenshots have `alt` |
| 3a note | docs on IDs and the journal locale | True against the code: `plugins.org:299-301` matches `id_paths` (open documents, agenda files, then roam files, deduplicated by canonical path); `plugins.org:424-428` matches `capture::format_time` (C locale, UTC) |
| 3a, 3b | undocumented gaps | 28 options and commands now in the parity table as missing, each confirmed absent from `settings.rs` and the code; two names were dropped: `org-journal-carryover-headings-only` and `org-journal-encrypt-on` are neither implemented nor listed |
| 3b | `::/regexp/` searches | Partly: a sparse tree only when the link points at its own file (`server.rs:4276-4291`); `file:other.org::/re/` still fails (`open.rs:670`), and the new parity row says exactly that, so the doc is true |

### Claimed fixed in #228, not re-verified

From the commit message; each should be checked by re-running the
round-four trigger before being considered closed: J1 (`[:alpha:]` now
letters, marks and letter numbers), J2 (`id:` links looked up in roam
files), J3 (capture to an existing node recorded from Emacs; the commit
says org-roam *does* put the node's ID at the file level too, which would
make the round-four J3 a non-bug: the new recorded cases in `tests/roam/`
and `scripts/roam_capture_reference.py` are the evidence to read), J4
(`immediate_finish`, `jump_to_captured`, `clock_in` on roam templates), J5
(an open journal file emptied through an edit before deletion), 2.1 (edits
sent one line for one line), 2.2 (prefix forms as a background job with a
rebuild on answer), 2.3 (hard-linked file written beside first), 2.5 (flat
LCS table), R6 (view directory made on first write), A1 (only what Org's
`timep` takes is a time), L2 (`%` that starts no escape encoded), L5
(`~name/` is that user's home or a literal name), W1 (ID files compared
through links), the 3b items (SCHEDULED and DEADLINE set as properties
schedule; priorities leave tags alone with `org_auto_align_tags` off).

Things to look at when verifying, from reading the diff summary only:
whether other `[[:class:]]` translations in `journal.rs` have J1's problem;
the cost of J2's lookup per hover; whether 2.2 shows the view at once
without a 10 s wait when Emacs is absent and what happens to marks and the
cursor between the two builds; whether 2.3's final copy into the existing
inode keeps permissions; what L5 does on Windows and without a passwd
database.

### Still open from earlier rounds

- **S5** — Neovim image conversion is still synchronous on the main loop
  (`lua/org-lsp/images.lua`, `vim.system(...):wait()` with a 10 s timeout).
  Not on the maintainer's left-as-is list.
- **2.4 (round four)** — mixed line endings in one file still put `\r` into
  a table cell; pure-CRLF files are fine.

---

## 2. New findings this round (Neovim plugin, docs)

### N1. Medium — Neovim's and Vim's `S-TAB` have no CONTENTS state, but the docs claim it
`lua/org-lsp/folds.lua:128-143` (`global_cycle`): `foldlevel == 0` gives `zR`, otherwise `zM`, two states. `tests/nvim/e2e.lua:143-146` asserts exactly that. Yet `scripts/features.mjs:138` ("overview, contents, everything", true) renders `docs/parity.org:82` as ✅ and `editors/neovim/TESTING.org:44` as "overview → contents → everything". Emacs's `org-global-cycle` is OVERVIEW → CONTENTS → SHOW ALL. The Vim plugin does the same two states (`editors/vim/autoload/org_lsp.vim:2210-2220`). Verified by the test suite's own assertion.
Fix: add a CONTENTS step (close every fold, then open each headline fold one level, or ask the server for the fold set), or change the row and check text.

### N2. Low-medium (unverified) — Starting the server from a non-Org buffer can split the plugin over two servers
`lua/org-lsp/init.lua:162-174` (`with_client`) calls `vim.lsp.start(vim.lsp.config[SERVER], { attach = false })` for `<Leader>oa` and `:Org` from a non-Org buffer. Checked against Neovim 0.12.0's `lsp.lua`: with a `vim.lsp.config('*', { root_markers = { '.git' } })` (the documented example), `lsp.start` ignores `root_markers`, so this client has no root; when an Org file opens later, `vim.lsp.enable`'s path resolves a root and `reuse_client_default` refuses the rootless client, starting a second `org_lsp`. `running_client()` (`init.lua:149`) then returns the first, which never got `didOpen` for Org buffers, so the agenda state and the buffers live in different servers. A `root_dir` function on `'*'` reaches `lsp.start` unresolved.
Fix: start through the same path `restart()` uses (`vim.lsp.enable` plus a synthetic `FileType`), or prefer `vim.lsp.get_clients({ bufnr })` when the current buffer is attached.

### N3. Low — `setup()` after start deep-merges and never re-sends settings
`init.lua:1324-1330` calls `vim.lsp.config(SERVER, {...})`, whose call form deep-merges, so a removed `settings` key survives and nothing reaches the running server. `docs/neovim.org:42-43` says `setup()` can be called at any time; `docs/configuration.org:320-323` says a restart is needed. Say so in `neovim.org`, and consider the assignment form, which replaces.

### N4. Low — Hidden leading stars reappear on the cursor line
`lua/org-lsp/indent.lua:107-114` hides stars with per-character `conceal = ' '` marks, which need `conceallevel` and follow the user's `concealcursor` (default empty), so the stars show on the cursor line where `org-indent-mode` keeps them hidden. `docs/configuration.org:393-395` says it needs `conceal` but not this. A doc sentence.

### N5. Low — Smaller Neovim items
- `init.lua:1035` builds Neovim's private semantic-token namespace name `nvim.lsp.semantic_tokens:<id>`; it matches 0.12.0 and the e2e test pins it, but a rename in a later Neovim would leave conceal marks in a namespace nothing clears.
- `indent.lua:118-124` sets `breakindent` and `formatlistpat` when virtual indent turns on and never restores them when a later notification has no lines.
- `view.lua:602` picks an agenda window with `win_findbuf` across all tabpages and then switches tabpage; Emacs stays in the current frame. Filter by the current tabpage's windows.
- `.org.gpg` is not an Org filetype in Neovim (no filetype registration; Neovim's own table has `org` and `org_archive`). No doc promises it, so a parity note.

---

## 3. Checked and found sound this round

- All four generators pass in check mode against the rebuilt binary; README and every `docs/<editor>.org` table regenerate identically; the parity table arithmetic holds.
- `scripts/roam_capture_reference.py` threads its new `node` field correctly; `tests/roam/capture.json` holds the new cases.
- `site/src/main.rs:498-504`: a `file:site/…` image that exists in the checkout maps to the site's own path, and only the logo is such a file.
- Neovim `init.lua`: key mapping from the manifest (per-scope keys, user overrides, `nowait` only where no longer key starts with it, `expr` plus `g@l` for dot-repeat, Visual mappings for region commands, insert-mode `<CR>` with the `pumvisible` and completion-plugin handling that `docs/keys/neovim.org:385-391` describes and `e2e.lua:777-820` tests); `g?` help; `:Org` completion; `vim.lsp.config`/`enable` for 0.12; `restart()`; `indentexpr` flushing changes first; `formatexpr` cleared; `foldexpr` with startup folds after a bounded wait; sparse trees.
- `view.lua`: `buftype=nofile`, `modifiable` toggled around minimal line replacement, views saved per window, window reuse per `win_split_mode`, the date prompt as a `bufhidden=wipe` float, `q` restoring the origin, `agendaCursor` sent only when the line changes, `agendaWidth` on resize.
- `display.lua` (version check, multibyte via `strcharpart`), `highlights.lua` (faces as default groups, priority above semantic tokens), `clock.lua` (timers closed with state), `columns.lua` (keys and winbar restored), `buffers.lua`, `help.lua`, `notify.lua`, `plugin/org-lsp.lua`: no defects found.
- `tests/nvim/*.lua` against the docs: the two-state `S-TAB` (N1) is the only contradiction.
