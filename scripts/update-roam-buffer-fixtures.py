#!/usr/bin/env python3
"""Regenerate the `expected` results in tests/roam/buffer.json with Emacs.

Requires Emacs and org-roam (see roam_reference.py for
ORG_LSP_EMACS_PACKAGES). The results are committed, so the tests run
without them.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from roam_buffer_reference import load, reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "roam", "buffer.json")

if __name__ == "__main__":
    cases = load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
