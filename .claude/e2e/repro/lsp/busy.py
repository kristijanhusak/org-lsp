"""While Emacs is busy with a long job: does the server still answer, and
what do other Emacs-backed commands do?"""
import os, sys, time, threading, subprocess
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-busy"))
c = Client(profile=FULL, root=ws, options=options, name="busy", timeout=90)
g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
before = {int(l.split()[0]) for l in subprocess.run(["ps","-eo","pid,args"],capture_output=True,text=True).stdout.splitlines()[1:] if "bridge.el" in l}
c.command("org.table.create", c.at(g, 73, 0, input="2000x2000"))
time.sleep(1)
def timed(label, method, params, timeout=90):
    t0 = time.time()
    try:
        r = c.request(method, params, timeout=timeout); s = "ok"
    except Exception as e:
        r = str(e); s = "err"
    print(f"  {label:40} {s} {round(time.time()-t0,2)}s {str(r)[:100]}")
timed("hover (pure server)", "textDocument/hover", {"textDocument": {"uri": g}, "position": {"line": 64, "character": 22}})
timed("org.todo.next (pure server)", "workspace/executeCommand", {"command": "org.todo.next", "arguments": [c.at(g, 10, 3)]})
timed("org.edit.special (sync Emacs call)", "workspace/executeCommand", {"command": "org.edit.special", "arguments": [c.at(g, 82, 3)]})
timed("hover right after", "textDocument/hover", {"textDocument": {"uri": g}, "position": {"line": 64, "character": 22}})
timed("org.export.dispatch (menu)", "workspace/executeCommand", {"command": "org.export.dispatch", "arguments": [c.at(g, 10, 3)]})
c.shutdown()
for l in subprocess.run(["ps","-eo","pid,args"],capture_output=True,text=True).stdout.splitlines()[1:]:
    if "bridge.el" in l and int(l.split()[0]) not in before:
        os.kill(int(l.split()[0]), 9); print("  killed orphan", l.split()[0])
