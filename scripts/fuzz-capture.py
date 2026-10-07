#!/usr/bin/env python3
"""Compare the server's capture with Emacs's on random files and templates.

    scripts/fuzz-capture.py [seed] [count]

Writes the cases with Emacs's results to a temporary file and runs the
conformance test on it. Requires Emacs and faketime.
"""

import json
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_reference import reference  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TITLES = ["Tasks", "Inbox", "Ideas", "Work", "Home"]


def document(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append(rng.choice(["#+TITLE: Notes", "Some text before.", ""]))
    level = 1
    for _ in range(rng.randint(0, 6)):
        level = max(1, min(level + rng.choice([-1, 0, 0, 1]), 3))
        head = "*" * level + " " + rng.choice(["", "TODO ", "DONE "]) + rng.choice(TITLES)
        if rng.random() < 0.2:
            head += " :tag:"
        lines.append(head)
        for _ in range(rng.choice([0, 0, 1, 2])):
            lines.append(rng.choice(["", "", "body text", "SCHEDULED: <2026-05-18 Mon>", ":PROPERTIES:\n:ID: x\n:END:", "- item"]))
    text = "\n".join(lines)
    return text + rng.choice(["\n", "\n", "", "\n\n", "\n\n\n"])


TEMPLATES = [
    "* TODO %?",
    "* %?\n%U",
    "* TODO %? :tag:\n  %t",
    "** %?\n*** sub",
    "\n\n* %?\n\n",
    "* %^{Title} %?\n%^t",
    "Note: %?",
    "%? plain",
    "* %?\n- item\n\n",
]


def case(rng):
    text = document(rng)
    kind = rng.choice(["entry", "entry", "plain"])
    template = rng.choice(TEMPLATES)
    if kind == "entry" and not any(line.startswith("*") for line in template.split("\n")):
        template = "* " + template
    target = rng.choice([
        ["file", "notes.org"],
        ["file+headline", "notes.org", rng.choice(TITLES)],
        ["file+olp", "notes.org", rng.choice(TITLES)],
    ])
    spec = {"type": kind, "target": target, "template": template}
    if rng.random() < 0.4:
        spec["prepend"] = True
    for key in ["empty_lines", "empty_lines_before", "empty_lines_after"]:
        if rng.random() < 0.2:
            spec[key] = rng.randint(0, 2)
    out = {"clock": "2026-05-18 10:30", "files": {"notes.org": text}, "template": spec,
           "typed": rng.choice(["x", "Call Bob", "", "line\nmore"])}
    if "%^" in template:
        out["answers"] = [rng.choice(["Title", ""]), rng.choice(["fri", "+2d", "2026-06-01 10:00"])]
    if rng.random() < 0.35:
        out["options"] = {"org_adapt_indentation": rng.choice([True, "headline-data", False])}
    return out


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    cases = reference([case(rng) for _ in range(count)])
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False)
    env = dict(os.environ, ORG_CAPTURE_CASES=handle.name)
    result = subprocess.run(["cargo", "test", "-q", "-p", "org-lsp", "--test", "capture"], cwd=ROOT, env=env)
    print(f"seed {seed}: {count} cases in {handle.name}")
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
