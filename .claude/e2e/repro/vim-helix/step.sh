#!/usr/bin/env bash
# step.sh NAME FILE LINE COL -- tmux-keys... : open FILE afresh (:e!), put cursor, send keys, wait, dump the file buffer.
# WAIT env (seconds, default 2).
V=/home/user/org-lsp/.claude/e2e/repro/vim-helix/vh.sh
name=$1 file=$2 line=$3 col=$4; shift 5
$V keys $name Escape; sleep 0.2
$V lit $name ":silent! e! $file | call cursor($line, $col)"; $V keys $name Enter; sleep ${OPEN:-1.5}
for k in "$@"; do
  case $k in
    LIT:*) $V lit $name "${k#LIT:}" ;;
    SLEEP:*) sleep ${k#SLEEP:} ;;
    *) $V keys $name "$k" ;;
  esac
  sleep 0.15
done
sleep ${WAIT:-2}
$V screen $name > /tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/last-screen-$name.txt
VHCMD=VHCur $V dump $name
