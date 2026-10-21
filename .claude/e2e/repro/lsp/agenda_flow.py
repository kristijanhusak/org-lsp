#!/usr/bin/env python3
"""Agenda edits that write back to files, in org-lsp (full or plain
client) and in Emacs (batch), compared: the agenda's text after the
action and the files.

    python3 agenda_flow.py full|plain [scenario...]
"""
import difflib
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import FULL, PLAIN, SCRATCH, Client, ResponseError  # noqa
import compare  # noqa
import workspace  # noqa


def emacs_agenda(ws, line_match, form, answers, extra="", view="(org-agenda-list)"):
    answers_lisp = "(list " + " ".join(json.dumps(a) for a in answers) + ")"
    code = (compare.EMACS_PRELUDE % {"org": compare.ORG, "ws": ws, "scratch": SCRATCH, "extra": extra}) + f"""
(setq e2e-answers {answers_lisp})
(setq org-agenda-window-setup 'current-window org-agenda-sticky nil)
{view}
(goto-char (point-min))
(re-search-forward {json.dumps(line_match)})
(setq last-command 'e2e-previous this-command 'e2e-command)
(cl-letf (((symbol-function 'read-string) #'e2e-pop)
          ((symbol-function 'read-from-minibuffer) #'e2e-pop)
          ((symbol-function 'completing-read) #'e2e-pop)
          ((symbol-function 'org-completing-read) #'e2e-pop)
          ((symbol-function 'read-char-exclusive) #'e2e-char)
          ((symbol-function 'read-char) #'e2e-char)
          ((symbol-function 'y-or-n-p) (lambda (&rest _) t))
          ((symbol-function 'yes-or-no-p) (lambda (&rest _) t)))
  (condition-case err (progn {form} (run-hooks 'post-command-hook))
    (error (princ (format "E2E-ERROR: %S\\n" err)))))
(princ "E2E-AGENDA-START\\n")
(princ (buffer-substring-no-properties (point-min) (point-max)))
(princ "E2E-AGENDA-END\\n")
(princ (format "E2E-MESSAGE: %s\\n" (current-message)))
(save-some-buffers t)
"""
    r = subprocess.run(["emacs", "--batch", "-Q", "--eval", "(progn " + code + ")"], capture_output=True, text=True,
                       timeout=90, stdin=subprocess.DEVNULL)
    out = r.stdout
    agenda = out.split("E2E-AGENDA-START\n", 1)[-1].split("E2E-AGENDA-END", 1)[0] if "E2E-AGENDA-START" in out else ""
    return agenda, out, r.stderr


def server_agenda(ws, options, profile, line_match, command, answers, open_files=True, view="org.agenda.list",
                  view_answers=()):
    extra = {"experimental": {"orgDatePrompt": False}} if profile == FULL else None
    c = Client(profile=profile, root=ws, options=options, name="agenda-flow", timeout=30, caps_extra=extra)
    if open_files:
        c.open(os.path.join(ws, "gtd.org"))
    c.settle(0.3, 5)
    if view_answers:
        r, steps = c.run(view, {}, answers=list(view_answers))
    else:
        c.command(view, {})
    c.settle(0.5, 10)
    c.run_fetches()
    agenda = [u for u in c.docs if "gtd.org" not in u and "notes.org" not in u]
    if not agenda:
        return None, c, "no agenda shown"
    uri = agenda[-1]
    lines = c.docs[uri]["text"].split("\n")
    line = next((i for i, l in enumerate(lines) if re.search(line_match, l)), None)
    if line is None:
        return None, c, f"no line matching {line_match}"
    arg = c.at(uri, line, 2)
    err = None
    try:
        if profile == FULL:
            res, steps = c.run(command, arg, answers=list(answers), timeout=40)
        else:
            # Keys are picked among the buttons of showMessageRequest,
            # text is typed as a rename at the cursor.
            queue = list(answers)
            seen = len(c.messages)
            c.picks = list(queue)
            res = c.command(command, arg, timeout=40)
            for _ in range(6):
                c.settle(0.5, 10)
                asked = [m["message"] for m in c.messages[seen:] if "answer with a rename" in m["message"]]
                seen = len(c.messages)
                if not asked:
                    break
                # The answers the buttons did not take, in order.
                left = c.picks if c.picks else queue
                text = left.pop(0) if left else ""
                c.picks = []
                queue = left
                p = arg["position"]
                c.rename_answer(arg["textDocument"]["uri"], p["line"], p["character"], text)
        c.apply_result(arg, res)
    except ResponseError as e:
        err = str(e)
    c.settle(1.5, 20)
    c.run_fetches()
    c.settle(0.5, 10)
    c.run_fetches()
    return (c.docs[uri]["text"], err), c, None


SC = []


def sc(**kw):
    SC.append(kw)


sc(name="todo-done", match="Scheduled:  TODO \\[#A\\] Write", command="org.todo", answers=["d"],
   emacs="(org-agenda-todo \"DONE\")")
sc(name="todo-next", match="Scheduled:  TODO \\[#A\\] Write", command="org.todo.next", answers=[],
   emacs="(org-agenda-todo 'right)")
sc(name="todo-repeat", match="Scheduled:  TODO Weekly", command="org.todo", answers=["d"],
   emacs="(org-agenda-todo \"DONE\")")
