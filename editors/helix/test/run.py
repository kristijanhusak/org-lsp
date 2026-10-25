#!/usr/bin/env python3
"""The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in Helix.

Each scenario runs in a Helix of its own, in a tmux session, with the
repository's languages.toml (editors/helix/languages.toml) pointing at the
server and the scenario's options added to its `config`, which Helix
passes as the initialization options. The steps are what a Helix user
does: the code action menu (`Space a`, then the action picked with `C-n`
and Enter) for the commands the server offers there, in Org files, the
agenda and capture buffers, `gd` for following a link: every key typed in Helix's terminal through tmux, as a user types it.
What the server asks for is answered as docs/helix.org says: a choice
from the code action menu, which starts with the choices while the
question waits (`message_requests` off), and text with a rename at the
cursor (`Space r`, the answer typed). Helix gives a code action no count:
a step with one picks the action that many times, or once where Org gives
the count no meaning (`COMMAND count` under `editors.helix` in
tests/e2e/exceptions.json). A step that adds below the cursor's line goes
past the line's end first (`gl l`), as `C-e M-RET` does.

What is looked at: the buffers as Helix has them (the text it told the
server of, `didOpen` and `didChange`, from its log), files on disk that
Helix has not opened, the cursor from the status line, and the messages
Helix was sent (`window/showMessage`), which it shows in the status line
in turn. What a scenario expects is what the server's own runner saw
(server/org-lsp/tests/e2e_scenarios.rs, tests/e2e/expected/NAME.txt).
In the exceptions, `scenarios.helix` names the scenarios Helix cannot
run, `steps.helix` the steps it cannot take (`NAME STEP`), and
`snapshots.helix` what it shows otherwise on purpose (`NAME STEP KIND`),
each with the reason.

Each Helix has Org's colors (editors/helix/runtime) in its configuration's
runtime, as install.sh puts them. One more check, `highlight`, looks at
them: tests/e2e/highlight/sample.org and an agenda the server shows, in a
theme with a color for each scope of the queries.

  HX=/path/to/hx editors/helix/test/run.py

ORG_LSP_BIN is the server (default: the debug build); ORG_LSP_E2E=NAME
runs only the scenarios whose name has NAME in it. HELIX_RUNTIME defaults
to the `runtime` directory beside `hx` when there is one. ORG_LSP_HX_KEEP
keeps each scenario's directory, Helix's log among its files.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
E2E = os.path.join(ROOT, "tests", "e2e")
RUNTIME = os.path.join(ROOT, "editors", "helix", "runtime")
HX = os.environ.get("HX", "")
SERVER = os.environ.get("ORG_LSP_BIN", os.path.join(ROOT, "target", "debug", "org-lsp"))
ONLY = os.environ.get("ORG_LSP_E2E", "")
WIDTH, HEIGHT = 200, 50

# The kind of the code action that runs a scenario's command where no
# action has the command's own: `org.todo` without keys cycles the state
# in an Org file, as the action does.
ACTIONS = {
    "org.todo": "org.todo.next",
}
# Helix's own keys that run a command: go to definition follows a link
# (in an Org file; in the agenda it goes to the line's entry).
KEYS = {"org.open": ["g", "d"]}
# Helix's own commands that do what a command does: Helix saves its
# files itself, as the server cannot.
COMMAND_LINE = {"org.agenda.save": "write-all"}
# The commands that move the subtree at the cursor: Helix applies their
# code action's edit and leaves the cursor where it was in the text, and
# the user goes after the subtree (a search for its line), where Emacs's
# cursor goes with it.
MOVES = {"org.meta.up", "org.meta.down"}

# What the server says when it waits for an answer.
RENAME = ": answer with a rename at the cursor"
CHOICE = ": pick the answer from the code actions"
# The kind of the code actions that answer a question.
ANSWER = "org.answer"


def load(path):
    with open(path, encoding="utf-8") as file:
        return json.load(file)


class Failure(Exception):
    pass


def tmux(*args, check=True):
    return subprocess.run(["tmux", *args], check=check, capture_output=True, text=True).stdout


def toml_value(value):
    """`value` as TOML, inline tables for objects."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return json.dumps(value)
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{toml_key(key)} = {toml_value(item)}" for key, item in value.items()) + " }"
    raise ValueError(f"no TOML for {value!r}")


def toml_key(key):
    return key if re.fullmatch(r"[A-Za-z0-9_-]+", key) else json.dumps(key)


def substitute(value, directory):
    if isinstance(value, str):
        return value.replace("$DIR", directory)
    if isinstance(value, list):
        return [substitute(item, directory) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, directory) for key, item in value.items()}
    return value


def fixture_files(spec):
    """The scenario's files by name, as the server's runner writes them."""
    files = {}
    if "files_from" in spec:
        directory = os.path.join(ROOT, "tests", spec["files_from"])
        for name in os.listdir(directory):
            if name.endswith(".org") or name.endswith(".org_archive"):
                with open(os.path.join(directory, name), encoding="utf-8") as file:
                    files[name] = file.read()
    for name, text in spec.get("files", {}).items():
        files[name] = "".join(line + "\n" for line in text) if isinstance(text, list) else text
    return files


