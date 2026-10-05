#!/usr/bin/env python3
"""Differential fuzzing of subtree folding against Emacs.

Generates random documents of headlines, text, drawers, blocks, lists and
blank lines, has Emacs fold every subtree, and runs the test over the same
cases. The cases stay in the output file.

    scripts/fuzz-fold.py [--count 300] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fold_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

TEXT = ["text", "more text", "*bold* start", "  indented", "- item", "  - nested item", "1. first", "#+TITLE: x",
        "# comment", "| a | b |", ": fixed", "SCHEDULED: <2026-05-18 Mon>", "*not a headline", "x *"]
BLANKS = ["", "", "", " ", "\t", "   "]
ELEMENTS = [
    ":PROPERTIES:\n:ID: 1\n:END:",
    ":LOGBOOK:\n- note\n:END:",
    "#+begin_src sh\necho\n\n#+end_src",
    "#+begin_quote\nquoted\n#+end_quote",
    "- one\n  two\n\n- three",
    "\\begin{equation}\nx\n\\end{equation}",
]


class Generator:
    def __init__(self, rng):
        self.rng = rng

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def blanks(self):
        return [self.pick(BLANKS) for _ in range(self.pick([0, 0, 1, 1, 2, 2, 3, 4]))]

    def case(self):
        lines = []
        if self.chance(0.3):
            lines.append(self.pick(TEXT))
            lines.extend(self.blanks())
        level = 1
        for _ in range(self.rng.randint(1, 7)):
            level = max(1, min(4, level + self.pick([-2, -1, 0, 0, 1, 1])))
            lines.append("*" * level + " " + self.pick(["a", "TODO b", "c :tag:", "d", ""]).rstrip()
                         if self.chance(0.95) else "*" * level)
            for _ in range(self.pick([0, 0, 1, 1, 2, 3])):
                if self.chance(0.25):
                    lines.extend(self.pick(ELEMENTS).split("\n"))
                else:
                    lines.append(self.pick(TEXT))
                if self.chance(0.3):
                    lines.extend(self.blanks())
            lines.extend(self.blanks())
        text = "\n".join(lines)
        ending = self.pick(["\n", "\n", "\n", "", "\n\n", "\n \n"])
        case = {"text": text + ending}
        if self.chance(0.4):
            case["separatorLines"] = self.pick([-3, -2, -1, 0, 1, 1, 3, 4])
        return case


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    generator = Generator(random.Random(args.seed))
    cases = reference([generator.case() for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-fold-fuzz-"), "cases.json")
    save(out, cases)

    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "folding"],
        cwd=ROOT,
        env=dict(os.environ, ORG_FOLD_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" folds differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
