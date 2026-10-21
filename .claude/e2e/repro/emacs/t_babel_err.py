"""Babel errors through the server, against what Emacs says."""
import sys, json, os, time, subprocess; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
BLOCKS = {'read-string': '#+begin_src emacs-lisp\n(read-string "Your name: ")\n#+end_src\n',
          'python raise': '#+begin_src python\nraise ValueError("bad")\n#+end_src\n',
          'shell exit 3': '#+begin_src shell\necho out; echo err >&2; exit 3\n#+end_src\n',
          'elisp error': '#+begin_src emacs-lisp\n(car 1)\n#+end_src\n',
          'unknown lang': '#+begin_src nosuchlang\nx\n#+end_src\n'}
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD,
                     'emacs_options': {'org-confirm-babel-evaluate': False,
                                       'org-babel-load-languages': {'emacs-lisp': True, 'python': True, 'shell': True}}})
for i, (label, block) in enumerate(BLOCKS.items()):
    doc = '* B\n' + block
    u = c.open(SCRATCH + f'/misc/be{i}.org', doc)
    m = len(c.messages); n = len(c.requests)
    r = c.cmd('org.babel.execute', u, (2, 0)); c.settle(6)
    shown = [p['uri'][-50:] for k, p in c.requests[n:] if k == 'window/showDocument' and 'be%d' % i not in p['uri']]
    print(f'{label}: ->', r, c.messages[m:], 'shown', shown, '| results', repr(c.text(u)[len(doc):]))
    el = f"""(progn (org-babel-do-load-languages 'org-babel-load-languages '((emacs-lisp . t) (python . t) (shell . t)))
      (setq org-confirm-babel-evaluate nil) (with-temp-buffer (insert {json.dumps(doc)}) (org-mode) (goto-char (point-min)) (forward-line 2)
      (condition-case e (org-ctrl-c-ctrl-c) (error (message "ERROR: %s" (error-message-string e))))
      (message "BUFFER: %S" (buffer-substring (+ {len(doc)} 1) (point-max)))
      (when (get-buffer "*Org-Babel Error Output*") (message "ERRBUF: %S" (with-current-buffer "*Org-Babel Error Output*" (buffer-string))))))"""
    out = subprocess.run(['timeout', '60', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval', el], capture_output=True, text=True, input='')
    print('   emacs:', [l for l in out.stderr.split('\n') if l and not l.startswith('Loading')][-4:])
c.close()