def options_of(spec, directory):
    """The server's options, as the server's runner gives them."""
    options = {
        "config_file": False,
        "clock": spec.get("clock", {"today": "2026-05-18", "now": "10:30"}),
        "org_agenda_files": [directory],
        "org_directory": directory,
        "org_default_notes_file": f"{directory}/notes.org",
    }
    for key, value in spec.get("options", {}).items():
        options[key] = substitute(value, directory)
    if spec.get("emacs"):
        config = options.get("emacs_config", "")
        if os.environ.get("ORG_LISP_DIR"):
            config = f"(add-to-list 'load-path {json.dumps(os.environ['ORG_LISP_DIR'])}) {config}"
        options["emacs_config"] = config
    return options


def languages_toml(options):
    """editors/helix/languages.toml, with the server's whole path and the
    scenario's options added to its `config`, as docs/helix.org sets it
    up."""
    with open(os.path.join(ROOT, "editors", "helix", "languages.toml"), encoding="utf-8") as file:
        text = file.read()
    for line in ('command = "org-lsp"', "[language-server.org-lsp.config]"):
        if line not in text:
            raise Failure(f"editors/helix/languages.toml has no `{line}`")
    text = text.replace('command = "org-lsp"', f"command = {json.dumps(SERVER)}", 1)
    table = "[language-server.org-lsp.config]"
    lines = "".join(f"\n{toml_key(key)} = {toml_value(value)}" for key, value in options.items())
    return text.replace(table, table + lines, 1)


def expected_snapshots(name):
    """The expected snapshots of a scenario, by `STEP KIND`."""
    path = os.path.join(E2E, "expected", f"{name}.txt")
    snapshots, key = {}, None
    with open(path, encoding="utf-8") as file:
        for line in file.read().splitlines(keepends=True):
            header = re.fullmatch(r"=== (\d+) (\S+)\n", line)
            if header:
                key = f"{header.group(1)} {header.group(2)}"
                snapshots[key] = ""
            elif key is not None:
                snapshots[key] += line
    return snapshots


def path_of(uri):
    return urllib.parse.unquote(urllib.parse.urlparse(uri).path)


def offset(text, position, encoding):
    """The index in `text` of an LSP position, its character counted in
    `encoding`."""
    lines = text.split("\n")
    line = position["line"]
    start = sum(len(each) + 1 for each in lines[:line])
    if line >= len(lines):
        return len(text)
    rest, count, at = lines[line], position["character"], 0
    for char in rest:
        if count <= 0:
            break
        count -= len(char.encode("utf-8")) if encoding == "utf-8" else \
            (2 if ord(char) > 0xFFFF else 1) if encoding == "utf-16" else 1
        at += 1
    return start + at


class Log:
    """Helix's log (`-v`): what it sent the server and what it was sent,
    and the text of each document it has open."""

    def __init__(self, path):
        self.path = path
        self.offset = 0
        self.partial = ""
        self.sent = []  # (index, message) Helix -> server
        self.received = []  # (index, message) server -> Helix
        self.count = 0
        self.documents = {}  # path -> text
        self.encoding = "utf-16"

    def read(self):
        try:
            with open(self.path, encoding="utf-8", errors="replace") as file:
                file.seek(self.offset)
                chunk = file.read()
                self.offset = file.tell()
        except FileNotFoundError:
            return
        lines = (self.partial + chunk).split("\n")
        self.partial = lines.pop()
        for line in lines:
            match = re.search(r"\] org-lsp (->|<-) (\{.*)$", line)
            if not match:
                continue
            try:
                message = json.loads(match.group(2))
            except json.JSONDecodeError:
                continue
            self.count += 1
            if match.group(1) == "->":
                self.sent.append((self.count, message))
                self.track(message)
            else:
                self.received.append((self.count, message))
                encoding = (message.get("result") or {}).get("capabilities", {}).get("positionEncoding") \
                    if isinstance(message.get("result"), dict) else None
                if encoding:
                    self.encoding = encoding

    def track(self, message):
        """The documents' text, as Helix tells the server of it."""
        method, params = message.get("method"), message.get("params") or {}
        if method == "textDocument/didOpen":
            self.documents[path_of(params["textDocument"]["uri"])] = params["textDocument"]["text"]
        elif method == "textDocument/didChange":
            path = path_of(params["textDocument"]["uri"])
            text = self.documents.get(path, "")
            for change in params["contentChanges"]:
                if "range" not in change:
                    text = change["text"]
                    continue
                start = offset(text, change["range"]["start"], self.encoding)
                end = offset(text, change["range"]["end"], self.encoding)
                text = text[:start] + change["text"] + text[end:]
            self.documents[path] = text
        elif method == "textDocument/didClose":
            self.documents.pop(path_of(params["textDocument"]["uri"]), None)

    def shown(self):
        """The documents the server asked Helix to show, in turn."""
        return [path_of(m["params"]["uri"]) for _, m in self.received if m.get("method") == "window/showDocument"]

    def pending(self):
        """Whether a request either side sent has no answer yet."""
        def unanswered(requests, answers):
            asked = {json.dumps(m["id"]) for _, m in requests if "method" in m and "id" in m}
            done = {json.dumps(m["id"]) for _, m in answers if "method" not in m and "id" in m}
            cancelled = {
                json.dumps(m["params"]["id"]) for _, m in requests if m.get("method") == "$/cancelRequest"
            }
            return asked - done - cancelled
        return unanswered(self.sent, self.received) or unanswered(self.received, self.sent)

    def response_to(self, method, after):
        """The answer to the last `method` request Helix sent after message
        number `after`, once it has come."""
        requests = [m for index, m in self.sent if index > after and m.get("method") == method and "id" in m]
        if not requests:
            return None
        for _, message in self.received:
            if "method" not in message and message.get("id") == requests[-1]["id"]:
                return message
        return None


