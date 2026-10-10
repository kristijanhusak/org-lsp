"""Neovim in a terminal of its own, read cell by cell: what the screenshots
are made of.

`Term` runs Neovim in a pseudo-terminal that pyte reads as a terminal does
(colors, bold, the cursor); `frame()` is what it shows, which `html()`
turns into a page for the browser to draw.
"""

import fcntl
import html as escape
import os
import pty
import re
import select
import struct
import termios
import time

import pyte

# Neovim's default colors, for the cells that keep the terminal's own.
FOREGROUND = "#e0e2ea"
BACKGROUND = "#14161b"
NAMED = {
    "black": "#07080d", "red": "#ffc0b9", "green": "#b3f6c0", "brown": "#fce094",
    "blue": "#a6dbff", "magenta": "#ffcaff", "cyan": "#8cf8f7", "white": "#eef1f8",
}


class Term:
    def __init__(self, argv, env, columns=100, lines=30, cwd=None):
        self.screen = pyte.Screen(columns, lines)
        self.stream = pyte.ByteStream(self.screen)
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            if cwd:
                os.chdir(cwd)
            os.environ.update(env)
            os.environ["TERM"] = "xterm-256color"
            os.environ["COLORTERM"] = "truecolor"
            os.execvp(argv[0], argv)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", lines, columns, 0, 0))

    def drain(self, seconds):
        """Read what Neovim draws for `seconds`."""
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([self.fd], [], [], 0.02)
            if ready:
                try:
                    data = os.read(self.fd, 65536)
                except OSError:
                    return
                self.answer(data)
                # Device control strings (capability queries) are not
                # text, which pyte would show.
                data = re.sub(rb"\x1bP.*?\x1b\\", b"", data, flags=re.S)
                self.stream.feed(re.sub(rb"\x1b\[([0-9;:]*)m", plain_sgr, data))

    def answer(self, data):
        """Answer what Neovim asks the terminal: the background color, and
        the device's attributes after it."""
        if b"\x1b]11;?" in data:
            os.write(self.fd, b"\x1b]11;rgb:1414/1616/1b1b\x07")
        if b"\x1b[5n" in data:
            os.write(self.fd, b"\x1b[0n")
        if b"\x1b[c" in data:
            os.write(self.fd, b"\x1b[?62;22c")

    def text(self):
        return "\n".join(self.screen.display)

    def until(self, predicate, timeout=15, what="the screen"):
        """Wait until `predicate(text)` holds of what shows."""
        end = time.time() + timeout
        while time.time() < end:
            self.drain(0.1)
            if predicate(self.text()):
                self.drain(0.3)
                return
        raise TimeoutError(f"waiting for {what}:\n{self.text()}")

    def send(self, keys, pause=0.4):
        os.write(self.fd, keys.encode())
        self.drain(pause)

    def frame(self):
        """The cells that show, row by row, and the cursor."""
        rows = []
        for y in range(self.screen.lines):
            line = self.screen.buffer[y]
            rows.append([line[x] for x in range(self.screen.columns)])
        cursor = None if self.screen.cursor.hidden else (self.screen.cursor.y, self.screen.cursor.x)
        return rows, cursor

    def close(self):
        try:
            os.kill(self.pid, 9)
            os.waitpid(self.pid, 0)
        except OSError:
            pass


def plain_sgr(match):
    """Rendition with the colon forms Neovim uses for underlines
    (`4:1`, `4:0`, `58:2::r:g:b`) as pyte reads them."""
    params = []
    for param in match.group(1).decode().split(";"):
        if ":" not in param:
            params.append(param)
        elif param.startswith("4:"):
            params.append("24" if param == "4:0" else "4")
    return ("\x1b[" + ";".join(params) + "m").encode()


def color(value, default):
    if value in (None, "default"):
        return default
    if value in NAMED:
        return NAMED[value]
    if len(value) == 6 and all(c in "0123456789abcdefABCDEF" for c in value):
        return "#" + value
    return default


def html(frame, title=""):
    """A page that draws `frame` as a terminal window would."""
    rows, cursor = frame
    # The window takes the background most of its cells have.
    counts = {}
    for row in rows:
        for cell in row:
            key = color(cell.bg, BACKGROUND)
            counts[key] = counts.get(key, 0) + 1
    background = max(counts, key=counts.get)
    out = []
    for y, row in enumerate(rows):
        spans = []
        for x, cell in enumerate(row):
            fg = color(cell.fg, FOREGROUND)
            bg = color(cell.bg, BACKGROUND)
            if cell.reverse:
                fg, bg = bg, fg
            if cursor == (y, x):
                fg, bg = BACKGROUND, FOREGROUND
            style = f"color:{fg};background:{bg}"
            if cell.bold:
                style += ";font-weight:bold"
            if cell.italics:
                style += ";font-style:italic"
            decorations = [name for name, on in (("underline", cell.underscore), ("line-through", cell.strikethrough)) if on]
            if decorations:
                style += ";text-decoration:" + " ".join(decorations)
            char = cell.data or " "
            if spans and spans[-1][0] == style:
                spans[-1][1].append(char)
            else:
                spans.append((style, [char]))
        out.append("".join(f'<span style="{style}">{escape.escape("".join(chars))}</span>' for style, chars in spans))
    body = "\n".join(out)
    return f"""<!doctype html><html><head><meta charset="utf-8"><style>
body {{ margin: 0; background: transparent; }}
#window {{ display: inline-block; border-radius: 10px; overflow: hidden; background: {background};
  box-shadow: 0 8px 28px rgba(0,0,0,.35); margin: 18px; }}
#bar {{ height: 30px; background: #2b2f38; display: flex; align-items: center; padding: 0 12px; gap: 8px;
  font: 13px 'DejaVu Sans', sans-serif; color: #9aa0ad; }}
.dot {{ width: 12px; height: 12px; border-radius: 50%; }}
#title {{ flex: 1; text-align: center; margin-right: 52px; }}
pre {{ margin: 0; padding: 10px 12px; font: 14px/1.32 'DejaVu Sans Mono', monospace; white-space: pre;
  width: {len(rows[0])}ch; overflow: hidden; }}
</style></head><body><div id="window"><div id="bar">
<span class="dot" style="background:#ff5f57"></span><span class="dot" style="background:#febc2e"></span>
<span class="dot" style="background:#28c840"></span><span id="title">{escape.escape(title)}</span></div>
<pre>{body}</pre></div></body></html>"""
