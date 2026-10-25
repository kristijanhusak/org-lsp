"""Helix: Babel, an abbreviated link, formulas, the rest of the table
edits, what the server and Helix said."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

A = """#+LINK: gh https://example.org/%s
* Code
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
src_emacs-lisp{(* 6 7)}
* Links
[[gh:org-lsp]] [[https://orgmode.org]]
* Table
| a | b |   |
|---+---+---|
| 1 | 2 |   |
| 3 | 4 |   |
#+TBLFM: $3=$1*$2
* Sum
| x |
|---|
| 1.5 |
| 2.25 |
|   |
#+TBLFM: @>$1=vsum(@2..@-1);%.2f
* Dates
| <2026-05-18 Mon> |   |
#+TBLFM: $2=$1+7
* Elisp
| 1 | 2 |   |
#+TBLFM: $3='(+ $1 $2);N
"""


def said(x, after):
    out = []
    for index, m in x.h.log.received:
        if index > after and m.get("method") in ("window/showMessage", "window/showDocument", "window/showMessageRequest",
                                                  "workspace/applyEdit"):
            out.append(json.dumps(m)[:300])
    for index, m in x.h.log.sent:
        if index > after and m.get("method") == "workspace/executeCommand":
            out.append(json.dumps(m)[:300])
    return out


def babel(x):
    before = x.h.log.count
    x.at("(+ 1 2)")
    x.act("Evaluate the code block")
    x.h.settle()
    return [said(x, before), x.lines("a.org")[2:9], x.screen().split("\n")[-1]]


def answer_babel(x):
    titles = [t for t in x.titles() if t.endswith("]") or "Yes" in t or "yes" in t]
    return titles


def link(x):
    before = x.h.log.count
    x.at("[[gh:org-lsp]]", 2); x.keys("g", "d")
    a = said(x, before)
    before = x.h.log.count
    x.at("[[https://orgmode.org]]", 2); x.keys("g", "d")
    return [a, said(x, before), x.screen().split("\n")[-1]]


def formulas(x):
    out = []
    for line in ("#+TBLFM: @>", "#+TBLFM: $2=$1+7", "#+TBLFM: $3='"):
        x.at(line)
        titles = [t for t in x.titles() if "checkbox" in t or "Recalc" in t]
        x.act("Recalculate the table")
        out.append(titles)
    return out + [x.lines("a.org")[15:27], x.msgs()]


def table(x):
    x.at("| 1 |", 2)
    x.act("Demote the subtree")
    c = x.lines("a.org")[9:14]
    x.act("Promote the subtree")
    x.act("Insert a table row above")
    d = x.lines("a.org")[9:15]
    x.act("Delete the table row")
    x.at("| 3 |", 2); x.act("Blank the table field")
    return [c, d, x.lines("a.org")[9:15]]


run("typed3b", {"a.org": A}, {}, [
    ("babel", babel),
    ("babel answers", answer_babel),
    ("links", link),
    ("formulas", formulas),
    ("table", table),
], first="a.org")
