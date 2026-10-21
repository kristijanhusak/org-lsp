"""Emacs-backed flows: export (menu walk), Babel with the confirmation
asked as showMessageRequest, tangle, footnotes; full and plain."""
import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
def mine(c):
    import subprocess
    out = subprocess.run(["ps", "-eo", "pid,ppid,args"], capture_output=True, text=True).stdout
    return [int(l.split()[0]) for l in out.splitlines()[1:] if "bridge.el" in l and int(l.split()[1]) == c.proc.pid]
for profile in ([p for p in (FULL, PLAIN) if p in os.environ.get("PROFILES", "full plain")]):
    ws, options = workspace.make(os.path.join(SCRATCH, f"ws-emacscmds-{profile}"))
    c = Client(profile=profile, root=ws, options=options, name="emacscmds", timeout=60)
    gp = os.path.join(ws, "gtd.org")
    if os.environ.get("NO_ROAM_ID"):
        t = open(gp).read().replace("[[id:0a1b2c3d-0000-4000-8000-000000000002][Alpha node]],", "")
        open(gp, "w").write(t)
    g = c.open(gp); c.settle(0.3, 5)
    print(f"===== {profile}")
    # Export: walk the menu: first step.
    r = c.command("org.export.dispatch", c.at(g, 10, 3)); c.settle(1, 20)
    if profile == FULL:
        print("export menu:", [ (ch["key"], ch["label"]) for ch in r["input"]["choices"]][:30])
        for keys in (["h", "h"], ["t", "U"], ["h", "H"], ["l", "l"]):
            r2, steps = c.run("org.export.dispatch", c.at(g, 10, 3), answers=keys, timeout=60)
            c.settle(2, 30)
            print(f"  export {keys}: steps {[json.dumps(s)[:120] for s in steps[1:]]} | msgs {[m['message'] for m in c.messages][-2:]} | shown {[s['uri'].rsplit('/',1)[-1] for s in c.shown][-1:]}")
        print("  files now:", sorted(f for f in os.listdir(ws) if f.startswith("gtd.")))
    else:
        c.picks = ["h", "h"]
        r = c.command("org.export.dispatch", c.at(g, 10, 3)); c.settle(3, 30)
        print("  plain export h h:", r and str(r)[:100], [m["message"] for m in c.messages][-3:], sorted(f for f in os.listdir(ws) if f.startswith("gtd.")))
    # Babel: python block (asks to confirm)
    def run_babel(answer):
        n = len(c.server_requests)
        c.pick = lambda params: next((a["title"] for a in params.get("actions", []) if a["title"].lower().startswith(answer)), None)
        r = c.command("org.babel.execute", c.at_text(g, "print(40")); c.settle(3, 30)
        reqs = [(m, p.get("message"), [a["title"] for a in (p.get("actions") or [])]) for m, p in c.server_requests[n:] if m == "window/showMessageRequest"]
        print(f"  babel ({answer}): result {r} asked {reqs} | msgs {[m['message'] for m in c.messages][-2:]}")
        print("     results in file:", [l for l in c.text(g).split("\n") if "RESULTS" in l or l.strip().startswith(": 42")])
    run_babel("no")
    run_babel("yes")
    # elisp block
    c.pick = lambda params: next((a["title"] for a in params.get("actions", []) if a["title"].lower().startswith("yes")), None)
    r = c.command("org.babel.execute", c.at_text(g, "(+ 1 2)")); c.settle(3, 30)
    print("  elisp block:", [l for l in c.text(g).split("\n") if l.strip().startswith(": 3")])
    r = c.command("org.babel.tangle", c.at(g, 0, 0)); c.settle(2, 20)
    tf = os.path.join(ws, "out", "hello.sh")
    print("  tangle:", r, os.path.exists(tf) and open(tf).read())
    # Open links
    for needle, d in (("[[https://orgmode.org]", 4), ("[[#custom]", 4), ("[[*Tasks]", 4), ("[[elisp:(+ 1 2)]]", 4),
                      ("[[file:tables.org]", 5), ("<2026-10-22 Thu 14:00", 3), ("[cite:@knuth1984]", 8), ("[fn:1]", 2),
                      ("https://example.com", 5)):
        n = len(c.shown); nm = len(c.messages); nr = len(c.server_requests)
        try:
            r = c.command("org.open", c.at_text(g, needle, delta=d)); c.settle(1, 10); c.run_fetches()
        except ResponseError as e:
            r = f"ERR {e}"
        sh = [(s["uri"].rsplit("/", 1)[-1][:40], s.get("external"), (s.get("selection") or {}).get("start")) for s in c.shown[n:]]
        asked = [p.get("message") for m, p in c.server_requests[nr:] if m == "window/showMessageRequest"]
        print(f"  open {needle[:30]!r:34} -> {str(r)[:80]} shown {sh} msgs {[m['message'] for m in c.messages[nm:]]} asked {asked}")
    pids = mine(c)
    print("  stderr:", c.stderr()[-300:])
    c.shutdown()
