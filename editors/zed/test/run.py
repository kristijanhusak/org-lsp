#!/usr/bin/env python3
"""The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in Zed.

Each scenario runs in a Zed of its own, on an X display of its own
(Xvfb), with a home directory of its own: the extension installed as
scripts/install.sh installs it, and the settings, the keymap and the tasks
docs/zed.org and docs/keys/zed.org tell users to write. The steps are
taken as a Zed user takes them, every key typed in Zed's window with
`xdotool`: the key keymap.json binds to the step's command where the
cursor is (a task, which runs the command through the server's command
port; Zed's own `workspace::SaveAll` for the agenda's save), the code
actions menu (`ctrl-.`, down to the action, Enter) for a command Zed has
no key for, the buttons of a question (clicked with the mouse: Zed's
notifications take no keys), a rename at the cursor (`F2`, the answer
typed) for typed text, `ctrl-g` to go to a line, the file finder
(`ctrl-p`, the file's name typed) to open a file or go back to the
agenda. What a scenario expects is what the server's own runner saw
(server/org-lsp/tests/e2e_scenarios.rs, tests/e2e/expected/NAME.txt).

What Zed has open and where its cursor is comes from what it tells the
server: proxy.py stands between them and writes down every message. The
current buffer is the window's title (`window_title_format` is the file's
path).

    ZED=/path/to/zed editors/zed/test/run.py [NAME ...]

`ZED` is Zed's command line (`bin/zed` of the Linux build). The server is
`ORG_LSP_BIN` (default: the debug build); the extension is built with
`cargo build --release --target wasm32-wasip2` in editors/zed; the Org
grammar, which Zed would build from the source named in extension.toml, is
`ZED_ORG_GRAMMAR` (an `org.wasm`), or built from `ZED_TREE_SITTER_ORG` (a
checkout of tree-sitter-org) with `WASI_SDK`'s clang. `ORG_LSP_E2E=NAME`
runs the scenarios whose name has NAME in it, `ORG_LSP_E2E_JOBS` of them
at a time (1: Zeds side by side slow each other down a lot); `ORG_LSP_ZED_KEEP=1` keeps each one's directory in /tmp
(`ozed-*`: the home, the files, Zed's output, the log of what was said). What Zed cannot do is in tests/e2e/exceptions.json:
`editors.zed` the commands it has no key for, run from the code actions,
and as `COMMAND count` those whose key is pressed once for a count;
`snapshots.zed` what it shows otherwise, as `NAME STEP KIND`, and the
steps it cannot take, as `NAME STEP`; and
`scenarios.zed` the scenarios it cannot take at all. A failure leaves a
screenshot in target/zed-e2e. A scenario fails when it takes more than
`ORG_LSP_ZED_BUDGET` seconds (600); run.py stopped by a signal closes its
Zeds and their displays. Each Zed takes the first free X display from
`ORG_LSP_ZED_DISPLAY` (90) up.
"""

import atexit
import collections
import concurrent.futures
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
E2E = os.path.join(ROOT, "tests", "e2e")
EXTENSION = os.path.join(ROOT, "editors", "zed")
SERVER = os.environ.get("ORG_LSP_BIN") or os.path.join(ROOT, "target", "debug", "org-lsp")
SHOTS = os.path.join(ROOT, "target", "zed-e2e")
RENAME_PROMPT = ": answer with a rename at the cursor"
# The screen Zed's window fills, which places a question's buttons.
WIDTH, HEIGHT = 1280, 800
# A key of the harness's own, for Zed's `workspace::ClearAllNotifications`:
# messages pile up at the bottom right, over a question's buttons.
CLEAR_NOTIFICATIONS = "ctrl-alt-shift-f12"

exceptions = json.load(open(os.path.join(E2E, "exceptions.json")))
EXCEPTED = exceptions.get("editors", {}).get("zed", {})
DIFFERENCES = exceptions.get("snapshots", {}).get("zed", {})
SKIPPED = exceptions.get("scenarios", {}).get("zed", {})


class Failure(Exception):
    pass


# How long a scenario may take in all (`ORG_LSP_ZED_BUDGET`, in seconds):
# each wait has a bound of its own, and this one ends a scenario whose
# waits all run long, with the step and what it waited for.
BUDGET = float(os.environ.get("ORG_LSP_ZED_BUDGET", "600"))
# The end of the budget of the scenario this thread runs.
deadline = threading.local()


def over_budget(what):
    end = getattr(deadline, "at", None)
    if end is not None and time.monotonic() > end:
        raise Failure(f"the scenario took more than its {BUDGET:.0f}s, waiting for {what}")


def until(what, probe, timeout=10.0, interval=0.05):
    """Wait until `probe` returns something truthy, and return it."""
    start = time.monotonic()
    while True:
        value = probe()
        if value:
            return value
        if time.monotonic() - start > timeout:
            raise Failure(f"timed out waiting for {what}")
        over_budget(what)
        time.sleep(interval)


# ----- text positions, as Zed counts them (UTF-16)


def utf16_index(line, character):
    """The index in the str `line` of UTF-16 offset `character`."""
    units = 0
    for index, char in enumerate(line):
        if units >= character:
            return index
        units += 2 if ord(char) > 0xFFFF else 1
    return len(line)


def offset(text, position):
    lines = text.split("\n")
    line = min(position["line"], len(lines) - 1)
    start = sum(len(part) + 1 for part in lines[:line])
    return start + utf16_index(lines[line], position["character"])


def apply_change(text, change):
    if "range" not in change:
        return change["text"]
    start, end = offset(text, change["range"]["start"]), offset(text, change["range"]["end"])
    return text[:start] + change["text"] + text[end:]


def path_of(uri):
    return urllib.parse.unquote(urllib.parse.urlparse(uri).path)


# ----- what Zed and the server say to each other


