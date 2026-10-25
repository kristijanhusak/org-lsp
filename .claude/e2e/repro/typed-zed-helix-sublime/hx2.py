"""Helix, TESTING.org: the agenda beyond the scenarios, clocking, capture,
refile and archive, notes."""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hxlib import run  # noqa: E402

W = """* TODO Write report :work:urgent:
SCHEDULED: <2026-05-18 Mon>
:PROPERTIES:
:EFFORT:   0:30
:END:
:LOGBOOK:
CLOCK: [2026-05-18 Mon 09:00]--[2026-05-18 Mon 09:40] =>  0:40
:END:
* TODO Call Bob :work:
DEADLINE: <2026-05-20 Wed>
* Projects
** TODO Plan
* Clocks
#+BEGIN: clocktable :scope file :maxlevel 2 :block today
#+END:
"""
H = """* TODO Water plants :home:
SCHEDULED: <2026-05-18 Mon>
"""
opts = {
    "org_agenda_custom_commands": [{"key": "n", "description": "Next actions", "type": "todo", "match": "TODO"}],
    "org_capture_templates": [
        {"key": "t", "description": "Task", "target": ["file", "$DIR/inbox.org"], "template": "* TODO %?"},
        {"key": "n", "description": "Named", "target": ["file", "$DIR/inbox.org"], "template": "* %^{Name}"},
        {"key": "d", "description": "Diary", "target": ["file+olp+datetree", "$DIR/diary.org"], "template": "* %?"},
        {"key": "a", "description": "Link", "target": ["file", "$DIR/inbox.org"], "template": "* %a"},
        {"key": "s", "description": "Snippet", "target": ["file", "$DIR/inbox.org"], "template": "* %[snippet.txt]"},
        {"key": "u", "description": "Upcase", "target": ["file", "$DIR/inbox.org"], "template": "* %(upcase \"abc\")"},
    ],
    "org_refile_targets": [{"files": ["$DIR/work.org", "$DIR/home.org"], "maxlevel": 2}],
    "org_log_into_drawer": True,
}


def menu(x):
    x.act("Agenda commands")
    titles = [t for t in x.titles() if t.endswith("]")]
    x.h.answer("n", "menu")
    agenda = x.h.agenda()
    return [titles, (x.h.text(agenda) or "").split("\n")[:4]]


def tag_search(x):
    x.h.open("work.org")
    x.act("Agenda commands", ["m", "work+urgent"])
    return (x.h.text(x.h.agenda()) or "").split("\n")[:4]


def word_search(x):
    x.h.open("work.org")
    x.act("Agenda commands", ["s", "Bob"])
    return (x.h.text(x.h.agenda()) or "").split("\n")[:4]


def week(x):
    x.h.open("work.org")
    x.act("Agenda for the current week or day")
    return (x.h.text(x.h.agenda()) or "").split("\n")[:14]


def hover(x):
    x.at("Call Bob")
    x.keys("Space", "k")
    screen = x.screen()
    x.h.keys("Escape"); x.h.settle()
    return [row.strip()[:100] for row in screen.split("\n") if "Bob" in row or "DEADLINE" in row][:4]


def line_actions(x):
    x.at("Call Bob")
    return [t for t in x.titles() if "keyword set" in t or "Remove" in t]


def note(x):
    x.at("Water plants"); x.act("Add a note to the entry", ["Bought soil"])
    return [x.lines("home.org"), x.msgs()]


def changed_in_file(x):
    x.h.open("home.org"); x.keys("g", "e"); x.h.keys("o"); x.h.type("* TODO Fresh\nSCHEDULED: <2026-05-18 Mon>"); x.h.escape()
    x.cmd("write")
    x.h.go_to_buffer(x.h.agenda())
    x.act("Agenda: rebuild")
    return [l for l in (x.h.text(x.h.agenda()) or "").split("\n") if "Fresh" in l]


