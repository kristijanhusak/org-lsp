#!/usr/bin/env python3
"""Differential fuzzing of RET against Emacs.

Random documents from the indentation fuzzer, with the cursor at a random
place, and more headlines with keywords, priorities and tags, lists with
checkboxes and tables. Emacs presses RET; the test compares.

    scripts/fuzz-return.py [--count 300] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import importlib.util
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from return_reference import reference, save  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_indent", os.path.join(HERE, "fuzz-indent.py"))
fuzz_indent = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz_indent)

ROOT = os.path.dirname(HERE)
EXTRA = [
    "* TODO [#A] A title here :work:home:",
    "** DONE Done title :x:",
    "* Plain title",
    "*** TODO",
    "- [ ] a box item",
    "- [X] checked item with text",
    "  - nested item",
    "1) numbered",
    "| a | bb | ccc |\n|---+----+-----|\n| 1 | 22 |",
    "| x |\n| y | z |",
    "Text with   trailing   ",
    "* Title   :tag:",
]


def case(rng, generator):
    document = generator.case()
    lines = document["text"].split("\n")
    for _ in range(rng.randint(0, 3)):
        lines.insert(rng.randint(0, len(lines)), rng.choice(EXTRA))
    text = "\n".join(lines)
    return {"text": text, "point": rng.randint(0, len(text)), "adapt": document["adapt"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    generator = fuzz_indent.Generator(rng)
    cases = reference([case(rng, generator) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-return-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "newline"],
        cwd=ROOT, env=dict(os.environ, ORG_RETURN_CASES=out), capture_output=True, text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:8000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
