#!/usr/bin/env python3
"""Write tests/bibtex/cases.json: the commands of `ol-bibtex.el` with what
Emacs makes of them.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-bibtex-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bibtex_reference import reference, save  # noqa: E402

ARTICLE = (
    "@article{smith2020,\n  author = {John Smith and Jane Doe},\n  title = \"The {Great} Title\",\n"
    "  journal = {J. Things},\n  year = 2020,\n  keywords = {alpha beta, gamma}\n}\n"
)
ODD = (
    "Some text before.\n@Book{knuth,\n  author = \"Donald  E.\n   Knuth\",\n  title={{The Art} of Programming},\n"
    "  OPTpublisher = {AW},\n  ALTyear = 1968,\n  option = {kept},\n  note = \"a\" # jan # {b}\n  month = jan\n}\n"
)
NOKEY = "@misc{,\n  title = {No key}\n}\n"
UNKNOWN = "@online{web,\n  title = {Online}\n}\n"
STRINGS = "@string{foo = \"Foo\"}\n@comment{nothing}\n" + ARTICLE + "\n@misc(paren, title = {Paren})\n"
UNCLOSED = "@misc{open,\n  title = {Unclosed\n"
ENTRY = "* The heading :t1:t2:\n:PROPERTIES:\n:BTYPE: article\n:CUSTOM_ID: k\n:AUTHOR: A\n:YEAR: 2000\n:END:\n"
NESTED = "#+FILETAGS: file\n* Parent :p:\n** Child :c:bib:skip:\n:PROPERTIES:\n:BTYPE: misc\n:CUSTOM_ID: ch\n:END:\n"
PREFIXED = (
    "* Pre\n:PROPERTIES:\n:BIB_BTYPE: book\n:BIB_TITLE: T\n:BIB_FOO: x\n:BIB_FOO+: y\n:bib_bar: z\n"
    ":CUSTOM_ID: k\n:OTHER: o\n:END:\n"
)
FILE = (
    "#+TITLE: refs\n* A\n:PROPERTIES:\n:BTYPE: misc\n:END:\n* B\nno type\n** C\n:PROPERTIES:\n:BTYPE: book\n"
    ":CUSTOM_ID: c\n:AUTHOR: Ann\n:PUBLISHER: P\n:YEAR: 1999\n:END:\n* TODO [#A] D :x:\n:PROPERTIES:\n"
    ":BTYPE: Article\n:END:\n"
)
ARTICLE_ANSWERS = ["article", "My Title", "Smith", "Journal", "2020", "key1"]
CASES = [
    # org-bibtex-create
    {"command": "create", "text": "* a\n", "point": 3, "answers": ARTICLE_ANSWERS},
    {"command": "create", "text": "* a\n", "point": 0, "answers": ARTICLE_ANSWERS},
    {"command": "create", "text": "", "point": 0, "answers": ARTICLE_ANSWERS},
    {"command": "create", "text": "* a\nbody\n** b\n", "point": 6, "answers": ARTICLE_ANSWERS},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["book", "T", "author", "Ann", "Pub", "", "k"]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["book", "T", "", "x"]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["book", "T", "Editor", "Ed", "P", "1990", "k"]},
    {"command": "create", "text": "* a\n", "point": 4, "optional": True,
     "answers": ["article", "T", "A", "J", "2001", "1", "", "3-4", "", "", "10.1/x", "kk"]},
    {"command": "create", "text": "* a\n", "point": 4,
     "answers": ["misc", "Misc title", " spaced "]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", "", "x"]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["foo"]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": [""]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["Article"]},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", "Tagged", "k"],
     "options": {"org-bibtex-tags": "'(\"bib\" \"ref\")"}},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", "Prefixed", "k"],
     "options": {"org-bibtex-prefix": "\"BIB_\""}},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", "Typed", "k"],
     "options": {"org-bibtex-type-property-name": "\"kind\"", "org-bibtex-key-property": "\"ID\""}},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", "Treat", "k"],
     "options": {"org-bibtex-treat-headline-as-title": "nil", "org-bibtex-autogen-keys": "nil"}},
    {"command": "create", "text": "* a\n", "point": 3, "answers": ["article", "My Title", "Smith", "Journal", "2020"],
     "options": {"org-bibtex-autogen-keys": "t"}},
    {"command": "create", "text": "* a\n", "point": 3, "answers": ["misc", "No year"],
     "options": {"org-bibtex-autogen-keys": "t"}},
    {"command": "create", "text": "* a\n", "point": 3, "answers": ["misc", "Plain", "k"],
     "options": {"org-adapt-indentation": "t"}},
    # org-bibtex-create-in-current-entry
    {"command": "create-here", "text": "* TODO Some heading :x:\ntext\n", "point": 10, "answers": ["misc", "k"]},
    {"command": "create-here", "text": "* Heading\n", "point": 3, "answers": ["misc", "Title", "k"],
     "options": {"org-bibtex-treat-headline-as-title": "nil"}},
    {"command": "create-here", "text": "* H\n:PROPERTIES:\n:BTYPE: book\n:END:\n", "point": 3,
     "answers": ["article", "T", "A", "J", "2000", "k"]},
    {"command": "create-here", "text": "* H\n:PROPERTIES:\n:BTYPE: misc\n:CUSTOM_ID: done\n:END:\n", "point": 20,
     "answers": ["misc", "A", "T", "", "", "", "", "", ""], "optional": True},
    {"command": "create-here", "text": "* H\n", "point": 0, "answers": ["misc", "T", "k"],
     "options": {"org-bibtex-tags": "'(\"bib\")"}},
    # org-bibtex-check and org-bibtex-check-all
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: article\n:AUTHOR: me\n:END:\n", "point": 0,
     "answers": ["J", "2001", "kk"]},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: article\n:AUTHOR: me\n:END:\n", "point": 0,
     "answers": ["T", "J", "2001", "kk"], "options": {"org-bibtex-treat-headline-as-title": "nil"}},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: misc\n:CUSTOM_ID: x\n:END:\n", "point": 2,
     "optional": True, "answers": ["", "", "", "", "", "", "", ""]},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: inbook\n:CUSTOM_ID: x\n:EDITOR: E\n:END:\n",
     "point": 2, "answers": ["pages", "1-2", "P", "1999"]},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: Misc\n:END:\n", "point": 0, "answers": []},
    {"command": "check", "text": "* H\nno type\n", "point": 4, "answers": []},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BTYPE: unknown\n:END:\n", "point": 0, "answers": []},
    {"command": "check", "text": "* H\n:PROPERTIES:\n:BIB_BTYPE: misc\n:END:\n", "point": 0, "answers": ["k"],
     "options": {"org-bibtex-prefix": "\"BIB_\""}},
    {"command": "check-all", "text": FILE, "point": 0, "answers": ["kA", "Bob", "kD"]},
    {"command": "check-all", "text": "* A\n:PROPERTIES:\n:BTYPE: article\n:END:\n* B\n:PROPERTIES:\n:BTYPE: misc\n:END:\n",
     "point": 40, "answers": ["au", "jo", "1", "ka", "kb"]},
    # org-bibtex-yank
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": ARTICLE},
    {"command": "yank", "text": "* a\n", "point": 1, "clipboard": ARTICLE},
    {"command": "yank", "text": "* a\nbody", "point": 6, "clipboard": ARTICLE},
    {"command": "yank", "text": "", "point": 0, "clipboard": ARTICLE.rstrip("\n")},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": ARTICLE,
     "options": {"org-bibtex-tags-are-keywords": "t", "org-bibtex-tags": "'(\"bib\")"}},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": ARTICLE,
     "options": {"org-bibtex-prefix": "\"BIB_\"", "org-bibtex-key-property": "\"ID\""}},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": ODD},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "@misc{a, title={First}}\n@misc{b, title={Second}}\n"},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "@misc{c\n  title = {No comma}\n  year = 1\n}\n"},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": NOKEY},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": UNKNOWN},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "no entry at all\n"},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "@misc{k, year = 2000}\n"},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": UNCLOSED},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "@misc{q,\n  title = \"Unclosed\n}\n"},
    {"command": "import", "text": "* a\n", "point": 4, "bib": "@misc{ok, title={OK}}\n" + UNCLOSED},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": "@misc{k, title={T}, keywords={a b, , c-d!}}\n",
     "options": {"org-bibtex-tags-are-keywords": "t"}},
    {"command": "yank", "text": "* H\n:PROPERTIES:\n:OLD: 1\n:END:\n", "point": 2, "optional": True,
     "clipboard": ARTICLE},
    {"command": "yank", "text": "* H :x:\nbody\n", "point": 9, "optional": True,
     "clipboard": ARTICLE, "options": {"org-bibtex-tags-are-keywords": "t"}},
    {"command": "yank", "text": "* a\n", "point": 4, "clipboard": ARTICLE, "options": {"org-adapt-indentation": "t"}},
    # Found by fuzzing: before the first headline; the point in a line
    # written again; a title asked for when the entry read has none.
    {"command": "yank", "text": "intro\n", "point": 0, "optional": True, "clipboard": ARTICLE},
    {"command": "create-here", "text": "intro\n", "point": 3, "answers": ["misc"]},
    {"command": "check", "text": "\n", "point": 1, "answers": []},
    {"command": "create", "text": "* a\n", "point": 4, "answers": ["misc", " pad ", "k"],
     "options": {"org-bibtex-tags": "'(\"bib\")"}},
    {"command": "yank", "text": "* H\n:PROPERTIES:\n:A: 1\n:YEAR: x\n:END:\n", "point": 22, "optional": True,
     "clipboard": "@misc{k, title={T}, note={N}}\n", "options": {"org-adapt-indentation": "t"}},
    {"command": "yank", "text": "* H\n:PROPERTIES:\n:TITLE: old\n:END:\n* I\n", "point": 36, "optional": True,
     "clipboard": "@misc{k, year={2000}}\n", "answers": [""]},
    {"command": "import", "text": "* H\n:PROPERTIES:\n:FOO: some value\n:END:\n", "point": 22, "bib": "nothing\n"},
    # org-bibtex-import-from-file
    {"command": "import", "text": "* a\n", "point": 4, "bib": ARTICLE + "\n@misc{k2, title={Other}}\n"},
    {"command": "import", "text": "* a\n* b\n", "point": 4, "bib": STRINGS},
    {"command": "import", "text": "", "point": 0, "bib": ODD},
    {"command": "import", "text": "* a\n", "point": 4, "bib": "nothing here\n"},
    {"command": "import", "text": "* a\n", "point": 4, "bib": "@misc{a, title={A}}\n@misc{b, title={B}}",
     "options": {"org-bibtex-tags": "'(\"bib\")"}},
    # org-bibtex-export-to-kill-ring (org-bibtex-headline)
    {"command": "copy", "text": ENTRY, "point": 0},
    {"command": "copy", "text": ENTRY, "point": 30},
    {"command": "copy", "text": "* COMMENT TODO [#B] Commented :t:\n:PROPERTIES:\n:BTYPE: misc\n:END:\n", "point": 0},
    {"command": "copy", "text": "* TODO [#B] Title  here   :t:\n:PROPERTIES:\n:BTYPE: misc\n:END:\n", "point": 0},
    {"command": "copy", "text": "* :tag:\n:PROPERTIES:\n:BTYPE: misc\n:END:\n", "point": 0},
    {"command": "copy", "text": "* Title\n:PROPERTIES:\n:BTYPE: misc\n:TITLE:  Own title  \n:END:\n", "point": 0},
    {"command": "copy", "text": "* No type\n", "point": 0},
    {"command": "copy", "text": "* T\n:PROPERTIES:\n:BTYPE: Article\n:CUSTOM_ID: x\n:END:\n", "point": 0},
    {"command": "copy", "text": "* T\n:PROPERTIES:\n:BTYPE: book\n:EDITOR: E\n:AUTHOR: A\n:VOLUME: 2\n:END:\n", "point": 0},
    {"command": "copy", "text": ENTRY, "point": 0, "options": {"org-bibtex-tags-are-keywords": "t"}},
    {"command": "copy", "text": ENTRY.replace(":YEAR: 2000", ":YEAR: 2000\n:KEYWORDS: x, y"), "point": 0,
     "options": {"org-bibtex-tags-are-keywords": "t"}},
    {"command": "copy", "text": "* T :a:\n:PROPERTIES:\n:BTYPE: misc\n:KEYWORDS: x\n:END:\n", "point": 0,
     "options": {"org-bibtex-tags-are-keywords": "t", "org-bibtex-export-arbitrary-fields": "t",
                 "org-bibtex-prefix": "\"\""}},
    {"command": "copy", "text": NESTED, "point": 30, "options": {"org-bibtex-tags-are-keywords": "t"}},
    {"command": "copy", "text": NESTED, "point": 30,
     "options": {"org-bibtex-tags-are-keywords": "t", "org-bibtex-inherit-tags": "t",
                 "org-bibtex-tags": "'(\"bib\")", "org-bibtex-no-export-tags": "'(\"skip\")"}},
    {"command": "copy", "text": PREFIXED, "point": 0, "options": {"org-bibtex-prefix": "\"BIB_\""}},
    {"command": "copy", "text": PREFIXED, "point": 0,
     "options": {"org-bibtex-prefix": "\"BIB_\"", "org-bibtex-export-arbitrary-fields": "t"}},
    {"command": "copy", "text": PREFIXED + ":PROPERTIES:\n", "point": 0,
     "options": {"org-bibtex-prefix": "\"BIB_\"", "org-bibtex-export-arbitrary-fields": "t"}},
    {"command": "copy", "text": "* T\n:PROPERTIES:\n:BTYPE: misc\n:END:\n", "point": 0,
     "options": {"org-bibtex-export-arbitrary-fields": "t"}},
    {"command": "copy", "text": "* T\n:PROPERTIES:\n:KIND: misc\n:ID: the-id\n:END:\n", "point": 0,
     "options": {"org-bibtex-type-property-name": "\"kind\"", "org-bibtex-key-property": "\"ID\""}},
    # org-bibtex
    {"command": "export", "text": FILE, "point": 0},
    {"command": "export", "text": "* none\n", "point": 0},
    {"command": "export", "text": "", "point": 0},
]

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "bibtex", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
