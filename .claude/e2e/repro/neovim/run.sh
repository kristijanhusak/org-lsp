#!/usr/bin/env bash
# run.sh <version> <script.lua> [extra env]: one of the tester's scripts, isolated XDG.
v=$1; s=$2
S=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/neovim
X=$S/xdg-run-$v-$(basename $s .lua)
rm -rf $X; mkdir -p $X/config $X/data $X/state $X/cache
export XDG_CONFIG_HOME=$X/config XDG_DATA_HOME=$X/data XDG_STATE_HOME=$X/state XDG_CACHE_HOME=$X/cache
export ORG_LISP_DIR=${ORG_LISP_DIR:-/root/org-mode/lisp}
export ORG_LSP_CONFIG=${ORG_LSP_CONFIG-/nonexistent/org-lsp/config.toml}
cd /home/user/org-lsp
timeout ${T:-300} /root/nvim-$v/bin/nvim --headless -u NONE -l .claude/e2e/repro/neovim/$s 2>&1
echo "exit $?"