def clock(x):
    x.h.open("work.org"); x.at("* TODO Write report", 3); x.act("Clock in")
    a = [x.msgs()]
    x.at("** TODO Plan", 4); x.act("Clock in")
    a.append(x.msgs())
    x.act("Clock out")
    a.append(x.msgs())
    x.act("Clock in the last clocked task")
    a.append(x.msgs())
    x.act("Cancel the clock")
    a.append(x.msgs())
    lines = x.lines("work.org")
    return a + [lines[lines.index("** TODO Plan"):lines.index("** TODO Plan") + 5]]


def goto_clocked(x):
    x.h.open("work.org"); x.at("** TODO Plan", 4); x.act("Clock in")
    x.h.open("home.org"); x.act("Go to the clocked entry")
    a = x.where()
    x.h.open("work.org"); x.at("** TODO Plan", 4); x.act("Clock out")
    return [a, x.msgs()]


def clocktable(x):
    x.at("#+BEGIN: clocktable"); x.act("Update the dynamic block")
    lines = x.lines("work.org")
    i = lines.index("* Clocks")
    a = lines[i + 1:i + 10]
    x.at("#+BEGIN: clocktable", 60)
    x.act("Move the date one day earlier") if False else None
    return [a, x.msgs()]


def capture_menu(x):
    x.h.open("work.org"); x.act("Capture")
    titles = [t for t in x.titles() if t.endswith("]")]
    x.h.answer("t", "capture")
    a = x.where()
    x.type("Captured")
    x.act("Capture: finish")
    return [titles, a, x.lines("inbox.org"), x.msgs()]


def capture_prompt(x):
    x.act("Capture", ["n", "Ann"])
    a = x.where()
    x.act("Capture: abort")
    return [a, x.lines("inbox.org"), x.msgs()]


def capture_refile(x):
    x.act("Capture", ["t"]); x.type("Move me")
    x.act("Capture: refile the entry", [{"choice": "Projects"}])
    return [x.lines("work.org")[10:14], x.msgs()]


def datetree(x):
    x.act("Capture", ["d"]); x.type("Diary entry"); x.act("Capture: finish")
    return x.lines("diary.org")


def link_template(x):
    x.h.open("work.org"); x.at("Call Bob")
    x.act("Capture", ["a"]); x.act("Capture: finish")
    return x.lines("inbox.org")[-3:]


def snippet(x):
    x.act("Capture", ["s"]); x.act("Capture: finish")
    x.h.open("work.org")
    x.act("Capture", ["u"]); x.act("Capture: finish")
    return x.lines("inbox.org")[-3:]


def refile(x):
    x.h.open("work.org"); x.at("* TODO Call Bob", 3); x.act("Refile the entry", [{"choice": "Water plants"}])
    return [x.lines("home.org"), x.msgs()]


def archive(x):
    x.h.open("work.org"); x.at("** TODO Plan", 4); x.act("Archive the subtree")
    return [x.lines("work.org_archive")[:8], x.msgs()]


def archive_sibling(x):
    x.h.open("home.org"); x.at("* TODO Water plants", 3); x.act("Toggle the ARCHIVE tag")
    a = x.lines("home.org")[0]
    x.act("Toggle the ARCHIVE tag")
    x.act("Archive the subtree under its Archive sibling")
    return [a, x.lines("home.org"), x.msgs()]


run("typed2", {"work.org": W, "home.org": H, "inbox.org": "", "snippet.txt": "snip text\n"}, opts, [
    ("agenda menu + custom command", menu),
    ("tag search work+urgent", tag_search),
    ("word search", word_search),
    ("week view", week),
    ("hover a line", hover),
    ("keyword sets and removing dates on an agenda line", line_actions),
    ("note from the agenda", note),
    ("entry changed in its file, rebuilt", changed_in_file),
    ("clock in/out/last/cancel", clock),
    ("go to the clocked entry", goto_clocked),
    ("clock table update", clocktable),
    ("capture menu + finish", capture_menu),
    ("capture %^{} + abort", capture_prompt),
    ("capture refile", capture_refile),
    ("datetree", datetree),
    ("%a", link_template),
    ("%[file] and %()", snippet),
    ("refile to another file", refile),
    ("archive", archive),
    ("archive tag and sibling", archive_sibling),
], first="work.org")
