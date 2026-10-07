#!/usr/bin/env python3
"""Differential fuzzing of the commands that ask for a date against Emacs.

Builds documents the way `fuzz-shift.py` does (headlines, planning lines,
timestamps and ranges of every shape) and answers the way
`fuzz-read-date.py` does, has Emacs run `org-schedule`, `org-deadline`,
`org-timestamp` or `org-timestamp-inactive`, and runs the test over the
same cases. The cases stay in the output file.

    scripts/fuzz-planning.py [--count 400] [--seed 1] [--out FILE]

Requires Emacs, faketime and cargo.
"""

import argparse
import datetime
import importlib.util
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from planning_reference import COMMANDS, reference, save  # noqa: E402
from todo_reference import CURSOR  # noqa: E402

ROOT = os.path.dirname(HERE)


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), os.path.join(HERE, name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


shift = load("fuzz-shift")
answers = load("fuzz-read-date")


def case(rng, now):
    documents = shift.Generator(rng, now)
    base = documents.case()
    out = {"clock": base["clock"], "text": base["text"], "command": rng.choice(list(COMMANDS))}
    out["answer"] = answers.Generator(rng).answer()
    if rng.random() < 0.2:
        out["repeated"] = True
    # The settings the date commands read; the others do not matter here.
    options = {key: value for key, value in base.get("options", {}).items() if key in ("org_log_done",)}
    if rng.random() < 0.35:
        options["org_adapt_indentation"] = rng.choice([True, "headline-data", False])
    if options:
        out["options"] = options
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=400)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    # One clock per invocation: Emacs is started once for all cases.
    now = datetime.datetime(2026, 1, 1) + datetime.timedelta(minutes=rng.randint(0, 800 * 1440))
    cases = reference([case(rng, now) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-planning-fuzz-"), "cases.json")
    save(out, cases)

    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "planning"],
        cwd=ROOT,
        env=dict(os.environ, ORG_PLANNING_CASES=out),
        capture_output=True,
        text=True,
    )
    errors = sum(1 for case in cases if case["expected"].startswith("ERROR:"))
    print(f"{args.count} cases ({errors} where Emacs refuses), clock {now:%Y-%m-%d %H:%M}; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" date commands differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
