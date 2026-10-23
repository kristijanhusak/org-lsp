#!/usr/bin/env bash
# Run each Neovim suite file on its own (going on after a failure), in an
# isolated XDG. Usage: suite.sh <version: 0.12.0|stable|nightly> [tests...]
v=$1; shift
S=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/neovim
cd /home/user/org-lsp
export XDG_CONFIG_HOME=$S/xdg-$v/config XDG_DATA_HOME=$S/xdg-$v/data XDG_STATE_HOME=$S/xdg-$v/state XDG_CACHE_HOME=$S/xdg-$v/cache
mkdir -p $XDG_CONFIG_HOME
export ORG_LISP_DIR=/root/org-mode/lisp ORG_LSP_CONFIG=/nonexistent/org-lsp/config.toml
tests=${*:-e2e highlight agenda scenarios images latex download protocol}
for t in $tests; do
  start=$(date +%s)
  timeout 600 /root/nvim-$v/bin/nvim --headless -u NONE -l tests/nvim/$t.lua > $S/suite-$v-$t.log 2>&1
  echo "$v $t exit=$? $(( $(date +%s)-start ))s $(grep -o 'ok: [0-9]* checks passed' $S/suite-$v-$t.log) $(grep -m1 FAIL $S/suite-$v-$t.log)"
done
