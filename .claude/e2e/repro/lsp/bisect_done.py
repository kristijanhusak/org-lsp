import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
def trial(text, opts_override=None, target="TODO Ship it", key="d"):
    ws, opts = workspace.make(os.path.join(SCRATCH, "ws-bis"))
    p = os.path.join(ws, "gtd.org"); open(p, "w").write(text)
    opts["org_agenda_files"] = [p]
    if opts_override: opts.update(opts_override)
    c = Client(profile=FULL, root=ws, options=opts, name="bis")
    c.open(p)
    c.command("org.agenda.todos", {}); c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if x.startswith("org-agenda")][0]
    lines = c.docs[u]["text"].split("\n")
    i = next(i for i, l in enumerate(lines) if target in l)
    c.run("org.todo", c.at(u, i, 2), answers=[key]); c.settle(1, 10); c.run_fetches()
    after = c.docs[u]["text"].split("\n")[i]
    c.shutdown()
    return after
if __name__ == "__main__":
    base = workspace.GTD
    print("full sample:", trial(base))
    print("no #+TODO line:", trial(base.replace("#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)\n", "")))
    small = "#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)\n* Tasks\n** TODO Project Alpha [1/3]                                        :work:\n   :PROPERTIES:\n   :ORDERED:  t\n   :END:\n*** DONE Plan it\n*** TODO Build it\n*** TODO Ship it\n"
    print("small:", trial(small))
    print("small no cookie:", trial(small.replace(" [1/3]", "")))
    print("small no ORDERED:", trial(small.replace("   :PROPERTIES:\n   :ORDERED:  t\n   :END:\n", "")))
    print("small no #+TODO:", trial(small.replace("#+TODO: TODO NEXT WAIT(w@) | DONE(d!) CANCELLED(c@)\n", "")))
