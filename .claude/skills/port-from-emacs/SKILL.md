---
name: port-from-emacs
description: Port an Org mode feature from the Emacs sources into org-core and prove it behaves as Emacs does, with recorded cases and differential fuzzing; or decide to run it in Emacs through the bridge instead. Use when adding or fixing Org behaviour (editing, agenda, dates, tables, lists, links…) in the server.
---

# Porting an Org feature from Emacs

The reference is Org 9.8.10, in the `lisp` directory that `ORG_LISP_DIR`
points at. Every ported area has the same four parts; `sort` is a small,
complete example to copy:

| Part | Example |
| --- | --- |
| Reference driver: runs Emacs in batch on JSON cases, records what Org does | `scripts/sort_reference.py` |
| Fixture writer: hand-picked cases, recorded | `scripts/update-sort-fixtures.py` → `tests/sort/cases.json` |
| Rust test: replays the recorded cases against the port | `server/org-core/tests/sort.rs` |
| Fuzzer: random cases, recorded, replayed through the same test | `scripts/fuzz-sort.py` (sets `ORG_SORT_CASES`) |

## Steps

1. **Read the Org source first**, the command and everything it calls, in
   `$ORG_LISP_DIR`. Note its options, prompts, messages and errors (the
   port has to say exactly what Emacs says), and what it reads from the
   buffer.
2. **Decide: port, or the bridge.** Port what is pure text work. Run in
   Emacs through the bridge (`server/org-lsp/src/bridge.el`, a function
   `org-lsp-bridge-NAME` called as method `NAME` from `emacs.rs`) what
   would be a lot of code for little use, needs Lisp (user functions,
   `%(sexp)`), or needs Emacs's own machinery (export, Babel, LaTeX
   images, org-crypt). Bridge code must run in batch: no window system,
   no prompts (answer them through `org-lsp-bridge--ask`).
3. **Write the reference driver.** Cases as JSON (text, point in
   characters from the start, arguments); `reference(cases)` adds
   `expected`: the text and point after, the message, or `{"error"}`.
   Fix the clock (`NOW`, `faketime`) and locale (C) so the run is
   reproducible.
4. **Record hand-picked cases** with the fixture writer: the ordinary
   case, every option, the edges (end of buffer without a newline, empty
   entries, the cursor before the first headline, errors).
5. **Port into `server/org-core`**, following Emacs's structure and names
   so that the two can be read side by side; comments name the Emacs
   function each part comes from. Write the Rust test that replays the
   cases.
6. **Fuzz** until a few thousand random cases pass
   (`scripts/fuzz-NAME.py --count 2000 --seed N`, several seeds). Turn
   each real difference found into a hand-picked case before fixing it.
   A difference kept on purpose is written down in the code and in
   `docs/parity.org` (`scripts/features.mjs`).
7. **Wire it into the server** (`server/org-lsp`): the command in
   `commands.rs` with its Emacs key, options in `settings.rs`, the
   handler in `server.rs`; an LSP test in `server/org-lsp/tests/`.
8. **Record it** in the table of `PLAN.org` ("How the work is checked"), in
   `scripts/features.mjs` (Emacs feature, editors' support, manual checks),
   then `scripts/regen.sh`. If editors need code for it, continue with the
   `editor-feature` skill.
9. **Check**: `scripts/check.sh` (with `ORG_LISP_DIR`), then
   `scripts/test-all.sh` before the pull request.
