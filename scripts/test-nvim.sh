#!/usr/bin/env bash
# Run the Neovim end-to-end tests against a fresh debug build of the server.
# Uses `nvim` from PATH, or the one named by $NVIM. The shared scenarios
# run twice: headless, then typed, in Neovim's terminal UI in tmux, as a
# user types them. ORG_LSP_NVIM_TESTS=typed runs only the typed ones, those
# named in the arguments or all:
#
#   ORG_LSP_NVIM_TESTS=typed scripts/test-nvim.sh [NAME…]
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
# The tests' own options only, whatever configuration file the machine has.
export ORG_LSP_CONFIG=/dev/null
if [ "${ORG_LSP_NVIM_TESTS:-}" != typed ]; then
  for test in tests/nvim/e2e.lua tests/nvim/highlight.lua tests/nvim/agenda.lua tests/nvim/scenarios.lua tests/nvim/images.lua tests/nvim/latex.lua tests/nvim/download.lua tests/nvim/protocol.lua; do
    echo "== $test"
    "${NVIM:-nvim}" --headless -u NONE -l "$test"
    echo
  done
fi
echo "== tests/nvim/scenarios.lua --typed"
"${NVIM:-nvim}" --headless -u NONE -l tests/nvim/scenarios.lua --typed "$@"
