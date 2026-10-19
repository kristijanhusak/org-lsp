#!/usr/bin/env python3
"""Pictures of the shared highlighting sample (tests/e2e/highlight) in
each editor, for a person to look at: the Org file, and where the editor
shows them the week's agenda, the TODO list and the date prompt.

    scripts/highlight-screenshots.py [nvim|helix|vscode|zed ...]

Writes target/highlight-screenshots/EDITOR-VIEW.png. Without names, each
editor that is there is taken. The server is built first.

- Neovim (`NVIM`, else `nvim`) and Helix (`HX`, else `hx`) run in a
  terminal of their own, read with pyte and drawn as scripts/screenshots.py
  draws Neovim (Node's Playwright and its Chromium).
- VS Code (`VSCODE_EXECUTABLE`, as for its tests) runs the extension's
  `screenshots` suite under `xvfb-run`; ImageMagick's `import` takes the
  screen.
- Zed (`ZED`, its `bin/zed`; `ZED_ORG_GRAMMAR`, an `org.wasm`) is set up as
  editors/zed/test/run.py sets it up, the extension built with
  `cargo build --release --target wasm32-wasip2` in editors/zed. Zed has
  no highlighting test of its own: here it fails when a background its
  rules give (a habit's days) shows nowhere in the agenda.

Helix has no semantic tokens: what it shows is its own tree-sitter
highlighting, the server's tokens left aside.
"""

import collections
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "target", "highlight-screenshots")
SAMPLE = os.path.join(ROOT, "tests", "e2e", "highlight")
SERVER = os.path.join(ROOT, "target", "debug", "org-lsp")
ESC = "\x1b"
CR = "\r"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


sys.path.insert(0, os.path.join(HERE, "screenshots"))
from term import Term, html  # noqa: E402

shots = load("screenshots", os.path.join(HERE, "screenshots.py"))


def fixture():
    """The sample copied to a directory of its own, and its options with
    `$DIR` that directory."""
    directory = os.path.realpath(tempfile.mkdtemp(prefix="org-lsp-highlight-"))
    shutil.copytree(SAMPLE, directory, dirs_exist_ok=True)
    with open(os.path.join(directory, "options.json"), encoding="utf-8") as file:
        options = json.loads(file.read().replace("$DIR", directory))
    options["config_file"] = False
    return directory, options


def pictures(name, frames):
    """Each (view, frame) as EDITOR-VIEW.png."""
    shots.write_pngs([(os.path.join(OUT, f"{name}-{view}.png"), html(frame, title)) for view, title, frame in frames])


def nvim():
    directory, options = fixture()
    env = {
        "SHOT_ROOT": ROOT,
        "SHOT_WORKSPACE": directory,
        "SHOT_SETTINGS": json.dumps(options),
        "SHOT_COLORS": "default",
        "ORG_LSP_CONFIG": "/nonexistent/org-lsp/config.toml",
        "XDG_STATE_HOME": os.path.join(directory, ".state"),
        "XDG_DATA_HOME": os.path.join(directory, ".data"),
    }
    argv = [os.environ.get("NVIM", "nvim"), "--clean", "-n", "-u", os.path.join(HERE, "screenshots", "init.lua"), "sample.org"]
    term = Term(argv, env, 110, 78, cwd=directory)
    frames = []
    try:
        term.until(lambda screen: "Highlighting sample" in screen, 20, "the sample")
        term.drain(3)
        frames.append(("sample", "sample.org", term.frame()))

        def run(command, shows):
            term.send(ESC + f":lua require('org-lsp').run('{command}')" + CR, 0.5)
            term.until(lambda screen: shows in screen, 20, shows)
            term.send(ESC + ":only" + CR, 0.3)
            term.drain(2)

        run("org.agenda.list", "Week-agenda")
        frames.append(("agenda", "Org Agenda", term.frame()))
        run("org.agenda.todos", "Global list")
        frames.append(("todo", "Org Agenda: TODO list", term.frame()))
        term.send(ESC + ":buffer sample.org" + CR + ":6" + CR, 0.5)
        term.send(ESC + ":lua require('org-lsp').run('org.schedule')" + CR, 0.5)
        term.until(lambda screen: "SCHEDULED Date+time" in screen, 20, "the date prompt")
        term.drain(2)
        frames.append(("calendar", "Date prompt", term.frame()))
    finally:
        term.close()
    pictures("nvim", frames)


