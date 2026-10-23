"""Helix: evaluating an emacs-lisp block (y/n question) and inserting a clock table; footnote back-reference."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = "* Alpha\nSee [fn:1].\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n[fn:1] The note.\n"
h = Helix("babel", {"files": {"a.org": FILE}, "emacs": True})
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    goto(h, 4, 2); acts = h.code_actions(); idx = next(i for i, a in enumerate(acts) if "Evaluate" in a["title"]); h.pick_action(acts, idx)
    time.sleep(8); h.settle()
    print("after evaluate, received:", [m.get("method") or ("result" if "result" in m else "error") for _, m in h.log.received][-6:])
    print("messages:", [m["params"]["message"] for _, m in h.log.received if m.get("method") == "window/showMessage"][-4:])
    print("question:", h.question(), "| status row:", h.screen()[-1].strip()[:150])
    t = titles(h); print("actions now:", t[:6])
    goto(h, 1, 3); acts = h.code_actions(); idx = next(i for i, a in enumerate(acts) if "clock table" in a["title"]); h.pick_action(acts, idx); time.sleep(2); h.settle()
    print("clock table:", h.text(A).split("\n")[:4], [m["params"]["message"] for _, m in h.log.received if m.get("method") == "window/showMessage"][-2:])
    goto(h, 6, 2); h.keys("g", "d"); h.settle(); print("gd at definition label:", h.status()[2:])
finally:
    h.stop()
