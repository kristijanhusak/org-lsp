#!/usr/bin/env python3
"""Regenerate tests/agenda/*/expected-*.txt from Emacs.

Requires Emacs and faketime, and for the `super-*` cases ORG_LSP_EMACS_PACKAGES
(see agenda_reference.py). The outputs are committed, so the tests run
without any of them.
"""

import glob
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from agenda_reference import ROOT, reference  # noqa: E402

cases = sorted(os.path.dirname(path) for path in glob.glob(os.path.join(ROOT, "tests", "agenda", "*", "case.json")))
if any(os.path.basename(case).startswith("super-") for case in cases) and not os.environ.get("ORG_LSP_EMACS_PACKAGES"):
    sys.exit("Set ORG_LSP_EMACS_PACKAGES to a directory with org-super-agenda and what it needs (compat, dash, s, ht, ts)")
for case in cases:
    for stale in glob.glob(os.path.join(case, "expected-*.txt")):
        os.remove(stale)
reference(cases)
print(f"updated {len(cases)} cases")
