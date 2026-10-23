"""Editing commands Emacs runs (footnotes, toggles, templates, drawer, clone, copy-down, sum, create), vs Emacs batch at the same point."""
import sys, json, os, subprocess; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = "* Heading\nSome text here[fn:2].\n- item one\nplain line\n| 1 | 2 |\n| 3 |   |\n\n[fn:2] Two.\n"
OPTS = {'org-footnote-auto-label': 'random', 'org-footnote-section': 'Notes', 'org-table-copy-increment': False}
SETQ = "(setq org-footnote-auto-label 'random org-footnote-section \"Notes\" org-table-copy-increment nil)"
cases = [  # command, pos, answers, emacs form
    ('org.footnote.menu', (1, 3), ['r'], '(org-footnote-renumber-fn:N)'),
    ('org.footnote.menu', (1, 3), ['s'], '(org-footnote-sort)'),
    ('org.footnote.menu', (1, 3), ['n'], '(org-footnote-normalize)'),
    ('org.toggle.item', (3, 2), [], '(org-toggle-item nil)'),
    ('org.toggle.heading', (3, 2), [], '(org-toggle-heading nil)'),
    ('org.toggle.comment', (0, 2), [], '(org-toggle-comment)'),
    ('org.drawer.insert', (3, 2), ['LOGBOOK'], '(org-insert-drawer nil "LOGBOOK")'),
    ('org.structure.template', (3, 0), ['q'], '(org-insert-structure-template "quote")'),
    ('org.table.copy.down', (5, 6), [], '(org-table-copy-down 1)'),
    ('org.table.create', (6, 0), ['3x2'], '(org-table-create "3x2")'),
]
def emacs(pos, form):
    el = f"""(progn {SETQ} (require 'org) (require 'org-footnote) (with-temp-buffer (insert {json.dumps(DOC)}) (org-mode)
      (goto-char (point-min)) (forward-line {pos[0]}) (forward-char {pos[1]})
      (condition-case e {form} (error (message "ERR %S" e))) (princ (buffer-string))))"""
    return subprocess.run(['timeout', '60', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval', el], capture_output=True, text=True).stdout
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': OPTS})
for i, (cmd, pos, ans, form) in enumerate(cases):
    u = c.open(SCRATCH + f'/edits/e{i}.org', DOC)
    m = len(c.messages)
    r = c.cmd(cmd, u, pos, answers=ans); c.settle(1.5)
    got, want = c.text(u), emacs(pos, form)
    print(('SAME ' if got == want else 'DIFF ') + ('changed ' if got != DOC else 'UNCHANGED ') + cmd, ans, json.dumps(r)[:100], c.messages[m:])
    if got != want:
        print('   server:', repr(got)); print('   emacs: ', repr(want))
c.close()
