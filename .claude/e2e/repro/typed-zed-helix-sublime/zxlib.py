"""By-hand checks in Zed, typed with xdotool through editors/zed/test/run.py:
`run(name, files, options, steps)` takes steps as the scenarios write them
(`open`, `at`, `command` with `answers`, `type`, `focus`), and after each
prints what it asks for (`show`: buffer, cursor, messages, file:NAME,
agenda, screen), or a check of its own (`check`: a function of the
Scenario)."""
import importlib.util
import os
import sys
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
os.environ.setdefault("ZED", "/root/zed/zed.app/bin/zed")
os.environ.setdefault("ZED_ORG_GRAMMAR", "/root/zed/org.wasm")
os.environ.setdefault("ORG_LSP_BIN", "/home/user/org-lsp/target/debug/org-lsp")
os.environ.setdefault("ORG_LISP_DIR", "/root/org-mode/lisp")
os.environ.setdefault("ORG_LSP_ZED_DISPLAY", "86")
spec = importlib.util.spec_from_file_location("zedrun", os.path.join(ROOT, "editors", "zed", "test", "run.py"))
zr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(zr)
Failure = zr.Failure
SHOTS = os.path.join(HERE, "shots")


class Scenario(zr.Scenario):
    def __init__(self, name, keys, files, options):
        self.name = name
        self.keys = keys
        self.spec = {"files": files, "options": options, "steps": []}
        self.expected = {}
        self.zed = None
        self.directory = None
        self.seen = 0


def show(scenario, kind):
    if kind == "screen":
        os.makedirs(SHOTS, exist_ok=True)
        path = os.path.join(SHOTS, f"{scenario.name}-{len(os.listdir(SHOTS))}.png")
        scenario.zed.screenshot(path)
        return path
    text = scenario.snapshot(kind).replace(scenario.directory, "$DIR")
    if kind == "messages":
        scenario.seen = len(scenario.zed.log.messages)
    return text.rstrip("\n").replace("\n", " ⏎ ")


def run(name, files, options, steps):
    keys = zr.Keys()
    scenario = Scenario(name, keys, files, options)
    scenario.zed = zr.Zed(scenario, keys)
    scenario.directory = os.path.realpath(scenario.zed.work)
    for file, text in files.items():
        path = os.path.join(scenario.directory, file)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as out:
            out.write(text)
    scenario.zed.install(scenario.options())
    zr.deadline.at = None
    try:
        scenario.zed.start()
        for index, step in enumerate(steps, 1):
            if step.get("hold"):
                # Zed stays as it is for a look by hand until the file goes.
                hold = os.path.join(HERE, "hold")
                with open(hold, "w") as out:
                    out.write(f"{scenario.zed.display} {scenario.zed.window} {scenario.directory}\n")
                print(f"hold: display :{scenario.zed.display} window {scenario.zed.window}; rm {hold}", flush=True)
                while os.path.exists(hold):
                    time.sleep(1)
                continue
            title = step.pop("title", json_of(step))
            check = step.pop("check", None)
            kinds = step.pop("show", [])
            try:
                scenario.step(index, step)
                out = [show(scenario, kind) for kind in kinds]
                if check:
                    out.append(check(scenario))
                print(f"ok   {index} {title}: {out}", flush=True)
            except Exception as error:  # noqa: BLE001
                detail = str(error) if isinstance(error, Failure) else traceback.format_exc()
                try:
                    shot = show(scenario, "screen")
                except Exception:  # noqa: BLE001
                    shot = None
                print(f"FAIL {index} {title}: {detail[:1500]} (screen {shot})", flush=True)
                try:
                    scenario.zed.press("escape")
                except Exception:  # noqa: BLE001
                    pass
    finally:
        scenario.zed.stop()


def json_of(step):
    import json
    return json.dumps(step, default=str)[:120]
