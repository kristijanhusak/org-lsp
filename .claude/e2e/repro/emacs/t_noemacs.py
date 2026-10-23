"""Without Emacs: every Emacs-backed command says so; the rest works;
emacs_options name checks still reported."""
import sys, json, os; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
mode = sys.argv[1] if len(sys.argv) > 1 else 'path'
env = {'PATH': '/usr/local/sbin:/nonexistent'} if mode == 'path' else {}
settings = {'message_prompts': False, 'emacs_options': {'org-attach-method': 'mv', 'org-log-done': 'time'},
            'org_capture_templates': [{'key': 's', 'description': 'sexp', 'type': 'plain',
                                       'target': ['file', SCRATCH + '/noemacs-target.org'], 'template': 'x %(+ 1 2)'}]}
if mode == 'setting':
    settings['emacs'] = '/nonexistent/emacs'
c = Client(settings=settings, env=env)
DOC = """* TODO Task :crypt:
:PROPERTIES:
:COLUMNS: %25ITEM %TODO
:END:
body[fn:1]
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
| 1 | 2 |
|   |   |
#+TBLFM: @2$1='(+ 1 1)

[fn:1] note
"""
u = c.open(SCRATCH + '/noemacs/n.org', DOC)
c.settle(1)
print('start messages:', c.messages)
cases = [('org.export.dispatch', (0, 0), []), ('org.export.dispatch', (0, 0), ['h', 'h']),
         ('org.babel.execute', (5, 0), []), ('org.babel.tangle', (5, 0), []),
         ('org.attach', (0, 0), ['a', 'x.txt']), ('org.crypt.encrypt', (0, 0), []),
         ('org.columns', (0, 0), []), ('org.timer.start', (4, 0), []), ('org.table.recalculate', (8, 1), []),
         ('org.footnote.action', (4, 6), []), ('org.capture', (0, 0), ['s']), ('org.edit.special', (5, 0), []),
         ('org.table.edit.field', (8, 2), []), ('org.plot.gnuplot', (8, 1), []), ('org.latex.preview', (0, 0), []),
         ('org.agenda.appt', (0, 0), []), ('org.toggle.item', (4, 0), []), ('org.emphasize', (4, 0), ['*']),
         ('org.table.sum', (8, 2), []), ('org.timer.set', (4, 0), ['5']), ('org.dblock.insert', (0, 0), ['columnview']),
         ('org.todo', (0, 2), ['d']), ('org.table.import', (4, 0), ['/etc/hostname'])]
for cmd, pos, ans in cases:
    m = len(c.messages)
    try:
        r = c.cmd(cmd, u, pos, answers=ans, timeout=30)
    except TimeoutError as e:
        r = 'TIMEOUT'
    c.settle(1)
    print(f'{cmd} {ans}: ->', json.dumps(r, ensure_ascii=False)[:160], '| msgs:', c.messages[m:])
print(c.text(u)[:60])
c.close()
