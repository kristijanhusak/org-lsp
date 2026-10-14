#!/usr/bin/env python3
"""Differential fuzzing of the agenda against Emacs.

Generates random agenda files, has Emacs print the agenda for them under a
fixed clock, and runs the agenda test over the same cases. Cases the two
disagree on stay in the output directory.

    scripts/fuzz-agenda.py [--count 200] [--seed 1] [--out DIR] [--super] [--sorting] [--todo-options] [--skip-options] [--display-options]

With --super, each case also turns org-super-agenda on with random groups;
ORG_LSP_EMACS_PACKAGES must then name the directory of its packages (see
agenda_reference.py). With --sorting, each case sorts and limits its views
with random strategies (`org-agenda-sorting-strategy`) and limits
(`org-agenda-max-entries` and its kin). With --todo-options, each case
sets the options of the TODO list at random (`org-agenda-todo-ignore-*`,
`org-agenda-todo-list-sublevels`, `org-agenda-skip-comment-trees`,
`org-agenda-tags-todo-honor-ignore-options`). With --skip-options, the
options of what the agenda leaves out or where repeats show
(`org-agenda-skip-scheduled-if-deadline-is-shown` and its kin,
`org-agenda-skip-additional-timestamps-same-entry`,
`org-agenda-prefer-last-repeat`, `org-agenda-include-deadlines`). With
--display-options, those of how lines show their times
(`org-agenda-time-leading-zero` and its kin).

Requires Emacs, faketime and cargo.
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
from agenda_reference import ROOT, reference  # noqa: E402

WORDS = ["alpha", "beta", "Call", "review", "x", "the", "report", "žć", "日本", "a.b", "(c)", "it's", "100%", "$5"]
TIMES = ["10:00", "9:30", "09:30", "3pm", "8am-10am", "14:00-15:30", "8:15pm", "24:30", "12am", "7:5", "at 6", "13:00--14:00"]
TAGS = [":a:", ":work:home:", ":ARCHIVE:", ":x_y:@z:", ":a::b:", ":日本:", ":1:", " :spaced: ", ":a-b:"]
REPEATERS = ["+1d", "+2d", "+1w", "+2w", "+1m", "+3m", "+1y", "++1w", ".+3d", "+5h", "+30h", "+0d", ".+1m", "++2y"]
WARNINGS = ["-3d", "-0d", "-2w", "--2d", "-1m", "-10d", "--1w"]
DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def super_groups(rng):
    """Random `org-super-agenda-groups`: selectors with simple arguments,
    some negated, combined or grouped automatically."""
    simple = [
        lambda: {"time-grid": True},
        lambda: {"date": rng.choice([True, False, "today"])},
        lambda: {"deadline": rng.choice([True, "past", "today", "future", False])},
        lambda: {"scheduled": rng.choice([True, "past", "today", "future", False])},
        lambda: {"todo": rng.choice([True, False, "TODO", ["NEXT", "WAIT"]])},
        lambda: {"priority": rng.choice(["A", ["A", "B"]])},
        lambda: {rng.choice(["priority>", "priority<", "priority>=", "priority<="]): rng.choice(["A", "B", "C"])},
        lambda: {"tag": rng.choice(["a", "work", ["home", "x_y"], "file"])},
        lambda: {"category": rng.choice(["work", "home", "a", "notes", "prop"])},
        lambda: {"regexp": rng.choice(["review", "alpha|beta", "[0-9]+"])},
        lambda: {"heading-regexp": rng.choice(["^Call", "report", "x"])},
        lambda: {"effort<": rng.choice(["0:30", "1:00", "2h"])},
        lambda: {"effort>": rng.choice(["0:10", "1:00"])},
        lambda: {"property": rng.choice(["WHEN", ["agenda-group", "one"]])},
        lambda: {"children": rng.choice([True, False, "todo", "TODO"])},
        lambda: {"file-path": rng.choice(["a.org", "notes"])},
        lambda: {"anything": True},
    ]
    automatic = [
        {"auto-category": True}, {"auto-todo": True}, {"auto-priority": True}, {"auto-tags": True},
        {"auto-planning": True}, {"auto-ts": rng.choice([True, "reverse"])}, {"auto-group": True},
        {"auto-property": "agenda-group"}, {"auto-parent": True}, {"auto-outline-path": True},
        {"auto-dir-name": True},
    ]
    groups = []
    for _ in range(rng.randint(1, 4)):
        roll = rng.random()
        if roll < 0.15:
            group = dict(rng.choice(automatic))
        else:
            group = {}
            for _ in range(rng.choice([1, 1, 1, 2])):
                group.update(rng.choice(simple)())
            if rng.random() < 0.1:
                group = {"not": group} if rng.random() < 0.5 else {"and": group}
            if rng.random() < 0.6:
                group = {"name": rng.choice(["Group", "Today", "Later", False]), **group}
        if rng.random() < 0.3:
            group["order"] = rng.randint(-2, 5)
        if rng.random() < 0.1:
            group = {"discard": group}
        groups.append(group)
    return groups


STRATEGIES = [
    "time-up", "time-down", "timestamp-up", "timestamp-down", "scheduled-up", "scheduled-down", "deadline-up",
    "deadline-down", "tsia-up", "tsia-down", "ts-up", "ts-down", "stats-up", "stats-down", "priority-up",
    "priority-down", "urgency-up", "urgency-down", "effort-up", "effort-down", "category-up", "category-down",
    "category-keep", "tag-up", "tag-down", "todo-state-up", "todo-state-down", "habit-up", "habit-down", "alpha-up",
    "alpha-down",
]


def sorting_options(rng):
    """Random sorting strategies and limits, for org-lsp and as Emacs Lisp."""
    def strategies():
        return rng.sample(STRATEGIES, rng.randint(1, 4))

    options, forms = {}, []
    if rng.random() < 0.3:
        flat = strategies()
        options["org_agenda_sorting_strategy"] = flat
        forms.append(f"(setq org-agenda-sorting-strategy '({' '.join(flat)}))")
    else:
        views = {view: strategies() for view in rng.sample(["agenda", "todo", "tags", "search"], rng.randint(1, 4))}
        options["org_agenda_sorting_strategy"] = views
        alist = " ".join(f"({view} {' '.join(names)})" for view, names in views.items())
        forms.append(f"(setq org-agenda-sorting-strategy '({alist}))")
    for name in ("sort_notime_is_late", "sort_noeffort_is_high"):
        if rng.random() < 0.4:
            value = rng.random() < 0.5
            options[f"org_agenda_{name}"] = value
            forms.append(f"(setq org-agenda-{name.replace('_', '-')} {'t' if value else 'nil'})")
    for name in ("max_entries", "max_todos", "max_tags", "max_effort"):
        if rng.random() < 0.2:
            limit = lambda: rng.choice([-3, -1, 1, 2, 5]) * (30 if name == "max_effort" else 1)
            if rng.random() < 0.5:
                value = limit()
                options[f"org_agenda_{name}"] = value
                forms.append(f"(setq org-agenda-{name.replace('_', '-')} {value})")
            else:
                value = {view: limit() for view in rng.sample(["agenda", "todo", "tags", "search"], 2)}
                options[f"org_agenda_{name}"] = value
                pairs = " ".join(f"({view} . {limit})" for view, limit in value.items())
                forms.append(f"(setq org-agenda-{name.replace('_', '-')} '({pairs}))")
    return options, " ".join(forms)


def lisp_value(value):
    """A JSON option value as Emacs Lisp: t or nil, a symbol, a number."""
    if value is True:
        return "t"
    if value is False:
        return "nil"
    if isinstance(value, str):
        return f"'{value}"
    return str(value)


def todo_options(rng):
    """Random options of the TODO list, for org-lsp and as Emacs Lisp."""
    options, forms = {}, []

    def choose(name, values, probability):
        if rng.random() < probability:
            value = rng.choice(values)
            options[f"org_{name}"] = value
            forms.append(f"(setq org-{name.replace('_', '-')} {lisp_value(value)})")

    choose("agenda_skip_comment_trees", [False], 0.3)
    choose("agenda_todo_list_sublevels", [False], 0.4)
    choose("agenda_todo_ignore_with_date", [True], 0.2)
    dated = [True, "all", "future", "past", 0, 2, 30, -1, -10]
    choose("agenda_todo_ignore_scheduled", dated, 0.5)
    choose("agenda_todo_ignore_deadlines", [True, "near", "far", "all", "future", "past", 0, 3, -2], 0.5)
    choose("agenda_todo_ignore_timestamp", dated, 0.4)
    choose("agenda_todo_ignore_time_comparison_use_seconds", [True], 0.3)
    choose("agenda_tags_todo_honor_ignore_options", [True], 0.5)
    choose("deadline_warning_days", [0, 3, 30], 0.3)
    return options, " ".join(forms)


def skip_options(rng):
    """Random options of what the day agenda leaves out, for org-lsp and as Emacs Lisp."""
    options, forms = {}, []

    def choose(name, values, probability, lisp=None):
        if rng.random() < probability:
            value = rng.choice(values)
            options[f"org_{name}"] = value
            text = lisp(value) if lisp else lisp_value(value)
            forms.append(f"(setq org-{name.replace('_', '-')} {text})")

    choose("agenda_include_deadlines", [False], 0.15)
    choose(
        "agenda_prefer_last_repeat",
        [True, ["TODO"], ["DONE", "NEXT"]],
        0.4,
        lambda value: lisp_value(value) if value is True else "'(" + " ".join(f'"{state}"' for state in value) + ")",
    )
    choose("agenda_skip_deadline_prewarning_if_scheduled", [True, "pre-scheduled", 0, 3, 10], 0.5)
    choose("agenda_skip_scheduled_delay_if_deadline", [True, "post-deadline", 2], 0.4)
    choose("agenda_skip_scheduled_if_deadline_is_shown", [True, "not-today", "repeated-after-deadline"], 0.5)
    choose("agenda_skip_scheduled_repeats_after_deadline", [True], 0.3)
    choose("agenda_skip_scheduled_if_done", [True], 0.2)
    choose("agenda_skip_deadline_if_done", [True], 0.2)
    choose("agenda_skip_additional_timestamps_same_entry", [True], 0.5)
    return options, " ".join(forms)


def display_options(rng):
    """Random options of how the agenda shows times, for org-lsp and as Emacs Lisp."""
    options, forms = {}, []

    def choose(name, values, probability):
        if rng.random() < probability:
            value = rng.choice(values)
            options[f"org_{name}"] = value
            text = '"' + value.replace("\\", "\\\\") + '"' if name == "agenda_diary_sexp_prefix" else lisp_value(value)
            forms.append(f"(setq org-{name.replace('_', '-')} {text})")

    choose("agenda_time_leading_zero", [True], 0.4)
    choose("agenda_timegrid_use_ampm", [True], 0.4)
    choose("agenda_search_headline_for_time", [False], 0.3)
    choose("agenda_remove_times_when_in_prefix", [False, "beg"], 0.4)
    choose("agenda_default_appointment_duration", [30, 60, 90], 0.4)
    choose("agenda_remove_timeranges_from_blocks", [True], 0.4)
    choose("agenda_diary_sexp_prefix", ["^[A-Za-z]+ ", "^X"], 0.3)
    views = [["todo"], ["agenda"], ["search", "todo"], ["agenda", "search"]]
    symbols = lambda value: "'(" + " ".join(value) + ")" if isinstance(value, list) else lisp_value(value)
    for name, values in (("show_inherited_tags", [False, "always", *views]), ("use_tag_inheritance", [True, False, *views])):
        if rng.random() < 0.3:
            value = rng.choice(values)
            options[f"org_agenda_{name}"] = value
            forms.append(f"(setq org-agenda-{name.replace('_', '-')} {symbols(value)})")
    if rng.random() < 0.3:
        value = rng.choice(["^f", "a", "^[xy]", "日本"])
        options["org_agenda_hide_tags_regexp"] = value
        forms.append(f'(setq org-agenda-hide-tags-regexp "{value}")')
    return options, " ".join(forms)


def search_string(rng):
    """A search string for `org-search-view`: a phrase, or Boolean words,
    regexps and quoted phrases, with the `*`, `!` and `:` prefixes."""
    words = [rng.choice(WORDS) for _ in range(rng.randint(1, 3))]
    if rng.random() < 0.5:
        parts = []
        for word in words:
            sign = rng.choice(["", "+", "+", "-"])
            kind = rng.random()
            if kind < 0.2:
                word = "{" + rng.choice(["^\\*+ TODO", "al.ha", "[0-9]+", "re[a-z]*", "^call"]) + "}"
            elif kind < 0.3:
                word = '"' + word + " " + rng.choice(WORDS) + '"'
            parts.append(sign + word)
        string = " ".join(parts)
        if not string.startswith(("+", "-", "{")):
            string = "+" + string
    else:
        string = " ".join(words)
    prefix = rng.choice(["", "", "", "*", "!", ":", "*!", "*:"])
    return prefix + string


class Generator:
    def __init__(self, rng, today):
        self.rng = rng
        self.today = today

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, probability):
        return self.rng.random() < probability

    def date(self):
        spread = self.pick([3, 3, 10, 10, 40, 400])
        day = self.today + datetime.timedelta(days=self.rng.randint(-spread, spread))
        text = day.strftime("%Y-%m-%d")
        if self.chance(0.85):
            # Org does not check the day name, so a wrong one must not matter.
            text += " " + (day.strftime("%a") if self.chance(0.9) else self.pick(DAY_NAMES))
        return text

    def stamp(self, active=True, planning=False):
        body = self.date()
        if self.chance(0.4):
            body += " " + self.pick(["10:00", "09:30", "9:30", "14:00-15:30", "23:45", "00:00", "25:00", "8:00-9:00"])
        if self.chance(0.35):
            body += " " + self.pick(REPEATERS)
        if self.chance(0.25 if planning else 0.1):
            body += " " + self.pick(WARNINGS)
        if self.chance(0.05):
            body += self.pick([" ", "  x", " +1w -2d +3d"])
        open_, close = ("<", ">") if active else ("[", "]")
        return open_ + body + close

    def range(self):
        # Inactive ranges, and those with a bracket of each kind, for
        # `org-agenda-include-inactive-timestamps`.
        first = self.stamp(active=self.chance(0.8))
        return first + self.pick(["--", "--", "-", "---"]) + self.stamp(active=self.chance(0.8))

    def title(self):
        parts = []
        for _ in range(self.rng.randint(0, 4)):
            roll = self.rng.random()
            if roll < 0.6:
                parts.append(self.pick(WORDS))
            elif roll < 0.75:
                parts.append(self.pick(TIMES))
            elif roll < 0.85:
                parts.append(self.stamp(active=self.chance(0.8)))
            elif roll < 0.9:
                parts.append(self.range())
            else:
                parts.append(self.pick(["[#A]", "[#C]", "[1/2]", "[[link][desc]]", "*bold*", "COMMENT", "::", "[#1]",
                                        "[#a]", "[#64]", "[#65]", "[[plain]]", "https://example.com/x"]))
        return " ".join(parts)

    def headline(self, keywords):
        line = "*" * self.pick([1, 1, 1, 2, 2, 3])
        if self.chance(0.6):
            line += self.pick([" ", " ", " ", " ", "  ", " \t"]) + self.pick(keywords)
            if self.chance(0.03):
                line += "\t"
        if self.chance(0.2):
            line += self.pick([" [#A]", " [#B]", " [#C]", " [#Z]", "  [#A]", " [#a]", " [#7]", " [#40]", "\t[#A]"])
        if self.chance(0.06):
            line += " COMMENT"
        title = self.title()
        if title or self.chance(0.9):
            line += " " + title
        if self.chance(0.3):
            line += self.pick([" ", "   ", "\t"]) + self.pick(TAGS)
        return line

    def planning(self):
        parts = []
        for _ in range(self.pick([1, 1, 1, 2, 2, 3])):
            keyword = self.pick(["SCHEDULED:", "SCHEDULED:", "DEADLINE:", "DEADLINE:", "CLOSED:"])
            stamp = self.stamp(active=(keyword != "CLOSED:") == self.chance(0.93), planning=True)
            if self.chance(0.04):
                stamp = self.range()
            parts.append(keyword + self.pick([" ", " ", " ", " ", " ", "  ", "", "\t"]) + stamp)
        return self.pick(["", "", "", "  "]) + " ".join(parts)

    def body_line(self):
        roll = self.rng.random()
        if roll < 0.25:
            return self.pick(WORDS) + " " + self.stamp(active=self.chance(0.8)) + " " + self.pick(WORDS)
        if roll < 0.35:
            return self.range() + " " + self.pick(WORDS)
        if roll < 0.45:
            return "- " + self.pick(WORDS) + " " + self.stamp()
        if roll < 0.5:
            return "# " + self.stamp()
        if roll < 0.56:
            return "#+begin_src org\n" + self.stamp() + "\n" + self.range() + "\n#+end_src"
        if roll < 0.6:
            return "#+begin_example\n" + self.stamp() + " " + self.range() + "\n#+end_example"
        if roll < 0.65:
            return "| " + self.stamp() + " | " + self.pick(WORDS) + " |"
        if roll < 0.7:
            return ":LOGBOOK:\n" + self.pick([
                "- note " + self.stamp(active=False) + " " + self.stamp(),
                "CLOCK: " + self.stamp(active=False) + "--" + self.stamp(active=False) + " =>  1:00",
                '- State "DONE"       from "TODO"       ' + self.stamp(active=False),
            ]) + "\n:END:"
        if roll < 0.74:
            return "#+KEY: " + self.stamp()
        if roll < 0.78:
            return ": " + self.stamp()
        if roll < 0.82:
            return self.planning()
        if roll < 0.86:
            return ""
        if roll < 0.92:
            # What entry text (`E`) cleans up: indentation, tabs, clock
            # lines, drawers without an end.
            return self.pick(["  ", "    ", "\t", " \t ", "", "   "]) + self.pick([
                " ".join(self.pick(WORDS) for _ in range(2)),
                "CLOCK: [2026-01-01 Thu 09:00]--[2026-01-01 Thu 10:00] =>  1:00",
                ":NOTE:\n" + self.pick(WORDS),
                ":end:",
                "closed: " + self.stamp(active=False),
                "",
            ])
        return " ".join(self.pick(WORDS) for _ in range(self.rng.randint(1, 4)))

    def document(self):
        lines = []
        keywords = ["TODO", "DONE"]
        if self.chance(0.3):
            lines.append("#+TODO: TODO NEXT(n) WAIT | DONE CANCELLED")
            keywords = ["TODO", "NEXT", "WAIT", "DONE", "CANCELLED"]
        if self.chance(0.3):
            lines.append("#+CATEGORY: " + self.pick(["work", "home", "日本語", "a-very-long-category", "x"]))
        if self.chance(0.25):
            lines.append("#+FILETAGS: " + self.pick([":file:", ":f1:f2:", "f3 :f4:", ":a:"]))
        if self.chance(0.03):
            lines.append("#+FILETAGS: :ARCHIVE:")
        if self.chance(0.1):
            lines = [":PROPERTIES:", ":CATEGORY: " + self.pick(["top", "nil", ""]), ":END:"] + lines
        if self.chance(0.2):
            lines.append(self.body_line())
        for _ in range(self.rng.randint(1, 12)):
            lines.append(self.headline(keywords + ["TODO", "TODO"]))
            if self.chance(0.6):
                lines.append(self.planning())
            if self.chance(0.2):
                drawer = [":PROPERTIES:"]
                if self.chance(0.6):
                    drawer.append(self.pick([":CATEGORY: ", ":CATEGORY: ", ":CATEGORY: ", ":category: ", ":CATEGORY+: "])
                                  + self.pick(["prop", "nil", "cat2", ""]))
                if self.chance(0.5):
                    drawer.append(":WHEN: " + self.stamp(active=self.chance(0.8)))
                if self.chance(0.3):
                    drawer.append(":agenda-group: " + self.pick(["one", "two", "One"]))
                if self.chance(0.3):
                    drawer.append(":Effort: " + self.pick(["0:15", "1:00", "2:30", "1h", "junk"]))
                drawer.append(":END:")
                lines.extend(drawer)
            for _ in range(self.pick([0, 0, 0, 1, 1, 2, 3])):
                lines.append(self.body_line())
            if self.chance(0.1):
                lines.append("#+CATEGORY: " + self.pick(["late", "work"]))
        return "\n".join(lines) + ("\n" if self.chance(0.95) else "")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    parser.add_argument("--super", action="store_true", help="group the views with org-super-agenda")
    parser.add_argument("--sorting", action="store_true", help="random sorting strategies and limits")
    parser.add_argument("--todo-options", action="store_true", help="random options of the TODO list")
    parser.add_argument("--skip-options", action="store_true", help="random options of what the agenda leaves out")
    parser.add_argument("--display-options", action="store_true", help="random options of how times show")
    args = parser.parse_args()

    out = args.out or tempfile.mkdtemp(prefix="org-agenda-fuzz-")
    os.makedirs(out, exist_ok=True)
    rng = random.Random(args.seed)
    # One clock per invocation: Emacs is started once for all cases.
    today = datetime.date(2026, 1, 1) + datetime.timedelta(days=rng.randint(0, 800))
    now = f"{rng.randint(0, 23):02}:{rng.randint(0, 59):02}"
    generator = Generator(rng, today)

    directories = []
    for index in range(args.count):
        directory = os.path.join(out, f"case-{args.seed}-{index:04}")
        os.makedirs(directory, exist_ok=True)
        for name in rng.sample(["a.org", "b.org", "notes.org"], rng.randint(1, 3)):
            with open(os.path.join(directory, name), "w", encoding="utf-8") as handle:
                handle.write(generator.document())
        offset = rng.randint(-12, 12)
        other = (today + datetime.timedelta(days=offset)).isoformat()
        case = {
            "today": today.isoformat(),
            "now": now,
            "runs": [
                {"view": "agenda", "span": "week"},
                {"view": "agenda", "span": "day"},
                {"view": "agenda", "span": rng.choice(["day", "week", 3, "fortnight"]), "start": other},
                {"view": "todo"},
                {"view": rng.choice(["search", "search-todo"]), "match": search_string(rng)},
                {"view": "todo", "entry_text": rng.choice([1, 2, 5, 9])},
                {"view": "agenda", "span": rng.choice(["day", "week"]), "start": other, "inactive": True},
            ],
        }
        if args.super:
            case["options"] = {"org_super_agenda_mode": True, "org_super_agenda_groups": super_groups(rng)}
            if rng.random() < 0.3:
                case["options"]["org_super_agenda_hide_empty_groups"] = True
            if rng.random() < 0.3:
                case["options"]["org_super_agenda_keep_order"] = True
        if args.sorting:
            options, emacs = sorting_options(rng)
            case.setdefault("options", {}).update(options)
            case["emacs"] = emacs
            case["runs"].append({"view": rng.choice(["tags", "tags-todo"]), "match": rng.choice(["LEVEL>0", "a|work", "-home"])})
        if args.display_options:
            options, emacs = display_options(rng)
            case.setdefault("options", {}).update(options)
            case["emacs"] = " ".join(part for part in (case.get("emacs", ""), emacs) if part)
        if args.skip_options:
            options, emacs = skip_options(rng)
            case.setdefault("options", {}).update(options)
            case["emacs"] = " ".join(part for part in (case.get("emacs", ""), emacs) if part)
        if args.todo_options:
            options, emacs = todo_options(rng)
            case.setdefault("options", {}).update(options)
            case["emacs"] = " ".join(part for part in (case.get("emacs", ""), emacs) if part)
            case["runs"].append({"view": "tags-todo", "match": rng.choice(["LEVEL>0", "a|work", "-home", "/TODO|NEXT"])})
        with open(os.path.join(directory, "case.json"), "w", encoding="utf-8") as handle:
            json.dump(case, handle)
        directories.append(directory)

    reference(directories)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-lsp", "--test", "agenda"],
        cwd=ROOT,
        env=dict(os.environ, ORG_AGENDA_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases, clock {today} {now}; files in {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find(" agenda runs differ")
        start = report.rfind("\n", 0, start) + 1 if start >= 0 else 0
        end = report.find("stack backtrace", start)
        print(report[start:end if end >= 0 else None].rstrip())
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
