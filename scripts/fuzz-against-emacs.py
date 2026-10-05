#!/usr/bin/env python3
"""Differential fuzzing of org-syntax against Emacs.

Generates random Org documents from syntax fragments, has Emacs dump their
org-element trees, and runs the conformance test over them. Any document the
two parsers disagree on is left in the output directory for inspection.

    scripts/fuzz-against-emacs.py [--count 300] [--seed 1] [--out DIR]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ORG_LISP_DIR selects an Org checkout instead of the bundled version.
EMACS = ["emacs", "--batch", "-Q"] + (["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else [])

WORDS = ["alpha", "beta", "x", "y_z", "a/b", "foo.", "bar,", "(baz)", "žć", "日本", "1", "2.5", "it's", '"q"']

INLINE = [
    "*bold*", "/italic/", "_under_", "+strike+", "=verb=", "~code~", "*a /b/ c*", "*open", "close*", "/x", "y/",
    "* ", " *", "**", "_", "__", "=", "~", "+", "a*b*c", "*a*b", "a_b", "a^2", "a_{b c}", "a^(x)", "a_*", "x_y_",
    "[[link]]", "[[file:a.org][desc]]", "[[a][*b*]]", "[[a][b]", "[[a]", "[[]]", "]]", "[[a\\]b]]",
    "https://example.com/a.b", "http://x.y/z).", "mailto:a@b.c", "file:x", "id:abc", "<https://a.b/c d>",
    "<<target>>", "<<<radio>>>", "<<x", "y>>", "[fn:1]", "[fn:a:inline *b*]", "[fn::anon]", "[fn:", "[fn:x:[y]]",
    "[1/2]", "[50%]", "[/]", "[%]", "[a]", "[ ]", "[X]",
    "<2026-01-01 Thu>", "[2026-01-01 Thu 10:00]", "<2026-01-01 Thu>--<2026-01-02 Fri>", "<2026-01-01", "2026-01-01>",
    "<2026-01-01 Thu +1w>", "[2026-01-01]--[2026-01-02]", "::", " :: ", ":tag:", ":a:b:", "|", "\\", "#", ":",
    "\\alpha", "\\to{}", "\\foo{x}", "\\foo[a]{b}", "\\(a\\)", "\\[b\\]", "\\(", "$x$", "$$y$$", "$", "$5", "\\_ ",
    "@@html:<b>@@", "@@", "{{{m}}}", "{{{m(a,b)}}}", "{{{", "}}}", "src_sh{x}", "src_py[:a b]{y}", "call_f(x)",
    "call_g[h](y)[i]", "[cite:@key]", "[cite/t:see @a; @b]", "[cite:", "HTTP://A.B/c", "<FILE:x y>", "\\\\",
    "(", ")", "{", "}", "[", "]", "<", ">", "'", '"', "-", ".", ",", ";", "!", "?",
]

LINE_STARTS = [
    "", "", "", "", "- ", "+ ", "1. ", "2) ", "- [ ] ", "- [X] ", "- [@3] ", "* ", "** ", "*** ", "* TODO ", "** DONE ",
    "* TODO [#A] ", "# ", "#", ": ", ":", "| ", "|-", "#+KEY: ", "#+NAME: ", "#+CAPTION: ", "#+ATTR_X: ", "#+TBLFM: ",
    "#+begin_src ", "#+end_src", "#+begin_quote", "#+end_quote", "#+begin_example", "#+end_example", "#+BEGIN: x ",
    "#+END:", ":DRAWER:", ":END:", ":PROPERTIES:", ":ID: ", "SCHEDULED: ", "DEADLINE: ", "CLOSED: ", "CLOCK: ",
    "[fn:1] ", "[fn:b] ", "-----", "%%(", "\\begin{x}", "\\end{x}", "*", "+-+", "- a :: ", "*bold* ",
    "* TODO", "* TODO\t", "*  TODO  ", "* \tDONE ", "* [#A]", "* [#B]\t", "* [#AB] ", "* TODO [#C]", "* COMMENT ",
    "* COMMENT\n", "* TODO COMMENT ", "* DONE [#A] COMMENT\t", "*  ", "* \t", "* TODOX ",
]

INDENTS = ["", "", "", "", " ", "  ", "  ", "   ", "    ", "\t"]


def text(rng):
    parts = []
    for _ in range(rng.randint(0, 5)):
        parts.append(rng.choice(INLINE) if rng.random() < 0.5 else rng.choice(WORDS))
        parts.append(rng.choice([" ", " ", " ", "", "  "]))
    return "".join(parts).rstrip(" ") if rng.random() < 0.8 else "".join(parts)


def document(rng):
    lines = []
    for _ in range(rng.randint(1, 30)):
        roll = rng.random()
        if roll < 0.15:
            lines.append("")
        elif roll < 0.2:
            lines.append(rng.choice(["", " ", "\t"]))
        else:
            start = rng.choice(LINE_STARTS)
            indent = "" if start.startswith("*") and rng.random() < 0.8 else rng.choice(INDENTS)
            lines.append(indent + start + text(rng))
    body = "\n".join(lines)
    return body + ("\n" if rng.random() < 0.9 else "")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    out = args.out or tempfile.mkdtemp(prefix="org-fuzz-")
    os.makedirs(out, exist_ok=True)
    rng = random.Random(args.seed)
    paths = []
    for index in range(args.count):
        path = os.path.join(out, f"fuzz-{args.seed}-{index:05}.org")
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(document(rng))
        paths.append(path)

    dump = os.path.join(ROOT, "scripts", "org-element-dump.el")
    for start in range(0, len(paths), 200):
        subprocess.run([*EMACS, "-l", dump, *paths[start : start + 200]], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    rejected = [path for path in paths if not os.path.exists(path[:-4] + ".elements")]
    for path in rejected:
        os.remove(path)

    env = dict(os.environ, ORG_FIXTURES=out)
    result = subprocess.run(["cargo", "test", "-q", "-p", "org-syntax", "--test", "org_element"], cwd=ROOT, env=env,
                            capture_output=True, text=True)
    print(f"{len(paths) - len(rejected)} documents compared, {len(rejected)} rejected by Emacs; files in {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("parser disagrees")
        print(report[start:] if start >= 0 else report)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
