#!/usr/bin/env python3
"""Regenerate the `expected` results in tests/archive/cases.json with Emacs.

Requires Emacs. The results are committed, so the tests run without it.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from archive_reference import load, reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "archive", "cases.json")

if __name__ == "__main__":
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
