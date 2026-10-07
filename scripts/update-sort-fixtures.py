#!/usr/bin/env python3
"""Write tests/sort/cases.json: sorting cases with what Emacs makes of them.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-sort-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sort_reference import reference, save  # noqa: E402

KIDS = "* Parent\n** b\n** C\ntext\n** a\n*** deep\n** b\n* Next\n"
TEXTS = [
    KIDS,
    "* Parent\n** b\n** a\n\n\n* Next\n",
    "* Parent\n** b\n** a\n\n* Next\n",
    "* Parent\n** b\n** a",
    "intro\n* b\n* a\n* c",
    "* only\n",
    "no headlines\n",
    "* P\n*** deep b\n** a\n*** deep a\n",
    "* P\n*** deep b\n*** deep a\n* Q\n",
    "* P\n** TODO [#A] *bold* zed :tag:\n** [[https://x.org][Alpha]] link\n** COMMENT beta\n** [[file:gamma.org]]\n** ~code~ delta\n** <https://eps.org>\n",
    "* P\n** 10 apples\n** 9 pears\n** -2 below\n** 1.5 half\n** none\n** 1e2 hundred\n",
    "* P\n** [#C] c\n** b\n** [#A] a\n** [#1] one\n** title with [#B] inside\n",
    "#+TODO: TODO NEXT | DONE CANCELLED\n* P\n** DONE d\n** a\n** NEXT n\n** CANCELLED c\n** TODO t\n",
    "* P\n** later\nSCHEDULED: <2026-06-01 Mon>\n** none\n** sooner\nSCHEDULED: <2026-05-01 Fri 09:00>\n** today\nSCHEDULED: <2026-05-18 Mon 10:30>\n",
    "* P\n** later\nDEADLINE: <2026-06-01 Mon>\n** none\n** sooner\nDEADLINE: <2026-05-01 Fri>\n",
    "* P\n** body time <2026-06-02 Tue>\n** inactive [2026-04-02 Thu]\n** heading <2026-05-01 Fri 08:00>\n** none\n",
    "* P\n** made late\n[2026-06-01 Mon 10:00]\n** made early\n  [2026-01-01 Thu]\n** none\n** not at bol x [2025-01-01 Wed]\n",
    "* P\n** two\n:LOGBOOK:\nCLOCK: [2026-05-01 Fri 10:00]--[2026-05-01 Fri 12:00] =>  2:00\n:END:\n** none\n** one\n:LOGBOOK:\nCLOCK: [2026-05-01 Fri 10:00]--[2026-05-01 Fri 11:00] =>  1:00\n:END:\n*** sub\nCLOCK: [2026-05-01 Fri 10:00]--[2026-05-01 Fri 12:30] =>  2:30\n",
    "* P\n** c\n:PROPERTIES:\n:WHO: carol\n:END:\n** a\n:PROPERTIES:\n:WHO: Alice\n:END:\n** none\n** b\nSCHEDULED: <2026-05-01 Fri>\n:PROPERTIES:\n:who: bob\n:END:\n",
    "* P\n** same\n** b\n** same\n** a\n",
    "* P\n** Émile\n** zed\n** Ärger\n** abc\n",
]
CASES = []
for text in TEXTS:
    for point in sorted({0, text.find("\n") + 1 if "\n" in text else 0, len(text) // 2}):
        for kind in "aAnNpPoOtTsSdDcCkKr":
            case = {"text": text, "point": point, "type": kind}
            if kind == "r":
                case["property"] = "WHO"
            CASES.append(case)
CASES.append({"text": TEXTS[18], "point": 0, "type": "R", "property": "WHO"})
CASES.append({"text": KIDS, "point": 0, "type": "x"})

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "sort", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
