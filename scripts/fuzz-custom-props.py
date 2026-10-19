#!/usr/bin/env python3
"""Differential fuzzing of the lines `org-toggle-custom-properties-visibility`
hides (`org-custom-properties`) against Emacs.

Random documents: headlines with planning lines and property drawers
whose properties are named in other cases of letters, indented, without
values, with `+`; drawers broken by other lines, or not where Org looks
for them; lines like properties in the text and in other drawers; a drawer
before the first headline after comments; no final newline at times.

    scripts/fuzz-custom-props.py [--count 300] [--seed 1] [--out FILE]

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
from custom_properties_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
NAMES = ["ID", "id", "CATEGORY", "Custom_ID", "X", "a.b", "WHO", "IDX"]


def property_line(rng):
    name = rng.choice(NAMES) + rng.choice(["", "", "", "+"])
    value = rng.choice(["", " 1", " some value", "\tx", "  ", " :ID: y"])
    return rng.choice(["", "", "  ", "\t"]) + f":{name}:{value}" + rng.choice(["", "", " "])


def drawer(rng):
    lines = [rng.choice([":PROPERTIES:", ":properties:", "  :PROPERTIES:", ":PROPERTIES: "])]
    for _ in range(rng.randint(0, 4)):
        lines.append(property_line(rng))
    if rng.random() < 0.1:
        lines.append(rng.choice(["not a property", "", ":ID:x"]))
    lines.append(rng.choice([":END:", ":end:", "  :END:"]))
    return lines


def body(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append(rng.choice(["SCHEDULED: <2026-10-05 Mon>", "DEADLINE: <2026-10-09 Fri>", "CLOSED: [2026-10-01 Thu]"]))
    if rng.random() < 0.6:
        lines += drawer(rng)
    if rng.random() < 0.2:
        lines += [":LOGBOOK:", property_line(rng), ":END:"]
    for _ in range(rng.choice([0, 0, 1, 2])):
        lines.append(rng.choice(["text", property_line(rng), "- item", "", "*bold* text"]))
    return lines


def document(rng):
    lines = []
    if rng.random() < 0.3:
        lines += rng.choice([["# comment"], ["#", "# more"], ["#+TITLE: x"], []])
        if rng.random() < 0.6:
            lines += drawer(rng)
    if rng.random() < 0.2:
        lines.append(property_line(rng))
    for _ in range(rng.randint(0, 6)):
        lines.append("*" * rng.randint(1, 3) + rng.choice([" A", " TODO B", " ", "C"]))
        lines += body(rng)
    text = "\n".join(lines)
    return text if rng.random() < 0.2 else text + "\n"


def case(rng):
    names = rng.sample(["ID", "CATEGORY", "x", "Who", "a.b"], rng.randint(0, 3))
    return {"text": document(rng), "names": names}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-custom-properties-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "custom_properties"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_CUSTOM_PROPERTIES_CASES=out),
        capture_output=True, text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("failures")
        print(report[max(0, start - 20):].rstrip()[:12000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
