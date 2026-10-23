"""Full client's display notifications: virtualIndent with startup indent,
display (pretty entities, bullets), lineHighlights, startupFolds; then a
plain client's inlay hints for the same (display falls back to hints)."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-notifs"); os.makedirs(d, exist_ok=True)
p = os.path.join(d, "n.org")
open(p, "w").write("#+STARTUP: indent overview hidestars\n* Top \\alpha x^2\n** Sub\nText.\n- item\n  - [X] done\n| a | b |\n* Two\n:PROPERTIES:\n:VISIBILITY: children\n:END:\n** Child\n")
opts = {"org_pretty_entities": True, "org_bullets_mode": True, "org_fontify_whole_heading_line": True}
for prof in (FULL, PLAIN):
    c = Client(profile=prof, root=d, options=opts, name="notifs")
    u = c.open(p); c.settle(1, 5)
    print("=====", prof)
    for m in ("org-lsp/virtualIndent", "org-lsp/display", "org-lsp/lineHighlights"):
        n = c.notes(m)
        print(" ", m, json.dumps(n[-1])[:400] if n else None)
    print("  startupFolds:", c.request("org-lsp/startupFolds", {"textDocument": {"uri": u}}))
    ih = c.request("textDocument/inlayHint", {"textDocument": {"uri": u}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 20, "character": 0}}})
    print("  inlay hints:", [(h["position"], h["label"], h.get("paddingLeft")) for h in ih or []][:12])
    toks = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": u}})
    print("  tokens line 1:", [(t[3], t[4], t[5]) for t in decode_tokens(toks["data"], c.legend, c.text(p), c.encoding) if t[0] == 1])
    c.shutdown()
# Post-shutdown
c = Client(profile=PLAIN, root=d, options={}, name="postshut")
u = c.open(p)
print("shutdown ->", c.request("shutdown", None))
for m, prm in (("textDocument/hover", {"textDocument": {"uri": u}, "position": {"line": 0, "character": 0}}),
               ("workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(p, 1, 2)]})):
    try:
        print(" after shutdown", m, "->", c.request(m, prm, timeout=5))
    except Exception as e:
        print(" after shutdown", m, "->", e)
c.notify("exit")
try:
    print(" exit code:", c.proc.wait(timeout=5))
except Exception as e:
    print(" not exited", e); c.kill()
