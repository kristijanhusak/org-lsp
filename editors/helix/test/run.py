#!/usr/bin/env python3
"""The shared end-to-end scenarios (tests/e2e/scenarios/*.json) in Helix.

Each scenario runs in a Helix of its own, in a tmux session, with the
repository's languages.toml (editors/helix/languages.toml) pointing at the
server and the scenario's options as its `config`, which Helix passes as
the initialization options. The steps are what a Helix user does: the
code action menu (`Space a`, then the action picked with `C-n` and Enter)
for the commands the server offers there, `gd` for following a link, and
for the others, which tests/e2e/exceptions.json names under
`editors.helix` with the reason, `:lsp-workspace-command` with the
cursor's place as its argument. What the server asks for as text is
answered with a rename at the cursor (`Space r`), as docs/any-editor.org
says. Helix gives a code action no count: a step with one runs its
command with `:lsp-workspace-command` too, the count in its argument.

What is looked at: files as Helix writes them (`:write-all`), the cursor
from the status line, and the messages Helix was sent
(`window/showMessage`, from its log), which it shows in the status line in
turn. What a scenario expects is what the server's own runner saw
(server/org-lsp/tests/e2e_scenarios.rs, tests/e2e/expected/NAME.txt).
In the exceptions, `scenarios.helix` names the scenarios Helix cannot
run, `steps.helix` the steps it cannot take (`NAME STEP`), and
`snapshots.helix` what it shows otherwise on purpose (`NAME STEP KIND`),
each with the reason.

  HX=/path/to/hx editors/helix/test/run.py

ORG_LSP_BIN is the server (default: the debug build); ORG_LSP_E2E=NAME
runs only the scenarios whose name has NAME in it. HELIX_RUNTIME defaults
to the `runtime` directory beside `hx` when there is one.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
E2E = os.path.join(ROOT, "tests", "e2e")
HX = os.environ.get("HX", "")
SERVER = os.environ.get("ORG_LSP_BIN", os.path.join(ROOT, "target", "debug", "org-lsp"))
ONLY = os.environ.get("ORG_LSP_E2E", "")
WIDTH, HEIGHT = 200, 50

# The commands Helix offers as code actions, by the action's kind; the
# scenario's command when it has no other name there. `org.todo` without
# keys cycles the state, as the action does.
ACTIONS = {
    "org.todo": "org.todo.next",
}
OFFERED = {
    "org.todo.next", "org.todo.previous", "org.priority.up", "org.priority.down",
    "org.timestamp.up", "org.timestamp.down", "org.date.later", "org.date.earlier",
    "org.clock.in", "org.clock.out", "org.clock.cancel", "org.checkbox.toggle",
    "org.table.recalculate", "org.meta.left", "org.meta.right", "org.shiftmeta.left",
    "org.shiftmeta.right", "org.meta.up", "org.meta.down", "org.shiftmeta.up",
    "org.shiftmeta.down", "org.table.insert.row.below", "org.table.hline",
    "org.table.hline.above", "org.table.blank.field", "org.table.toggle.column.width",
    "org.babel.tangle", "org.babel.execute", "org.edit.special", "org.archive.subtree",
    "org.archive.sibling", "org.archive.tag",
}
# Helix's own keys that run a command: go to definition follows a link.
KEYS = {"org.open": ["g", "d"]}

PROMPT = ": answer with a rename at the cursor"


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
    scenario's options as its `config`, as docs/helix.org sets it up."""
    with open(os.path.join(ROOT, "editors", "helix", "languages.toml"), encoding="utf-8") as file:
        text = file.read()
    line = 'command = "org-lsp"'
    if line not in text:
        raise Failure("editors/helix/languages.toml has no `command = \"org-lsp\"`")
    return text.replace(line, f"command = {json.dumps(SERVER)}\nconfig = {toml_value(options)}", 1)


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