class Helix:
    def __init__(self, name, spec):
        self.name = name
        self.spec = spec
        self.work = tempfile.mkdtemp(prefix=f"org-lsp-hx-{name}-")
        self.directory = os.path.realpath(os.path.join(self.work, "files"))
        os.makedirs(self.directory)
        for file, text in fixture_files(spec).items():
            path = os.path.join(self.directory, file)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as out:
                out.write(text)
        config = os.path.join(self.work, "config", "helix")
        os.makedirs(config)
        with open(os.path.join(config, "languages.toml"), "w", encoding="utf-8") as out:
            out.write(languages_toml(options_of(spec, self.directory)))
        # Org's colors, where install.sh puts them.
        shutil.copytree(RUNTIME, os.path.join(config, "runtime"))
        # A search finds the text as it is written, as the scenarios' `at`.
        with open(os.path.join(config, "config.toml"), "w", encoding="utf-8") as out:
            out.write("[editor.search]\nsmart-case = false\n")
        self.config = config
        # More of Helix's environment.
        self.environment = {}
        self.log = Log(os.path.join(self.work, "helix.log"))
        self.session = f"org-lsp-hx-{name}-{os.getpid()}"
        self.messages_from = 0
        # The messages that others took the place of, as ranges of the
        # log's messages.
        self.replaced = []
        self.asked_from = 0
        self.started = False

    def start(self, file):
        runtime = os.environ.get("HELIX_RUNTIME") or os.path.join(os.path.dirname(os.path.realpath(HX)), "runtime")
        environment = {
            # Not above the files: the server writes paths there with `~`.
            "HOME": os.path.join(self.work, "home"),
            "XDG_CONFIG_HOME": os.path.join(self.work, "config"),
            "XDG_CACHE_HOME": os.path.join(self.work, "cache"),
            "XDG_STATE_HOME": os.path.join(self.work, "state"),
            "XDG_DATA_HOME": os.path.join(self.work, "data"),
            "ORG_LSP_CONFIG": "/dev/null",
            "TERM": "xterm-256color",
            **self.environment,
        }
        if os.path.isdir(runtime):
            environment["HELIX_RUNTIME"] = runtime
        command = ["env", *[f"{key}={value}" for key, value in environment.items()],
                   HX, "-v", "--log", self.log.path, file]
        tmux("new-session", "-d", "-s", self.session, "-x", str(WIDTH), "-y", str(HEIGHT),
             "-c", self.directory, " ".join(shell_quote(part) for part in command))
        self.started = True
        self.wait(lambda: any(m.get("method") == "textDocument/didOpen" for _, m in self.log.sent),
                  "the server to start", read=True)
        self.settle()

    def stop(self):
        if self.started:
            # Helix quits, and the server with it, which removes the
            # agenda's file.
            self.keys("Escape")
            self.keys(":")
            self.type("quit-all!")
            self.keys("Enter")
            end = time.monotonic() + 5
            while time.monotonic() < end and self.session in tmux("list-sessions", "-F", "#S", check=False).split():
                time.sleep(0.1)
            tmux("kill-session", "-t", self.session, check=False)
        if not os.environ.get("ORG_LSP_HX_KEEP"):
            shutil.rmtree(self.work, ignore_errors=True)

    # The screen.

    def screen(self, escapes=False):
        args = ["capture-pane", "-p", "-t", self.session]
        if escapes:
            args.insert(1, "-e")
        return tmux(*args).split("\n")

    def status(self):
        """The mode, the buffer's path and the cursor's line and column
        (from 1) in the status line."""
        for row in reversed(self.screen()):
            # Side by side, a window's status line ends at `│`; only the
            # focused window's shows the mode.
            for part in row.split("│"):
                match = re.match(r"\s*(NOR|INS|SEL)\s+(\S+)(?:\s+\[\+\])?.*?\s(\d+):(\d+)\s*$", part)
                if match:
                    break
            if match:
                name = os.path.join(self.directory, os.path.expanduser(match.group(2)))
                return match.group(1), name, int(match.group(3)), int(match.group(4))
        raise Failure("no status line on the screen:\n" + "\n".join(self.screen()))

    def name_of(self, path):
        """A buffer's name as the scenarios write it: the agenda's as
        `agenda`, a file's from the scenario's directory."""
        if path.startswith(self.directory + "/"):
            return path[len(self.directory) + 1:]
        if os.path.basename(path) == "Agenda.org":
            return "agenda"
        return os.path.basename(path)

    def agenda(self):
        """The agenda's file, while Helix has it open."""
        for path in reversed(self.log.shown()):
            if os.path.basename(path) == "Agenda.org" and not path.startswith(self.directory + "/"):
                return path if path in self.log.documents else None
        return None

    def keys(self, *keys):
        tmux("send-keys", "-t", self.session, *keys)

    def escape(self):
        """Back to normal mode. Alone: a key right after Escape reads as
        that key with Alt."""
        self.keys("Escape")
        self.settle()

    def type(self, text):
        tmux("send-keys", "-t", self.session, "-l", "--", text)

    def wait(self, done, what, timeout=15, read=False):
        start = time.monotonic()
        while True:
            if read:
                self.log.read()
            if done():
                return
            if time.monotonic() - start > timeout:
                raise Failure(f"waited {timeout}s for {what}; Helix shows:\n" + "\n".join(self.screen()).rstrip())
            time.sleep(0.05)

    def settle(self):
        """Wait until every request either side sent is answered and the
        log and the screen have been still for a moment."""
        start = time.monotonic()
        last, still = None, time.monotonic()
        while True:
            self.log.read()
            now = (self.log.count, "\n".join(self.screen()))
            if now != last:
                last, still = now, time.monotonic()
            elif not self.log.pending() and time.monotonic() - still > 0.25:
                return
            if time.monotonic() - start > 20:
                raise Failure("Helix and the server did not settle in 20s")
            time.sleep(0.05)

    def command_line(self, text):
        self.escape()
        self.keys(":")
        self.type(text)
        self.keys("Enter")
        self.settle()

    def text(self, path):
        """A file as Helix has it, or else as it is on disk."""
        if path in self.log.documents:
            return self.log.documents[path]
        try:
            with open(path, encoding="utf-8") as file:
                return file.read()
        except FileNotFoundError:
            return None

    # What a user does.

    def open(self, path):
        """`:open` a file, or go to its buffer if it has one."""
        self.command_line(f"open {shell_quote(path)}")

    def go_to_buffer(self, path):
        """Go back to a buffer from the buffer picker (`Space b`), where
        its cursor was: `:open` would put it at the start."""
        self.escape()
        self.keys("Space", "b")
        self.wait(lambda: any(os.path.basename(path) in row for row in self.screen()), "the buffer picker")
        self.type(os.path.basename(path))
        time.sleep(0.3)
        self.settle()
        self.keys("Enter")
        self.settle()
        if self.status()[1] != path:
            raise Failure(f"the buffer picker went to {self.status()[1]}, not {path}")

    def place(self, step):
        column = step.get("column", 0)
        if "line" in step:
            self.command_line(f"goto {step['line']}")
            start = 0
        elif "at" in step:
            # Searched for from the buffer's start, as the first place it
            # is in the buffer.
            self.escape()
            self.keys("g", "g", "/")
            self.type(re.escape(step["at"]))
            self.keys("Enter")
            self.settle()
            _, path, line, end = self.status()
            # The search selects the match, the cursor on its last character.
            start = end - len(step["at"])
            lines = (self.text(path) or "").split("\n")
            first = next((number for number, text in enumerate(lines, start=1) if step["at"] in text), None)
            if first is None:
                raise Failure(f"`{step['at']}` is not in the buffer")
            if first != line or lines[line - 1][start:].find(step["at"]) != 0:
                raise Failure(f"the search for `{step['at']}` went to {line}:{end}, not to line {first}")
        else:
            return
        self.type(";")
        self.keys("g", "h")
        if start + column:
            self.keys(str(start + column), "l")
        self.settle()

    def type_text(self, text):
        self.keys("i")
        self.type(text)
        self.settle()
        for _ in range(3):
            self.escape()
            if self.status()[0] == "NOR":
                return
        raise Failure("Helix stays in insert mode")

    def menu_rows(self):
        """The rows of the screen, each as its text and which of its
        characters are in reverse video, as the menu shows its selection."""
        rows = []
        for row in self.screen(escapes=True):
            text, reverse, on = "", [], False
            for part in re.split(r"(\x1b\[[0-9;?]*[A-Za-z])", row):
                if part.startswith("\x1b["):
                    if part.endswith("m"):
                        codes = (part[2:-1] or "0").split(";")
                        while codes:
                            code = codes.pop(0)
                            if code in ("38", "48", "58"):
                                # A colour: 5;N or 2;R;G;B follow.
                                del codes[:1 if not codes else (2 if codes[0] == "5" else 4)]
                            elif code in ("0", "", "27"):
                                on = False
                            elif code == "7":
                                on = True
                    continue
                text += part
                reverse += [on] * len(part)
            rows.append((text, reverse))
        return rows

    def colored_rows(self):
        """The rows of the screen, each as its text and the color of each
        of its characters (`(r, g, b)`, or None)."""
        rows = []
        for row in self.screen(escapes=True):
            text, colors, color = "", [], None
            for part in re.split(r"(\x1b\[[0-9;?]*[A-Za-z])", row):
                if part.startswith("\x1b["):
                    if part.endswith("m"):
                        codes = (part[2:-1] or "0").split(";")
                        while codes:
                            code = codes.pop(0)
                            if code == "38" and codes and codes[0] == "2":
                                color = tuple(int(each) for each in codes[1:4])
                                del codes[:4]
                            elif code in ("48", "58"):
                                del codes[:1 if not codes else (2 if codes[0] == "5" else 4)]
                            elif code in ("0", "", "39"):
                                color = None
                    continue
                text += part
                colors += [color] * len(part)
            rows.append((text, colors))
        return rows

    def code_actions(self):
        """Open the code action menu (`Space a`): the actions it shows."""
        self.escape()
        after = self.log.count
        self.keys("Space", "a")
        self.wait(lambda: self.log.response_to("textDocument/codeAction", after) is not None,
                  "the code actions", read=True)
        return self.log.response_to("textDocument/codeAction", after).get("result") or []

    def pick_action(self, actions, index):
        """Pick the action at `index` in the open code action menu."""
        titles = [action["title"] for action in actions]
        title = titles[index]
        # The longest first: a title may start with another.
        by_length = sorted(set(titles), key=len, reverse=True)

        def selected():
            # The whole title in reverse video: the cursor is one
            # character of it.
            for text, reverse in self.menu_rows():
                for each in by_length:
                    at = text.find(f" {each} ")
                    if at >= 0:
                        if all(reverse[at + 1:at + 1 + len(each)]):
                            return each
                        break
            return None

        def shown():
            return any(each in text for text, _ in self.menu_rows() for each in titles)

        if titles.count(title) > 1:
            raise Failure(f"two code actions are titled `{title}`")
        # The menu may open with no action selected: `C-n` goes down it.
        self.wait(shown, "the code action menu")
        for _ in range(len(titles) + 2):
            current = selected()
            if current == title:
                break
            self.keys("C-n")
            self.wait(lambda: selected() not in (None, current), "the menu to go down")
        else:
            raise Failure(f"could not select `{title}` in the menu")
        self.keys("Enter")
        self.settle()

    def code_action(self, command):
        """Run `command` from the code action menu, by its action's kind or
        command."""
        actions = self.code_actions()

        def find(kind):
            return next((at for at, action in enumerate(actions)
                         if action.get("kind") == kind or (action.get("command") or {}).get("command") == kind), None)
        kind = command if find(command) is not None else ACTIONS.get(command, command)
        index = find(kind)
        if index is None:
            self.keys("Escape")
            raise Failure(f"no code action {kind} here; Helix offers: {[action['title'] for action in actions]}")
        self.pick_action(actions, index)

    def question(self):
        """What the server asked since the last answer: `choice`, `text`
        or None."""
        for index, message in reversed(self.log.received):
            if index <= self.asked_from:
                break
            if message.get("method") == "window/showMessage":
                text = message.get("params", {}).get("message", "")
                if text.endswith(CHOICE):
                    return "choice"
                if text.endswith(RENAME):
                    return "text"
        return None

    def answer(self, answer, command):
        """Answer the server's question: a choice from the code actions,
        text with a rename at the cursor."""
        asked = self.question()
        if asked is None:
            raise Failure(f"{command} did not ask for {answer!r}")
        self.asked_from = self.log.count
        if asked == "choice":
            actions = self.code_actions()
            choices = [action for action in actions if action.get("kind") == ANSWER]
            index = pick([action["title"] for action in choices], answer)
            if index is None:
                self.keys("Escape")
                raise Failure(f"no choice {json.dumps(answer)} in {[action['title'] for action in choices]}")
            self.pick_action(actions, actions.index(choices[index]))
            return
        if not isinstance(answer, str):
            raise Failure(f"{command} asks for text, not {json.dumps(answer)}")
        self.keys("Space", "r")
        self.wait(lambda: any("rename-to:" in row for row in self.screen()), "the rename prompt")
        self.keys("C-u")
        self.type(answer)
        self.keys("Enter")
        self.settle()

    def run(self, step, excepted):
        command = step["command"]
        self.asked_from = self.log.count
        started = self.log.count
        answers = list(step.get("answers", []))
        if step.get("below"):
            # The code action adds where the cursor is, as M-RET does: to
            # add below the line, the cursor goes past its end first
            # (`gl`, then `l` onto the line's end), as `C-e M-RET` does.
            self.escape()
            self.keys("g", "l")
            self.keys("l")
            self.settle()
        # A code action takes no count: it is picked that many times, or
        # once for a command Org gives the count no meaning (`COMMAND
        # count` in the exceptions).
        times = 1 if f"{command} count" in excepted else step.get("count", 1)
        if command in MOVES:
            _, path, line, _ = self.status()
            moved = ((self.text(path) or "").split("\n") + [""])[line - 1]
        first = self.log.count
        for time_ in range(times):
            if time_ > 0:
                # What the action taken again says takes the place of what
                # it said before, as in Emacs's echo area for one key with
                # a count.
                self.log.read()
                self.replaced.append((first, self.log.count))
            if command in KEYS and self.name_of(self.status()[1]) != "agenda":
                self.escape()
                self.keys(*KEYS[command])
                self.settle()
            elif command in COMMAND_LINE:
                self.command_line(COMMAND_LINE[command])
            else:
                self.code_action(command)
        if command in MOVES:
            _, path, line, _ = self.status()
            if ((self.text(path) or "").split("\n") + [""])[line - 1] != moved:
                self.place({"at": moved})
        if command == "org.agenda.save":
            # Each Org file as Helix has it is on disk.
            self.wait(self.saved, "Helix to save the Org files", read=True)
        for answer in answers:
            self.answer(answer, command)
        # Helix shows a document the server shows without the focus (an
        # entry in follow mode) in a split to the right, and focuses it:
        # the user goes back to the window on the left.
        unfocused = [m for index, m in self.log.received if index > started
                     and m.get("method") == "window/showDocument" and not m["params"].get("takeFocus")]
        if unfocused:
            self.escape()
            self.keys("C-w", "h")
            self.settle()

    def saved(self):
        for path, text in self.log.documents.items():
            if path.startswith(self.directory + "/") and os.path.exists(path):
                with open(path, encoding="utf-8") as file:
                    if file.read() != text:
                        return False
        return True

    # What is looked at.

    def messages(self):
        """What the server said since the messages were last looked at,
        without its questions (the code actions and a rename answer
        them)."""
        said = []
        for index, message in self.log.received:
            if index <= self.messages_from or any(start < index <= end for start, end in self.replaced):
                continue
            if message.get("method") == "window/showMessage":
                text = message["params"]["message"]
                if not text.endswith(RENAME) and not text.endswith(CHOICE):
                    said.append(text)
            elif "error" in message and "method" not in message:
                said.append(message["error"].get("message", ""))
        self.messages_from = self.log.count
        return "".join(f"{text}\n" for text in said)

    def snapshot(self, kind):
        ended = lambda text: text.rstrip("\n") + "\n"
        self.log.read()
        _, path, line, _ = self.status()
        if kind == "buffer":
            return f"[{self.name_of(path)}]\n" + ended(self.text(path) or "")
        if kind == "cursor":
            lines = (self.text(path) or "").split("\n")
            return f"{self.name_of(path)}:{line}: {lines[line - 1] if line - 1 < len(lines) else ''}\n"
        if kind == "messages":
            return self.messages()
        if kind.startswith("file:"):
            text = self.text(os.path.join(self.directory, kind[len("file:"):]))
            return "(no file)\n" if text is None else ended(text)
        if kind == "agenda":
            agenda = self.agenda()
            return ended(self.log.documents[agenda]) if agenda else "(no agenda)\n"
        raise Failure(f"Helix cannot show {kind}")


