"""Column view through Emacs, with emacs_options org-columns-default-format."""
import sys, json, os, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = """* Project
** TODO One
:PROPERTIES:
:Effort:   1:00
:END:
** DONE Two
:PROPERTIES:
:Effort:   0:30
:END:
"""
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-columns-default-format': '%30ITEM %TODO %Effort{:}'},
                     'org_agenda_files': [SCRATCH + '/cols/c.org']})
u = c.open(SCRATCH + '/cols/c.org', DOC)
def cols():
    n = [x for x in c.notifications if x['method'] == 'org-lsp/columns']
    return n[-1]['params'] if n else None
r = c.cmd('org.columns', u, (1, 3)); c.settle(3)
p = cols(); print('columns ->', r, json.dumps(p)[:700])
r = c.cmd('org.columns.next', u, (1, 3), extra={'column': 1}); c.settle(2)
print('next on TODO ->', r, c.text(u).split('\n')[1], json.dumps(cols()['lines'])[:300])
r = c.cmd('org.columns.edit', u, (1, 3), extra={'column': 2}, answers=['2:00']); c.settle(2)
print('edit Effort ->', json.dumps(c.trail)[:300], '\n', c.text(u).replace('\n', '|'), json.dumps(cols()['lines'])[:300])
r = c.cmd('org.columns.quit', u, (1, 3)); c.settle(2)
print('quit ->', r, 'lines', cols()['lines'] if cols() else None)
# dynamic block
c.change(u, DOC + '\n#+BEGIN: columnview :hlines 1 :id global\n#+END:\n')
r = c.cmd('org.dblock.update', u, (10, 3)); c.settle(3)
print('dblock update ->', r, '\n' + c.text(u)[len(DOC):])
print('msgs', c.messages)
# agenda column view
r = c.cmd('org.agenda.dispatch', u, (0, 0), answers=['t']); c.settle(2)
print('agenda ->', json.dumps(r)[:200])
docs = [p for m, p in c.requests if m == 'window/showDocument']
print('agenda shown', docs[-1:] if docs else None)
c.close()

# agenda column view and appointments (second server, file on disk)
open(SCRATCH + '/cols/c.org', 'w').write(DOC.replace('** TODO One', '** TODO One <2026-10-21 Wed 23:50>'))
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-columns-default-format-for-agenda': '%25ITEM %TODO %Effort{:}'},
                     'org_agenda_files': [SCRATCH + '/cols/c.org']})
u = c.open(SCRATCH + '/cols/c.org')
c.cmd('org.agenda.dispatch', u, (0, 0), answers=['t']); c.settle(2)
au = [p for m, p in c.requests if m == 'window/showDocument'][-1]['uri']
txt = c.request('workspace/textDocumentContent', {'uri': au}) if au.startswith('org') else None
print('agenda uri', au)
r = c.cmd('org.agenda.columns', au, (1, 2)); c.settle(3)
n = [x for x in c.notifications if x['method'] == 'org-lsp/columns']
print('agenda columns ->', json.dumps(r)[:200], json.dumps(n[-1]['params'])[:600] if n else None, c.messages)
r = c.cmd('org.agenda.appt', u, (0, 0)); c.settle(2)
print('appt ->', json.dumps(r)[:300], c.messages[-2:])
c.close()
