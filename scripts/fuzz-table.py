#!/usr/bin/env python3
"""Differential fuzzing of Org's table commands against Emacs: random
tables (ragged rows, rules, empty fields, numbers, indentation, wide
characters, a `#+TBLFM:` line naming rows and columns) with text around
them, the cursor anywhere on a table line, and one of the commands.

    scripts/fuzz-table.py [--count 300] [--seed 1] [--out FILE]

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
from table_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
KEYS = ["TAB", "<backtab>", "M-<left>", "M-<right>", "M-<up>", "M-<down>", "M-S-<left>", "M-S-<right>",
        "M-S-<up>", "M-S-<down>", "C-u M-S-<down>", "C-c -", "C-u C-c -", "C-c RET", "fn:org-table-blank-field",
        "C-c C-c", "C-c ^ a", "C-c ^ A", "C-c ^ n", "C-c ^ N", "C-c ^ t", "C-c ^ T"]
WORDS = ["a", "bb", "ccc", "1", "22", "3.5", "-4", "", "", "x y", "Zeta", "été", "日本", "10:30", "2:05",
         "<2024-03-01 Fri>", "[2023-01-02]", "1h", "<l>", "<r5>", "foo bar baz", "[[https://x.org][link]]",
         "[[file:a.org]]", "*bold*", "\tx"]


def row(rng, columns):
    if rng.random() < 0.15:
        return "|" + "+".join("-" * rng.randint(1, 5) for _ in range(columns)) + "|"
    count = max(1, columns + rng.choice([0, 0, 0, -1, 1]))
    cells = [rng.choice([" ", "", "  "]) + rng.choice(WORDS) + rng.choice([" ", "", "   "]) for _ in range(count)]
    end = rng.choice(["|", "|", "|", "", "|  "])
    return "|" + "|".join(cells) + end


def table(rng):
    columns = rng.randint(1, 4)
    indent = rng.choice(["", "", "  ", "\t"])
    rows = [indent + row(rng, columns) for _ in range(rng.randint(1, 6))]
    if all(r.strip().startswith("|-") for r in rows):
        rows.append(indent + row(rng, columns))
    out = "\n".join(rows)
    if rng.random() < 0.4:
        terms = rng.sample(["$2=$1*2", "@3$1=5", "$3=vsum(@2..@3)", "@2$2=$1", "$1=remote(t,@1$1)", "@1=2"],
                           rng.randint(1, 3))
        out += "\n#+TBLFM: " + "::".join(terms)
    return out


def case(rng):
    before = rng.choice(["", "Text\n", "* Head\n", "Para\n\n"])
    after = rng.choice(["\n", "", "\nAfter\n", "\n\n* Next\n"])
    body = table(rng)
    if rng.random() < 0.15:
        body += rng.choice(["\n\n", "\nText between\n"]) + table(rng)
    text = before + body + after
    lines = [i for i in range(len(before), len(before) + len(body)) if text[i - 1] == "\n" or i == len(before)]
    # A place on a table line.
    starts = [i for i in lines if text[i:].lstrip(" \t").startswith("|")]
    start = rng.choice(starts)
    end = text.find("\n", start)
    end = len(text) if end < 0 else end
    at = rng.randint(start, end)
    key = rng.choice(KEYS)
    # `C-c C-c` on a timestamp changes the timestamp.
    line = text[start:end]
    for opening, closing in (("<", ">"), ("[", "]")):
        index = line.find(opening + "20")
        while index >= 0 and key == "C-c C-c":
            close = line.find(closing, index)
            if start + index <= at <= start + close + 1:
                key = "TAB"
            index = line.find(opening + "20", index + 1)
    return {"text": text[:at] + "¦" + text[at:], "key": key}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-table-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "table"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_TABLE_CASES=out), capture_output=True, text=True,
    )
    errors = sum(1 for case in cases if "error" in case["expected"])
    print(f"{args.count} cases ({errors} where Emacs fails); saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:15000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
