#!/usr/bin/env python3
"""Run the same editing command in org-lsp (over LSP, as a full client)
and in Emacs (batch, Org 9.8.10) on copies of the sample workspace, and
diff the files afterwards.

    python3 compare.py [scenario-name-substring...]

A scenario: name, file, needle (+delta: where the cursor is), the
server's command and answers, the Emacs form and its answers (what the
minibuffer and the key prompts read, in order).
"""
import difflib
import json
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import FULL, PLAIN, SCRATCH, Client, ResponseError, Timeout  # noqa
import workspace  # noqa

ORG = "/root/org-mode/lisp"

EMACS_PRELUDE = r"""
(add-to-list 'load-path "%(org)s")
(require 'org) (require 'org-agenda) (require 'org-habit) (require 'org-inlinetask)
(require 'org-id) (require 'ol-bibtex) (require 'org-archive) (require 'org-clock) (require 'org-colview)
(require 'cl-lib)
(setq org-id-locations-file "%(scratch)s/emacs-id-locations")
(setq org-agenda-files (list "%(ws)s/gtd.org" "%(ws)s/notes.org"))
(setq org-directory "%(ws)s" org-default-notes-file "%(ws)s/notes.org")
(setq org-todo-keywords (quote ((sequence "TODO" "NEXT" "WAIT(w@)" "|" "DONE(d!)" "CANCELLED(c@)"))))
(setq org-log-done 'time org-log-into-drawer t)
(setq org-cite-global-bibliography (list "%(ws)s/refs.bib"))
(setq org-refile-targets '((org-agenda-files :maxlevel . 2)))
(setq org-read-date-popup-calendar nil)
(setq org-modules '(org-habit))
(defvar e2e-answers nil)
(defun e2e-pop (&rest _) (let ((a (pop e2e-answers))) (if a a (error "e2e: no answer left"))))
(defun e2e-char (&rest _) (let ((a (e2e-pop))) (if (stringp a) (string-to-char a) a)))
%(extra)s
"""


def emacs_run(ws, file, line, col, form, answers, extra="", timeout=60):
    answers_lisp = "(list " + " ".join(json.dumps(a) for a in answers) + ")"
    code = (EMACS_PRELUDE % {"org": ORG, "ws": ws, "scratch": SCRATCH, "extra": extra}) + f"""
(setq e2e-answers {answers_lisp})
(find-file "{os.path.join(ws, file)}")
(org-fold-show-all)
(setq last-command 'e2e-previous this-command 'e2e-command)
(goto-char (point-min)) (forward-line {line}) (forward-char {col})
(cl-letf (((symbol-function 'read-string) #'e2e-pop)
          ((symbol-function 'read-from-minibuffer) #'e2e-pop)
          ((symbol-function 'completing-read) #'e2e-pop)
          ((symbol-function 'org-completing-read) #'e2e-pop)
          ((symbol-function 'org-icompleting-read) #'e2e-pop)
          ((symbol-function 'read-char-exclusive) #'e2e-char)
          ((symbol-function 'read-char) #'e2e-char)
          ((symbol-function 'read-key) #'e2e-char)
          ((symbol-function 'org--mks-read-key) (lambda (&rest _) (e2e-pop)))
          ((symbol-function 'y-or-n-p) (lambda (&rest _) t))
          ((symbol-function 'yes-or-no-p) (lambda (&rest _) t)))
  (condition-case err
      (progn {form} (run-hooks 'post-command-hook))
    (error (princ (format "E2E-ERROR: %S\\n" err)))))
(princ (format "E2E-POINT: %d %d\\n" (1- (line-number-at-pos)) (current-column)))
(princ (format "E2E-MESSAGE: %s\\n" (current-message)))
(save-some-buffers t)
"""
    r = subprocess.run(["emacs", "--batch", "-Q", "--eval", "(progn " + code + ")"], capture_output=True, text=True,
                       timeout=timeout, stdin=subprocess.DEVNULL)
    return r.stdout, r.stderr


