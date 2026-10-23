"""Helix flows of editors/helix/TESTING.org, step by step; prints what each gave."""
import sys, traceback; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = """* TODO Task
* Parent [/]
** TODO Child
** TODO Other
* Lists
1. a
3. b
2. c
- [ ] x
- [ ] y
* Table
| 1 |   |
| 2 |   |
#+TBLFM: $2=$1*10
* Links
[[Task]] and [[file:b.org::*Bee]]
"""
opts = {"org_todo_keywords": ["TODO(t)", "WAIT(w@)", "|", "DONE(d)"], "org_log_done": "time",
        "org_capture_templates": [{"key": "t", "description": "Task", "target": ["file", "$DIR/inbox.org"], "template": "* TODO %?"},
                                  {"key": "n", "description": "Named", "target": ["file", "$DIR/inbox.org"], "template": "* %^{Name}"}]}
h = Helix("flows", {"files": {"a.org": FILE, "b.org": "* Bee\nSCHEDULED: <2026-05-18 Mon>\n", "inbox.org": "#+title: Inbox\n"}, "options": opts})
A = None
def step(name, fn):
    try:
        r = fn()
        print(f"ok   {name}: {r if r is not None else ''}")
    except Exception as e:
        print(f"FAIL {name}: {e}".splitlines()[0][:400])
        try: h.keys("Escape"); h.settle()
        except Exception: pass
def line(n):
    return (h.text(A) or "").split("\n")[n - 1]
def msgs():
    return h.messages().strip().replace("\n", " | ")
try:
    A = os.path.join(h.directory, "a.org")
    h.start(A)
    def todo_note():
        goto(h, 1, 3); act(h, "Change the TODO state", ["w", "waiting on Bob"])
        return [line(1), line(2), line(3), msgs()]
    step("TODO → WAIT with a note (choice, then rename)", todo_note)
    def todo_done():
        goto(h, 1, 3); act(h, "Change the TODO state", ["d"]); return [line(1), line(2), msgs()]
    step("DONE writes CLOSED", todo_done)
    def cookie():
        n = [l for l in h.text(A).split("\n")].index("** TODO Child") + 1
        goto(h, n, 4); act(h, "Change the TODO state", ["d"]); return [l for l in h.text(A).split("\n") if l.startswith("* Parent")] + [msgs()]
    step("parent cookie recounted", cookie)
    def sched():
        n = h.text(A).split("\n").index("** TODO Other") + 1
        goto(h, n, 4); act(h, "Schedule the entry", ["+2d"]); return [h.text(A).split("\n")[n], msgs()]
    step("schedule +2d by rename", sched)
    def tags():
        n = h.text(A).split("\n").index("** TODO Other") + 1
        goto(h, n, 4); act(h, "Set the tags of the headline", ["b:a"]); return [h.text(A).split("\n")[n - 1], msgs()]
    step("tags typed b:a (sorted? aligned)", tags)
    def renumber():
        n = h.text(A).split("\n").index("1. a") + 1
        goto(h, n, 4); t = titles(h)
        if not any("checkbox" in x for x in t): return "no Toggle the checkbox on '1. a': " + str(t[:6])
        act(h, "Toggle the checkbox"); return h.text(A).split("\n")[n - 1:n + 5]
    step("list 1,3,2 renumbered by Toggle the checkbox", renumber)
    def newitem():
        n = h.text(A).split("\n").index("- [ ] y") + 1
        goto(h, n, 4); act(h, "New heading, item or table row"); return h.text(A).split("\n")[n - 1:n + 1] + [str(h.status()[2:])]
    step("new item after a checkbox item", newitem)
    def recalc():
        n = h.text(A).split("\n").index("| 1 |   |") + 1
        goto(h, n, 3); act(h, "Recalculate the table"); return h.text(A).split("\n")[n - 1:n + 1]
    step("Recalculate the table", recalc)
    def clock():
        goto(h, 1, 3); act(h, "Clock in"); t = titles(h)
        r = [x for x in t if "lock" in x]
        act(h, "Clock out"); return [r, msgs(), [l for l in h.text(A).split("\n") if "CLOCK" in l]]
    step("clock in → out offered, out", clock)
    def clocklast():
        goto(h, 1, 3); act(h, "Clock in the last clocked task"); r = msgs()
        act(h, "Cancel the clock"); return [r, msgs()]
    step("clock in last, cancel", clocklast)
    def capture():
        act(h, "Capture", ["t"]); st = h.status()
        h.type_text("Captured")
        act(h, "Capture: finish"); return [str(st[1:]), msgs(), h.text(os.path.join(h.directory, "inbox.org")), str(h.status()[1:])]
    step("capture t, finish", capture)
    def capture_named():
        h.open(A); act(h, "Capture", ["n", "Named one"]); act(h, "Capture: finish")
        return [h.text(os.path.join(h.directory, "inbox.org")), msgs()]
    step("capture %^{Name}", capture_named)
    def gd():
        h.open(A); n = h.text(A).split("\n").index("[[Task]] and [[file:b.org::*Bee]]") + 1
        goto(h, n, 3); h.keys("g", "d"); h.settle(); r1 = str(h.status()[1:])
        h.open(A); goto(h, n, 20); h.keys("g", "d"); h.settle(); return [r1, str(h.status()[1:])]
    step("gd on [[Task]] and file:b.org::*Bee", gd)
    def refile():
        h.open(A); goto(h, 1, 3); act(h, "Refile the entry", ["Bee"]); return [h.text(os.path.join(h.directory, "b.org")), msgs()]
    step("refile Task to b.org/Bee", refile)
    def archive():
        h.open(A); n = h.text(A).split("\n").index("* Links") + 1
        goto(h, n, 3); act(h, "Archive the subtree"); return [msgs(), os.listdir(h.directory)]
    step("archive Links subtree", archive)
finally:
    h.stop()
