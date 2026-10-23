#!/usr/bin/env bash
# Drive Vim in tmux (socket vh). Usage:
#   vh.sh start NAME CLIENT WORK [settings.json] [extra.vim] [file]
#   vh.sh keys NAME keys...   (tmux send-keys args)
#   vh.sh lit NAME text       (literal)
#   vh.sh screen NAME
#   vh.sh dump NAME           (writes WORK/dump.txt via :VHDump, prints it)
#   vh.sh stop NAME
set -u
cmd=$1; name=$2; shift 2
T="tmux -L vh"
case $cmd in
  start)
    client=$1 work=$2 settings=${3:-} extra=${4:-} file=${5:-}
    mkdir -p "$work"; echo "$work" > /tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/.work-$name
    $T new-session -d -s "$name" -x 120 -y 40 "cd $work && env HOME=$work ORG_LSP_CONFIG=/nonexistent ORG_LISP_DIR=/root/org-mode/lisp VH_CLIENT=$client VH_WORK=$work VH_SETTINGS=$settings VH_EXTRA=$extra vim -N -u /home/user/org-lsp/.claude/e2e/repro/vim-helix/vimrc -i NONE $file; sleep 600"
    ;;
  keys) $T send-keys -t "$name" "$@" ;;
  lit) $T send-keys -t "$name" -l "$*" ;;
  screen) $T capture-pane -p -t "$name" ;;
  dump)
    work=$(cat /tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/.work-$name)
    rm -f "$work/dump.txt"; $T send-keys -t "$name" Escape; $T send-keys -t "$name" -l ":${VHCMD:-VHDump} ${1:-}"; $T send-keys -t "$name" Enter
    for i in $(seq 50); do [ -f "$work/dump.txt" ] && break; sleep 0.1; done; cat "$work/dump.txt" ;;
  stop) $T kill-session -t "$name" ;;
esac
