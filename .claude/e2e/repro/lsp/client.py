#!/usr/bin/env python3
"""A generic LSP client for driving org-lsp over stdio, with no editor code.

Usage as a library:

    from client import Client, PLAIN, FULL
    c = Client(profile=FULL, root=workspace, options={...})
    c.open(path)
    r = c.command("org.todo.next", c.at(path, 3, 0))
    c.shutdown()

Profiles:
- PLAIN: no `experimental`, no `orgClient`; announces showMessage
  (message requests), showDocument, applyEdit with documentChanges.
- FULL: every `experimental.org*` capability and `orgClient`.
- BARE: a plain client that announces neither window.showDocument nor
  window.showMessage (the minimal client).

Server-to-client requests are answered as an editor would: applyEdit
applied to the in-memory document (opened from disk first when it is not
open, as editors load a buffer), showMessageRequest answered by `pick`,
showDocument recorded (and the document opened), textDocumentContent
refresh followed by a fetch and didChange.
"""
import json
import os
import queue
import subprocess
import threading
import time
import urllib.parse

SERVER = "/home/user/org-lsp/target/debug/org-lsp"
ORG_LISP = "/root/org-mode/lisp"
SCRATCH = "/tmp/claude-0/-home-user-org-lsp/25b18cf7-c433-5d20-a6ec-4b7d33312b78/scratchpad/lsp"

PLAIN, FULL, BARE = "plain", "full", "bare"

EXPERIMENTAL = {
    "orgClient": True,
    "orgDatePrompt": True,
    "orgVirtualIndent": True,
    "orgDisplay": True,
    "orgImages": True,
    "orgLineHighlights": True,
    "orgHiddenLines": True,
    "orgRunCommand": True,
}


class Timeout(Exception):
    pass


class ResponseError(Exception):
    def __init__(self, error):
        super().__init__(f"{error.get('code')}: {error.get('message')}")
        self.error = error
        self.code = error.get("code")
        self.message = error.get("message")
        self.data = error.get("data")


def uri_of(path):
    return "file://" + urllib.parse.quote(os.path.abspath(path))


def path_of(uri):
    if uri.startswith("file://"):
        return urllib.parse.unquote(uri[7:])
    return None


def capabilities(profile, encodings=None, extra=None):
    caps = {
        "general": {"positionEncodings": encodings or ["utf-16"]},
        "workspace": {
            "applyEdit": True,
            "workspaceEdit": {
                "documentChanges": True,
                "resourceOperations": ["create", "rename", "delete"],
            },
            "configuration": True,
            "executeCommand": {},
            "semanticTokens": {"refreshSupport": True},
            "inlayHint": {"refreshSupport": True},
            "didChangeWatchedFiles": {"dynamicRegistration": True},
            "symbol": {},
        },
        "textDocument": {
            "synchronization": {"didSave": True, "willSaveWaitUntil": True},
            "documentSymbol": {"hierarchicalDocumentSymbolSupport": True},
            "semanticTokens": {
                "requests": {"full": {"delta": True}, "range": True},
                "tokenTypes": [],
                "tokenModifiers": [],
                "formats": ["relative"],
            },
            "foldingRange": {"lineFoldingOnly": True},
            "completion": {"completionItem": {"snippetSupport": True}},
            "codeAction": {
                "codeActionLiteralSupport": {"codeActionKind": {"valueSet": ["", "quickfix", "refactor", "source"]}}
            },
            "rename": {"prepareSupport": True},
            "inlayHint": {},
            "publishDiagnostics": {"relatedInformation": True},
        },
        "window": {
            "workDoneProgress": True,
            "showMessage": {"messageActionItem": {"additionalPropertiesSupport": False}},
            "showDocument": {"support": True},
        },
    }
    if profile == FULL:
        caps["experimental"] = dict(EXPERIMENTAL)
        caps["workspace"]["textDocumentContent"] = {}
    if profile == BARE:
        del caps["window"]["showMessage"]
        del caps["window"]["showDocument"]
    if extra:
        _merge(caps, extra)
    return caps


def _merge(a, b):
    for k, v in b.items():
        if isinstance(v, dict) and isinstance(a.get(k), dict):
            _merge(a[k], v)
        elif v is None:
            a.pop(k, None)
        else:
            a[k] = v


# ----- positions


def _units(ch, encoding):
    if encoding == "utf-8":
        return len(ch.encode("utf-8"))
    if encoding == "utf-16":
        return 2 if ord(ch) > 0xFFFF else 1
    return 1


