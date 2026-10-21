# Notes of the lsp tester, 2026-10-21

Server: `/home/user/org-lsp/target/debug/org-lsp` (built from main, Oct 20 19:29),
driven over stdio by `.claude/e2e/repro/lsp/client.py` (no editor code).
Emacs: /usr/bin/emacs; Org 9.8.10 from /root/org-mode/lisp. The server does
not read `ORG_LISP_DIR` itself (only its tests do, `server/org-lsp/tests/lsp.rs`):
to make its Emacs use that Org, the harness passes
`emacs_config = (add-to-list 'load-path "/root/org-mode/lisp")`.
Settings reach the server only through `initializationOptions` and the shared
config file (`$ORG_LSP_CONFIG`, `$XDG_CONFIG_HOME/org-lsp/config.toml`); the
harness sets `XDG_CONFIG_HOME` to a scratch dir so no user file leaks in.
No `workspace/configuration` request is made by the server.

Manifest: 313 commands (`org-lsp manifest`).

## Harness (.claude/e2e/repro/lsp/)

- `client.py`: the LSP client. Profiles PLAIN (no experimental, no orgClient;
  showMessage + showDocument announced), FULL (every `experimental.org*`:
  orgClient, orgDatePrompt, orgVirtualIndent, orgDisplay, orgImages,
  orgLineHighlights, orgHiddenLines, orgRunCommand, plus
  `workspace.textDocumentContent`), BARE (plain without showMessage and
  showDocument). Applies `workspace/applyEdit` (opening the file first, as
  an editor loads a buffer; checks versions), answers showMessageRequest
  from a queue (`c.picks`), opens what `showDocument` shows (fetching
  `org-agenda:` documents), follows `textDocumentContent/refresh`, saves on
  `org-lsp/save`. `c.run()` answers `input` questions as an org client does.
- `workspace.py`: the sample workspace (gtd.org with TODO/schedules/
  deadlines/clock/habit/ORDERED/checkboxes/inline task/footnotes/citations/
  links/src blocks; notes.org with refile targets and a clocktable;
  tables.org; refs.bib; gtd.org_archive; roam/ with nodes and a daily;
  journal/20261020), and the options (roam and journal on, capture
  templates, refile targets, agenda files).
- `sweep.py full|plain [cmds]`: every manifest command once, fresh server and
  workspace each, answering its questions; `SWEEP_QUIET=3` waits for the
  Emacs-backed ones. Output `$SCRATCH/lsp/sweep-<profile>[-slow].jsonl`.
- `compare.py [names]`: the same command in org-lsp (full client, date
  prompt document off) and Emacs batch (Org 9.8.10; minibuffer and key
  prompts stubbed with the same answers; `org-fold-show-all`,
  `post-command-hook` run, `last-command` set), files diffed.
- `features.py`, `completion.py`, `columns.py`, `roamlink.py`,
  `roamdiag.py`, `caps.py`, `diag.py`: focused checks.
- `emacs.py` / `emacs_agenda.py`: Emacs batch helpers.

Emacs-batch artefacts to remember when comparing: `#+STARTUP: content`
folds the file and `org-insert-item` refuses an invisible list
(`org-fold-show-all` first); `org-priority-up` with `last-command` =
`this-command` (both nil in batch) wraps to the lowest; state logging runs
on `post-command-hook` (run it by hand); `org-todo` with no argument reads a
key (hangs without stdin).

## Results so far

- Initialize: the three profiles get the same capabilities; plain gets the
  standard legend (15 standard types, modifier `deprecated` only), full the
  Org legend. `executeCommandProvider` lists 315 commands (manifest 313 +
  `org.cycle`, `org.date.preview`). `semanticTokens/full/delta` answered
  with MethodNotFound (-32601), fine (not announced).
- Diagnostics: severities 3/4 only; org-lint matches Emacs's
  `org-lint--generate-reports` on gtd.org line for line (misplaced planning,
  fuzzy link, missing PRINT_BIBLIOGRAPHY) except roam links (lsp-4).
- Tokens (plain, UTF-16): no overlaps, none past line ends, range tokens
  equal the full ones in the range, CJK/emoji headline tokens decode to the
  right text.
- Completion: TODO keywords, tags (`:`), `#+` keywords and `#+STARTUP:`
  values, blocks (lowercase `begin_src`, no trailing space as Emacs's
  `BEGIN_SRC `), drawers, property keys, entities, link types, files,
  headlines `[[*`, custom ids `[[#`, roam nodes in roam files (`[[roam:` and
  `[[id:` replaced by an `id:` link), cite keys (twice: lsp-1). Nothing for
  `#+OPTIONS:`, `#+FILETAGS:`, `#+TAGS:`, `src_`, `{{{`, `[[id:` outside
  roam files (pcomplete has options/filetags/tags: not claimed in parity).
- Column view: `columns.py`; Emacs batch gives the same file changes
  (`:EFFORT:` renamed, `#+COLUMNS:` rewritten, tags misaligned in column
  view); the questions differ (lsp-5).
- Sweeps (both profiles, all 313 commands): no InternalError, no timeout,
  no panic, no stderr; refusals are contextual and in Emacs's words
  (`No active clock`, `Cannot move past superior level or buffer limit`,
  `Not on a date!`, `No more buttons` (org-journal's own when a search
  finds nothing), `Wrong type argument: stringp, nil` (org-bibtex-copy off
  a BibTeX entry: Emacs's own error too)).
