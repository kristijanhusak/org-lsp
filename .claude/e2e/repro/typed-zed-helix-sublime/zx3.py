"""Zed, TESTING.org: clocking, capture, Babel, export, timers, notes,
editing helpers, attachments; each step typed."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

A = """* TODO Write report
:PROPERTIES:
:EFFORT:   0:30
:ID: att-1
:END:
:LOGBOOK:
CLOCK: [2026-05-18 Mon 09:00]--[2026-05-18 Mon 09:40] =>  0:40
:END:
[[attachment:note.txt]]
* TODO Plan
* Code
#+name: three
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
src_emacs-lisp{(* 6 7)}
#+CALL: three()
* Clocks
#+BEGIN: clocktable :scope file :maxlevel 2 :block today
#+END:
"""
opts = {
    "org_capture_templates": [
        {"key": "t", "description": "Task", "target": ["file", "$DIR/inbox.org"], "template": "* TODO %?"},
        {"key": "n", "description": "Named", "target": ["file", "$DIR/inbox.org"], "template": "* %^{Name}"},
        {"key": "d", "description": "Diary", "target": ["file+olp+datetree", "$DIR/diary.org"], "template": "* %?"},
        {"key": "a", "description": "Link", "target": ["file", "$DIR/inbox.org"], "template": "* %a"},
        {"key": "s", "description": "Snippet", "target": ["file", "$DIR/inbox.org"], "template": "* %[snippet.txt]"},
        {"key": "u", "description": "Upcase", "target": ["file", "$DIR/inbox.org"], "template": "* %(upcase \"abc\")"},
    ],
    "org_log_into_drawer": True,
}

steps = [
    {"open": "a.org"},
    {"title": "clock in, effort passed", "at": "* TODO Write report", "command": "org.clock.in", "show": ["messages"]},
    {"title": "clock in another", "at": "* TODO Plan", "command": "org.clock.in", "show": ["messages"]},
    {"title": "clock out", "command": "org.clock.out", "show": ["messages"]},
    {"title": "clock in last", "command": "org.clock.in.last", "show": ["messages"]},
    {"title": "go to the clocked entry", "at": "* Code", "command": "org.clock.goto", "show": ["cursor"]},
    {"title": "cancel", "command": "org.clock.cancel", "show": ["messages"]},
    {"title": "clock display", "command": "org.clock.display", "show": ["messages"]},
    {"title": "clock table update", "at": "#+BEGIN: clocktable", "command": "org.dblock.update", "show": ["buffer"]},
    {"title": "babel yes", "at": "(+ 1 2)", "command": "org.babel.execute", "answers": ["y"], "show": ["messages"]},
    {"title": "inline src", "at": "src_emacs-lisp", "column": 3, "command": "org.babel.execute", "answers": ["y"]},
    {"title": "#+CALL", "at": "#+CALL:", "command": "org.babel.execute", "answers": ["y"], "show": ["buffer"]},
    {"title": "remove result", "at": "(+ 1 2)", "command": "org.babel.remove_result", "show": ["buffer"]},
    {"title": "next block", "at": "* Code", "command": "org.babel.next", "show": ["cursor"]},
    {"title": "tangle", "command": "org.babel.tangle", "show": ["messages"]},
    {"title": "export menu, html file", "command": "org.export.dispatch", "answers": ["h", "h"], "show": ["messages"],
     "check": lambda s: os.path.exists(os.path.join(s.directory, "a.html"))},
    {"title": "timer start", "at": "* TODO Plan", "command": "org.timer.start", "show": ["messages"]},
    {"title": "timer item", "below": True, "command": "org.timer.item", "show": ["cursor"]},
    {"title": "timer stop", "command": "org.timer.stop", "show": ["messages"]},
    {"title": "note", "at": "* TODO Plan", "command": "org.add.note", "answers": ["one line note"], "show": ["buffer"]},
    {"title": "structure template", "at": "* Clocks", "below": True, "command": "org.structure.template",
     "answers": ["s"], "show": ["cursor"]},
    {"title": "attachment link", "at": "[[attachment:", "column": 2, "command": "org.open", "show": ["cursor", "messages"]},
    {"open": "a.org"},
    {"title": "capture menu, finish", "command": "org.capture", "answers": ["t"], "show": ["cursor"]},
    {"type": "Captured"},
    {"command": "org.capture.finish", "show": ["file:inbox.org", "messages"]},
    {"title": "%^{Name}, abort", "command": "org.capture", "answers": ["n", "Ann"], "show": ["cursor"]},
    {"command": "org.capture.abort", "show": ["messages"]},
    {"title": "datetree", "command": "org.capture", "answers": ["d"]},
    {"type": "Diary"},
    {"command": "org.capture.finish", "show": ["file:diary.org"]},
    {"title": "%a", "at": "* TODO Plan", "command": "org.capture", "answers": ["a"]},
    {"command": "org.capture.finish"},
    {"title": "%[file] %()", "command": "org.capture", "answers": ["s"]},
    {"command": "org.capture.finish"},
    {"command": "org.capture", "answers": ["u"]},
    {"command": "org.capture.finish", "show": ["file:inbox.org"]},
]

files = {"a.org": A, "inbox.org": "", "snippet.txt": "snip text\n", "data/at/t-1/note.txt": "attached\n"}
zxlib.run("zx3", files, opts, steps)
