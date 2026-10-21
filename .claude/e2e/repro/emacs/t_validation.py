import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
exprs = ['(progn (require (quote ox-html)) org-html-validation-link)', "(get 'org-html-validation-link 'custom-type)",
         '(progn (require (quote ox)) org-export-time-stamp-file)']
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'org_capture_templates': [probe_template(exprs)],
                     'emacs_options': {'org-html-validation-link': False, 'org-export-time-stamp-file': False}})
u = c.open(SCRATCH + '/v.org', '* H\n')
print(probe(c, u, exprs)); c.settle(1); print(c.messages); c.close()
