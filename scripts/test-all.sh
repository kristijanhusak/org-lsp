#!/usr/bin/env bash
# Everything before a pull request, side by side once the server is
# built: the fast checks (scripts/check.sh) and the end-to-end tests of
# each editor against the real server, each into a log of its own; the
# log of one that fails is shown.
#
#   ORG_LISP_DIR   the `lisp` directory of an Org 9.8.10 checkout
#   NVIM           the Neovim to run (default: nvim from PATH)
#   CLIENTS        the directory with Vim's LSP clients checked out (see
#                  editors/vim/test/run.sh); the Vim tests are skipped
#                  without it
#   VSCODE_EXECUTABLE  the VS Code (or VSCodium) to run; without it the
#                  test runner downloads VS Code
#   HX             the Helix to run the end-to-end scenarios in (see
#                  editors/helix/test/run.py); skipped without it
#   ZED            Zed's command line (bin/zed); the Zed scenarios are
#                  skipped without it. ZED_ORG_GRAMMAR is the Org grammar
#                  it needs (see editors/zed/test/run.py)
#   ORG_LSP_SERIAL set to 1 to run the editors one after another, their
#                  output as it comes
#
# The VS Code tests need a display: they run under xvfb-run when there is
# none. Zed's scenarios take their X displays from 120 up, away from the
# ones xvfb-run takes (99 up).
set -euo pipefail
cd "$(dirname "$0")/.."
# The server the editors run, which check.sh's build leaves as it is.
cargo build -q --workspace

vim_tests() {
  editors/vim/test/run.sh
  editors/vim/test/download.sh
}

vscode_tests() {
  cd editors/vscode
  npm run -s build
  if [ -z "${DISPLAY:-}" ] && command -v xvfb-run > /dev/null; then
    xvfb-run -a npm test
  else
    npm test
  fi
}

suites=(check neovim)
[ -n "${CLIENTS:-}" ] && suites+=(vim) || echo "== vim: skipped (set CLIENTS)"
[ -n "${HX:-}" ] && suites+=(helix) || echo "== helix: skipped (set HX)"
[ -n "${ZED:-}" ] && suites+=(zed) || echo "== zed: skipped (set ZED)"
suites+=(vscode)

run_suite() {
  case "$1" in
    check) scripts/check.sh ;;
    neovim) scripts/test-nvim.sh ;;
    vim) vim_tests ;;
    helix) editors/helix/test/run.py ;;
    zed) ORG_LSP_ZED_DISPLAY="${ORG_LSP_ZED_DISPLAY:-120}" editors/zed/test/run.py ;;
    vscode) vscode_tests ;;
  esac
}

if [ "${ORG_LSP_SERIAL:-}" = 1 ]; then
  for suite in "${suites[@]}"; do
    echo "== $suite"
    run_suite "$suite"
  done
  exit 0
fi

logs="$(mktemp -d)"
declare -A pids=() began=()
for suite in "${suites[@]}"; do
  (run_suite "$suite") > "$logs/$suite.log" 2>&1 &
  pids[$suite]=$!
  began[$suite]=$SECONDS
done
failed=0
# Each as it finishes, with how long it took.
while [ "${#pids[@]}" -gt 0 ]; do
  wait -n "${pids[@]}" 2> /dev/null || true
  for suite in "${!pids[@]}"; do
    if ! kill -0 "${pids[$suite]}" 2> /dev/null; then
      status=0
      wait "${pids[$suite]}" || status=$?
      took=$((SECONDS - began[$suite]))
      if [ "$status" -eq 0 ]; then
        echo "== $suite: passed ($((took / 60))m $((took % 60))s)"
      else
        echo "== $suite: FAILED ($((took / 60))m $((took % 60))s), the end of $logs/$suite.log:"
        tail -n 40 "$logs/$suite.log"
        failed=1
      fi
      unset "pids[$suite]"
    fi
  done
done
echo "logs: $logs"
exit "$failed"
