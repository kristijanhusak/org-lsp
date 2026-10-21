"""Exports through the server with emacs_options, against Emacs batch with
the same setq on the same file."""
import sys, json, os, re, subprocess, shutil, time; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
D = SCRATCH + '/exp'
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
DOC = """#+title: Export test
#+author: Someone

* Intro
Some /text/ with a footnote[fn:1] and x^2.
:LOGBOOK:
- Note taken on [2026-10-01 Thu 10:00]
:END:
** TODO A task
SCHEDULED: <2026-10-21 Wed>
#+begin_src python
print(1)
#+end_src
* Hidden :noexport:
secret
* Table
| a | b |
|---+---|
| 1 | 2 |

[fn:1] The note.
"""
OPTS = {'org-export-with-toc': False, 'org-export-with-author': False, 'org-html-validation-link': False,
        'org-export-with-section-numbers': False, 'org-export-backends': ['ascii', 'html', 'latex', 'md', 'odt'],
        'org-latex-compiler': 'xelatex', 'org-export-with-sub-superscripts': '{}', 'org-export-with-tasks': 'todo',
        'org-html-head-include-default-style': False, 'org-export-time-stamp-file': False}
SETQ = """(setq org-export-with-toc nil org-export-with-author nil org-export-with-section-numbers nil
 org-latex-compiler "xelatex" org-export-with-sub-superscripts '{} org-export-with-tasks 'todo
 org-export-time-stamp-file nil)
(with-eval-after-load 'ox-html (setq org-html-validation-link nil org-html-head-include-default-style nil))
(setq org-export-backends '(ascii html latex md odt))"""

def emacs_batch(fn, src):
    out = subprocess.run(['timeout', '120', 'emacs', '--batch', '-Q', '--eval', LOAD, '--eval', f'(progn {SETQ})',
                          '--eval', "(require 'ox)", src, '--eval', f'(princ ({fn}))'],
                         capture_output=True, text=True)
    return out.stdout.strip(), out.stderr[-500:]

def norm(t):
    t = re.sub(r'org[0-9a-f]{7}', 'orgID', t)
    t = re.sub(r'<meta name="generator".*', '', t)
    t = re.sub(r'\d{4}-\d\d-\d\d \w{3} \d\d:\d\d', 'DATE', t)
    return t

os.makedirs(D, exist_ok=True)
src = D + '/ex.org'
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': OPTS})
u = c.open(src, DOC)
results = {}
def export(keys, wait_for):
    n = len(c.requests); m = len(c.messages)
    r = c.cmd('org.export.dispatch', u, (5, 0), answers=keys)
    end = time.time() + 120
    while time.time() < end:
        docs = [p for k, p in c.requests[n:] if k == 'window/showDocument']
        msgs = c.messages[m:]
        if docs or any(wait_for in str(x) for x in msgs) or any(x[0] == 1 for x in msgs):
            break
        time.sleep(0.2)
    time.sleep(0.5)
    return r, [p for k, p in c.requests[n:] if k == 'window/showDocument'], c.messages[m:]

menu = c.cmd('org.export.dispatch', u, (5, 0))
print('menu:', [ch['key'] + ' ' + ch['label'] for ch in menu['input']['choices']])
for keys, fn, out in [(['h', 'h'], 'org-html-export-to-html', D + '/ex.html'),
                      (['l', 'l'], 'org-latex-export-to-latex', D + '/ex.tex'),
                      (['m', 'm'], 'org-md-export-to-markdown', D + '/ex.md'),
                      (['t', 'a'], 'org-ascii-export-to-ascii', D + '/ex.txt')]:
    if os.path.exists(out): os.remove(out)
    r, docs, msgs = export(keys, 'Exported')
    print('==', keys, 'result', r, 'docs', docs, 'msgs', msgs)
    got = open(out).read() if os.path.exists(out) else None
    shutil.copy(out, out + '.server') if got else None
    os.remove(out) if got else None
    e, err = emacs_batch(fn, src)
    want = open(out).read() if os.path.exists(out) else None
    print('   server file', bool(got), 'emacs file', bool(want), err[-200:] if not want else '')
    if got and want:
        if norm(got) == norm(want):
            print('   SAME as Emacs')
        else:
            import difflib
            print('   DIFF:'); print(''.join(list(difflib.unified_diff(norm(want).splitlines(1), norm(got).splitlines(1)))[:40]))
# buffers, body-only, subtree, visible
for keys in [['t', 'A'], ['\u0002', 'h', 'H'], ['\u0013', 't', 'A'], ['l', 'p'], ['o', 'o'], ['#'], ['&'], ['P', 'x'], ['h', 'o']]:
    r, docs, msgs = export(keys, 'Export')
    print('==', repr(keys), 'result', json.dumps(r)[:300], 'docs', docs, 'msgs', msgs)
    for d in docs:
        p = d['uri'][7:]
        if os.path.exists(p) and not p.endswith(('.odt', '.pdf', '.html')):
            print('   ', open(p).read()[:400].replace('\n', '\n    '))
c.close()
