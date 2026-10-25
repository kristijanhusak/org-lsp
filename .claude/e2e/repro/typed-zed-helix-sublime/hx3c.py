"""Helix: an Emacs Lisp formula, Babel answered once its question comes,
the rest of the table edits."""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

A = """* Elisp
| 1 | 2 |   |
#+TBLFM: $3='(+ $1 $2);N
* Code
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
src_emacs-lisp{(* 6 7)}
* Table
| a | b |   |
|---+---+---|
| 1 | 2 |   |
| 3 | 4 |   |
"""


def elisp(x):
    x.at("#+TBLFM: $3='")
    titles = [t for t in x.titles() if "Recalc" in t or "checkbox" in t]
    x.act("Recalculate the table")
    for _ in range(40):
        x.h.settle()
        if "3 |" in x.lines("a.org")[1]:
            break
        time.sleep(0.5)
    return [titles, x.lines("a.org")[1], x.msgs()]


def wait_question(x, before):
    for _ in range(60):
        x.h.log.read()
        if x.h.question():
            return True
        time.sleep(0.5)
    return False


def babel(x):
    x.at("(+ 1 2)")
    before = x.h.log.count
    x.act("Evaluate the code block")
    asked = wait_question(x, before)
    x.h.answer({"choice": "Yes"}, "babel")
    for _ in range(40):
        x.h.settle()
        if "#+RESULTS:" in x.text():
            break
        time.sleep(0.5)
    a = x.lines("a.org")[4:10]
    x.at("src_emacs-lisp", 3)
    x.act("Evaluate the code block")
    wait_question(x, 0)
    x.h.answer({"choice": "Yes"}, "babel")
    for _ in range(40):
        x.h.settle()
        if "results" in x.text():
            break
        time.sleep(0.5)
    return [asked, a, [l for l in x.lines("a.org") if "src_emacs" in l], x.msgs()]


def table(x):
    x.at("| 1 |", 2)
    x.act("Demote the subtree")
    c = x.lines("a.org")[11:16]
    x.act("Promote the subtree")
    x.act("Insert a table row above")
    d = x.lines("a.org")[11:17]
    x.act("Delete the table row")
    x.at("| 3 |", 2); x.act("Blank the table field")
    return [c, d, x.lines("a.org")[11:17]]


run("typed3c", {"a.org": A}, {}, [
    ("elisp formula", elisp),
    ("babel", babel),
    ("table", table),
], first="a.org")
