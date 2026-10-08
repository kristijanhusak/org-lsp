"""Compare org-lsp's motion commands with Emacs on random documents.

    ORG_LISP_DIR=... python3 scripts/fuzz-motion.py [COUNT] [SEED]

Writes the cases with Emacs's answers to /tmp/org-lsp-fuzz-motion.json and
runs the motion test on them (`ORG_MOTION_CASES`).
"""

import os
import random
import subprocess
import sys

import motion_reference

PIECES = [
    "* Heading\n", "** Sub\n", "*** Deep\n", "* TODO Task :tag:\n", "** DONE Done\n", "**** Four\n",
    "Text line.\n", "\n", "- item\n- item two\n",
    "#+begin_src org\n* not a heading?\n#+end_src\n", "#+begin_example\n** inside\n#+end_example\n",
    "*bold* text\n", "*not heading\n", " * indented star\n",
    "A [[https://orgmode.org][link]] and [[*Heading]].\n", "Plain https://example.com/x and <mailto:a@b.c>.\n",
    "=[[verbatim]]= text\n", "[[file:a.org]]\n", "#+TITLE: Doc\n",
    ":PROPERTIES:\n:ID: x\n:END:\n",
]
COMMANDS = [
    "org-next-visible-heading", "org-previous-visible-heading", "org-forward-heading-same-level",
    "org-backward-heading-same-level", "outline-up-heading", "org-next-link", "org-previous-link",
]


def case(rng):
    text = "".join(rng.choice(PIECES) for _ in range(rng.randint(1, 9)))
    point = len(text[: rng.randint(0, len(text))].encode("utf-8"))
    out = {"text": text, "point": point, "command": rng.choice(COMMANDS)}
    if rng.random() < 0.3:
        out["count"] = rng.choice([2, 3, -1, -2])
    if rng.random() < 0.2:
        out["times"] = rng.choice([2, 3])
    return out


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    cases = motion_reference.reference([case(rng) for _ in range(count)])
    path = "/tmp/org-lsp-fuzz-motion.json"
    motion_reference.save(path, cases)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    run = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "motion"],
        cwd=os.path.join(root, "server"),
        env=dict(os.environ, ORG_MOTION_CASES=path),
        capture_output=True,
        text=True,
    )
    print(run.stdout[-8000:] if run.returncode else f"ok: {len(cases)} cases")


if __name__ == "__main__":
    main()
