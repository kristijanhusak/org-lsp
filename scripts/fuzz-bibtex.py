#!/usr/bin/env python3
"""Differential fuzzing of `ol-bibtex.el` against Emacs.

Random BibTeX entries (types known and not, keys or none, braces, quotes,
`#` joins, constants, missing commas, `OPT` fields, keywords) yanked or
imported into random Org documents; random headlines with BibTeX
properties, prefixed or not, with tags, copied as BibTeX or exported;
`org-bibtex-check` and `org-bibtex-create` answered at random. The options
of `ol-bibtex.el` at random, and the cursor anywhere.

    scripts/fuzz-bibtex.py [--count 300] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from bibtex_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
TYPES = ["article", "Book", "misc", "INPROCEEDINGS", "techreport", "online", "string", "unpublished", "inbook"]
FIELDS = ["author", "title", "journal", "year", "publisher", "editor", "note", "keywords", "pages", "volume",
          "OPTnote", "ALTauthor", "url", "doi", "chapter", "booktitle", "howpublished", "foo"]
WORDS = ["Smith", "J. Doe", "and", "The", "{Great}", "Title", "of", "x", "1999", "2020", "\\'e", "Émile",
         "a, b", "multi\n   line", "tag one", "c-d!", "jan"]
TAGS = ["t1", "bib", "ref", "skip", "x_y", "@home"]


def words(rng, low=1, high=4):
    return " ".join(rng.choice(WORDS) for _ in range(rng.randint(low, high)))


def value(rng):
    roll = rng.random()
    if roll < 0.45:
        return "{" + words(rng) + "}"
    if roll < 0.7:
        return '"' + words(rng).replace('"', "") + '"'
    if roll < 0.8:
        return str(rng.randint(1, 3000))
    if roll < 0.9:
        return '"' + words(rng) + '" # ' + rng.choice(["jan", "{b}", '"c"'])
    return rng.choice(["jan", "{{Nested} braces}", "{}"])


def entry(rng):
    kind = rng.choice(TYPES)
    key = rng.choice(["k1", "doe:2020", "", "Key_2"]) if rng.random() < 0.95 else ""
    open_, close = ("(", ")") if rng.random() < 0.1 else ("{", "}")
    fields = []
    for _ in range(rng.randint(0, 6)):
        fields.append(f"{rng.choice(FIELDS)} = {value(rng)}")
    if rng.random() < 0.7 and not any(field.startswith("title") for field in fields):
        fields.insert(0, f"title = {{{words(rng)}}}")
    text = f"@{kind}{open_}{key}"
    for field in fields:
        text += (",\n  " if rng.random() < 0.9 else "\n  ") + field
    text += ("," if rng.random() < 0.2 else "") + f"\n{close}"
    return text


def bib(rng):
    parts = [entry(rng) for _ in range(rng.randint(0, 3))]
    if rng.random() < 0.2:
        parts.insert(0, "% comment line")
    text = "\n\n".join(parts)
    return text + ("\n" if rng.random() < 0.8 else "")


def headline(rng, level):
    parts = []
    if rng.random() < 0.2:
        parts.append(rng.choice(["TODO", "DONE"]))
    if rng.random() < 0.1:
        parts.append("[#A]")
    if rng.random() < 0.1:
        parts.append("COMMENT")
    parts.append(words(rng, 0, 3).replace("\n", " "))
    line = "*" * level + " " + " ".join(part for part in parts if part)
    if rng.random() < 0.3:
        line += " :" + ":".join(rng.sample(TAGS, rng.randint(1, 2))) + ":"
    return line


def properties(rng, prefix):
    lines = []
    if rng.random() < 0.7:
        name = rng.choice(["BTYPE", "btype", prefix + "BTYPE"])
        lines.append(f":{name}: {rng.choice(['article', 'misc', 'book', 'Article', 'inbook', 'weird'])}")
    for _ in range(rng.randint(0, 4)):
        name = rng.choice(["AUTHOR", "TITLE", "YEAR", "JOURNAL", "CUSTOM_ID", "EDITOR", "KEYWORDS", "FOO", "PAGES"])
        if rng.random() < 0.3:
            name = prefix + name
        lines.append(f":{name}: {words(rng).replace(chr(10), ' ')}")
    return [":PROPERTIES:", *lines, ":END:"] if lines else []


def document(rng, prefix):
    lines = []
    if rng.random() < 0.15:
        lines.append("#+FILETAGS: :file:")
    if rng.random() < 0.2:
        lines.append("intro")
    level = 1
    for index in range(rng.randint(0, 4)):
        level = max(1, min(3, level + rng.choice([-1, 0, 1] if index else [0])))
        lines.append(headline(rng, level))
        lines += properties(rng, prefix)
        if rng.random() < 0.3:
            lines.append("body text")
    text = "\n".join(lines)
    return text if rng.random() < 0.2 else text + "\n"


def options(rng):
    chosen = {}
    prefix = ""
    if rng.random() < 0.3:
        prefix = "BIB_"
        chosen["org-bibtex-prefix"] = '"BIB_"'
        if rng.random() < 0.5:
            chosen["org-bibtex-export-arbitrary-fields"] = "t"
    for name in ["org-bibtex-tags-are-keywords", "org-bibtex-inherit-tags"]:
        if rng.random() < 0.3:
            chosen[name] = "t"
    if rng.random() < 0.2:
        chosen["org-bibtex-treat-headline-as-title"] = "nil"
    if rng.random() < 0.2:
        chosen["org-bibtex-tags"] = '\'("bib")'
    if rng.random() < 0.2:
        chosen["org-bibtex-no-export-tags"] = '\'("skip")'
    if rng.random() < 0.1:
        chosen["org-bibtex-key-property"] = '"ID"'
    if rng.random() < 0.15:
        chosen["org-adapt-indentation"] = "t"
    return chosen, prefix


def case(rng):
    chosen, prefix = options(rng)
    text = document(rng, prefix)
    command = rng.choice(["yank", "yank", "import", "copy", "copy", "export", "check", "check-all", "create",
                          "create-here"])
    starts = [0] + [index + 1 for index, c in enumerate(text) if c == "\n"]
    point = rng.choice(starts) if rng.random() < 0.7 else rng.randint(0, len(text))
    out = {"command": command, "text": text, "point": point, "options": chosen}
    if command == "yank":
        out["clipboard"] = bib(rng)
        out["optional"] = rng.random() < 0.2
    if command == "import":
        # Emacs reads the file as raw bytes (`find-file-noselect` with
        # RAWFILE): beyond ASCII it inserts them, where org-lsp reads UTF-8.
        out["bib"] = bib(rng).replace("É", "E")
    if command in ("check", "create", "create-here"):
        out["optional"] = rng.random() < 0.3
    if command in ("check", "check-all", "create", "create-here"):
        answers = []
        if command.startswith("create"):
            answers.append(rng.choice(["article", "book", "misc", "inbook", "techreport", "Article", "x"]))
        answers += [rng.choice(["", "A", "x y", " pad ", "author", "editor", "Pages"]) for _ in range(30)]
        out["answers"] = answers
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-bibtex-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "bibtex"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_BIBTEX_CASES=out), capture_output=True, text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:12000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
