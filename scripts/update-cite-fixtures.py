#!/usr/bin/env python3
"""Write tests/cite/cases.json: bibliographies (BibTeX and CSL-JSON)
named by `#+BIBLIOGRAPHY:` lines, chosen ones and the random ones of
`scripts/fuzz-cite.py` (seed 0), with what Org's `oc-basic` makes of them;
and tests/cite/insert.json: `org-cite-insert` in chosen places and the
random ones of `scripts/fuzz-cite-insert.py` (seed 0).

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-cite-fixtures.py
"""

import importlib.util
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from cite_reference import reference, save  # noqa: E402
import cite_insert_reference  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_cite", os.path.join(HERE, "fuzz-cite.py"))
fuzz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz)
spec = importlib.util.spec_from_file_location("fuzz_cite_insert", os.path.join(HERE, "fuzz-cite-insert.py"))
fuzz_insert = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz_insert)

BIB = r"""@string{acm = "Assoc. {C}omputing"}
@Book{knuth1984,
  author    = {Knuth, Donald E.},
  title     = {The {\TeX}book},
  publisher = acm # " Press",
  year      = 1984,
  month     = jan
}

@article{doe2020,
  author = "John Doe and Jane   Roe",
  title = {A {Study} of
     Things},
  journal = {J. of \emph{Stuff}},
  date = {2020-05-01}
}
@misc(nobody,
  title = {Untitled},
)
@online{web,
  editor = {Ed, Itor},
  title = {A Page},
  institution = {Somewhere},
  OPTnote = {Kept as note}
}
@comment{@article{inside, title = {Not read}}}
"""
JSON = """[{"id": "smith2019", "type": "book", "title": "Smith's Book",
  "author": [{"family": "Smith", "given": "Ann"}, {"family": "Jones", "given": "B."}],
  "issued": {"date-parts": [[2019, 3]]}, "publisher": "Pub"},
 {"id": "literal", "type": "book", "title": "Old", "issued": {"literal": "Spring 2003"}}]"""
CASES = [
    {"text": "#+bibliography: refs.bib\n#+BIBLIOGRAPHY: more.json\n", "files": {"refs.bib": BIB, "more.json": JSON},
     "unknown": ["knuth1985", "doe202", "zzz", "Smith2019"]},
    # Quoted, missing, and a `.bibtex` file whose constants Org cannot expand.
    {"text": '#+bibliography: "refs.bib"\n#+bibliography: missing.bib\n', "files": {"refs.bib": BIB}, "unknown": []},
    {"text": "#+bibliography: refs.bibtex\n", "files": {"refs.bibtex": BIB}, "unknown": []},
    # Malformed: unbalanced braces, a duplicate key.
    {"text": "#+bibliography: bad.bib\n", "files": {"bad.bib": "@article{a,\n title = {Open\n}\n"}, "unknown": []},
    {"text": "#+bibliography: dup.bib\n", "files": {"dup.bib": "@misc{a, title={One}}\n@misc{a, title={Two}}\n"},
     "unknown": []},
]
rng = random.Random(0)
CASES += [fuzz.case(rng) for _ in range(150)]

SMALL = "@misc{a1, title={One}, author={Doe, J.}}\n@misc{b2, title={Two}}\n@misc{c3, title={Three}}\n"
HEAD = "#+bibliography: refs.bib\n\n"


def insert_case(text, arg=False, keys=("c3", ""), style=""):
    return {"text": HEAD + text, "files": {"refs.bib": SMALL}, "arg": arg, "keys": list(keys), "style": style}


INSERT_CASES = [
    # A new citation: keys until an empty answer (Org puts the last
    # first), the style with the prefix argument.
    insert_case("See ¦here.\n", keys=["a1", "b2", ""]),
    insert_case("See ¦here.\n", arg=True, keys=["a1", ""], style="text"),
    insert_case("See ¦here.\n", keys=[""]),
    insert_case("See [cite/t:@a1; @b2]¦ here.\n"),
    # The style, the affixes, a reference.
    insert_case("See [ci¦te/t:@a1; @b2] here.\n", style="nil"),
    insert_case("See [cite/t¦:@a1] here.\n", style=""),
    insert_case("See [cite/t:@a¦1; @b2] here.\n", keys=["c3"]),
    insert_case("See [cite/t:@a1; ¦@b2] here.\n", keys=["c3"]),
    insert_case("See [cite/t:@a1; @b2¦] here.\n", keys=["c3"]),
    insert_case("See [cite/t:pre ¦;@a1; @b2] here.\n", keys=["c3"]),
    insert_case("See [cite/t:@a1; @b2; post¦] here.\n", keys=["c3"]),
    insert_case("See [cite/t:@a¦1] here.\n", keys=[""]),
    insert_case("See [cite/t:pre ¦;@a1] here.\n", keys=[""]),
    # Deleting.
    insert_case("See [cite/t:@a1; @b¦2] here.\n", arg=True),
    insert_case("See [cite/t:@a¦1; @b2] here.\n", arg=True),
    insert_case("See [ci¦te/t:@a1; @b2] here.\n", arg=True),
    insert_case("[ci¦te:@a1]\nnext\n", arg=True),
    insert_case("[ci¦te:@a1] next\n", arg=True),
    insert_case("See [ci¦te:@a1]\n", arg=True),
    insert_case("| a | [ci¦te:@a1] |\n", arg=True),
    # Where a citation may go.
    insert_case("* TODO He¦ad :tag:\n"),
    insert_case("* Head :t¦ag:\n"),
    insert_case("*¦ Head\n"),
    insert_case("#+title: A ¦title\n"),
    insert_case("#+caption: a ¦caption\n| t |\n"),
    insert_case("#+begin_verse\nA ¦verse\n#+end_verse\n"),
    insert_case("#+begin_src emacs-lisp\n(¦foo)\n#+end_src\n"),
    insert_case("- [ ]¦ item\n"),
    insert_case("- tag¦ :: item\n"),
    insert_case("| a |¦ b |\n"),
    insert_case("SCHEDULED: <2024-01-¦02 Tue>\n"),
    insert_case("[[https://example.com][a ¦link]]\n"),
    insert_case("*bo¦ld*\n"),
]
rng = random.Random(0)
INSERT_CASES += [fuzz_insert.case(rng) for _ in range(300)]

if __name__ == "__main__":
    path = os.path.join(HERE, "..", "tests", "cite", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
    path = os.path.join(HERE, "..", "tests", "cite", "insert.json")
    cite_insert_reference.save(path, cite_insert_reference.reference(INSERT_CASES))
    print(f"wrote {len(INSERT_CASES)} cases to {path}")
