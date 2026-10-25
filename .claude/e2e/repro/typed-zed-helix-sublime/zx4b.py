"""Zed: org-journal's new entry, previous entry, and a search."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

YES = {"choice": "Yes"}
opts = {"org_journal_enabled": True, "org_journal_dir": "$DIR/journal",
        "org_journal_file_format": "%Y%m%d.org"}
steps = [
    {"open": "work.org"},
    {"title": "journal new entry", "command": "org.journal.new.entry", "answers": ["y"], "show": ["buffer", "cursor"]},
    {"type": "Wrote this"},
    {"title": "previous entry", "command": "org.journal.previous.entry", "show": ["cursor", "messages"]},
    {"title": "search", "command": "org.journal.search", "answers": ["Wrote", "2026-05-01", "2026-05-31"],
     "show": ["buffer"]},
]
zxlib.run("zx4b", {"work.org": "* A\n", "journal/20260517.org": "* Sunday, 05/17/26\n** TODO carried\n"}, opts, steps)
