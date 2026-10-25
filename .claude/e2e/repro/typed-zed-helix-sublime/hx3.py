"""Helix, TESTING.org: links, footnotes, tables, formulas, Babel,
citations, diagnostics, indentation, completion."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

A = """#+LINK: gh https://example.org/%s
* Target heading
:PROPERTIES:
:CUSTOM_ID: cid
:ID: 1234-abcd
:END:
<<here>>
* Links
[[Target heading]] [[#cid]] [[here]] [[file:b.org::*Bee]] [[id:1234-abcd]] [[gh:org-lsp]]
See [fn:1].
Missing [fn:2].
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
* Code
#+begin_src emacs-lisp :tangle out.el
(+ 1 2)
#+end_src
src_emacs-lisp{(* 6 7)}
* Cite
[cite:@knuth]
[cite:@knuht]
#+bibliography: refs.bib
* Lint
text** heading
[[file:nowhere.org]]
#+begin_src python :foo 1
print(1)
#+end_src
#+begin_quote
open

[fn:1] The note.
"""
B = """* Bee
"""
BIB = """@book{knuth, title={The Art}, author={Knuth, Donald}, year={1968}}
"""
I = """#+STARTUP: indent
* Top
Some text under top.
** Child
Text of the child.
"""


def follow(x, text, column=2):
    x.at(text, column)
    x.keys("g", "d")
    return x.where()


def links(x):
    return [follow(x, "[[Target heading]]"), follow(x, "[[#cid]]"), follow(x, "[[here]]"),
            follow(x, "[[file:b.org::*Bee]]")]


def more_links(x):
    x.h.open("a.org")
    a = follow(x, "[[id:1234-abcd]]")
    x.at("[[gh:org-lsp]]", 2); x.keys("g", "d")
    return [a, x.msgs(), x.where()]


def hover_refs(x):
    x.h.open("a.org"); x.at("[[Target heading]]", 3); x.keys("Space", "k")
    hover = [r.strip()[:90] for r in x.screen().split("\n") if "Target" in r][:3]
    x.h.keys("Escape"); x.h.settle()
    x.at("* Target heading", 3); x.keys("g", "r")
    refs = [r.strip()[:90] for r in x.screen().split("\n") if "a.org" in r][:4]
    x.h.keys("Escape"); x.h.settle()
    return [hover, refs]


def rename_heading(x):
    x.at("* Target heading", 3); x.h.keys("Space", "r")
    x.h.wait(lambda: any("rename-to:" in row for row in x.h.screen()), "the rename prompt")
    x.h.keys("C-u"); x.h.type("Goal heading"); x.h.keys("Enter"); x.h.settle()
    return [l for l in x.lines("a.org") if "Goal" in l]


def symbols(x):
    x.keys("Space", "S"); x.h.type("Bee"); x.h.settle()
    rows = [r.strip()[:90] for r in x.screen().split("\n") if "Bee" in r][:3]
    x.h.keys("Escape"); x.h.settle()
    return rows


def link_complete(x):
    x.at("See [fn:1]."); x.keys("o"); x.h.keys("Escape") if False else None
    x.h.type("[[Go"); x.h.keys("C-x"); x.h.settle()
    rows = [r.strip()[:90] for r in x.screen().split("\n") if "Goal" in r][:3]
    x.h.keys("Escape"); x.h.settle(); x.keys("x", "d")
    return rows


def footnote(x):
    a = follow(x, "[fn:1]", 1)
    b = follow(x, "[fn:1] The note.", 1)
    x.keys("g", "d")
    return [a, b, x.msgs()]


def table(x):
    x.at("| 1 |", 2); x.act("Demote the heading")
    a = x.lines("a.org")[13:17]
    x.act("Promote the heading")
    x.act("Move the subtree, the item or the table row down")
    b = x.lines("a.org")[13:17]
    x.act("Move the subtree, the item or the table row up")
    x.act("Demote the subtree")
    c = x.lines("a.org")[13:18]
    x.act("Promote the subtree")
    x.act("Insert a table row above")
    d = x.lines("a.org")[13:18]
    x.act("Delete the table row")
    x.at("| 3 |", 2); x.act("Blank the table field")
    e = x.lines("a.org")[13:18]
    return [a, b, c, d, e]


def shrink(x):
    x.at("| a |", 2); x.act("Shrink or expand the table column")
    return x.msgs()


def recalc(x):
    x.at("| 1 |", 2); x.act("Recalculate the table")
    a = x.lines("a.org")[13:18]
    x.at("#+TBLFM: @>"); x.act("Toggle the checkbox")
    b = x.lines("a.org")[19:26]
    x.at("#+TBLFM: $2=$1+7"); x.act("Toggle the checkbox")
    c = x.lines("a.org")[27:29]
    x.at("#+TBLFM: $3='"); x.act("Toggle the checkbox")
    return [a, b, c, x.lines("a.org")[30:32], x.msgs()]


def sort_rows(x):
    x.at("| 3 |", 2); x.act("Sort the entries", ["N"])
    return [x.lines("a.org")[13:18], x.msgs()]


def tangle(x):
    x.at("(+ 1 2)"); x.act("Tangle the file's code blocks")
    return [x.text("out.el"), x.msgs()]


def babel(x):
    x.at("(+ 1 2)"); x.act("Evaluate the code block", [{"choice": "Yes"}])
    lines = x.lines("a.org")
    i = lines.index("(+ 1 2)")
    a = lines[i + 1:i + 4]
    x.at("src_emacs-lisp", 3); x.act("Evaluate the code block", [{"choice": "Yes"}])
    return [a, [l for l in x.lines("a.org") if "src_emacs" in l], x.msgs()]


def cite(x):
    x.at("[cite:@knuth]", 8); x.keys("Space", "k")
    hover = [r.strip()[:90] for r in x.screen().split("\n") if "Knuth" in r or "Art" in r][:3]
    x.h.keys("Escape"); x.h.settle()
    x.keys("g", "d")
    return [hover, x.where()]


def diagnostics(x):
    x.h.open("a.org"); x.keys("Space", "d")
    rows = [r.strip()[:120] for r in x.screen().split("\n") if "│" in r][:16]
    x.h.keys("Escape"); x.h.settle()
    return rows


def indent(x):
    x.h.open("i.org")
    return [r[:60] for r in x.screen().split("\n")[:6]]


def complete(x):
    x.h.open("i.org"); x.keys("g", "e"); x.h.keys("o"); x.h.type("#+STA"); x.h.keys("C-x"); x.h.settle()
    a = [r.strip()[:60] for r in x.screen().split("\n") if "STARTUP" in r][:3]
    x.h.keys("Escape"); x.h.settle(); x.keys("x", "d")
    x.h.keys("o"); x.h.type("\\alp"); x.h.keys("C-x"); x.h.settle()
    b = [r.strip()[:60] for r in x.screen().split("\n") if "alpha" in r][:3]
    x.h.keys("Escape"); x.h.settle(); x.keys("x", "d")
    return [a, b]


run("typed3", {"a.org": A, "b.org": B, "refs.bib": BIB, "i.org": I},
    {"org_cite_global_bibliography": []}, [
        ("follow links", links),
        ("id and #+LINK", more_links),
        ("hover and references", hover_refs),
        ("rename a heading", rename_heading),
        ("workspace symbols", symbols),
        ("link completion", link_complete),
        ("footnote there and back", footnote),
        ("table edits", table),
        ("shrink", shrink),
        ("formulas", recalc),
        ("sort rows", sort_rows),
        ("tangle", tangle),
        ("babel", babel),
        ("citations", cite),
        ("diagnostics", diagnostics),
        ("indent", indent),
        ("completion", complete),
    ], first="a.org", config="[editor.lsp]\ndisplay-inlay-hints = true\n")
