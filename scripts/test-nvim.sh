#!/usr/bin/env bash
# Run the Neovim end-to-end tests against a fresh debug build of the server.
# Uses `nvim` from PATH, or the one named by $NVIM. The shared scenarios
# run typed, in Neovim's terminal UI in tmux, as a user types them.
# ORG_LSP_NVIM_TESTS=typed runs only those, the ones named in the arguments
# or all; ORG_LSP_NVIM_TESTS=headless runs them headless instead, their keys
# fed and the prompts answered: the same steps, quicker to look into.
#
#   ORG_LSP_NVIM_TESTS=typed scripts/test-nvim.sh [NAME…]
#   ORG_LSP_NVIM_TESTS=headless scripts/test-nvim.sh [NAME…]
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
# The tests' own options only, whatever configuration file the machine has.
export ORG_LSP_CONFIG=/dev/null
if [ "${ORG_LSP_NVIM_TESTS:-}" = headless ]; then
  echo "== tests/nvim/scenarios.lua"
  exec "${NVIM:-nvim}" --headless -u NONE -l tests/nvim/scenarios.lua "$@"
fi
if [ "${ORG_LSP_NVIM_TESTS:-}" != typed ]; then
  for test in tests/nvim/e2e.lua tests/nvim/highlight.lua tests/nvim/syntax.lua tests/nvim/agenda.lua tests/nvim/images.lua tests/nvim/latex.lua tests/nvim/download.lua tests/nvim/protocol.lua; do
    echo "== $test"
    "${NVIM:-nvim}" --headless -u NONE -l "$test"
    echo
  done
fi
echo "== tests/nvim/scenarios.lua --typed"
"${NVIM:-nvim}" --headless -u NONE -l tests/nvim/scenarios.lua --typed "$@"
