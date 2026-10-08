#!/usr/bin/env python3
"""Write tests/lists/cases.json: Org's meta and shift keys on list items
(`org-indent-item`, `org-outdent-item`, their `-tree` forms,
`org-move-item-up`/`-down`, `org-cycle-list-bullet`, `org-previous-item`
and `org-next-item` on the shifted up and down keys, and `C-c C-c`, which
repairs and renumbers a list from an item without a checkbox, and `C-c ^`,
`org-sort-list` from the start of an item's line), with what Emacs makes of
them.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-lists-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from return_reference import reference, save  # noqa: E402

TEXTS = [
    "- a\n- b\n  - c\n  - d\n- e\n",
    "* H\n1. one\n2. two\n   1) sub\n   2) sub2\n3. three\n",
    "- [ ] a\n- [X] b\n  - [ ] c\n  - [X] d\n- [ ] e\n",
    "* H [/]\n- [ ] a\n- [X] b\n- [ ] c\n",
    "  + x\n  + y\n\n  + z\n    text of z\n  + w\n",
    "- term :: desc\n- other :: thing\n  - inner\n",
    "1. a\n2. b\n\n\n3. c\nafter\n",
    "- only\n",
    "- a\n  continued\n- b\n  - b1\n    - b2\n- c\n",
    "* H\n  - indented a\n  - indented b\n    - deep\n",
    "- [@3] three\n- four\n- five\n",
    "1. a\n3. b\n   - c\n    + d\n 7) e\n",
    "- a\n+ b\n  * c\n    1) d\n- [ ] e\n  - [X] f\n",
    "* H [/]\n:PROPERTIES:\n:ORDERED: t\n:END:\n- [ ] a\n- [ ] b\n- [X] c\n  - [ ] c1\n",
    "- b 10 <2026-05-03 Sun>\n- [X] a 9\n- C 2 [2026-01-01 Thu]\n  - sub\n  - *bold* 3\n- [ ] d -3\n\n- e 0:01:05 :: timer\n",
    "1) 0:10:00 :: ten\n2) 0:02:00 :: two\n3) -0:01:00 :: minus\n",
]
SORT_KEYS = ["C-c ^ " + kind for kind in "aAnNtTxX"]
KEYS = ["M-<left>", "M-<right>", "M-S-<left>", "M-S-<right>", "M-<up>", "M-<down>", "S-<left>", "S-<right>",
        "S-<up>", "S-<down>", "C-c C-c"]
CASES = []
for text in TEXTS:
    starts = [0] + [index + 1 for index, c in enumerate(text) if c == "\n" and index + 1 < len(text)]
    for start in starts:
        line = text[start:].split("\n", 1)[0]
        stripped = line.lstrip()
        if not (stripped.startswith(("-", "+", "*")) or stripped[:1].isdigit()) or line.startswith("* "):
            continue
        for column in sorted({0, len(line) - len(stripped) + 2, len(line)}):
            for key in KEYS + (SORT_KEYS if column == 0 else []):
                CASES.append({"text": text, "point": start + min(column, len(line)), "key": key})

# `org-list-use-circular-motion`: the moves past the first and the last item,
# in lists without blank lines between items (where Org re-spaces the list
# as it inserts the item, which org-lsp does not).
for text in ["- a\n- b\n  - c\n  - d\n- e\n", "1. one\n2. two\n3. three\nafter\n", "- a\n  more\n- b\n- c\n"]:
    starts = [0] + [index + 1 for index, c in enumerate(text) if c == "\n" and index + 1 < len(text)]
    for start in starts:
        line = text[start:].split("\n", 1)[0]
        if not line.lstrip()[:1] in ("-", "1", "2", "3"):
            continue
        for column in (0, len(line)):
            for key in ["S-<up>", "S-<down>", "M-<up>", "M-<down>"]:
                CASES.append({"text": text, "point": start + column, "key": key, "circular": True})

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "lists", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES, preamble="(require 'org-timer)"))
    print(f"wrote {len(CASES)} cases to {path}")
