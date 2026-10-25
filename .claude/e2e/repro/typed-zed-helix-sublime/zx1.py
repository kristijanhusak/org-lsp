"""Zed, TESTING.org: folding, headings, lists, TODO keywords, priorities,
tags, properties, dates; each step typed."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

for command in ("org.todo.next", "org.todo.previous", "org.priority.up", "org.priority.down", "org.meta.left",
                "org.meta.right", "org.shiftmeta.left", "org.shiftmeta.right", "org.meta.up", "org.meta.down",
                "org.timestamp.up", "org.timestamp.down", "org.date.later", "org.date.earlier"):
    zxlib.zr.EXCEPTED.setdefault(command, "by hand: from the code actions")

A = """* TODO Task :work:
* Parent [/]
** TODO Child
** TODO Other
* Second
** b
** a
* Repeat
** TODO Weekly
SCHEDULED: <2026-05-18 Mon +1w>
* Stamp <2026-05-18 Mon 10:30>
"""
L = """* Lists
1. a
3. b
2. c
* Boxes [/]
- [ ] x
- [ ] y
* Words
- zeta
- alpha
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
"""
opts = {"org_todo_keywords": ["TODO(t)", "WAIT(w@)", "|", "DONE(d)"], "org_log_done": "time",
        "org_enforce_todo_dependencies": True, "org_allow_promoting_top_level_subtree": True}

steps = [
    {"open": "a.org"},
    {"title": "new heading (alt-enter, at the end of the line)", "at": "* TODO Task", "below": True,
     "command": "org.meta.return", "show": []},
    {"type": "Added", "show": ["buffer"]},
    {"title": "TODO heading after the subtree", "at": "** TODO Other", "command": "org.insert.todo.heading.after"},
    {"type": "After", "show": ["buffer"]},
    {"title": "demote, promote", "at": "** TODO Child", "command": "org.meta.right", "show": ["cursor"]},
    {"command": "org.meta.left", "show": ["cursor"]},
    {"title": "subtree down", "at": "** b", "command": "org.meta.down", "show": ["cursor"]},
    {"title": "cut subtree", "at": "** a", "command": "org.cut.subtree", "show": ["messages"]},
    {"title": "paste subtree", "at": "** b", "command": "org.paste.subtree", "show": ["buffer", "messages"]},
    {"title": "sort children", "at": "* Second", "command": "org.sort", "answers": ["a"], "show": ["buffer", "messages"]},
    {"title": "next heading", "at": "* Parent", "command": "org.heading.next", "show": ["cursor"]},
    {"title": "forward same level", "command": "org.heading.forward", "show": ["cursor"]},
    {"title": "up to parent", "command": "org.heading.up", "show": ["cursor"]},
    {"title": "up at top level", "at": "* Second", "command": "org.heading.up", "show": ["messages"]},
    {"title": "TODO by key with note", "at": "* TODO Task", "command": "org.todo", "answers": ["w", "on Bob"],
     "show": ["buffer", "messages"]},
    {"title": "DONE writes CLOSED", "command": "org.todo", "answers": ["d"], "show": ["buffer"]},
    {"title": "cookie", "at": "** TODO Child", "command": "org.todo", "answers": ["d"], "show": ["buffer"]},
    {"title": "repeater", "at": "** TODO Weekly", "command": "org.todo", "answers": ["d"], "show": ["buffer", "messages"]},
    {"title": "priority set and removed", "at": "** TODO Other", "command": "org.priority", "answers": ["a"], "show": ["cursor"]},
    {"command": "org.priority", "answers": [" "], "show": ["cursor", "messages"]},
    {"title": "tags typed", "command": "org.tags.set", "answers": ["b:a"], "show": ["cursor"]},
    {"title": "property", "command": "org.property.set", "answers": ["Owner", "Ann"], "show": ["buffer"]},
    {"title": "effort", "command": "org.effort.set", "answers": ["1:00"], "show": ["messages"]},
    {"title": "ORDERED", "command": "org.ordered.toggle", "show": ["messages"]},
    {"title": "schedule +2d", "at": "* Second", "command": "org.schedule", "answers": ["+2d"], "show": ["messages"]},
    {"title": "deadline fri 10:00", "command": "org.deadline", "answers": ["fri 10:00"], "show": ["buffer", "messages"]},
    {"title": "timestamp part up", "at": "<2026-05-18 Mon 10:30>", "column": 9, "command": "org.timestamp.up",
     "show": ["cursor"]},
    {"title": "day later", "command": "org.date.later", "show": ["cursor"]},
    {"title": "insert active timestamp", "at": "* Stamp", "below": True, "command": "org.timestamp.insert",
     "answers": ["2026-06-01"], "show": ["cursor"]},
    {"title": "lists file", "open": "l.org"},
    {"title": "new item", "at": "3. b", "below": True, "command": "org.meta.return"},
    {"type": "new", "show": ["buffer"]},
    {"title": "checkbox", "at": "- [ ] x", "command": "org.checkbox.toggle", "show": ["buffer"]},
    {"title": "sort items", "at": "- zeta", "command": "org.sort", "answers": ["a"], "show": ["buffer", "messages"]},
    {"title": "keyword sets", "open": "c.org"},
    {"at": "* BUG", "command": "org.todo", "show": ["cursor"]},
    {"command": "org.todo", "show": ["cursor"]},
    {"command": "org.todo.nextset", "show": ["cursor", "messages"]},
    {"title": "priority numbers", "open": "d.org"},
    {"at": "* TODO numbers", "command": "org.priority.up", "show": ["cursor"]},
    {"title": "blocked", "open": "e.org"},
    {"at": "** TODO two", "command": "org.todo", "answers": ["d"], "show": ["cursor", "messages"]},
]

zxlib.run("zx1", {"a.org": A, "l.org": L, "c.org": C, "d.org": D, "e.org": E}, opts, steps)
