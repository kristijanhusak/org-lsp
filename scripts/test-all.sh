#!/usr/bin/env bash
# Everything before a pull request: the fast checks (scripts/check.sh),
# then the end-to-end tests of each editor against the real server.
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
#
# The VS Code tests need a display: they run under xvfb-run when there is
# none.
set -euo pipefail
cd "$(dirname "$0")/.."
scripts/check.sh
scripts/test-nvim.sh
if [ -n "${CLIENTS:-}" ]; then
  editors/vim/test/run.sh
  editors/vim/test/download.sh
else
  echo "== vim: skipped (set CLIENTS)"
fi
if [ -n "${HX:-}" ]; then
  echo "== helix: scenarios"
  editors/helix/test/run.py
else
  echo "== helix: skipped (set HX)"
fi
if [ -n "${ZED:-}" ]; then
  echo "== zed: scenarios"
  editors/zed/test/run.py
else
  echo "== zed: skipped (set ZED)"
fi
cd editors/vscode
npm run -s build
if [ -z "${DISPLAY:-}" ] && command -v xvfb-run > /dev/null; then
  xvfb-run -a npm test
else
  npm test
fi
