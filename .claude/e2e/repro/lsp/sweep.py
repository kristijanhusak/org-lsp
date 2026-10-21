#!/usr/bin/env python3
"""Run every command of the manifest once, in a fresh server and a fresh
copy of the sample workspace, in a profile (full or plain), answering its
questions; record what happened in a JSON lines file.

    python3 sweep.py full [command-prefix...]
    python3 sweep.py plain [command-prefix...]
"""
import json
import os
import sys
import time
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import BARE, FULL, PLAIN, SCRATCH, Client, ResponseError, Timeout, path_of  # noqa
import workspace  # noqa

MANIFEST = json.load(open(os.path.join(SCRATCH, "..", "manifest.json")))

# Answers to text prompts, by the start of the prompt.
TEXT = [
    ("Table size", "3x2"),
    ("Number of clones", "2"),
    ("Effort", "0:30"),
    ("Property", "Owner"),
    ("Value", "me"),
    ("How much time left", "5"),
    ("Insert link", "https://example.com"),
    ("Link", "https://example.com"),
    ("Description", "Example"),
    ("Filter", "+work"),
    ("Narrow to entries", "report"),
    ("Mark entries matching", "report"),
    ("BibTeX file", "SCRATCH/out.bib"),
    ("Import file", "SCRATCH/table.csv"),
    ("Export table to", "SCRATCH/table-out.csv"),
    ("File", "WS/refs.bib"),
    ("Drawer", "NOTES"),
    ("Node", "Alpha"),
    ("Alias", "Alias1"),
    ("Remove", "A node"),
    ("Tag", "t1"),
    ("Ref", "https://example.com/beta"),
    ("Extract node to", "WS/roam/extracted.org"),
    ("Enter a string", "entry"),
    ("Topic", "Topic"),
    ("Type", "book"),
    ("Globally remove property", "Effort"),
    ("Tags", ":work:"),
    ("Note", "e2e note"),
    ("Search", "report"),
    ("Match", "work"),
]


def auto(ws):
    def answer(inp):
        kind = inp.get("kind")
        prompt = inp.get("prompt", "")
        choices = inp.get("choices") or []
        if kind in ("key", "choice") and choices:
            return choices[0]["key"]
        if kind == "date":
            return "+1d"
        if kind == "priority":
            return "B"
        if kind == "note":
            return "e2e note"
        if inp.get("completions"):
            return inp["completions"][0]
        for start, value in TEXT:
            if prompt.startswith(start):
                return value.replace("SCRATCH", SCRATCH).replace("WS", ws)
        return "test"

    return answer


