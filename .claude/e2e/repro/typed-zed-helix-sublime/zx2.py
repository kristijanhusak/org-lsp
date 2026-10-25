"""Zed, TESTING.org: narrowing, inline tasks, goto, comment, clone, links,
footnotes, tables and formulas; each step typed."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

for command in ("org.meta.right", "org.table.recalculate", "org.shiftmeta.right", "org.shiftmeta.left",
                "org.shiftmeta.down", "org.shiftmeta.up", "org.table.blank.field", "org.meta.down"):
    zxlib.zr.EXCEPTED.setdefault(command, "by hand: from the code actions")

A = """#+LINK: gh https://example.org/%s
* Target heading
:PROPERTIES:
:CUSTOM_ID: cid
:ID: 1234-abcd
:END:
<<here>>
* Links
[[Target heading]] [[#cid]] [[here]] [[file:b.org::*Bee]] [[id:1234-abcd]] [[gh:org-lsp]]
[[No such heading]] [[shell:echo hi]] <2026-05-20 Wed>
See [fn:1].
* Narrow me
Some text.
* Clone <2026-05-18 Mon>
* Table
| a | b |   |
|---+---+---|
| 1 | 2 |   |
| 3 | 4 |   |
#+TBLFM: $3=$1*$2
* Elisp
| 1 | 2 |   |
#+TBLFM: $3='(+ $1 $2);N

[fn:1] The note.
"""
B = "* Bee\n"

steps = [
    {"open": "a.org"},
    {"title": "follow heading link", "at": "[[Target heading]]", "column": 2, "command": "org.open", "show": ["cursor"]},
    {"title": "custom id", "at": "[[#cid]]", "column": 2, "command": "org.open", "show": ["cursor"]},
    {"title": "target", "at": "[[here]]", "column": 2, "command": "org.open", "show": ["cursor"]},
    {"title": "file::*search", "at": "[[file:b.org", "column": 2, "command": "org.open", "show": ["cursor"]},
    {"open": "a.org"},
    {"title": "id link", "at": "[[id:1234", "column": 2, "command": "org.open", "show": ["cursor"]},
    {"title": "#+LINK abbreviation (external)", "at": "[[gh:org-lsp]]", "column": 2, "command": "org.open",
     "show": ["messages"], "check": lambda s: [m for m in s.zed.log.shows][-2:]},
    {"title": "missing heading, yes", "at": "[[No such heading]]", "column": 2, "command": "org.open",
     "answers": ["y"], "show": ["cursor", "messages"]},
    {"title": "shell link yes", "at": "[[shell:echo hi]]", "column": 2, "command": "org.open", "answers": ["y"],
     "show": ["messages"]},
    {"title": "timestamp opens the day's agenda", "at": "<2026-05-20 Wed>", "column": 2, "command": "org.open",
     "show": ["agenda"]},
    {"open": "a.org"},
    {"title": "footnote there", "at": "See [fn:1]", "column": 5, "command": "org.open", "show": ["cursor"]},
    {"title": "and back", "command": "org.open", "show": ["cursor", "messages"]},
    {"title": "store link", "at": "* Target heading", "command": "org.link.store", "show": ["messages"]},
    {"title": "insert link", "at": "Some text.", "below": True, "command": "org.link.insert",
     "answers": ["", "Desc"], "show": ["buffer"]},
    {"title": "next link", "at": "* Links", "command": "org.link.next", "show": ["cursor"]},
    {"title": "inline task", "at": "Some text.", "below": True, "command": "org.inlinetask.insert", "show": ["buffer"]},
    {"title": "go to heading", "command": "org.goto", "answers": ["Table/"], "show": ["cursor"]},
    {"title": "COMMENT", "at": "* Narrow me", "command": "org.toggle.comment", "show": ["cursor"]},
    {"command": "org.toggle.comment", "show": ["cursor"]},
    {"title": "clone", "at": "* Clone", "command": "org.subtree.clone", "answers": ["2", "+1w"], "show": ["buffer"]},
    {"title": "narrow", "at": "* Narrow me", "command": "org.narrow.subtree", "show": ["buffer"]},
    {"type": "More text.\n", "show": []},
    {"title": "widen", "command": "org.narrow.widen", "show": ["buffer", "file:a.org"]},
    {"title": "recalculate", "at": "| 1 |", "column": 2, "command": "org.table.recalculate", "show": ["buffer"]},
    {"title": "elisp formula C-c C-c", "at": "#+TBLFM: $3='", "command": "org.checkbox.toggle",
     "show": ["buffer", "messages"]},
    {"title": "column right", "at": "| 1 |", "column": 2, "command": "org.meta.right", "show": ["buffer"]},
    {"title": "rule below", "at": "| 2 |", "column": 2, "command": "org.table.hline", "show": ["buffer"]},
    {"title": "sum", "at": "| 2 |", "column": 2, "command": "org.table.sum", "show": ["messages"]},
    {"title": "field info", "command": "org.table.field.info", "show": ["messages"]},
    {"title": "create table", "at": "* Elisp", "below": True, "command": "org.table.create", "answers": ["2x1"],
     "show": ["buffer"]},
]

zxlib.run("zx2", {"a.org": A, "b.org": B}, {}, steps)