sc(name="habit-done", match="TODO Water the plants", command="org.todo", answers=["d"],
   emacs="(org-agenda-todo \"DONE\")")
sc(name="prio-up", match="Deadline:   NEXT Call", command="org.priority.up", answers=[],
   emacs="(org-agenda-priority-up)")
sc(name="schedule", match="Deadline:   NEXT Call", command="org.schedule", answers=["+3d"],
   emacs="(org-agenda-schedule nil)", emacs_answers=["+3d"])
sc(name="deadline", match="Scheduled:  TODO Weekly", command="org.deadline", answers=["fri"],
   emacs="(org-agenda-deadline nil)", emacs_answers=["fri"])
sc(name="date-later", match="Meeting with Bob", command="org.date.later", answers=[],
   emacs="(org-agenda-date-later 1)")
sc(name="date-earlier-past", match="2 d. ago:  NEXT Call", command="org.date.earlier", answers=[],
   emacs="(org-agenda-date-earlier 1)")
sc(name="date-later-past", match="2 d. ago:  NEXT Call", command="org.date.later", answers=[],
   emacs="(org-agenda-date-later 1)")
sc(name="tags", match="Scheduled:  TODO Weekly", command="org.tags.set", answers=[":home:"],
   emacs="(org-agenda-set-tags \"home\" 'on)")
sc(name="archive", match="Meeting with Bob", command="org.archive.subtree", answers=["y"],
   emacs="(org-agenda-archive)")
sc(name="clock-in", match="Scheduled:  TODO Weekly", command="org.clock.in", answers=[],
   emacs="(org-agenda-clock-in)")
sc(name="property", match="Scheduled:  TODO Weekly", command="org.property.set", answers=["Owner", "me"],
   emacs="(org-agenda-set-property)", emacs_answers=["Owner", "me"])
sc(name="effort", match="Scheduled:  TODO Weekly", command="org.effort.set", answers=["0:30"],
   emacs="(org-agenda-set-effort)", emacs_answers=["0:30"])
sc(name="todo-list-done", view="org.agenda.todos", eview="(org-todo-list)", match="TODO Build it", command="org.todo",
   answers=["d"], emacs="(org-agenda-todo \"DONE\")")
sc(name="todo-list-cjk-tags", view="org.agenda.todos", eview="(org-todo-list)", match="TODO Groceries", command="org.tags.set",
   answers=[":家:"], emacs="(org-agenda-set-tags \"家\" 'on)")


def run(profile, sc):
    base = os.path.join(SCRATCH, "agflow")
    we, opts = workspace.make(os.path.join(base, "emacs"))
    wsv, opts = workspace.make(os.path.join(base, "server"))
    eag, out, err = emacs_agenda(we, sc["match"], sc["emacs"], sc.get("emacs_answers", []),
                                 view=sc.get("eview", "(org-agenda-list)"))
    res, c, problem = server_agenda(wsv, opts, profile, sc["match"], sc["command"], sc.get("answers", []),
                                    view=sc.get("view", "org.agenda.list"))
    print(f"===== {sc['name']} ({profile}): {sc['command']} on /{sc['match']}/")
    print("  emacs:", [l for l in out.splitlines() if l.startswith("E2E-E") or l.startswith("E2E-M")], err.strip().splitlines()[-2:])
    if problem:
        print("  SERVER PROBLEM:", problem)
    else:
        sag, serr = res
        print("  server error:", serr, "messages:", [m["message"] for m in c.messages][-3:],
              "edit failures:", c.edit_failures)
        notes = sorted({m for _, m, _ in c.notifications})
        print("  server notifications:", notes)
        # Agenda text, ignoring the time grid's "now" line and the
        # difference in the tags column (Emacs batch: 79, server: 80).
        def norm(t):
            t = re.sub(r"^ +\d+:\d+\.+ now .*$", "NOW", t, flags=re.M)
            t = re.sub(r" +(:[^ ]+:)$", r" \1", t, flags=re.M)
            t = re.sub(r"\n+$", "\n", t)
            # Emacs's help line of the TODO list.
            t = re.sub(r"^Press .*\n( {20,}.*\n)?", "", t, flags=re.M)
            return t
        d = "".join(difflib.unified_diff(norm(eag).splitlines(True), norm(sag).splitlines(True), "emacs/agenda", "server/agenda", n=1))
        if d:
            print(d)
        else:
            print("  AGENDA SAME")
    c.save_all()
    c.shutdown()
    se, ss = compare.snapshot(we), compare.snapshot(wsv)
    same = True
    for f in sorted(set(se) | set(ss)):
        if se.get(f) != ss.get(f) and not f.endswith("~"):
            a = se.get(f, "").replace(we, "WS")
            b = ss.get(f, "").replace(wsv, "WS")
            if a != b:
                same = False
                print("".join(difflib.unified_diff(a.splitlines(True), b.splitlines(True), "emacs/" + f, "server/" + f, n=1)))
    print("  FILES SAME" if same else "  FILES DIFFER")


if __name__ == "__main__":
    profile = sys.argv[1]
    want = sys.argv[2:]
    for s in SC:
        if want and not any(w in s["name"] for w in want):
            continue
        try:
            run(profile, s)
        except Exception:
            import traceback

            traceback.print_exc()
