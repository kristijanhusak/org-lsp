#!/usr/bin/env python3
"""Write tests/cite/cases.json: bibliographies (BibTeX and CSL-JSON)
named by `#+BIBLIOGRAPHY:` lines, chosen ones and the random ones of
`scripts/fuzz-cite.py` (seed 0), with what Org's `oc-basic` makes of them.

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-cite-fixtures.py
"""

import importlib.util
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from cite_reference import reference, save  # noqa: E402

spec = importlib.util.spec_from_file_location("fuzz_cite", os.path.join(HERE, "fuzz-cite.py"))
fuzz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fuzz)

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

if __name__ == "__main__":
    path = os.path.join(HERE, "..", "tests", "cite", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
