#!/usr/bin/env python3
"""Write tests/table/cases.json: Org's table commands (`TAB`, `S-TAB`,
`M-<arrows>`, `M-S-<arrows>`, `C-c -`, `C-c RET`, `C-c ^`, `C-c C-c`,
`org-table-blank-field`) on chosen tables and on the random ones of
`scripts/fuzz-table.py` (seed 0), with what Emacs makes of them.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-table-fixtures.py
"""

import importlib.util
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from table_reference import reference, save  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_table", os.path.join(HERE, "fuzz-table.py"))
fuzz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz)

TABLE = "Text\n| a | bb |\n|---+----|\n| 1 |  2 |\n| ccc |  |\n#+TBLFM: $2=$1*2::@3$1=5\n"


def at(text, needle, offset=1):
    index = text.index(needle) + offset
    return text[:index] + "¦" + text[index:]


CASES = []
for key in fuzz.KEYS:
    for needle in ["bb", "2 |", "ccc", "|---"]:
        CASES.append({"text": at(TABLE, needle), "key": key})
CASES += [
    # The last field: a new row, or over the rule to the next row.
    {"text": "| a | b¦ |\n", "key": "TAB"},
    {"text": "| a | b¦ |\n|---+---|\n| c | d |\n", "key": "TAB"},
    {"text": "| a | b¦ |", "key": "TAB"},
    {"text": "| ¦a | b |\n", "key": "<backtab>"},
    # Links show their descriptions.
    {"text": "| [[https://orgmode.org][Org]] | x¦ |\n| y | z |\n", "key": "C-c -"},
    {"text": "| [[https://orgmode.org][Org]] | x |\n| y¦ | z |\n", "key": "M-<up>"},
    # Sorting: numbers, times, reversed; before the first column it asks.
    {"text": "| 10 | b |\n| 9¦ | a |\n| 100 | c |\n", "key": "C-c ^ n"},
    {"text": "| <2024-03-01 Fri> |\n| 1:30¦ |\n| [2023-01-02] |\n", "key": "C-c ^ T"},
    {"text": "¦| b |\n| a |\n", "key": "C-c ^ a"},
    # Formulas follow the columns and rows they name.
    {"text": "| a | b¦ | c |\n#+TBLFM: $3=$1+$2::@1$2=5\n", "key": "M-S-<left>"},
    {"text": "| a | b |\n| c¦ | d |\n#+TBLFM: @2$1=1::$2=remote(t,@2$1)\n", "key": "M-S-<up>"},
]
rng = random.Random(0)
CASES += [fuzz.case(rng) for _ in range(300)]

if __name__ == "__main__":
    path = os.path.join(HERE, "..", "tests", "table", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
