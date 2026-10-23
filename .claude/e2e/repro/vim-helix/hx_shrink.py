"""Helix: shrinking a <5> column: what the server sends and what Helix shows."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
h = Helix("shrink", {"files": {"a.org": "* Head\n| <5>   |\n| long text here |\n"}})
with open(os.path.join(h.config, "config.toml"), "a") as f:
    f.write("\n[editor.lsp]\ndisplay-inlay-hints = true\n")
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    goto(h, 3, 3); before = h.log.count; act(h, "Shrink or expand the table column"); time.sleep(2); h.settle()
    print("screen:", [r.rstrip() for r in h.screen()[0:3]])
    for i, m in h.log.received:
        if i > before: print("<-", json.dumps(m)[:300])
    goto(h, 1, 3); h.type_text("x"); time.sleep(1); h.settle(); time.sleep(2); h.settle()
    print("after an edit:", [r.rstrip() for r in h.screen()[0:3]])
finally:
    h.stop()