def snapshot(ws):
    out = {}
    for root, _, files in os.walk(ws):
        if ".git" in root:
            continue
        for f in files:
            p = os.path.join(root, f)
            try:
                out[os.path.relpath(p, ws)] = open(p, encoding="utf-8").read()
            except Exception:
                pass
    return out


def server_run(ws, options, file, line, col_units, command, answers, extra_arg=None, profile=FULL, pre=None):
    # A full client, but without the date prompt document: dates are
    # answered as typed text (the calendar is tested on its own).
    extra = {"experimental": {"orgDatePrompt": False}} if profile == FULL else None
    c = Client(profile=profile, root=ws, options=options, name="cmp", timeout=30, caps_extra=extra)
    path = os.path.join(ws, file)
    uri = c.open(path)
    c.settle(0.3, 5)
    if pre:
        pre(c, uri)
    arg = c.at(path, line, col_units, **(extra_arg or {}))
    res = None
    err = None
    try:
        answers = list(answers)
        res, steps = c.run(command, arg, answers=answers, timeout=40)
        c.apply_result(arg, res)
        # A note buffer (CAPTURE-Note.org) the full client was shown:
        # type the note below its comment lines and finish it.
        notes = [u for u in c.docs if "CAPTURE-Note" in u]
        if notes:
            used = len(steps) - 1
            text = answers[used] if used < len(answers) else ""
            u = notes[-1]
            n = len(c.docs[u]["text"].split("\n"))
            c.change(u, text=c.docs[u]["text"].rstrip("\n") + "\n" + text + "\n")
            res = c.command("org.capture.finish", c.at(u, n - 1, 0), timeout=40)
    except (ResponseError, Timeout) as e:
        err = str(e)
    c.settle(2.5, 30)
    c.run_fetches()
    c.save_all()
    msgs = [m["message"] for m in c.messages]
    fails = c.edit_failures
    c.shutdown()
    return res, err, msgs, fails, c.stderr()


def diff(a, b, name):
    return "".join(difflib.unified_diff(a.splitlines(True), b.splitlines(True), "emacs/" + name, "server/" + name, n=1))


def compare(sc):
    name = sc["name"]
    base = os.path.join(SCRATCH, "cmp")
    we, opts = workspace.make(os.path.join(base, "emacs"))
    wsrv, opts = workspace.make(os.path.join(base, "server"))
    if sc.get("setup"):
        for w in (we, wsrv):
            sc["setup"](w)
    opts.update(sc.get("options", {}))
    text = open(os.path.join(we, sc["file"]), encoding="utf-8").read()
    idx = text.index(sc["needle"]) + sc.get("delta", 0)
    line = text.count("\n", 0, idx)
    col = idx - (text.rfind("\n", 0, idx) + 1)
    line_text = text.split("\n")[line]
    col16 = len(line_text[:col].encode("utf-16-le")) // 2
    before_e = snapshot(we)
    out, errout = emacs_run(we, sc["file"], line, col, sc["emacs"], sc.get("emacs_answers", []), sc.get("emacs_extra", ""))
    res, err, msgs, fails, stderr = server_run(wsrv, opts, sc["file"], line, col16, sc["command"], sc.get("answers", []),
                                               sc.get("arg"), sc.get("profile", FULL), sc.get("pre"))
    after_e = snapshot(we)
    after_s = snapshot(wsrv)
    print(f"===== {name}: {sc['command']} at {sc['file']}:{line}:{col} ({line_text.strip()[:50]!r})")
    eerr = [l for l in out.splitlines() if l.startswith("E2E-")]
    print("  emacs:", eerr, "| stderr tail:", errout.strip().splitlines()[-2:] if errout.strip() else "")
    print("  server result:", json.dumps(res)[:300] if res is not None else None, "error:", err, "messages:", msgs)
    if fails:
        print("  EDIT FAILURES:", fails)
    if stderr.strip():
        print("  SERVER STDERR:", stderr[-800:])
    same = True
    for f in sorted(set(after_e) | set(after_s)):
        a, b = after_e.get(f, ""), after_s.get(f, "")
        if f.endswith("~") or "CAPTURE" in f:
            continue
        if a != b:
            same = False
            print(diff(a, b, f))
    if same:
        changed = [f for f in after_e if after_e[f] != before_e.get(f)]
        print("  SAME", "(changed:", changed, ")")
    return same