class Log:
    """Helix's log (`-v`): what it sent the server and what it was sent."""

    def __init__(self, path):
        self.path = path
        self.offset = 0
        self.partial = ""
        self.sent = []  # (index, message) Helix -> server
        self.received = []  # (index, message) server -> Helix
        self.count = 0

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
            (self.sent if match.group(1) == "->" else self.received).append((self.count, message))

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
        # A search finds the text as it is written, as the scenarios' `at`.
        with open(os.path.join(config, "config.toml"), "w", encoding="utf-8") as out:
            out.write("[editor.search]\nsmart-case = false\n")
        self.log = Log(os.path.join(self.work, "helix.log"))
        self.session = f"org-lsp-hx-{name}-{os.getpid()}"
        self.messages_from = 0
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
            "ORG_LSP_CONFIG": "/nonexistent/config.toml",
            "TERM": "xterm-256color",
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
            tmux("kill-session", "-t", self.session, check=False)
        shutil.rmtree(self.work, ignore_errors=True)

    # The screen.

    def screen(self, escapes=False):
        args = ["capture-pane", "-p", "-t", self.session]
        if escapes:
            args.insert(1, "-e")
        return tmux(*args).split("\n")

    def status(self):
        """The mode, the buffer's name and the cursor's line and column
        (from 1) in the status line."""
        for row in reversed(self.screen()):
            match = re.match(r"\s*(NOR|INS|SEL)\s+(\S+)(?:\s+\[\+\])?.*?\s(\d+):(\d+)\s*$", row)
            if match:
                return match.group(1), match.group(2), int(match.group(3)), int(match.group(4))
        raise Failure("no status line on the screen:\n" + "\n".join(self.screen()))

    def keys(self, *keys):
        tmux("send-keys", "-t", self.session, *keys)

    def escape(self):
        """Back to normal mode. Alone: a key right after Escape reads as
        that key with Alt."""
        self.keys("Escape")
        self.settle()

    def type(self, text):
        tmux("send-keys", "-t", self.session, "-l", text)

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

    # What a user does.

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
            _, _, line, end = self.status()
            # The search selects the match, the cursor on its last character.
            start = end - len(step["at"])
            self.write_all()
            lines = (self.read(self.status()[1]) or "").split("\n")
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

    def code_action(self, kind):
        """Pick the code action of `kind` in the menu (`Space a`)."""
        self.escape()
        after = self.log.count
        self.keys("Space", "a")
        self.wait(lambda: self.log.response_to("textDocument/codeAction", after) is not None,
                  "the code actions", read=True)
        actions = self.log.response_to("textDocument/codeAction", after).get("result") or []
        titles = [action["title"] for action in actions]
        wanted = [action["title"] for action in actions if action.get("kind") == kind]
        if not wanted:
            self.keys("Escape")
            raise Failure(f"no code action {kind} here; Helix offers: {titles}")
        title = wanted[0]

        def selected():
            for text, reverse in self.menu_rows():
                for each in titles:
                    at = text.find(f" {each} ")
                    if at >= 0 and reverse[at + 1]:
                        return each
            return None

        def shown():
            return any(each in text for text, _ in self.menu_rows() for each in titles)

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

    def workspace_command(self, command, step):
        """`:lsp-workspace-command`, with the cursor's place as its argument
        and the server applying the edit itself (`apply`)."""
        _, _, line, column = self.status()
        context = {
            "textDocument": {"uri": f"file://{self.directory}/{self.status()[1]}"},
            "position": {"line": line - 1, "character": column - 1},
            "apply": True,
        }
        if "count" in step:
            context["count"] = step["count"]
        if step.get("below"):
            context["below"] = True
        self.command_line(f"lsp-workspace-command {command} {json.dumps(context, separators=(',', ':'))}")

    def answer(self, answer, command):
        """Answer the server's question with a rename at the cursor."""
        def asked():
            return any(PROMPT in m.get("params", {}).get("message", "") for i, m in self.log.received
                       if i > self.asked_from and m.get("method") == "window/showMessage")
        if not asked():
            raise Failure(f"{command} did not ask for {answer!r} with a rename")
        if not isinstance(answer, str):
            raise Failure(f"{command} asks for {answer!r}, which is no text: Helix has no list for a server")
        self.asked_from = self.log.count
        self.keys("Space", "r")
        self.wait(lambda: any("rename-to:" in row for row in self.screen()), "the rename prompt")
        self.keys("C-u")
        self.type(answer)
        self.keys("Enter")
        self.settle()

    def run(self, step, excepted):
        command = step["command"]
        kind = ACTIONS.get(command, command)
        self.asked_from = self.log.count
        answers = list(step.get("answers", []))
        if command in excepted:
            self.workspace_command(command, step)
        elif command in KEYS:
            self.escape()
            self.keys(*KEYS[command])
            self.settle()
        elif kind in OFFERED and "count" in step:
            # A code action takes no count: the command as from Helix's
            # command line, with the count in its argument.
            self.workspace_command(command, step)
        elif kind in OFFERED:
            self.code_action(kind)
        else:
            raise Failure(f"{command} has no key nor code action in Helix, and editors.helix does not say why")
        for answer in answers:
            self.answer(answer, command)

    # What is looked at.

    def write_all(self):
        self.command_line("write-all")

    def read(self, name):
        try:
            with open(os.path.join(self.directory, name), encoding="utf-8") as file:
                return file.read()
        except FileNotFoundError:
            return None

    def messages(self):
        """What the server said since the messages were last looked at,
        without its questions (a rename answers them)."""
        said = []
        for index, message in self.log.received:
            if index <= self.messages_from:
                continue
            if message.get("method") == "window/showMessage":
                text = message["params"]["message"]
                if PROMPT not in text:
                    said.append(text)
            elif "error" in message and "method" not in message:
                said.append(message["error"].get("message", ""))
        self.messages_from = self.log.count
        return "".join(f"{text}\n" for text in said)

    def snapshot(self, kind):
        ended = lambda text: text.rstrip("\n") + "\n"
        if kind in ("buffer", "cursor") or kind.startswith("file:"):
            self.write_all()
        _, name, line, _ = self.status()
        if kind == "buffer":
            return f"[{name}]\n" + ended(self.read(name) or "")
        if kind == "cursor":
            lines = (self.read(name) or "").split("\n")
            return f"{name}:{line}: {lines[line - 1] if line - 1 < len(lines) else ''}\n"
        if kind == "messages":
            self.log.read()
            return self.messages()
        if kind.startswith("file:"):
            text = self.read(kind[len("file:"):])
            return "(no file)\n" if text is None else ended(text)
        if kind == "agenda":
            return "(no agenda)\n"
        raise Failure(f"Helix cannot show {kind}")


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
                continue
            try:
                if "open" in step and number > 1:
                    helix.command_line(f"open {step['open']}")
                if "focus" in step or "close" in step:
                    raise Failure("Helix has no agenda to go to or close")
                helix.place(step)
                if "type" in step:
                    helix.type_text(step["type"])
                if "command" in step:
                    helix.run(step, excepted)
                for kind in step.get("expect", []):
                    key = f"{number} {kind}"
                    actual = helix.snapshot(kind).replace(helix.directory, "$DIR")
                    want = expected.get(key)
                    if want is None:
                        raise Failure(f"no expected snapshot `{key}`")
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
    print(f"helix: {failed} failed, in {time.monotonic() - began:.0f}s")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
