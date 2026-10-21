"""lsp-9: the server leaves its Emacs running when it exits while Emacs is
busy (a long job): shutdown+exit, and stdin closed."""
import os, sys, time, subprocess
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
def emacs_children(pid_before):
    out = subprocess.run(["ps", "-eo", "pid,ppid,etimes,args"], capture_output=True, text=True).stdout
    return [l[:80] for l in out.splitlines() if "bridge.el" in l and int(l.split()[0]) not in pid_before]
def pids():
    out = subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout
    return {int(l.split()[0]) for l in out.splitlines()[1:] if "bridge.el" in l}
for how in ("shutdown+exit", "stdin closed", "idle shutdown+exit"):
    before = pids()
    ws, options = workspace.make(os.path.join(SCRATCH, "ws-orphan"))
    c = Client(profile=FULL, root=ws, options=options, name="orphan")
    g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
    if how.startswith("idle"):
        # Emacs started and idle: run something quick through it.
        c.command("org.table.sum", c.at(os.path.join(ws, "gtd.org"), 0, 0)) if False else None
        c.open(os.path.join(ws, "tables.org"))
        c.command("org.table.sum", c.at(os.path.join(ws, "tables.org"), 4, 11)); c.settle(3, 10)
    else:
        # A job that keeps Emacs busy (a table of 3000 x 3000 empty fields).
        c.command("org.table.create", c.at(g, 73, 0, input="3000x3000"))
        time.sleep(2)
    print(how, "emacs running before exit:", emacs_children(before))
    t0 = time.time()
    if how == "stdin closed":
        c.proc.stdin.close()
        code = c.proc.wait(timeout=20)
    else:
        code = c.shutdown()
    print("   server exit", code, "after", round(time.time() - t0, 2), "s")
    time.sleep(3)
    left = emacs_children(before)
    print("   emacs left running 3 s later:", left)
    for l in left:
        os.kill(int(l.split()[0]), 9)
