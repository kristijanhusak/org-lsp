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
    "* Project :work:\n** Child\n*** Grandchild :urgent:\n* Plain\n** Deep :work:\n*** Below\n* Other :work:\n",
]
QUERIES = [
    ("r", ["apple"]), ("r", ["Ap+le"]), ("r", ["^\\*+ "]), ("r", ["\\(ap\\|ba\\)ke"]),
    ("t", []), ("T", ["NEXT|WAIT"]), ("T", ["DONE"]), ("T", ["TODO"]),
    ("m", ["work"]), ("m", ["urgent-work"]), ("m", ["food|home"]), ("m", ["WHO=\"alice\""]), ("m", ["TODO=\"NEXT\""]),
    ("m", ["LEVEL>1"]), ("m", ["/!"]),
    ("M", ["work"]), ("M", ["urgent|work"]), ("M", ["LEVEL>0"]), ("M", ["food|home"]),
    ("d", []),
    ("b", ["2026-05-20"]), ("b", ["2026-05-18 10:00"]), ("a", ["2026-05-19"]), ("a", ["2026-01-01"]),
    ("D", ["2026-05-01", "2026-05-20"]), ("D", ["2026-04-01", "2026-12-31 23:59"]),
]
CASES = [{"text": text, "type": kind, "args": args} for text in TEXTS for kind, args in QUERIES]

# `org-occur-case-fold-search` and `org-sparse-tree-default-date-type`.
CASE_TEXTS = TEXTS[:3] + [
    "* TODO dates :x:\nSCHEDULED: <2026-05-10 Sun> DEADLINE: <2026-05-30 Sat>\nCLOSED: [2026-05-12 Tue]\n"
    "Body <2026-05-05 Tue 09:00> and [2026-05-25 Mon] and <2026-06-10 Wed>-<2026-06-12 Fri>\n"
    "=<2026-05-06 Wed>= ~[2026-05-07 Thu]~\n:PROPERTIES:\n:WHEN: <2026-05-08 Fri>\n:END:\n"
    "#+begin_src org\n<2026-05-09 Sat>\n#+end_src\n[[https://x.org][<2026-05-11 Mon>]]\n"
    "* DONE closed\nCLOSED: [2026-05-20 Wed 10:00]\n- item <2026-05-21 Thu>\n| <2026-05-22 Fri> |\n"
    "* <2026-05-23 Sat> in a headline\n",
]
CASE_QUERIES = [
    ("r", ["apple"]), ("r", ["Apple"]), ("r", ["\\Wapple"]), ("r", ["[[:upper:]]pple"]),
    ("t", []), ("T", ["todo"]), ("T", ["TODO"]), ("d", []),
    ("b", ["2026-05-20"]), ("a", ["2026-05-01"]), ("D", ["2026-05-01", "2026-06-30"]),
]
for fold in ["nil", "smart"]:
    CASES += [{"text": text, "type": kind, "args": args, "case": fold}
              for text in CASE_TEXTS for kind, args in CASE_QUERIES]
DATE_QUERIES = [("b", ["2026-05-20"]), ("a", ["2026-05-10"]), ("D", ["2026-05-01", "2026-06-30"])]
for date_type in ["all", "active", "inactive", "scheduled", "deadline", "closed"]:
    CASES += [{"text": text, "type": kind, "args": args, "date_type": date_type}
              for text in CASE_TEXTS for kind, args in DATE_QUERIES]

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "sparse", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
