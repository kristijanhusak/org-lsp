"""Each value kind of emacs_options, set to something other than the
default, read back from Emacs."""
import sys, json; sys.path.insert(0, '/home/user/org-lsp/.claude/e2e/repro/emacs')
from lsp import *
LOAD = '(add-to-list (quote load-path) "/root/org-mode/lisp")'
cases = [
    # name, feature, value, expected %S
    ('org-export-with-author', 'ox', False, 'nil'),
    ('org-export-headline-levels', 'ox', 2, '2'),
    ('org-export-default-language', 'ox', 'fr', '"fr"'),
    ('org-attach-method', 'org-attach', 'lns', 'lns'),
    ('org-export-with-toc', 'ox', 2, '2'),
    ('org-export-with-sub-superscripts', 'ox', '{}', '{}'),
    ('org-export-exclude-tags', 'ox', ['private', 'x'], '("private" "x")'),
    ('org-babel-load-languages', 'org', {'emacs-lisp': True, 'python': True, 'shell': True},
     '((emacs-lisp . t) (python . t) (shell . t))'),
    ('org-format-latex-options', 'org', {'scale': 2.0, 'foreground': 'default'}, '(:scale 2.0 :foreground default)'),
    ('org-html-table-default-attributes', 'ox-html', {'border': '1', 'rules': 'all'}, '(:border "1" :rules "all")'),
    ('org-odt-pixels-per-inch', 'ox-odt', 100, '100.0'),
    ('org-babel-comint-fallback-regexp-threshold', 'ob-comint', 3, '3.0'),
    ('org-ascii-underline', 'ox-ascii', [['ascii', '=', '-'], ['latin1', '=', '~'], ['utf-8', '═', '─']],
     '((ascii 61 45) (latin1 61 126) (utf-8 9552 9472))'),
    ('org-cite-natbib-options', 'oc-natbib', ['round', 'sort'], '(round sort)'),
    ('org-export-backends', 'ox', ['ascii', 'html', 'md'], '(ascii html md)'),
    ('org-html-table-header-tags', 'ox-html', ['<th%s>', '</th>'], '("<th%s>" . "</th>")'),
    ('org-structure-template-alist', 'org', {'q': 'quote', 'z': 'src zsh'}, '(("q" . "quote") ("z" . "src zsh"))'),
    ('org-file-apps', 'org', {'auto-mode': 'emacs', '\\.pdf\\\'': 'evince %s'},
     '((auto-mode . emacs) ("\\\\.pdf\\\\\'" . "evince %s"))'),
    ('org-icalendar-include-body', 'ox-icalendar', 100, '100'),
    ('org-icalendar-include-todo', 'ox-icalendar', ['TODO', 'NEXT'], '("TODO" "NEXT")'),
    ('org-export-with-tasks', 'ox', 'todo', 'todo'),
    ('org-export-with-drawers', 'ox', ['not', 'LOGBOOK'], '(not "LOGBOOK")'),
    ('org-html-coding-system', 'ox-html', 'utf-8-unix', 'utf-8-unix'),
    ('org-latex-compiler', 'ox-latex', 'xelatex', '"xelatex"'),
    ('org-html-doctype', 'ox-html', 'html5', '"html5"'),
    ('org-table-copy-increment', 'org-table', 5, '5'),
    ('org-html-postamble', 'ox-html', 'auto', 'auto'),
    ('org-html-postamble', 'ox-html', 'auto', 'auto'),
    ('org-latex-pdf-process', 'ox-latex', ['latexmk -pdf %f'], '("latexmk -pdf %f")'),
    ('org-src-lang-modes', 'org-src', {'zsh': 'sh'}, '(("zsh" . sh))'),
    ('org-export-smart-quotes-alist', 'ox', {'xx': {'primary-opening': {'utf-8': '«'}}}, None),
    ('org-html-mathjax-options', 'ox-html', [['path', 'x.js'], ['scale', 1.0]], None),
    ('org-columns-checkbox-allowed-values', 'org-colview', ['[ ]', '[X]'], '("[ ]" "[X]")'),
    ('org-babel-python-command', 'ob-python', 'python3', '"python3"'),
    ('org-babel-python-command-session', 'ob-python', 'auto', 'auto'),
    ('org-export-with-broken-links', 'ox', 'mark', 'mark'),
    ('org-attach-id-dir', 'org-attach', 'my-data/', '"my-data/"'),
    ('org-plantuml-args', 'ob-plantuml', ['-headless', '-x'], '("-headless" "-x")'),
    ('org-babel-js-cmd', 'ob-js', 'node', '"node"'),
    ('org-latex-classes', 'ox-latex', [['mini', '\\documentclass{article}', ['\\section{%s}', '\\section*{%s}']]],
     '(("mini" "\\\\documentclass{article}" ("\\\\section{%s}" . "\\\\section*{%s}")))'),
]
opts = {}
for n, f, v, e in cases:
    opts[n] = v
exprs = [f"(condition-case e (progn (require (quote {f})) (replace-regexp-in-string \"\\n\" \" \" (format \"%S\" {n}))) (error (format \"ERR %S\" e)))"
         for n, f, v, e in cases]
c = Client(settings={'message_prompts': False, 'emacs_config': LOAD, 'emacs_options': opts,
                     'org_capture_templates': [probe_template(exprs)]})
u = c.open(SCRATCH + '/kinds.org', '* H\n')
res = probe(c, u, exprs)
for (n, f, v, e), x in zip(cases, exprs):
    got = res.get(x)
    if got is not None:
        got = json.loads(got) if got.startswith('"') else got
    ok = (e is None) or got == e
    print('OK ' if ok else 'BAD', n, json.dumps(v, ensure_ascii=False), '->', got, '' if ok else f'(want {e})')
c.settle(1)
print('messages:'); [print(' ', m) for m in c.messages]
c.close()