class Log:
    """What proxy.py wrote down, read as it grows: the documents Zed has
    open and their text, where its cursor was last, what was said and
    asked, and the requests not answered yet."""

    def __init__(self, path):
        self.path = path
        self.position = 0
        self.lines = 0
        self.documents = {}
        self.cursors = {}
        self.messages = []
        # Every message shown, prompts too, in turn.
        self.shown = []
        self.waiting = {}
        # When each request waiting was read, and the answers read before
        # their request.
        self.asked_at = {}
        self.early = {}
        self.prompts = []
        # What the server asked Zed to show, and whether to focus it.
        self.shows = []
        self.code_actions = {}
        # When each document last changed, and its code actions last came,
        # as lines of the log.
        self.changed = {}
        self.actions_came = {}
        # The commands Zed ran, from the code actions.
        self.executed = []
        # The documents Zed asked the folding ranges of.
        self.folded = set()
        self.renames = 0
        self.prepared = 0
        self.initialized = False
        self.last_change = time.monotonic()

    def read(self):
        if not os.path.exists(self.path):
            return
        with open(self.path, encoding="utf-8") as file:
            file.seek(self.position)
            while True:
                line = file.readline()
                if not line.endswith("\n"):
                    break
                self.position = file.tell()
                self.lines += 1
                self.last_change = time.monotonic()
                entry = json.loads(line)
                self.take(entry["from"], entry["message"] or {})

    def take(self, who, message):
        method = message.get("method")
        params = message.get("params") or {}
        key = (who, json.dumps(message.get("id")))
        if method and "id" in message:
            # proxy.py writes a request down before its answer; an answer
            # met first (a log of an older proxy.py) answers it still.
            early = self.early.pop(key, None)
            if early is None:
                self.waiting[key] = message
                self.asked_at[key] = time.monotonic()
            else:
                self.answered(message, early)
        elif "id" in message:
            # An answer, to a request of the other side's.
            other = "server" if who == "zed" else "zed"
            request = self.waiting.pop((other, json.dumps(message.get("id"))), None)
            self.asked_at.pop((other, json.dumps(message.get("id"))), None)
            if request:
                self.answered(request, message)
            else:
                self.early[(other, json.dumps(message.get("id")))] = message
        if who == "zed":
            if method == "textDocument/didOpen":
                document = params["textDocument"]
                self.documents[document["uri"]] = document["text"]
            elif method == "textDocument/didChange":
                uri = params["textDocument"]["uri"]
                self.changed[uri] = self.lines
                text = self.documents.get(uri, "")
                for change in params["contentChanges"]:
                    text = apply_change(text, change)
                self.documents[uri] = text
            elif method == "textDocument/didClose":
                self.documents.pop(params["textDocument"]["uri"], None)
            elif method == "textDocument/documentHighlight":
                position = params["position"]
                self.cursors[params["textDocument"]["uri"]] = (position["line"], position["character"])
            elif method == "initialized":
                self.initialized = True
            elif method == "workspace/executeCommand":
                self.executed.append(params.get("command"))
            elif method == "textDocument/foldingRange":
                self.folded.add(params["textDocument"]["uri"])
        else:
            if method == "window/showMessage":
                text = params.get("message", "")
                self.shown.append(text)
                if not text.endswith(RENAME_PROMPT):
                    self.messages.append(text)
            elif method == "window/showMessageRequest":
                self.shown.append(params.get("message", ""))
                self.prompts.append(message)
            elif method == "window/showDocument":
                self.shows.append((path_of(params["uri"]), params.get("takeFocus") is not False))

    def answered(self, request, answer):
        method = request.get("method")
        if method == "textDocument/codeAction":
            uri = request["params"]["textDocument"]["uri"]
            self.code_actions[uri] = (request["params"]["range"], answer.get("result") or [])
            self.actions_came[uri] = self.lines
        elif method == "textDocument/rename":
            self.renames += 1
        elif method == "textDocument/prepareRename":
            self.prepared += 1
        elif method == "window/showMessageRequest":
            self.prompts = [prompt for prompt in self.prompts if prompt.get("id") != request.get("id")]

    def busy(self):
        """Whether a request is waiting for its answer. Zed's inlay hints
        and the like may wait a while; the commands' do not."""
        slow = {"textDocument/inlayHint", "textDocument/semanticTokens/full", "textDocument/documentLink",
                "textDocument/codeLens", "textDocument/foldingRange", "workspace/diagnostic",
                "textDocument/diagnostic", "window/showMessageRequest"}
        return [
            (key, request) for key, request in self.waiting.items() if request.get("method") not in slow
        ]

    def unanswered(self, longer_than):
        """The requests of `busy` that have waited more than `longer_than`
        seconds for their answer, as `METHOD (id ID, from WHO)`."""
        now = time.monotonic()
        return [
            f"{request.get('method')} (id {key[1]}, from {key[0]})"
            for key, request in self.busy()
            if now - self.asked_at.get(key, now) > longer_than
        ]


# ----- keys


def xdotool_keys(key):
    """`xdotool`'s names for a key of Zed's, stroke by stroke (`ctrl-c a`)."""
    names = {
        "enter": "Return", "escape": "Escape", "tab": "Tab", "space": "space", "backspace": "BackSpace",
        "delete": "Delete", "home": "Home", "end": "End", "pageup": "Prior", "pagedown": "Next",
        "up": "Up", "down": "Down", "left": "Left", "right": "Right",
        ",": "comma", ".": "period", "/": "slash", "\\": "backslash", ";": "semicolon", "'": "apostrophe",
        "[": "bracketleft", "]": "bracketright", "-": "minus", "=": "equal", "`": "grave",
        "{": "braceleft", "}": "braceright", "$": "dollar", "*": "asterisk", "%": "percent", "+": "plus",
        ":": "colon", "<": "less", ">": "greater", "_": "underscore", "^": "asciicircum", "|": "bar",
        "@": "at", "!": "exclam", "?": "question", '"': "quotedbl", "&": "ampersand", "#": "numbersign",
        "~": "asciitilde", "(": "parenleft", ")": "parenright",
    }
    strokes = []
    for stroke in key.split(" "):
        parts = []
        rest = stroke
        while True:
            found = re.match(r"^(ctrl|shift|alt|super)-(.+)$", rest)
            if not found:
                break
            parts.append(found.group(1))
            rest = found.group(2)
        last = names.get(rest, rest.upper() if re.fullmatch(r"f\d+", rest) else rest)
        strokes.append("+".join(parts + [last]))
    return strokes


