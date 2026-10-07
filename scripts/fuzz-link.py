"""Random documents through Emacs's link commands and org-lsp's, compared.

    ORG_LISP_DIR=… python3 scripts/fuzz-link.py [count] [seed]

Writes the cases that differ to /tmp/org-lsp-fuzz-link.json, for
`ORG_LINK_CASES=… cargo test -p org-core --test link`.
"""

import json
import os
import random
import subprocess
import sys

import link_reference

PIECES = [
    "* TODO [#B] Head [1/3] :a:b:\n", "* Plain headline\n", "** Child entry\n", "*** COMMENT  Deep   one\n",
    "* Head\n:PROPERTIES:\n:CUSTOM_ID: cid\n:END:\n", "* Owner\n:PROPERTIES:\n:ID: own-id\n:END:\n",
    "Some text line.\n", "(parenthesised [2/5] line)\n", "\n", "#+NAME: thing\n| a | b |\n",
    "#+name: code\n#+begin_src sh\necho hi\n#+end_src\n", "see <<target>> here\n", "- item [50%]\n- other\n",
    "SCHEDULED: <2026-01-01 Thu>\n", "Link [[https://orgmode.org][Org]] and https://a.b/c x\n", "#+TITLE: Doc\n",
    "* See [[file:x.org][X]] link\n", "  indented text\n",
]
OPTIONS = [
    {}, {"org_id_link_to_org_use_id": "t"}, {"org_id_link_to_org_use_id": "use-existing"},
    {"org_id_link_to_org_use_id": "create-if-interactive"}, {"org_link_context_for_files": False},
    {"org_id_link_to_org_use_id": "use-existing", "org_id_link_consider_parent_id": True},
    {"org_link_file_path_type": "absolute"},
]


def case(rng):
    text = "".join(rng.choice(PIECES) for _ in range(rng.randint(1, 6)))
    at = rng.randint(0, len(text))
    text = text[:at] + "¦" + text[at:]
    steps = []
    for _ in range(rng.randint(1, 3)):
        kind = rng.random()
        if kind < 0.35:
            steps.append(["store"] if rng.random() < 0.8 else ["store", "C-u"])
        elif kind < 0.55:
            steps.append(["annotation"])
        elif kind < 0.75:
            steps.append(["insert", "", None, None, None])
        elif kind < 0.85:
            steps.append(["insert-last"])
        else:
            steps.append(["goto", rng.choice(["Head", "text", "item", "echo", "here", "a |", "\n"])])
    result = {"text": text, "steps": steps, "options": rng.choice(OPTIONS)}
    return result


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    cases = [case(rng) for _ in range(count)]
    # A goto to text that is not there: dropped.
    cases = [c for c in cases if all(s[0] != "goto" or s[1] in c["text"].replace("¦", "") for s in c["steps"])]
    link_reference.reference(cases)
    path = "/tmp/org-lsp-fuzz-link.json"
    link_reference.save(path, cases)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    run = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "link"],
        cwd=os.path.join(root, "server"),
        env=dict(os.environ, ORG_LINK_CASES=path),
        capture_output=True,
        text=True,
    )
    print(run.stdout[-6000:] if run.returncode else f"ok: {len(cases)} cases")


if __name__ == "__main__":
    main()
