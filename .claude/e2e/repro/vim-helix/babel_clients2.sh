#!/usr/bin/env bash
# babel_clients.sh CLIENT: <Leader>obe on an emacs-lisp block (Emacs's yes/no is a window/showMessageRequest).
c=$1; V=/home/user/org-lsp/.claude/e2e/repro/vim-helix/vh.sh; n=bab$c
W=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/bab-$c; rm -rf $W; mkdir -p $W
echo "{\"emacs_config\": \"(add-to-list 'load-path \\\"/root/org-mode/lisp\\\")\"}" > $W/settings.json
printf '* Code\n#+begin_src emacs-lisp\n(* 6 7)\n#+end_src\n' > $W/e.org
$V stop $n 2>/dev/null; $V start $n $c $W $W/settings.json "" e.org; sleep ${BOOT:-6}
$V keys $n 3G Space o b e; sleep 6
echo "--- screen after <Leader>obe:"; $V screen $n | tail -5
$V keys $n 1 Enter; sleep 12
$V keys $n Escape; $V lit $n ':redraw!'; $V keys $n Enter; sleep 0.5
echo "--- buffer:"; VHCMD=VHCur $V dump $n | sed -n '2,12p'
$V stop $n
