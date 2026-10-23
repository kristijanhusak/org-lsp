import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
exprs = ['org-attach-method', '(progn (require (quote org-attach)) org-attach-method)', 'org-export-with-toc',
         '(progn (require (quote ox)) org-export-with-toc)', 'emacs-version', '(org-version)']
c = Client(settings={'emacs_options': {'org-attach-method': 'mv', 'org-export-with-toc': False},
                     'org_capture_templates': [probe_template(exprs)]}, verbose='-v' in sys.argv)
u = c.open(SCRATCH + '/basic.org', '* H\n')
print(json.dumps(probe(c, u, exprs), indent=1))
print(c.messages)
c.close()