# Where each command runs: (file, needle, delta) or a setup function name.
GTD_HEAD = ("gtd.org", "Write the report", 0)
SPEC = {
    "org.checkbox.toggle": ("gtd.org", "[ ] bread", 0),
    "org.toggle.item": ("gtd.org", "A paragraph.", 0),
    "org.cite.insert": ("gtd.org", "@knuth1984]", 2),
    "org.cite.insert.prefix": ("gtd.org", "@knuth1984]", 2),
    "org.footnote.action": ("gtd.org", "[fn:1]", 1),
    "org.footnote.menu": ("gtd.org", "[fn:1]", 1),
    "org.open": ("gtd.org", "[[#custom]", 3),
    "org.link.next": GTD_HEAD,
    "org.babel.execute": ("gtd.org", "print(40", 0),
    "org.babel.remove_result": ("gtd.org", "print(40", 0),
    "org.edit.special": ("gtd.org", "print(40", 0),
    "org.narrow.block": ("gtd.org", "print(40", 0),
    "org.babel.next": GTD_HEAD,
    "org.block.next": GTD_HEAD,
    "org.babel.tangle": GTD_HEAD,
    "org.timestamp.up": ("gtd.org", "<2026-10-21 Wed 10:00>", 3),
    "org.timestamp.down": ("gtd.org", "<2026-10-21 Wed 10:00>", 3),
    "org.timestamp.change": ("gtd.org", "<2026-10-22 Thu 14:00", 3),
    "org.shift.up": ("gtd.org", "<2026-10-21 Wed 10:00>", 10),
    "org.shift.down": ("gtd.org", "<2026-10-21 Wed 10:00>", 10),
    "org.shift.left": ("gtd.org", "<2026-10-22 Thu 14:00", 10),
    "org.shift.right": ("gtd.org", "<2026-10-22 Thu 14:00", 10),
    "org.date.later": ("gtd.org", "<2026-10-22 Thu 14:00", 10),
    "org.date.earlier": ("gtd.org", "<2026-10-22 Thu 14:00", 10),
    "org.goto.calendar": ("gtd.org", "<2026-10-22 Thu 14:00", 10),
    "org.timestamp.overlays.toggle": GTD_HEAD,
    "org.effort.increase": GTD_HEAD,
    "org.toggle.heading": ("gtd.org", "A paragraph.", 0),
    "org.table.toggle.column.width": ("tables.org", "|   3 |", 3),
    "org.plot.gnuplot": ("tables.org", "| Apple", 2),
    "org.table.ascii.plot": ("tables.org", "|   3 |", 3),
    "org.table.import": ("tables.org", "* Sort me", 0),
    "org.table.create": ("tables.org", "* Sort me", 0),
    "org.sort": ("tables.org", "* Headlines to sort", 2),
    "org.columns": GTD_HEAD,
    "org.latex.preview": GTD_HEAD,
    "org.link.preview": GTD_HEAD,
    "org.crypt.encrypt": GTD_HEAD,
    "org.subtree.clone": ("gtd.org", "Weekly review", 0),
    "org.inlinetask.insert": ("gtd.org", "A paragraph.", 0),
    "org.inlinetask.insert.nostate": ("gtd.org", "A paragraph.", 0),
    "org.drawer.insert": ("gtd.org", "A paragraph.", 0),
    "org.structure.template": ("gtd.org", "A paragraph.", 0),
    "org.emphasize": ("gtd.org", "A paragraph.", 0),
    "org.return": ("gtd.org", "A paragraph.", 2),
    "org.return.item": ("gtd.org", "2. second", 5),
    "org.meta.return": ("gtd.org", "2. second", 9),
    "org.insert.todo.heading": GTD_HEAD,
    "org.open.line": ("gtd.org", "A paragraph.", 2),
    "org.kill.line": GTD_HEAD,
    "org.beginning.of.line": ("gtd.org", "Write the report", 5),
    "org.end.of.line": GTD_HEAD,
    "org.yank": ("gtd.org", "* Notes", 0),
    "org.paste.subtree": ("gtd.org", "* Notes", 0),
    "org.levels.odd": GTD_HEAD,
    "org.levels.oddeven": GTD_HEAD,
    "org.ctags.find": GTD_HEAD,
    "org.bibtex.copy": GTD_HEAD,
    "org.id.create": ("gtd.org", "Weekly review", 0),
}
for c in MANIFEST["commands"]:
    n = c["command"]
    if n.startswith("org.table.") and n not in SPEC:
        SPEC[n] = ("tables.org", "|   3 |", 3)
    if n in ("org.shiftmeta.up", "org.shiftmeta.down"):
        SPEC[n] = ("tables.org", "| Pear", 2)
    if n.startswith("org.roam.") and n not in SPEC:
        SPEC[n] = ("roam/alpha.org", "Alpha links", 0)
    if n.startswith("org.journal.") and n not in SPEC:
        SPEC[n] = ("journal/20261020", "Morning entry", 0)
    if n.startswith("org.babel.") and n not in SPEC:
        SPEC[n] = ("gtd.org", "print(40", 0)
    if n.startswith("org.dblock.") or n == "org.clock.report":
        SPEC[n] = ("notes.org", "#+BEGIN: clocktable", 2)
    if n.startswith("org.bibtex.") and n not in SPEC:
        SPEC[n] = GTD_HEAD
    if n.startswith("org.timer."):
        SPEC[n] = ("gtd.org", "A paragraph.", 0)


