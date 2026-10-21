"""Clocking: in, the org-lsp/clock notification, goto, out, in last,
cancel, display (inlay hints), switching tasks, a dangling clock at start."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for profile in (FULL, PLAIN):
    ws, options = workspace.make(os.path.join(SCRATCH, f"ws-clock-{profile}"))
    c = Client(profile=profile, root=ws, options=options, name="clock")
    g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
    print("=====", profile)
    def run(cmd, needle=None, **kw):
        n = len(c.messages)
        arg = c.at_text(g, needle) if needle else c.at(g, 0, 0)
        try:
            r = c.command(cmd, dict(arg, **kw))
            c.apply_result(arg, r)
        except ResponseError as e:
            r = f"ERR {e}"
        c.settle(0.5, 5); c.run_fetches()
        print(f"  {cmd} {needle or ''}: {json.dumps(r)[:160]} msgs {[m['message'] for m in c.messages[n:]]}")
    run("org.clock.in", "Weekly review")
    print("   clock note:", c.notes("org-lsp/clock")[-1:])
    run("org.clock.goto")
    print("   shown:", c.shown[-1:] )
    run("org.clock.in", "Call the plumber")   # switching
    print("   clock note:", c.notes("org-lsp/clock")[-1:])
    print("   weekly review logbook:", [l for l in c.text(g).split("\n") if "CLOCK:" in l])
    run("org.clock.out", "Call the plumber")
    print("   clock note:", c.notes("org-lsp/clock")[-1:])
    run("org.clock.in.last", "A paragraph.")
    run("org.clock.cancel", "A paragraph.")
    run("org.clock.display", "Write the report")
    ih = c.request("textDocument/inlayHint", {"textDocument": {"uri": g}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 120, "character": 0}}})
    print("   inlay hints:", [(h["position"]["line"], h["label"]) for h in ih or []][:8])
    run("org.num.mode", "Write the report")
    ih = c.request("textDocument/inlayHint", {"textDocument": {"uri": g}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 120, "character": 0}}})
    print("   inlay hints with num mode:", [(h["position"]["line"], h["label"]) for h in ih or []][:8])
    print("   CLOCK lines:", [l.strip() for l in c.text(g).split("\n") if "CLOCK:" in l])
    print("   failures", c.edit_failures, c.stderr()[-200:])
    c.save_all()
    c.shutdown()
# A clock left running in a file, at start: resolve
ws, options = workspace.make(os.path.join(SCRATCH, "ws-clock-dangling"))
p = os.path.join(ws, "gtd.org")
t = open(p).read().replace("   CLOCK: [2026-10-20 Tue 09:00]--[2026-10-20 Tue 10:30] =>  1:30", "   CLOCK: [2026-10-21 Wed 01:00]")
open(p, "w").write(t)
c = Client(profile=FULL, root=ws, options=options, name="clock2")
g = c.open(p); c.settle(1, 5)
print("dangling clock at start: clock notes", c.notes("org-lsp/clock"), "messages", [m["message"] for m in c.messages])
r, steps = c.run("org.clock.resolve", c.at(g, 10, 3), answers=["k"])
print("resolve:", [json.dumps(s)[:300] for s in steps])
print("  line:", [l.strip() for l in c.text(g).split("\n") if "CLOCK:" in l])
c.shutdown()
