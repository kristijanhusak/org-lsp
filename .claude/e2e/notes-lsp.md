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

## Incident: Emacs processes killed that may not have been mine (06:0x UTC)
`busy.py` (first version) killed every `bridge.el` Emacs that started after
it did, to clean up the orphan of lsp-9: three pids, 12480, 16905, 17125.
Only one was certainly its own; the others may have belonged to another
tester's server running at the same time (a command of theirs through Emacs
would then have failed with Emacs stopping, and their server starts Emacs
again on the next request). The scripts now kill only Emacs processes whose
parent is their own server (`my_emacs`).

## Second part (06:00–06:30 UTC)

- `compare.py` (≈120 scenarios, org-lsp full client vs Emacs batch, files
  diffed) — the SAME ones: TODO cycling/fast selection/logging (`!`, `@`
  note through the CAPTURE-Note buffer), repeaters and habits, statistics
  cookies, ORDERED blocking, priorities, schedule/deadline with `+2d`,
  `fri 10:00`, `2026-11-02`, `10:00-11:30`, `w44`, repeater kept, CLOSED
  taken out, removal; timestamps up/down on each part, brackets,
  repeaters, CLOCK lines, ranges; shift keys (TODO, bullets, priority);
  meta/shiftmeta on headings, items, checkboxes, tables; M-RET/C-RET/
  M-S-RET in headings, items, checkboxes, tables; RET; toggle heading/item
  (also on a region), COMMENT; checkboxes; sort a A o p n s d, lists,
  tables; table align/recalc (all, line, iterate)/insert/kill row/delete,
  insert, move column/hline/blank/copy down/sum/create/convert region;
  archive subtree/sibling/tag/done; refile (plain and `file` outline
  path); property set/delete; tags (CJK); effort; ID; clock in/out; notes;
  inline tasks; cut/paste/yank subtree; kill line; C-a special; sparse
  TODO tree; footnotes new/sort/normalize; clone; bibtex create; emphasis
  (with and without region); drawer; structure template; ascii plot.
  Differences found: lsp-6 (C-c C-c on a cookie). `org.levels.odd`: Emacs
  gives an inline task's END line one star more than its opening line (30
  vs 29); org-lsp keeps them equal — Emacs's oddity, not logged.
- Agenda edits writing back (`agenda_flow.py full|plain`, against Emacs
  batch `org-agenda-list` / `org-todo-list`): TODO state, repeat, habit,
  priority, schedule, deadline, date later/earlier (past dates go to today
  first), tags, archive, clock in, property, effort: files identical to
  Emacs; agenda lines identical except lsp-7 and lsp-8 (and the
  `S => Scheduled to …` / `=> <date>` suffix the server writes as text where
  Emacs puts a `display` property: what Emacs shows, as documented).
- Plain client: agenda as `Agenda.org` via the code action, 68 code actions
  on an entry line, 34 on a date line; edits come as applyEdit with
  versions, no failures; no diagnostics on Agenda.org; tokens standard.
  `message_requests=false`: choices as `org.answer` code actions (and a
  rename with the key) work for TODO, capture templates, the agenda menu.
  Bare client (no showDocument): `org.agenda.list` refused "This editor
  cannot open documents on the server's request"; `show_document=true`
  turns it on; questions still come as showMessageRequest (allowed by LSP).
- Date prompt document (`dateprompt.py`): every calendar key, type,
  select, time picking (Time: line, hour/minute steps), today, cancel,
  preview; `close` returned and honoured. `org.date.preview` of nonsense
  (`zzz`) answers the default date instead of failing as protocol.org
  says ("fails while the answer means nothing"): Emacs's `org-read-date`
  shows the default for nonsense too, so the doc is what is off (not
  logged as an issue).
- Capture (`capture.py`, both profiles): templates t (file+headline), n
  (file), j (datetree), finish/abort/refile; `%?` cursor; org-lsp/save with
  auto_save; CJK/emoji text. All right.
- Clocking (`clock.py`): in, switch, out, in last, cancel, goto,
  org-lsp/clock contents, effort notification, display as inlay hints, a
  dangling clock at start and `org.clock.resolve`. Right.