SCENARIOS = []


def sc(**kw):
    SCENARIOS.append(kw)


# ---- TODO
sc(name="todo-next", file="gtd.org", needle="Write the report", command="org.todo.next", emacs="(org-todo 'right)")
sc(name="todo-prev", file="gtd.org", needle="Write the report", command="org.todo.previous", emacs="(org-todo 'left)")
sc(name="todo-done-log", file="gtd.org", needle="Write the report", command="org.todo", answers=["d"],
   emacs="(org-todo \"DONE\")")
sc(name="todo-wait-note", file="gtd.org", needle="Weekly review", command="org.todo", answers=["w", "a reason"],
   emacs="(org-todo \"WAIT\") (org-add-log-note) (insert \"a reason\") (org-store-log-note)")
sc(name="todo-repeat-habit", file="gtd.org", needle="Water the plants", command="org.todo", answers=["d"],
   emacs="(org-todo \"DONE\")")
sc(name="todo-repeat-weekly", file="gtd.org", needle="Weekly review", command="org.todo", answers=["d"],
   emacs="(org-todo \"DONE\")")
sc(name="todo-child-cookie", file="gtd.org", needle="Build it", command="org.todo", answers=["d"],
   emacs="(org-todo \"DONE\")")
sc(name="todo-body-cursor", file="gtd.org", needle="Draft the report", command="org.todo.next", emacs="(org-todo 'right)")
sc(name="todo-inline", file="gtd.org", needle="TODO An inline task", delta=5, command="org.todo.next",
   emacs="(org-todo 'right)")
sc(name="todo-cjk", file="gtd.org", needle="Ideas about", command="org.todo.next", emacs="(org-todo 'right)")
# ---- priority
sc(name="prio-up", file="gtd.org", needle="Call the plumber", command="org.priority.up", emacs="(org-priority-up)")
sc(name="prio-down", file="gtd.org", needle="Write the report", command="org.priority.down", emacs="(org-priority-down)")
sc(name="prio-set-c", file="gtd.org", needle="Call the plumber", command="org.priority", answers=["C"],
   emacs="(org-priority ?C)")
sc(name="prio-remove", file="gtd.org", needle="Write the report", command="org.priority", answers=[" "],
   emacs="(org-priority 'remove)")
# ---- scheduling
sc(name="schedule-+2d", file="gtd.org", needle="Call the plumber", command="org.schedule", answers=["+2d"],
   emacs="(org-schedule nil)", emacs_answers=["+2d"])
sc(name="schedule-fri-time", file="gtd.org", needle="Call the plumber", command="org.schedule", answers=["fri 10:00"],
   emacs="(org-schedule nil)", emacs_answers=["fri 10:00"])
sc(name="schedule-replace", file="gtd.org", needle="Write the report", command="org.schedule", answers=["2026-11-02"],
   emacs="(org-schedule nil)", emacs_answers=["2026-11-02"])
sc(name="schedule-repeater", file="gtd.org", needle="Weekly review", command="org.schedule", answers=["+3d"],
   emacs="(org-schedule nil)", emacs_answers=["+3d"])
sc(name="schedule-remove", file="gtd.org", needle="Write the report", command="org.schedule.remove",
   emacs="(org-schedule '(4))")
sc(name="deadline-set", file="gtd.org", needle="Reply from Alice", command="org.deadline", answers=["nov 5"],
   emacs="(org-deadline nil)", emacs_answers=["nov 5"])
