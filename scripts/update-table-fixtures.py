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
    # A row marked `#` is recalculated on the way (`org-table-maybe-recalculate-line`).
    {"text": "| # | 1¦ |  |\n#+TBLFM: $3=$2*2\n", "key": "TAB"},
    {"text": "| # | 1 | ¦ |\n| # | 3 |  |\n#+TBLFM: $3=$2*2\n", "key": "<backtab>"},
    {"text": "| a | b | c |\n| # | 2¦ |  |\n#+TBLFM: $3=$2*2\n", "key": "C-c RET"},
    {"text": "| a | b | c |\n| # | 2¦ |  |\n#+TBLFM: $3=$2*2\n", "key": "C-c C-c"},
    {"text": "| a | b | c |\n| # | 20¦ | 1 |\n| # | 3 |  |\n#+TBLFM: $3=$2*2\n", "key": "TAB"},
]
rng = random.Random(0)
CASES += [fuzz.case(rng) for _ in range(300)]

# The options, each a setting of the test and a binding for Emacs: a
# formula in a field left as text (`org-table-formula-evaluate-inline`), a
# line marked `#` not recalculated
# (`org-table-allow-automatic-line-recalculation`).
INLINE = {"settings": {"evaluate_inline": False}, "emacs": "(org-table-formula-evaluate-inline nil)"}
LINE = {"settings": {"line_recalculation": False}, "emacs": "(org-table-allow-automatic-line-recalculation nil)"}
CASES += [
    {"text": "| a | =1+2¦ |\n| b | c |\n", "key": "TAB", **INLINE},
    {"text": "| a | :=1+2¦ |\n| b | c |\n", "key": "TAB", **INLINE},
    {"text": "| a | =1+2¦ |\n| b | c |\n", "key": "C-c C-c", **INLINE},
    {"text": "| # | 1 | ¦ |\n#+TBLFM: $3=$2*2\n", "key": "TAB", **LINE},
    {"text": "| # | 1 | ¦ |\n#+TBLFM: $3=$2*2\n", "key": "C-c C-c", **LINE},
    {"text": "| # | 1 | ¦ |\n#+TBLFM: $3=$2*2\n", "key": "<backtab>", **LINE},
]

# Without `org-table-automatic-realign` TAB moves without aligning; without
# `org-table-tab-jumps-over-hlines` it makes a row before a rule.
REALIGN = {"settings": {"automatic_realign": False}, "emacs": "(org-table-automatic-realign nil)"}
JUMP = {"settings": {"tab_jumps_over_hlines": False}, "emacs": "(org-table-tab-jumps-over-hlines nil)"}
CASES += [
    {"text": "| a | bbbb¦ |\n| cc | d |\n", "key": "TAB", **REALIGN},
    {"text": "| a | bbbb |\n| cc | d¦ |\n", "key": "TAB", **REALIGN},
    {"text": "| a | bbbb |\n| cc¦ | d |\n", "key": "<backtab>", **REALIGN},
    {"text": "| aaa | b |\n| c | ddd¦ |\n|---+---|\n| e | f |\n", "key": "TAB", **REALIGN},
    {"text": "| a | b¦ |\n|---+---|\n| c | d |\n", "key": "TAB", **JUMP},
    {"text": "| a | b¦ |\n|---+---|\n", "key": "TAB", **JUMP},
    {"text": "| aaa | b |\n| c¦ | ddd |\n|---+---|\n| e | f |\n", "key": "TAB", **JUMP},
]

if __name__ == "__main__":
    path = os.path.join(HERE, "..", "tests", "table", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
