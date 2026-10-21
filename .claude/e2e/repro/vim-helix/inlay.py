import os, sys, json
sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/lsp")
from client import *
ws = "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vim-helix/w7"
p = os.path.join(ws, "c.org")
for prof in (FULL, PLAIN):
    c = Client(profile=prof, root=ws, options={"org_client": prof == FULL}, name="inl")
    print("provider:", prof, c.init_result["capabilities"].get("inlayHintProvider") if hasattr(c, "init_result") else "?")
    c.open(p); c.settle(0.5, 5)
    r = c.command("org.clock.display", c.at(p, 0, 0)); c.settle(0.5, 5)
    h = c.request("textDocument/inlayHint", {"textDocument": {"uri": c.uri(p)}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 10, "character": 0}}})
    print(prof, "display result:", r, "hints:", json.dumps(h)[:400])
    c.shutdown()