def manifest():
    return json.loads(subprocess.run([SERVER, "manifest"], capture_output=True, check=True, text=True).stdout)


def zed_key(emacs):
    """An Emacs key (`C-c C-s`) in Zed's notation, as editors/zed/generate.mjs writes it."""
    names = {"RET": "enter", "TAB": "tab", "SPC": "space", "DEL": "backspace", "ESC": "escape"}
    modifiers = {"C": "ctrl", "M": "alt", "S": "shift"}
    strokes = []
    for stroke in emacs.split(" "):
        parts = []
        rest = stroke
        while True:
            found = re.match(r"^([CMS])-(.+)$", rest)
            if not found:
                break
            parts.append(modifiers[found.group(1)])
            rest = found.group(2)
        named = re.match(r"^<(.+)>$", rest)
        base = names.get(rest, named.group(1) if named else rest)
        if len(base) == 1 and base != base.lower():
            parts.append("shift")
            base = base.lower()
        strokes.append("-".join(parts + [base]))
    return " ".join(strokes)


class Keys:
    """The keys keymap.json binds, by where they are pressed (`org-agenda`,
    `org`) and the command they run."""

    def __init__(self):
        self.keymap = json.load(open(os.path.join(EXTENSION, "keymap.json")))
        tasks = {task["label"]: task["command"] for task in json.load(open(os.path.join(EXTENSION, "tasks.json")))}
        self.manifest = manifest()
        self.bound = collections.defaultdict(list)
        for block in self.keymap:
            where = "org-agenda" if "org-agenda" in block["context"] else "org"
            for key, action in block["bindings"].items():
                if isinstance(action, list) and action[0] == "task::Spawn":
                    run = tasks[action[1]["task_name"]].split("agenda-command ", 1)[1].strip("'")
                    for command in self.commands_of(run, where):
                        self.bound[(where, command)].append(key)
                elif action == "editor::GoToDefinition":
                    self.bound[(where, "org.agenda.goto")].append(key)
                elif action == "editor::GoToDefinitionSplit":
                    self.bound[(where, "org.agenda.goto.other")].append(key)
                elif action == "pane::CloseActiveItem":
                    self.bound[(where, "close")].append(key)
                elif action == "workspace::SaveAll":
                    # Zed saves the files itself, as no task can.
                    self.bound[(where, "org.agenda.save")].append(key)

    def commands_of(self, run, where):
        """The commands a task's `run` stands for: `file:COMMAND`, or `key:KEY`
        (the commands of that Org key in an Org file and a capture buffer)."""
        if run.startswith("key:"):
            key = run[4:]
            return [
                command["command"]
                for command in self.manifest["commands"]
                for bound in command["keys"]
                if bound.get("in") in ("org", "capture") and bound.get("preset") == "emacs" and bound["key"] == key
            ]
        return [run.removeprefix("file:")]

    def key_of(self, command, where):
        keys = self.bound.get((where, command))
        return keys[0] if keys else None

    def title_of(self, command):
        for entry in self.manifest["commands"]:
            if entry["command"] == command:
                return entry.get("title")
        return None


# ----- a scenario


def expected_of(name):
    expected = {}
    current = None
    with open(os.path.join(E2E, "expected", f"{name}.txt"), encoding="utf-8") as file:
        lines = file.read().split("\n")[:-1]
    for line in lines:
        header = re.match(r"^=== (\d+) (.+)$", line)
        if header:
            current = f"{header.group(1)} {header.group(2)}"
            expected[current] = []
        elif current:
            expected[current].append(line)
    return expected


def ended(text):
    return text.rstrip("\n") + "\n"


DISPLAYS = threading.Lock()
# The Zeds running, each with its display: what goes when run.py is
# stopped, as Xvfb and Zed run in sessions of their own and would stay.
RUNNING = set()
RUNNING_LOCK = threading.Lock()


def stop_all():
    with RUNNING_LOCK:
        running = list(RUNNING)
    for zed in running:
        zed.stop()


def on_signal(number, _frame):
    """Stopped (`timeout`'s SIGTERM, Ctrl-C): the Zeds and their displays
    go too, and run.py ends as the signal would end it."""
    signal.signal(number, signal.SIG_IGN)
    print(f"stopped by signal {number}: closing {len(RUNNING)} Zed(s)", file=sys.stderr, flush=True)
    stop_all()
    os._exit(128 + number)


def free_display():
    first = int(os.environ.get("ORG_LSP_ZED_DISPLAY", "90"))
    for number in range(first, first + 110):
        if not os.path.exists(f"/tmp/.X11-unix/X{number}") and not os.path.exists(f"/tmp/.X{number}-lock"):
            return number
    raise Failure("no free X display")


