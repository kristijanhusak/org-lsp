#!/usr/bin/env python3
"""Make the screenshots and animations of the docs (docs/images/) from
Neovim running the server of this checkout.

    scripts/screenshots.py [SCENE …]

Each scene starts Neovim in a terminal of its own (scripts/screenshots/,
read with pyte) on copies of the sample files, at a fixed date and time,
presses keys as a person would, and takes pictures of what shows: PNG
images, and GIF animations of several pictures. Without names, every
scene is made.

Requires Neovim (`NVIM`, else `nvim` on PATH), Python's pyte, Node with
Playwright and its Chromium, and ffmpeg. The server is built first.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "screenshots"))
from term import Term, html  # noqa: E402

OUT = os.path.join(ROOT, "docs", "images")
FILES = os.path.join(HERE, "screenshots", "files")
ESC = "\x1b"
CR = "\r"


class Scene:
    """Neovim on copies of the sample files, with `settings` on top of
    those of scripts/screenshots/init.lua."""

    def __init__(self, name, settings=None, files=None, open=(), columns=100, lines=36):
        self.name = name
        self.workspace = tempfile.mkdtemp(prefix=f"org-lsp-shot-{name}-")
        for file in files or os.listdir(FILES):
            source = os.path.join(FILES, file)
            target = os.path.join(self.workspace, file)
            if os.path.isdir(source):
                shutil.copytree(source, target)
            else:
                shutil.copy(source, target)
        import json

        env = {
            "SHOT_ROOT": ROOT,
            "SHOT_WORKSPACE": self.workspace,
            # `{workspace}` in a setting is the copies' directory.
            "SHOT_SETTINGS": json.dumps(settings or {}).replace("{workspace}", self.workspace),
            "SHOT_COLORS": os.environ.get("SHOT_COLORS", "habamax"),
            "ORG_LSP_CONFIG": "/dev/null",
            "XDG_STATE_HOME": os.path.join(self.workspace, ".state"),
            "XDG_DATA_HOME": os.path.join(self.workspace, ".data"),
        }
        nvim = os.environ.get("NVIM", "nvim")
        init = os.path.join(HERE, "screenshots", "init.lua")
        argv = [nvim, "--clean", "-n", "-u", init, *open]
        self.term = Term(argv, env, columns, lines, cwd=self.workspace)
        self.term.drain(1.0)
        self.frames = []

    def path(self, file):
        return os.path.join(self.workspace, file)

    def keys(self, keys, pause=0.5):
        self.term.send(keys, pause)

    def command(self, command, pause=0.8):
        self.term.send(ESC + ":" + command + CR, pause)

    def clear(self):
        """Nothing on the command line."""
        self.command('echo ""', 0.5)

    def until(self, text, timeout=20):
        self.term.until(lambda screen: text in screen, timeout, repr(text))

    def settle(self, seconds=1.5):
        self.term.drain(seconds)

    def snap(self, title, hold=1.0):
        """A picture of what shows, held for `hold` seconds in an
        animation."""
        self.frames.append((html(self.term.frame(), title), hold))

    def png(self, name):
        """The last picture, as docs/images/NAME.png."""
        write_pngs([(os.path.join(OUT, name + ".png"), self.frames[-1][0])])

    def gif(self, name):
        """Every picture so far, as docs/images/NAME.gif."""
        work = tempfile.mkdtemp(prefix="org-lsp-gif-")
        pngs = []
        for index, (page, hold) in enumerate(self.frames):
            pngs.append((os.path.join(work, f"{index:03}.png"), page, hold))
        write_pngs([(path, page) for path, page, _ in pngs])
        listing = os.path.join(work, "frames.txt")
        with open(listing, "w") as handle:
            for path, _, hold in pngs:
                handle.write(f"file '{path}'\nduration {hold}\n")
            handle.write(f"file '{pngs[-1][0]}'\n")
        target = os.path.join(OUT, name + ".gif")
        palette = os.path.join(work, "palette.png")
        concat = ["-f", "concat", "-safe", "0", "-i", listing]
        scale = "scale=iw/2:-1:flags=lanczos"
        subprocess.run(["ffmpeg", "-y", "-v", "error", *concat, "-vf", f"{scale},palettegen=stats_mode=diff", palette], check=True)
        subprocess.run(
            ["ffmpeg", "-y", "-v", "error", *concat, "-i", palette, "-lavfi",
             f"[0:v]{scale}[x];[x][1:v]paletteuse=dither=none:diff_mode=rectangle", "-loop", "0", target],
            check=True,
        )
        shutil.rmtree(work)

    def close(self):
        self.term.close()
        shutil.rmtree(self.workspace, ignore_errors=True)


def write_pngs(pages):
    """Draw each (path, page) as a PNG at `path`."""
    work = tempfile.mkdtemp(prefix="org-lsp-png-")
    files = []
    for index, (path, page) in enumerate(pages):
        source = os.path.join(work, f"{index:03}.html")
        with open(source, "w", encoding="utf-8") as handle:
            handle.write(page)
        files.append((source, path))
    env = dict(os.environ)
    env.setdefault("NODE_PATH", subprocess.run(["npm", "root", "-g"], capture_output=True, text=True).stdout.strip())
    subprocess.run(["node", os.path.join(HERE, "screenshots", "render.cjs"), *[source for source, _ in files]], check=True, env=env)
    for source, path in files:
        shutil.move(source[: -len(".html")] + ".png", path)
    shutil.rmtree(work)


# The scenes.


def org_file():
    """An Org file as the server colors it."""
    scene = Scene("org-file", open=["project.org"])
    scene.until("Project notes")
    scene.settle(3)
    scene.snap("project.org")
    scene.png("org-file")
    scene.close()


def agenda():
    """The week's agenda, from the agenda menu."""
    scene = Scene("agenda", open=["project.org"], lines=30)
    scene.until("Project notes")
    scene.settle(2)
    scene.keys(" oa", 1.5)
    scene.keys("a")
    scene.until("Week-agenda")
    scene.keys("\x17o", 1.0)
    scene.settle(2)
    scene.snap("Org Agenda")
    scene.png("agenda")
    scene.close()