def pick(titles, answer):
    """The choice an answer picks: a key (`[t]` at the end of its title),
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


def shell_quote(text):
    return "'" + text.replace("'", "'\\''") + "'" if re.search(r"[^\w@%+=:,./-]", text) else text


def run_scenario(name, spec, exceptions):
    excepted = exceptions.get("editors", {}).get("helix", {})
    differences = exceptions.get("snapshots", {}).get("helix", {})
    skipped = exceptions.get("steps", {}).get("helix", {})
    expected = expected_snapshots(name)
    steps = spec["steps"]
    helix = Helix(name, spec)
    # What differs, the scenario going on past a snapshot that does.
    differ = []
    try:
        first = steps[0].get("open") if steps else None
        if not first:
            raise Failure("the scenario opens no file first")
        helix.start(first)
        for number, step in enumerate(steps, start=1):
            if f"{name} {number}" in skipped:
                # Where the step puts the cursor, which the steps after it
                # need, without what Helix cannot do there.
                if step.get("focus") == "agenda" and helix.agenda():
                    helix.go_to_buffer(helix.agenda())
                elif "focus" in step:
                    helix.open(step["focus"])
                helix.place(step)
                continue
            try:
                if "open" in step and number > 1:
                    helix.open(step["open"])
                if step.get("focus") == "agenda":
                    agenda = helix.agenda()
                    if not agenda:
                        raise Failure("no agenda to go to")
                    helix.go_to_buffer(agenda)
                elif "focus" in step:
                    helix.open(step["focus"])
                if step.get("close"):
                    # The agenda's buffer, changed by the server's edits.
                    helix.command_line("buffer-close!")
                helix.place(step)
                if "type" in step:
                    helix.type_text(step["type"])
                if "command" in step:
                    helix.run(step, excepted)
                for kind in step.get("expect", []):
                    key = f"{number} {kind}"
                    want = expected.get(key)
                    if want is None:
                        raise Failure(f"no expected snapshot `{key}`")
                    actual = helix.snapshot(kind).replace(helix.directory, "$DIR")
                    if f"{name} {key}" in differences:
                        # An exception for what is the same is out of date.
                        if actual == want:
                            differ.append(f"step {number}: {kind} is excepted for Helix, but the same")
                        continue
                    if actual != want:
                        differ.append(f"step {number}: {kind} differs:\n--- expected\n{want}--- Helix\n{actual}")
            except Failure as failure:
                differ.append(f"step {number}: {failure}")
                break
    finally:
        helix.stop()
    if differ:
        raise Failure("\n".join(differ))


# The colors of Org's queries (editors/helix/runtime): on the screen, in a
# theme with a color of its own for each scope, the words of a row (found
# by a part of it) in the scope they should have. Spaces are left out, as
# Helix colors no space between two words.
SAMPLE_COLORS = [
    ("#+TITLE:", "Highlighting sample", "markup.heading.title"),
    ("#+SUBTITLE:", "#+SUBTITLE:", "comment.keyword"),
    ("#+SUBTITLE:", "Every construct the server colours", "string.info"),
    ("level one", "* Headline level one", "markup.heading.1"),
    ("level two", "** Headline level two", "markup.heading.2"),
    ("level three", "*** Headline level three", "markup.heading.3"),
    ("level six", "****** Headline level six", "markup.heading.6"),
    ("level eight", "******** Headline level eight", "markup.heading.8"),
    ("A task to do", "TODO", "diff.minus.todo"),
    ("A task to do", "[#A]", "constant.numeric.priority"),
    ("A task to do", "A task to do", "markup.heading.1"),
    ("A task to do", ":work:urgent:", "tag"),
    ("SCHEDULED:", "SCHEDULED:", "keyword.planning"),
    ("SCHEDULED:", "<2026-05-20 Wed>", "constant.numeric.timestamp"),
    ("A finished task", "DONE", "diff.plus.done"),
    ("A finished task", "A finished task", "comment.done"),
    ("CLOSED:", "[2026-05-17 Sun 09:00]", "comment.timestamp"),
    ("An archived headline", "An archived headline", "comment.archived"),
    (":PROPERTIES:", ":PROPERTIES:", "function.drawer"),
    (":CUSTOM_ID:", ":CUSTOM_ID:", "variable.other.member.property"),
    (":LOGBOOK:", ":LOGBOOK:", "function.drawer"),
    ("CLOCK:", "CLOCK:", "keyword.planning"),
    ("CLOCK:", "[2026-05-17 Sun 10:00]", "comment.timestamp"),
    ("* Lists", "[1/3]", "constant.numeric.cookie"),
    ("Complete lists", "[2/2]", "diff.plus.cookie"),
    ("A ticked item", "-", "markup.list"),
    ("A ticked item", "[X]", "markup.list.checked"),
    ("A partly ticked item", "[-]", "markup.list.unchecked"),
    ("its description", "term", "markup.bold.term"),
    ("Text in", "*bold*", "markup.bold"),
    ("Text in", "/italic/", "markup.italic"),
    ("Text in", "_underline_", "markup.underline"),
    ("Text in", "+strike-through+", "markup.strikethrough"),
    ("Text in", "=verbatim=", "markup.raw.inline.verbatim"),
    ("Text in", "~code~", "markup.raw.inline.code"),
    ("Nested:", "*bold with", "markup.bold"),
    ("Nested:", "/italic/", "markup.italic"),
    ("Nested:", "inside*", "markup.bold"),
    ("described link", "[[https://orgmode.org][described link]]", "markup.link.url"),
    ("described link", "https://example.org", "markup.link.url"),
    ("<<target>>", "<<target>>", "label.target"),
    ("<<target>>", "[fn:1]", "markup.link.url.footnote"),
    ("<<target>>", "{{{greet(world)}}}", "function.macro"),
    ("Entities", "\\alpha", "constant.character.escape.entity"),
    ("Entities", "$x^2$", "markup.raw.inline.latex"),
    ("Inline src", "<2026-05-19 Tue>", "constant.numeric.timestamp"),
    ("Inline src", "[2026-05-19 Tue]", "comment.timestamp"),
    ("# A comment line", "# A comment line", "comment"),
    ("#+begin_src", "#+begin_src python :results output", "comment.block"),
    # The code of the block in Python's colors.
    ('"source code"', '"source code"', "string"),
    ("Quoted text.", "Quoted text.", "markup.quote"),
    ("Commented out.", "Commented out.", "comment"),
    ("\\begin{equation}", "\\begin{equation}", "markup.raw.block"),
    ("|------+-------|", "|------+-------|", "punctuation.special.table"),
    ("| Name | Value |", "Name", "markup.heading.table"),
    ("| Name | Value |", "Value", "markup.heading.table"),
    # The rows below the rule in no color.
    ("| a    |", "a", None),
    (": Fixed-width text", ": Fixed-width text", "markup.raw.inline.fixed"),
    ("An inline task", "An inline task", "comment.inlinetask"),
    ("The footnote's text.", "[fn:1]", "markup.link.url.footnote"),
]

# The agenda of HIGHLIGHT_AGENDA on 2026-05-18 at 10:30, in the language
# `org-agenda` that editors/helix/languages.toml gives the agenda's file.
HIGHLIGHT_AGENDA = [
    "* TODO Today",
    "SCHEDULED: <2026-05-18 Mon>",
    "* TODO [#A] Earlier :work:",
    "SCHEDULED: <2026-05-15 Fri>",
    "* TODO Soon",
    "DEADLINE: <2026-05-21 Thu>",
    "* TODO Late",
    "DEADLINE: <2026-05-16 Sat>",
    "* DONE Finished",
    "SCHEDULED: <2026-05-18 Mon>",
    "* TODO Timed",
    "SCHEDULED: <2026-05-18 Mon 11:00>",
]
AGENDA_COLORS = [
    ("Week-agenda", "Week-agenda (W21):", "markup.heading.agenda"),
    ("Monday", "Monday 18 May 2026 W21", "markup.heading.date"),
    ("Tuesday", "Tuesday 19 May 2026", "markup.heading.date"),
    ("Saturday", "Saturday 23 May 2026", "markup.italic.weekend"),
    (" 8:00......", "8:00...... ----------------", "comment.timegrid"),
    ("now - -", "10:30...... now - -", "markup.bold.now"),
    ("Timed", "agenda: 11:00...... Scheduled:", "diff.plus.scheduled"),
    ("Today", "agenda: Scheduled:", "diff.plus.scheduled"),
    ("Today", "TODO", "diff.minus.todo"),
    ("Today", "Today", "diff.plus.scheduled"),
    ("Earlier", "Sched. 3x:", "diff.delta.scheduled"),
    ("Earlier", "[#A]", "constant.numeric.priority"),
    ("Earlier", ":work:", "tag"),
    ("In   3 d.:", "agenda: In 3 d.:", "diff.delta.deadline"),
    ("Late", "2 d. ago:", "diff.minus.deadline"),
    ("Finished", "agenda: Scheduled:", "comment.done"),
    ("Finished", "DONE", "diff.plus.done"),
]


def scope_colors(config):
    """A theme with a color for each scope of the queries (and Python's
    strings), set as Helix's theme: the colors by scope."""
    names = {"string"}
    for directory, _, files in os.walk(RUNTIME):
        for file in files:
            with open(os.path.join(directory, file), encoding="utf-8") as text:
                names |= {name for name in re.findall(r"@([a-z][a-z0-9_.]*)", text.read())
                          if not name.startswith("injection")}
    colors = {name: (1, 2 + index // 200, 1 + index % 200) for index, name in enumerate(sorted(names))}
    os.makedirs(os.path.join(config, "themes"))
    with open(os.path.join(config, "themes", "org-lsp-scopes.toml"), "w", encoding="utf-8") as out:
        out.write('"ui.background" = { bg = "#000000" }\n"ui.text" = "#fefefe"\n')
        out.write('"ui.selection" = { bg = "#202020" }\n"ui.cursor" = { bg = "#303030" }\n')
        # The code action menu shows its selection in reverse video.
        out.write('"ui.menu.selected" = { modifiers = ["reversed"] }\n')
        for name, color in colors.items():
            out.write(f'"{name}" = "#{color[0]:02x}{color[1]:02x}{color[2]:02x}"\n')
    with open(os.path.join(config, "config.toml"), "a", encoding="utf-8") as out:
        out.write('theme = "org-lsp-scopes"\n')
    # `theme` goes before the tables of config.toml.
    with open(os.path.join(config, "config.toml"), encoding="utf-8") as file:
        lines = file.read().splitlines()
    with open(os.path.join(config, "config.toml"), "w", encoding="utf-8") as out:
        out.write("\n".join([lines[-1], *lines[:-1]]) + "\n")
    return {color: name for name, color in colors.items()}


def check_colors(helix, scopes, expected):
    """What of `expected` the screen does not show."""
    rows = helix.colored_rows()
    wrong = []
    for part, words, scope in expected:
        found = [(text, colors) for text, colors in rows if part in text]
        if len(found) != 1:
            wrong.append(f"{len(found)} rows with `{part}` on the screen")
            continue
        text, colors = found[0]
        # The words, however many spaces apart.
        match = re.search(r"\s+".join(re.escape(word) for word in words.split()), text)
        if not match:
            wrong.append(f"no `{words}` in `{text.strip()}`")
            continue
        shown = {scopes.get(colors[at]) for at in range(match.start(), match.end()) if not text[at].isspace()}
        if shown != {scope}:
            wrong.append(f"`{words}` is {', '.join(sorted(str(each) for each in shown))}, not {scope}")
    return wrong


def run_highlight():
    """Org's colors in an Org file and in the agenda."""
    with open(os.path.join(E2E, "highlight", "sample.org"), encoding="utf-8") as file:
        sample = file.read()
    helix = Helix("highlight", {"files": {"sample.org": sample, "agenda.org": HIGHLIGHT_AGENDA}})
    scopes = scope_colors(helix.config)
    # The theme's colors as they are.
    helix.environment["COLORTERM"] = "truecolor"
    wrong = []
    try:
        helix.start("sample.org")
        # The file's start, then its end.
        pending = list(SAMPLE_COLORS)
        for line in (None, len(sample.splitlines())):
            if line:
                helix.command_line(f"goto {line}")
            rows = helix.colored_rows()
            here = [each for each in pending if any(each[0] in text for text, _ in rows)]
            wrong += check_colors(helix, scopes, here)
            pending = [each for each in pending if each not in here]
        wrong += [f"`{part}` was not on the screen" for part, _, _ in pending]
        helix.run({"command": "org.agenda.dispatch", "answers": ["a"]}, {})
        if not helix.agenda():
            raise Failure("no agenda")
        wrong += check_colors(helix, scopes, AGENDA_COLORS)
    finally:
        helix.stop()
    if wrong:
        raise Failure("\n".join(wrong))


def main():
    if not HX:
        sys.exit("set HX to the hx to run")
    if not os.path.exists(SERVER):
        sys.exit(f"no server at {SERVER}: build it (cargo build)")
    exceptions = load(os.path.join(E2E, "exceptions.json"))
    skipped = exceptions.get("scenarios", {}).get("helix", {})
    names = sorted(f[:-5] for f in os.listdir(os.path.join(E2E, "scenarios")) if f.endswith(".json"))
    failed = 0
    began = time.monotonic()
    for name in names:
        if ONLY not in name:
            continue
        if name in skipped:
            print(f"skip - {name}: {skipped[name]}")
            continue
        spec = load(os.path.join(E2E, "scenarios", f"{name}.json"))
        start = time.monotonic()
        try:
            run_scenario(name, spec, exceptions)
            print(f"ok - {name} ({time.monotonic() - start:.0f}s)")
        except Failure as failure:
            failed += 1
            print(f"not ok - {name}: {failure}")
        sys.stdout.flush()
    if ONLY in "highlight":
        start = time.monotonic()
        try:
            run_highlight()
            print(f"ok - highlight ({time.monotonic() - start:.0f}s)")
        except Failure as failure:
            failed += 1
            print(f"not ok - highlight: {failure}")
    print(f"helix: {failed} failed, in {time.monotonic() - began:.0f}s")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
