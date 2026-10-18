#!/usr/bin/env python3
"""Write tests/inlinetask/cases.json: keys on inline tasks, with what Emacs
makes of them when `org-inlinetask` is loaded.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-inlinetask-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from inlinetask_reference import reference, save  # noqa: E402

S15 = "*" * 15
S16 = "*" * 16
CLOSED = f"* A\n{S15} TODO Task :tag:\nbody\n{S15} END\n* B\n"
OPEN = f"* A\n{S15} Task\nafter\n* B\n"
DEEP = f"* A\n{S16} Task\nbody\n{S16} END\n"
NESTED = f"* A\n** B\n{S15} Task\n{S15} END\n*** C\n* D\n"
INDENTED = f"* A\n{S15} Task\n                 body\n{S15} END\n"
ODD = f"#+STARTUP: odd\n* A\n{'*' * 29} Task\nbody\n{'*' * 29} END\n"


def at(text, needle, offset=0):
    return text.index(needle) + offset


CASES = []


def case(text, point, key, emacs=None, mark=None):
    entry = {"text": text, "point": point, "key": key}
    if mark is not None:
        entry["mark"] = mark
    if emacs:
        entry["emacs"] = emacs
    CASES.append(entry)


STATE = '(org-inlinetask-default-state "TODO")'
# `org-inlinetask-insert-task`
for text in ["", "* A\nbody\n", "* A\nbody", "text", "* A\n\n* B\n", OPEN, CLOSED]:
    for point in sorted({0, len(text), text.find("\n") + 1 if "\n" in text else 0, len(text) // 2}):
        case(text, point, "C-c C-x t")
        case(text, point, "C-c C-x t", STATE)
        case(text, point, "C-u C-c C-x t", STATE)
case(CLOSED, at(CLOSED, "body", 2), "C-c C-x t")
case(CLOSED, at(CLOSED, S15), "C-c C-x t")
case(CLOSED, at(CLOSED, S15 + " END"), "C-c C-x t")
case(CLOSED, at(CLOSED, " END", 4), "C-c C-x t")
case("* A\nsome text\nmore\n", at("* A\nsome text\nmore\n", "more", 4), "C-c C-x t", mark=4)
case("* A\nsome text\nmore\n", 4, "C-c C-x t", mark=at("* A\nsome text\nmore\n", "more", 4))
case("* A\nsome text\nmore\n", at("* A\nsome text\nmore\n", "more", 2), "C-c C-x t", mark=6)
case("* A\nsome text\nmore\n", at("* A\nsome text\nmore\n", "more"), "C-c C-x t", STATE, mark=4)
case("* A\nbody\n", 4, "C-c C-x t", "(org-odd-levels-only t)")
case("* A\nbody\n", 4, "C-c C-x t", "(org-inlinetask-min-level 5)")
case("* A\nbody\n", 4, "C-c C-x t", '(org-inlinetask-default-state "DONE")')

# `org-inlinetask-promote` and `org-inlinetask-demote` through `M-<left>`
# and `M-<right>`; a subtree's promotion leaves inline tasks as they are.
for text in [CLOSED, OPEN, DEEP, NESTED, INDENTED, DEEP.rstrip("\n"), f"* A\n{S16} Task\n{S16} END   \nafter\n"]:
    for needle in ["Task", "END"]:
        if needle in text:
            for offset in [0, -1]:
                point = at(text, needle, offset)
                for key in ["M-<left>", "M-<right>"]:
                    case(text, point, key)
                    case(text, point, key, "(org-adapt-indentation t)")
for text in [NESTED, CLOSED, DEEP]:
    for point in [0, text.find("\n") + 1, at(text, "Task")]:
        for key in ["M-S-<left>", "M-S-<right>"]:
            case(text, point, key)
BODY = f"* A\n text\n{S15} Task\n body\n{S15} END\n after\n** B\n"
for key in ["M-<right>", "M-S-<right>", "M-<left>"]:
    case(BODY, 0, key, "(org-adapt-indentation t)")
    case(BODY, at(BODY, "* B"), key, "(org-adapt-indentation t)")
case(ODD, at(ODD, "Task"), "M-<right>")
case(ODD, at(ODD, "Task"), "M-<left>")
case(ODD, 15, "M-S-<right>")

# `org-next-visible-heading` and `org-previous-visible-heading` pass inline
# tasks.
for text in [CLOSED, OPEN, NESTED]:
    for point in sorted({0, text.find("\n") + 1, at(text, "Task"), len(text) - 1}):
        for key in ["C-c C-n", "C-c C-p", "C-u 2 C-c C-n"]:
            case(text, point, key)

# `TAB` on an inline task (`org-inlinetask-toggle-visibility`) and on its
# parent.
for text in [CLOSED, OPEN, DEEP, NESTED, f"* A\n{S15} Task\n{S15} END\n", f"* A\n{S15} T\n{S15} U\nx\n{S15} END\n"]:
    for point in sorted({0, at(text, "Task") if "Task" in text else at(text, " T", 1), text.find("\n") + 1}):
        case(text, point, "TAB")
        case(text, point, "TAB TAB")

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "inlinetask", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
