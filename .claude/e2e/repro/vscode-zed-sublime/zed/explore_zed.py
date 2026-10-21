#!/usr/bin/env python3
"""Exploratory Zed checks of editors/zed/TESTING.org, with the scenario
runner's machinery (editors/zed/test/run.py): each scenario here runs in a
Zed of its own; what each step's `expect` names is recorded (not compared),
and a failing step is recorded and the next steps still run.

    ZED=/root/zed/zed.app/bin/zed ZED_ORG_GRAMMAR=/root/zed/org.wasm \
      ORG_LISP_DIR=/root/org-mode/lisp python3 explore_zed.py OUT.json [NAME ...]

A command with no key in keymap.json runs from the code actions.
"""
import importlib.util
import json
import os
import sys
import time

spec = importlib.util.spec_from_file_location("zedrun", "/home/user/org-lsp/editors/zed/test/run.py")
zr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(zr)


class AllExcepted(dict):
    """Every command may run from the code actions; a count is pressed once."""

    def __contains__(self, key):
        return not key.endswith(" count")


zr.EXCEPTED = AllExcepted()
zr.DIFFERENCES = {}

B = ["buffer", "messages"]

SCENARIOS = {
    "dates-rename": {
        "files": {"d.org": "* TODO Task\n* Other\nwhen \n"},
        "steps": [
            {"open": "d.org"},
            {"at": "Task", "command": "org.schedule", "answers": ["+2d"], "expect": B},
            {"at": "Task", "command": "org.deadline", "answers": ["fri 10:00"], "expect": B},
            {"at": "when", "column": 5, "command": "org.timestamp.insert", "answers": ["2026-05-20"], "expect": B},
            {"command": "org.timestamp.insert", "answers": ["2026-05-22"], "expect": B},
            {"at": "Other", "command": "org.timestamp.insert.inactive", "answers": ["2026-05-21"], "expect": B},
        ],
    },
    "notes-tags-props": {
        "files": {"n.org": "* TODO Entry\n:PROPERTIES:\n:LOG_INTO_DRAWER: t\n:END:\n* B\n"},
        "steps": [
            {"open": "n.org"},
            {"at": "Entry", "command": "org.add.note", "answers": ["A one-line note"], "expect": B},
            {"at": "Entry", "command": "org.tags.set", "answers": ["zz:aa"], "expect": B},
            {"at": "Entry", "command": "org.effort.set", "answers": ["0:30"], "expect": B},
            {"at": "Entry", "command": "org.ordered.toggle", "expect": B},
            {"at": "B", "command": "org.priority", "answers": ["a"], "expect": B},
            {"at": "B", "command": "org.priority", "answers": [" "], "expect": B},
        ],
    },
    "babel-emacs": {
        "emacs": True,
        "files": {"b.org": "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\nsrc_emacs-lisp{(* 6 7)}\n* Toggle me\n"},
        "steps": [
            {"open": "b.org"},
            {"at": "(+ 1 2)", "command": "org.babel.execute", "answers": [{"choice": "Yes"}], "expect": B},
            {"at": "src_emacs", "column": 3, "command": "org.babel.execute", "answers": [{"choice": "Yes"}], "expect": B},
            {"at": "Toggle me", "command": "org.toggle.comment", "expect": B},
            {"at": "Toggle me", "command": "org.table.create", "answers": ["3x2"], "expect": B},
        ],
    },
    "links-footnotes": {
        "files": {
            "l.org": "* Target\n<<tgt>> here\n* B\n[[*Target]] [[tgt]] text[fn:1] [[No such heading]]\n\n[fn:1] Def.\n",
        },
        "steps": [
            {"open": "l.org"},
            {"at": "[[*Target]]", "column": 3, "command": "org.open", "expect": ["cursor"]},
            {"at": "[[tgt]]", "column": 3, "command": "org.open", "expect": ["cursor"]},
            {"at": "text[fn:1]", "column": 6, "command": "org.open", "expect": ["cursor"]},
            {"at": "[[No such", "column": 4, "command": "org.open", "answers": ["y"], "expect": ["buffer", "cursor", "messages"]},
            {"at": "Target", "command": "org.link.store", "expect": ["messages"]},
            {"at": "Def.", "column": 4, "command": "org.link.insert", "answers": ["", "Desc"], "expect": B},
            {"at": "* B", "command": "org.link.next", "expect": ["cursor"]},
        ],
    },
    "roam-journal-timer": {
        "files": {
            "roam/a.org": ":PROPERTIES:\n:ID:       a-id\n:END:\n#+title: A note\n\nText.\n",
            "roam/b.org": ":PROPERTIES:\n:ID:       b-id\n:END:\n#+title: B note\n\nSee [[id:a-id][A]].\n",
            "journal/20260517": "* Sunday, 05/17/26\n** 09:00 standup\n** TODO carried\na note\n",
            "t.org": "* Timer\n\n",
        },
        "options": {
            "org_roam_enabled": True, "org_roam_directory": "$DIR/roam",
            "org_journal_enabled": True, "org_journal_dir": "$DIR/journal",
        },
        "steps": [
            {"open": "roam/a.org"},
            {"command": "org.roam.node.find", "answers": ["B note"], "expect": ["buffer"]},
            {"at": "See", "command": "org.roam.node.insert", "answers": ["A note"], "expect": ["buffer"]},
            {"command": "org.roam.buffer.toggle", "expect": ["messages"]},
            {"command": "org.journal.new.entry", "expect": ["buffer", "file:journal/20260518"]},
            {"command": "org.journal.previous.entry", "expect": ["buffer", "messages"]},
            {"open": "t.org"},
            {"line": 2, "command": "org.timer.start", "expect": ["messages"]},
            {"command": "org.timer.item", "expect": B},
            {"command": "org.timer.stop", "expect": ["messages"]},
            {"command": "org.timer.set", "answers": ["1"], "expect": ["messages"]},
            {"command": "org.timer.stop", "expect": ["messages"]},
        ],
    },
    "structure-misc": {
        "files": {"s.org": "* B\n* A\n* C\n** inner\n#+TODO: TODO | DONE\n"},
        "steps": [
            {"open": "s.org"},
            {"at": "* B", "command": "org.insert.heading.after", "expect": B},
            {"type": "New"},
            {"at": "inner", "command": "org.inlinetask.insert", "expect": B},
            {"at": "* A", "command": "org.heading.up", "expect": ["messages"]},
            {"at": "* C", "command": "org.goto", "answers": [{"choice": "A"}], "expect": ["cursor"]},
            {"at": "* C", "command": "org.todo.nextset", "expect": ["messages"]},
            {"at": "* C", "command": "org.cut.subtree", "expect": B},
            {"at": "* A", "command": "org.paste.subtree", "expect": B},
        ],
    },
}


