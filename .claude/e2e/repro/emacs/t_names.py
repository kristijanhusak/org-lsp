"""The messages of emacs_options: names the server leaves out, values Emacs
leaves out, notices that come once a library loads, errors in :set."""
import sys, json, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
opts = {
    'org-log-done': 'time',                      # setting
    'org-babel-post-tangle-hook': [],            # lisp
    'org-cite-basic-max-key-distance': 2,        # emacs-only
    'org-tab-follows-link': True,                # other
    'org-bbdb-default-anniversary-format': 'x',  # absent
    'org-fold-show-context-detail': [],          # missing
    'org-atach-method': 'mv',                    # typo
    'org-zzzzzzzzzz': 1,                         # nothing close
    'org_attach_method': 'mv',                   # underscores
    'org-attach-method': 'move',                 # wrong value
    'org-export-with-toc': 'yes',                # wrong value, choice
    'org-export-headline-levels': 2.5,           # float for integer
    'org-babel-python-command-nonsession': 7,    # ob-python, loads later
    'org-babel-load-languages': {'emacs-lisp': True, 'nosuchlang': True},  # :set error?
    'org-export-backends': ['ascii', 'html', 'nosuch'],
}
argv = sys.argv[1:]
settings = {'message_prompts': False, 'emacs_options': opts}
if 'bundled' not in argv:
    settings['emacs_config'] = LOAD
c = Client(settings=settings)
c.settle(1)
print('-- at start:')
for m in c.messages: print('  ', m)
n = len(c.messages)
u = c.open(SCRATCH + '/names.org', '* H\n#+begin_src python\nprint(1)\n#+end_src\n')
r = c.cmd('org.export.dispatch', u, (0, 0), answers=['t', 'A'])
print('export result', json.dumps(r)[:300])
c.settle(5)
print('-- after export:')
for m in c.messages[n:]: print('  ', m)
n = len(c.messages)
c.request_answers = ['Yes']
r = c.cmd('org.babel.execute', u, (2, 0))
print('babel result', json.dumps(r)[:300])
c.settle(5)
print('-- after babel:')
for m in c.messages[n:]: print('  ', m)
print(c.text(u))
c.close()
