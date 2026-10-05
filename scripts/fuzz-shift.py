#!/usr/bin/env python3
"""Differential fuzzing of priority and timestamp changes against Emacs.

Generates random documents with headlines, priority cookies and
timestamps of every shape, puts the cursor somewhere, has Emacs run one
of its shifted-cursor commands, and runs the test over the same cases.
The cases stay in the output file.

    scripts/fuzz-shift.py [--count 400] [--seed 1] [--out FILE]

Requires Emacs, faketime and cargo.
"""

import argparse
import datetime
import os
import random
import re
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shift_reference import COMMANDS, CURSOR, reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

TITLES = ["a", "Call them", "review the report", "žć", "日本 語", "with [[https://x.org][link]]", "", "x  y"]
TAGS = ["", "", "", ":a:", ":work:home:", ":x_y:@z:", ":日本:"]
NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun", "Monday", "Lun", "sáb", "月", "", "", "x"]
TIMES = ["10:00", "09:30", "9:30", "14:00", "23:45", "00:00", "0:07", "10:58", "23:59", "24:00", "8:03"]
ENDS = ["-11:00", "-09:45", "-23:59", "-00:10", "-10:58", "-9:30", "-29:00"]
REPEATERS = [" +1d", " +2w", " +1m", " +1y", " ++1w", " .+1d", " +5h", " +12d", " .+2d/4d", " +1w/2w", "  +3d", " +1W",
             " .+2D"]
WARNINGS = [" -3d", " -0d", " -2w", " --2d", " -1m", " -1y", " -10d", " -2D", " --1W"]
JUNK = ["", "", "", "", " foo", " x y", "  ", " 12", " +", " -", " +1x"]
PRIORITIES = ["[#A]", "[#B]", "[#C]", "[#D]", "[#Z]", "[#a]", "[#1]", "[#5]", "[#12]", "[#64]", "[#65]", "[#]", "[#AB]"]