def helix():
    directory, options = fixture()
    hx = os.environ.get("HX", "hx")
    work = tempfile.mkdtemp(prefix="org-lsp-hx-shot-")
    config = os.path.join(work, "config", "helix")
    os.makedirs(config)
    with open(os.path.join(ROOT, "editors", "helix", "languages.toml"), encoding="utf-8") as file:
        languages = file.read()
    toml = load("helix_run", os.path.join(ROOT, "editors", "helix", "test", "run.py")).toml_value
    languages = languages.replace('command = "org-lsp"', f"command = {json.dumps(SERVER)}\nconfig = {toml(options)}", 1)
    with open(os.path.join(config, "languages.toml"), "w", encoding="utf-8") as file:
        file.write(languages)
    env = {
        "HOME": os.path.join(work, "home"),
        "XDG_CONFIG_HOME": os.path.join(work, "config"),
        "XDG_CACHE_HOME": os.path.join(work, "cache"),
        "ORG_LSP_CONFIG": "/nonexistent/config.toml",
    }
    runtime = os.path.join(os.path.dirname(os.path.realpath(shutil.which(hx) or hx)), "runtime")
    if os.path.isdir(runtime):
        env["HELIX_RUNTIME"] = runtime
    term = Term([hx, "sample.org"], env, 110, 78, cwd=directory)
    try:
        term.until(lambda screen: "Highlighting sample" in screen, 20, "the sample")
        term.drain(4)
        frames = [("sample", "sample.org", term.frame())]
    finally:
        term.close()
        shutil.rmtree(work, ignore_errors=True)
    pictures("helix", frames)


def vscode():
    env = dict(os.environ, ORG_LSP_TEST_SHOTS=OUT)
    command = ["xvfb-run", "-a", "-s", "-screen 0 1600x1000x24", "npm", "test", "--", "screenshots"]
    subprocess.run(command, cwd=os.path.join(ROOT, "editors", "vscode"), env=env, check=True)


def zed_backgrounds(screen, wanted):
    """Which tokens of `wanted` ({token: color of its rule, `#rrggbbaa`})
    show nowhere on `screen` (rows of r, g, b bytes): their color laid over
    the editor's background, the color most of the screen has."""
    counts = collections.Counter(screen[at : at + 3] for at in range(0, len(screen) - 2, 3))
    background = counts.most_common(1)[0][0]
    missing = []
    for token, color in wanted.items():
        rgb = [int(color[i : i + 2], 16) for i in (1, 3, 5)]
        alpha = int(color[7:9] or "ff", 16) / 255
        shown = [round(alpha * c + (1 - alpha) * b) for c, b in zip(rgb, background)]
        if not any(all(abs(p - s) <= 2 for p, s in zip(pixel, shown)) for pixel in counts):
            missing.append(f"{token} ({color}, as {bytes(shown).hex()})")
    return missing


def zed():
    os.environ.setdefault("ZED_ORG_GRAMMAR", "/root/zed/org.wasm")
    harness = load("zed_run", os.path.join(ROOT, "editors", "zed", "test", "run.py"))
    keys = harness.Keys()
    editor = harness.Zed(None, keys)
    directory = os.path.realpath(editor.work)
    shutil.copytree(SAMPLE, directory, dirs_exist_ok=True)
    with open(os.path.join(directory, "options.json"), encoding="utf-8") as file:
        options = json.loads(file.read().replace("$DIR", directory))
    options["config_file"] = False
    editor.install(options)
    editor.start()
    try:
        sample = os.path.join(directory, "sample.org")
        editor.open(sample)
        editor.settle(1.0, 30)
        time.sleep(3)
        editor.screenshot(os.path.join(OUT, "zed-sample.png"))
        editor.press("ctrl-end")
        time.sleep(2)
        editor.screenshot(os.path.join(OUT, "zed-sample-end.png"))
        # The tasks of these commands have no key: their command, as the
        # task runs it.
        script = os.path.join(editor.home, ".local", "state", "org-lsp", "agenda-command")
        with open(os.path.join(ROOT, "editors", "zed", "languages", "org", "semantic_token_rules.json")) as file:
            rules = json.load(file)
        # The backgrounds the rules give: the days of a habit's graph.
        backgrounds = {
            ".".join([rule["token_type"], *rule.get("token_modifiers", [])]): rule["background_color"]
            for rule in rules
            if "background_color" in rule and rule.get("token_type") == "agendaHabit"
        }
        missing = []
        for command, view in (("org.agenda.list", "agenda"), ("org.agenda.todos", "todo")):
            subprocess.run([script, command], env=editor.environment(), cwd=editor.work, timeout=60, check=True)
            editor.settle(1.0, 30)
            time.sleep(3)
            editor.screenshot(os.path.join(OUT, f"zed-{view}.png"))
            if view == "agenda":
                missing = zed_backgrounds(editor.pixels(), backgrounds)
            editor.open(sample)
        if missing:
            raise RuntimeError("not shown in the agenda: " + ", ".join(missing))
    finally:
        editor.stop()


EDITORS = {"nvim": nvim, "helix": helix, "vscode": vscode, "zed": zed}


def main():
    names = sys.argv[1:] or list(EDITORS)
    subprocess.run(["cargo", "build", "-q", "-p", "org-lsp"], cwd=os.path.join(ROOT, "server"), check=True)
    os.makedirs(OUT, exist_ok=True)
    failed = []
    for name in names:
        print(f"== {name}", flush=True)
        try:
            EDITORS[name]()
        except Exception as error:  # noqa: BLE001 - the others still get taken
            print(f"{name}: {error}", file=sys.stderr)
            failed.append(name)
    print(f"pictures in {OUT}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
