"""Zed: screenshots of tests/e2e/highlight (the sample, its end, the week's
agenda with a habit and a running clock), a file with #+STARTUP: indent,
and the diagnostics of a file, to look at."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import zxlib  # noqa: E402

ROOT = zxlib.ROOT
SAMPLE = os.path.join(ROOT, "tests", "e2e", "highlight")
files = {}
for directory, _, names in os.walk(SAMPLE):
    for name in names:
        if name.endswith(".org") or "journal" in directory:
            path = os.path.join(directory, name)
            files[os.path.relpath(path, SAMPLE)] = open(path, encoding="utf-8").read()
files["indent.org"] = "#+STARTUP: indent\n* Top\nSome text under top.\n** Child\nText of the child.\n"
files["lint.org"] = ("* Lint\ntext** heading\n[[file:nowhere.org]]\n#+begin_src python :foo 1\nprint(1)\n"
                     "#+end_src\n#+begin_quote\nopen\n")
options = json.load(open(os.path.join(SAMPLE, "options.json")))
lines = files["sample.org"].count("\n")
steps = [
    {"open": "sample.org", "show": ["screen"]},
    {"line": lines - 30, "show": ["screen"]},
    {"line": lines, "show": ["screen"]},
    {"command": "org.agenda.dispatch", "answers": ["a"], "show": ["screen"]},
    {"open": "indent.org", "show": ["screen"]},
    {"open": "lint.org", "show": ["screen"],
     "check": lambda s: [d.get("message") for d in (s.zed.log.diagnostics if hasattr(s.zed.log, "diagnostics") else [])]},
]
zxlib.run("zx5", files, options, steps)
