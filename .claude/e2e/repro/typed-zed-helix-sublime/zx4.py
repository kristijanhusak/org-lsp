"""Zed, TESTING.org: org-roam, org-journal, the agenda's keys pressed many
times, export, completion, citations; each step typed."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

YES = {"choice": "Yes"}
NOTE = """:PROPERTIES:
:ID: node-a
:END:
#+title: Alpha note
Links to [[id:node-b][Beta note]].
"""
BETA = """:PROPERTIES:
:ID: node-b
:END:
#+title: Beta note
"""
W = """* TODO Write report
SCHEDULED: <2026-05-18 Mon>
* Cite
[cite:@knuth]
#+bibliography: refs.bib
"""
BIB = "@book{knuth, title={The Art}, author={Knuth, Donald}, year={1968}}\n"
opts = {
    "org_roam_enabled": True,
    "org_roam_directory": "$DIR/roam",
    "org_journal_enabled": True,
    "org_journal_dir": "$DIR/journal",
}

steps = [
    {"open": "work.org"},
    {"title": "agenda, f five times", "command": "org.agenda.dispatch", "answers": ["a"]},
    {"command": "org.agenda.later", "count": 5, "show": ["cursor"],
     "check": lambda s: (s.zed.log.documents.get(s.agenda_uri()) or "").split("\n")[:2]},
    {"focus": "work.org", "title": "roam find", "command": "org.roam.node.find", "answers": ["Beta note"], "show": ["cursor"]},
    {"title": "roam buffer", "focus": "roam/b.org", "command": "org.roam.buffer.toggle", "show": ["messages"],
     "check": lambda s: s.zed.log.shows[-1:]},
    {"focus": "roam/b.org", "title": "dailies today", "command": "org.roam.dailies.goto.today", "show": ["buffer"]},
    {"focus": "work.org", "title": "journal new entry", "command": "org.journal.new.entry", "show": ["buffer"]},
    {"focus": "work.org", "title": "export html file", "at": "* Cite", "command": "org.export.dispatch",
     "answers": ["h", "h"], "show": ["messages"],
     "check": lambda s: os.path.exists(os.path.join(s.directory, "work.html"))},
    {"title": "follow a citation", "at": "[cite:@knuth]", "column": 8, "command": "org.open", "show": ["cursor"]},
]
files = {"work.org": W, "refs.bib": BIB, "roam/a.org": NOTE, "roam/b.org": BETA}
zxlib.run("zx4", files, opts, steps)