class Generator:
    def __init__(self, rng, now):
        self.rng = rng
        self.now = now

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def date(self):
        if self.chance(0.06):
            return self.pick(["2026-02-30", "2026-13-01", "2026-00-10", "2026-12-32", "2025-02-29", "1999-12-31",
                              "2100-01-01", "2026-01-00", "2024-02-29", "1970-01-01"])
        spread = self.pick([2, 10, 40, 400, 4000])
        day = self.now.date() + datetime.timedelta(days=self.rng.randint(-spread, spread))
        return day.strftime("%Y-%m-%d")

    def stamp(self):
        body = self.date()
        name = self.pick(NAMES)
        if name:
            body += self.pick([" ", " ", " ", "  ", ""]) + name
        if self.chance(0.5):
            body += self.pick([" ", " ", " ", "  "]) + self.pick(TIMES)
            if self.chance(0.4):
                body += self.pick(ENDS)
        if self.chance(0.4):
            body += self.pick(REPEATERS)
        if self.chance(0.3):
            body += self.pick(WARNINGS)
        body += self.pick(JUNK)
        open_, close = self.pick([("<", ">"), ("<", ">"), ("[", "]"), ("[", "]"), ("<", "]"), ("[", ">")])
        return open_ + body + close

    def stamps(self):
        """One timestamp, or a range of two."""
        if self.chance(0.15):
            return self.stamp() + self.pick(["--", "-", "---"]) + self.stamp()
        return self.stamp()

    def headline(self, level, keywords):
        line = "*" * level
        if self.chance(0.6):
            line += self.pick([" ", " ", " ", "  "]) + self.pick(keywords)
        if self.chance(0.5):
            line += self.pick([" ", " ", "  ", ""]) + self.pick(PRIORITIES)
        title = self.pick(TITLES)
        if self.chance(0.2):
            title += " " + self.stamps()
        # A cookie after the title. Not behind a timestamp: Org's parser
        # leaves what stands before such a cookie out of the title, while
        # the command that repeats an entry reads timestamps there.
        if self.chance(0.1) and "<" not in title:
            title += " " + self.pick(PRIORITIES)
        if title:
            line += self.pick([" ", " ", "  ", "\t"]) + title
        tags = self.pick(TAGS)
        if tags:
            line += self.pick([" ", "   ", "\t", " " * 30]) + tags
        return line if len(line) > level else line + " "

    def clock(self):
        line = self.pick(["CLOCK:", "CLOCK:", "clock:", "CLOCK: ", "CLOCK:  "]) + self.pick([" ", " ", ""])
        line += self.stamp()
        if self.chance(0.8):
            line += self.pick(["--", "--", "-", "---"]) + self.stamp()
            if self.chance(0.7):
                line += self.pick([" => ", " =>  ", "=>", "  => "]) + self.pick(["0:22", " 1:00", "12:34", "x"])
            if self.chance(0.1):
                line += " rest"
        return line

    def entry(self, level, keywords):
        indent = self.pick(["", "", "", "  ", " " * (level + 1)])
        lines = [self.headline(level, keywords)]
        if self.chance(0.5):
            parts = [keyword + self.pick([" ", " ", "  "]) + self.stamps()
                     for keyword in self.rng.sample(["SCHEDULED:", "DEADLINE:", "CLOSED:"], self.pick([1, 1, 2, 3]))]
            lines.append(indent + " ".join(parts))
        for _ in range(self.pick([0, 1, 1, 2, 3])):
            roll = self.rng.random()
            if roll < 0.35:
                lines.append(indent + self.clock())
            elif roll < 0.7:
                lines.append(indent + "Text with " + self.stamps() + self.pick(["", " in it.", " " + self.stamp()]))
            elif roll < 0.8:
                lines.append("# " + self.stamps())
            elif roll < 0.9:
                lines.append(indent + "- item " + self.stamps())
            else:
                lines.append(indent + self.pick(["plain text", "", "| a | " + self.stamp() + " |"]))
        return lines

    def cursor(self, line):
        """A column in `line`, more often than not at a place of interest."""
        marks = [index for index, c in enumerate(line) if c in "<[>]"]
        if marks and self.chance(0.8):
            # Inside or right next to a timestamp.
            start = self.pick(marks)
            return max(0, min(len(line), start + self.rng.randint(-1, 34)))
        return self.rng.randint(0, len(line))

    def case(self):
        lines = []
        keywords = ["TODO", "DONE"]
        options = {}
        if self.chance(0.2):
            definition = self.pick(["TODO NEXT | DONE", "TODO(t) WAIT(w@/!) | DONE(d!) CANCELLED(c@)", "A B C"])
            lines.append("#+TODO: " + definition)
            keywords = [word.split("(")[0] for word in definition.split() if word != "|"]
        if self.chance(0.15):
            lines.append(self.pick(["#+PRIORITIES: A E C", "#+PRIORITIES: 1 9 5", "#+priorities: A C A",
                                    "#+PRIORITIES: A B", "#+PRIORITIES: C A B", "#+PRIORITIES: 1 20 10 x"]))
        if self.chance(0.3):
            lines.append("Text before the first headline " + self.stamps())
        entries = [self.entry(self.pick([1, 1, 2]), keywords) for _ in range(self.rng.randint(1, 3))]
        target = self.rng.randrange(len(entries))
        for index, entry in enumerate(entries):
            if index == target:
                # The headline and lines with timestamps are the places that matter.
                weights = [3 if number == 0 or any(c in line for c in "<[") else 1 for number, line in enumerate(entry)]
                line = self.rng.choices(range(len(entry)), weights)[0]
                column = self.cursor(entry[line])
                entry[line] = entry[line][:column] + CURSOR + entry[line][column:]
            lines += entry
        text = "\n".join(lines) + ("\n" if self.chance(0.9) else "")

        if self.chance(0.15):
            options.update(self.pick([
                {"priorityHighest": "A", "priorityLowest": "E", "priorityDefault": "C"},
                {"priorityHighest": 1, "priorityLowest": 9, "priorityDefault": 5},
                {"priorityHighest": 1, "priorityLowest": 64, "priorityDefault": 12},
                {"priorityHighest": "A", "priorityLowest": "C", "priorityDefault": "A"},
                {"priorityHighest": "A", "priorityLowest": "C", "priorityDefault": "D"},
                {"priorityHighest": "B", "priorityLowest": "B", "priorityDefault": "B"},
            ]))
        if self.chance(0.15):
            options["priorityStartCycleWithDefault"] = False
        if self.chance(0.2):
            options["timestampRoundingMinutes"] = self.pick([0, 1, 5, 10, 15, 30, 7])
        if self.chance(0.1):
            options["tagsColumn"] = self.pick([-80, -40, 0, 30, 77])
        if self.chance(0.1):
            options["logDone"] = self.pick(["time", "note"])
        command = self.pick(list(COMMANDS))
        line = next(line for line in text.split("\n") if CURSOR in line).replace(CURSOR, "")
        on_headline = line.startswith("*")
        if command.startswith("shift-") and line.lstrip().startswith(("- ", "|")):
            # On list items and in tables the shifted keys do things the
            # server does not do yet. What they do on a timestamp there is
            # the same as the plain commands.
            command = {"shift-up": "timestamp-up", "shift-down": "timestamp-down", "shift-left": "date-earlier",
                       "shift-right": "date-later"}[command]
        if command.startswith("date-") and on_headline:
            # Called by name on a headline, Org's command sets the first
            # TODO keyword, which no key leads to. The shifted keys are
            # what a user reaches it through.
            command = {"date-later": "shift-right", "date-earlier": "shift-left"}[command]
        case = {"clock": self.now.strftime("%Y-%m-%d %H:%M"), "text": text, "command": command}
        if self.chance(0.25):
            case["count"] = self.pick([1, 2, 2, 3, 7, 30, 70, 0, -1, -2, 400])
            # Moved by an hour or more at once, the end of a time range
            # comes out of Org with minutes like `70`; the server carries
            # them properly.
            if abs(case["count"]) >= 60 and re.search(r"-\d{1,2}:\d\d", text):
                case["count"] = 30
        if self.chance(0.25):
            case["repeated"] = True
        if options:
            case["options"] = options
        return case


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=400)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    # One clock per invocation: Emacs is started once for all cases.
    now = datetime.datetime(2026, 1, 1) + datetime.timedelta(minutes=rng.randint(0, 800 * 1440))
    generator = Generator(rng, now)
    cases = reference([generator.case() for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-shift-fuzz-"), "cases.json")
    save(out, cases)

    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "shift"],
        cwd=ROOT,
        env=dict(os.environ, ORG_SHIFT_CASES=out),
        capture_output=True,
        text=True,
    )
    errors = sum(1 for case in cases if case["expected"].startswith("ERROR:"))
    changed = sum(1 for case in cases if case["expected"].replace(CURSOR, "") != case["text"].replace(CURSOR, ""))
    print(f"{args.count} cases ({errors} where Emacs refuses, {changed - errors} that change the text), "
          f"clock {now:%Y-%m-%d %H:%M}; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" changes differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
