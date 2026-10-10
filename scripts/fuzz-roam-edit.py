#!/usr/bin/env python3
"""Differential fuzzing of org-roam's editing commands against Emacs.

Generates random directories of Org files (as fuzz-roam.py does), puts the
cursor somewhere in one of them, runs an alias, tag or ref command or
`org-roam-link-replace-all` there in Emacs, and runs the roam test over the
same cases. Cases the two disagree on are reported; all of them stay in
the output file.

    scripts/fuzz-roam-edit.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs, org-roam (see roam_reference.py) and cargo.
"""

import argparse
import importlib.util
import json
import os
import random
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from roam_edit_reference import reference  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_roam", os.path.join(HERE, "fuzz-roam.py"))
fuzz_roam = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz_roam)

ROOT = os.path.dirname(HERE)
CURSOR = "¦"
# A headline with no title: org-roam leaves its node out of the database,
# and Emacs then changes the cursor's line for tags (a difference org-lsp
# does not follow), with Org reading the tags of such headlines its own way.
EMPTY_HEADLINE = re.compile(r"(?m)^\*+[ \t]+(?:(?:TODO|DONE|NEXT)[ \t]+)?(?:\[#\w+\][ \t]*)?(?::[^ \t\n]+:)?[ \t]*$")
NAMES = ["a.org", "b.org", "sub/c.org"]
COMMANDS = ["alias-add", "alias-remove", "tag-add", "tag-remove", "ref-add", "ref-remove", "replace-links"]


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
        names = rng.sample(NAMES, rng.randint(1, 3))
        files = {}
        for name in names:
            document = generator.document()
            while EMPTY_HEADLINE.search(document):
                document = generator.document()
            files[name] = document
        edited = rng.choice(names)
        text = files[edited]
        at = rng.randint(0, len(text))
        files[edited] = text[:at] + CURSOR + text[at:]
        command = rng.choice(COMMANDS)
        if command.startswith("tag"):
            arg = rng.sample(fuzz_roam.TAGS + ["new", "x y"], rng.randint(1, 2))
        elif command.startswith("alias"):
            arg = rng.choice(fuzz_roam.WORDS + ["the note two", 'q"uote', "back\\slash"])
        elif command.startswith("ref"):
            arg = rng.choice(["https://example.com/r", "@smith2020", "https://x.org/a b", "[cite:@doe2021]", "new"])
        else:
            arg = None
        case = {"files": files, "command": command}
        if arg is not None:
            case["arg"] = arg
        cases.append(case)
    reference(cases)
    out = args.out or tempfile.mkstemp(prefix="org-roam-edit-fuzz-", suffix=".json")[1]
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "roam", "roam_edits"],
        cwd=ROOT,
        env=dict(os.environ, ORG_ROAM_EDIT_CASES=out),
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
