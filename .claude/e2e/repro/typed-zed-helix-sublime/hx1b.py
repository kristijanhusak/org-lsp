"""Helix, TESTING.org: lists, top-level promotion, cut and paste,
timestamps, each on a file of its own."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

L = """* Lists
1. a
3. b
2. c
* Boxes
- [ ] x
- [ ] y
* Words
- zeta
- alpha
"""
T = """* Stamp <2026-05-18 Mon 10:30>
* Repeat
SCHEDULED: <2026-05-18 Mon +1w>
"""
S = """* One
** b
** a
* Two
** c
"""


def show(x, name):
    return x.lines(name)


def list_item(x):
    x.at("3. b"); x.keys("g", "l", "l"); x.act("New heading, item or table row"); x.type("new")
    a = show(x, "l.org")
    x.at("- [ ] y"); x.keys("g", "l", "l"); x.act("New heading, item or table row"); x.type("noBox")
    return [a, show(x, "l.org")]


def list_indent(x):
    x.at("- alpha"); x.act("Demote the heading")
    a = show(x, "l.org")
    x.act("Promote the heading")
    x.at("- alpha"); x.act("Move the subtree, the item or the table row up")
    return [a, show(x, "l.org")]


def checkbox(x):
    x.at("- [ ] x"); x.act("Toggle the checkbox")
    return show(x, "l.org")


def renumber(x):
    x.h.open("r.org"); x.at("1. a"); x.act("Toggle the checkbox")
    return [show(x, "r.org"), x.msgs()]


def list_sort(x):
    x.h.open("l.org"); x.at("- alpha"); x.act("Sort the entries", ["a"])
    return [show(x, "l.org"), x.msgs()]


def top_level(x):
    x.h.open("s.org"); x.at("* Two"); x.act("Promote the heading")
    return [show(x, "s.org"), x.msgs()]


def cut_paste(x):
    x.at("** b"); x.act("Cut the subtree")
    a = show(x, "s.org")
    x.at("** a"); x.act("Paste the subtree")
    b = show(x, "s.org")
    x.at("** a"); x.act("Copy the subtree"); x.at("* One"); x.act("Paste the subtree")
    return [a, b, show(x, "s.org"), x.msgs()]


def stamp(x):
    x.h.open("t.org")
    seen = []
    for column in (3, 6, 9, 17, 20):
        x.at("Stamp <", 6 + column); x.act("Increase the part of the timestamp at the cursor")
        seen.append(show(x, "t.org")[0])
    x.act("Decrease the part of the timestamp at the cursor")
    seen.append(show(x, "t.org")[0])
    return seen


def day(x):
    x.at("+1w", 0)
    titles = [t for t in x.titles() if "later" in t or "earlier" in t or "day" in t]
    return titles


run("typed1b", {"l.org": L, "t.org": T, "s.org": S, "r.org": "* R\n1. a\n3. b\n2. c\n"}, {"org_allow_promoting_top_level_subtree": True}, [
    ("new list items", list_item),
    ("indent/move items", list_indent),
    ("toggle checkbox", checkbox),
    ("renumber 1 3 2", renumber),
    ("sort items", list_sort),
    ("promote top level", top_level),
    ("cut/paste copy/paste", cut_paste),
    ("timestamp parts", stamp),
    ("day actions on a timestamp", day),
], first="l.org")
