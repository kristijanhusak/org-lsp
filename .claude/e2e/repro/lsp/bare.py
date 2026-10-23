"""A bare client (no window.showMessage, no window.showDocument): what the
code actions offer, and what the commands that ask or show do."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for label, prof, opts in (("bare", BARE, {}), ("bare+show_document", BARE, {"show_document": True}),
                          ("bare+message_requests=false", BARE, {"message_requests": False}),
                          ("plain+message_requests=false", PLAIN, {"message_requests": False})):
    ws, options = workspace.make(os.path.join(SCRATCH, "ws-bare"))
    options.update(opts)
    c = Client(profile=prof, root=ws, options=options, name="bare")
    g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
    acts = c.code_actions(g, 10, 5)
    print(f"== {label}: {len(acts)} actions:", [a["title"] for a in acts])
    for cmd in ("org.agenda.list", "org.capture", "org.todo", "org.schedule"):
        n = len(c.messages); nreq = len(c.server_requests)
        try:
            r = c.command(cmd, c.at_text(g, "Write the report"))
            c.settle(0.5, 5)
            print(f"   {cmd}: result {json.dumps(r)[:150]} | messages {[m['message'] for m in c.messages[n:]]} | requests {[m for m, _ in c.server_requests[nreq:]]}")
        except ResponseError as e:
            print(f"   {cmd}: ERROR {e}")
    if "message_requests" in label:
        # The choices come first in the code actions.
        acts = c.code_actions(g, 10, 5)
        print("   actions after org.todo asked:", [(a["title"], a.get("kind")) for a in acts][:8])
        ans = [a for a in acts if a.get("kind") == "org.answer"]
        if ans:
            c.run_action(ans[1]); c.settle(0.5, 5)
            print("   picked", ans[1]["title"], "->", [l for l in c.text(g).split("\n") if "Write the report" in l], [m['message'] for m in c.messages][-2:])
    c.shutdown()
