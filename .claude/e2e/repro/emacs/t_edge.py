"""Edge values of emacs_options: JSON null, non-ASCII, a TOML date, nested empty table, an option given twice by both spellings."""
import sys, json, os; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
D = SCRATCH + '/edge'; os.makedirs(D + '/home/.config/org-lsp', exist_ok=True)
open(D + '/home/.config/org-lsp/config.toml', 'w').write(f"emacs_config = '{LOAD}'\n[emacs_options]\norg-export-default-language = 2026-10-21\norg-html-text-markup-alist = {{}}\n")
exprs = ['(progn (require (quote ox)) org-export-with-toc)', '(progn (require (quote ox)) org-export-with-author)',
         '(progn (require (quote ox)) org-export-default-language)', '(progn (require (quote ox-html)) org-html-text-markup-alist)',
         '(progn (require (quote ox-html)) org-html-footnotes-section)', '(progn (require (quote ox)) org-export-headline-levels)']
c = Client(settings={'message_prompts': False, 'org_capture_templates': [probe_template(exprs)],
                     'emacs_options': {'org-export-with-toc': None, 'org-html-footnotes-section': 'Notes — ✓ 日本 😀 %s %s',
                                       'org-export-headline-levels': 1e2}}, home=D + '/home')
u = c.open(D + '/e.org', '* H\n')
for k, v in probe(c, u, exprs).items(): print(k[:60], '=>', v)
print(c.messages)
c.close()