sc(name="deadline-remove", file="gtd.org", needle="Write the report", command="org.deadline.remove",
   emacs="(org-deadline '(4))")
sc(name="schedule-done-closed", file="gtd.org", needle="Pay the rent", command="org.schedule", answers=["+1d"],
   emacs="(org-schedule nil)", emacs_answers=["+1d"])
sc(name="schedule-range", file="gtd.org", needle="Call the plumber", command="org.schedule", answers=["10:00-11:30"],
   emacs="(org-schedule nil)", emacs_answers=["10:00-11:30"])
sc(name="schedule-w44", file="gtd.org", needle="Call the plumber", command="org.schedule", answers=["w44"],
   emacs="(org-schedule nil)", emacs_answers=["w44"])
sc(name="schedule-cjk", file="gtd.org", needle="Ideas about", command="org.schedule", answers=["+1w"],
   emacs="(org-schedule nil)", emacs_answers=["+1w"])
# ---- timestamps
sc(name="ts-up-day", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=10, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-up-year", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=2, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-down-month", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=7, command="org.timestamp.down",
   emacs="(org-timestamp-down)")
sc(name="ts-up-hour", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=17, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-up-minute", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=20, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-up-weekday", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=13, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-up-bracket", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=0, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-repeater", file="gtd.org", needle="+1w>", delta=2, command="org.timestamp.up", emacs="(org-timestamp-up)")
sc(name="ts-clock-line", file="gtd.org", needle="10:30] =>", delta=-2, command="org.timestamp.up",
   emacs="(org-timestamp-up)")
sc(name="ts-range-day", file="gtd.org", needle="<2026-10-26 Mon>", delta=10, command="org.date.later",
   emacs="(org-timestamp-up-day)")
sc(name="ts-insert", file="gtd.org", needle="A paragraph.", delta=12, command="org.timestamp.insert", answers=["+1d"],
   emacs="(org-timestamp nil)", emacs_answers=["+1d"])
sc(name="ts-insert-inactive", file="gtd.org", needle="A paragraph.", delta=12, command="org.timestamp.insert.inactive",
   answers=["2026-12-24 18:00"], emacs="(org-timestamp-inactive nil)", emacs_answers=["2026-12-24 18:00"])
sc(name="ts-change", file="gtd.org", needle="<2026-10-22 Thu 14:00", delta=3, command="org.timestamp.insert",
   answers=["+1d"], emacs="(org-timestamp nil)", emacs_answers=["+1d"])
# ---- shift
sc(name="shift-right-heading", file="gtd.org", needle="Call the plumber", command="org.shift.right",
   emacs="(org-shiftright)")
sc(name="shift-left-heading", file="gtd.org", needle="Call the plumber", command="org.shift.left", emacs="(org-shiftleft)")
sc(name="shift-right-bullet", file="gtd.org", needle="- nested a", delta=0, command="org.shift.right",
   emacs="(org-shiftright)")
sc(name="shift-up-heading", file="gtd.org", needle="Call the plumber", command="org.shift.up", emacs="(org-shiftup)")
sc(name="shift-up-prio-line", file="gtd.org", needle="[#A]", delta=2, command="org.shift.up", emacs="(org-shiftup)")
# ---- structure
sc(name="meta-left-sub", file="gtd.org", needle="Plan it", command="org.meta.left", emacs="(org-metaleft)")
sc(name="meta-right", file="gtd.org", needle="Call the plumber", command="org.meta.right", emacs="(org-metaright)")
sc(name="shiftmeta-right-tree", file="gtd.org", needle="Project Alpha", command="org.shiftmeta.right",
   emacs="(org-shiftmetaright)")
sc(name="shiftmeta-left-tree", file="gtd.org", needle="Project Alpha", command="org.shiftmeta.left",
   emacs="(org-shiftmetaleft)")
