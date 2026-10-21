"""Leftovers: an error in an option's :set, a question Emacs asks that is not yes/no, a Babel error,
LaTeX preview without latex, visible-only export of folded parts, export of unsaved text with #+INCLUDE."""
import sys, json, os, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/misc'; os.makedirs(D, exist_ok=True)
open(D + '/inc.org', 'w').write('Included text.\n')
CONFIG = LOAD + "\n(put 'org-attach-method 'custom-set (lambda (s v) (if (eq v (quote mv)) (error \"set failed for %s\" v) (set-default s v))))"
c = Client(settings={'message_prompts': False, 'emacs_config': CONFIG,
                     'emacs_options': {'org-attach-method': 'mv', 'org-confirm-babel-evaluate': False,
                                       'org-babel-load-languages': {'emacs-lisp': True, 'python': True},
                                       'org-export-with-toc': False, 'org-export-with-section-numbers': False}},
           capabilities={'workspace': {'applyEdit': True, 'workspaceEdit': {'documentChanges': True}},
                         'window': {'showDocument': {'support': True}},
                         'experimental': {'orgImages': True}})
DOC = """#+title: Misc
* Shown
visible
* Folded
hidden text
* Babel
#+begin_src emacs-lisp
(read-string "Your name: ")
#+end_src
#+begin_src python
raise ValueError("bad")
#+end_src
#+begin_src emacs-lisp
(car 1)
#+end_src
* Math
$x^2$
#+INCLUDE: "inc.org"
"""
u = c.open(D + '/m.org', DOC)
m = len(c.messages)
c.cmd('org.attach', u, (1, 0), answers=['n', 'n.txt']); c.settle(2)
print('attach with failing :set ->', c.messages[m:])
for line, label in [(7, 'read-string'), (10, 'python raise'), (13, 'elisp error')]:
    m = len(c.messages); n = len(c.requests)
    r = c.cmd('org.babel.execute', u, (line, 0)); c.settle(4)
    print(label, '->', r, c.messages[m:], [json.dumps(p)[:150] for k, p in c.requests[n:] if k == 'window/showDocument'])
print(repr(c.text(u)[c.text(u).index('* Babel'):][:400]))
m = len(c.messages)
r = c.cmd('org.latex.preview', u, (16, 1)); c.settle(4); print('latex preview ->', json.dumps(r)[:200], c.messages[m:])
# visible only, with "Folded" folded (lines 3-4) and unsaved text
c.change(u, c.text(u).replace('visible', 'visible UNSAVED'))
n = len(c.requests)
r = c.cmd('org.export.dispatch', u, (0, 0), answers=['\u0016', 't', 'A'], extra={'folded': [[3, 4]]}); c.settle(4)
d = [p for k, p in c.requests[n:] if k == 'window/showDocument']
import urllib.parse
print('visible-only ->', r, open(urllib.parse.unquote(d[-1]['uri'][7:])).read() if d else c.messages[-2:])
c.close()
