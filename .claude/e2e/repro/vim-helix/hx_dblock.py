"""Helix: clock table insert/update and sort: does the edit land?"""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = "* Top\n** b\n** a\n#+BEGIN: clocktable :scope file\n#+END:\n"
h = Helix("dblock", {"files": {"a.org": FILE}})
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    goto(h, 4, 3); act(h, "Update the dynamic block"); print("update:", h.text(A).split("\n")[3:8], h.messages().strip())
    goto(h, 1, 3); act(h, "Sort the entries", ["a"]); print("sort:", h.text(A).split("\n")[:3], h.messages().strip())
    goto(h, 1, 3); act(h, "Insert or update a clock table"); print("insert:", h.text(A).split("\n")[:4], h.messages().strip())
finally:
    h.stop()
