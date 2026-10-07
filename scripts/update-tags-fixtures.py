#!/usr/bin/env python3
"""Write tests/tags/cases.json: `org-set-tags-command` (`C-c C-q`) with
fast tag selection by key and with the completion prompt, under the
options that change it, with what Emacs makes of each.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-tags-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from tags_reference import reference, save  # noqa: E402

TABLE = "#+TAGS: work(w) home(h) { @office(o) @home(m) } errand\n"
GROUPS = "#+TAGS: [ GTD : Control(c) Persp(p) ] { A(a) B(b) C } : x\n#+TAGS: other(t) work\n"
DOCUMENTS = [
    TABLE + "* TODO Task :old:\nbody\n",
    TABLE + "* Task\n** Child :work:\ntext\n",
    TABLE + "* Ünïcode title with words                                  :work:@office:\n",
    GROUPS + "* H :A:x:\n",
    "* Plain :a:b:\n* Other :c:\n#+FILETAGS: :ft:\n",
    "#+TAGS: alpha beta gamma(a) delta\n* Entry\n",
    "* \n",
    "* :only:\n",
    "*** DONE [#A] Deep one  :x:   \n",
    "Before\n* First :f:\n",
]
FAST = ["\r", "w\r", "wo\r", "om\r", "owm\r", " \r", "w w\r", "ho\r", "!om\r", "\tnew\r\r",
        "x\r", "q", "ac\r", "ab\r", "cp\r", "a\r", "zz\r", "tw\r"]
ANSWERS = ["", "new", "a:b", ":x:y:", "bad!char", "work::home"]
OPTIONS = [
    {},
    {"org_fast_tag_selection_single_key": True},
    {"org_tags_sort_function": "string<"},
    {"org_tags_sort_function": "string>", "org_auto_align_tags": False},
    {"org_tag_persistent_alist": "per(e) work(k)"},
    {"org_tag_alist": "conf(f) { x(x) y(y) }"},
    {"org_use_fast_tag_selection": True},
    {"org_use_fast_tag_selection": False},
]
CASES = []
for text in DOCUMENTS:
    lines = text.split("\n")
    # The cursor at the start of the last line with text.
    starts = [sum(len(line) + 1 for line in lines[:index]) for index in range(len(lines) - 1)]
    for options in OPTIONS:
        for point in starts[-1:]:
            for typed in FAST:
                CASES.append({"text": text, "point": point, "typed": typed, "options": options})
            for answer in ANSWERS:
                typed = "\x01\x0b" + answer + "\r"
                CASES.append({"text": text, "point": point, "typed": typed, "answer": answer, "options": options})

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "tags", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
