"""TODO list: which entries lose their keyword on their line once done."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-inh2"); os.makedirs(d, exist_ok=True)
cases = {
 "logdone-tag": ("* Project :work:\n** TODO Child\n", {"org_log_done": "time"}),
 "logdone-notag": ("* Project\n** TODO Child\n", {"org_log_done": "time"}),
 "logdone-top": ("* TODO Child :work:\n", {"org_log_done": "time"}),
 "plain": ("* Project :work:\n** TODO Child\n", {}),
 "log": ("#+TODO: TODO | DONE(d!)\n* Project :work:\n** TODO Child\n", {"org_log_into_drawer": True}),
 "ordered": ("* Project :work:\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO Child\n", {}),
 "ordered-notag": ("* Project\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO Child\n", {}),
 "todo-parent": ("* TODO Project [0/1]\n** TODO Child\n", {}),
 "level3": ("* A\n** B :x:\n*** TODO Child\n", {}),
}
for name, (text, opts) in cases.items():
    p = os.path.join(d, name + ".org"); open(p, "w").write(text)
    o = {"org_agenda_files": [p]}; o.update(opts)
    c = Client(profile=FULL, root=d, options=o, name="inh2")
    c.open(p)
    c.command("org.agenda.todos", {}); c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if x.startswith("org-agenda")][0]
    lines = c.docs[u]["text"].split("\n")
    i = next(i for i, l in enumerate(lines) if "TODO Child" in l)
    r, steps = c.run("org.todo", c.at(u, i, 2), answers=["d"])
    c.settle(1, 10); c.run_fetches()
    print(f"{name:14} {lines[i].strip()!r:50} -> {c.docs[u]['text'].split(chr(10))[i].strip()!r}")
    c.shutdown()
