#!/usr/bin/env bash
# Run the Vim plugin's tests against the real server, once per LSP client:
# the plugin's own cases (test.vim), the highlighting of the shared sample
# (tests/e2e/highlight, by highlight.vim), then the shared end-to-end
# scenarios (tests/e2e/scenarios, by scenarios.vim), each scenario in a Vim
# of its own, a few at a time.
#
#   editors/vim/test/run.sh [lsp|vim_lsp|coc|ale ...]
#
# CLIENTS is the directory with the clients checked out: lsp
# (yegappan/lsp), vim-lsp, coc.nvim (its release branch) and ale. VIM is
# the Vim to use (9.1 or newer); ORG_LSP_BIN the server (default: the debug
# build). Vim runs in a tmux session, as it needs a terminal.
# ORG_LSP_VIM_TESTS is `cases` (with the highlighting), `highlight` or
# `scenarios` for only those;
# ORG_LSP_E2E=NAME runs only the scenarios whose name has NAME in it, and
# ORG_LSP_E2E_JOBS of them at a time (4). ORG_LSP_TEST_FILE,
# ORG_LSP_TEST_PLUGIN and ORG_LSP_TEST_RELEASES are passed on (download.sh
# sets them).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
clients="${CLIENTS:?set CLIENTS to the directory with the LSP clients}"
vim="${VIM_BIN:-vim}"
server="${ORG_LSP_BIN:-$root/target/debug/org-lsp}"
only="${ORG_LSP_VIM_TESTS:-}"
jobs="${ORG_LSP_E2E_JOBS:-4}"
[ "$#" -gt 0 ] || set -- lsp vim_lsp coc ale
status=0

# Start Vim with `client` in a tmux session `session` of `size` (COLSxROWS),
# working in `work`; the other arguments are variables for vimrc, over
# those it always has.
start() {
  local client="$1" session="$2" size="$3" work="$4"
  shift 4
  # The command tmux gives a shell, each word quoted: paths may have spaces.
  local command
  command="$(printf '%q ' env "ORG_LSP_TEST_CLIENT=$client" "ORG_LSP_TEST_WORK=$work" "ORG_LSP_TEST_CLIENTS=$clients" \
    "ORG_LSP_TEST_RESULT=$work/result.txt" "ORG_LSP_BIN=$server" ORG_LSP_CONFIG=/dev/null "HOME=$work" "$@" \
    "$vim" -N -u "$here/vimrc" -i NONE)"
  tmux new-session -d -s "$session" -x "${size%x*}" -y "${size#*x}" "$command; sleep 1"
}

# The result of the Vim in `session`, once it is written, then the session
# and its work directory gone. Prints what Vim shows when it did not finish.
finish() {
  local session="$1" work="$2" what="$3"
  [ -f "$work/result.txt" ] || tmux capture-pane -p -t "$session" 2>/dev/null | grep -v '^~ *$' || true
  tmux kill-session -t "$session" 2>/dev/null || true
  if [ -f "$work/result.txt" ]; then
    cat "$work/result.txt"
  else
    echo "not ok - $what: no result (Vim did not finish)"
  fi
  rm -rf "$work"
}

for client in "$@"; do
  output="$(mktemp)"
  if [ "$only" != scenarios ] && [ "$only" != highlight ]; then
    work="$(mktemp -d)"
    session="org-lsp-vim-$client-$$"
    start "$client" "$session" 200x50 "$work" \
      "ORG_LSP_TEST_FILE=${ORG_LSP_TEST_FILE:-}" "ORG_LSP_TEST_PLUGIN=${ORG_LSP_TEST_PLUGIN:-}" "ORG_LSP_TEST_RELEASES=${ORG_LSP_TEST_RELEASES:-}"
    # Up to five minutes: the tests wait for the server many times over.
    for _ in $(seq 1 3000); do
      [ -f "$work/result.txt" ] && break
      sleep 0.1
    done
    echo "== $client"
    finish "$session" "$work" tests | tee -a "$output"
  fi
  # The highlighting of the shared sample (tests/e2e/highlight), in a Vim
  # of its own: it starts the server with the sample's options.
  if { [ -z "$only" ] || [ "$only" = cases ] || [ "$only" = highlight ]; } && [ -z "${ORG_LSP_TEST_FILE:-}" ]; then
    work="$(mktemp -d)"
    session="org-lsp-vim-$client-highlight-$$"
    start "$client" "$session" 200x50 "$work" "ORG_LSP_TEST_FILE=$here/highlight.vim"
    for _ in $(seq 1 1200); do
      [ -f "$work/result.txt" ] && break
      sleep 0.1
    done
    echo "== $client: highlighting"
    finish "$session" "$work" highlighting | tee -a "$output"
  fi
  if [ "$only" != cases ] && [ "$only" != highlight ] && [ -z "${ORG_LSP_TEST_FILE:-}" ]; then
    echo "== $client: scenarios"
    names=()
    for file in "$root"/tests/e2e/scenarios/*.json; do
      name="$(basename "$file" .json)"
      [[ "$name" == *"${ORG_LSP_E2E:-}"* ]] && names+=("$name")
    done
    scenarios="$(mktemp)"
    started="$SECONDS"
    # The scenarios running: their work directories, and when each began.
    declare -A running=() began=()
    next=0
    while [ "$next" -lt "${#names[@]}" ] || [ "${#running[@]}" -gt 0 ]; do
      while [ "$next" -lt "${#names[@]}" ] && [ "${#running[@]}" -lt "$jobs" ]; do
        name="${names[$next]}"
        next=$((next + 1))
        work="$(mktemp -d)"
        # The scenario's files out of the home directory, which the server
        # would write as `~` in paths; as wide as the server's runner says
        # the agenda is.
        mkdir "$work/home"
        start "$client" "org-lsp-vim-$client-$name-$$" 80x24 "$work" "HOME=$work/home" "ORG_LSP_TEST_SCENARIO=$name" \
          "ORG_LISP_DIR=${ORG_LISP_DIR:-}" "ORG_LSP_REQUIRE_EMACS=${ORG_LSP_REQUIRE_EMACS:-}"
        running[$name]="$work"
        began[$name]="$SECONDS"
      done
      sleep 0.2
      for name in "${!running[@]}"; do
        work="${running[$name]}"
        # Up to three minutes a scenario.
        if [ -f "$work/result.txt" ] || [ $((SECONDS - began[$name])) -gt 180 ]; then
          finish "org-lsp-vim-$client-$name-$$" "$work" "$name" | tee -a "$scenarios"
          unset "running[$name]"
        fi
      done
    done
    unset running began
    echo "${#names[@]} scenarios, $(grep -c '^not ok' "$scenarios" || true) failed, $((SECONDS - started)) s"
    cat "$scenarios" >> "$output"
    rm -f "$scenarios"
  fi
  grep -q '^not ok' "$output" && status=1
  grep -q '^ok' "$output" || { echo "not ok: no test ran"; status=1; }
  rm -f "$output"
done
exit $status
