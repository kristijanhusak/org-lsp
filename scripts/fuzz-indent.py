#!/usr/bin/env python3
"""Differential fuzzing of indentation against Emacs.

Generates random documents of headlines, planning lines, drawers, lists,
blocks, tables, footnotes and text at random indentation, has Emacs indent
every line, a new line after every line, and the whole document, under each
`org-adapt-indentation`, and runs the test over the same cases. The cases
stay in the output file.

    scripts/fuzz-indent.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from indent_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

TEXT = ["text", "more text", "*bold* start", "a longer line of text", "#+TITLE: x", "# comment", "| a | b |",
        "|---+---|", ": fixed", "-----", "CLOCK: [2026-10-05 Mon 09:00]--[2026-10-05 Mon 10:00] =>  1:00",
        "#+NAME: thing", "%%(diary-float t 4 2)", "[fn:1] note", "x *", "\\begin{equation}", "\\end{equation}"]
INDENTS = ["", "", "", " ", "  ", "   ", "    ", "      ", "\t"]
BLANKS = ["", "", "", " ", "  "]
ELEMENTS = [
    "SCHEDULED: <2026-10-05 Mon>",
    "DEADLINE: <2026-10-09 Fri> SCHEDULED: <2026-10-05 Mon>",
    ":PROPERTIES:\n:ID: 1\n:Effort:   1:00\n:END:",
    ":PROPERTIES:\n:END:",
    ":LOGBOOK:\n- note\nCLOCK: [2026-10-05 Mon 09:00]--[2026-10-05 Mon 10:00] =>  1:00\n:END:",
    ":NOTES:\ninside\n:END:",
    "#+begin_src sh\necho\n  nested\n#+end_src",
    "#+begin_src emacs-lisp -i\n(message)\n#+end_src",
    "#+begin_example\n  example\nmore\n#+end_example",
    "#+begin_quote\nquoted\n\nagain\n#+end_quote",
    "#+begin_verse\n  verse\nlines\n#+end_verse",
    "#+begin_export html\n<b>x</b>\n#+end_export",
    "#+begin_comment\ncommented\n#+end_comment",
    "- one\n  two\n\n- three",
    "- [ ] box\n  - nested\n    body\n- [X] done",
    "1. first\n2. second\n   continued",
    "- term :: description\n- other :: x",
    "| a | b |\n|---+---|\n| 1 | 2 |",
    "#+NAME: tbl\n| x |",
    "[fn:2] A footnote\ncontinued\n\nafter one blank",
    "\\begin{equation}\nx\n\\end{equation}",
    "#+BEGIN: clocktable\n| x |\n#+END:",
]


class Generator:
    def __init__(self, rng):
        self.rng = rng

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def blanks(self):
        return [self.pick(BLANKS) for _ in range(self.pick([0, 0, 0, 1, 1, 2, 3]))]

    def body(self):
        lines = []
        indent = self.pick(INDENTS)
        if self.chance(0.6):
            lines.append(indent + self.pick(ELEMENTS[:6]).replace("\n", "\n" + indent))
        for _ in range(self.pick([0, 1, 1, 2, 3, 4])):
            indent = self.pick(INDENTS) if self.chance(0.5) else indent
            if self.chance(0.4):
                lines.append(indent + self.pick(ELEMENTS).replace("\n", "\n" + indent))
            else:
                lines.append(indent + self.pick(TEXT))
            if self.chance(0.3):
                lines.extend(self.blanks())
        return lines

    def case(self):
        lines = []
        if self.chance(0.3):
            lines.extend(self.body())
            lines.extend(self.blanks())
        level = 1
        for _ in range(self.rng.randint(1, 5)):
            level = max(1, min(4, level + self.pick([-2, -1, 0, 0, 1, 1])))
            lines.append("*" * level + " " + self.pick(["a", "TODO b", "c :tag:", "d"]))
            lines.extend(self.blanks() if self.chance(0.2) else [])
            lines.extend(self.body())
            lines.extend(self.blanks())
        text = "\n".join(lines)
        ending = self.pick(["\n", "\n", "\n", "", "\n\n"])
        return {"text": text + ending, "adapt": self.pick(["nil", "t", "headline-data"])}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    generator = Generator(random.Random(args.seed))
    cases = reference([generator.case() for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-indent-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "indent"],
        cwd=ROOT,
        env=dict(os.environ, ORG_INDENT_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" differences:")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