def split_lines(text):
    """Lines as the server sees them: \\n, \\r\\n, lone \\r end a line."""
    lines, start, i = [], 0, 0
    while i < len(text):
        c = text[i]
        if c == "\n" or c == "\r":
            lines.append((start, i))
            if c == "\r" and i + 1 < len(text) and text[i + 1] == "\n":
                i += 1
            start = i + 1
        i += 1
    lines.append((start, len(text)))
    return lines


def offset_of(text, pos, encoding):
    lines = split_lines(text)
    line = pos["line"]
    if line >= len(lines):
        return len(text)
    start, end = lines[line]
    units = pos["character"]
    i = start
    while i < end and units > 0:
        units -= _units(text[i], encoding)
        i += 1
    if units < 0:
        raise ValueError(f"position {pos} splits a character")
    return i


def position_of(text, offset, encoding):
    lines = split_lines(text)
    for n, (start, end) in enumerate(lines):
        if offset <= end or n == len(lines) - 1:
            return {"line": n, "character": sum(_units(c, encoding) for c in text[start:offset])}
    raise ValueError


def apply_text_edits(text, edits, encoding):
    spans = []
    for e in edits:
        s = offset_of(text, e["range"]["start"], encoding)
        t = offset_of(text, e["range"]["end"], encoding)
        if t < s:
            raise ValueError(f"edit with end before start: {e}")
        spans.append((s, t, e.get("newText", "")))
    # Stable: edits at the same position apply in their order.
    order = sorted(range(len(spans)), key=lambda i: (spans[i][0], spans[i][1], i))
    for a, b in zip(order, order[1:]):
        if spans[a][1] > spans[b][0]:
            raise ValueError(f"overlapping edits: {edits[a]} {edits[b]}")
    out, last = [], 0
    for i in order:
        s, t, new = spans[i]
        out.append(text[last:s])
        out.append(new)
        last = t
    out.append(text[last:])
    return "".join(out)


def decode_tokens(data, legend, text=None, encoding="utf-16"):
    """[(line, char, length, type, [modifiers], text)]."""
    types, mods = legend["tokenTypes"], legend["tokenModifiers"]
    out, line, char = [], 0, 0
    lines = text.split("\n") if text is not None else None
    for i in range(0, len(data), 5):
        dl, dc, ln, ty, md = data[i : i + 5]
        line += dl
        char = char + dc if dl == 0 else dc
        m = [mods[b] for b in range(len(mods)) if md >> b & 1]
        snippet = None
        if lines is not None and line < len(lines):
            # Convert units to characters on this line.
            l = lines[line]
            s = offset_of(l, {"line": 0, "character": char}, encoding)
            e = offset_of(l, {"line": 0, "character": char + ln}, encoding)
            snippet = l[s:e]
        out.append((line, char, ln, types[ty] if ty < len(types) else f"?{ty}", m, snippet))
    return out


# ----- the client


