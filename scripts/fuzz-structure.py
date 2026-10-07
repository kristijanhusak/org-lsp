#!/usr/bin/env python3
"""Differential fuzzing of promoting, demoting and moving subtrees against
Emacs: `M-<left>`, `M-<right>`, `M-S-<left>`, `M-S-<right>`, `M-<up>`,
`M-<down>`, with counts and `org-adapt-indentation`.

Random documents of headlines of random levels, with TODO keywords, tags,
planning lines, drawers, text and blank lines, the cursor on a headline.
Emacs presses the key; the test compares.

    scripts/fuzz-structure.py [--count 300] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from return_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
KEYS = ["M-<left>", "M-<right>", "M-S-<left>", "M-S-<right>", "M-<up>", "M-<down>",
        "C-u 2 M-<up>", "C-u 2 M-<down>", "C-u 3 M-<down>"]
BODY = [
    "text", "  indented text", "SCHEDULED: <2026-05-18 Mon>", "  DEADLINE: <2026-05-20 Wed>",
    ":PROPERTIES:\n:ID: x\n:END:", "  :PROPERTIES:\n  :A: 1\n  :END:", ":LOGBOOK:\n- Note\n:END:",
    "- item", "  - item two", "| a | b |", "#+begin_src sh\necho\n#+end_src", "", "",
    "[fn:1] a footnote", "   deep",
]


def document(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append(rng.choice(["Intro text", "#+TITLE: x", ""]))
    level = 1
    for _ in range(rng.randint(1, 7)):
        level = max(1, min(5, level + rng.choice([-2, -1, 0, 0, 1, 1])))
        keyword = rng.choice(["", "", "TODO ", "DONE "])
        title = rng.choice(["A", "Title words", "", "x y z"])
        tags = rng.choice(["", "", " :tag:", "    :a:b:", " " * 60 + ":long:"])
        lines.append(("*" * level + " " + keyword + title + tags).rstrip(" ") if rng.random() < 0.9 else "*" * level + " ")
        for _ in range(rng.randint(0, 3)):
            lines.append(rng.choice(BODY))
    return "\n".join(lines) + rng.choice(["", "\n", "\n\n"])


def case(rng):
    text = document(rng)
    starts = [0] + [i + 1 for i, c in enumerate(text) if c == "\n" and i + 1 < len(text)]
    heads = [s for s in starts if text[s] == "*"] or [0]
    start = rng.choice(heads)
    end = text.find("\n", start)
    end = len(text) if end < 0 else end
    point = rng.choice([start, start, min(start + 1, end), min(start + 3, end), end, rng.randint(start, end)])
    result = {"text": text, "point": point, "key": rng.choice(KEYS)}
    if rng.random() < 0.25:
        result["adapt"] = rng.choice(["t", "headline-data"])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-structure-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "structure"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_STRUCTURE_CASES=out), capture_output=True, text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:12000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
