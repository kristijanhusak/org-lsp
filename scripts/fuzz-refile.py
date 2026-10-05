#!/usr/bin/env python3
"""Compare the server's refiling with Emacs's on random documents.

    scripts/fuzz-refile.py [seed] [count]

Writes the cases with Emacs's results to a temporary file and runs the
conformance test on it. Requires Emacs.
"""

import json
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from refile_reference import reference  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORDS = ["Inbox", "Work", "Home", "Read", "Call", "Plan", "Notes", "Misc", "a/b", "[[x][Link]]", "Task [1/2]"]


def document(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append("#+TITLE: Doc")
    level = 1
    for _ in range(rng.randint(1, 8)):
        level = max(1, min(level + rng.choice([-2, -1, 0, 0, 1, 1, 2]), 4))
        head = "*" * level + " "
        if rng.random() < 0.3:
            head += rng.choice(["TODO ", "DONE "])
        if rng.random() < 0.15:
            head += "[#A] "
        head += rng.choice(WORDS)
        if rng.random() < 0.3:
            head += " " * rng.randint(1, 30) + rng.choice([":tag:", ":a:b:", ":proj:"])
        lines.append(head)
        for _ in range(rng.choice([0, 0, 1, 2])):
            lines.append(rng.choice(["Some text.", "", "  - item", "SCHEDULED: <2026-05-18 Mon>"]))
    text = "\n".join(lines)
    return text + rng.choice(["\n", "\n", "", "\n\n"])


def case(rng):
    names = ["a.org"] if rng.random() < 0.6 else ["a.org", "b.org"]
    files = {name: document(rng) for name in names}
    # The cursor on some line of the first file.
    text = files["a.org"]
    starts = [0] + [i + 1 for i, c in enumerate(text) if c == "\n" and i + 1 < len(text)]
    at = rng.choice(starts) + rng.randint(0, 2)
    at = min(at, len(text))
    files["a.org"] = text[:at] + "¦" + text[at:]
    options = {}
    select = rng.choice([{"level": 1}, {"level": 2}, {"maxLevel": 2}, {"maxLevel": 3}, {"all": True}, {"tag": "proj"}, {"todo": "TODO"}])
    target = dict(select)
    if len(names) > 1 and rng.random() < 0.7:
        target["files"] = "agenda"
    if rng.random() < 0.8:
        options["refileTargets"] = [target]
    if rng.random() < 0.4:
        options["refileUseOutlinePath"] = rng.choice([True, "file", "title"])
    if rng.random() < 0.3:
        options["reverseNoteOrder"] = True
    if rng.random() < 0.2:
        options["tagsColumn"] = rng.choice([-40, 20, 0])
    out = {"files": files, "answerIndex": rng.randint(0, 12)}
    if options:
        out["options"] = options
    return out


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    cases = reference([case(rng) for _ in range(count)])
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False)
    env = dict(os.environ, ORG_REFILE_CASES=handle.name)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "refile"], cwd=ROOT, env=env
    )
    print(f"seed {seed}: {count} cases in {handle.name}")
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
