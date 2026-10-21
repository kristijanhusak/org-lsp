#!/usr/bin/env bash
# msg_clients.sh CLIENT: how a window/showMessage (over-effort clock-in) and a result message show.
c=$1; V=/home/user/org-lsp/.claude/e2e/repro/vim-helix/vh.sh; n=msg$c
W=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/msg-$c; rm -rf $W; mkdir -p $W
echo '{}' > $W/settings.json
printf '* TODO Long task\n:PROPERTIES:\n:Effort:   0:30\n:END:\n:LOGBOOK:\nCLOCK: [2026-10-20 Tue 09:00]--[2026-10-20 Tue 09:40] =>  0:40\n:END:\n' > $W/c.org
$V stop $n 2>/dev/null; $V start $n $c $W $W/settings.json "" c.org; sleep ${BOOT:-8}
$V keys $n 1G Space o x i; sleep 3
echo "--- screen:"; $V screen $n | tail -3
$V lit $n ":redir! > $W/m.txt | silent messages | redir END"; $V keys $n Enter; sleep 0.5
echo "--- :messages:"; cat $W/m.txt
$V lit $n ":redrawstatus!"; $V keys $n Enter; sleep 0.5; echo "--- after redrawstatus:"; $V screen $n | tail -2; $V stop $n
