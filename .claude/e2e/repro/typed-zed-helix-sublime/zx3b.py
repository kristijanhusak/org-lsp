"""Zed: Babel answered with its buttons, export, %a, roam and journal."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

A = """* Code
#+name: three
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
src_emacs-lisp{(* 6 7)}
#+CALL: three()
* Plan
"""
YES = {"choice": "Yes"}
opts = {
    "org_capture_templates": [
        {"key": "a", "description": "Link", "target": ["file", "$DIR/inbox.org"], "template": "* %a"},
    ],
}

steps = [
    {"open": "a.org"},
    {"title": "babel yes", "at": "(+ 1 2)", "command": "org.babel.execute", "answers": [YES], "show": ["messages"]},
    {"title": "inline src", "at": "src_emacs-lisp", "column": 3, "command": "org.babel.execute", "answers": [YES]},
    {"title": "#+CALL", "at": "#+CALL:", "command": "org.babel.execute", "answers": [YES], "show": ["buffer"]},
    {"title": "export html file", "at": "* Plan", "command": "org.export.dispatch", "answers": ["h", "h", YES, YES, YES],
     "show": ["messages"], "check": lambda s: os.path.exists(os.path.join(s.directory, "a.html"))},
    {"title": "%a", "at": "* Plan", "command": "org.capture", "answers": ["a"], "show": ["buffer"]},
    {"command": "org.capture.finish", "show": ["file:inbox.org"]},
]
zxlib.run("zx3b", {"a.org": A, "inbox.org": ""}, opts, steps)
