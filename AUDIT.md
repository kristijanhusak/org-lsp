# Audit findings, round seven

A review of the fixes to the sixth round. They are on branch
`claude/elegant-meitner-ggxbid` (commit f0cea9c, "The server waits for no
download or program, and trusts no link with options or paths"), **not yet
on `main`**; `main` only gained #356 (the two parity counting items). This
round was run on that branch merged with `main`, in a separate worktree;
`check.sh` passes there. Earlier rounds are in this file's git history.

Three reviewers: one re-ran every round-six trigger the commit addresses,
one did an adversarial security review of exactly that diff, and one
confirmed which round-six items are still open, with current line numbers.
"Verified" means run here; no Emacs or Windows is available.

A correction to round six: **C2 was wrong.** The server uses lsp-server's
`initialize_finish`, which waits for the client's `initialized` before the
server loop starts, so nothing was ever sent early; the branch adds a test
proving it.

Deliberately left as they are, per the maintainer: unchanged list from the
earlier rounds (diagnostics debounce, workspace file list cache, Sublime
paths, coc.nvim on Windows, the Neovim edit version check, roam finalize
after a refiled capture, the encryption docs, tangle paths, the synchronous
Enter/o/Tab waits, the `zed-extension` hash).

---

## 1. Status of the round-six findings on the fix branch

| ID | Finding | Status |
| --- | --- | --- |
| P1 | outline quadratic from inline-task rescans | **Open**, `inlinetask.rs:62-67`, `outline.rs:518-524`. Re-timed: 2000 lines 62 ms, 8000 lines 967 ms (15.7× for 4×) |
| P2 | `hides_stars` O(lines²) | **Open**, `inlinetask.rs:39-41`, caller now `server.rs:15305`. Re-timed: 2000 lines 120 ms, 8000 lines 2.35 s, on a file with no inline task |
| P3 | remote images on the main loop | Fixed, verified: the preview returns at once, other requests are answered while curl runs, `org-lsp/images` arrives later with the document's version checked, a closed or changed document gets nothing; the 20 MB cap is curl's `--max-filesize`; one file per URL in a fresh 0700 directory. Residue: see 2.1, 2.2, 2.5 |
| P4 | duration regexes per call | **Open**, `duration.rs:228-268` |
| P5 | `id:` lookup parses every file | Fixed, verified: the link's file, then open documents by a regex scan, then only files whose kept IDs list it, one rescan on a miss |
| P6 | parser quadratic scans | **Open**, `objects.rs:656-660`, `:713-741`, `:944-953` |
| P7 | 557 KB search index on every page | **Open**, `site/src/main.rs:308`, `search.rs:18` |
| P8 | subprocesses without a deadline | Fixed for `man`, `info`, `ctags` and `dot` through the new `process.rs` (60 s, process group killed, reaped, both pipes drained). Residue: see 2.3, 2.4 |
| P9 | Neovim preview options dump, foldtext per character | **Open**, `preview.lua:166-174`, `:226-243`; `folds.lua:210-223`, `:273` |
| S1 | `man:` words reach `man` as options | **Partly**: words starting with `-` refused, `man 2 chmod` and `chmod(2)` still work; a word containing `/` still makes man-db render any readable file, see 2.2 |
| S2 | `org-file-apps` quoting on Windows | Fixed as `shell-quote-argument`'s w32 branch (traced on `x&calc.pdf`, `x"calc.pdf`, `%PATH%.pdf`, `x^.pdf`: no injection), unverified on Windows; the transport through `Command::arg` is suspect, see 2.6 |
| S3 | org-protocol `open-source` follows `..` | Fixed, verified: file and working directory both resolved lexically then canonicalised, component-wise prefix check; `%2e%2e`, symlinks inside pointing outside, sibling directories with the same prefix all refused; written down as a difference from Emacs. Residue: an empty `working_directory` disables it, see 2.7 |
| S4 | clock save non-atomic | Fixed (`replace_whole`), verified |
| S5 | socket path too long | Fixed, verified: `/tmp/org-lsp-UID` made 0700 with a plain `mkdir`, reused only when a real directory of that owner with mode exactly 0700 (a pre-planted symlink or another user's directory is refused), socket unlinked before bind and 0600, failure shown as a message |
| S6 | `install.sh` skips checksums on a transient failure | **Open**, `install.sh:54-89`, `:500-503` |
| C1 | debug overflow panics | **Open**, `todo.rs:1617`, `calc.rs:625-629`, `:668`, `dailies.rs:275-289` |
| C2 | messages before `initialized` | Not a bug (see above) |
| C3 | extract path not filled as a capture template | Fixed, verified; a template needing a prompt (`%^{…}`) is refused, a difference stated only in a code comment |
| C4 | smaller divergences | **Open**, all |
| C5 | editor and test items | **Open**, all: `view.lua:999-1003` and Vim's `s:echo_path` echo in any mode; `extension.ts:661-669` has `void` but no `.catch`; `run.mjs` temp dirs; `run.py:545` log handle; `run.sh:36-37` quoting; `e2e_scenarios.rs:864`; the three reference scripts without a timeout; `highlight-screenshots.py:170` |
| 4a.1-3 | message texts (`shell:` prompt quotes, remote-resource prompt, `Resume clock`) | **Open** |
| 4a.4-5 | parity counting | Fixed on `main` by #356; the options table numbers match the triage file exactly |
| 4b | Helix and Zed checklists, Helix ✅ rows, `docs/vim.org` version, `docs/sublime.org` contradiction | **Open**, all |
| info | `shell:` output uncapped | Fixed (first 50 lines or 8000 characters, then a count of the rest) |

---

## 2. Regressions and residue in the fix commit

### 2.1 Medium — curl runs without `--globoff`: one link fetches many URLs and shows the wrong file
`server/org-lsp/src/server/remote_images.rs:321-327`: curl expands `{a,b}` and `[1-N]` in a URL, and Org's plain-link syntax admits both, so `https://h/x[1-3].png` and `[[https://h/{a,b}.png]]` reach curl as written.
**Verified** over LSP with a local HTTP server: a document with `{a,b}.png` and `x[1-3].png` links made the server request `/a.png`, `/b.png`, `/x1.png`, `/x2.png`, `/x3.png`, and the preview item for the first link pointed at a file holding the second URL's body. Consequences: a link such as `http://h/i[1-100000].png` makes one preview job issue 100 000 requests, each with its own 30 s and 20 MB limits, with nothing to cancel it; the user is asked about one URL and curl fetches others; a legitimately named `photo[1].png` is fetched from the wrong URL. Emacs's `url-retrieve` does no globbing.
Fix: add `--globoff` and a test with a `{a,b}` URL expecting exactly one request.

### 2.2 Medium — A `man:` word containing `/` still renders any readable file
`server/org-core/src/manual.rs:48-53` (`man_refused`) refuses only words starting with `-`; man-db treats any name containing `/` as a file to render, so `[[man:/etc/hostname]]` and `[[man:./x.1]]` have S1's exposure. **Verified** that the refusal does not trigger (man-db's behaviour from its documentation; the `man` here is a stub). Fix: refuse words containing `/` too, and pass `--`.

### 2.3 Low — Programs' output is read whole into memory before any cap
`server/org-lsp/src/process.rs:78-93` (`read_to_end` on reader threads): the `shell:` cap (50 lines or 8000 characters) is applied after everything is read, and `shell:` links run with no deadline. **Verified**: `head -c 300000000 /dev/zero` returned 300 MB in under a second; `yes` would allocate at pipe speed until the 60 s deadline, or forever for a `shell:` link. `man`, `info`, `ctags` and `dot` output is read whole likewise. A `shell:` link needs the user's `y`, so this is robustness. Fix: read through `Read::take(LIMIT)` and stop the process at the limit.

### 2.4 Low — The group kill is Unix-only and needs an external `kill`
`process.rs:70-77`, `140-157`: on Unix the child gets its own process group, killed with the `kill` binary before `wait` (**verified**: no zombie, a backgrounded grandchild dies; a grandchild that calls `setsid` escapes, which is inherent). On Windows only the direct child is killed, so what `cmd.exe` or `man.exe` started runs on; the module comment says "with what they started". Without a `kill` binary only the leader dies. A doc note, or a job object on Windows.

### 2.5 Low — Remote image residue
- `remote_images.rs:326`: `--max-filesize` has no effect before curl 8.4.0 on a response without Content-Length (Ubuntu 22.04 ships 7.81, Debian 12 7.88), and nothing checks the size after the download, so "an image larger than 20 MB is not downloaded" (`configuration.org:7656`, `protocol.org:513`) is not true there. **Verified** on curl 8.5.0 that both cases are refused. Fix: check `metadata(partial).len()` before the rename.
- `:253-263`: one detached thread per preview job, downloads serial, no concurrency limit and no cancellation when the document changes or closes (only the result is dropped), so a client previewing repeatedly, or `download` mode, accumulates threads and curl processes.
- `server.rs:13640-13644`: a preview with any remote URL defers all its items, local file images included, until the whole job ends; before, local images showed in the command's answer.
- `:273-278`: "Unable to fetch file" messages are shown before the version check, so a closed document still produces them.
- `:296-311`: a URL whose last segment is over 255 bytes fails on the output file name with ENAMETOOLONG after the download (**verified**); truncate the name, keeping the extension. Names otherwise cannot collide (a counter prefix) or leave the directory (no separators survive).

### 2.6 Low (Windows only, unverified) — The `cmd /C` line is passed through `Command::arg`
`server/org-lsp/src/process.rs:57-60` (`shell`): std's Windows argument quoting wraps a line containing spaces in `"…"` and escapes inner quotes as `\"`, but `cmd.exe` does not parse with `CommandLineToArgvW`: it strips the outer quotes and keeps the backslashes. Re-traced with that mangling, S2's quoting still blocks injection (quote regions toggle at the same places and every metacharacter keeps its caret), but the viewer receives literal `\"` and `^` in the file name, so `org-file-apps` commands with `%s` most likely do not open the file on Windows at all. Pre-existing on `main`. Fix: `CommandExt::raw_arg(format!("/C {line}"))`, as std's docs recommend.

### 2.7 Low — An empty `working_directory` disables the open-source confinement
`server/org-lsp/src/protocol.rs:560-580` (`source_file`): `resolved("")` is an empty path and `Path::starts_with("")` is true for every path. **Verified**: with `working_directory: ""` and file `/etc/passwd` the result is `Ok(Some("/etc/passwd"))`. The schema (`settings.rs:4514`) requires the key but allows `""`. A misconfiguration, and Emacs behaves the same, but the check is meant as the backstop. Fix: refuse an empty or relative working directory at load or in `source_file`.

### 2.8 Nits
`protocol.rs:139-144` falls back to `/tmp` on any bind error, not only a long path, and takes the uid from the state directory's owner rather than the process (odd only under `sudo`). The `%^{…}` refusal in `org_roam_extract_new_file_path` is a deliberate difference not in `docs/plugins.org`.

---

## 3. Checked and found sound on the fix branch

- curl: `--proto =http,https --proto-redir =http,https` kept, `--` before the URL, URLs always start with `http(s)://`, so no option or `file:` redirect from a document; `--fail` and the partial-then-rename keep failed downloads from being shown; the `prompt` policy is asked once per URL with answers keyed by URL, a `didChange` between question and answer re-runs the preview on the new text, startup asks nothing; the images directory is a fresh `create_new` 0700 directory with a random name under the real temp directory; no ImageMagick runs in the server.
- `info:` passes the file after `--file` and the node after `--node`, never as a bare word.
- The socket fallback's directory checks, the entry file naming the socket so `org-lsp protocol` finds it, the token model unchanged; the commit's test delivers a URL through the fallback.
- `source_file`: `sub/../ok.org` allowed, `../`, `sub/../../`, deep missing paths with `..`, symlinked files and directories pointing outside, and `proj2` vs `proj/` all refused; `store-link` and `capture` take `url=file:///…` only as text for `%:link`, as Emacs does.
- `clock_persist`: temp file beside the target with `create_new`, permissions copied, fsync, rename; the parent directory still created.
- `find_id`: `track_globally` off still returns nothing; open documents searched from kept parses; a dangling ID costs one walk, as before.
- `process.rs`: both pipes drained concurrently, zombies reaped in both paths, the four former blocking sites all converted; the remaining `.output()` calls are `hostname` and `ps`.
- The download thread touches no server state, its results are consumed under `guarded`, and it dies with the process at exit.
- Editors: `images.apply(preview, quiet)`, VS Code `applyPreview(params, quiet)` and Sublime `apply_preview(params, quiet=)` exist with those signatures; the startup paths tolerate the command's null answer; a non-startup preview still reports as Emacs does.
- Docs on the branch: `protocol.org` (null answer, `org-lsp/images` with `kind: link` and `startup: true`), `org-protocol.org` (the `/tmp/org-lsp-UID` rules, the refusal message), the parity rows for the `man:` refusal, the open-source confinement and the `shell:` cap all match the code; every generator passes in check mode; `docs/config.toml`'s one change is a comment, the default `org_display_remote_inline_images = "skip"` unchanged everywhere.
- `shell_quote_w32`: twelve unit cases match Emacs's w32 branch, including backslashes before a newline.
