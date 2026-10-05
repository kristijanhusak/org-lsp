#!/usr/bin/env python3
"""Differential fuzzing of the date prompt (`org-read-date`) against Emacs.

Generates answers from the pieces Org understands (relative shifts,
weekdays, month names, ISO, European and American dates, ISO weeks, times
with am/pm or `h`, ranges and durations), mixed with near misses and
noise, has Emacs read each one, and runs the test over the same cases.
The cases stay in the output file.

    scripts/fuzz-read-date.py [--count 600] [--seed 1] [--out FILE]

Requires Emacs, faketime and cargo.
"""

import argparse
import datetime
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from read_date_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

WEEKDAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun", "monday", "Friday", "SAT", "Thursday", "tues", "frid"]
MONTHS = ["jan", "feb", "mar", "Apr", "may", "june", "July", "aug", "sep", "sept", "Oct", "november", "DEC", "febr"]
UNITS = ["", "d", "w", "m", "y", "h", "D", "W", "x"]
NOISE = ["", "", "", "", " ", "  ", "x", "at", "the", "gmt", "Z", "+0200", "-05:00", ",", "/", ".", "-", "T"]


class Generator:
    def __init__(self, rng):
        self.rng = rng

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def number(self, *choices):
        return str(self.pick(choices) if choices else self.rng.randint(0, 40))

    def relative(self):
        sign = self.pick(["+", "-", "++", "--", "", "+-"])
        count = self.pick(["", "", "1", "2", "3", "10", "0", "52"])
        unit = self.pick(UNITS + WEEKDAYS[:7]) if self.chance(0.8) else ""
        return sign + count + unit

    def date(self):
        year = self.pick([2026, 2027, 2025, 1999, 2040, 1969, 26, 7, 99, 50, 0])
        month = self.rng.randint(0, 14)
        day = self.rng.randint(0, 33)
        shapes = [
            lambda: f"{year:04d}-{month:02d}-{day:02d}" if year >= 100 else f"{year}-{month}-{day}",
            lambda: f"{year}-{month}-{day}",
            lambda: f"{month}-{day}",
            lambda: f"{day}.{month}.",
            lambda: f"{day}. {month}. {year:04d}",
            lambda: f"{day}.{month}.{year}",
            lambda: f"{month}/{day}",
            lambda: f"{month}/{day}/{year}",
            lambda: f"{self.pick(MONTHS)} {day}",
            lambda: f"{day} {self.pick(MONTHS)}",
            lambda: f"{self.pick(MONTHS)} {day} {year}",
            lambda: f"{self.pick(MONTHS)}",
            lambda: f"{day}",
            lambda: f"{year:04d}{month:02d}{day:02d}" if year >= 1000 else f"{year}",
            lambda: f"{year:04d}-W{self.rng.randint(0, 54):02d}" + self.pick(["", f"-{self.rng.randint(0, 9)}"]),
            lambda: f"w{self.rng.randint(0, 54)}" + self.pick(["", f"-{self.rng.randint(0, 7)}"]),
            lambda: f"{self.pick(['', '26-', '2027-'])}w{self.rng.randint(1, 53)}",
            lambda: f"{year:04d}-{self.rng.randint(0, 370):03d}",
            lambda: f"--{month:02d}-{day:02d}",
            lambda: f"---{day:02d}",
            lambda: self.pick(WEEKDAYS),
        ]
        return self.pick(shapes)()

    def time(self):
        hour = self.rng.randint(0, 25)
        minute = self.pick([0, 5, 15, 30, 45, 59, 60, 7])
        shapes = [
            lambda: f"{hour}:{minute:02d}",
            lambda: f"{hour:02d}:{minute:02d}",
            lambda: f"{hour % 13 or 12}{self.pick(['am', 'pm', 'AM', 'Pm'])}",
            lambda: f"{hour % 13}:{minute:02d}{self.pick(['am', 'pm'])}",
            lambda: f"{hour}h{minute:02d}",
            lambda: f"{hour}h",
            lambda: f"h{minute:02d}",
            lambda: f"{hour}:{minute:02d}:{self.rng.randint(0, 61):02d}",
            lambda: f"{hour % 10}:{minute:02d}",
        ]
        first = self.pick(shapes)()
        if self.chance(0.3):
            return first + self.pick(["-", "--", "+"]) + self.pick(shapes + [lambda: f"{self.rng.randint(0, 3)}"])()
        return first

    def answer(self):
        parts = []
        if self.chance(0.35):
            parts.append(self.relative())
        if self.chance(0.6):
            parts.append(self.date())
        if self.chance(0.45):
            parts.append(self.time())
        if self.chance(0.2):
            parts.append(self.pick(WEEKDAYS))
        if self.chance(0.2):
            parts.append(self.pick(NOISE))
        if self.chance(0.1):
            self.rng.shuffle(parts)
        if not parts or self.chance(0.03):
            parts.append(self.pick(["", ".", " . ", "now", "today", "+0"]))
        text = self.pick([" ", " ", " ", "  ", ", "]).join(parts)
        if self.chance(0.1):
            text = self.pick([" ", "\t"]) + text
        if self.chance(0.1):
            text += " "
        return text

    def case(self, clock):
        case = {"clock": clock, "answer": self.answer()}
        if self.chance(0.25):
            day = datetime.date(2026, 1, 1) + datetime.timedelta(days=self.rng.randint(-400, 800))
            case["default"] = f"{day:%Y-%m-%d} {self.rng.randint(0, 23):02d}:{self.pick([0, 15, 30, 59]):02d}"
        if self.chance(0.2):
            case["preferFuture"] = self.pick([False, "time"])
        return case


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=600)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    generator = Generator(rng)
    # A few clocks per run: Emacs starts once for each.
    clocks = []
    for _ in range(3):
        moment = datetime.datetime(2026, 1, 1) + datetime.timedelta(minutes=rng.randint(-400 * 1440, 800 * 1440))
        clocks.append(f"{moment:%Y-%m-%d %H:%M}")
    cases = reference([generator.case(rng.choice(clocks)) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-read-date-fuzz-"), "cases.json")
    save(out, cases)

    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "read_date"],
        cwd=ROOT,
        env=dict(os.environ, ORG_READ_DATE_CASES=out),
        capture_output=True,
        text=True,
    )
    errors = sum(1 for case in cases if case["expected"].startswith("ERROR:"))
    print(f"{args.count} cases ({errors} where Emacs signals an error), clocks {', '.join(clocks)}; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" dates differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
