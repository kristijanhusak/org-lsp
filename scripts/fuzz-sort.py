#!/usr/bin/env python3
"""Differential fuzzing of sorting entries against Emacs
(`org-sort-entries`).

Random documents: headlines of a few levels whose titles have keywords,
priority cookies, numbers, emphasis, links and tags; bodies with
timestamps, planning lines, property drawers and clock lines; blank lines
and no final newline at times. The cursor anywhere, any sorting type.

    scripts/fuzz-sort.py [--count 300] [--seed 1] [--out FILE]

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
from sort_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
WORDS = ["apple", "Banana", "cherry", "10", "9", "-3", "2.5", "1e1", "Émile", "zed", "Zulu", "beta", "alpha",
         "*bold*", "/it/", "~code~", "=verb=", "[[https://x.org][Link]]", "[[file:a.org]]", "<https://y.org>",
         "_under_", "+strike+", "same", "[#B]", "x*y", "COMMENT"]
KEYWORDS = ["TODO", "DONE", "NEXT", "WAIT", "CANCELLED"]


def date(rng):
    return f"2026-{rng.randint(1, 12):02d}-{rng.randint(1, 28):02d}"


def stamp(rng, active=True):
    time = f" {rng.randint(0, 23)}:{rng.choice(['00', '30', '45'])}" if rng.random() < 0.5 else ""
    text = f"{date(rng)} Mon{time}"
    return f"<{text}>" if active else f"[{text}]"


def heading(rng, level):
    parts = []
    if rng.random() < 0.4:
        parts.append(rng.choice(KEYWORDS))
    if rng.random() < 0.3:
        parts.append(rng.choice(["[#A]", "[#B]", "[#C]", "[#1]", "[#15]"]))
    parts += [rng.choice(WORDS) for _ in range(rng.randint(0, 3))]
    if rng.random() < 0.2:
        parts.append(stamp(rng, rng.random() < 0.5))
    line = "*" * level + " " + " ".join(parts)
    if rng.random() < 0.2:
        line += " :" + rng.choice(["tag", "a:b", "work"]) + ":"
    return line


def body(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append(rng.choice(["SCHEDULED: ", "DEADLINE: "]) + stamp(rng))
    if rng.random() < 0.2:
        lines.append(f"SCHEDULED: {stamp(rng)} DEADLINE: {stamp(rng)}")
    if rng.random() < 0.3:
        lines += [":PROPERTIES:", f":WHO: {rng.choice(['alice', 'Bob', 'carol', ''])}", ":END:"]
    if rng.random() < 0.25:
        start, end = rng.randint(8, 12), rng.randint(13, 18)
        day = date(rng)
        lines += [":LOGBOOK:", f"CLOCK: [{day} Mon {start}:00]--[{day} Mon {end}:00] => {end - start:2d}:00", ":END:"]
    if rng.random() < 0.3:
        lines.append(rng.choice(["  ", ""]) + stamp(rng, False))
    if rng.random() < 0.3:
        lines.append(rng.choice(["text", "more text " + stamp(rng), "- item"]))
    for _ in range(rng.choice([0, 0, 0, 1, 2])):
        lines.append("")
    return lines


def document(rng):
    lines = []
    if rng.random() < 0.2:
        lines.append("#+TODO: TODO NEXT WAIT | DONE CANCELLED")
    if rng.random() < 0.3:
        lines.append("intro")
    level = 1
    for index in range(rng.randint(2, 10)):
        level = max(1, min(4, level + rng.choice([-1, 0, 0, 0, 1] if index else [0])))
        lines.append(heading(rng, level))
        lines += body(rng)
    text = "\n".join(lines)
    return text if rng.random() < 0.2 else text + "\n"


def case(rng):
    text = document(rng)
    kind = rng.choice("aAnNpPoOtTsSdDcCkKrR")
    # Mostly on a headline (whose children are sorted), or before the
    # first one.
    starts = [0] + [index + 1 for index, c in enumerate(text) if c == "\n" and text[index + 1:index + 2] == "*"]
    point = rng.choice(starts) if rng.random() < 0.7 else rng.randint(0, len(text))
    out = {"text": text, "point": point, "type": kind}
    if kind in "rR":
        out["property"] = "WHO"
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-sort-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "sort"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_SORT_CASES=out), capture_output=True, text=True,
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
