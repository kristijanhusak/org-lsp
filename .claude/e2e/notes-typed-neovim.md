# Neovim, typed: notes, 2026-10-25

Neovim's real terminal UI in tmux (80x24, `updatetime` 100), keys typed with
`tmux send-keys`, 30-80 ms apart. Nightly not tried. Harness: the typed
scenarios (`ORG_LSP_NVIM_TESTS=typed scripts/test-nvim.sh`), and for the
checklist a scratch `tui.sh`/`k.sh` like `.claude/e2e/repro/neovim/tui.sh`
(server: the worktree's debug build; clock 2026-05-18 10:30).

## Typed scenarios
- All 16 shared scenarios pass typed on 0.12.0 and 0.12.5, with Neovim's own
  typed checks `tests/nvim/typed/date-preview.json` and `choices-wrap.json`.
- Found by them: a key prompt longer than the echo area (the effort filter's
  `Effort operator? (> = or <)     or press `_' again to remove filter:`, 71
  columns of 68) gave a hit-enter prompt before the key was read, and after
  Enter the prompt was gone while the key was still awaited. Fixed in 68b90e0
  (the command line grows while it asks); `agenda-filters` typed fails without it.
- Not a bug, but what the harness taught: Neovim holds scheduled callbacks
  while typed keys may start a mapping, so a Neovim that types to itself
  from its own timers stalls `\` for 'timeoutlen'; the runner types from
  outside. A `<C-c>` with keys still unread before it drops them (Neovim's
  typeahead flush): keys are typed one at a time. tmux takes a lone `;` as
  its command separator even after `-l` (fixed in the runner, 29b6b9a).

## TESTING.org, by group
- Setup: `:checkhealth org-lsp` ok; `g?` (Org file) ok; `<Leader>oa` from the health buffer ok. vim.pack install, `:OrgLspDownload`, config.toml not redone (no network; the earlier round's pack.sh covered them).
- Highlighting: not redone by hand (tests/nvim/highlight.lua, typed screens look right: links concealed, tags aligned).
- Folding: TAB on a headline, `TAB` on a new `** ` line toggles `* `/`** ` (neovim-16 is gone); C-k on a folded headline with `org_ctrl_k_protect_subtree` asks, `n` says /‘C-k’ aborted as it would kill a hidden subtree/: ok.
- Headings: `<Leader><CR>`, `>>`: ok (spot check only).
- Lists, Priorities: `ciR`, `<Leader>o,` + `B` (/Priority of current item set to B/): ok.
- TODO keywords: `cit` menu stays up past CursorHold (2 s), `w` (WAIT(w@)) opens the note buffer, `<C-c>` files the note, `<Leader>ok` keeps WAIT without one: ok.
- Tags: `<Leader>ot` fast selection, the title shows the tags so far, `m` replaces `@office` (exclusive group), RET: ok.
- Properties and column view: `<Leader>oxc` (Emacs), `n` on TODO, `v` (/Value is: TODO/): ok. `e` on PRI asks /Priority A-C, SPC to remove:/ as typed text (one key in Emacs): server, see S2.
- Dates: calendar moves (`l`, `j`, `<S-Right>`, `<M-S-Right>`, `>`, `.`), `t` with `l`/`k` (10:30 → 10:40), RET: ok. What a typed date means came a key late on the calendar's last line (/+2    => <2026-05-20 Wed>/ with `+2d` typed): fixed in bcb8b77, test `tests/nvim/typed/date-preview.json`. `<C-a>`, `3<C-x>`, `<S-Up>` on a timestamp's day and hour: ok.
- Dates, typed ahead: `<Leader>ois` then `i+2d<CR>` typed within a few ms (faster than a person, as a paste): `i` goes to the Org file before the calendar shows, then the calendar opens in insert mode and `<CR>` splits one of its lines. 30 ms apart works, and so does a macro (keys already there when the command starts). Not fixed: see the report.
- Agenda: menu, `a`, `f`, `b`, `vd`, `vw`, `J` + `i` + date, `.`, `t` menu + `d`, `+`, `\` filter (waits 'timeoutlen' first, as `\` starts the leader keys) + `w`, `|`, `m` `m` `B` `+` tag: ok; `F` follow, outline path echo, `K` hover: ok. Bulk `+` offers only tags used in headlines: server, S1.
- Clocking: `<Leader>oxi` (/Clock starts at …/), `clock_status()` /[0:00/0:30] (Write report)/; after a restart `<Leader>oxz` asks /Dangling clock started 0 mins ago [jkKtTgGSscCiq]?/; `K`, Enter: /Clock stopped at … after 0:00/. The choices window hid its last choices when lines wrapped ([C] cut with @@@, [j] and [J] gone at 80 columns): fixed in 7495e19, test `tests/nvim/typed/choices-wrap.json`.
- Capture, refile: the typed `capture` and `agenda-entries` scenarios (menu, sub-menu, `%^{Name}` prompt, refile through `inputlist()`). /Refile to "Projects (work.org)" in file …: done/ is longer than 80 columns and gives a hit-enter prompt (Neovim's, for any long message; Emacs's echo area grows).
- org-protocol: not done by hand (tests/nvim/protocol.lua).
- Notes (org-roam): `<Leader>nf` with `Fi<Tab>` completes the alias /First/ and visits alpha.org; `<Leader>nl` shows /Backlinks (1)/ beside: ok.
- Journal: the typed `journal-calendar` scenario only.
- Refile and archive: typed `agenda-entries` only.
- Links: `[[No such heading]]` asks, `y` adds the heading; `[[shell:echo hi]]` asks, `y` says /hi/: ok.
- Footnotes: `<Leader>of` on `[fn:9]` without a definition asks /No definition for 9.  Create one?:/ through `inputlist()` (the `?:` is Neovim's own `window/showMessageRequest` handler, which adds `:`).
- Tables: `<Leader>oi|` asks the size, `3x2` makes the table (Emacs): ok.
- Table formulas: `<Leader>o{` then `<Leader>o*` shows the substitution history and asks /Debugging Formula.  Continue to next?/, `2` says /Abort/: ok.
- Code blocks: `<Leader>obe` asks through `inputlist()` (Yes/No), `1` puts `#+RESULTS: : 42`: ok.
- Export: `<Leader>oe` menu, `C-b` toggles /Body only: On/, `t` sub-menu, `U`: Emacs's error /Definition not found for footnote 9/ (the file has a dangling `[fn:9]`): menu ok.
- Citations: `<Leader>oic` lists the entries, `1`, then `(done)`: inserts `[cite:@knuth1984]` before the cursor's character (point is before the character the cursor is on): ok.
- Timers: `<Leader>ox;` asks /How much time left? (minutes or h:mm:ss)/, `1`; `clock_status()` counts down (/<0:00:59>/, /<0:00:55>/); after a minute /Last: time out/: ok.
- Attachments: `<Leader>oxa` menu, `c` asks /File to keep as an attachment:/ with no file name completion (Tab types a tab): server, S3.
- Diagnostics: information signs (`I`) and hints (`H`) only: ok (seen in passing).
- Not done by hand this round: highlighting options, sparse trees, narrowing, sorting, inline tasks, kill/yank of subtrees, property delete/drawer `C-c C-c`, deadline warnings, `org_enforce_todo_dependencies`, super agenda, habits, appointments, capture templates `%a` `%c` `%[file]` `%(sexp)`, org-protocol, journal by hand, ctags/man links, export to file and opened, Babel tangle/detangle, citations' BibTeX commands, encryption, indentation, images and LaTeX (need Neovim 0.13 and a Kitty terminal).

## Server-side (not fixed)
- S1: bulk `+` in the agenda (`m` on two lines, `B`, `+`) completes only the tags used in headlines (`agenda_tags`, server.rs), not those of `#+TAGS:` or `org_tag_alist` as Emacs's `org-global-tags-completion-table` does. Repro: work.org with `#+TAGS: work(w) home(h)` and only `:work:` used; `<Leader>oaa`, `m`, `B`, `+`, `ho<Tab>`: nothing to complete.
- S2: column view (Emacs, `<Leader>oxc` on a headline) `e` on the PRIORITY column asks /Priority A-C, SPC to remove:/ as text: `C` waits for Enter (Emacs reads one key).
- S3: prompts for a file name (`<Leader>oxa` `c` /File to keep as an attachment/, `s` /Attachment directory/) are plain text: no file name completion, as `read-file-name` has in Emacs. Needs a prompt kind for files in the protocol.
