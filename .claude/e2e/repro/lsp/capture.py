"""Capture end to end: each template, typing in the buffer, finish, abort,
refile; full and plain clients."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
for profile in (FULL, PLAIN):
    for key, action in (("t", "finish"), ("n", "finish"), ("j", "finish"), ("t", "abort"), ("t", "refile")):
        ws, options = workspace.make(os.path.join(SCRATCH, f"ws-cap-{profile}"))
        options["auto_save"] = True
        c = Client(profile=profile, root=ws, options=options, name="cap")
        g = c.open(os.path.join(ws, "gtd.org"))
        c.settle(0.3, 5)
        arg = c.at_text(g, "Write the report")
        if profile == FULL:
            r, steps = c.run("org.capture", arg, answers=[key])
        else:
            c.picks = [key]
            r = c.command("org.capture", arg); c.settle(0.5, 5); c.run_fetches()
        cap = [u for u in c.docs if "CAPTURE-" in u]
        if not cap:
            print(profile, key, "NO CAPTURE BUFFER", r, c.shown, c.messages); c.shutdown(); continue
        u = cap[-1]
        text = c.docs[u]["text"]
        sel = c.shown[-1].get("selection")
        # Type at the cursor the server put (%?).
        p = sel["start"] if sel else {"line": 0, "character": 0}
        c.type_at(u, p["line"], p["character"], "Captured 日本 🎉")
        if action == "finish":
            cmd = "org.capture.finish"; answers = []
        elif action == "abort":
            cmd = "org.capture.abort"; answers = []
        else:
            cmd = "org.capture.refile"; answers = ["Projects"]
        if profile == FULL:
            r2, steps = c.run(cmd, c.at(u, p["line"], 0), answers=answers)
        else:
            c.picks = list(answers)
            r2 = c.command(cmd, c.at(u, p["line"], 0)); c.settle(0.8, 8); c.run_fetches()
        c.settle(0.8, 8)
        notes = open(os.path.join(ws, "notes.org")).read()
        notes_buf = c.docs.get(c.uri(os.path.join(ws, "notes.org")), {}).get("text")
        print(f"--- {profile} {key} {action}: buffer {u.rsplit('/',1)[-1]} cursor {p}")
        print("   buffer text:", repr(text))
        print("   result:", r2, "| messages:", [m['message'] for m in c.messages][-3:], "| saves:", c.notes("org-lsp/save"))
        import difflib
        print("   notes.org on disk:", "".join(list(difflib.unified_diff(workspace.NOTES.splitlines(True), notes.splitlines(True), n=0))[2:]) or "(unchanged)")
        if notes_buf is not None and notes_buf != notes:
            print("   notes.org buffer differs from disk:", "".join(list(difflib.unified_diff(notes.splitlines(True), notes_buf.splitlines(True), n=0))[2:]))
        if c.edit_failures: print("   EDIT FAILURES", c.edit_failures)
        c.shutdown()
