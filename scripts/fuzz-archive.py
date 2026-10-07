#!/usr/bin/env python3
"""Compare the server's archiving with Emacs's on random documents.

    scripts/fuzz-archive.py [seed] [count]

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
from archive_reference import reference  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORDS = ["Inbox", "Work", "Home", "Read", "Call", "Archive", "Notes", "[[x][Link]]", "Task [1/2]", "Sum [50%]", "2026"]
LOCATIONS = [
    "%s_archive::",
    "%s_archive::* Archived Tasks",
    "::* Archive",
    "::",
    "::** Archive",
    "%s_archive::datetree/",
    "::datetree/",
    "%s_archive::datetree/* Done",
    "old.org::* From %s",
]


def document(rng, headings=8):
    lines = []
    if rng.random() < 0.2:
        lines.append(rng.choice(["#+TITLE: Doc", "#+FILETAGS: :f:", "#+CATEGORY: cat", "#+ARCHIVE: kw.org::"]))
    if rng.random() < 0.1:
        lines.append("Intro text")
    level = 1
    for _ in range(rng.randint(1, headings)):
        level = max(1, min(level + rng.choice([-2, -1, 0, 0, 1, 1, 2]), 4))
        head = "*" * level + " "
        if rng.random() < 0.4:
            head += rng.choice(["TODO ", "DONE ", "TODO ", "NEXT "])
        if rng.random() < 0.1:
            head += "[#B] "
        head += rng.choice(WORDS)
        if rng.random() < 0.3:
            head += " " * rng.randint(1, 20) + rng.choice([":tag:", ":a:b:", ":ARCHIVE:", ":x:ARCHIVE:"])
        lines.append(head)
        for _ in range(rng.choice([0, 0, 1, 2, 3])):
            lines.append(rng.choice([
                "Some text.",
                "",
                "  - [ ] item",
                "- [X] done item",
                "SCHEDULED: <2026-05-18 Mon>",
                "CLOSED: [2026-05-10 Sun 09:00]",
                ":PROPERTIES:\n:ID: id-1\n:END:",
                ":PROPERTIES:\n:ARCHIVE: ::* Here\n:END:",
                ":PROPERTIES:\n:CATEGORY: c2\n:END:",
                "   indented",
            ]))
    text = "\n".join(lines)
    return text + rng.choice(["\n", "\n", "", "\n\n"])


def case(rng):
    files = {"a.org": document(rng)}
    command = rng.choice(["subtree", "subtree", "subtree", "sibling", "sibling", "tag", "done", "tagDone"])
    if rng.random() < 0.3:
        files["a.org_archive"] = document(rng, 4) if rng.random() < 0.6 else rng.choice(
            ["* 2026\n** 2026-05 May\n*** 2026-05-09 Saturday\n", "* Archived Tasks\n** Old\n", "", "Text"]
        )
    text = files["a.org"]
    starts = [0] + [i + 1 for i, c in enumerate(text) if c == "\n" and i + 1 < len(text)]
    at = min(rng.choice(starts) + rng.randint(0, 3), len(text))
    files["a.org"] = text[:at] + "¦" + text[at:]
    options = {"org_todo_keywords": ["TODO", "NEXT", "|", "DONE"]}
    if rng.random() < 0.6:
        options["org_archive_location"] = rng.choice(LOCATIONS)
    if rng.random() < 0.25:
        options["org_archive_reversed_order"] = True
    if rng.random() < 0.2:
        options["org_archive_mark_done"] = rng.choice([True, "DONE"])
    if rng.random() < 0.2:
        options["org_archive_subtree_add_inherited_tags"] = rng.choice([True, False])
    if rng.random() < 0.15:
        options["org_archive_save_context_info"] = rng.sample(["time", "file", "olpath", "category", "todo", "itags", "ltags", "olid"], 3)
    if rng.random() < 0.2:
        options["org_tags_column"] = rng.choice([-40, 20, 0])
    if rng.random() < 0.25:
        options["org_adapt_indentation"] = rng.choice([True, "headline-data"])
    out = {"files": files, "command": command, "options": options}
    if command in ("done", "tagDone"):
        out["answers"] = [rng.random() < 0.7 for _ in range(4)]
    if rng.random() < 0.2:
        out["clock"] = "2027-01-03 08:15"
    return out


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    cases = reference([case(rng) for _ in range(count)])
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False)
    env = dict(os.environ, ORG_ARCHIVE_CASES=handle.name)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "archive"], cwd=ROOT, env=env
    )
    print(f"seed {seed}: {count} cases in {handle.name}")
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