def scope(command):
    keys = command.get("keys", [])
    ins = {k["in"] for k in keys}
    if ins == {"agenda"} or command["command"].startswith("org.agenda.") and command["command"] not in (
        "org.agenda.dispatch", "org.agenda.list", "org.agenda.todos", "org.agenda.appt"):
        return "agenda"
    if "calendar" in ins and "org" not in ins and not command["command"].startswith("org.journal."):
        return "calendar"
    if command["command"].startswith("org.calendar."):
        return "calendar"
    if "columns" in ins:
        return "columns"
    if command["command"] in ("org.capture.finish", "org.capture.abort", "org.capture.refile"):
        return "capture"
    return "org"


def text_prompt(messages):
    for m in messages:
        if "answer with a rename" in m or "pick the answer from the code actions" in m:
            return m
    return None


def run_one(profile, command, record):
    name = command["command"]
    ws = os.path.join(SCRATCH, f"ws-sweep-{profile}")
    ws, options = workspace.make(ws)
    with open(os.path.join(SCRATCH, "table.csv"), "w") as f:
        f.write("a,b\n1,2\n")
    c = Client(profile=profile, root=ws, options=options, name=f"sweep-{profile}-{name}", timeout=20)
    answer = auto(ws)
    t0 = time.time()
    rec = {"command": name, "profile": profile}
    try:
        sc = scope(command)
        rec["scope"] = sc
        argument = None
        if sc == "agenda":
            c.open(os.path.join(ws, "gtd.org"))
            r = c.command("org.agenda.list", {})
            c.settle(0.5, 10)
            c.run_fetches()
            agenda = [u for u in c.docs if "gtd.org" not in u]
            rec["agenda_docs"] = agenda
            if not agenda:
                rec["setup_error"] = f"no agenda document shown: {c.shown}"
                return rec
            uri = agenda[-1]
            lines = c.docs[uri]["text"].split("\n")
            line = next((i for i, l in enumerate(lines) if "Write the report" in l), 2)
            argument = c.at(uri, line, 2)
        elif sc == "calendar":
            g = c.open(os.path.join(ws, "gtd.org"))
            r = c.command("org.schedule", c.at_text(g, "Weekly review"))
            c.settle(0.5, 10)
            c.run_fetches()
            dates = [u for u in c.docs if "Date" in u]
            rec["date_docs"] = dates
            if not dates:
                rec["setup_error"] = f"no date prompt shown: result {r}, shown {c.shown}"
                return rec
            sel = (c.shown[-1].get("selection") or {}).get("start", {"line": 3, "character": 10})
            argument = c.at(dates[-1], sel["line"], sel["character"])
        elif sc == "columns":
            g = c.open(os.path.join(ws, "gtd.org"))
            r = c.command("org.columns", c.at_text(g, "Write the report"), timeout=40)
            rec["setup_result"] = r
            c.settle(0.5, 10)
            argument = c.at_text(g, "Write the report", column=1)
        elif sc == "capture":
            g = c.open(os.path.join(ws, "gtd.org"))
            if profile == FULL:
                r, steps = c.run("org.capture", c.at_text(g, "Write the report"), answers=["t"])
            else:
                c.picks = ["t"]
                r = c.command("org.capture", c.at_text(g, "Write the report"))
                c.settle(0.5, 10)
                c.run_fetches()
            cap = [u for u in c.docs if "CAPTURE-" in u]
            rec["capture_docs"] = cap
            if not cap:
                rec["setup_error"] = f"no capture buffer: {r} shown {c.shown}"
                return rec
            argument = c.at(cap[-1], 0, 0)
        else:
            f, needle, delta = SPEC.get(name, GTD_HEAD)
            p = c.open(os.path.join(ws, f))
            argument = c.at_text(p, needle, delta=delta)
        if name in ("org.yank", "org.paste.subtree", "org.bibtex.yank"):
            argument["clipboard"] = "* Pasted\n** Child\n" if name != "org.bibtex.yank" else open(
                os.path.join(ws, "refs.bib")).read()
        rec["argument"] = argument
        before = {u: d["text"] for u, d in c.docs.items()}
        nmsg = len(c.messages)
        if profile == FULL:
            result, steps = c.run(name, argument, answers=[answer] * 8, timeout=40)
            rec["steps"] = steps
            c.apply_result(argument, result)
        else:
            result = c.command(name, argument, timeout=40)
            rec["steps"] = [result]
            c.apply_result(argument, result)
            c.settle(0.5, 10)
            # The server asks; answer text prompts with a rename.
            for _ in range(6):
                q = text_prompt(c.last_messages(nmsg))
                if not q:
                    break
                nmsg = len(c.messages)
                pos = argument["position"]
                uri = argument["textDocument"]["uri"]
                try:
                    prep = c.request("textDocument/prepareRename", {"textDocument": {"uri": uri}, "position": pos})
                    rec.setdefault("prepare", []).append(prep)
                    inp = {"prompt": q.split(":")[0], "kind": "text"}
                    if "pick the answer" in q:
                        acts = c.code_actions(uri, pos["line"], pos["character"])
                        rec.setdefault("answer_actions", []).append([a.get("title") for a in acts[:5]])
                        if acts:
                            c.run_action(acts[0])
                        continue
                    ans = answer(dict(inp, kind="date" if "date" in q.lower() or "Schedule" in q or "Deadline" in q else "text"))
                    r = c.request("textDocument/rename", {"textDocument": {"uri": uri}, "position": pos, "newName": ans})
                    rec.setdefault("renames", []).append([q, ans, r])
                    if r:
                        c._apply_workspace_edit(r)
                    c.settle(0.5, 10)
                    c.run_fetches()
                except ResponseError as err:
                    rec.setdefault("rename_errors", []).append([q, str(err)])
                    break
        rec["result"] = result
    except ResponseError as err:
        rec["error"] = {"code": err.code, "message": err.message, "data": err.data}
    except Timeout as err:
        rec["timeout"] = str(err)
    except Exception as err:  # noqa
        rec["exception"] = traceback.format_exc()
    finally:
        try:
            c.settle(0.3, 5)
            c.run_fetches()
        except Exception:
            pass
        rec["elapsed"] = round(time.time() - t0, 2)
        rec["messages"] = [m["message"] for m in c.messages]
        rec["shown"] = [s["uri"] for s in c.shown]
        rec["edit_failures"] = c.edit_failures
        rec["applied"] = len(c.applied)
        rec["notes"] = sorted({m for _, m, _ in c.notifications if not m.startswith("textDocument/publish")})
        changed = {}
        for u, d in c.docs.items():
            if before.get(u) != d["text"] if "before" in dir() else False:
                changed[u] = d["text"]
        rec["changed"] = {u: _diff(before.get(u, ""), t) for u, t in changed.items()} if "before" in dir() else {}
        rec["requests"] = [m for m, _ in c.server_requests]
        code = c.shutdown()
        rec["exit"] = code
        err = c.stderr()
        if err.strip():
            rec["stderr"] = err[-3000:]
        rec["trace"] = c.trace_path
    return rec


def _diff(a, b):
    import difflib

    return "".join(list(difflib.unified_diff(a.splitlines(True), b.splitlines(True), n=1))[2:80])


def main():
    profile = sys.argv[1]
    prefixes = sys.argv[2:]
    out = os.path.join(SCRATCH, f"sweep-{profile}.jsonl")
    mode = "a" if prefixes else "w"
    with open(out, mode) as f:
        for command in MANIFEST["commands"]:
            name = command["command"]
            if prefixes and not any(name == p or (p.endswith(".") and name.startswith(p)) for p in prefixes):
                continue
            rec = run_one(profile, command, None)
            f.write(json.dumps(rec, default=str) + "\n")
            f.flush()
            flag = "ERR" if rec.get("error") else ("TIMEOUT" if rec.get("timeout") else ("EXC" if rec.get("exception") else "ok"))
            print(f"{flag:7} {name:45} {rec.get('elapsed')}s {str(rec.get('error') or rec.get('timeout') or rec.get('setup_error') or '')[:120]}", flush=True)


if __name__ == "__main__":
    main()
