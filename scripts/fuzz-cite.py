#!/usr/bin/env python3
"""Differential fuzzing of reading bibliographies against Emacs (Org's
`oc-basic`): random BibTeX files (entry types in any case, parentheses,
`@String`s, constants and `#`, nested braces, malformed entries,
duplicate keys) and CSL-JSON files, named by `#+BIBLIOGRAPHY:` lines.

    scripts/fuzz-cite.py [--count 200] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from cite_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
TYPES = ["article", "Book", "BOOK", "misc", "InProceedings", "booklet", "online", "phdthesis", "TechReport",
         "foo", "Comment", "unpublished", "MVBook", "report"]
KEYS = ["knuth1984", "doe2020", "Doe2020", "smith:19", "a-b_c", "x.y", "lamport94", "k1", "Lamport94", "été"]
NAMES = ["author", "editor", "title", "journal", "publisher", "institution", "school", "year", "date", "month",
         "note", "OPTnote", "ALTauthor", "Title", "AUTHOR", "optional", "url"]
WORDS = ["Knuth, Donald E.", "John Doe and Jane   Roe", "{The} {\\TeX}book", "A {Study} of\n    Things",
         "J. of \\emph{Stuff}", "2020", "2020-05-01", "1999", "{ACM}", "Ünïcode Titlé", "Lamport, L. and Others, A.",
         "", "Smith", "Word with \"quote\""]
CONSTS = ["1984", "jan", "acm", "undefined", "2020", "may", "cacm"]


def value(rng):
    roll = rng.random()
    if roll < 0.45:
        return "{" + rng.choice(WORDS).replace('"', "'") + "}"
    if roll < 0.7:
        return '"' + rng.choice(WORDS).replace('"', "") + '"'
    if roll < 0.85:
        return rng.choice(CONSTS)
    parts = [rng.choice(['"' + rng.choice(WORDS).replace('"', "") + '"', rng.choice(CONSTS)]) for _ in range(2)]
    return " # ".join(parts)


def entry(rng):
    kind = rng.choice(TYPES)
    open_, close = ("(", ")") if rng.random() < 0.15 else ("{", "}")
    key = rng.choice(KEYS) if rng.random() > 0.05 else ""
    fields = []
    for _ in range(rng.randint(0, 5)):
        name = rng.choice(NAMES)
        fields.append(f"{rng.choice(['', ' ', '  '])}{name} {rng.choice(['=', ' = ', '='])} {value(rng)}")
    sep = rng.choice([",\n", ", ", ",\n  "])
    body = sep.join([key] + fields)
    if rng.random() < 0.3:
        body += ","
    if rng.random() < 0.04:
        close = ""
    lead = rng.choice(["", "", "", "  "])
    return f"{lead}@{kind}{rng.choice(['', ' '])}{open_}{body}\n{close}"


def bibtex(rng):
    parts = []
    if rng.random() < 0.4:
        parts.append('@string{acm = "Assoc. {C}omputing"}')
    if rng.random() < 0.1:
        parts.append("@String(jan = {Jan.})")
    if rng.random() < 0.08:
        parts.append('@string{bad = acm # " x"}')
    if rng.random() < 0.1:
        parts.append("@preamble{\"\\newcommand{\\x}{y}\"}")
    for _ in range(rng.randint(1, 5)):
        parts.append(entry(rng))
    if rng.random() < 0.1:
        parts.append("Some text between entries.")
    return rng.choice(["\n", "\n\n"]).join(parts) + "\n"


def person(rng):
    return {k: v for k, v in [("family", rng.choice(["Smith", "Doe", "Ünï"])), ("given", rng.choice(["Ann", "J."]))]
            if rng.random() < 0.9}


def csl(rng):
    items = []
    for _ in range(rng.randint(1, 4)):
        item = {"id": rng.choice(KEYS), "type": rng.choice(["book", "article-journal"])}
        if rng.random() < 0.8:
            item["title"] = rng.choice(["Smith's Book", "A Title", "Ünïcode"])
        if rng.random() < 0.8:
            item[rng.choice(["author", "editor"])] = [person(rng) for _ in range(rng.randint(1, 3))]
        roll = rng.random()
        if roll < 0.4:
            item["issued"] = {"date-parts": [[rng.choice([2019, 1999, "2001"]), 3]]}
        elif roll < 0.55:
            item["issued"] = {"literal": rng.choice(["Spring 2003", "circa", "12 May 1999"])}
        elif roll < 0.65:
            item["issued"] = {"raw": "2004-05"}
        if rng.random() < 0.4:
            item[rng.choice(["publisher", "container-title"])] = "Pub"
        if rng.random() < 0.1:
            item["volume"] = 5
        items.append(item)
    return json.dumps(items, ensure_ascii=False, indent=rng.choice([None, 1]))


def case(rng):
    files = {}
    lines = []
    for index in range(rng.randint(1, 2)):
        roll = rng.random()
        if roll < 0.65:
            name = f"refs{index}.bib"
            files[name] = bibtex(rng)
        elif roll < 0.85:
            name = f"refs{index}.json"
            files[name] = csl(rng)
        elif roll < 0.95:
            name = f"refs{index}.bibtex"
            files[name] = bibtex(rng)
        else:
            name = "missing.bib"
        lines.append("#+bibliography: " + (f'"{name}"' if rng.random() < 0.2 else name))
    text = "\n".join(lines) + "\n\nSee [cite:@" + rng.choice(KEYS) + "].\n"
    unknown = [rng.choice(KEYS) + rng.choice(["", "x", "9"]) for _ in range(2)] + ["zzzz"]
    return {"text": text, "files": files, "unknown": unknown}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)])
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-cite-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "cite"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_CITE_CASES=out), capture_output=True, text=True,
    )
    errors = sum(1 for case in cases if "error" in case["expected"])
    print(f"{args.count} cases ({errors} where Emacs fails); saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:10000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
