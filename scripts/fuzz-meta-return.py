#!/usr/bin/env python3
"""Differential fuzzing of M-RET, M-S-RET, C-RET and C-S-RET against Emacs.

Random documents from the indentation fuzzer with more headlines, lists
(ordered, nested, with checkboxes, counters, descriptions and blank lines)
and tables, the cursor at a random place, one of the keys. Emacs presses
the key; the test compares. A case Emacs never finishes (its list code
loops forever on a few broken lists) is recorded as such and left out.

    scripts/fuzz-meta-return.py [--count 300] [--seed 1] [--out FILE]

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
KEYS = ["M-RET", "M-RET", "M-S-<return>", "M-S-<return>", "C-<return>", "C-S-<return>"]
EXTRA = [
    "* TODO [#A] A title here :work:home:",
    "** DONE Done title :x:",
    "* Plain title",
    "*** TODO",
    "* A [0/2]",
    "** NEXT x",
    "",
    "",
    "- [ ] a box item",
    "- [X] checked item with text",
    "- [ ] parent [1/2]\n  - [X] one\n  - [ ] two",
    "  - nested item",
    "1) numbered",
    "1. one\n2. two\n3. three",
    "3. [@3] counted\n4. next",
    "- term :: definition\n- other :: more",
    "+ a\n\n+ b",
    "- a\n  text of a\n- b",
    "| a | bb | ccc |\n|---+----+-----|\n| 1 | 22 |",
    "| x |\n| y | z |",
    "Text with   trailing   ",
    "* Title   :tag:",
]


def list_document(rng):
    """Mostly lists and tables: bullets, numbers, counters, checkboxes,
    descriptions, nesting, blank lines and continuation lines."""
    lines = []
    if rng.random() < 0.7:
        lines.append(rng.choice(["* A", "** B :tag:", "* TODO C [0/2]", "* D [1/3]"]))
    for _ in range(rng.randint(1, 8)):
        kind = rng.random()
        indent = " " * rng.choice([0, 0, 2, 3, 4])
        if kind < 0.6:
            bullet = rng.choice(["-", "-", "+", "1.", "2)", "3.", "*"])
            if bullet == "*" and not indent:
                indent = "  "
            text = rng.choice(["one", "two words", "", "a longer item text", "term :: definition", "x"])
            box = rng.choice(["", "", "[ ] ", "[X] ", "[-] "])
            counter = "[@4] " if rng.random() < 0.1 else ""
            cookie = rng.choice(["", "", " [1/2]", " [%]"])
            lines.append(f"{indent}{bullet} {counter}{box}{text}{cookie}".rstrip(" ") if rng.random() < 0.8 else f"{indent}{bullet} ")
        elif kind < 0.72:
            lines.append(indent + rng.choice(["continued text", "more", "\tz"]))
        elif kind < 0.82:
            lines.append("")
        elif kind < 0.92:
            lines.append(indent + rng.choice(["| a | bb |", "|---+----|", "| c | d |", "| x |"]))
        else:
            lines.append(rng.choice(["* E", "** F", "text at column 0", ":DRAWER:\n- in drawer\n:END:"]))
    return "\n".join(lines) + rng.choice(["", "\n", "\n\n"])


def case(rng, generator):
    if rng.random() < 0.5:
        text = list_document(rng)
        return {"text": text, "point": rng.randint(0, len(text)), "key": rng.choice(KEYS)}
    document = generator.case()
    lines = document["text"].split("\n")
    for _ in range(rng.randint(0, 4)):
        lines.insert(rng.randint(0, len(lines)), rng.choice(EXTRA))
    text = "\n".join(lines)
    return {"text": text, "point": rng.randint(0, len(text)), "key": rng.choice(KEYS)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    generator = fuzz_indent.Generator(rng)
    cases = reference([case(rng, generator) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-meta-return-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "meta_return"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_META_RETURN_CASES=out), capture_output=True, text=True,
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
