"""C-c ' (src block, table formulas), C-c ` (field), formula debugger, table import/export, plot, ascii plot."""
import sys, json, os, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = """* T
#+begin_src python
x = 1
#+end_src

| a | b | c |
|---+---+---|
| 1 | 2 |   |
| 3 | 4 |   |
#+TBLFM: $3=$1+$2

text
"""
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-src-lang-modes': {'python': 'python'}}}, request_answers=['Yes', 'No', 'Yes', 'Yes'])
u = c.open(SCRATCH + '/edit/e.org', DOC)
def shown(n):
    return [p for m, p in c.requests[n:] if m == 'window/showDocument']
def edit_and_save(cmd, pos, change, label):
    n = len(c.requests); m = len(c.messages)
    r = c.cmd(cmd, u, pos); c.settle(1)
    d = shown(n)
    print(f'== {label}: ->', json.dumps(r)[:200], d[-1]['uri'][-40:] if d else None, c.messages[m:])
    if not d: return
    eu = d[-1]['uri']; path = eu[7:]
    text = open(path).read(); print('   edit buffer:', repr(text[:200]))
    new = change(text)
    open(path, 'w').write(new)
    c.open(path, new)
    c.notify('textDocument/didSave', {'textDocument': {'uri': eu}, 'text': new}); c.settle(2)
    print('   after save:', repr(c.text(u)), c.messages[m:])
edit_and_save('org.edit.special', (2, 0), lambda t: t.replace('x = 1', 'x = 2\ny = 3'), "C-c ' src")
edit_and_save('org.edit.special', (6, 2), lambda t: t.replace('$3=$1+$2', '$3=$1*$2'), "C-c ' table formulas")
edit_and_save('org.table.edit.field', (7, 2), lambda t: t, 'C-c ` field on hline')
edit_and_save('org.table.edit.field', (8, 2), lambda t: t.replace('1', '10'), 'C-c ` field')
edit_and_save('org.edit.special', (5, 0), lambda t: t, "C-c ' on blank line")
m = len(c.messages)
r = c.cmd('org.table.recalculate', u, (8, 2)); c.settle(2)
print('== recalc ->', r, repr(c.text(u)[40:140]), c.messages[m:])
m = len(c.messages)
r = c.cmd('org.table.formula.debugger', u, (7, 2)); c.settle(1); print('== debugger on ->', r, c.messages[m:])
c.request_answers[:] = ['Yes', 'No']
r = c.cmd('org.table.recalculate', u, (7, 2)); c.settle(3)
print('== recalc with debugger ->', r, c.messages[m:], repr(c.text(u)[40:140]))
r = c.cmd('org.table.formula.debugger', u, (7, 2)); c.settle(1)
m = len(c.messages)
r = c.cmd('org.table.export', u, (7, 2), answers=[SCRATCH + '/edit/out.csv']); c.settle(2)
print('== export ->', json.dumps(c.trail)[:300], c.messages[m:], open(SCRATCH + '/edit/out.csv').read() if os.path.exists(SCRATCH + '/edit/out.csv') else 'NO FILE')
open(SCRATCH + '/edit/in.csv', 'w').write('x,y\n5,6\n')
m = len(c.messages)
r = c.cmd('org.table.import', u, (11, 4), answers=[SCRATCH + '/edit/in.csv']); c.settle(2)
print('== import ->', json.dumps(c.trail)[:300], c.messages[m:], repr(c.text(u)[-80:]))
m = len(c.messages)
r = c.cmd('org.plot.gnuplot', u, (7, 2)); c.settle(3); print('== plot ->', r, c.messages[m:])
m = len(c.messages)
r = c.cmd('org.table.ascii.plot', u, (7, 2)); c.settle(3); print('== ascii plot ->', r, c.messages[m:], repr(c.text(u)[40:400]))
c.close()
