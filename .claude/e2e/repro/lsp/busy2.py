"""lsp-10: a synchronous Emacs call (org.edit.special) blocks every other
answer of the server while Emacs is busy: a hover sent right after it
comes back only when the call times out (60 s)."""
import os, sys, time, subprocess
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace

def my_emacs(server_pid):
    """The bridge Emacs processes the server `server_pid` started (also
    after it exited, by their recorded pid)."""
    out = subprocess.run(["ps", "-eo", "pid,ppid,args"], capture_output=True, text=True).stdout
    return [int(l.split()[0]) for l in out.splitlines()[1:] if "bridge.el" in l and int(l.split()[1]) == server_pid]
ws, options = workspace.make(os.path.join(SCRATCH, "ws-busy2"))
c = Client(profile=FULL, root=ws, options=options, name="busy2", timeout=90)
g = c.open(os.path.join(ws, "gtd.org")); c.settle(0.3, 5)
c.command("org.table.create", c.at(g, 73, 0, input="2000x2000"))
time.sleep(1)
mine = my_emacs(c.proc.pid)
print('my emacs:', mine)
t0 = time.time()
c.next_id += 1; a = c.next_id
c._send({"id": a, "method": "workspace/executeCommand", "params": {"command": "org.edit.special", "arguments": [c.at(g, 82, 3)]}})
c.next_id += 1; b = c.next_id
c._send({"id": b, "method": "textDocument/hover", "params": {"textDocument": {"uri": g}, "position": {"line": 64, "character": 22}}})
got = {}
while len(got) < 2 and time.time() - t0 < 80:
    c.pump(lambda: any(i in c.responses and i not in got for i in (a, b)), 1)
    for i in (a, b):
        if i in c.responses and i not in got:
            got[i] = round(time.time() - t0, 2)
print("edit.special answered after", got.get(a), "s:", str(c.responses.get(a))[:120])
print("hover answered after", got.get(b), "s")
c.shutdown()
for pid in mine:
    try:
        os.kill(pid, 9); print('killed my orphaned Emacs', pid)
    except ProcessLookupError:
        pass
