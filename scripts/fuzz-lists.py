#!/usr/bin/env python3
"""Differential fuzzing of the meta and shift keys on list items against
Emacs (`org-indent-item`, `org-outdent-item` and their `-tree` forms,
`org-move-item-up`/`-down`, `org-cycle-list-bullet`, `org-previous-item`,
`org-next-item`, `C-c C-c` on an item, and `C-c ^`, `org-sort-list`).

Random lists: bullets of every kind, nesting, checkboxes, counters,
description tags, continuation lines, blank lines, a headline above with
a statistics cookie at times, text after the list. The cursor on an
item's line, any of the keys.

    scripts/fuzz-lists.py [--count 300] [--seed 1] [--out FILE]

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
KEYS = ["M-<left>", "M-<right>", "M-S-<left>", "M-S-<right>", "M-<up>", "M-<down>", "S-<left>", "S-<right>",
        "S-<up>", "S-<down>", "C-c C-c"] + ["C-c ^ " + kind for kind in "aAnNtTxX"]
WORDS = ["a", "word", "two words", "x", "é", "*bold*", "[[https://x.org][link]]", "B", "10", "-2.5", "<2026-03-04 Wed>",
         "[2026-02-01 Sun 10:00]", "0:01:30 ::", ""]


def item(rng, depth, number, bullets):
    bullet = bullets[depth]
    if bullet[0] in ".)":
        bullet = f"{number}{bullet}"
    parts = [bullet]
    if rng.random() < 0.08:
        parts.append(f"[@{rng.randint(1, 9)}]")
    if rng.random() < 0.3:
        parts.append(rng.choice(["[ ]", "[X]", "[-]"]))
    if rng.random() < 0.15:
        parts.append(rng.choice(WORDS[:-1]) + " ::")
    parts.append(rng.choice(WORDS))
    if rng.random() < 0.08:
        parts.append(rng.choice(["[/]", "[%]", "[1/2]"]))
    return " " * (bullets["base"] + depth * rng.choice([2, 2, 3])) + " ".join(part for part in parts if part).rstrip()


def document(rng):
    lines = []
    heading = rng.random() < 0.4
    if heading:
        lines.append("* H" + rng.choice(["", " [/]", " [%]"]))
        if rng.random() < 0.3:
            lines += [":PROPERTIES:", ":ORDERED: t", ":END:"]
    if rng.random() < 0.2:
        lines.append("text before")
    choices = ["-", "+", "*", ".", ")"]
    bullets = {"base": rng.choice([0, 0, 1, 2])}
    for depth in range(4):
        bullets[depth] = rng.choice(choices if depth or bullets["base"] else choices[:2] + choices[3:])
    depth, numbers = 0, [1] * 4
    for _ in range(rng.randint(1, 8)):
        depth = max(0, min(3, depth + rng.choice([-1, 0, 0, 1])))
        lines.append(item(rng, depth, numbers[depth], bullets))
        numbers[depth] += 1
        for d in range(depth + 1, 4):
            numbers[d] = 1
        if rng.random() < 0.15:
            lines.append(" " * (bullets["base"] + depth * 2 + 2) + "continued")
        if rng.random() < 0.1:
            lines.append("")
    if rng.random() < 0.2:
        lines += ["", "after"]
    text = "\n".join(lines)
    return text if rng.random() < 0.15 else text + "\n"


def case(rng):
    text = document(rng)
    starts = [0] + [index + 1 for index, c in enumerate(text) if c == "\n" and index + 1 < len(text)]
    rows = [start for start in starts if text[start:].lstrip(" ")[:1] in list("-+*0123456789")
            and not text[start:].startswith("* ")]
    start = rng.choice(rows or starts)
    line = text[start:].split("\n", 1)[0]
    point = start + rng.randint(0, len(line))
    return {"text": text, "point": point, "key": rng.choice(KEYS)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], preamble="(require 'org-timer)")
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-lists-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "lists"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_LISTS_CASES=out), capture_output=True, text=True,
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
