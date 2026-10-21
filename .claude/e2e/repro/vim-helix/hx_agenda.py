"""Helix: the agenda's code actions and edits."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
A_ = "* TODO Write report :work:\nSCHEDULED: <2026-05-18 Mon 09:00>\n* TODO Call :home:\nDEADLINE: <2026-05-20 Wed>\n:PROPERTIES:\n:Effort: 0:30\n:END:\n"
h = Helix("agenda", {"files": {"a.org": A_, "b.org": "* TODO Bee\nSCHEDULED: <2026-05-18 Mon>\n"},
                     "options": {"org_todo_keywords": ["TODO(t)", "|", "DONE(d)"]}})
def step(name, fn):
    try:
        r = fn(); print(f"ok   {name}: {r if r is not None else ''}")
    except Exception as e:
        print(f"FAIL {name}: {str(e).splitlines()[0][:400]}")
        try: h.keys("Escape"); h.settle()
        except Exception: pass
def ag():
    return h.log.documents.get(h.agenda()) or ""
def find(text):
    lines = ag().split("\n"); n = next(i for i, l in enumerate(lines, 1) if text in l); goto(h, n, 3); return n
try:
    A = os.path.join(h.directory, "a.org"); h.start(A)
    step("open agenda", lambda: (act(h, "Agenda for the current week or day"), str(h.status()[1:]), ag().split("\n")[:3])[1:])
    def entry_actions():
        find("Write report"); t = titles(h); return t
    step("actions on an entry line", entry_actions)
    def date_actions():
        n = next(i for i, l in enumerate(ag().split("\n"), 1) if l.startswith("Tuesday")); goto(h, n, 1); return titles(h)
    step("actions on a date line", date_actions)
    def todo():
        find("Write report"); act(h, "Change the TODO state", ["d"]); return [l for l in ag().split("\n") if "Write" in l] + [h.messages().strip()]
    step("t → DONE, line follows", todo)
    def prio():
        find("Bee"); act(h, "Raise the priority"); return [l for l in ag().split("\n") if "Bee" in l] + [h.messages().strip()]
    step("raise priority", prio)
    def sched():
        find("Bee"); act(h, "Schedule the entry", ["+1d"]); return [l for l in ag().split("\n") if "Bee" in l] + [h.text(os.path.join(h.directory, "b.org"))]
    step("schedule +1d", sched)
    def hover():
        find("Call"); h.keys("Space", "k"); time.sleep(1.5); h.settle(); scr = "\n".join(h.screen()); h.keys("Escape"); h.settle()
        return "Effort" in scr or "DEADLINE" in scr
    step("Space k hover shows the entry", hover)
    def filt():
        find("Call"); act(h, "Agenda: filter by the line's category"); r = [l for l in ag().split("\n") if "Bee" in l or "Call" in l]
        act(h, "Agenda: remove all filters"); return [r, len([l for l in ag().split("\n") if "Bee" in l])]
    step("filter by category, remove", filt)
    def save():
        act(h, "Save all Org files"); return [h.messages().strip(), open(os.path.join(h.directory, "a.org")).read().split("\n")[:2]]
    step("save all", save)
    def gd():
        find("Call"); h.keys("g", "d"); h.settle(); return str(h.status()[1:])
    step("gd on agenda line", gd)
    def back():
        h.go_to_buffer(h.agenda()); return str(h.status()[1:])
    step("Space b back to agenda", back)
    def later():
        act(h, "Agenda: go forward in time"); return ag().split("\n")[:2]
    step("later", later)
    def todos():
        act(h, "Agenda commands", ["t"]); return ag().split("\n")[:5]
    step("Agenda commands → t", todos)
finally:
    h.stop()
