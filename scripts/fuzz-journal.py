#!/usr/bin/env python3
"""Differential fuzzing of org-journal's commands against Emacs.

Generates random journal directories (daily, weekly, monthly and yearly
files, entries with times, TODO items, drawers and timestamps), options,
clocks and commands, records what org-journal does with them, and runs
the journal test over the same cases. Cases the two disagree on are
reported; all of them stay in the output file.

    scripts/fuzz-journal.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs, faketime, org-journal (see journal_reference.py) and cargo.
"""

import argparse
import datetime
import json
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from journal_reference import reference  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

WORDS = ["standup", "call Bob", "review", "žć note", "日本", "lunch", "x", "write it down"]
DATE_FORMATS = ["%A, %x", "%A, %x", "%Y-%m-%d %a", "%a %-d %b", "%^A %e", "%B %d, %Y"]
TIME_FORMATS = ["%R ", "%R ", "%I:%M %p ", "%H%M ", ""]
TIME_PREFIXES = ["** ", "** ", "** ", "*** ", "- "]
DATE_PREFIXES = ["* ", "* ", "* ", "#+title: "]
CARRYOVER = ['TODO="TODO"', 'TODO="TODO"', 'TODO="TODO"', "", 'TODO="TODO"|TODO="NEXT"', "+work", 'TODO="DONE"']
KEYWORDS = ["TODO", "DONE", "NEXT", None, None, None]


class Generator:
    def __init__(self, rng, options):
        self.rng = rng
        self.options = options

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, p):
        return self.rng.random() < p

    def date_header(self, day):
        fmt = self.options.get("org_journal_date_format", "%A, %x")
        return self.options.get("org_journal_date_prefix", "* ") + format_time(fmt, day)

    def stamp(self, day):
        text = day.strftime("%Y-%m-%d")
        if self.chance(0.7):
            name = day.strftime("%a")
            text += " " + self.pick([name, name.upper(), name.lower()])
        return f"<{text}>"

    def body(self, day, depth):
        lines = []
        for _ in range(self.rng.randint(0, 2)):
            roll = self.rng.random()
            if roll < 0.25:
                lines.append(self.pick(WORDS))
            elif roll < 0.4:
                lines.append(self.stamp(day + datetime.timedelta(days=self.rng.randint(-3, 3))))
            elif roll < 0.5:
                lines.append("SCHEDULED: " + self.stamp(day))
            elif roll < 0.6:
                lines.append(":LOGBOOK:\n- note\n:END:")
            elif roll < 0.7:
                lines.append(":PROPERTIES:\n:X: 1\n:END:")
            elif roll < 0.8:
                lines.append("")
            else:
                lines.append("- [ ] item")
        return lines

    def item(self, day, level, depth=0):
        stars = "*" * level
        keyword = self.pick(KEYWORDS)
        words = []
        if self.chance(0.4) and self.options.get("org_journal_time_format", "%R ") == "%R ":
            words.append(f"{self.rng.randint(6, 22):02}:{self.pick(['00', '30'])}")
        words.append(self.pick(WORDS))
        tags = " :work:" if self.chance(0.15) else ""
        line = f"{stars} {keyword + ' ' if keyword else ''}{' '.join(words)}{tags}"
        lines = [line] + self.body(day, depth)
        if depth < 2 and self.chance(0.3):
            for _ in range(self.rng.randint(1, 2)):
                lines.extend(self.item(day, level + 1, depth + 1))
        return lines

    def entry(self, day, created):
        lines = [self.date_header(day)]
        if created:
            fmt = self.options.get("org_journal_created_property_timestamp_format", "%Y%m%d")
            lines += [":PROPERTIES:", f":CREATED:  {format_time(fmt, day)}", ":END:"]
        level = self.options.get("org_journal_time_prefix", "** ").count("*") or 2
        for _ in range(self.rng.randint(0, 3)):
            lines.extend(self.item(day, level))
        return lines


def format_time(fmt, day):
    """The formats the generator uses, as Emacs writes them in the C locale."""
    out = fmt.replace("%A, %x", day.strftime("%A, %m/%d/%y")).replace("%-d", str(day.day))
    out = out.replace("%^A", day.strftime("%A").upper()).replace("%e", f"{day.day:2}")
    return day.strftime(out)


def files_for(rng, options, today):
    gen = Generator(rng, options)
    kind = options.get("org_journal_file_type", "daily")
    fmt = options.get("org_journal_file_format", "%Y%m%d")
    files = {}
    days = sorted({today + datetime.timedelta(days=rng.randint(-12, 6)) for _ in range(rng.randint(0, 4))})
    if kind == "daily":
        for day in days:
            lines = []
            if rng.random() < 0.15:
                lines.append("#+title: notes")
            lines += gen.entry(day, False)
            text = "\n".join(lines)
            if rng.random() < 0.85:
                text += "\n"
            files[format_time(fmt, day)] = text
    else:
        groups = {}
        for day in days:
            if kind == "weekly":
                start = day - datetime.timedelta(days=day.weekday())
            elif kind == "monthly":
                start = day.replace(day=1)
            else:
                start = day.replace(month=1, day=1)
            groups.setdefault(start, []).append(day)
        for start, members in groups.items():
            lines = []
            for day in members:
                lines += gen.entry(day, True)
            files[format_time(fmt, start)] = "\n".join(lines) + "\n"
    if rng.random() < 0.1:
        files["notes.org"] = "* not a journal\n"
    return files


