#!/usr/bin/env python3
"""Regenerate tests/agenda/*/expected-*.txt from Emacs.

Requires Emacs and faketime. The outputs are committed, so the tests run
without either.
"""

import glob
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from agenda_reference import ROOT, reference  # noqa: E402

cases = sorted(os.path.dirname(path) for path in glob.glob(os.path.join(ROOT, "tests", "agenda", "*", "case.json")))
for case in cases:
    for stale in glob.glob(os.path.join(case, "expected-*.txt")):
        os.remove(stale)
reference(cases)
print(f"updated {len(cases)} cases")
