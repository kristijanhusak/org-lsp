#!/usr/bin/env python3
"""Compare document formatting with Emacs on random tables and tagged
headlines.

    scripts/fuzz-format.py [count] [seed]

Writes the cases that differ to tests/format/fuzz-failures.json.
"""

import json
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from format_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CELLS = [
    "", "a", "bb", "long cell text", "12", "-3.5", "1e5", "0x1F", "4#17", "nan", "Inf", "+inf", "12%",
    "3:45", ">3.5", "(2)", "<r>", "<c>", "<l>", "<l5>", "<8>", "*bold*", "=code=", "ünï", "😀", "漢字",
    "[[https://orgmode.org][Org]]", "[[file:x.org]]", "[[a][b c]]", "x y", "1/2", "a|b", "12.5e-3x",
]


def table(rng):
    indent = rng.choice(["", "", "  ", "\t"])
    columns = rng.randint(1, 4)
    lines = []
    for _ in range(rng.randint(1, 5)):
        if rng.random() < 0.2:
            lines.append(indent + rng.choice(["|-", "|---+--|", "|--", "|-+-+-|"]))
            continue
        cells = [rng.choice(CELLS).replace("|", "") for _ in range(rng.randint(0, columns))]
        line = indent + "|" + "|".join(rng.choice(["", " ", "  "]) + c + rng.choice(["", " "]) for c in cells)
        if rng.random() < 0.7:
            line += "|"
        lines.append(line)
    return lines


def headline(rng):
    stars = "*" * rng.randint(1, 3)
    keyword = rng.choice(["", "TODO ", "DONE "])
    title = rng.choice(["Title", "A longer headline title", "x", "漢字 title", "Very " * rng.randint(1, 12)])
    tags = ":" + ":".join(rng.sample(["work", "a", "urgent", "ü", "x_y"], rng.randint(1, 3))) + ":"
    spaces = " " * rng.randint(1, 50)
    return f"{stars} {keyword}{title}{spaces}{tags}" if rng.random() < 0.8 else f"{stars} {keyword}{title}"


def document(rng):
    parts = []
    for _ in range(rng.randint(1, 4)):
        kind = rng.random()
        if kind < 0.5:
            parts.extend(table(rng))
        elif kind < 0.8:
            parts.append(headline(rng))
        else:
            parts.append(rng.choice(["Some text", "", "- item", "#+TBLFM: $1=1"]))
    return "\n".join(parts) + "\n"


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else random.randrange(1 << 30)
    rng = random.Random(seed)
    cases = [{"text": document(rng), "tagsColumn": rng.choice([-77, -77, -40, 20, 0])} for _ in range(count)]
    reference(cases)
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, "cases.json")
        save(path, cases)
        result = subprocess.run(
            ["cargo", "test", "-q", "-p", "org-core", "--test", "format"],
            cwd=ROOT, env={**os.environ, "ORG_FORMAT_CASES": path}, capture_output=True, text=True,
        )
    print(f"seed {seed}: {count} documents")
    if result.returncode != 0:
        print(result.stdout[-6000:])
        sys.exit(1)
    print("all match")


if __name__ == "__main__":
    main()