def make_case(rng):
    today = datetime.date(2026, 10, 10) + datetime.timedelta(days=rng.randint(-3, 3))
    hour = rng.choice([1, 3, 9, 14, 23])
    clock = f"{today.isoformat()} {hour:02}:{rng.choice(['05', '30'])}"
    options = {}
    kind = rng.choice(["daily", "daily", "daily", "weekly", "monthly", "yearly"])
    if kind != "daily":
        options["org_journal_file_type"] = kind
        if kind == "yearly":
            options["org_journal_file_format"] = "%Y.org"
        elif kind == "monthly" and rng.random() < 0.5:
            options["org_journal_file_format"] = "%Y-%m.org"
        if kind == "weekly" and rng.random() < 0.3:
            options["org_journal_start_on_weekday"] = rng.choice([3, 7])
        if rng.random() < 0.2:
            options["org_journal_created_property_timestamp_format"] = "%Y-%m-%d"
    elif rng.random() < 0.3:
        options["org_journal_file_format"] = rng.choice(["%Y-%m-%d.org", "%F.org", "%Y%m%d.org"])
    if rng.random() < 0.3:
        options["org_journal_date_format"] = rng.choice(DATE_FORMATS)
    if rng.random() < 0.2:
        options["org_journal_time_format"] = rng.choice(TIME_FORMATS)
    if rng.random() < 0.15:
        options["org_journal_time_prefix"] = rng.choice(TIME_PREFIXES)
    if kind == "daily" and rng.random() < 0.1:
        options["org_journal_date_prefix"] = rng.choice(DATE_PREFIXES)
    if rng.random() < 0.2:
        options["org_journal_hide_entries_p"] = False
    if rng.random() < 0.3:
        options["org_journal_carryover_items"] = rng.choice(CARRYOVER)
    if rng.random() < 0.2:
        options["org_journal_skip_carryover_drawers"] = rng.choice([["LOGBOOK"], ["PROPERTIES"], ["LOGBOOK", "X"]])
    if rng.random() < 0.3:
        options["org_journal_carryover_delete_empty_journal"] = rng.choice(["always", "ask"])
    if rng.random() < 0.1:
        options["org_journal_file_header"] = rng.choice(["#+title: %F\n", "Week %V\n", "#+startup: overview\n"])
    if rng.random() < 0.1:
        options["org_journal_enable_encryption"] = True
    if rng.random() < 0.2:
        options["org_extend_today_until"] = 4
        if rng.random() < 0.5:
            options["org_journal_time_format_post_midnight"] = "%R late "
    if rng.random() < 0.1:
        options["org_journal_scheduled_string"] = "SCHEDULED:"
    if rng.random() < 0.15:
        options["org_todo_keywords"] = ["TODO", "NEXT", "|", "DONE"]
    files = files_for(rng, options, today)
    case = {"clock": clock, "files": files, "options": options}
    roll = rng.random()
    journal = [name for name in files if name != "notes.org"]
    if roll < 0.5:
        case["command"] = "new-entry"
        case["prefix"] = 1 if rng.random() < 0.2 else 0
    elif roll < 0.65:
        case["command"] = "new-date-entry"
        day = today + datetime.timedelta(days=rng.randint(-8, 5))
        case["date"] = day.isoformat() + (rng.choice(["", " 10:00"]) if rng.random() < 0.3 else "")
        case["prefix"] = 1 if rng.random() < 0.2 else 0
    elif roll < 0.75:
        case["command"] = "new-scheduled-entry"
        day = today + datetime.timedelta(days=rng.randint(-1, 5))
        case["date"] = day.isoformat() + rng.choice(["", " 10:00", " 9:00-10:30"])
        case["prefix"] = 1 if rng.random() < 0.3 else 0
    elif roll < 0.92 and files:
        case["command"] = rng.choice(["next", "previous"])
        case["from"] = rng.choice(journal or list(files))
        case["point"] = rng.randint(0, len(files[case["from"]]))
    else:
        case["command"] = "open-current"
    if options.get("org_journal_carryover_delete_empty_journal") == "ask":
        case["answers"] = [rng.choice(["y", "n"])]
    if "/" in options.get("org_journal_file_format", ""):
        case.setdefault("answers", []).append("y")
    return case


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = [make_case(rng) for _ in range(args.count)]
    reference(cases)
    out = args.out or tempfile.mkstemp(prefix="org-journal-fuzz-", suffix=".json")[1]
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "journal"],
        cwd=ROOT,
        env=dict(os.environ, ORG_JOURNAL_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases in {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        print(report[max(0, start - 10):report.find("stack backtrace") if "stack backtrace" in report else None][:20000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
