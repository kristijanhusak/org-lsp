"""Which code actions Helix offers where: compare with editors/helix/TESTING.org."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = """#+TITLE: T
* TODO [#B] Head :tag:
SCHEDULED: <2026-05-18 Mon>
:PROPERTIES:
:Effort: 0:30
:END:
- [ ] item one
- [ ] item two
| a | b |
|---+---|
| 1 | 2 |
#+TBLFM: $2=$1*2
#+begin_src emacs-lisp
(+ 1 2)
#+end_src
[[Head]] [fn:1] <<tgt>>
[fn:1] note
* Done :ARCHIVE:
"""
places = {"headline": (2, 8), "planning timestamp": (3, 14), "property": (5, 2), "item": (7, 4), "table": (11, 3),
          "tblfm": (12, 3), "src": (14, 2), "link": (16, 3), "footnote": (16, 11), "plain text": (1, 3)}
h = Helix("acts", {"files": {"t.org": FILE}, "options": {"org_todo_keywords": ["TODO(t)", "WAIT(w@)", "|", "DONE(d)"]}})
try:
    h.start(os.path.join(h.directory, "t.org"))
    out = {}
    for name, (l, c) in places.items():
        goto(h, l, c)
        out[name] = titles(h)
        print(f"== {name} {l}:{c}")
        for t in out[name]: print("   ", t)
    json.dump(out, open("/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/hx_actions.json", "w"), indent=1)
finally:
    h.stop()
