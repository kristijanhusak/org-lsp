"""Precedence: shared file vs editor settings for emacs_options, Neovim's [],
and emacs_config_file < emacs_config < emacs_options."""
import sys, json, os, shutil; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
exprs = ['(progn (require (quote org-attach)) org-attach-method)', 'org-babel-load-languages',
         '(progn (require (quote ox)) org-export-with-toc)', '(progn (require (quote ox)) org-export-with-author)',
         '(progn (require (quote ox-html)) org-html-validation-link)', 'my-from-file', 'my-from-config']

def run(label, file_text, editor):
    home = os.path.join(SCRATCH, 'home-' + label)
    shutil.rmtree(home, ignore_errors=True)
    os.makedirs(home + '/.config/org-lsp')
    if file_text is not None:
        open(home + '/.config/org-lsp/config.toml', 'w').write(file_text)
    open(home + '/my.el', 'w').write("(setq my-from-file 'yes)\n(setq org-attach-method 'cp)\n(setq org-html-validation-link \"FILE\")\n")
    s = dict(editor)
    s.setdefault('message_prompts', False)
    s['org_capture_templates'] = [probe_template(exprs)]
    c = Client(settings=s, home=home)
    u = c.open(SCRATCH + '/prec.org', '* H\n')
    r = probe(c, u, exprs)
    c.settle(1)
    print('==', label)
    for e in exprs:
        print('  ', e[:60], '=>', r.get(e))
    for m in c.messages:
        print('   msg', m)
    c.close()

FILE = f'''emacs_config_file = "~/my.el"
emacs_config = \'\'\'
{LOAD}
(setq my-from-config 'yes)
(setq org-attach-method 'ln)
(with-eval-after-load 'ox (setq org-export-with-author 'from-config))
\'\'\'
[emacs_options]
org-attach-method = "mv"
org-babel-load-languages = {{ python = true, emacs-lisp = true }}
org-export-with-toc = false
'''
run('file-only', FILE, {})
run('editor-over', FILE, {'emacs_options': {'org-babel-load-languages': {'shell': True}, 'org-export-with-toc': 3}})
run('neovim-empty', FILE, {'emacs_options': []})
run('editor-empty-object', FILE, {'emacs_options': {}})
run('editor-config-over', FILE, {'emacs_config': LOAD + "\n(setq my-from-config 'editor)"})
