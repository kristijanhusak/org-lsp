#!/usr/bin/env bash
# Run the Neovim end-to-end test against a fresh debug build of the server.
# Uses `nvim` from PATH, or the one named by $NVIM.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p org-lsp
exec "${NVIM:-nvim}" --headless -u NONE -l tests/nvim/e2e.lua
