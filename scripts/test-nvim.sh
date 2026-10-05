#!/usr/bin/env bash
# Run the Neovim end-to-end tests against a fresh debug build of the server.
# Uses `nvim` from PATH, or the one named by $NVIM.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
for test in tests/nvim/e2e.lua tests/nvim/agenda.lua; do
  echo "== $test"
  "${NVIM:-nvim}" --headless -u NONE -l "$test"
  echo
done
