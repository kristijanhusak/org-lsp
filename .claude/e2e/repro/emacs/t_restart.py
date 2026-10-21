"""Emacs exiting during a command (kill-emacs from a block), what it said on stderr, and the restart (notices again?)."""
import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-confirm-babel-evaluate': False, 'org-export-with-toc': 'bad'}})
u = c.open(SCRATCH + '/misc/rs.org', '* H\n#+begin_src emacs-lisp\n(progn (message "about to die") (kill-emacs 3))\n#+end_src\n#+begin_src emacs-lisp\n(+ 1 1)\n#+end_src\n')
for i in range(2):
    m = len(c.messages)
    c.cmd('org.babel.execute', u, (2, 0)); c.settle(4)
    print('kill-emacs run', i, c.messages[m:])
m = len(c.messages)
c.cmd('org.babel.execute', u, (5, 0)); c.settle(4)
print('next block', c.messages[m:], repr(c.text(u)[-30:]))
c.close()
