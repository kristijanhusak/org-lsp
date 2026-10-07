#!/usr/bin/env python3
"""Write tests/sparse/cases.json: sparse trees with what Emacs finds.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-sparse-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sparse_reference import reference, save  # noqa: E402

TEXTS = [
    "* TODO a\nbody apple\n** DONE b\nDEADLINE: <2026-05-20 Wed>\n* NEXT c :work:\nDEADLINE: <2026-05-25 Mon>\nSCHEDULED: <2026-05-01 Fri>\n* x\nSCHEDULED: <2026-06-01 Mon 10:00>\n",
    "#+TODO: TODO NEXT WAIT | DONE\n* TODO Apple pie :food:\n:PROPERTIES:\n:WHO: alice\n:END:\n** NEXT bake\nSCHEDULED: <2026-05-18 Mon 09:00> DEADLINE: <2026-05-19 Tue>\n** WAIT oven :home:\n* DONE old\nCLOSED: [2026-05-01 Fri] DEADLINE: <2026-05-02 Sat>\n* todo lower\n* TODO\n* TODOx\n",
    "* Notes\nApple apple APPLE\n  deadline: <2026-05-10 Sun>\n* Plan :work:urgent:\n  SCHEDULED: <2026-07-01 Wed>\n** Sub :urgent:\nDEADLINE: <2026-04-01 Wed> <2026-05-01 Fri>\n",
    "no headlines, apple\n",
]
QUERIES = [
    ("r", ["apple"]), ("r", ["Ap+le"]), ("r", ["^\\*+ "]), ("r", ["\\(ap\\|ba\\)ke"]),
    ("t", []), ("T", ["NEXT|WAIT"]), ("T", ["DONE"]), ("T", ["TODO"]),
    ("m", ["work"]), ("m", ["urgent-work"]), ("m", ["food|home"]), ("m", ["WHO=\"alice\""]), ("m", ["TODO=\"NEXT\""]),
    ("m", ["LEVEL>1"]), ("m", ["/!"]),
    ("d", []),
    ("b", ["2026-05-20"]), ("b", ["2026-05-18 10:00"]), ("a", ["2026-05-19"]), ("a", ["2026-01-01"]),
    ("D", ["2026-05-01", "2026-05-20"]), ("D", ["2026-04-01", "2026-12-31 23:59"]),
]
CASES = [{"text": text, "type": kind, "args": args} for text in TEXTS for kind, args in QUERIES]

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "sparse", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
