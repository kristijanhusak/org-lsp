"""Clocking into an entry while another file's... (the same file's) clock
is left running: the server asks about the dangling clock, then edits.
Prints every workspace/applyEdit the server sends, with the versions it
names, to see why VS Code refuses the last one."""
import json, os, sys
sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/lsp')
from client import *
ws = os.path.join(SCRATCH, '..', 'vscode-zed-sublime', 'lsp-dangling')
os.makedirs(ws, exist_ok=True)
path = os.path.join(ws, 'dang.org')
open(path, 'w').write('* TODO Left running\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]\n:END:\n* TODO Other\n')
for profile in (FULL, PLAIN):
    c = Client(profile=profile, root=ws, options={'clock': {'today': '2026-05-18', 'now': '10:30'}}, name='dangling')
    g = c.open(path); c.settle(0.5, 5)
    arg = c.at(g, 4, 3)
    answers = ['K', '']
    r = c.command('org.clock.in', arg)
    while isinstance(r, dict) and r.get('input'):
        print('  asked:', r['input'].get('prompt'))
        arg = dict(arg, **(r['input'].get('context') or {}), input=answers.pop(0))
        r = c.command('org.clock.in', arg)
    c.apply_result(arg, r)
    c.settle(0.8, 5)
    print('=====', profile, 'result', json.dumps(r)[:300])
    for method, params in c.server_requests:
        if method == 'workspace/applyEdit':
            print('  applyEdit', json.dumps(params)[:700])
    print('  failures', c.edit_failures, [m['message'] for m in c.messages])
    print('  text', json.dumps(c.text(g)))
    c.shutdown()
