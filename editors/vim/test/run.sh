#!/usr/bin/env bash
# Run the Vim plugin's tests against the real server, once per LSP client.
#
#   editors/vim/test/run.sh [lsp|vim_lsp|coc|ale ...]
#
# CLIENTS is the directory with the clients checked out: lsp
# (yegappan/lsp), vim-lsp, coc.nvim (its release branch) and ale. VIM is
# the Vim to use (9.1 or newer); ORG_LSP_BIN the server (default: the debug
# build). Vim runs in a tmux session, as it needs a terminal.
# ORG_LSP_TEST_FILE, ORG_LSP_TEST_PLUGIN and ORG_LSP_TEST_RELEASES are
# passed on (download.sh sets them).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
clients="${CLIENTS:?set CLIENTS to the directory with the LSP clients}"
vim="${VIM_BIN:-vim}"
server="${ORG_LSP_BIN:-$root/target/debug/org-lsp}"
[ "$#" -gt 0 ] || set -- lsp vim_lsp coc ale
status=0
for client in "$@"; do
  work="$(mktemp -d)"
  result="$work/result.txt"
  session="org-lsp-vim-$client-$$"
  tmux new-session -d -s "$session" -x 200 -y 50 \
    "ORG_LSP_TEST_FILE=${ORG_LSP_TEST_FILE:-} ORG_LSP_TEST_PLUGIN=${ORG_LSP_TEST_PLUGIN:-} ORG_LSP_TEST_RELEASES=${ORG_LSP_TEST_RELEASES:-} ORG_LSP_TEST_CLIENT=$client ORG_LSP_TEST_WORK=$work ORG_LSP_TEST_CLIENTS=$clients ORG_LSP_TEST_RESULT=$result ORG_LSP_BIN=$server ORG_LSP_CONFIG=/nonexistent/config.toml HOME=$work $vim -N -u $here/vimrc -i NONE; sleep 1"
  # Up to five minutes: the tests wait for the server many times over.
  for _ in $(seq 1 3000); do
    [ -f "$result" ] && break
    sleep 0.1
  done
  echo "== $client"
  # What Vim shows, when it did not finish.
  [ -f "$result" ] || tmux capture-pane -p -t "$session" 2>/dev/null | grep -v '^~ *$' || true
  tmux kill-session -t "$session" 2>/dev/null || true
  if [ -f "$result" ]; then
    cat "$result"
    grep -q '^not ok' "$result" && status=1
    grep -q '^ok' "$result" || { echo "not ok: no test ran"; status=1; }
  else
    echo "not ok: no result (Vim did not finish)"
    status=1
  fi
  rm -rf "$work"
done
exit $status
