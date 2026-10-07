#!/usr/bin/env python3
"""Differential fuzzing of setting tags against Emacs
(`org-set-tags-command` with fast tag selection, or the completion
prompt's answer).

Random tag tables (`#+TAGS:` lines with keys, exclusive groups and group
tags, or none), headlines with tags, keys and options.

    scripts/fuzz-tags.py [--count 300] [--seed 1] [--out FILE]

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
from tags_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
TAGS = ["work", "home", "@office", "@home", "a", "b", "Ab", "x_y", "café", "q", "t1", "#hash", "%p"]


def table(rng):
    tokens = []
    for _ in range(rng.randint(0, 8)):
        choice = rng.random()
        if choice < 0.15:
            group = rng.sample(TAGS, rng.randint(1, 3))
            tokens += ["{"] + [t + (f"({rng.choice('abcxyz')})" if rng.random() < 0.5 else "") for t in group] + ["}"]
        elif choice < 0.25:
            group = rng.sample(TAGS, rng.randint(2, 3))
            tokens += ["[", group[0], ":"] + group[1:] + ["]"]
        else:
            tag = rng.choice(TAGS)
            tokens.append(tag + (f"({rng.choice('abcdefghwxyzq')})" if rng.random() < 0.5 else ""))
    return " ".join(tokens)


def document(rng):
    lines = []
    if rng.random() < 0.7:
        lines.append("#+TAGS: " + table(rng))
    if rng.random() < 0.2:
        lines.append("#+FILETAGS: :" + rng.choice(TAGS) + ":")
    for _ in range(rng.randint(1, 3)):
        title = rng.choice(["Task", "TODO Thing", "DONE [#A] Done one", "", "Title with words"])
        tags = rng.sample(TAGS, rng.randint(0, 3))
        head = "*" * rng.randint(1, 3) + " " + title
        if tags:
            head += rng.choice([" ", "  ", "\t"]) + ":" + ":".join(tags) + ":" + rng.choice(["", " "])
        lines.append(head)
        if rng.random() < 0.4:
            lines.append("text")
    return "\n".join(lines) + "\n"


def case(rng):
    text = document(rng)
    starts = [0] + [i + 1 for i, c in enumerate(text) if c == "\n" and i + 1 < len(text)]
    point = rng.choice(starts)
    options = {}
    if rng.random() < 0.3:
        options["org_use_fast_tag_selection"] = rng.choice([True, False])
    if rng.random() < 0.2:
        options["org_tags_sort_function"] = rng.choice(["string<", "string>"])
    if rng.random() < 0.2:
        options["org_tag_alist"] = table(rng)
    if rng.random() < 0.2:
        options["org_tag_persistent_alist"] = table(rng)
    if rng.random() < 0.1:
        options["org_auto_align_tags"] = False
    if rng.random() < 0.4:
        answer = ":".join(rng.sample(TAGS, rng.randint(0, 3)))
        return {"text": text, "point": point, "typed": "", "answer": answer, "options": options}
    keys = "".join(rng.choice("abcdefghwxyzq !") for _ in range(rng.randint(0, 5)))
    return {"text": text, "point": point, "typed": keys + "\r", "options": options}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-tags-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "tags"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_TAGS_CASES=out), capture_output=True, text=True,
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
