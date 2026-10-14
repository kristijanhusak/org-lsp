#!/usr/bin/env python3
"""Differential fuzzing of TODO state changes against Emacs.

Generates random entries with planning lines, repeaters, drawers, notes
and parents that count their children, has Emacs change their state under
a fixed clock, and runs the test over the same cases. The cases stay in
the output file.

    scripts/fuzz-todo.py [--count 300] [--seed 1] [--out FILE]

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
from todo_reference import CURSOR, reference, save  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

REPEATERS = ["+1d", "+2d", "+1w", "+3w", "+1m", "+2m", "+1y", "++1d", "++1w", "++2m", "++1y", ".+1d", ".+3d", ".+1w",
             ".+1m", "+5h", "++3h", ".+2h", "+0d", ".+2d/4d", "++1w/2w"]
WARNINGS = ["-3d", "-0d", "-2w", "--2d", "--1w", "-1m"]
TITLES = ["a", "Call them", "review the report", "žć", "日本 語", "with [[https://x.org][link]]", "", "x  y"]
TAGS = ["", "", "", ":a:", ":work:home:", ":x_y:@z:", ":日本:"]
DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


class Generator:
    def __init__(self, rng, now):
        self.rng = rng
        self.now = now

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def date(self):
        spread = self.pick([2, 2, 10, 40, 400])
        day = self.now.date() + datetime.timedelta(days=self.rng.randint(-spread, spread))
        name = day.strftime("%a") if self.chance(0.9) else self.pick(DAYS)
        return day.strftime("%Y-%m-%d") + " " + name

    def stamp(self, active=True, repeat=0.5):
        body = self.date()
        if self.chance(0.4):
            body += " " + self.pick(["10:00", "09:30", "9:30", "14:00-15:30", "23:45", "00:00", "8:00-9:00"])
        if self.chance(repeat):
            # Org garbles the end of a time range when `.+Nh` moves it by
            # minutes; the server keeps the duration instead.
            choices = [r for r in REPEATERS if not (r.startswith(".+") and r.endswith("h") and "-" in body[11:])]
            body += " " + self.pick(choices)
        if self.chance(0.2):
            body += " " + self.pick(WARNINGS)
        open_, close = ("<", ">") if active else ("[", "]")
        return open_ + body + close

    def headline(self, level, keywords, cookie=False):
        line = "*" * level
        if self.chance(0.8):
            line += self.pick([" ", " ", " ", "  "]) + self.pick(keywords)
        if self.chance(0.2):
            line += " " + self.pick(["[#A]", "[#B]", "[#C]"])
        title = self.pick(TITLES)
        if cookie:
            title += " " + self.pick(["[/]", "[%]", "[0/0]", "[1/3]", "[50%]", "[/] [%]"])
        if title:
            line += self.pick([" ", " ", "  "]) + title
        tags = self.pick(TAGS)
        if tags:
            line += self.pick([" ", "   ", "\t", " " * 30]) + tags
        return line if len(line) > level else line + " "

    def entry(self, level, keywords, cookie=False):
        indent = self.pick(["", "", "", "  ", " " * (level + 1)])
        lines = [self.headline(level, keywords, cookie)]
        if self.chance(0.7):
            parts = []
            for keyword in self.rng.sample(["SCHEDULED:", "DEADLINE:", "CLOSED:"], self.pick([1, 1, 2, 3])):
                active = keyword != "CLOSED:"
                parts.append(keyword + self.pick([" ", " ", " ", "  "]) + self.stamp(active, 0.6 if active else 0))
            lines.append(indent + " ".join(parts))
        if self.chance(0.3):
            lines.append(indent + ":PROPERTIES:")
            names = ["ID", "LAST_REPEAT", "REPEAT_TO_STATE", "STYLE", "COOKIE_DATA", "LOGGING", "LOGGING", "LOG_INTO_DRAWER"]
            for key in self.rng.sample(names, self.pick([0, 1, 1, 2])):
                value = {
                    "ID": "abc",
                    "LAST_REPEAT": "[2026-01-01 Thu 09:00]",
                    "REPEAT_TO_STATE": self.pick(keywords + ["NOPE"]),
                    "STYLE": "habit",
                    "COOKIE_DATA": self.pick(["todo", "checkbox", "todo recursive"]),
                    "LOGGING": self.pick([
                        "nil", "logdone", "lognotedone", "logrepeat", "lognoterepeat nologdone",
                        f"{keywords[0]}(!)", f"{keywords[-1]}(@)", f"{keywords[0]}(t@/!) logdone",
                        "logdrawer", "NOPE(!)", f"{keywords[-1]}(x)",
                    ]),
                    "LOG_INTO_DRAWER": self.pick(["t", "nil", "NOTES"]),
                }[key]
                lines.append(indent + f":{key}: {value}")
            lines.append(indent + ":END:")
        if self.chance(0.15):
            lines.append(indent + ":NOTES:")
            lines.append(indent + "kept")
            lines.append(indent + ":END:")
        if self.chance(0.2):
            # State notes outside a drawer, maybe after a clock line.
            if self.chance(0.3):
                lines.append(indent + "CLOCK: [2026-01-02 Fri 09:00]--[2026-01-02 Fri 10:00] =>  1:00")
            for _ in range(self.pick([1, 2])):
                lines.append(indent + self.pick([
                    '- State "DONE"       from "TODO"       [2026-01-01 Thu 09:00]',
                    '- State "TODO"       from              [2026-01-03 Sat 09:00]',
                    '- Note taken on [2026-01-04 Sun 09:00] \\\\\n' + indent + '  text',
                ]))
        if self.chance(0.15):
            lines.append(indent + ":LOGBOOK:")
            if self.chance(0.6):
                lines.append(indent + '- State "DONE"       from "TODO"       [2026-01-01 Thu 09:00]')
            if self.chance(0.3):
                lines.append(indent + "CLOCK: [2026-01-01 Thu 09:00]--[2026-01-01 Thu 10:00] =>  1:00")
            lines.append(indent + ":END:")
        for _ in range(self.pick([0, 0, 1, 1, 2, 3])):
            roll = self.rng.random()
            if roll < 0.2:
                lines.append("")
            elif roll < 0.4:
                lines.append(indent + "- " + self.pick(["note", "item " + self.stamp(repeat=0.3)]))
            elif roll < 0.6:
                lines.append(indent + "Text with " + self.stamp(self.chance(0.8), 0.5) + " in it.")
            elif roll < 0.7:
                lines.append("# " + self.stamp())
            elif roll < 0.8:
                lines.append("#+begin_example\n" + self.stamp() + "\n#+end_example")
            else:
                lines.append(indent + self.pick(["plain text", "more", "1. numbered"]))
        return lines

    def case(self):
        lines = []
        keywords = ["TODO", "DONE"]
        options = {}
        if self.chance(0.35):
            definition = self.pick([
                "TODO NEXT | DONE",
                "TODO(t) WAIT(w@/!) | DONE(d!) CANCELLED(c@)",
                "TODO(t!) NEXT(n) | DONE(d)",
                "TODO | DONE(d@)",
                "A B C",
            ])
            lines.append("#+TODO: " + definition)
            keywords = [word.split("(")[0] for word in definition.split() if word != "|"]
        elif self.chance(0.2):
            options["org_todo_keywords"] = ["TODO", "NEXT(n!)", "|", "DONE", "GONE"]
            keywords = ["TODO", "NEXT", "DONE", "GONE"]
        if self.chance(0.2):
            lines.append("#+STARTUP: " + " ".join(self.rng.sample(
                ["logdone", "lognotedone", "nologdone", "logrepeat", "lognoterepeat", "nologrepeat",
                 "logdrawer", "nologdrawer", "overview", "logstatesreversed", "nologstatesreversed"],
                self.pick([1, 1, 2]))))
        if self.chance(0.3):
            lines.append("Text before the first headline.")

        # An optional parent, then siblings; the cursor goes into one of them.
        level = 1
        if self.chance(0.5):
            lines += self.entry(1, keywords, cookie=self.chance(0.8))
            level = self.pick([2, 2, 3])
        entries = [self.entry(level, keywords) for _ in range(self.rng.randint(1, 4))]
        target = self.rng.randrange(len(entries))
        for index, entry in enumerate(entries):
            if index == target:
                line = self.rng.randrange(len(entry))
                column = self.rng.randint(0, len(entry[line]))
                entry[line] = entry[line][:column] + CURSOR + entry[line][column:]
            lines += entry
            if self.chance(0.2):
                lines += self.entry(level + 1, keywords)
        text = "\n".join(lines) + ("\n" if self.chance(0.9) else "")
        if CURSOR not in text:
            text = text.replace("\n", CURSOR + "\n", 1) if "\n" in text else text + CURSOR

        if self.chance(0.4):
            options["org_log_done"] = self.pick(["time", "time", "note"])
        if self.chance(0.25):
            options["org_log_repeat"] = self.pick([False, "note", "time"])
        if self.chance(0.25):
            options["org_log_into_drawer"] = self.pick([True, True, "NOTES"])
        if self.chance(0.15):
            options["org_tags_column"] = self.pick([-80, -40, 0, 30, 77])
        if self.chance(0.15):
            options["org_todo_repeat_to_state"] = self.pick([True, keywords[0], keywords[-1]])
        if self.chance(0.35):
            options["org_adapt_indentation"] = self.pick([True, "headline-data", False])
        if self.chance(0.2):
            options["org_log_done_with_time"] = False
        if self.chance(0.3):
            options["org_log_states_order_reversed"] = False
        if self.chance(0.3):
            options["org_log_state_notes_insert_after_drawers"] = True
        if self.chance(0.25):
            options["org_log_note_headings"] = self.pick([
                {"state": "State %s from %S %t"},
                {"state": "%-8s <- %-8S at %d"},
                {"done": "Closed %D"},
                {"done": ""},
                {"state": "", "done": "Done %T"},
            ])
        roll = self.rng.random()
        command = "next" if roll < 0.5 else "previous" if roll < 0.65 else {"state": self.pick(keywords)}
        case = {"clock": self.now.strftime("%Y-%m-%d %H:%M"), "text": text, "command": command}
        if self.chance(0.4):
            case["note"] = self.pick(["A note", "two\nlines", "  padded  "])
        if options:
            case["options"] = options
        return case


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    # One clock per invocation: Emacs is started once for all cases.
    now = datetime.datetime(2026, 1, 1) + datetime.timedelta(minutes=rng.randint(0, 800 * 1440))
    generator = Generator(rng, now)
    cases = reference([generator.case() for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-todo-fuzz-"), "cases.json")
    save(out, cases)

    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "todo"],
        cwd=ROOT,
        env=dict(os.environ, ORG_TODO_CASES=out),
        capture_output=True,
        text=True,
    )
    errors = sum(1 for case in cases if case["expected"].startswith("ERROR:"))
    print(f"{args.count} cases ({errors} where Emacs stops with an error), clock {now:%Y-%m-%d %H:%M}; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" TODO changes differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
