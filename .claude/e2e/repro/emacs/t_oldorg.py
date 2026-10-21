"""emacs_options with the Org that Emacs ships (9.6.15): an option newer than it."""
import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
c = Client(settings={'message_prompts': False, 'emacs_options': {
    'org-babel-exp-code-template': '#+begin_src %lang\n%body\n#+end_src',   # 9.7, ob-exp
    'org-cite-bibtex-bibliography-style': 'plain',                        # 9.8, oc-bibtex
    'org-attach-store-link-p': 'file',                                    # 9.7 type change
    'org-export-with-toc': False, 'org-export-body-only': True, 'org-html-datetime-formats': ['a','b']}})
u = c.open(SCRATCH + '/old.org', '* H\ntext\n')
r = c.cmd('org.export.dispatch', u, (0, 0), answers=['t', 'A'])
c.settle(4)
for m in c.messages: print(m)
c.close()
