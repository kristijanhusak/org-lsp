"""LaTeX preview without latex installed, with emacs_options org-format-latex-options / org-preview-latex-default-process."""
import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-preview-latex-default-process': 'dvisvgm'}},
           capabilities={'workspace': {'applyEdit': True}, 'window': {'showDocument': {'support': True}},
                         'experimental': {'orgImages': True}})
u = c.open(SCRATCH + '/misc/lx.org', '* Math\nSee $x^2$ here.\n')
r = c.cmd('org.latex.preview', u, (1, 6)); c.settle(6)
print(json.dumps(r)[:300], c.messages, [n['method'] for n in c.notifications if 'org-lsp' in n['method']])
c.close()
