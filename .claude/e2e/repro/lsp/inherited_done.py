"""TODO list: marking an entry with inherited tags done drops its keyword from its line."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
d = os.path.join(SCRATCH, "ws-inh"); os.makedirs(d, exist_ok=True)
for name, text in {"a.org": "* Project :work:\n** TODO Child\n** TODO Other\n* TODO Plain :home:\n"}.items():
    open(os.path.join(d, name), "w").write(text)
for variant in ("inherited", "local"):
    c = Client(profile=FULL, root=d, options={"org_agenda_files": [os.path.join(d, "a.org")]}, name="inh")
    c.open(os.path.join(d, "a.org"))
    c.command("org.agenda.todos", {}); c.settle(0.5, 5); c.run_fetches()
    u = [x for x in c.docs if x.startswith("org-agenda")][0]
    lines = c.docs[u]["text"].split("\n")
    target = "TODO Child" if variant == "inherited" else "TODO Plain"
    i = next(i for i, l in enumerate(lines) if target in l)
    r, steps = c.run("org.todo", c.at(u, i, 2), answers=["d"]) if False else (None, None)
    r = c.command("org.todo.next", c.at(u, i, 2))  # TODO -> DONE
    c.settle(1, 10); c.run_fetches()
    print(variant, "before:", repr(lines[i]))
    print(variant, "after: ", repr(c.docs[u]["text"].split("\n")[i]))
    print("   file:", c.text(os.path.join(d, "a.org")).split("\n")[1 if variant == "inherited" else 3])
    c.shutdown()
