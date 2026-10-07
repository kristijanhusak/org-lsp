#!/usr/bin/env python3
"""Differential fuzzing of `org-cite-insert` (`C-c C-x @`) against Emacs:
random documents with citations (styles, global and local affixes, several
references, blanks, citations side by side) among headlines, lists,
blocks, drawers, keywords, tables and links; the cursor anywhere; with and
without the prefix argument.

    scripts/fuzz-cite-insert.py [--count 300] [--seed 1] [--out FILE]

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
from cite_insert_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
BIB = "@misc{a1, title={One}, author={Doe, J.}}\n@misc{b2, title={Two}}\n@misc{c3, title={Three}, year=1999}\n" \
      "@book{d-4, author={Roe, R. and Poe, P.}, title={Four}}\n"
KEYS = ["a1", "b2", "c3", "d-4"]
STYLES = ["", "text", "author", "nil", "note", "numeric", "noauthor", "nocite", " "]


def citation(rng):
    style = rng.choice(["", "", "/t", "/text", "/a/c", "/nil"])
    references = []
    for _ in range(rng.choice([1, 1, 2, 3])):
        prefix = rng.choice(["", "", "", "see ", "pre "])
        suffix = rng.choice(["", "", "", " p. 3", ", ch.2"])
        references.append(f"{prefix}@{rng.choice(KEYS + ['zz'])}{suffix}")
    body = rng.choice([";", "; ", " ;"]).join(references)
    if rng.random() < 0.2:
        body = rng.choice(["global ", "g;", "g ; "]) + ";" + body if not body.startswith(";") else body
    if rng.random() < 0.2:
        body = body + rng.choice([";end", "; end", ";"])
    lead = rng.choice(["", "", " ", "\n"]) if rng.random() < 0.1 else ""
    if rng.random() < 0.05:
        body = body.replace("; ", ";\n", 1)
    return f"[cite{style}:{lead}{body}]"


def paragraph(rng):
    words = []
    for _ in range(rng.randint(1, 4)):
        roll = rng.random()
        if roll < 0.45:
            words.append(citation(rng))
        elif roll < 0.55:
            words.append(citation(rng) + citation(rng))
        elif roll < 0.62:
            words.append("[[https://example.com][a link]]")
        elif roll < 0.68:
            words.append("*bold*")
        else:
            words.append(rng.choice(["See", "text", "and", "word."]))
    return rng.choice([" ", "  ", "\t"]).join(words) + rng.choice(["", " ", "  "])


def element(rng):
    roll = rng.random()
    if roll < 0.35:
        return paragraph(rng)
    if roll < 0.5:
        todo = rng.choice(["", "TODO ", "DONE "])
        prio = rng.choice(["", "[#A] "])
        tags = rng.choice(["", " :tag:", "   :a:b:"])
        title = rng.choice(["Head", "Head " + citation(rng), ""])
        return f"{'*' * rng.randint(1, 2)} {todo}{prio}{title}{tags}"
    if roll < 0.6:
        return f"{rng.choice(['- ', '1. ', '- [ ] '])}{paragraph(rng)}"
    if roll < 0.66:
        return f"#+begin_verse\n{paragraph(rng)}\n#+end_verse"
    if roll < 0.7:
        return "#+begin_src emacs-lisp\n(foo)\n#+end_src"
    if roll < 0.74:
        return f":DRAWER:\n{paragraph(rng)}\n:END:"
    if roll < 0.78:
        return rng.choice(["#+title: A " + citation(rng), "#+caption: cap", "#+author: me"])
    if roll < 0.82:
        return f"| a | {citation(rng)} |\n| b |   |"
    if roll < 0.86:
        return "SCHEDULED: <2024-01-02 Tue>"
    if roll < 0.9:
        return f"{rng.choice(['#+caption: x', '#+caption[s ' + citation(rng) + ']: long', '#+name: n'])}\n{paragraph(rng)}"
    if roll < 0.92:
        return f"[fn:1] {paragraph(rng)}"
    if roll < 0.94:
        return f"{rng.choice(['- ', '  + '])}tag {citation(rng)} :: {paragraph(rng)}"
    if roll < 0.95:
        return f"* COMMENT {citation(rng)}"
    if roll < 0.96:
        return f"|---+---|\n| {citation(rng)} | x |"
    if roll < 0.97:
        return f"{citation(rng)}"
    if roll < 0.98:
        return f"word x_{{y}} a^b [fn::inline {citation(rng)}] <<<radio>>>"
    if roll < 0.99:
        return ":PROPERTIES:\n:A: [cite:@a1]\n:END:"
    return ""


def case(rng):
    body = "\n".join(element(rng) for _ in range(rng.randint(1, 4)))
    text = "#+bibliography: refs.bib\n\n" + body + rng.choice(["\n", "", "\n\n"])
    start = len("#+bibliography: refs.bib\n\n")
    at = rng.randint(start, len(text))
    keys = rng.sample(KEYS, rng.randint(1, 3)) + [""]
    if rng.random() < 0.05:
        keys = [""]
    return {"text": text[:at] + "¦" + text[at:], "files": {"refs.bib": BIB}, "arg": rng.random() < 0.3,
            "keys": keys, "style": rng.choice(STYLES)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-cite-insert-fuzz-"), "insert.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "cite_insert"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_CITE_INSERT_CASES=out), capture_output=True,
        text=True,
    )
    errors = sum(1 for case in cases if "error" in case["expected"])
    print(f"{args.count} cases ({errors} where Emacs fails); saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:12000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