sc(name="meta-down-heading", file="gtd.org", needle="Write the report", command="org.meta.down", emacs="(org-metadown)")
sc(name="meta-up-heading", file="gtd.org", needle="Call the plumber", command="org.meta.up", emacs="(org-metaup)")
sc(name="meta-down-item", file="gtd.org", needle="1. first", command="org.meta.down", emacs="(org-metadown)")
sc(name="meta-right-item", file="gtd.org", needle="2. second", command="org.meta.right", emacs="(org-metaright)")
sc(name="meta-left-nested", file="gtd.org", needle="- nested b", command="org.meta.left", emacs="(org-metaleft)")
sc(name="meta-up-checkbox", file="gtd.org", needle="- [ ] bread", command="org.meta.up", emacs="(org-metaup)")
sc(name="meta-return-heading-end", file="gtd.org", needle="Call the plumber", delta=16, command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="meta-return-heading-mid", file="gtd.org", needle="Call the plumber", delta=5, command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="meta-return-bol", file="gtd.org", needle="** NEXT Call the plumber", command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="meta-return-item", file="gtd.org", needle="1. first", delta=8, command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="meta-return-checkbox", file="gtd.org", needle="- [ ] bread", delta=11, command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="meta-return-body", file="gtd.org", needle="Bob said hello.", file_override=None, command="org.meta.return",
   emacs="(org-meta-return)")
sc(name="todo-heading", file="gtd.org", needle="Call the plumber", delta=16, command="org.insert.todo.heading",
   emacs="(org-insert-todo-heading nil)")
sc(name="todo-heading-checkbox", file="gtd.org", needle="- [ ] bread", delta=11, command="org.insert.todo.heading",
   emacs="(org-insert-todo-heading nil)")
sc(name="heading-after", file="gtd.org", needle="Write the report", command="org.insert.heading.after",
   emacs="(org-insert-heading-respect-content)")
sc(name="todo-heading-after", file="gtd.org", needle="Project Alpha", command="org.insert.todo.heading.after",
   emacs="(org-insert-todo-heading-respect-content)")
sc(name="return-heading", file="gtd.org", needle="Call the plumber", delta=5, command="org.return", emacs="(org-return)")
sc(name="return-item", file="gtd.org", needle="1. first", delta=8, command="org.return.item",
   emacs="(org-return nil t)", emacs_extra="")
sc(name="return-table", file="tables.org", needle="| Apple", delta=3, command="org.return", emacs="(org-return)")
sc(name="toggle-heading", file="gtd.org", needle="A paragraph.", command="org.toggle.heading", emacs="(org-toggle-heading)")
sc(name="toggle-item", file="gtd.org", needle="A paragraph.", command="org.toggle.item", emacs="(org-toggle-item nil)")
sc(name="toggle-heading-off", file="gtd.org", needle="** Code", command="org.toggle.heading", emacs="(org-toggle-heading)")
sc(name="toggle-comment", file="gtd.org", needle="Weekly review", command="org.toggle.comment",
   emacs="(org-toggle-comment)")
# ---- checkboxes
sc(name="checkbox", file="gtd.org", needle="- [ ] bread", delta=3, command="org.checkbox.toggle",
   emacs="(org-toggle-checkbox)")
sc(name="checkbox-done", file="gtd.org", needle="- [X] milk", delta=3, command="org.checkbox.toggle",
   emacs="(org-toggle-checkbox)")
sc(name="ctrl-c-ctrl-c-cookie", file="gtd.org", needle="Groceries [33%]", delta=11, command="org.checkbox.toggle",
   emacs="(org-ctrl-c-ctrl-c)")
# ---- sort
for k in ("a", "A", "o", "p", "n"):
    sc(name=f"sort-{k}", file="tables.org", needle="* Headlines to sort", command="org.sort", answers=[k],
       emacs=f"(org-sort-entries nil ?{k})")
sc(name="sort-tasks-o", file="gtd.org", needle="* Tasks", command="org.sort", answers=["o"],
   emacs="(org-sort-entries nil ?o)")
