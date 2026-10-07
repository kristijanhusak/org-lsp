"""Random Org texts through Emacs's `org-lint` and org-lsp's, compared.

    ORG_LISP_DIR=… python3 scripts/fuzz-lint.py [count] [seed]

Writes the cases to /tmp/org-lsp-fuzz-lint.json, for
`ORG_LINT_CASES=… cargo test -p org-core --test lint`.
"""

import os
import random
import subprocess
import sys

import lint_reference

# Pieces that set checkers off, and pieces that should not.
PIECES = [
    "* Heading\n", "** Sub heading [1/2] :tag:\n", "* TODO [#A] Task :a::b:\n", "* COMMENT QUOTE old\n",
    "* QUOTE section\n", "* Footnotes\n", "Paragraph** Oops heading\n", "text ** not a heading\n",
    ":PROPERTIES:\n:CUSTOM_ID: one\n:EFFORT: 1:00\n:END:\n", ":PROPERTIES:\n:CUSTOM_ID: one\n:END:\n",
    ":PROPERTIES:\n:EFFORT: soon\n:ID: a::b\n:TODO: x\n:cache: yes\n:END:\n", ":PROPERTIES:\n:END:\n",
    "SCHEDULED: <2024-01-01 Mon +1w> DEADLINE: <2024-01-02 Tue +2d>\n", "SCHEDULED: [2024-01-01 Mon]\n",
    "DEADLINE: [2024-01-01 Mon]\n", "  SCHEDULED: <2024-01-01 Mon>\n", "#+NAME: tbl\n| a | b |\n",
    "#+name: tbl\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n", "#+TBLNAME: other\n| x |\n", "#+NAME: alone\n\n",
    "#+CAPTION: alone\n", "#+ATTR_HTML: :width 10\n", "#+RESULTS:\n", "#+TITLE: Doc\n#+NAME: n\nPara.\n",
    "#+BEGIN_SRC\nx\n#+END_SRC\n", "#+begin_export\nx\n#+end_export\n", "#+BEGIN_HTML\n<b>\n#+END_HTML\n",
    "#+begin_example\n#+notakeyword here\n:DRAWER:\n#+end_example\n", "#+BEGIN_QUOTE\nunfinished\n",
    "#+END_SRC trailing\n", "#+begin_center\nok\n#+end_center\n", "#+BEGIN: clocktable\n#+END:\n",
    "#+TITLE without colon\n", "#+ \n", "#+CATEGORY: one\n", "#+CATEGORY: two\n", "#+PROPERTY: cache yes\n",
    "#+SETUPFILE: missing.org\n", "#+SETUPFILE: \"other.org\"\n", "#+BIBLIOGRAPHY: refs.bib\n",
    "#+INCLUDE: \"other.org\" HTML\n", "#+INCLUDE: x.org src\n", "[cite:@key] in text\n", "#+PRINT_BIBLIOGRAPHY:\n",
    ":LOGBOOK:\n- note\n:END:\n", ":NOTES:\n:INNER:\nx\n:END:\n", ":DANGLING:\nno end\n", ":END:\n",
    "  %%(diary-anniversary 1 1 2000)\n", "%%(diary-float t 1 1)\n", "[[*Heading]] and [[*Missing]]\n",
    "[[#one]] [[#two]] [[target]] <<target>> <<target>>\n", "[[(ref)]] [[(missing)]]\n",
    "#+begin_src sh -r\necho hi (ref:ref)\n#+end_src\n", "[[file:other.org]] [[file:nope.txt]] [[./missing/x.png]]\n",
    "[[file+sys:a.pdf]] [[file+emacs:b.org]]\n", "[[https://a.b][desc [with] bracket]] ]\n",
    "[[https://a.b][open [ bracket]]\n", "[[a%20b]] [[a%2Fb]] [[100%]]\n", "[[tbl]] [[TBL]] [[other]]\n",
    "[fn:1] and [fn:2] and [fn:inline:def]\n", "[fn:1] defined\n[fn:3] lonely\n", "[fn:3] again\n",
    "\\begin{orgframe} \\end{orgframe}\n", "- item\n- item\n", "1. one\n3. three\n", "#+MACRO: m $1\n",
    "Plain line with ** stars inside.\n", "\n",
    "#+begin_src python :results output :foo 1\nprint(1)\n#+end_src\n", "#+begin_src sh -r :results silent append\nx\n#+end_src\n",
    "#+begin_src emacs-lisp :results table list :exports bogus :tangle out.el\n()\n#+end_src\n",
    "#+HEADER: :var x=1\n#+HEADER: :bad\n#+begin_src R :session\n1\n#+end_src\n", "#+begin_src unknownlang :file-mode 755\n#+end_src\n",
    "#+CALL: f() :results raw\n", "#+CALL: f[:x 1](a=1) [:results raw]\n", "#+CALL: (x)\n",
    "Inline call_g[:bad 1](2)[:results raw] and src_python[:results value :nope]{1+1} here.\n",
    "#+PROPERTY: header-args :results silent :zz 1\n", "#+PROPERTY: header-args:python+ :session x :pp 2\n",
    ":PROPERTIES:\n:header-args: :exports both :qq 1\n:HEADER-ARGS:sh+: :dir /tmp\n:END:\n",
    "#+OPTIONS: toc:nil num:t bogus:1 H:2 lonely:\n", "#+OPTIONS: \\n:t ^:{} tex:verbatim author\n",
    ":PROPERTIES:\n:TITLE: x\n:HTML_HEAD: y\n:DESCRIPTION: d\n:EXPORT_TITLE: z\n:CATEGORY: c\n:END:\n",
]


def case(rng):
    text = "".join(rng.choice(PIECES) for _ in range(rng.randint(1, 7)))
    files = [["other.org", "* Other\n"], ["refs.bib", "@book{key, title={T}}\n"]]
    return {"text": text, "files": files}


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    cases = [case(rng) for _ in range(count)]
    lint_reference.reference(cases)
    path = "/tmp/org-lsp-fuzz-lint.json"
    lint_reference.save(path, cases)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    run = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "lint"],
        cwd=os.path.join(root, "server"),
        env=dict(os.environ, ORG_LINT_CASES=path),
        capture_output=True,
        text=True,
    )
    print(run.stdout[-8000:] if run.returncode else f"ok: {len(cases)} cases")


if __name__ == "__main__":
    main()
