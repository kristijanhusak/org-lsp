"""Helix, TESTING.org: headings, lists, TODO keywords, priorities, tags,
properties, dates."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

A = """* TODO Task :work:
* Parent [/]
** TODO Child
** TODO Other
* Second
** b
** a
* Lists
1. a
3. b
2. c
- [ ] x
- [ ] y
- zeta
- alpha
* Repeat
** TODO Weekly
SCHEDULED: <2026-05-18 Mon +1w>
* Stamp <2026-05-18 Mon 10:30>
"""
B = """#+TODO: A B | C
* A first
"""
C = """#+TODO: TODO | DONE
#+TODO: REPORT BUG | FIXED
* BUG thing
"""
D = """#+PRIORITIES: 1 5 3
* TODO numbers
"""
E = """* Blocked parent
:PROPERTIES:
:ORDERED: t
:END:
** TODO one
** TODO two
* TODO Has box
- [ ] unchecked
"""
opts = {"org_todo_keywords": ["TODO(t)", "WAIT(w@)", "|", "DONE(d)"], "org_log_done": "time",
        "org_enforce_todo_dependencies": True, "org_enforce_todo_checkbox_dependencies": True,
        "org_allow_promoting_top_level_subtree": True}


def first(x):
    return x.titles()[:12]


def new_heading(x):
    x.at("* TODO Task"); x.keys("g", "l", "l")
    x.act("New heading, item or table row"); x.type("Added")
    return [x.lines("a.org")[:3], x.where()]


def todo_heading(x):
    x.at("* Second"); x.keys("g", "l", "l")
    x.act("New TODO heading"); x.type("T2")
    return x.lines("a.org")[4:8]


def after_subtree(x):
    x.at("* Parent", 3)
    x.act("New heading after the subtree"); x.type("After")
    x.at("* Parent", 3)
    x.act("New TODO heading after the subtree"); x.type("After2")
    return x.lines("a.org")[2:9]


def promote(x):
    x.at("** TODO Child", 4); x.act("Demote the heading")
    a = x.where()
    x.act("Promote the heading")
    b = x.where()
    x.at("* Parent", 3); x.act("Demote the subtree")
    c = x.lines("a.org")[3:8]
    x.act("Promote the subtree")
    return [a, b, c, x.lines("a.org")[3:8]]


def moves(x):
    x.at("** b"); x.act("Move the subtree, the item or the table row down")
    a = x.lines("a.org")
    x.act("Move the subtree, the item or the table row up")
    return [a[a.index("* Second") + 1:a.index("* Second") + 3], x.where()]


def cut_paste(x):
    x.at("** a"); x.act("Cut the subtree")
    a = x.lines("a.org")
    x.at("** TODO Child"); x.keys("j")
    x.at("** TODO Other"); x.act("Paste the subtree")
    b = x.lines("a.org")
    x.at("** a"); x.act("Copy the subtree"); x.at("* Lists"); x.act("Paste the subtree")
    return ["** a" in a, b[b.index("** TODO Other"):b.index("** TODO Other") + 3], x.lines("a.org").count("** a")]


def top_level(x):
    x.at("* Lists"); x.act("Promote the heading")
    a = [l for l in x.lines("a.org") if "Lists" in l]
    x.act("Demote the heading")
    return [a, [l for l in x.lines("a.org") if "Lists" in l], x.msgs()]


def sort(x):
    x.at("* Second"); x.act("Sort the entries", ["a"])
    lines = x.lines("a.org")
    i = lines.index("* Second")
    return [lines[i + 1:i + 3], x.msgs()]


def outline(x):
    x.keys("Space", "s")
    screen = x.screen()
    x.keys("Escape")
    return [row.strip() for row in screen.split("\n") if "Child" in row or "Parent" in row][:4]


def list_item(x):
    x.at("3. b"); x.keys("g", "l", "l"); x.act("New heading, item or table row"); x.type("new")
    a = x.lines("a.org")
    x.at("- [ ] y"); x.keys("g", "l", "l"); x.act("New heading, item or table row"); x.type("noBox")
    b = x.lines("a.org")
    i, j = a.index("* Lists"), b.index("* Lists")
    return [a[i + 1:i + 5], b[j + 5:j + 9]]


def list_indent(x):
    x.at("- zeta"); x.act("Demote the heading")
    a = [l for l in x.lines("a.org") if "zeta" in l]
    x.act("Promote the heading")
    x.at("- alpha"); x.act("Move the subtree, the item or the table row up")
    return [a, [l for l in x.lines("a.org") if "alpha" in l or "zeta" in l]]


def checkbox(x):
    x.at("- [ ] x"); x.act("Toggle the checkbox")
    return [l for l in x.lines("a.org") if "] x" in l]


def renumber(x):
    x.at("1. a"); x.act("Toggle the checkbox")
    i = x.lines("a.org").index("* Lists")
    return [x.lines("a.org")[i + 1:i + 5], x.msgs()]


def list_sort(x):
    x.at("- alpha"); x.act("Sort the entries", ["a"])
    return [[l for l in x.lines("a.org") if l in ("- zeta", "- alpha")], x.msgs()]


def todo_key(x):
    x.at("* TODO Task", 3); x.act("Change the TODO state", ["w", "waiting on Bob"])
    return [x.lines("a.org")[:4], x.msgs()]


def todo_done(x):
    x.at("* WAIT Task", 3); x.act("Change the TODO state", ["d"])
    return [x.lines("a.org")[:3], x.msgs()]


def cycle(x):
    x.at("** TODO Other", 4); x.act("Cycle TODO state forward")
    a = x.where()
    x.act("Cycle TODO state backward")
    return [a, x.where()]


def cookie(x):
    x.at("** TODO Child", 4); x.act("Change the TODO state", ["d"])
    return [l for l in x.lines("a.org") if "Parent" in l] + [x.msgs()]


def repeater(x):
    x.at("** TODO Weekly", 4); x.act("Change the TODO state", ["d"])
    i = x.lines("a.org").index("* Repeat")
    return [x.lines("a.org")[i + 1:i + 6], x.msgs()]


def todo_line(x):
    x.h.open("b.org"); x.at("* A first", 3); x.act("Change the TODO state", ["b"])
    return [x.lines("b.org")[1], x.msgs()]


def todo_sets(x):
    x.h.open("c.org"); x.at("* BUG", 3)
    seen = []
    for _ in range(4):
        x.act("Cycle TODO state forward")
        seen.append(x.lines("c.org")[2])
    return seen + [x.msgs()]


def blocked(x):
    x.h.open("e.org"); x.at("** TODO two", 4); x.act("Change the TODO state", ["d"])
    a = [x.lines("e.org")[5], x.msgs()]
    x.at("* TODO Has box", 3); x.act("Change the TODO state", ["d"])
    return a + [x.lines("e.org")[6], x.msgs()]


def priorities(x):
    x.h.open("a.org"); x.at("** TODO Other", 4); x.act("Raise the priority")
    a = x.where()
    x.act("Lower the priority"); x.act("Lower the priority")
    b = x.where()
    x.act("Set the priority", ["a"])
    c = x.where()
    x.act("Set the priority", [" "])
    return [a, b, c, x.where(), x.msgs()]


def priority_numbers(x):
    x.h.open("d.org"); x.at("* TODO numbers", 3); x.act("Raise the priority")
    a = x.lines("d.org")[1]
    x.act("Raise the priority")
    return [a, x.lines("d.org")[1], x.msgs()]


def tags(x):
    x.h.open("a.org"); x.at("** TODO Other", 4); x.act("Set the tags of the headline", ["b:a"])
    return [l for l in x.lines("a.org") if "Other" in l] + [x.msgs()]


def tag_complete(x):
    x.at("** b"); x.keys("g", "l"); x.h.keys("a"); x.h.type(" :wo"); x.h.keys("C-x"); x.h.settle()
    screen = x.screen()
    x.h.keys("Escape"); x.h.settle()
    return [row.strip() for row in screen.split("\n") if "work" in row][:3]


def tag_rename(x):
    x.at(":work:", 1); x.h.keys("Space", "r")
    x.h.wait(lambda: any("rename-to:" in row for row in x.h.screen()), "the rename prompt")
    x.h.keys("C-u"); x.h.type("job"); x.h.keys("Enter"); x.h.settle()
    return [l for l in x.lines("a.org") if ":job:" in l or ":work:" in l]


def effort(x):
    x.at("** TODO Other", 4); x.act("Set the effort of the entry", ["1:00"])
    i = x.lines("a.org").index([l for l in x.lines("a.org") if "Other" in l][0])
    return [x.lines("a.org")[i:i + 4], x.msgs()]


def schedule(x):
    x.at("* Second", 3); x.act("Schedule the entry", ["+2d"])
    x.act("Set the deadline of the entry", ["fri 10:00"])
    i = x.lines("a.org").index("* Second")
    return [x.lines("a.org")[i + 1], x.msgs()]


def timestamp_parts(x):
    seen = []
    for column in (8, 12, 15, 22, 25):
        x.at("<2026-05-18 Mon 10:30>", column); x.act("Increase the part of the timestamp at the cursor")
        seen.append([l for l in x.lines("a.org") if l.startswith("* Stamp")][0])
    x.act("Decrease the part of the timestamp at the cursor")
    seen.append([l for l in x.lines("a.org") if l.startswith("* Stamp")][0])
    return seen


def day_later(x):
    x.at("<2026-05-18 Mon +1w>", 3)
    titles = x.titles()
    x.act("Move the date one day later") if any(t.startswith("Move the date one day later") for t in titles) else None
    a = [l for l in x.lines("a.org") if "+1w" in l]
    x.act("Move the date one day earlier") if any(t.startswith("Move the date one day earlier") for t in titles) else None
    return [[t for t in titles if "day" in t], a, [l for l in x.lines("a.org") if "+1w" in l]]


def warning_kept(x):
    x.at("* Second", 3); x.act("Set the deadline of the entry", ["fri -2d"])
    i = x.lines("a.org").index("* Second")
    line = x.lines("a.org")[i + 1]
    x.at("DEADLINE: <", 11); x.act("Increase the part of the timestamp at the cursor")
    return [line, x.lines("a.org")[i + 1]]


run("typed1", {"a.org": A, "b.org": B, "c.org": C, "d.org": D, "e.org": E}, opts, [
    ("actions on a headline", first),
    ("new heading", new_heading),
    ("new TODO heading", todo_heading),
    ("headings after the subtree", after_subtree),
    ("promote/demote heading and subtree", promote),
    ("subtree down/up", moves),
    ("cut/paste, copy/paste", cut_paste),
    ("promote top level", top_level),
    ("sort children", sort),
    ("outline Space s", outline),
    ("new list items", list_item),
    ("indent/move items", list_indent),
    ("toggle checkbox", checkbox),
    ("renumber 1 3 2", renumber),
    ("sort items", list_sort),
    ("TODO by key + note", todo_key),
    ("DONE CLOSED", todo_done),
    ("cycle", cycle),
    ("cookie", cookie),
    ("repeater", repeater),
    ("#+TODO line", todo_line),
    ("keyword sets cycle", todo_sets),
    ("blocked by deps", blocked),
    ("priorities", priorities),
    ("#+PRIORITIES", priority_numbers),
    ("tags typed", tags),
    ("tag completion", tag_complete),
    ("tag rename", tag_rename),
    ("effort", effort),
    ("schedule/deadline", schedule),
    ("timestamp parts", timestamp_parts),
    ("day later/earlier", day_later),
    ("warning kept", warning_kept),
], first="a.org")
