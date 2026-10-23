import sys, json, os; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n#+TBLFM: $3=$1+$2\n"
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD})
u = c.open(SCRATCH + '/edit/f.org', DOC)
n = len(c.requests)
c.cmd('org.edit.special', u, (2, 2)); c.settle(1)
eu = [p for m, p in c.requests[n:] if m == 'window/showDocument'][-1]['uri']
t = open(eu[7:]).read(); new = t.replace('$1+$2', '$1*$2') + '@3$1 = 7\n'
open(eu[7:], 'w').write(new); c.open(eu[7:], new)
c.notify('textDocument/didSave', {'textDocument': {'uri': eu}}); c.settle(2)
print('formulas saved ->', repr(c.text(u)), c.messages)
r = c.cmd('org.table.recalculate', u, (2, 2)); c.settle(2); print(repr(c.text(u)))
c.trail = []
r = c.cmd('org.table.export', u, (2, 2), answers=[SCRATCH + '/edit/out.csv']); print(json.dumps(r)[:900])
r = c.cmd('org.table.export', u, (2, 2), answers=[SCRATCH + '/edit/out.csv', 'orgtbl-to-csv']); c.settle(2)
print(json.dumps(c.trail)[-300:], c.messages[-2:], open(SCRATCH + '/edit/out.csv').read() if os.path.exists(SCRATCH + '/edit/out.csv') else 'NO FILE')
c.close()
