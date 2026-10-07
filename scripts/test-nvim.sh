#!/usr/bin/env bash
# Run the Neovim end-to-end tests against a fresh debug build of the server.
# Uses `nvim` from PATH, or the one named by $NVIM.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
# The tests' own options only, whatever configuration file the machine has.
export ORG_LSP_CONFIG=/nonexistent/org-lsp/config.toml
for test in tests/nvim/e2e.lua tests/nvim/agenda.lua tests/nvim/download.lua; do
  echo "== $test"
  "${NVIM:-nvim}" --headless -u NONE -l "$test"
  echo
done
