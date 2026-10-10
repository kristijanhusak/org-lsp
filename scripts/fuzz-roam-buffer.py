#!/usr/bin/env python3
"""Differential fuzzing of org-roam's buffer against Emacs.

Generates random directories of Org files (as fuzz-roam.py does), puts the
cursor somewhere in one of them, renders the org-roam buffer for the node
there in Emacs with some of its sections, and runs the roam test over the
same cases. Cases the two disagree on are reported; all of them stay in
the output file.

    scripts/fuzz-roam-buffer.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs, org-roam (see roam_reference.py), ripgrep and cargo.
"""

import argparse
import importlib.util
import json
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from roam_buffer_reference import reference  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_roam", os.path.join(HERE, "fuzz-roam.py"))
fuzz_roam = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz_roam)

ROOT = os.path.dirname(HERE)
CURSOR = "¦"
NAMES = ["a.org", "b.org", "sub/c.org", "data/d.org", "e.txt"]
SECTIONS = ["backlinks", "reflinks", "unlinked-references"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = []
    for _ in range(args.count):
        generator = fuzz_roam.Generator(rng)
        names = rng.sample(NAMES, rng.randint(2, 4))
        files = {name: generator.document() for name in names}
        edited = rng.choice([name for name in names if name.endswith(".org")] or names)
        text = files[edited]
        at = rng.randint(0, len(text))
        files[edited] = text[:at] + CURSOR + text[at:]
        case = {"files": files}
        if rng.random() < 0.6:
            case["sections"] = rng.sample(SECTIONS, rng.randint(1, 3))
        cases.append(case)
    reference(cases)
    out = args.out or tempfile.mkstemp(prefix="org-roam-buffer-fuzz-", suffix=".json")[1]
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "roam", "roam_buffer"],
        cwd=ROOT,
        env=dict(os.environ, ORG_ROAM_BUFFER_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases in {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("differences from Emacs")
        print(report[max(0, start - 10):report.find("stack backtrace") if "stack backtrace" in report else None][:20000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
