import json, os, sys
sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/lsp')
from client import *
ws = os.path.join(SCRATCH, '..', 'vscode-zed-sublime', 'lsp-zed-actions')
open(os.path.join(ws, 'b.org'), 'w').write('* A\n** B\nSCHEDULED: <2026-05-18 Mon>\n| 1 | 2 |   |\n#+TBLFM: $3=$1*$2\n')
opts = {'virtual_documents': False, 'org_client': True, 'command_actions': True, 'message_prompts': True}
c = Client(profile=PLAIN, root=ws, options=opts, name='zedactions2')
g = c.open(os.path.join(ws, 'b.org')); c.settle(0.5, 5)
for (l, ch) in [(1, 1), (2, 15), (3, 2), (4, 3)]:
    r = c.request('textDocument/codeAction', {'textDocument': {'uri': g}, 'range': {'start': {'line': l, 'character': ch}, 'end': {'line': l, 'character': ch}}, 'context': {'diagnostics': []}})
    print(l, ch, sorted({(a.get('command') or {}).get('command') or a.get('kind') for a in r or []} & {'org.meta.left', 'org.timestamp.up', 'org.timestamp.down', 'org.date.later', 'org.date.earlier', 'org.table.recalculate'}))
c.shutdown()
