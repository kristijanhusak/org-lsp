#!/usr/bin/env bash
# tui.sh start <name> <file> [json] | keys <name> <keys...> | show <name> | stop <name>
S=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/neovim
T="tmux -L neovimtester"
cmd=$1; name=$2; shift 2
case $cmd in
  start)
    X=$S/xdg-tui-$name; rm -rf $X; mkdir -p $X/{config,data,state,cache}
    $T new-session -d -s $name -x ${COLS:-120} -y ${ROWS:-40} "env XDG_CONFIG_HOME=$X/config XDG_DATA_HOME=$X/data XDG_STATE_HOME=$X/state XDG_CACHE_HOME=$X/cache ORG_LSP_CONFIG=/nonexistent ORG_LISP_DIR=/root/org-mode/lisp ORG_LSP_JSON='${2:-{\}}' ${NV:-/root/nvim-stable/bin/nvim} -u /home/user/org-lsp/.claude/e2e/repro/neovim/tui-init.lua $1"
    sleep ${WAIT:-3};;
  keys) $T send-keys -t $name "$@"; sleep ${WAIT:-1};;
  show) $T capture-pane -p -t $name;;
  stop) $T kill-session -t $name;;
esac
