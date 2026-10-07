#!/usr/bin/env python3
"""Write tests/properties/cases.json: Org's property commands
(`org-set-property`, `org-delete-property`,
`org-delete-property-globally`, `S-<right>`/`S-<left>` on a property line,
`org-set-effort`, `org-inc-effort`, `org-property-action`,
`org-toggle-ordered-property`) on chosen documents and on random ones
(those of `scripts/fuzz-properties.py`, seed 0), with what Emacs makes of
each.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-properties-fixtures.py
"""

import importlib.util
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from properties_reference import reference, save  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_properties", os.path.join(HERE, "fuzz-properties.py"))
fuzz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz)

DOCUMENT = (
    "#+PROPERTY: Color_ALL red green \"light blue\"\n"
    "* TODO [#B] Task :work:\n"
    "SCHEDULED: <2026-05-01 Fri>\n"
    ":PROPERTIES:\n"
    ":Owner:    bob\n"
    ":Effort_ALL: 0:10 0:30 1:00\n"
    ":Effort:   0:30\n"
    ":END:\n"
    "** Child\n"
    "   :PROPERTIES:\n"
    "   :Color:    red\n"
    "   :Done:     [ ]\n"
    "   :END:\n"
    "** Bare child\n"
    "text\n"
)


def at(text, needle, offset=0):
    return text.index(needle) + offset


CASES = [
    # Setting: a new property, an existing one, an allowed value, TODO
    # and PRIORITY, a name with a space, a special property.
    {"command": "set", "point": at(DOCUMENT, "Bare"), "answers": ["Owner", "carol"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["owner", "alice"]},
    {"command": "set", "point": at(DOCUMENT, "Child"), "answers": ["Color", "light blue"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["TODO", "DONE"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["PRIORITY", "A"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["two words", "x"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["ITEM", "x"]},
    {"command": "set", "point": at(DOCUMENT, ":Owner:", 2), "answers": ["", "dave"]},
    {"command": "set", "point": at(DOCUMENT, "Task"), "answers": ["Owner", ""]},
    {"command": "set", "point": 3, "answers": ["Title", "File"]},
    # Deleting.
    {"command": "delete", "point": at(DOCUMENT, "Task"), "answers": ["OWNER"]},
    {"command": "delete", "point": at(DOCUMENT, "Child"), "answers": ["DONE"]},
    {"command": "delete", "point": at(DOCUMENT, "Bare"), "answers": []},
    {"command": "delete-globally", "point": 0, "answers": ["Color"]},
    {"command": "action", "key": "d", "point": at(DOCUMENT, ":Owner:"), "answers": ["EFFORT"]},
    {"command": "action", "key": "s", "point": at(DOCUMENT, ":Owner:"), "answers": ["", "eve"]},
    {"command": "action", "key": "x", "point": at(DOCUMENT, ":Owner:"), "answers": []},
    # Allowed values.
    {"command": "next", "point": at(DOCUMENT, ":Effort:"), "answers": []},
    {"command": "previous", "point": at(DOCUMENT, ":Effort:"), "answers": []},
    {"command": "next", "point": at(DOCUMENT, ":Color:"), "answers": []},
    {"command": "next", "point": at(DOCUMENT, ":Done:"), "answers": []},
    {"command": "next", "point": at(DOCUMENT, ":Owner:"), "answers": []},
    {"command": "next", "point": at(DOCUMENT, "Task"), "answers": []},
    # Effort.
    {"command": "effort", "point": at(DOCUMENT, "Task"), "answers": ["1:00"]},
    {"command": "effort", "point": at(DOCUMENT, "Bare"), "answers": ["2h 30min"]},
    {"command": "effort", "point": at(DOCUMENT, "Bare"), "answers": ["soon"]},
    {"command": "inc-effort", "point": at(DOCUMENT, "Task"), "answers": []},
    {"command": "inc-effort", "point": at(DOCUMENT, "Bare"), "answers": []},
    # ORDERED.
    {"command": "ordered", "point": at(DOCUMENT, "Task"), "answers": []},
    {"command": "ordered", "point": at(DOCUMENT, "text"), "answers": []},
]
for case in CASES:
    case["text"] = DOCUMENT
CASES.append({"text": "* Global\n", "point": 0, "command": "set", "answers": ["Size", "M"],
              "options": {"org_global_properties": {"Size_ALL": "S M L"}}})
CASES.append({"text": "* Last line without a break", "point": 0, "command": "set", "answers": ["A", "b"]})
CASES.append({"text": "# comment\n#+TITLE: x\n* H\n", "point": 0, "command": "set", "answers": ["A", "b"]})
rng = random.Random(0)
CASES += [fuzz.case(rng) for _ in range(400)]

if __name__ == "__main__":
    path = os.path.join(HERE, "..", "tests", "properties", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
