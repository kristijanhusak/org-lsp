#!/usr/bin/env python3
"""Differential fuzzing of cutting, copying and pasting subtrees against
Emacs (`org-cut-subtree`, `org-copy-subtree`, `org-paste-subtree`).

Random documents of headlines and text from scripts/fuzz-structure.py,
the cursor anywhere, with counts, levels and `org-adapt-indentation`; for
a paste, a random tree (or something that is none).

    scripts/fuzz-subtree.py [--count 300] [--seed 1] [--out FILE]

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
from subtree_reference import reference, save  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_structure", os.path.join(HERE, "fuzz-structure.py"))
fuzz_structure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz_structure)

ROOT = os.path.dirname(HERE)


def case(rng):
    text = fuzz_structure.document(rng)
    point = rng.randint(0, len(text))
    op = rng.choice(["cut", "copy", "paste", "paste", "paste"])
    result = {"text": text, "point": point, "op": op}
    if op != "paste" and rng.random() < 0.3:
        result["count"] = rng.randint(2, 4)
    if op == "paste":
        tree = fuzz_structure.document(rng)
        if rng.random() < 0.8:
            # From its first headline, as a cut leaves it.
            start = tree.find("*")
            tree = tree[start:] if start >= 0 else tree
        result["kill"] = tree
        if rng.random() < 0.15:
            result["level"] = rng.randint(1, 4)
    if rng.random() < 0.2:
        result["adapt"] = rng.choice(["t", "headline-data"])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-subtree-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "subtree"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_SUBTREE_CASES=out), capture_output=True, text=True,
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