class Client:
    def __init__(
        self,
        profile=PLAIN,
        root=None,
        options=None,
        caps_extra=None,
        encodings=None,
        env=None,
        pick=None,
        org_lisp=True,
        name="client",
        open_shown=True,
        log_dir=None,
        initialize=True,
        timeout=20,
        folders=None,
    ):
        self.profile = profile
        self.root = root
        self.timeout = timeout
        self.name = name
        self.log_dir = log_dir or os.path.join(SCRATCH, "logs")
        os.makedirs(self.log_dir, exist_ok=True)
        self.stderr_path = os.path.join(self.log_dir, f"{name}-{os.getpid()}-{int(time.time()*1000)}.stderr")
        self.trace_path = self.stderr_path.replace(".stderr", ".trace")
        self.trace = open(self.trace_path, "w")
        e = dict(os.environ)
        cfg = os.path.join(SCRATCH, "xdg-config")
        os.makedirs(cfg, exist_ok=True)
        e.setdefault("XDG_CONFIG_HOME", cfg)
        e["XDG_CONFIG_HOME"] = cfg
        e["XDG_STATE_HOME"] = os.path.join(SCRATCH, "xdg-state")
        e["XDG_DATA_HOME"] = os.path.join(SCRATCH, "xdg-data")
        e["XDG_CACHE_HOME"] = os.path.join(SCRATCH, "xdg-cache")
        e.pop("ORG_LSP_CONFIG", None)
        e["ORG_LISP_DIR"] = ORG_LISP
        if env:
            e.update(env)
        self.proc = subprocess.Popen(
            [SERVER],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=open(self.stderr_path, "w"),
            env=e,
            cwd=root or SCRATCH,
        )
        self.queue = queue.Queue()
        self.next_id = 0
        self.responses = {}
        self.docs = {}  # uri -> {"text","version","languageId"}
        self.notifications = []  # (time, method, params)
        self.server_requests = []  # (method, params, response)
        self.messages = []  # showMessage params
        self.shown = []  # showDocument params
        self.applied = []  # (edit, ok, reason)
        self.edit_failures = []
        self.diagnostics = {}
        self.pick = pick  # fn(params) -> title or None
        self.picks = []  # a queue of titles/keys to answer showMessageRequest
        self.open_shown = open_shown
        self.apply_ok = True
        self.encoding = "utf-16"
        self.closed = False
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        self.options = dict(options or {})
        if org_lisp and "emacs_config" not in self.options:
            self.options["emacs_config"] = f'(add-to-list \'load-path "{ORG_LISP}")'
        self.caps = capabilities(profile, encodings, caps_extra)
        self.init_result = None
        self.folders = folders
        if initialize:
            self.initialize()

    # ----- framing

    def _read(self):
        out = self.proc.stdout
        while True:
            headers = {}
            while True:
                line = out.readline()
                if not line:
                    self.queue.put(None)
                    return
                line = line.decode("ascii").strip()
                if not line:
                    break
                k, _, v = line.partition(":")
                headers[k.strip().lower()] = v.strip()
            n = int(headers.get("content-length", 0))
            body = out.read(n)
            try:
                msg = json.loads(body)
            except Exception as err:
                msg = {"__bad__": body.decode("utf-8", "replace"), "error": str(err)}
            self.trace.write("<- " + json.dumps(msg)[:4000] + "\n")
            self.trace.flush()
            self.queue.put(msg)

    def _send(self, msg):
        if self.closed:
            return
        msg["jsonrpc"] = "2.0"
        data = json.dumps(msg).encode("utf-8")
        self.trace.write("-> " + json.dumps(msg)[:4000] + "\n")
        self.trace.flush()
        try:
            self.proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
            self.proc.stdin.flush()
        except BrokenPipeError:
            self.closed = True

    def send_raw(self, raw: bytes):
        self.proc.stdin.write(raw)
        self.proc.stdin.flush()

    # ----- pumping

    def _handle(self, msg):
        if msg is None:
            self.closed = True
            return
        if "__bad__" in msg:
            self.notifications.append((time.time(), "__bad_json__", msg))
            return
        if "id" in msg and ("result" in msg or "error" in msg) and "method" not in msg:
            self.responses[msg["id"]] = msg
            return
        method = msg.get("method")
        params = msg.get("params")
        if "id" in msg:
            try:
                result = self._server_request(method, params)
                self._send({"id": msg["id"], "result": result})
            except ResponseError as err:
                self._send({"id": msg["id"], "error": err.error})
            except Exception as err:  # noqa
                self._send({"id": msg["id"], "error": {"code": -32603, "message": f"client: {err!r}"}})
                self.edit_failures.append(("handler", method, repr(err)))
            self.server_requests.append((method, params))
        else:
            self._notification(method, params)

    def pump(self, until=None, timeout=None):
        """Handle messages until `until()` is true or `timeout` passes."""
        deadline = time.time() + (self.timeout if timeout is None else timeout)
        while True:
            if until and until():
                return True
            left = deadline - time.time()
            if left <= 0:
                return False
            try:
                msg = self.queue.get(timeout=min(left, 0.05))
            except queue.Empty:
                continue
            self._handle(msg)
            if self.closed and msg is None:
                return until() if until else False

    def settle(self, quiet=0.3, timeout=10):
        """Pump until no message came for `quiet` seconds."""
        deadline = time.time() + timeout
        last = time.time()
        while time.time() < deadline:
            try:
                msg = self.queue.get(timeout=0.05)
            except queue.Empty:
                if time.time() - last >= quiet:
                    return True
                continue
            last = time.time()
            self._handle(msg)
        return False

    # ----- requests

    def request(self, method, params=None, timeout=None):
        self.next_id += 1
        rid = self.next_id
        self._send({"id": rid, "method": method, "params": params})
        ok = self.pump(lambda: rid in self.responses or self.closed, timeout)
        if rid not in self.responses:
            raise Timeout(f"{method} timed out after {timeout or self.timeout}s")
        msg = self.responses.pop(rid)
        if "error" in msg:
            raise ResponseError(msg["error"])
        return msg.get("result")

    def notify(self, method, params=None):
        self._send({"method": method, "params": params})

    def initialize(self):
        folders = self.folders or ([self.root] if self.root else [])
        params = {
            "processId": os.getpid(),
            "clientInfo": {"name": f"e2e-{self.profile}", "version": "0"},
            "rootUri": uri_of(folders[0]) if folders else None,
            "workspaceFolders": [{"uri": uri_of(f), "name": os.path.basename(f)} for f in folders] or None,
            "capabilities": self.caps,
            "initializationOptions": self.options,
        }
        self.init_result = self.request("initialize", params, timeout=30)
        self.encoding = self.init_result["capabilities"].get("positionEncoding", "utf-16")
        self.notify("initialized", {})
        return self.init_result

    @property
    def legend(self):
        return self.init_result["capabilities"]["semanticTokensProvider"]["legend"]

    @property
    def manifest(self):
        return self.init_result["capabilities"].get("experimental", {}).get("org")

    def shutdown(self):
        try:
            self.request("shutdown", None, timeout=10)
            self.notify("exit")
        except Exception as err:
            self.notifications.append((time.time(), "__shutdown_error__", repr(err)))
        try:
            code = self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            code = "killed"
        self.trace.close()
        return code

    def kill(self):
        self.proc.kill()
        self.proc.wait()

    def stderr(self):
        with open(self.stderr_path) as f:
            return f.read()

    # ----- documents

    def open(self, path_or_uri, text=None, language="org"):
        uri = path_or_uri if "://" in path_or_uri or path_or_uri.startswith("org-") else uri_of(path_or_uri)
        if text is None:
            p = path_of(uri)
            text = open(p, encoding="utf-8", newline="").read() if p and os.path.exists(p) else ""
        if uri in self.docs:
            return uri
        self.docs[uri] = {"text": text, "version": 1, "languageId": language}
        self.notify("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": language, "version": 1, "text": text}})
        return uri

    def uri(self, path):
        return path if "://" in path or path.startswith("org-") else uri_of(path)

    def text(self, path):
        return self.docs[self.uri(path)]["text"]

    def close(self, path):
        uri = self.uri(path)
        self.docs.pop(uri, None)
        self.notify("textDocument/didClose", {"textDocument": {"uri": uri}})

    def change(self, path, edits=None, text=None):
        """Edit an open document as a user would: TextEdits (all against the
        current text) sent as incremental changes, or the whole text."""
        uri = self.uri(path)
        d = self.docs[uri]
        d["version"] += 1
        if text is not None:
            d["text"] = text
            changes = [{"text": text}]
        else:
            changes = self._incremental(d["text"], edits)
            d["text"] = apply_text_edits(d["text"], edits, self.encoding)
        self.notify(
            "textDocument/didChange",
            {"textDocument": {"uri": uri, "version": d["version"]}, "contentChanges": changes},
        )

    def _incremental(self, text, edits):
        # Bottom-up, so that each change's range is right after the ones
        # before it are applied.
        def key(e):
            return (e["range"]["start"]["line"], e["range"]["start"]["character"])

        return [{"range": e["range"], "text": e.get("newText", "")} for e in sorted(edits, key=key, reverse=True)]

    def type_at(self, path, line, char, s):
        p = {"line": line, "character": char}
        self.change(path, [{"range": {"start": p, "end": p}, "newText": s}])

    def save(self, path):
        uri = self.uri(path)
        p = path_of(uri)
        d = self.docs[uri]
        with open(p, "w", encoding="utf-8", newline="") as f:
            f.write(d["text"])
        self.notify("textDocument/didSave", {"textDocument": {"uri": uri}, "text": d["text"]})

    def save_all(self):
        for uri in list(self.docs):
            if path_of(uri) and not uri.endswith(".org-agenda"):
                p = path_of(uri)
                cur = open(p, encoding="utf-8", newline="").read() if os.path.exists(p) else None
                if cur != self.docs[uri]["text"]:
                    self.save(uri)

    # ----- server requests

    def _server_request(self, method, params):
        if method == "workspace/applyEdit":
            return self._apply_edit(params)
        if method == "window/showMessageRequest":
            return self._message_request(params)
        if method == "window/showDocument":
            self.shown.append(params)
            if self.open_shown:
                uri = params["uri"]
                if uri.startswith("file://") and uri not in self.docs and not params.get("external"):
                    p = path_of(uri)
                    if os.path.exists(p):
                        lang = "org-agenda" if p.endswith(".org-agenda") else ("org-date" if p.endswith(".org-date") else "org")
                        self.open(uri, language=lang)
                elif uri.startswith("org-agenda:") and uri not in self.docs:
                    self._fetch_open(uri)
            return {"success": True}
        if method == "workspace/configuration":
            return [None for _ in params.get("items", [])]
        if method in ("window/workDoneProgress/create", "client/registerCapability", "client/unregisterCapability"):
            return None
        if method == "workspace/textDocumentContent/refresh":
            uri = params.get("uri")
            self.notifications.append((time.time(), method, params))
            if uri in self.docs:
                self._refetch(uri)
            return None
        if method in ("workspace/semanticTokens/refresh", "workspace/inlayHint/refresh", "workspace/codeLens/refresh",
                      "workspace/diagnostic/refresh", "workspace/foldingRange/refresh"):
            self.notifications.append((time.time(), method, params))
            return None
        raise ResponseError({"code": -32601, "message": f"client does not handle {method}"})

    def _fetch_open(self, uri):
        # Fetch asynchronously would deadlock here: we are inside the
        # handler. Queue a fetch to run after we answer.
        self._pending_fetch = getattr(self, "_pending_fetch", [])
        self._pending_fetch.append(("open", uri))

    def _refetch(self, uri):
        self._pending_fetch = getattr(self, "_pending_fetch", [])
        self._pending_fetch.append(("refresh", uri))

    def run_fetches(self):
        """Fetch the virtual documents the server showed or refreshed."""
        while getattr(self, "_pending_fetch", None):
            kind, uri = self._pending_fetch.pop(0)
            try:
                r = self.request("workspace/textDocumentContent", {"uri": uri}, timeout=15)
            except Exception as err:
                self.edit_failures.append(("fetch", uri, repr(err)))
                continue
            text = r.get("text", "") if isinstance(r, dict) else ""
            if uri in self.docs:
                if self.docs[uri]["text"] != text:
                    self.change(uri, text=text)
            else:
                lang = "org-date" if "Date" in uri else "org-agenda"
                self.open(uri, text=text, language=lang)

    def _message_request(self, params):
        actions = params.get("actions") or []
        title = None
        if self.picks:
            want = self.picks.pop(0)
            for a in actions:
                t = a["title"]
                if t == want or t.endswith(f"[{want}]") or t.startswith(want):
                    title = t
                    break
            if title is None and want is None:
                return None
            if title is None:
                self.edit_failures.append(("pick", want, [a["title"] for a in actions]))
        elif self.pick:
            title = self.pick(params)
        if title is None:
            return None
        return {"title": title}

    def _apply_edit(self, params):
        edit = params["edit"]
        try:
            self._apply_workspace_edit(edit)
            self.applied.append((params, True, None))
            return {"applied": True}
        except Exception as err:
            self.applied.append((params, False, repr(err)))
            self.edit_failures.append(("applyEdit", params.get("label"), repr(err)))
            return {"applied": False, "failureReason": repr(err)}

    def _ensure_open(self, uri):
        if uri not in self.docs:
            p = path_of(uri)
            if p is None:
                raise ValueError(f"edit for a document not open: {uri}")
            lang = "org-agenda" if p.endswith(".org-agenda") else "org"
            self.open(uri, language=lang)

    def _apply_workspace_edit(self, edit):
        if not self.apply_ok:
            raise ValueError("client refuses edits")
        if "documentChanges" in edit and edit["documentChanges"] is not None:
            for change in edit["documentChanges"]:
                kind = change.get("kind")
                if kind == "create":
                    p = path_of(change["uri"])
                    opts = change.get("options") or {}
                    if os.path.exists(p) and not opts.get("overwrite"):
                        if opts.get("ignoreIfExists"):
                            continue
                        raise ValueError(f"create: {p} exists")
                    os.makedirs(os.path.dirname(p), exist_ok=True)
                    open(p, "w").close()
                elif kind == "rename":
                    os.rename(path_of(change["oldUri"]), path_of(change["newUri"]))
                    if change["oldUri"] in self.docs:
                        self.docs[change["newUri"]] = self.docs.pop(change["oldUri"])
                elif kind == "delete":
                    p = path_of(change["uri"])
                    if os.path.exists(p):
                        os.remove(p)
                    self.docs.pop(change["uri"], None)
                else:
                    uri = change["textDocument"]["uri"]
                    self._ensure_open(uri)
                    version = change["textDocument"].get("version")
                    if version is not None and version != self.docs[uri]["version"]:
                        raise ValueError(f"version mismatch for {uri}: edit {version}, document {self.docs[uri]['version']}")
                    self.change(uri, change["edits"])
        for uri, edits in (edit.get("changes") or {}).items():
            self._ensure_open(uri)
            self.change(uri, edits)

    # ----- notifications

    def _notification(self, method, params):
        self.notifications.append((time.time(), method, params))
        if method == "textDocument/publishDiagnostics":
            self.diagnostics[params["uri"]] = params["diagnostics"]
        elif method in ("window/showMessage", "window/logMessage"):
            if method == "window/showMessage":
                self.messages.append(params)
        elif method == "org-lsp/save":
            for uri in params.get("uris", []):
                if uri in self.docs and path_of(uri):
                    self.save(uri)

    def notes(self, method):
        return [p for (_, m, p) in self.notifications if m == method]

    def last_messages(self, since=0):
        return [m["message"] for m in self.messages[since:]]

    # ----- helpers

    def at(self, path, line, char=0, **extra):
        a = {"textDocument": {"uri": self.uri(path)}, "position": {"line": line, "character": char}}
        a.update(extra)
        return a

    def find(self, path, needle, nth=0, delta=0):
        """The position of the `nth` occurrence of `needle` in the document
        (+delta characters), in the client's encoding."""
        text = self.text(path)
        i = -1
        for _ in range(nth + 1):
            i = text.index(needle, i + 1)
        return position_of(text, i + delta, self.encoding)

    def at_text(self, path, needle, nth=0, delta=0, **extra):
        p = self.find(path, needle, nth, delta)
        return self.at(path, p["line"], p["character"], **extra)

    def command(self, command, argument=None, timeout=None):
        args = [argument] if argument is not None else []
        r = self.request("workspace/executeCommand", {"command": command, "arguments": args}, timeout)
        self.run_fetches()
        return r

    def run(self, command, argument=None, answers=(), timeout=None, max_steps=12):
        """Run a command, answering its questions as an org client does
        (FULL): each `input` answered from `answers` in order (a callable
        gets the input and returns the answer). Returns (result, steps)."""
        answers = list(answers)
        steps = []
        argument = dict(argument or {})
        result = self.command(command, argument, timeout)
        steps.append(result)
        n = 0
        while isinstance(result, dict) and "input" in result and n < max_steps:
            n += 1
            inp = result["input"]
            if not answers:
                break
            ans = answers.pop(0)
            if callable(ans):
                ans = ans(inp)
            if ans is StopIteration:
                break
            arg = dict(argument)
            arg.update(inp.get("context") or {})
            arg["input"] = ans
            argument = arg
            result = self.command(command, argument, timeout)
            steps.append(result)
        self.settle(0.2, 5)
        self.run_fetches()
        return result, steps

    def apply_result(self, argument, result):
        """What an org client does with a command's result: apply
        `edits` to the document of the argument (`{ "edits", "cursor" }`)."""
        if isinstance(result, dict) and result.get("edits") and argument:
            uri = argument["textDocument"]["uri"]
            if uri in self.docs:
                self.change(uri, result["edits"])
        return result

    def rename_answer(self, path, line, char, text):
        """A plain client answers a text prompt with a rename."""
        uri = self.uri(path)
        pos = {"line": line, "character": char}
        prep = self.request("textDocument/prepareRename", {"textDocument": {"uri": uri}, "position": pos})
        r = self.request("textDocument/rename", {"textDocument": {"uri": uri}, "position": pos, "newName": text})
        if r:
            self._apply_workspace_edit(r)
        self.settle(0.3, 8)
        self.run_fetches()
        return prep, r

    def code_actions(self, path, line, char=0, end=None, only=None, diagnostics=None):
        uri = self.uri(path)
        start = {"line": line, "character": char}
        ctx = {"diagnostics": diagnostics or []}
        if only:
            ctx["only"] = only
        return self.request(
            "textDocument/codeAction",
            {"textDocument": {"uri": uri}, "range": {"start": start, "end": end or start}, "context": ctx},
        ) or []

    def run_action(self, action):
        """Do what a code action says: its edit, then its command."""
        if action.get("edit"):
            self._apply_workspace_edit(action["edit"])
        cmd = action.get("command")
        r = None
        if cmd:
            r = self.command(cmd["command"], (cmd.get("arguments") or [None])[0])
        self.settle(0.3, 8)
        self.run_fetches()
        return r
