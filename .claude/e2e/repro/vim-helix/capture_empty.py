"""Capture into an empty (0-byte) existing target: which edit does the server send?"""
import json, os, sys
sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/lsp")
from client import *
ws = "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/capws"
open(os.path.join(ws, "work.org"), "w").write("* A\n")
open(os.path.join(ws, "inbox.org"), "w").write(sys.argv[1] if len(sys.argv) > 1 else "")
options = {"org_capture_templates": [{"key": "t", "description": "Task", "target": ["file", os.path.join(ws, "inbox.org")], "template": "* TODO %?"}]}
c = Client(profile=FULL, root=ws, options=options, name="capempty")
seen = []
orig = c._apply_edit
def rec(params):
    seen.append(params); return orig(params)
c._apply_edit = rec
w = c.open(os.path.join(ws, "work.org")); c.settle(0.3, 5)
r, steps = c.run("org.capture", c.at(w, 0, 0), answers=["t"])
u = [u for u in c.docs if "CAPTURE-" in u][-1]
sel = c.shown[-1].get("selection"); p = sel["start"] if sel else {"line": 0, "character": 0}
c.type_at(u, p["line"], p["character"], "Captured")
r2, steps = c.run("org.capture.finish", c.at(u, p["line"], 0), answers=[])
c.settle(0.8, 8)
print("EDITS:", json.dumps(seen, indent=1)[:3000])
print("DISK:", repr(open(os.path.join(ws, "inbox.org")).read()))
print("r2", r2, "notes", c.notes("org-lsp/save"))
c.shutdown()