sc(name="sort-tasks-p", file="gtd.org", needle="* Tasks", command="org.sort", answers=["p"],
   emacs="(org-sort-entries nil ?p)")
sc(name="sort-tasks-s", file="gtd.org", needle="* Tasks", command="org.sort", answers=["s"],
   emacs="(org-sort-entries nil ?s)")
sc(name="sort-tasks-d", file="gtd.org", needle="* Tasks", command="org.sort", answers=["d"],
   emacs="(org-sort-entries nil ?d)")
sc(name="sort-list-a", file="tables.org", needle="- zebra", command="org.sort", answers=["a"],
   emacs="(org-sort-list nil ?a)")
sc(name="sort-table-n", file="tables.org", needle="|   3 |", delta=3, command="org.sort", answers=["N"],
   emacs="(org-table-sort-lines nil ?N)")
# ---- tables
sc(name="table-align", file="tables.org", needle="|a|bb", delta=1, command="org.table.next.field",
   emacs="(org-table-next-field)")
sc(name="table-recalc", file="tables.org", needle="| Apple", delta=3, command="org.table.recalculate",
   emacs="(org-table-recalculate t)")
sc(name="table-insert-row", file="tables.org", needle="| Pear", delta=3, command="org.shiftmeta.down",
   emacs="(org-shiftmetadown)")
sc(name="table-kill-row", file="tables.org", needle="| Pear", delta=3, command="org.shiftmeta.up",
   emacs="(org-shiftmetaup)")
sc(name="table-delete-col", file="tables.org", needle="|   3 |", delta=3, command="org.shiftmeta.left",
   emacs="(org-shiftmetaleft)")
sc(name="table-insert-col", file="tables.org", needle="|   3 |", delta=3, command="org.shiftmeta.right",
   emacs="(org-shiftmetaright)")
sc(name="table-move-col", file="tables.org", needle="|   3 |", delta=3, command="org.meta.right",
   emacs="(org-metaright)")
sc(name="table-move-row", file="tables.org", needle="| Pear", delta=3, command="org.meta.down", emacs="(org-metadown)")
sc(name="table-hline", file="tables.org", needle="| Pear", delta=3, command="org.table.hline",
   emacs="(org-table-insert-hline)")
sc(name="table-blank", file="tables.org", needle="| Pear", delta=3, command="org.table.blank.field",
   emacs="(org-table-blank-field)")
sc(name="table-next-last", file="tables.org", needle="|🎉|", delta=1, command="org.table.next.field",
   emacs="(org-table-next-field)")
sc(name="table-create", file="tables.org", needle="* Sort me", delta=9, command="org.table.create", answers=["3x2"],
   emacs="(end-of-line) (newline) (org-table-create \"3x2\")")
sc(name="table-copy-down", file="tables.org", needle="| Pear", delta=3, command="org.table.copy.down",
   emacs="(org-table-copy-down 1)")
sc(name="table-sum", file="tables.org", needle="|   3 |", delta=3, command="org.table.sum", emacs="(org-table-sum)")
sc(name="table-meta-return", file="tables.org", needle="| Apple", delta=3, command="org.meta.return",
   emacs="(org-meta-return)")
# ---- archive / refile
sc(name="archive-subtree", file="gtd.org", needle="Pay the rent", command="org.archive.subtree",
   emacs="(org-archive-subtree)")
sc(name="archive-sibling", file="gtd.org", needle="Pay the rent", command="org.archive.sibling",
   emacs="(org-archive-to-archive-sibling)")
sc(name="archive-tag", file="gtd.org", needle="Pay the rent", command="org.archive.tag",
   emacs="(org-toggle-archive-tag)")
sc(name="refile", file="gtd.org", needle="Call the plumber", command="org.refile", answers=["Projects"],
   emacs="(org-refile)", emacs_answers=["Projects"],
   emacs_extra="(setq org-refile-use-outline-path nil)")
