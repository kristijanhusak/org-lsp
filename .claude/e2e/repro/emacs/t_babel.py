"""Babel through the server (emacs_options for the languages and the
confirmation) against Emacs batch on the same file."""
import sys, json, os, subprocess, time, difflib; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = """#+title: Babel
#+property: header-args:python :results output

* Blocks
#+name: one
#+begin_src emacs-lisp
(+ 1 2)
#+end_src

#+begin_src python
print("hi")
#+end_src

#+begin_src shell :results value
echo a; echo b
#+end_src

#+begin_src python :session s :results value
x = 41
#+end_src

#+begin_src python :session s :results value
x + 1
#+end_src

#+name: greet
#+begin_src emacs-lisp :var who="you"
(format "hello %s" who)
#+end_src

#+begin_src emacs-lisp :noweb yes
(list <<one>>)
#+end_src

#+call: greet(who="call")

Inline: src_emacs-lisp{(* 6 7)} and call_greet(who="inline").

#+begin_src emacs-lisp :results table
'((1 2) (3 4))
#+end_src
"""
OPTS = {'org-babel-load-languages': {'emacs-lisp': True, 'python': True, 'shell': True},
        'org-confirm-babel-evaluate': False, 'org-babel-python-command': 'python3'}
D = SCRATCH + '/babel'
os.makedirs(D, exist_ok=True)
src = D + '/b.org'
open(src, 'w').write(DOC)
el = """(progn (org-babel-do-load-languages 'org-babel-load-languages '((emacs-lisp . t) (python . t) (shell . t)))
 (setq org-confirm-babel-evaluate nil org-babel-python-command "python3")
 (find-file "%s") (org-babel-execute-buffer) (princ (buffer-string)))""" % src
want = subprocess.run(['timeout', '120', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval', el],
                      capture_output=True, text=True).stdout
open(D + '/emacs.org', 'w').write(want)
open(src, 'w').write(DOC)
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': OPTS})
u = c.open(src, DOC)
t0 = time.time()
r = c.cmd('org.babel.execute.buffer', u, (3, 0), timeout=200)
print('execute.buffer ->', json.dumps(r)[:200], 'in', round(time.time() - t0, 1), 's')
c.settle(2)
got = c.text(u)
print('SAME as Emacs' if got == want else 'DIFF:\n' + ''.join(difflib.unified_diff(want.splitlines(1), got.splitlines(1))))
print('messages', c.messages)
# C-c C-c on one block, with confirmation now asked (change the option via a new server)
c.close()
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': dict(OPTS, **{'org-confirm-babel-evaluate': True})}, request_answers=['No', 'Yes'])
u = c.open(src, DOC)
for ans in ['No', 'Yes']:
    r = c.cmd('org.babel.execute', u, (5, 0))
    c.settle(2)
    print('C-c C-c answered', ans, '->', json.dumps(r)[:200])
print('messages', c.messages)
print(c.text(u)[:260])
r = c.cmd('org.babel.remove_result', u, (5, 0)); c.settle(1)
print('remove_result ->', json.dumps(r)[:200], '| result gone:', '#+RESULTS: one' not in c.text(u))
c.close()