class Zed:
    """One Zed, on a display of its own, with a home of its own."""

    def __init__(self, scenario, keys):
        self.scenario = scenario
        self.keys = keys
        # Short: Zed's socket in the home directory has a path of at most
        # 108 bytes.
        self.base = tempfile.mkdtemp(prefix="ozed-", dir="/tmp")
        self.home = os.path.join(self.base, "h")
        self.work = os.path.join(self.base, "w")
        os.makedirs(self.home)
        os.makedirs(self.work)
        self.log = Log(os.path.join(self.base, "lsp.log"))
        self.display = None
        self.xvfb = None
        self.zed = None
        self.window = None
        # Reentrant: a signal may come while this thread stops it.
        self.stopping = threading.RLock()
        self.stopped = False
        with RUNNING_LOCK:
            RUNNING.add(self)

    # --- setting up

    def install(self, options):
        data = os.path.join(self.home, ".local", "share", "zed")
        config = os.path.join(self.home, ".config", "zed")
        extension = os.path.join(data, "extensions", "installed", "org-lsp")
        os.makedirs(os.path.join(extension, "grammars"))
        os.makedirs(config)
        shutil.copy(os.path.join(EXTENSION, "extension.toml"), extension)
        shutil.copy(extension_wasm(), os.path.join(extension, "extension.wasm"))
        shutil.copy(grammar_wasm(), os.path.join(extension, "grammars", "org.wasm"))
        shutil.copytree(os.path.join(EXTENSION, "languages"), os.path.join(extension, "languages"))
        # The tasks and the keys as docs/keys/zed.org says to paste them,
        # and one key of the harness's own.
        shutil.copy(os.path.join(EXTENSION, "tasks.json"), config)
        keymap = list(self.keys.keymap)
        keymap.append({"context": "Workspace", "bindings": {CLEAR_NOTIFICATIONS: "workspace::ClearAllNotifications"}})
        with open(os.path.join(config, "keymap.json"), "w") as file:
            json.dump(keymap, file, indent=2)
        settings = {
            "telemetry": {"diagnostics": False, "metrics": False},
            "auto_update": False,
            "session": {"trust_all_worktrees": True},
            "restore_on_startup": "none",
            # The current buffer, for the harness to read.
            "window_title_format": "${filePath}",
            # The text typed stays as typed, and is saved as it is.
            "show_completions_on_input": False,
            "use_autoclose": False,
            "format_on_save": "off",
            "remove_trailing_whitespace_on_save": False,
            "ensure_final_newline_on_save": False,
            "show_edit_predictions": False,
            "languages": {"Org": {"semantic_tokens": "combined", "inlay_hints": {"enabled": True},
                                  "document_folding_ranges": "on"}},
            "lsp": {
                "org-lsp": {
                    "binary": {"path": sys.executable, "arguments": [os.path.join(HERE, "proxy.py"), SERVER]},
                    "initialization_options": options,
                }
            },
        }
        with open(os.path.join(config, "settings.json"), "w") as file:
            json.dump(settings, file, indent=2)

    def environment(self):
        env = {
            "HOME": self.home,
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "DISPLAY": f":{self.display}",
            "XDG_RUNTIME_DIR": self.base,
            "SHELL": "/bin/sh",
            "ORG_LSP_ZED_LOG": self.log.path,
            "ORG_LSP_CONFIG": "/dev/null",
            "ZED_ALLOW_EMULATED_GPU": "1",
            "LANG": os.environ.get("LANG", "C.UTF-8"),
        }
        if os.geteuid() == 0:
            env["ZED_ALLOW_ROOT"] = "true"
        lavapipe = "/usr/share/vulkan/icd.d/lvp_icd.json"
        if "VK_ICD_FILENAMES" in os.environ:
            env["VK_ICD_FILENAMES"] = os.environ["VK_ICD_FILENAMES"]
        elif os.path.exists(lavapipe):
            env["VK_ICD_FILENAMES"] = lavapipe
        for name in ("ORG_LISP_DIR", "EMACS"):
            if name in os.environ:
                env[name] = os.environ[name]
        return env

    def start(self):
        with DISPLAYS:
            self.display = free_display()
            self.xvfb = subprocess.Popen(
                ["Xvfb", f":{self.display}", "-screen", "0", f"{WIDTH}x{HEIGHT}x24", "-nolisten", "tcp"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
            )
            until("the X display", lambda: os.path.exists(f"/tmp/.X11-unix/X{self.display}"), 10)

        def window():
            out = self.x(["xdotool", "search", "--onlyvisible", "--name", "."]).split()
            return out[0] if out else None

        # Once more when no window comes: now and then Zed starts with none.
        for attempt in range(2):
            # Zed writes to the log; this process needs it no more once
            # Zed has it.
            with open(os.path.join(self.base, "zed.log"), "a") as log:
                self.zed = subprocess.Popen(
                    [os.environ["ZED"], "--foreground", self.work],
                    env=self.environment(), cwd=self.work, start_new_session=True,
                    stdout=log, stderr=subprocess.STDOUT,
                )
            try:
                self.window = until("Zed's window", window, 60)
                break
            except Failure:
                if attempt == 1:
                    raise
                os.killpg(self.zed.pid, signal.SIGKILL)
                self.zed.wait()
        # The window sized to the screen, so that the buttons are where
        # they are looked for.
        self.x(["xdotool", "windowsize", self.window, str(WIDTH), str(HEIGHT)])
        self.x(["xdotool", "windowmove", self.window, "0", "0"])

    def stop(self):
        """Zed, its server and the display go, once: at the scenario's
        end, or when run.py is stopped (`stop_all`)."""
        with self.stopping:
            if self.stopped:
                return
            self.stopped = True
            for process in (self.zed, self.xvfb):
                if process and process.poll() is None:
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
            for process in (self.zed, self.xvfb):
                if process:
                    try:
                        process.wait(5)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        process.wait()
            # Zed's processes, the proxy and the server are in Zed's process
            # group; whatever has left it (its command line names the
            # scenario's directory) goes too.
            subprocess.run(["pkill", "-KILL", "-f", self.base], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if not os.environ.get("ORG_LSP_ZED_KEEP"):
                shutil.rmtree(self.base, ignore_errors=True)
        with RUNNING_LOCK:
            RUNNING.discard(self)

    def x(self, command, timeout=30):
        env = dict(os.environ, DISPLAY=f":{self.display}")
        try:
            return subprocess.run(command, env=env, capture_output=True, text=True, timeout=timeout).stdout
        except subprocess.TimeoutExpired:
            raise Failure(f"`{' '.join(command[:2])}` took more than {timeout}s") from None

    def screenshot(self, path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        self.x(["import", "-window", "root", path])

    # --- what Zed shows

    def title(self):
        return self.x(["xdotool", "getwindowname", self.window]).strip()

    def current(self):
        """The uri of the buffer at hand, from the window's title."""
        title = self.title()
        if not title.startswith("/"):
            return None
        return "file://" + urllib.parse.quote(title)

    def find_uri(self, uri):
        """`uri` as Zed names it in what it sends (quoted as it quotes)."""
        wanted = path_of(uri)
        for known in list(self.log.documents) + list(self.log.cursors):
            if path_of(known) == wanted:
                return known
        return uri

    def settle(self, quiet=0.5, timeout=20.0):
        """Wait until Zed and the server have done what they do: nothing
        is said for a while, nothing waits for an answer, and the title
        stays. Zed may go on talking for longer than `timeout` (the
        moment then passed); a request with no answer for that long is a
        failure."""
        start = time.monotonic()
        title = self.title()
        while True:
            self.log.read()
            now = self.title()
            if now != title:
                title = now
                self.log.last_change = time.monotonic()
            if time.monotonic() - self.log.last_change > quiet and not self.log.busy():
                return
            if time.monotonic() - start > timeout:
                unanswered = self.log.unanswered(timeout)
                if unanswered:
                    raise Failure(f"no answer in {timeout:.0f}s to {', '.join(unanswered)}")
                return
            over_budget("Zed and the server to settle")
            time.sleep(0.05)

    def tasks_done(self):
        """Wait until the tasks the keys ran are done, and Zed has put
        their terminals away: one that goes takes the focus from a rename
        being typed, which ends it."""
        script = os.path.join(self.home, ".local", "state", "org-lsp", "agenda-command")
        running = lambda: subprocess.run(["pgrep", "-f", script], capture_output=True).returncode == 0
        until("the task to finish", lambda: not running(), 20)
        time.sleep(0.3)

    def wait_for_lines(self, before, timeout=4.0):
        """Wait for something said after `before` lines of the log."""
        start = time.monotonic()
        while time.monotonic() - start < timeout:
            self.log.read()
            if self.log.lines > before:
                return True
            time.sleep(0.03)
        return False

    # --- doing

    def press(self, key):
        self.x(["xdotool", "key", "--window", self.window, "--delay", "40"] + xdotool_keys(key))

    def type(self, text):
        # A new line is Enter: `xdotool` types it as a key Zed inserts nothing for.
        for index, line in enumerate(text.split("\n")):
            if index > 0:
                self.press("enter")
            if line:
                self.x(["xdotool", "type", "--window", self.window, "--delay", "20", "--", line])

    def click(self, x, y):
        self.x(["xdotool", "mousemove", str(x), str(y), "click", "1"])
        # The pointer out of the way again, off the buttons it would light.
        self.x(["xdotool", "mousemove", "1", str(HEIGHT // 2)])

    def open(self, path):
        """Go to `path` as a user does, with the file finder (`ctrl-p`): its
        path in the project typed, or the name of a file outside it, which
        the finder has among the files opened before (the agenda), then
        Enter."""
        if self.title() == path:
            return
        work = os.path.realpath(self.work)
        query = path[len(work) + 1:] if path.startswith(work + "/") else os.path.basename(path)
        # Again when the finder had not found it yet (Zed still reading the
        # project, as it starts).
        for attempt in range(3):
            self.press("ctrl-p")
            time.sleep(0.4)
            self.type(query)
            # The finder matches as the text comes.
            time.sleep(0.6)
            self.press("enter")
            try:
                until(f"{query} to be shown from the file finder", lambda: self.title() == path, 5)
                return
            except Failure:
                if attempt == 2:
                    raise
                self.press("escape")

    def pixels(self):
        """The screen, as rows of (r, g, b)."""
        env = dict(os.environ, DISPLAY=f":{self.display}")
        try:
            data = subprocess.run(["import", "-window", "root", "-depth", "8", "rgb:-"], env=env,
                                  capture_output=True, timeout=30).stdout
        except subprocess.TimeoutExpired:
            raise Failure("the screen could not be read in 30s") from None
        return data

    def button_rows(self, count):
        """Where the buttons of the question shown last are, from the top:
        Zed shows it bottom right, the newest notification lowest, its
        buttons stacked at its foot, 32 pixels apart."""
        data = self.pixels()

        def pixel(x, y):
            at = (y * WIDTH + x) * 3
            return tuple(data[at:at + 3])

        # Up from the status bar, along a column of the editor beside the
        # notification's left edge, to the notification's foot.
        column = WIDTH - 300
        background = pixel(column, HEIGHT - 40)
        bottom = None
        for y in range(HEIGHT - 32, HEIGHT // 3, -1):
            if pixel(column, y) != background:
                bottom = y
                break
        if bottom is None:
            raise Failure("no question to answer is shown")
        # Its padding (12), then the buttons from the last up.
        return [bottom - 12 - 16 - (count - 1 - index) * 32 for index in range(count)]

    def button_x(self):
        return WIDTH - 437 + 21


def extension_wasm():
    out = os.path.join(EXTENSION, "target", "wasm32-wasip2", "release", "zed_org_lsp.wasm")
    return out


def grammar_wasm():
    given = os.environ.get("ZED_ORG_GRAMMAR")
    if given:
        return given
    built = os.path.join(ROOT, "target", "zed-e2e", "org.wasm")
    if os.path.exists(built):
        return built
    source = os.environ.get("ZED_TREE_SITTER_ORG")
    sdk = os.environ.get("WASI_SDK")
    if not source or not sdk:
        raise Failure("set ZED_ORG_GRAMMAR to the Org grammar (org.wasm), or ZED_TREE_SITTER_ORG and WASI_SDK to build it")
    os.makedirs(os.path.dirname(built), exist_ok=True)
    # As Zed builds a grammar of an extension.
    subprocess.run(
        [os.path.join(sdk, "bin", "clang"), "-fPIC", "-shared", "-Os", "-Wl,--export=tree_sitter_org",
         "-o", built, "-I", os.path.join(source, "src"), os.path.join(source, "src", "parser.c"),
         os.path.join(source, "src", "scanner.c")],
        check=True,
    )
    return built


class Scenario:
    def __init__(self, name, keys):
        self.name = name
        self.keys = keys
        with open(os.path.join(E2E, "scenarios", f"{name}.json"), encoding="utf-8") as file:
            self.spec = json.load(file)
        self.expected = expected_of(name)
        self.zed = None
        self.directory = None
        self.seen = 0

    def files(self):
        files = {}
        source = self.spec.get("files_from")
        if source:
            directory = os.path.join(ROOT, "tests", source)
            for name in os.listdir(directory):
                if name.endswith(".org") or name.endswith(".org_archive"):
                    with open(os.path.join(directory, name), encoding="utf-8") as file:
                        files[name] = file.read()
        for name, text in self.spec.get("files", {}).items():
            files[name] = "".join(f"{line}\n" for line in text) if isinstance(text, list) else text
        return files

    def substitute(self, value):
        if isinstance(value, str):
            return value.replace("$DIR", self.directory)
        if isinstance(value, list):
            return [self.substitute(item) for item in value]
        if isinstance(value, dict):
            return {key: self.substitute(item) for key, item in value.items()}
        return value

    def options(self):
        options = {
            "config_file": False,
            "clock": self.spec.get("clock", {"today": "2026-05-18", "now": "10:30"}),
            "org_agenda_files": [self.directory],
            "org_directory": self.directory,
            "org_default_notes_file": f"{self.directory}/notes.org",
        }
        for key, value in self.spec.get("options", {}).items():
            options[key] = self.substitute(value)
        if self.spec.get("emacs"):
            lisp = os.environ.get("ORG_LISP_DIR")
            prefix = f"(add-to-list 'load-path {json.dumps(lisp)}) " if lisp else ""
            options["emacs_config"] = prefix + options.get("emacs_config", "")
        return options

    def name_of(self, uri):
        path = path_of(uri)
        if path.endswith(".org-agenda"):
            return "agenda"
        if path.startswith(self.directory + "/"):
            return path[len(self.directory) + 1:]
        return os.path.basename(path)

    def where(self):
        """Where the cursor is, for the keys: the agenda or an Org file."""
        current = self.zed.current() or ""
        return "org-agenda" if current.endswith(".org-agenda") else "org"

    def agenda_uri(self):
        for uri in self.zed.log.documents:
            if uri.endswith(".org-agenda"):
                return uri
        return None

    def cursor(self):
        uri = self.zed.find_uri(self.zed.current() or "")
        return uri, self.zed.log.cursors.get(uri, (0, 0))

    # --- running

    def run(self):
        self.zed = Zed(self, self.keys)
        self.directory = os.path.realpath(self.zed.work)
        for name, text in self.files().items():
            path = os.path.join(self.directory, name)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as file:
                file.write(text)
        self.zed.install(self.options())
        deadline.at = time.monotonic() + BUDGET
        try:
            self.zed.start()
            for index, step in enumerate(self.spec["steps"], 1):
                try:
                    began = time.monotonic()
                    over_budget(f"step {index} to begin")
                    self.step(index, step)
                    if os.environ.get("ORG_LSP_ZED_VERBOSE"):
                        print(f"{self.name} {index}: {time.monotonic() - began:.1f}s", flush=True)
                except Failure as error:
                    shot = os.path.join(SHOTS, f"{self.name}-{index}.png")
                    try:
                        self.zed.screenshot(shot)
                    except Failure:
                        shot = "(none: the screen could not be read)"
                    if os.environ.get("ORG_LSP_ZED_PAUSE"):
                        # Zed stays, for a look, until the file goes.
                        pause = os.path.join(self.zed.base, "pause")
                        open(pause, "w").close()
                        print(f"{self.name}: paused on display :{self.zed.display}, rm {pause}", flush=True)
                        while os.path.exists(pause):
                            time.sleep(1)
                    raise Failure(f"step {index} {json.dumps(step)}: {error}\n  screenshot: {shot}") from None
        finally:
            deadline.at = None
            self.zed.stop()

    def place(self, step):
        line = step.get("line")
        column = step.get("column", 0)
        if not line and "at" in step:
            uri, _ = self.cursor()
            text = self.zed.log.documents.get(uri)
            if text is None:
                raise Failure(f"Zed has not told the server of {self.name_of(uri)}")
            for number, content in enumerate(text.split("\n"), 1):
                found = content.find(step["at"])
                if found >= 0:
                    line, column = number, found + column
                    break
            else:
                raise Failure(f"`{step['at']}` is not in {self.name_of(uri)}")
        if not line:
            return
        uri, _ = self.cursor()
        self.go_to(uri, line - 1, column)

    def go_to(self, uri, line, column):
        """Put the cursor at `line`, `column` (from 0) with `go_to_line`."""
        text = self.zed.log.documents.get(uri, "")
        content = (text.split("\n") + [""])[line]
        # Zed counts columns in characters, from 1; the server, in UTF-16.
        units = len(content[:column].encode("utf-16-le")) // 2
        # Always: the cursor last told of may have moved since, as the
        # agenda's text changed under it.
        there = self.zed.log.cursors.get(uri) == (line, units)
        self.zed.press("ctrl-g")
        time.sleep(0.2)
        self.zed.type(f"{line + 1}:{column + 1}")
        self.zed.press("enter")
        if there:
            self.zed.settle()
        else:
            until(f"the cursor on line {line + 1}",
                  lambda: (self.zed.log.read(), self.zed.log.cursors.get(uri) == (line, units))[1], 5)
            self.zed.settle()

    def step(self, index, step):
        zed = self.zed
        # A step Zed cannot take as the others do.
        if f"{self.name} {index}" in DIFFERENCES:
            return
        if "open" in step:
            path = os.path.join(self.directory, step["open"])
            zed.open(path)
            until("the server to start", lambda: (zed.log.read(), zed.log.initialized)[1], 60)
            until(f"Zed to open {step['open']}", lambda: (zed.log.read(), zed.find_uri("file://" + path) in zed.log.documents)[1], 30)
            # Zed folds an Org file with the server's folding ranges, as
            # docs/zed.org sets it up (`document_folding_ranges`).
            until(f"Zed to ask for the folding ranges of {step['open']}",
                  lambda: (zed.log.read(), zed.find_uri("file://" + path) in zed.log.folded)[1], 30)
            zed.settle()
        if "focus" in step:
            if step["focus"] == "agenda":
                uri = self.agenda_uri()
                if not uri:
                    raise Failure("no agenda to go to")
                zed.open(path_of(uri))
            else:
                zed.open(os.path.join(self.directory, step["focus"]))
            zed.settle()
        if step.get("close"):
            key = self.keys.key_of("close", self.where())
            if not key:
                raise Failure(f"no key to close {self.name_of(zed.current() or '')}")
            zed.press(key)
            until("the agenda to close", lambda: (zed.log.read(), self.agenda_uri() is None)[1], 10)
            zed.settle()
        self.place(step)
        if "type" in step:
            uri, _ = self.cursor()
            before = zed.log.documents.get(uri, "")
            zed.type(step["type"])
            until("the text typed", lambda: (zed.log.read(), len(zed.log.documents.get(uri, "")) >= len(before) + len(step["type"]))[1], 10)
            zed.settle()
        if "command" in step:
            self.command(step)
        for kind in step.get("expect", []):
            self.check(index, kind)
        if "messages" in step.get("expect", []):
            self.seen = len(zed.log.messages)

    def command(self, step):
        zed = self.zed
        command = step["command"]
        answers = collections.deque(step.get("answers", []))
        if step.get("wait") == "columns":
            raise Failure("Zed shows no column view")
        # Messages left over from before are cleared away from the buttons.
        zed.press(CLEAR_NOTIFICATIONS)
        if step.get("below"):
            # Zed's keys add where the cursor is, as Emacs's do: to add
            # below the line, the cursor goes to its end first (`C-e M-RET`).
            uri, (line, _) = self.cursor()
            text = zed.log.documents.get(uri, "").split("\n")[line]
            self.go_to(uri, line, len(text))
        where = self.where()
        before_uri = zed.current()
        shows = len(zed.log.shows)
        key = self.keys.key_of(command, where)
        # Zed gives a key no count: the key is pressed that many times,
        # or once for a command Org gives the count no meaning
        # (`COMMAND count` in the exceptions).
        times = 1 if f"{command} count" in EXCEPTED else step.get("count", 1)
        for time_ in range(times):
            before = zed.log.lines
            shown = len(zed.log.shown)
            said = len(zed.log.messages)
            if key:
                zed.press(key)
            elif command in EXCEPTED:
                self.code_action(command)
            else:
                raise Failure(f"{command} has no key in Zed here ({where})")
            zed.wait_for_lines(before)
            self.answer(command, answers, shown)
            self.go_back(command, before_uri, shows)
            # What a key pressed again says takes the place of what it said
            # before, as in Emacs's echo area for one key with a count.
            if time_ > 0:
                del zed.log.messages[repeated:said]
            else:
                repeated = said
        if answers:
            raise Failure(f"{command} did not ask for {list(answers)}")
        if command == "org.agenda.save":
            # Zed's own save: each Org file as Zed has it is on disk.
            until("Zed to save the Org files", self.saved, 10)

    def saved(self):
        self.zed.log.read()
        for uri, text in self.zed.log.documents.items():
            path = path_of(uri)
            if path.startswith(self.directory + "/") and os.path.exists(path):
                with open(path, encoding="utf-8") as file:
                    if file.read() != text:
                        return False
        return True

    def go_back(self, command, before_uri, shows):
        """Zed shows a file a server's edit changes that no tab shows
        (`open_transaction_for_hidden_buffers`), and focuses what it is
        asked to show without focus: the user goes back to where they
        were."""
        zed = self.zed
        now = zed.current()
        focused = {path for path, focus in zed.log.shows[shows:] if focus}
        moved = now != before_uri and before_uri and path_of(now or "") not in focused
        if moved and command not in ("org.agenda.goto", "org.agenda.goto.other") and \
                zed.find_uri(before_uri) in zed.log.documents:
            zed.open(path_of(before_uri))
            zed.settle()

    def code_action(self, command):
        """Run `command` from the code actions menu (`ctrl-.`), down to its
        line and Enter."""
        zed = self.zed
        uri, (line, _) = self.cursor()
        title = self.keys.title_of(command)

        def matches(action):
            return (action.get("command") or {}).get("command") == command or action.get("kind") == command

        # Now and then the menu does not open, or shows the actions from
        # before: once more then.
        for _ in range(3):
            zed.settle()
            # Zed shows the actions it asked for after the last change, and
            # asks for them a moment after the cursor moves (or as the menu
            # opens).
            def current():
                zed.log.read()
                known = zed.log.code_actions.get(uri)
                fresh = zed.log.actions_came.get(uri, 0) > zed.log.changed.get(uri, 0)
                # On the cursor's line: Zed asks where its cursor went with
                # an edit, which it does not always tell of otherwise.
                return fresh and known[0]["start"]["line"] == line
            try:
                until("Zed to ask for the code actions", current, 3)
            except Failure:
                pass
            zed.press("ctrl-.")
            until("Zed to ask for the code actions", current, 5)
            time.sleep(0.3)
            zed.settle()
            actions = zed.log.code_actions[uri][1]
            index = next((at for at, action in enumerate(actions) if matches(action)), None)
            if index is None and title:
                index = next((at for at, action in enumerate(actions) if action.get("title", "").startswith(title)), None)
            if index is None:
                zed.press("escape")
                raise Failure(f"no code action for {command} here: {[action.get('title') for action in actions]}")
            ran, changed = len(zed.log.executed), zed.log.changed.get(uri, 0)
            for _ in range(index):
                zed.press("down")
            zed.press("enter")
            # A command Zed runs, or an edit it makes.
            wanted = (actions[index].get("command") or {}).get("command")

            def done():
                zed.log.read()
                return wanted in zed.log.executed[ran:] if wanted else zed.log.changed.get(uri, 0) > changed
            try:
                until(f"Zed to run {actions[index].get('title')}", done, 3)
                return
            except Failure:
                zed.press("escape")
        raise Failure(f"the code action for {command} did not run")

    def answer(self, command, answers, shown):
        """Answer what the command asks, as Zed shows it: buttons, or a
        message to answer with a rename."""
        zed = self.zed
        while True:
            zed.settle()
            if zed.log.prompts:
                prompt = zed.log.prompts[0]
                titles = [action["title"] for action in prompt["params"].get("actions") or []]
                if not answers:
                    raise Failure(f"{command} asks `{prompt['params'].get('message')}` and the step has no answer left")
                answer = answers.popleft()
                index = pick(titles, answer)
                if index is None:
                    raise Failure(f"no choice {json.dumps(answer)} in {titles}")
                zed.tasks_done()
                rows = zed.button_rows(len(titles))
                before = zed.log.lines
                shown = len(zed.log.shown)
                zed.click(zed.button_x(), rows[index])
                until(f"the button `{titles[index]}` to be clicked", lambda: (zed.log.read(), prompt not in zed.log.prompts)[1], 5)
                zed.wait_for_lines(before)
                continue
            asked = [text for text in zed.log.shown[shown:] if text.endswith(RENAME_PROMPT)]
            if asked:
                if not answers:
                    raise Failure(f"{command} asks `{asked[-1]}` and the step has no answer left")
                answer = answers.popleft()
                if not isinstance(answer, str):
                    raise Failure(f"`{asked[-1]}` asks for text, not {json.dumps(answer)}")
                # Zed sends no empty new name: `-` alone is the empty answer.
                answer = answer or "-"
                zed.tasks_done()
                renames = zed.log.renames
                prepared = zed.log.prepared
                shown = len(zed.log.shown)
                zed.press("f2")
                until("Zed to ask the server to rename", lambda: (zed.log.read(), zed.log.prepared > prepared)[1], 5)
                # The rename's editor comes a moment after the answer: a key
                # before goes to the agenda.
                time.sleep(0.5)
                zed.type(answer)
                zed.press("enter")
                until("the rename to be answered", lambda: (zed.log.read(), zed.log.renames > renames)[1], 10)
                zed.wait_for_lines(zed.log.lines, 1)
                continue
            return

    # --- looking

    def snapshot(self, kind):
        zed = self.zed
        zed.log.read()
        if kind == "agenda":
            uri = self.agenda_uri()
            return ended(zed.log.documents[uri]) if uri else "(no agenda)\n"
        if kind == "buffer":
            uri, _ = self.cursor()
            return f"[{self.name_of(uri)}]\n{ended(zed.log.documents.get(uri, ''))}"
        if kind == "cursor":
            uri, (line, _) = self.cursor()
            text = zed.log.documents.get(uri, "").split("\n")
            return f"{self.name_of(uri)}:{line + 1}: {text[line] if line < len(text) else ''}\n"
        if kind == "messages":
            return "".join(f"{message}\n" for message in zed.log.messages[self.seen:])
        if kind.startswith("file:"):
            path = os.path.join(self.directory, kind[5:])
            uri = zed.find_uri("file://" + urllib.parse.quote(path))
            if uri in zed.log.documents:
                return ended(zed.log.documents[uri])
            if os.path.exists(path):
                with open(path, encoding="utf-8") as file:
                    return ended(file.read())
            return "(no file)\n"
        raise Failure(f"Zed shows no {kind}")

    def check(self, index, kind):
        if f"{self.name} {index} {kind}" in DIFFERENCES:
            return
        want = self.expected.get(f"{index} {kind}")
        if want is None:
            raise Failure(f"no expected {kind}; run the server's runner with ORG_LSP_UPDATE_E2E=1")
        wanted = "\n".join(want) + "\n" if want else ""
        start = time.monotonic()
        while True:
            got = self.snapshot(kind).replace(self.directory, "$DIR")
            if got == wanted or time.monotonic() - start > 3:
                break
            self.zed.settle(0.3, 2)
        if got != wanted:
            raise Failure(f"{kind}:\n--- expected\n{wanted}--- actual\n{got}")


def pick(titles, answer):
    """The button an answer clicks: a key (`[t]` at the end of its title),
    or `{"choice": label}`."""
    if isinstance(answer, str):
        names = {" ": "SPC", "\r": "RET", "\t": "TAB"}
        key = names.get(answer, answer)
        for index, title in enumerate(titles):
            if title.endswith(f" [{key}]") or title == answer:
                return index
        # A key whose label is itself, in the other case (`C` for `c`), as
        # Org reads it.
        for index, title in enumerate(titles):
            if len(answer) == 1 and title.lower() == answer.lower():
                return index
        return None
    label = answer.get("choice")
    bare = [re.sub(r" \[[^\]]*\]$", "", title) for title in titles]
    for index, title in enumerate(bare):
        if title == label:
            return index
    for index, title in enumerate(bare):
        if label in title:
            return index
    return None


def run_one(name, keys):
    scenario = Scenario(name, keys)
    if name in SKIPPED:
        return name, "skip", SKIPPED[name]
    if scenario.spec.get("emacs") and not shutil.which("emacs"):
        if os.environ.get("ORG_LSP_REQUIRE_EMACS"):
            return name, "fail", "Emacs is required but not found"
        return name, "skip", "no Emacs"
    start = time.monotonic()
    try:
        scenario.run()
    except Failure as error:
        return name, "fail", str(error)
    except Exception as error:  # noqa: BLE001 - reported with the scenario
        return name, "fail", f"{type(error).__name__}: {error}"
    return name, "ok", f"{time.monotonic() - start:.0f}s"


def main():
    if not os.environ.get("ZED"):
        sys.exit("set ZED to Zed's command line (bin/zed)")
    for number in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(number, on_signal)
    atexit.register(stop_all)
    only = os.environ.get("ORG_LSP_E2E", "")
    names = sorted(file[:-5] for file in os.listdir(os.path.join(E2E, "scenarios")) if file.endswith(".json"))
    names = [name for name in names if only in name and (not sys.argv[1:] or name in sys.argv[1:])]
    subprocess.run(["cargo", "build", "--release", "--target", "wasm32-wasip2", "--quiet"], cwd=EXTENSION, check=True)
    grammar_wasm()
    keys = Keys()
    jobs = int(os.environ.get("ORG_LSP_E2E_JOBS", "1"))
    start = time.monotonic()
    failed = 0
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        for name, status, detail in pool.map(lambda name: run_one(name, keys), names):
            if status == "ok":
                print(f"ok - {name} ({detail})", flush=True)
            elif status == "skip":
                print(f"ok - {name} # skip {detail}", flush=True)
            else:
                failed += 1
                print(f"not ok - {name}: {detail}", flush=True)
    print(f"{len(names) - failed}/{len(names)} scenarios passed in Zed, {time.monotonic() - start:.0f}s")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