class Explore(zr.Scenario):
    def __init__(self, name, spec, keys):
        self.name = name
        self.keys = keys
        self.spec = spec
        self.expected = {}
        self.zed = None
        self.directory = None
        self.seen = 0
        self.record = []

    def check(self, index, kind):
        try:
            got = self.snapshot(kind).replace(self.directory, "$DIR")
        except zr.Failure as error:
            got = f"(failed: {error})"
        self.record.append({"step": index, "kind": kind, "got": got})

    def step(self, index, step):
        try:
            super().step(index, step)
        except zr.Failure as error:
            shot = os.path.join("/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/vscode-zed-sublime/zedshots", f"{self.name}-{index}.png")
            os.makedirs(os.path.dirname(shot), exist_ok=True)
            try:
                self.zed.screenshot(shot)
            except zr.Failure:
                shot = None
            self.record.append({"step": index, "failure": str(error), "shot": shot, "messages": list(self.zed.log.messages)})
            # Out of whatever is open: Escape twice.
            for _ in range(2):
                self.zed.press("escape")
            time.sleep(0.5)


def main():
    out = sys.argv[1]
    wanted = sys.argv[2:]
    zr.atexit.register(zr.stop_all)
    subprocess = zr.subprocess
    subprocess.run(["cargo", "build", "--release", "--target", "wasm32-wasip2", "--quiet"], cwd=zr.EXTENSION, check=True)
    keys = zr.Keys()
    results = {}
    for name, spec_ in SCENARIOS.items():
        if wanted and name not in wanted:
            continue
        scenario = Explore(name, spec_, keys)
        began = time.monotonic()
        try:
            scenario.run()
            status = "ran"
        except Exception as error:  # noqa: BLE001
            status = f"{type(error).__name__}: {error}"
        results[name] = {"status": status, "seconds": round(time.monotonic() - began), "record": scenario.record,
                         "all_messages": list(scenario.zed.log.messages) if scenario.zed else []}
        print(name, status, flush=True)
        with open(out, "w") as file:
            json.dump(results, file, indent=1)


if __name__ == "__main__":
    main()
