#!/usr/bin/env python3
"""Differential fuzzing of Org's property commands against Emacs:
`org-set-property`, `org-delete-property`, `org-delete-property-globally`,
`S-<right>`/`S-<left>` on a property line, `org-set-effort`,
`org-inc-effort`, `org-property-action` and `org-toggle-ordered-property`.

Random documents (property drawers, indented or not, allowed values in
`NAME_ALL` properties and `#+PROPERTY:` lines, comments before a drawer
of the file's own, planning lines), commands, answers and options.

    scripts/fuzz-properties.py [--count 300] [--seed 1] [--out FILE]

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
from properties_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
NAMES = ["Owner", "owner", "Color", "Effort", "ORDERED", "CUSTOM_ID", "Size", "x+", "Owner+", "LOGGING"]
VALUES = ["bob", "alice", "red", "green", "light blue", "0:30", "1:00", "2h", "t", "nil", "[ ]", "[X]", "", "7"]
ALLOWED = [
    "red green blue", "red \"light blue\" 1.50 007", "0:10 0:30 1:00 2:00", "small :ETC", "a", "[ ] [X]",
    "x\\ y z", "lone", "",
]
ANSWERS = ["Owner", "owner", "Color", "Effort", "Size", "New", "ORDERED", "CATEGORY", "TODO", "PRIORITY", "ITEM",
           "", "bad name", "red", "blue", "light blue", "  padded  ", "0:30", "1:00", "2h", "1d 3:00", "bad", "DONE",
           "A", "C", "x y", "007"]
COMMANDS = ["set", "set", "set", "delete", "delete-globally", "next", "next", "previous", "effort", "inc-effort",
            "action", "ordered"]


def drawer(rng, indent):
    lines = [indent + rng.choice([":PROPERTIES:", ":properties:"])]
    for _ in range(rng.randint(0, 4)):
        if rng.random() < 0.3:
            name, value = rng.choice(NAMES) + "_ALL", rng.choice(ALLOWED)
        else:
            name, value = rng.choice(NAMES), rng.choice(VALUES)
        gap = rng.choice([" ", "   ", "\t", " " * max(1, 10 - len(name))])
        lines.append(indent + f":{name}:" + (gap + value if value or rng.random() < 0.5 else ""))
    lines.append(indent + rng.choice([":END:", ":end:"]))
    return lines


def document(rng):
    lines = []
    if rng.random() < 0.3:
        lines.append("# a comment")
    if rng.random() < 0.3:
        lines += drawer(rng, "")
    if rng.random() < 0.4:
        lines.append(f"#+PROPERTY: {rng.choice(NAMES)}_ALL {rng.choice(ALLOWED)}")
    if rng.random() < 0.2:
        lines.append(f"#+PROPERTY: {rng.choice(NAMES)} {rng.choice(VALUES)}")
    if rng.random() < 0.15:
        lines.append("#+COLUMNS: %25ITEM %Size(Sz) %Effort{:}")
    if rng.random() < 0.2:
        lines.append("text before")
    for _ in range(rng.randint(1, 4)):
        level = rng.randint(1, 3)
        title = rng.choice(["Task", "TODO Thing", "DONE [#A] Done", "", "Title :tag:", "[#C] Prio"])
        lines.append("*" * level + " " + title)
        indent = rng.choice(["", "", " " * (level + 1)])
        if rng.random() < 0.3:
            lines.append(indent + "SCHEDULED: <2026-05-01 Fri>")
        if rng.random() < 0.6:
            lines += drawer(rng, indent)
        if rng.random() < 0.3:
            lines.append(indent + rng.choice(["body text", ":Owner: in body", ":PROPERTIES:", "- [ ] item"]))
    text = "\n".join(lines)
    return text + ("\n" if rng.random() < 0.85 else "")


def case(rng):
    text = document(rng)
    starts = [0] + [i + 1 for i, c in enumerate(text) if c == "\n" and i + 1 < len(text)]
    start = rng.choice(starts)
    end = text.find("\n", start)
    end = len(text) if end < 0 else end
    point = rng.randint(start, end)
    command = rng.choice(COMMANDS)
    result = {"text": text, "point": point, "command": command,
              "answers": [rng.choice(ANSWERS) for _ in range(rng.randint(0, 3))]}
    if command == "action":
        result["key"] = rng.choice("sdDsdx")
    options = {}
    if rng.random() < 0.2:
        options["org_global_properties"] = {rng.choice(NAMES) + "_ALL": rng.choice(ALLOWED)}
    if options:
        result["options"] = options
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-properties-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "properties"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_PROPERTIES_CASES=out), capture_output=True,
        text=True,
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