- Emacs-backed (`emacs_cmds.py`, `elispblock.py`): export menu (only
  ascii/html/icalendar/latex/odt: Emacs's default backends), export aborts
  (lsp-15); Babel confirmation as showMessageRequest (No → aborted message,
  Yes → runs; python has no `org-babel-execute` without configuration,
  Emacs's own message), emacs-lisp block result written; tangle writes
  out/hello.sh. Links: https external, #custom, *heading, file, timestamp
  (agenda of the day), citation (refs.bib), footnote, id; elisp refused
  (lsp-16).
- Encodings (`encodings.py`): UTF-8, UTF-16, UTF-32 on a line with CJK,
  accents and an emoji: rename ranges, code action edits, tag alignment
  (same as Emacs), tokens after typing after the emoji: all consistent.
- Robustness (`robust.py`, `storm.py`, `crlf.py`, `scale.py`, `orphan.py`,
  `busy*.py`): unknown command → -32803 with a message; missing
  position → "This command needs the cursor position"; unknown method
  -32601; null params -32602; documents not open → null; didChange to an
  unknown document ignored; 500-edit storms with CJK/emoji keep the server
  in sync (tokens equal a fresh server's), UTF-16 and UTF-8; shutdown then
  requests → -32600, exit 0; exit without shutdown → 1; stdin closed → 0.
  Issues: lsp-9 (orphaned Emacs), lsp-10 (main thread blocked by a
  synchronous Emacs call), lsp-11 (CRLF formatting), lsp-12 (large files),
  lsp-13 (malformed JSON kills the server), lsp-14 (refused edit silent).
  `$/cancelRequest` on a running Babel command: the request is still
  answered normally (null); nothing is cancelled (Emacs keeps working).
- org-protocol (`protocol.py`): `org-lsp protocol` store-link and capture
  reach the running server; full client gets `org-lsp/runCommand`
  (org.capture with keys and orgProtocol props), plain one gets the
  capture buffer and "Item captured.".
- Display notifications (`notifs.py`): full client gets virtualIndent,
  display (entities, superscripts, bullets, checkbox), lineHighlights;
  plain gets them as inlay hints; startupFolds honours overview and
  VISIBILITY.

## Coverage (end of round)

- Every one of the 313 manifest commands run in the full profile (313/313,
  no InternalError, no timeout, no exception; 29 contextual refusals in
  Emacs's words) and in the plain profile (298 run; the 15
  `org.calendar.*` need the date prompt document a plain client does not
  get, as documented; 26 contextual refusals). Emacs-backed commands
  re-run with a 3 s quiet wait (`sweep-full-slow.jsonl`) to see their
  edits. Answers: full by `input`/`context`, plain by showMessageRequest
  buttons and renames, `message_requests=false` by `org.answer` code
  actions.
- Compared with Emacs (Org 9.8.10, batch) on file contents: ~140 editing
  scenarios (compare.py), 17 agenda edits × 2 profiles (agenda_flow.py),
  9 agenda views (views.py: tags/property matches, search, stuck, TODO
  list: identical), org-lint, tag alignment with CJK/emoji, column view
  edits, table recalculation, journal search periods.
- Not compared with Emacs: roam commands (org-roam not loadable in plain
  batch here without its dependencies), journal entry creation (checked
  for sense only), export output (blocked by lsp-15 in the sample; plain
  export of a file without roam links not run), Babel beyond emacs-lisp.
- Not exercised: org.table.formula.debugger beyond toggling, the formula
  editor, LaTeX preview images (no latex/dvipng), link image previews
  (no images in the sample), crypt (no GPG key: Emacs's GPG error),
  workspace folders added after start, `willSaveWaitUntil` with crypt.

## Ambiguities (not logged as issues)

- protocol.org says "The agenda is built anew with r, another view, a
  change typed in a file, …"; typing (even adding an entry) does not
  rebuild the agenda, a save does (server.rs: a typed change only
  releases the held lines). Emacs never rebuilds on its own either; the
  sentence reads as if typing rebuilds. `typing_agenda.py`.
- `org.date.preview` of nonsense answers the default date (as Emacs's
  prompt shows), protocol.org says it fails.
- `org.columns.*` and `org.custom.properties.toggle` run for a plain client
  (columns through Emacs, the file changes) though it cannot show column
  view; the latter refuses ("This editor cannot hide lines").
- Hover over an agenda line collapses the headline's tag spacing
  (`** TODO [#A] Write the report :work:`).
- Plain clients get `org-lsp/display` items as inlay hints (`◉` before
  `*`, `α` before `\alpha`): the original text stays, so both show.
