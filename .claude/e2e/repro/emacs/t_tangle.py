"""Tangling with Lisp header arguments / noweb calls (Emacs fallback) vs Emacs batch."""
import sys, json, os, shutil, subprocess; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = """#+property: header-args :mkdirp yes
* Code
#+name: val
#+begin_src emacs-lisp
(* 6 7)
#+end_src

#+begin_src python :tangle (concat "out/" "a.py") :noweb yes
x = <<val()>>
#+end_src

#+begin_src shell :tangle out/b.sh :shebang "#!/bin/sh"
echo hi
#+end_src
"""
for who in ['emacs', 'server']:
    D = SCRATCH + f'/tangle-{who}'; shutil.rmtree(D, ignore_errors=True); os.makedirs(D)
    open(D + '/t.org', 'w').write(DOC)
    if who == 'emacs':
        r = subprocess.run(['timeout', '60', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval',
                            "(progn (require 'ob-tangle) (setq org-confirm-babel-evaluate nil) (find-file \"%s/t.org\") (org-babel-tangle))" % D],
                           capture_output=True, text=True)
        print('emacs:', r.stderr.strip().split('\n')[-1])
    else:
        c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': {'org-confirm-babel-evaluate': False}})
        u = c.open(D + '/t.org', DOC)
        r = c.cmd('org.babel.tangle', u, (0, 0)); c.settle(3)
        print('server:', r, c.messages)
        c.close()
    for f in sorted(os.listdir(D + '/out')) if os.path.isdir(D + '/out') else []:
        p = f'{D}/out/{f}'
        print('  ', f, oct(os.stat(p).st_mode & 0o777), repr(open(p).read()))
