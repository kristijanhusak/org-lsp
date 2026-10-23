#!/usr/bin/env bash
# clients.sh CLIENT: the same flows in one Vim per client; prints what each gave.
client=$1
V=/home/user/org-lsp/.claude/e2e/repro/vim-helix/vh.sh
W=/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/c-$client
n=c$client
$V stop $n 2>/dev/null; rm -rf $W; mkdir -p $W/notes
cat > $W/settings.json <<J
{"org_agenda_files": ["$W/notes"], "org_log_done": "time", "clock": {"today": "2026-10-21", "now": "10:30"},
 "org_capture_templates": [{"key":"t","description":"Task","target":["file","$W/notes/inbox.org"],"template":"* TODO %?"},
   {"key":"m","description":"Missing","target":["file","$W/notes/new.org"],"template":"* TODO %?"}]}
J
printf '* TODO Write report\nSCHEDULED: <2026-10-21 Wed 09:00>\n* TODO Call\n' > $W/notes/work.org
printf '* TODO Other thing\nSCHEDULED: <2026-10-21 Wed>\n' > $W/notes/other.org
: > $W/notes/inbox.org
$V start $n $client $W $W/settings.json "" notes/work.org; sleep ${BOOT:-5}
k() { $V keys $n "$@"; }
l() { $V lit $n "$@"; }
ex() { l ":$1"; k Enter; sleep ${2:-0.5}; }
k 3G c i t; sleep 1.5; k d; sleep 2
echo "1 cit: $(sed -n 3p <($V dump $n | grep -A3 'work.org changed' | tail -3))"; 
k Space o c; sleep 1.5; k t; sleep 2.5; l "Cap empty"; k Escape; sleep 1; k C-c; sleep 3
echo "2 capture empty target, disk: $(cat $W/notes/inbox.org | tr '\n' '|')"
k Space o c; sleep 1.5; k m; sleep 2.5; l "Cap missing"; k Escape; sleep 1; k C-c; sleep 3
echo "3 capture missing target, disk: $(cat $W/notes/new.org 2>&1| tr '\n' '|')"
ex 'only | b work.org'
k Space o a; sleep 1.5; k a; sleep 3
ex "call search('Other thing')"; k t; sleep 1.5; k d; sleep 2.5
echo "4 agenda t on unopened file, buffer line: $($V screen $n | grep 'Other thing' | head -1)"
k s; sleep 2
echo "5 after s, disk: $(head -1 $W/notes/other.org)"
printf '* TODO Ext added\nSCHEDULED: <2026-10-21 Wed>\n' >> $W/notes/other.org
ex 'sb other.org | e! | wincmd p' 1; k r; sleep 3
k Enter Escape; sleep 0.3; l ":call writefile(getbufline(bufnr('Agenda.org-agenda'),1,'\$'), '$W/agenda.txt')"; k Enter; sleep 0.5
echo "6 after :e! of other.org + r, agenda has Ext: $(grep -c 'Ext added' $W/agenda.txt)"
ex 'only | b work.org'
l ":redir! > $W/msgs.txt | silent messages | redir END"; k Enter; sleep 0.5
echo "7 messages:"; sed 's/^/   /' $W/msgs.txt
