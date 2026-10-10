#!/usr/bin/env python3
"""Write tests/journal/cases.json: hand-picked cases of org-journal's
commands, with what Emacs does with them.

Requires Emacs, faketime and org-journal (see journal_reference.py for
ORG_LSP_EMACS_PACKAGES). The results are committed, so the tests run
without them.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from journal_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "tests", "journal", "cases.json")

SAT = "2026-10-10 14:05"
FRIDAY = "* Friday, 10/09/26\n** 09:00 standup\nnotes\n"
TODOS = (
    "* Friday, 10/09/26\n"
    "** 09:00 standup\nnotes\n"
    "** TODO call Bob\n:PROPERTIES:\n:X: 1\n:END:\n<2026-10-09 Fri>\n"
    "** 10:00 work\n*** TODO review\nSCHEDULED: <2026-10-11 Sun>\n*** DONE merged\n"
    "** DONE paid\n"
)
WEEK = (
    "* Monday, 10/05/26\n:PROPERTIES:\n:CREATED:  20261005\n:END:\n** 09:00 a\n** TODO w\n"
    "* Wednesday, 10/07/26\n:PROPERTIES:\n:CREATED:  20261007\n:END:\n** TODO carry me\n** 11:00 b\n"
)
WEEKLY = {"org_journal_file_type": "weekly"}


def case(command, files, clock=SAT, **extra):
    out = {"clock": clock, "files": files, "command": command}
    out.update(extra)
    return out


CASES = [
    # New entries: a new file, today's file, another time of the day.
    case("new-entry", {}),
    case("new-entry", {"20261010": "* Saturday, 10/10/26\n** 09:00 morning\ntext\n"}),
    case("new-entry", {"20261010": "* Saturday, 10/10/26\n** 09:00 morning\ntext"}),
    case("new-entry", {"20261010": "* Saturday, 10/10/26\n"}, prefix=1),
    case("new-entry", {}, prefix=1),
    case("new-entry", {"20261010": "#+title: Day\n\n* Saturday, 10/10/26\n** 09:00 a\n*** sub\nbody\n** 10:00 b\n"}),
    # Options of the entry's text.
    case("new-entry", {}, options={"org_journal_date_format": "%Y-%m-%d %a", "org_journal_time_format": "%H:%M -- "}),
    case("new-entry", {}, options={"org_journal_date_prefix": "#+title: ", "org_journal_time_prefix": "* "}),
    case("new-entry", {}, options={"org_journal_time_prefix": "- ", "org_journal_time_format": "%R: "}),
    case("new-entry", {}, options={"org_journal_hide_entries_p": False}),
    case("new-entry", {}, options={"org_journal_file_format": "%Y-%m-%d.org", "org_journal_date_format": "%A %-d %B"}),
    case("new-entry", {}, options={"org_journal_file_format": "%Y/%m/%d.org"}, answers=["y"]),
    case("new-entry", {}, options={"org_journal_file_header": "#+title: Journal of %F\n#+startup: content\n"}),
    case("new-entry", {}, options={"org_journal_file_header": "Notes for %A"}),
    case("new-entry", {}, options={"org_journal_date_format": ""}),
    case("new-entry", {}, options={"org_journal_enable_encryption": True}),
    # After midnight, until `org-extend-today-until`, it is yesterday.
    case("new-entry", {}, clock="2026-10-10 02:30", options={"org_extend_today_until": 4}),
    case("new-entry", {}, clock="2026-10-10 02:30",
         options={"org_extend_today_until": 4, "org_journal_time_format_post_midnight": "%R (late) "}),
    # Weekly, monthly and yearly files.
    case("new-entry", {}, options=WEEKLY),
    case("new-entry", {"20261005": WEEK}, options=WEEKLY),
    case("new-entry", {"20261005": WEEK}, options={**WEEKLY, "org_journal_carryover_items": ""}),
    case("new-entry", {}, options={**WEEKLY, "org_journal_start_on_weekday": 7}),
    case("new-entry", {}, options={"org_journal_file_type": "monthly"}),
    case("new-entry", {}, options={"org_journal_file_type": "yearly", "org_journal_file_format": "%Y.org"}),
    case("new-entry", {"2026.org": "* Monday, 01/05/26\n:PROPERTIES:\n:CREATED:  20260105\n:END:\n** x\n"},
         options={"org_journal_file_type": "yearly", "org_journal_file_format": "%Y.org",
                  "org_journal_created_property_timestamp_format": "[%Y-%m-%d]"}),
    case("new-entry", {"202610": "* Thursday, 10/01/26\n:PROPERTIES:\n:CREATED:  20261001\n:END:\n"
                                 "* Monday, 10/12/26\n:PROPERTIES:\n:CREATED:  20261012\n:END:\n"},
         options={"org_journal_file_type": "monthly", "org_journal_file_format": "%Y%m"}),
    # Carrying over.
    case("new-entry", {"20261009": TODOS}),
    case("new-entry", {"20261008": TODOS.replace("10/09", "10/08")}),
    case("new-entry", {"20261009": TODOS}, options={"org_journal_skip_carryover_drawers": ["PROPERTIES"]}),
    case("new-entry", {"20261009": TODOS}, options={"org_journal_carryover_items": "TODO=\"TODO\"|TODO=\"DONE\""}),
    case("new-entry", {"20261009": "* Friday, 10/09/26\n** TODO only\n"},
         options={"org_journal_carryover_delete_empty_journal": "always"}),
    case("new-entry", {"20261009": "* Friday, 10/09/26\n** TODO only\n"},
         options={"org_journal_carryover_delete_empty_journal": "ask"}, answers=["y"]),
    case("new-entry", {"20261009": "* Friday, 10/09/26\n** TODO only\n"},
         options={"org_journal_carryover_delete_empty_journal": "ask"}, answers=["n"]),
    case("new-entry", {"20261009": FRIDAY}, options={"org_journal_carryover_delete_empty_journal": "always"}),
    case("new-entry", {"20261005": WEEK.replace("** TODO w\n", "")},
         options={**WEEKLY, "org_journal_carryover_delete_empty_journal": "always"}),
    case("new-entry", {"20261009": TODOS}, prefix=1),
    # Before every entry of a weekly file: the new one goes first.
    case("new-entry", {"20261012": "* Wednesday, 10/14/26\n:PROPERTIES:\n:CREATED:  20261014\n:END:\n** a\n"},
         clock="2026-10-13 09:00", options=WEEKLY),
    # The file before keeps an entry, but the buffer is left narrowed to
    # the emptied one and org-journal's dates are read from it: the file
    # goes.
    case("new-entry", {"20260921": "* Friday, 09/25/26\n:PROPERTIES:\n:CREATED:  20260925\n:END:\n"
                                   "* Saturday, 09/26/26\n:PROPERTIES:\n:CREATED:  20260926\n:END:\n"
                                   "** TODO call Bob\n*** 12:30 x\n- [ ] item\n"},
         clock="2026-10-07 01:05", options={**WEEKLY, "org_journal_carryover_delete_empty_journal": "always"}),
    # The entry before is in the same file, which org-journal saves: the
    # file gets its final newline then.
    case("new-entry", {"20261005": "* Monday, 10/05/26\n:PROPERTIES:\n:CREATED:  20261005\n:END:\n** NEXT lunch\n"
                                   "*** TODO standup\n"},
         clock="2026-10-11 03:30", prefix=1, options={**WEEKLY, "org_journal_time_format": ""}),
    # Encrypted entries need a headline for the tag.
    case("new-entry", {}, options={"org_journal_date_prefix": "#+title: ", "org_journal_enable_encryption": True}),
    # A date picked: before today, today, after.
    case("new-date-entry", {}, date="2026-10-02"),
    case("new-date-entry", {}, date="2026-10-10"),
    case("new-date-entry", {}, date="2026-10-12 10:00"),
    # The time is dropped: today at a later hour is still today.
    case("new-date-entry", {}, date="2026-10-10 16:00"),
    case("new-date-entry", {"20261002": "* Friday, 10/02/26\n** a\n"}, date="2026-10-02", prefix=1),
    case("new-scheduled-entry", {}, date="2026-10-12 10:00-11:00"),
    case("new-scheduled-entry", {}, date="2026-10-12", prefix=1),
    case("new-scheduled-entry", {}, date="2026-10-01"),
    case("new-scheduled-entry", {}, date="2026-10-12", options={"org_journal_scheduled_string": "SCHEDULED:"}),
    # Moving between entries.
    case("next", {"20261009": FRIDAY, "20261012": "* Monday, 10/12/26\n** x\n"}, **{"from": "20261009"}),
    case("previous", {"20261009": FRIDAY, "20261012": "* Monday, 10/12/26\n** x\n"}, **{"from": "20261012"}),
    case("previous", {"20261009": FRIDAY}, **{"from": "20261009"}),
    case("next", {"20261009": FRIDAY, "notes.org": "* x\n"}, **{"from": "notes.org"}),
    case("next", {"20261005": WEEK}, options=WEEKLY, **{"from": "20261005", "point": 60}),
    case("previous", {"20261005": WEEK}, options=WEEKLY, **{"from": "20261005", "point": 140}),
    case("next", {"20261009": FRIDAY, "20261011": "#+title: x\n* Sunday, 10/11/26\n** a\nb\n** c\n* other\n"},
         **{"from": "20261009"}),
    case("open-current", {"20261010": "* Saturday, 10/10/26\n** x\n"}),
    case("open-current", {}),
    case("open-current", {"20261005": WEEK}, options=WEEKLY),
    # The files the agenda gets (`org-journal-enable-agenda-integration`).
    case("agenda-files", {"20261009": FRIDAY, "20261010": "* x\n", "20261012": "* y\n", "notes.org": "* n\n"}),
    case("agenda-files", {"20261008": "* x\n", "20261009": "* x\n", "20261010": "* y\n"},
         clock="2026-10-10 02:05", options={"org_extend_today_until": 4}),
    case("agenda-files", {"20260928": "* x\n", "20261005": "* x\n", "20261012": "* y\n"}, options=WEEKLY),
    case("agenda-files", {"202609": "* x\n", "202610": "* x\n", "203101": "* y\n"},
         options={"org_journal_file_type": "monthly", "org_journal_file_format": "%Y%m"}),
]

if __name__ == "__main__":
    save(PATH, reference(CASES))
    print(f"wrote {len(CASES)} cases")
