"""Zed as install.sh zed leaves it (a home made by install_zed.sh, the
nightly server beside the extension): an Org file opened from the file
finder, ctrl-c a, the button of the week's agenda clicked. Screenshots in
shots/."""
import os
import subprocess
import sys
import time

HOME = sys.argv[1]
HERE = os.path.dirname(os.path.abspath(__file__))
SHOTS = os.path.join(HERE, "shots")
os.makedirs(SHOTS, exist_ok=True)
display = ":87"
work = os.path.join(HOME, "notes")
os.makedirs(work, exist_ok=True)
with open(os.path.join(work, "work.org"), "w") as out:
    out.write("* TODO Write report\nSCHEDULED: <2026-05-18 Mon>\n")
env = {"HOME": HOME, "PATH": "/usr/bin:/bin", "DISPLAY": display, "XDG_RUNTIME_DIR": HOME, "SHELL": "/bin/sh",
       "ZED_ALLOW_EMULATED_GPU": "1", "ZED_ALLOW_ROOT": "true", "LANG": "C.UTF-8",
       "VK_ICD_FILENAMES": "/usr/share/vulkan/icd.d/lvp_icd.json", "ORG_LSP_CONFIG": "/dev/null"}
xvfb = subprocess.Popen(["Xvfb", display, "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
time.sleep(2)
zed = subprocess.Popen(["/root/zed/zed.app/bin/zed", "--foreground", work], env=env, cwd=work,
                       stdout=open(os.path.join(HOME, "zed.log"), "w"), stderr=subprocess.STDOUT, start_new_session=True)


def x(*args):
    return subprocess.run(list(args), env=dict(os.environ, DISPLAY=display), capture_output=True, text=True).stdout


try:
    window = None
    for _ in range(60):
        out = x("xdotool", "search", "--onlyvisible", "--name", ".").split()
        if out:
            window = out[0]
            break
        time.sleep(1)
    x("xdotool", "windowsize", window, "1280", "800")
    x("xdotool", "windowmove", window, "0", "0")
    time.sleep(3)
    for _ in range(5):
        x("xdotool", "key", "--window", window, "ctrl+p")
        time.sleep(1)
        x("xdotool", "type", "--window", window, "work.org")
        time.sleep(1.5)
        x("xdotool", "key", "--window", window, "Return")
        time.sleep(3)
        if "work.org" in x("xdotool", "getwindowname", window):
            break
        x("xdotool", "key", "--window", window, "Escape")
    time.sleep(8)
    x("import", "-window", "root", os.path.join(SHOTS, "installed-1.png"))
    x("xdotool", "key", "--window", window, "--delay", "40", "ctrl+c", "a")
    time.sleep(4)
    x("import", "-window", "root", os.path.join(SHOTS, "installed-2.png"))
    print(window, x("xdotool", "getwindowname", window))
finally:
    for process in (zed, xvfb):
        try:
            os.killpg(process.pid, 15)
        except ProcessLookupError:
            pass