# ---- properties, tags, effort
sc(name="property-set", file="gtd.org", needle="Call the plumber", command="org.property.set", answers=["Owner", "me"],
   emacs="(org-set-property \"Owner\" \"me\")")
sc(name="property-delete", file="gtd.org", needle="Write the report", command="org.property.delete",
   answers=["Effort"], emacs="(org-delete-property \"Effort\")")
sc(name="tags-set", file="gtd.org", needle="Weekly review", command="org.tags.set", answers=[":home:urgent:"],
   emacs="(org-set-tags \":home:urgent:\")")
sc(name="tags-set-cjk", file="gtd.org", needle="Ideas about", command="org.tags.set", answers=[":urgent:home:"],
   emacs="(org-set-tags \":urgent:home:\")")
sc(name="effort-set", file="gtd.org", needle="Call the plumber", command="org.effort.set", answers=["0:30"],
   emacs="(org-set-effort nil \"0:30\")")
sc(name="id-create", file="gtd.org", needle="Weekly review", command="org.id.create", emacs="(org-id-get-create)")
sc(name="ordered", file="gtd.org", needle="Write the report", command="org.ordered.toggle",
   emacs="(org-toggle-ordered-property)")
# ---- clock
sc(name="clock-in", file="gtd.org", needle="Weekly review", command="org.clock.in", emacs="(org-clock-in)")
sc(name="add-note", file="gtd.org", needle="Weekly review", command="org.add.note", answers=["my note"],
   emacs="(org-add-note) (org-add-log-note) (insert \"my note\") (org-store-log-note)")
# ---- inline task, cut/paste
sc(name="inlinetask", file="gtd.org", needle="A paragraph.", command="org.inlinetask.insert",
   emacs="(org-inlinetask-insert-task)")
sc(name="inlinetask-eol", file="gtd.org", needle="A paragraph.", delta=12, command="org.inlinetask.insert",
   emacs="(org-inlinetask-insert-task)")
sc(name="cut-subtree", file="gtd.org", needle="Weekly review", command="org.cut.subtree", emacs="(org-cut-subtree)")
sc(name="kill-line-heading", file="gtd.org", needle="Call the plumber", delta=5, command="org.kill.line",
   emacs="(org-kill-line)", options={"org_special_ctrl_k": True}, emacs_extra="(setq org-special-ctrl-k t)")
sc(name="footnote-new", file="gtd.org", needle="A paragraph.", delta=11, command="org.footnote.action",
   emacs="(org-footnote-action)")
sc(name="clone", file="gtd.org", needle="Meeting with Bob", command="org.subtree.clone", answers=["2", "+1w"],
   emacs="(org-clone-subtree-with-time-shift 2 \"+1w\")")
sc(name="bibtex-create-bol", file="gtd.org", needle="** Meeting with Bob", command="org.bibtex.create",
   answers=["book", "TITLE", "AUTH", "PUB", "1999", "key1"],
   emacs="(org-bibtex-create)", emacs_answers=["book", "TITLE", "AUTH", "PUB", "1999", "key1"])
sc(name="bibtex-create-mid", file="gtd.org", needle="Meeting with Bob", command="org.bibtex.create",
   answers=["book", "TITLE", "AUTH", "PUB", "1999", "key1"],
   emacs="(org-bibtex-create)", emacs_answers=["book", "TITLE", "AUTH", "PUB", "1999", "key1"])
sc(name="promote-alpha-region", file="gtd.org", needle="Plan it", command="org.levels.odd",
   emacs="(org-convert-to-odd-levels)")


def main():
    want = sys.argv[1:]
    results = {}
    for s in SCENARIOS:
        if want and not any(w in s["name"] for w in want):
            continue
        try:
            results[s["name"]] = compare(s)
        except Exception as err:  # noqa
            import traceback

            traceback.print_exc()
            results[s["name"]] = None
    print("SUMMARY:", {k: v for k, v in results.items()})
    print("DIFFER:", [k for k, v in results.items() if not v])


if __name__ == "__main__":
    main()