def editing():
    """A TODO cycled to DONE, its parent's cookie counting it, and a date
    picked in the calendar."""
    scene = Scene("editing", open=["project.org"])
    scene.until("Project notes")
    scene.settle(2)
    scene.keys("/Book\r", 0.3)
    scene.command("nohlsearch", 0.3)
    scene.clear()
    scene.settle(1)
    scene.snap("project.org", 1.5)
    for _ in range(3):
        scene.keys("cit", 1.2)
        scene.snap("cit: the next TODO state", 1.2)
    scene.keys("j", 0.5)
    scene.snap("project.org", 0.8)
    scene.keys(" ois")
    scene.until("Date+time")
    scene.settle(1)
    scene.snap("<Leader>ois: schedule", 1.5)
    for _ in range(2):
        scene.keys("l", 0.8)
        scene.snap("l: the next day", 0.8)
    scene.keys("\r")
    scene.until("SCHEDULED: <2026-05-20 Wed>")
    scene.settle(1)
    scene.snap("RET: scheduled", 3)
    scene.gif("editing")
    scene.close()


SUPER_AGENDA = {
    "org_super_agenda_mode": True,
    "org_super_agenda_groups": [
        {"name": "Today", "time-grid": True, "scheduled": "today"},
        {"name": "Important", "priority": "A"},
        {"name": "Due soon", "deadline": "future"},
        {"name": "Calls and errands", "tag": ["phone", "errand"]},
        {"name": "Habits", "habit": True},
    ],
}


def super_agenda():
    """The day's agenda in org-super-agenda's groups."""
    scene = Scene("super-agenda", SUPER_AGENDA, open=["project.org"], lines=28)
    scene.until("Project notes")
    scene.settle(2)
    scene.keys(" oa", 1.5)
    scene.keys("a")
    scene.until("Week-agenda")
    scene.keys("\x17o", 0.5)
    scene.keys("vd", 1.0)
    scene.until("Day-agenda")
    scene.clear()
    scene.settle(2)
    scene.snap("Org Agenda")
    scene.png("super-agenda")
    scene.close()


def roam():
    """A note of org-roam with the buffer of what links to it."""
    # The notes are among the agenda's files too, which makes their IDs
    # known to the linter (`org-lint`'s unknown IDs).
    settings = {
        "org_roam_enabled": True,
        "org_roam_directory": "{workspace}/roam",
        "org_agenda_files": ["{workspace}/*.org", "{workspace}/roam/*.org"],
    }
    scene = Scene("roam", settings, open=["roam/20260510-org_mode.org"], columns=140, lines=20)
    scene.until("A plain-text system")
    scene.settle(2)
    scene.keys(" nl")
    scene.until("Backlinks")
    scene.settle(2)
    scene.snap("org-roam")
    scene.png("roam")
    scene.close()


def journal():
    """Today's journal entry, yesterday's TODO carried over to it, and a
    search of the journal."""
    settings = {"org_journal_enabled": True, "org_journal_dir": "{workspace}/journal"}
    # The server knows the journal's files once it runs, for an Org file.
    scene = Scene("journal", settings, open=["project.org"])
    scene.until("Project notes")
    scene.settle(2)
    scene.command("edit journal/20260517")
    scene.until("Morning pages")
    scene.clear()
    scene.settle(2)
    scene.snap("journal/20260517", 1.5)
    scene.keys(" jj")
    scene.until("Monday, 05/18/26")
    scene.settle(1.5)
    scene.snap("<Leader>jj: today's entry", 2.5)
    scene.keys(ESC, 1.0)
    scene.keys(" js", 0.8)
    scene.keys("conference", 0.4)
    scene.snap("<Leader>js: search", 1.2)
    scene.keys("\r")
    scene.until("Enter the search start")
    scene.keys("k", 0.8)
    scene.snap("The first day", 1.2)
    scene.keys("\r")
    scene.until("Enter the search end")
    scene.keys("\r")
    scene.until("Search results")
    scene.settle(1.5)
    scene.snap("The results", 3)
    scene.gif("journal")
    scene.close()


SCENES = {
    "org-file": org_file,
    "agenda": agenda,
    "editing": editing,
    "super-agenda": super_agenda,
    "roam": roam,
    "journal": journal,
}


def main():
    names = sys.argv[1:] or list(SCENES)
    subprocess.run(["cargo", "build", "-q", "-p", "org-lsp"], cwd=os.path.join(ROOT, "server"), check=True)
    os.makedirs(OUT, exist_ok=True)
    for name in names:
        print(f"== {name}")
        SCENES[name]()


if __name__ == "__main__":
    main()
