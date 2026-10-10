#!/usr/bin/env python3
"""Differential fuzzing of org-roam's database against Emacs.

Generates random directories of Org files with nodes, aliases, refs,
tags, links and citations, has org-roam sync its database over them, and
runs the roam test over the same cases. Cases the two disagree on are
reported; all of them stay in the output file.

    scripts/fuzz-roam.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs, org-roam (see roam_reference.py) and cargo.
"""

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from roam_reference import reference  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

WORDS = ["alpha", "Beta", "x", "the note", "žć", "日本", "a.b", "(c)", "it's", "100%", "$5", "Ünï", "Ça"]
TAGS = ["a", "work", "x_y", "@home", "b", "日本"]
NAMES = ["a.org", "b.org", "notes.org", "sub/deep.org", "sub/x/y.org", "drafts/d.org", "data/att.org",
         "other.txt", "c.org.gpg", "e.org~"]


class Generator:
    def __init__(self, rng):
        self.rng = rng
        self.ids = []
        self.counter = 0

    def pick(self, items):
        return self.rng.choice(items)

    def chance(self, p):
        return self.rng.random() < p

    def new_id(self):
        if self.ids and self.chance(0.04):
            # Now and then the same ID twice.
            return self.pick(self.ids)
        self.counter += 1
        value = self.pick(["id-", "", "X", "node-"]) + f"{self.rng.randint(0, 999):03}-{self.counter}"
        self.ids.append(value)
        return value

    def some_id(self):
        return self.pick(self.ids) if self.ids and self.chance(0.8) else "missing-" + str(self.rng.randint(0, 9))

    def link(self):
        roll = self.rng.random()
        if roll < 0.3:
            target = "id:" + self.some_id() + (self.pick(["::*Head", "::#c", "::text"]) if self.chance(0.15) else "")
            return f"[[{target}]" + (f"[{self.pick(WORDS)}]" if self.chance(0.6) else "") + "]"
        if roll < 0.45:
            return self.pick(["https://example.com/p", "http://Ex.org/A?b=1#c", "https://a.b", "mailto:a@b.c"])
        if roll < 0.55:
            return f"[[roam:{self.pick(WORDS)}]]"
        if roll < 0.62:
            return f"[[{self.pick(WORDS)}]]"
        if roll < 0.7:
            return f"[[file:{self.pick(['a.org', 'sub/deep.org', 'x y.org'])}{self.pick(['', '::*H', '::123'])}][f]]"
        if roll < 0.75:
            return "<https://angle.org/x y>"
        if roll < 0.8:
            return f"[[#{self.pick(['cid', 'x'])}]]"
        if roll < 0.85:
            return f"[[./rel.org][rel]]"
        if roll < 0.9:
            return "[[https://ex.com/a b][spaced]]"
        return f"[[*{self.pick(WORDS)}]]"

    def citation(self):
        keys = [self.pick(["smith2020", "doe:21", "k1", "a_b"]) for _ in range(self.rng.randint(1, 3))]
        refs = ";".join(self.pick(["", "see ", " "]) + "@" + key + self.pick(["", " p. 3", ""]) for key in keys)
        return f"[cite{self.pick(['', '/t', '/n/b'])}:{refs}]"

    def stamp(self):
        date = f"2026-{self.rng.randint(1, 12):02}-{self.rng.randint(1, 28):02}"
        body = date + self.pick(["", " Mon", " Tue 10:00", " 25:00", " Wed 9:30-10:00"])
        if self.chance(0.1):
            return f"<{body}>--<{date}>"
        return ("<" + body + ">") if self.chance(0.6) else ("[" + body + "]")

    def words(self):
        parts = []
        for _ in range(self.rng.randint(1, 4)):
            roll = self.rng.random()
            if roll < 0.25:
                parts.append(self.link())
            elif roll < 0.32:
                parts.append(self.citation())
            elif roll < 0.4:
                parts.append(self.stamp())
            else:
                parts.append(self.pick(WORDS))
        return " ".join(parts)

    def aliases(self):
        items = []
        for _ in range(self.rng.randint(1, 3)):
            word = self.pick(WORDS)
            items.append(f'"{word} two"' if self.chance(0.4) else word.replace(" ", "_"))
        if self.chance(0.05):
            items.append('"unclosed')
        if self.chance(0.05):
            items.append('"with \\"quote\\""')
        return " ".join(items)

    def refs(self):
        items = []
        for _ in range(self.rng.randint(1, 3)):
            items.append(self.pick([
                "https://example.com/r", "@smith2020", "[cite:@doe2021]", "[cite:@a;@b]",
                "[[https://ex.org/p][desc]]", "http://Host.COM/A", "cite:k9", "notalink", "id:abc",
                "https://x.org/a%20b", "\"https://q.org/a b\"",
            ]))
        return " ".join(items)

    def drawer(self, file_level=False):
        lines = [":PROPERTIES:"]
        if self.chance(0.9):
            lines.append(":ID:       " + self.new_id())
        if self.chance(0.3):
            lines.append(":ROAM_ALIASES: " + self.aliases())
        if self.chance(0.3):
            lines.append(":ROAM_REFS: " + self.refs())
        if self.chance(0.08):
            lines.append(":ROAM_EXCLUDE: " + self.pick(["t", "nil"]))
        if self.chance(0.15):
            lines.append(":CATEGORY: " + self.pick(["cat", "nil", "x y"]))
        if self.chance(0.2):
            lines.append(":" + self.pick(["CUSTOM", "Other", "LINKED", "TODO", "ITEM"]) + ": " +
                         self.pick(["value", self.link(), self.stamp(), ""]))
        if self.chance(0.08):
            lines.append(":CUSTOM+: more")
        lines.append(":END:")
        return lines

    def heading(self, level):
        line = "*" * level + " "
        if self.chance(0.4):
            line += self.pick(["TODO", "DONE", "NEXT"]) + self.pick([" ", " ", "\t"])
        if self.chance(0.2):
            line += self.pick(["[#A] ", "[#a] ", "[#12] ", "[#C] ", "[#99] "])
        if self.chance(0.95):
            line += self.words()
        if self.chance(0.3):
            line += self.pick([" ", "   ", "\t"]) + ":" + ":".join(self.rng.sample(TAGS, self.rng.randint(1, 2))) + ":"
        return line

    def body(self):
        roll = self.rng.random()
        if roll < 0.4:
            return self.words()
        if roll < 0.5:
            return "- " + self.words()
        if roll < 0.55:
            return "#+begin_src org\n" + self.link() + "\n#+end_src"
        if roll < 0.6:
            return "# " + self.link()
        if roll < 0.65:
            return "| " + self.link() + " | " + self.pick(WORDS) + " |"
        if roll < 0.7:
            return "#+" + self.pick(["foo", "transclude", "caption"]) + ": " + self.link()
        if roll < 0.75:
            return ":LOGBOOK:\n- note " + self.stamp() + "\n:END:"
        if roll < 0.8:
            return "CLOCK: [2026-01-01 Thu 09:00]--[2026-01-01 Thu 10:00] =>  1:00"
        if roll < 0.85:
            return ": " + self.link()
        return ""

    def planning(self):
        parts = []
        for keyword in self.rng.sample(["SCHEDULED:", "DEADLINE:", "CLOSED:"], self.rng.randint(1, 2)):
            parts.append(keyword + " " + self.stamp())
        return " ".join(parts)

    def document(self):
        lines = []
        if self.chance(0.1):
            lines.append("# a comment line")
        if self.chance(0.6):
            lines.extend(self.drawer(True))
        for _ in range(self.pick([0, 1, 1, 1, 2])):
            lines.append("#+" + self.pick(["title", "TITLE", "Title"]) + ": " + self.words())
        if self.chance(0.3):
            lines.append("#+filetags: :" + ":".join(self.rng.sample(TAGS, self.rng.randint(1, 3))) + ":")
        if self.chance(0.2):
            lines.append("#+TODO: TODO NEXT | DONE")
        if self.chance(0.3):
            lines.append(self.body())
        level = 1
        for _ in range(self.rng.randint(0, 6)):
            level = max(1, min(4, level + self.pick([-1, 0, 0, 1, 1])))
            lines.append(self.heading(level))
            if self.chance(0.3):
                lines.append(self.planning())
            if self.chance(0.7):
                lines.extend(self.drawer())
            for _ in range(self.pick([0, 1, 1, 2])):
                lines.append(self.body())
        return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = []
    for _ in range(args.count):
        generator = Generator(rng)
        names = rng.sample(NAMES, rng.randint(1, 4))
        case = {"files": {name: generator.document() for name in names}}
        if rng.random() < 0.3:
            case["options"] = {"org_roam_file_exclude_regexp": rng.choice([["^drafts/"], [], ["data/", "sub/x"]])}
        if rng.random() < 0.15:
            case.setdefault("options", {})["org_tags_exclude_from_inheritance"] = ["work"]
        if rng.random() < 0.5:
            case["templates"] = [rng.choice(["${title}", "${title:*} ${tags:10}", "${title} ${olp} ${todo}",
                                             "${file} ${title:20}|"])]
        if rng.random() < 0.3:
            case["titles"] = [generator.words() for _ in range(3)]
        cases.append(case)
    reference(cases)
    out = args.out or tempfile.mkstemp(prefix="org-roam-fuzz-", suffix=".json")[1]
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "roam"],
        cwd=ROOT,
        env=dict(os.environ, ORG_ROAM_CASES=out),
        capture_output=True,
        text=True,
    )
    print(f"{args.count} cases in {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("differences from Emacs")
        print(report[max(0, start - 10):report.find("stack backtrace") if "stack backtrace" in report else None][:20000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
