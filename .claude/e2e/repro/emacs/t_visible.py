"""Visible-only export of what the editor folded (C-v), vs Emacs with the same folds."""
import sys, json, os, urllib.parse, subprocess; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = "#+title: V\n#+options: toc:nil num:nil author:nil\n* Shown\nvisible\n* Folded\nhidden text\n** Sub\nsub text\n* Last\nend\n"
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD})
u = c.open(SCRATCH + '/misc/v.org', DOC)
for folded in [[[4, 7]], [[4, 5]], [[6, 7]]]:
    n = len(c.requests)
    r = c.cmd('org.export.dispatch', u, (0, 0), answers=['\u0016', 't', 'A'], extra={'folded': folded}); c.settle(3)
    print('trail', [json.dumps(t)[:120] for t in c.trail][-2:])
    d = [p for k, p in c.requests[n:] if k == 'window/showDocument']
    out = open(urllib.parse.unquote(d[-1]['uri'][7:])).read() if d else c.messages[-2:]
    print('folded', folded, '->', repr(out))
c.close()
el = """(progn (require 'org) (find-file "%s/misc/v.org") (goto-char (point-min)) (re-search-forward "^\\\\* Folded") (org-fold-subtree t)
 (princ (with-current-buffer (org-ascii-export-as-ascii nil nil t) (buffer-string))))""" % SCRATCH
print('emacs, Folded subtree folded:', repr(subprocess.run(['timeout', '60', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval', el], capture_output=True, text=True).stdout))
