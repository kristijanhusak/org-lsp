#!/usr/bin/env python3
"""Differential fuzzing of org-protocol URLs against Emacs
(`org-protocol.el`).

Random URLs: the three handlers, in the new style and the old, with runs of
slashes, `.` and `..`, case changes, percent escapes (UTF-8, single bytes,
broken ones, `%zz`), `+`, `=` and `&` in odd places, missing values, the
protocol twice; the default template key and data separator at times; for
open-source, URLs under the projects of `org-protocol-project-alist`, with
suffixes, queries, fragments and rewrites, among files that are there or
not.

    scripts/fuzz-protocol.py [--count 500] [--seed 1] [--out FILE]

Requires Emacs and cargo.
"""

import argparse
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from protocol_reference import reference, save  # noqa: E402

ROOT = os.path.dirname(HERE)
ESCAPES = ["%20", "%2F", "%3A", "%3F", "%26", "%3D", "%2B", "%25", "%C3%A9", "%E2%82%AC", "%F0%9F%98%80", "%E9",
           "%C3", "%zz", "%1z", "%0A", "%5B", "%5D", "%22", "%5C", "%C3%A9%E9", "%e2%82", "%2f%2F"]
WORDS = ["Org", "mode", "x", "a", "b", "title", "Café", "https:", "http:", "//", "/", "///", "example.com", ".", "..",
         "?", "#", "+", "=", "&", " ", "~", "org-protocol:", "capture", "store-link", "HTTPS:", "mailto:", "[", "]",
         "\\", "\"", "index.html", "org", "worg"]
KEYS = ["url", "title", "body", "template", "URL", "x", "", "u rl"]
SEPARATORS = ["/+\\|\\?", "|", ";", "/", "[/;]+", "x*", "\\?"]
WORG = {"name": "Worg", "base_url": "https://orgmode.org/worg/", "working_directory": "$DIR/worg/",
        "online_suffix": ".html", "working_suffix": ".org"}
NOTES = {"name": "notes", "base_url": "http://localhost/org/", "working_directory": "$DIR/notes/",
         "online_suffix": ".html", "working_suffix": ".org",
         "rewrites": [["\\(org/?\\)$", "index.org"], ["\\(blog/\\)[0-9]+/", "blog.org"]]}
SRC = {"name": "src", "base_url": "https://src.example.com/x/", "working_directory": "$DIR/src/"}
FILES = ["worg/index.org", "worg/a.org", "worg/sub/b.org", "notes/index.org", "notes/blog.org", "src/main.rs",
         "src/lib/"]
PATHS = ["index", "a", "sub/b", "missing", "sub/", "", "blog/2024/p", "main.rs", "lib", "a b", "x.html/y"]


def word(rng):
    return rng.choice(WORDS + ESCAPES) if rng.random() < 0.8 else rng.choice(ESCAPES)


def text(rng, most=4):
    return "".join(word(rng) for _ in range(rng.randint(0, most)))


def encoded_url(rng):
    scheme = rng.choice(["https%3A%2F%2F", "https:%2F%2F", "https://", "http%3A%2F%2F", "file%3A", "", "HTTPS%3A%2F%2F"])
    return scheme + rng.choice(["orgmode.org", "example.com", "x"]) + "".join(
        rng.choice(["%2F", "/", "//", "%2Fa", "/b", "/./", "/../", "%3Fq%3D1", "?q=1", "#f", "%23f", "%2F%2F"])
        for _ in range(rng.randint(0, 3)))


def query(rng):
    pairs = []
    for _ in range(rng.randint(0, 4)):
        key = rng.choice(KEYS)
        if key == "url" or (key == "URL" and rng.random() < 0.5):
            value = encoded_url(rng)
        elif key == "template":
            value = rng.choice(["x", "w", "ab", "", "%78"])
        else:
            value = text(rng)
        pairs.append(key if rng.random() < 0.05 else f"{key}={value}")
    if rng.random() < 0.8 and not any(pair.startswith("url=") for pair in pairs):
        pairs.insert(rng.randint(0, len(pairs)), "url=" + encoded_url(rng))
    return "&".join(pairs)


def old_style(rng, handler):
    parts = []
    if handler == "capture" and rng.random() < 0.6:
        parts.append(rng.choice(["x", "w", "ab"]))
    parts.append(encoded_url(rng).replace("/", "%2F") if rng.random() < 0.7 else encoded_url(rng))
    parts += [text(rng, 2) for _ in range(rng.randint(0, 4))]
    return rng.choice(["/", "/", "//", "?"]).join(parts)


def source_url(rng):
    base = rng.choice(["https://orgmode.org/worg/", "http://localhost/org/", "https://src.example.com/x/",
                       "https://elsewhere.org/", "HTTPS://OrgMode.org/worg/"])
    path = rng.choice(PATHS)
    suffix = rng.choice([".html", ".html", "", ".php", ".html.html"])
    tail = rng.choice(["", "", "?q=1", "#top", "?a=.html", "/"])
    url = base + path + suffix + tail
    if rng.random() < 0.8:
        return "".join(c if c.isalnum() or c in "-_." else "%%%02X" % ord(c) for c in url)
    return url


def case(rng):
    handler = rng.choice(["capture", "store-link", "open-source", "capture", "store-link"])
    name = handler if rng.random() < 0.9 else rng.choice([handler.upper(), handler.title(), "unknown", handler + "x"])
    protocol = rng.choice(["org-protocol://", "org-protocol://", "org-protocol:/", "org-protocol:///", "ORG-PROTOCOL://"])
    out = {}
    if handler == "open-source" and rng.random() < 0.8:
        out["options"] = {"projects": rng.sample([WORG, NOTES, SRC], rng.randint(1, 3))}
        out["files"] = FILES
        rest = ("?url=" + source_url(rng)) if rng.random() < 0.8 else (rng.choice([":/", "://"]) + source_url(rng))
    elif rng.random() < 0.65:
        rest = rng.choice(["?", "?", "/?", "//?"]) + query(rng)
    else:
        rest = rng.choice([":/", ":/", "://", ":"]) + old_style(rng, handler)
    if rng.random() < 0.1:
        rest += rng.choice(["/", "//", "/.", "/..", "&", "&org-protocol://capture?url=y"])
    if rng.random() < 0.2:
        out.setdefault("options", {})["default_template_key"] = rng.choice(["w", "x", "ab"])
    if rng.random() < 0.15:
        out.setdefault("options", {})["data_separator"] = rng.choice(SEPARATORS)
    out["url"] = protocol + name + rest
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=500)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    cases = reference([case(rng) for _ in range(args.count)], timeout=60 + args.count)
    out = args.out or os.path.join(tempfile.mkdtemp(prefix="org-protocol-fuzz-"), "cases.json")
    save(out, cases)
    result = subprocess.run(
        ["cargo", "test", "-q", "-p", "org-core", "--test", "protocol"],
        cwd=os.path.join(ROOT, "server"), env=dict(os.environ, ORG_PROTOCOL_CASES=out), capture_output=True, text=True,
    )
    print(f"{args.count} cases; saved to {out}")
    if result.returncode != 0:
        report = result.stdout + result.stderr
        start = report.find("cases differ")
        end = report.find("stack backtrace", start)
        print(report[max(0, start - 20):end if end >= 0 else None].rstrip()[:12000])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
