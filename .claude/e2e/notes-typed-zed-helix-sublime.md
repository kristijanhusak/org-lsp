# Notes: typed-zed-helix-sublime, 2026-10-25

Every key and answer typed in the real editors: Zed through xdotool (its
window under Xvfb), Helix through tmux. Server: /home/user/org-lsp/target/debug/org-lsp
(has #391). Scripts: `.claude/e2e/repro/typed-zed-helix-sublime/` (`hxlib.py` over
editors/helix/test/run.py's Helix class, `zxlib.py` over editors/zed/test/run.py's
Scenario and Zed classes; each `hxN.py`/`zxN.py` prints one line per check with what it
gave; `zxhold.py` keeps a Zed open for a look, `zx_installed.py` runs Zed as
`install.sh zed` leaves it). Screens looked at with `import -window root`.

## Runners

- Zed runner before: 16/16 (3 skipped by exceptions.json). What was not typed: files opened
  and the agenda gone back to with `zed --add` (Zed's command line); the agenda's save
  skipped. Now: the file finder (`ctrl-p`, name typed, Enter), Zed's `workspace::SaveAll`
  for `s` (files then checked on disk). Questions are clicked with the mouse: Zed's
  notifications take no keys (Tab goes to the editor: checked by screen).
- Helix runner before: 13 ok, 3 skipped. What was not typed as a user would:
  `:lsp-workspace-command CMD {json}` with a made-up position and `"apply": true` for
  counts, org.meta.up/down, org.meta.return below, follow mode, goto.other. Now: counts
  pick the code action N times (once for org.priority.down, as in Zed), below is `gl l`
  then the action, a moved subtree is followed with a search, save is `:write-all`;
  follow mode and the other window are `steps.helix` (Helix cannot), as are the counts Org
  reads as more than repeats (entry text 2, filter C-u C-u).
- Sublime Text: not installed here; nothing run for real. Stand-ins tightened (below).
- Final: Zed 17/17 (3 skips + check zed-capture-here), Helix 13 ok + highlight, 3 skips,
  Sublime 45 tests OK.

## Zed by hand (TESTING.org)

- Setup: ok. `install.sh zed` into an empty home (nightly): extension with grammars/ and
  languages/, server beside it, keys and tasks between BEGIN/END, the user's own key kept
  after them; Zed started there: ctrl-c a in an Org file asks with buttons. Code actions
  (ctrl-.) list the commands.
- Highlighting: ok by screen (tests/e2e/highlight: levels, keywords, priorities, tags,
  planning, drawers, blocks, ARCHIVE shaded, agenda with habit graph backgrounds and the
  clocked line). Option variants (n_level_faces, emphasis_alist) not tried.
- Folding: zed-fold (fixed, ed8c346): Zed asked for no folding ranges, so Org files had no
  folds at all; Zed needs `"document_folding_ranges": "on"` for the language. With it,
  ctrl-shift-[ folds a headline (screen). Tab on a headline indents it: Zed has no Org
  cycling (docs/zed.org says so; TESTING.org's "cycling goes folded → children → subtree"
  check has no key in Zed: doc issue of scripts/features.mjs). Narrowing ctrl-x n s /
  ctrl-x n w: ok (written back, saved).
- Headings and structure: ok (alt-enter at line end, TODO heading after subtree, demote and
  promote, subtree down by code action, cut/paste "Clipboard pasted as level 2 subtree",
  sort by the "alpha" button, next/forward/up, "Already at top level of the outline", goto
  `Table/`, COMMENT on and off, clone 2 × +1w, inline task as Emacs inserts it). The sort
  question has 21 buttons: taller than an 800-pixel window, it scrolls with the wheel.
- Lists: ok (new item renumbers 1 2 3 4, checkbox cookie [1/2], sort "Sorting items...done").
- TODO keywords: ok (key + note for WAIT(w@), CLOSED, cookie, repeater "Entry repeats",
  keyword sets BUG→FIXED→none, ctrl-shift-right "Keyword-Set 1/2: TODO DONE", blocked by
  ORDERED).
- Priorities, tags, properties: ok (a then SPC "Priority removed", #+PRIORITIES [#3], tags
  typed b:a aligned, property Owner/Ann, effort "Effort is now 1:00", ORDERED message).
- Dates: ok (+2d, fri 10:00, timestamp part up, day later, ctrl-c . typed date).
- Agenda: the 13 agenda scenarios run typed; by hand f pressed 5 times → W26 in the
  agenda and the editor.
- Clocking: ok (effort message once, out, in last "Clocking back: Plan (in a.org)", goto,
  cancel, display "Total file time: 0:40", clock table update).
- Capture: zed-capture (fixed, 711051d): ctrl-c c in an Org file ran capture with no place
  (the global task), so `%a` was empty (`* `), and with the agenda open it would run on
  the agenda's line. Repro before the fix: template `* %a`, cursor in work.org, ctrl-c c,
  pick it, ctrl-c ctrl-c: inbox.org gets `* `. Now `%a` is the heading's link. Rest ok:
  menu, `%^{Name}` typed by F2, abort, datetree, `%[snippet.txt]`, `%(upcase "abc")` → ABC.
- org-protocol: not tried in Zed (done in Helix, same server path).
- org-roam: ok (find by button, buffer shown beside without focus, dailies today file).
- org-journal: ok (new entry with time, previous entry, search with the dates typed;
  "between Thursday, 04/30/26" for 2026-05-01 is what Emacs's org-journal says too,
  tests/journal/cases.json).
- Refile and archive: covered by the typed scenarios (agenda-entries, capture).
- Links: ok (heading, custom id, target, file::*search, id, missing heading created after
  the y button, shell link "hi", timestamp → day agenda, #+LINK abbreviation sent as
  showDocument external, store and insert with description, next link).
- Footnotes: ok (to the definition; from there "No link found", as Emacs at that place).
- Tables and formulas: ok (recalculate, Emacs Lisp formula by C-c C-c → 3, column right
  with #+TBLFM following, rule below, sum "Sum of 2 items: 6", field info, create 2x1 as
  Emacs indents it).
- Babel: ok (Yes button, results; inline src {{{results(=42=)}}}; #+CALL; remove result;
  next block; tangle).
- Export: ok (h h: "Exported to $DIR/work.html", the file there). With an attachment: link
  in the file Emacs says "Unable to resolve link" (not looked into).
- Timers: ok (start, item — on a headline line it goes before the stars, as Emacs's
  org-timer-item does —, stop).
- Notes: ok (one line typed by F2, in LOGBOOK).
- Editing helpers: ok (structure template s → #+begin_src).
- Attachments: ok (attachment: link opens data/at/t-1/note.txt).
- Citations: ok (following opens refs.bib at the entry).
- Diagnostics: ok by screen (misplaced heading, link to nowhere, :foo, open block underlined).
- Indentation: ok by screen (#+STARTUP: indent as inlay hints).
- Encryption: not possible in Zed: it announces no willSaveWaitUntil (only didSave, checked
  in its initialize), so encrypting on save cannot happen; TESTING.org lists it as a check
  (doc issue of scripts/features.mjs).
- Not tried: restarting Zed with the agenda's tab, the task with no Org file open, hover
  with the mouse, dangling clock, gnuplot/man/ctags (not installed), LaTeX/images (Zed
  shows none).

## Helix by hand (TESTING.org)

- Setup: ok. `install.sh helix` into an empty home (nightly): languages.toml block with the
  server's path, `show_document`/`message_requests`; queries in runtime; `hx --health
  org-agenda` shows the server and the highlight queries. Inlay hints with
  display-inlay-hints.
- Highlighting: ok (the runner's highlight check; agenda colors).
- Headings: ok (new heading, TODO heading, after subtree, promote/demote heading and
  subtree, subtree moves — the cursor stays in the text, now in docs/helix.org —, cut/paste
  and copy/paste messages, top-level promotion to `# Two`, sort, outline Space s).
- Lists: ok (new item renumbers; lists one blank line apart are one list, as Emacs repairs
  them; no box after a box item; indent needs a previous item, as in Emacs; checkbox; 1 3 2
  repaired; sort).
- TODO keywords: ok (key + note, CLOSED, cycle, cookie, repeater with LAST_REPEAT, #+TODO
  line A→B→C without asking, keyword sets, ORDERED and checkbox blocking, agenda line offers
  keyword sets and removing dates).
- Priorities, tags, properties, dates: ok (raise/lower/set/SPC, #+PRIORITIES, tags typed and
  completed after `:`, tag rename by Space r, effort, +2d and fri 10:00, timestamp parts
  year/month/day/hour/minute (5-minute rounding), a warning kept).
- Agenda: ok (menu with a custom command, week view, tag search work+urgent, word search,
  Space k hover, note from the agenda as one line, an entry added in its file shows after
  rebuild) beside the typed scenarios.
- Clocking: ok (effort message, in/out/last/cancel, goto from another file, clock table).
- Capture: ok (menu, finish, %^{} by rename, abort, refile, datetree, %a, %[file], %()).
- org-protocol: ok (`org-lsp protocol` capture URL opens the capture buffer with title,
  link and text; store-link says Emacs's ‘C-c C-l’ message).
- Refile and archive: ok (refile to another file, archive with ARCHIVE_ properties, ARCHIVE
  tag on/off messages, archive sibling).
- Links: helix-gd-external (server): `gd` on `[[gh:org-lsp]]` (a #+LINK abbreviation) or
  `[[https://orgmode.org]]` says "No definition found." and nothing opens; repro below.
  Heading, custom id, target, file::*search, id links, hover, references, heading rename,
  workspace symbols, completion in `[[`: ok.
- Footnotes: ok (there, back from the label, "No link found" right after it).
- Tables: ok (column moves with #+TBLFM following, rows, insert/delete column and row,
  blank, sort by column). Shrink: no ellipsis until the next change (helix-8, documented).
- Formulas: helix-elisp-formula (server): on `#+TBLFM: $3='(+ $1 $2);N` Helix's code
  actions have no "Recalculate the table" (the vsum and date ones do), so the Emacs Lisp
  formula cannot be computed; repro below. `C-c C-c` on a #+TBLFM line is "Recalculate the
  table" among the code actions, not "Toggle the checkbox" as TESTING.org names it (doc).
- Babel: ok for a block (the question comes a moment later: Yes from the code actions,
  `#+RESULTS: 3`), tangle ok. helix-inline-babel (server): no "Evaluate the code block"
  action on `src_emacs-lisp{…}` or a `#+CALL:` line; repro below.
- Citations, diagnostics (INFO/HINT only), indentation (inlay hints), completion (#+STA,
  \alp): ok.
- Attachments: helix-gd-attachment (server): `gd` on `[[attachment:note.txt]]` does nothing
  (the file is in data/at/t-1/); repro below.
- Encryption: not possible in Helix: only didSave is sent, no willSaveWaitUntil capability
  (checked); TESTING.org lists the check (doc).

## Sublime Text (stand-ins only)

- Not run for real: no Sublime Text here.
- Hidden by the stand-ins before: `set_timeout` ran at once (a popup's hide after 4 s ran
  before the answer), LSP's answers came inside the request, `show_quick_panel` and
  `show_input_panel` answered at once, a panel shown over another did not cancel it, keys
  were never looked up in the keymap. Now: a loop with a clock of its own runs what is put
  off and the answers in time order; the user answers a panel 5 s later, if it is still
  shown; a new panel cancels the old one (on_select(-1)/on_cancel); `press()` finds the
  binding of Default.sublime-keymap whose contexts hold (asked of on_query_context) and
  runs its command. Two new tests (the agenda menu waits through a message's popup; a
  question over another gives that one up). No plugin bug found by them.
- Not modelled: Sublime's fuzzy ranking in the quick panel (the user goes down to the item),
  where a popup sits over a panel.

## Server-side, not fixed (repros)

1. helix-gd-external: Helix, file `#+LINK: gh https://example.org/%s` / `* Links` /
   `[[gh:org-lsp]] [[https://orgmode.org]]`; cursor in a link, `gd`: status line "No
   definition found.", no window/showDocument in Helix's log. `Server::definition`
   (server.rs ~4977) → `open_location` answers None for `Open::External`; org.open is not a
   code action in Helix, so web links and abbreviations cannot be followed there.
2. helix-gd-attachment: Helix, `* Task` with `:ID: att-1`, `[[attachment:note.txt]]`,
   `data/at/t-1/note.txt` beside it; `gd` on the link: nothing. `definition` does not
   resolve `Open::Attachment` (only `org.open` asks Emacs, server.rs ~5499).
3. helix-elisp-formula: Helix, `| 1 | 2 |   |` / `#+TBLFM: $3='(+ $1 $2);N`, cursor on the
   #+TBLFM line, Space a: no "Recalculate the table" (offered for `$3=$1*$2`). actions.rs
   `actions` drops an action whose in-server run fails (`edit::run(...).ok()?`), and the
   Emacs Lisp formula needs Emacs (table::RECALCULATE).
4. helix-inline-babel: Helix, `src_emacs-lisp{(* 6 7)}` and `#+CALL: three()` lines, Space
   a: no "Evaluate the code block or call at the cursor through Emacs": it is offered only
   `in_src_block` (server.rs ~4702).
5. Docs from scripts/features.mjs (not server code, outside this round's files): Helix and
   Zed TESTING.org list encrypting on save (neither sends willSaveWaitUntil); Zed's lists
   "cycling goes folded → children → subtree" (no key in Zed) and says "Save all Org files
   is not among its keys" (s is workspace::SaveAll); Helix's names "Toggle the checkbox"
   for C-c C-c on #+TBLFM (the action is "Recalculate the table"); docs/configuration.org's
   Zed settings lack `document_folding_ranges`.
