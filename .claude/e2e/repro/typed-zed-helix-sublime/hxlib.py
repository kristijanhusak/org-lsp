"""By-hand checks in Helix, typed through tmux with editors/helix/test/run.py's
Helix class: helpers, and `run(name, files, options, checks)` that prints
one line per check with what it gave."""
import importlib.util
import os
import sys
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
os.environ.setdefault("HX", "/root/helix/helix-25.07.1-x86_64-linux/hx")
os.environ.setdefault("ORG_LISP_DIR", "/root/org-mode/lisp")
os.environ.setdefault("ORG_LSP_BIN", "/home/user/org-lsp/target/debug/org-lsp")
spec = importlib.util.spec_from_file_location("hxrun", os.path.join(ROOT, "editors", "helix", "test", "run.py"))
hx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hx)
Helix, Failure = hx.Helix, hx.Failure


class H:
    def __init__(self, h):
        self.h = h

    def titles(self):
        actions = self.h.code_actions()
        self.h.keys("Escape")
        self.h.settle()
        return [a["title"] for a in actions]

    def goto(self, line, col=1):
        self.h.command_line(f"goto {line}")
        self.h.keys("g", "h")
        if col > 1:
            self.h.keys(str(col - 1), "l")
        self.h.settle()

    def at(self, text, column=0):
        self.h.place({"at": text, "column": column})

    def act(self, part, answers=()):
        actions = self.h.code_actions()
        index = next((i for i, a in enumerate(actions) if a["title"].startswith(part)), None)
        if index is None:
            index = next((i for i, a in enumerate(actions) if part in a["title"]), None)
        if index is None:
            self.h.keys("Escape")
            self.h.settle()
            raise Failure(f"no action like {part!r}: {[a['title'] for a in actions]}")
        self.h.asked_from = self.h.log.count
        self.h.pick_action(actions, index)
        for answer in answers:
            self.h.answer(answer, part)

    def path(self, name):
        return os.path.join(self.h.directory, name)

    def text(self, name=None):
        if name is None:
            return self.h.text(self.h.status()[1])
        return self.h.text(self.path(name))

    def lines(self, name=None):
        return (self.text(name) or "").split("\n")

    def where(self):
        _, path, line, col = self.h.status()
        return f"{self.h.name_of(path)}:{line}:{col}: {self.lines_of(path)[line - 1] if line <= len(self.lines_of(path)) else ''}"

    def lines_of(self, path):
        return (self.h.text(path) or "").split("\n")

    def msgs(self):
        return self.h.messages().strip().replace("\n", " | ")

    def screen(self):
        return "\n".join(self.h.screen()).rstrip()

    def type(self, text):
        self.h.type_text(text)

    def keys(self, *keys):
        self.h.escape()
        self.h.keys(*keys)
        self.h.settle()

    def cmd(self, text):
        self.h.command_line(text)


def run(name, files, options, checks, first=None, config=""):
    h = Helix(name, {"files": files, "options": options})
    if config:
        with open(os.path.join(h.config, "config.toml"), "a", encoding="utf-8") as out:
            out.write(config)
    x = H(h)
    try:
        h.start(first or sorted(files)[0])
        for title, check in checks:
            try:
                result = check(x)
                print(f"ok   {title}: {result if result is not None else ''}", flush=True)
            except Exception as error:  # noqa: BLE001
                detail = str(error) if isinstance(error, Failure) else traceback.format_exc()
                print(f"FAIL {title}: {detail[:1500]}", flush=True)
                try:
                    print("     at " + x.where() + "\n     buffer: " + repr(x.text()), flush=True)
                except Exception:  # noqa: BLE001
                    pass
                try:
                    h.keys("Escape")
                    h.settle()
                except Exception:  # noqa: BLE001
                    pass
    finally:
        h.stop()
