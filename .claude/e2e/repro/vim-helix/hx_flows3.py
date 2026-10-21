"""Helix: inlay hints (shrink, indent), completion, capture abort/refile, agenda note, effort."""
import sys; sys.path.insert(0, "/home/user/org-lsp/.claude/e2e/repro/vim-helix")
from hxlib import *
FILE = "#+STARTUP: indent\n* Head :work:\ntext under\n| <5>   |\n| long text here |\n* TODO Task\nSCHEDULED: <2026-05-18 Mon>\n"
opts = {"org_capture_templates": [{"key": "t", "description": "Task", "target": ["file", "$DIR/inbox.org"], "template": "* TODO %?"}]}
h = Helix("flows3", {"files": {"a.org": FILE, "inbox.org": "#+title: Inbox\n"}, "options": opts})
with open(os.path.join(h.config, "config.toml"), "a") as f:
    f.write("\n[editor.lsp]\ndisplay-inlay-hints = true\n")
def step(name, fn):
    try:
        r = fn(); print(f"ok   {name}: {r if r is not None else ''}")
    except Exception as e:
        print(f"FAIL {name}: {str(e).splitlines()[0][:400]}")
        try: h.keys("Escape"); h.settle()
        except Exception: pass
try:
    A = os.path.join(h.directory, "a.org"); h.start(A); time.sleep(2); h.settle()
    step("indent as inlay hints", lambda: [r.rstrip() for r in h.screen()[:7]])
    def shrink():
        goto(h, 4, 3); act(h, "Shrink or expand the table column"); time.sleep(1.5); h.settle(); return [r.rstrip() for r in h.screen()[3:6]]
    step("shrink <5>", shrink)
    def compl():
        goto(h, 3, 1); h.keys("o"); h.type("[["); time.sleep(2); h.settle(); s = [r.strip()[:60] for r in h.screen() if "Head" in r or "Task" in r or "file:" in r][:6]; h.keys("Escape"); h.keys("u"); h.settle(); return s
    step("completion after [[", compl)
    def tagcomp():
        goto(h, 6, 12); h.keys("A"); h.type(" :w"); time.sleep(2); h.settle(); s = [r.strip()[:60] for r in h.screen() if "work" in r][:4]; h.keys("Escape"); h.keys("u"); h.settle(); return s
    step("tag completion after : at headline end", tagcomp)
    def effort():
        goto(h, 6, 3); act(h, "Set the effort of the entry", ["0:45"]); return h.text(A).split("\n")[5:10]
    step("effort by rename", effort)
    def cap_abort():
        act(h, "Capture", ["t"]); h.type_text("Abort me"); act(h, "Capture: abort"); return [h.text(os.path.join(h.directory, "inbox.org")), str(h.status()[1:]), h.messages().strip()]
    step("capture abort", cap_abort)
    def cap_refile():
        h.open(A); act(h, "Capture", ["t"]); h.type_text("Refile me"); act(h, "Capture: refile the entry", ["Head"]); return [h.text(A).split("\n")[1:5], h.messages().strip()]
    step("capture refile to Head", cap_refile)
    def note():
        h.open(A); act(h, "Agenda for the current week or day")
        ag = h.log.documents.get(h.agenda()) or ""; n = next(i for i, l in enumerate(ag.split("\n"), 1) if "Task" in l); goto(h, n, 3)
        act(h, "Add a note to the entry", ["noted here"]); return [l for l in h.text(A).split("\n") if "Note" in l or "noted" in l] + [h.messages().strip()]
    step("agenda note by rename", note)
finally:
    h.stop()
