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
cd editors/vscode
npm run -s build
if [ -z "${DISPLAY:-}" ] && command -v xvfb-run > /dev/null; then
  xvfb-run -a npm test
else
  npm test
fi
