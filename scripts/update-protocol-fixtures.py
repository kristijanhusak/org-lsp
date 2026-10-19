#!/usr/bin/env python3
"""Write tests/protocol/cases.json: org-protocol URLs with what Emacs makes
of them (see protocol_reference.py).

    ORG_LISP_DIR=/path/to/org-mode/lisp scripts/update-protocol-fixtures.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from protocol_reference import reference, save  # noqa: E402

WORG = {
    "name": "Worg",
    "base_url": "https://orgmode.org/worg/",
    "working_directory": "$DIR/worg/",
    "online_suffix": ".html",
    "working_suffix": ".org",
}
NOTES = {
    "name": "localhost org-notes/",
    "base_url": "http://localhost/org/",
    "working_directory": "$DIR/notes/",
    "online_suffix": ".html",
    "working_suffix": ".org",
    "rewrites": [["\\(org/?\\)$", "index.org"], ["\\(blog/\\)[0-9]+/", "blog.org"]],
}
GROK = {"name": "OpenGrok", "base_url": "https://grok.example.com/xref/emacs/", "working_directory": "$DIR/emacs/"}
FILES = ["worg/index.org", "worg/org-tutorials/index.org", "worg/a b.org", "notes/index.org", "notes/blog.org",
         "emacs/lisp/simple.el", "emacs/src/"]
PROJECTS = {"options": {"projects": [WORG, NOTES, GROK]}, "files": FILES}

CASES = [
    # store-link, new style.
    {"url": "org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org%20mode"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F&title=Org+mode+%2B+more"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Forgmode.org%2F"},
    {"url": "org-protocol://store-link?title=No+URL"},
    {"url": "org-protocol://store-link?url=https://orgmode.org//manual/&title=x"},
    {"url": "org-protocol://store-link?url=https://orgmode.org/a/../b/./c&title=dots"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com%2Fsearch%3Fq%3Da%26b&title=Q%26A"},
    {"url": "org-protocol://store-link?url=http%3A%2F%2Fx.org%2F%22quoted%22%5C&title=back%5Cslash"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=Caf%C3%A9+%E2%82%AC+%F0%9F%98%80"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=Latin%E9%20one"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=%C3%A9%E9"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=%zz%1z"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=a%2"},
    {"url": "org-protocol://store-link?url=x&title=one&title=two"},
    {"url": "org-protocol://store-link?url=x&title=a=b=c"},
    {"url": "org-protocol://store-link?url=x&flag"},
    {"url": "org-protocol://store-link?"},
    {"url": "org-protocol://store-link?url="},
    {"url": "org-protocol://store-link/?url=https%3A%2F%2Fexample.com"},
    {"url": "org-protocol:/store-link?url=https%3A%2F%2Fexample.com"},
    {"url": "org-protocol:///store-link?url=https%3A%2F%2Fexample.com"},
    {"url": "ORG-PROTOCOL://Store-Link?url=https%3A%2F%2Fexample.com&title=Case"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=Trailing/"},
    {"url": "org-protocol://store-link?url=HTTPS:%2F%2FExample.com%2F%2Fx&title=t"},
    {"url": "org-protocol://store-link?url=mailto%3Ame%40example.com&title=Mail"},
    {"url": "org-protocol://store-link?url=https%3A%2F%2Fexample.com&title=line%0Abreak"},
    # store-link, old style.
    {"url": "org-protocol://store-link:/https:%2F%2Forgmode.org%2F/Org%20mode"},
    {"url": "org-protocol://store-link://https%3A%2F%2Forgmode.org/Org%20mode"},
    {"url": "org-protocol://store-link:/https:/orgmode.org/manual/Title"},
    {"url": "org-protocol://store-link:/https%3A%2F%2Fexample.com"},
    {"url": "org-protocol://store-link:/https%3A%2F%2Fexample.com/Title/key/value/odd"},
    {"url": "org-protocol://store-link:/https%3A%2F%2Fexample.com?Title?More"},
    {"url": "org-protocol://store-link:/"},
    # capture, new style.
    {"url": "org-protocol://capture?template=x&url=https%3A%2F%2Forgmode.org%2F&title=Org+mode&body=Some+text"},
    {"url": "org-protocol://capture?url=https%3A%2F%2Forgmode.org%2F&title=Org+mode&body=Some+text"},
    {"url": "org-protocol://capture?url=https%3A%2F%2Forgmode.org%2F&title=Org",
     "options": {"default_template_key": "w"}},
    {"url": "org-protocol://capture?template=b&url=https%3A%2F%2Forgmode.org%2F",
     "options": {"default_template_key": "w"}},
    {"url": "org-protocol://capture?template=x&title=Only+a+title&body=Line+one%0ALine+two"},
    {"url": "org-protocol://capture?template=x&url=https%3A%2F%2Fexample.com&title=%20%20"},
    {"url": "org-protocol://capture?template=x&url=https%3A%2F%2Fexample.com&title=%5Bbrackets%5D"},
    {"url": "org-protocol://capture?template=x&url=https%3A%2F%2Fexample.com%2F%5Bx%5D&title=a%5D%5Db"},
    {"url": "org-protocol://capture?template=x&url=&title=Empty"},
    {"url": "org-protocol://capture?template=x&url=%20&title="},
    {"url": "org-protocol://capture?template=x&url=file%3A%2Fhome%2Fme%2Fnotes.org&title=Notes"},
    {"url": "org-protocol://capture?template=x&url=no-scheme&title=Plain"},
    {"url": "org-protocol://capture?template=xy&url=https%3A%2F%2Fexample.com&extra=1"},
    {"url": "org-protocol://capture?body=Only+a+body"},
    {"url": "org-protocol://capture?template=x&url=https%3A%2F%2Fexample.com&title=t&body=org-protocol%3A%2F%2Fcapture%3Fx"},
    {"url": "org-protocol://capture?url=a&title=org-protocol://capture?url=b"},
    {"url": "org-protocol://store-link?url=a&title=org-protocol://capture:/x/y"},
    {"url": "org-protocol://capture?url=a&x=org-protocol://capture:/b"},
    {"url": "org-protocol://capture?url=x/../y"},
    {"url": "org-protocol://capture?url=x/.."},
    {"url": "org-protocol://capture?url=x/../../.."},
    # capture, old style.
    {"url": "org-protocol://capture:/x/https%3A%2F%2Forgmode.org%2F/Org%20mode/Some%20text"},
    {"url": "org-protocol://capture:/https%3A%2F%2Forgmode.org%2F/Org%20mode/Some%20text"},
    {"url": "org-protocol://capture://x/https%3A%2F%2Forgmode.org%2F/Org/Body/k/v"},
    {"url": "org-protocol://capture:/x/https:/orgmode.org/Org", "options": {"default_template_key": "w"}},
    {"url": "org-protocol://capture:/x"},
    {"url": "org-protocol://capture:/"},
    {"url": "org-protocol://capture:/x/u/t/b", "options": {"data_separator": "|"}},
    {"url": "org-protocol://capture:/x|u|t|b", "options": {"data_separator": "|"}},
    {"url": "org-protocol://capture:/x;u;;b", "options": {"data_separator": ";"}},
    {"url": "org-protocol://capture:/x/u/t", "options": {"data_separator": "x*"}},
    # Neither.
    {"url": "org-protocol://unknown?url=x"},
    {"url": "org-protocol:capture?url=x"},
    {"url": "org-protocol://capturex?url=x"},
    {"url": "org-protocol://capture"},
    {"url": "org-protocol://"},
    {"url": "org-protocol://capture?url=a/../../../org-protocol://store-link?url=b"},
    # open-source.
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Findex.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Forg-tutorials%2Findex.html%23top",
     **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Fmissing.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Fa%20b.html?q=1", **PROJECTS},
    {"url": "org-protocol://open-source?url=HTTPS%3A%2F%2FOrgMode.org%2Fworg%2Findex.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=http%3A%2F%2Flocalhost%2Forg%2F", **PROJECTS},
    {"url": "org-protocol://open-source?url=http%3A%2F%2Flocalhost%2Forg%2Findex.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=http%3A%2F%2Flocalhost%2Forg%2Fblog%2F2024%2Fpost.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=http%3A%2F%2Flocalhost%2Forg%2Fnothing.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fgrok.example.com%2Fxref%2Femacs%2Flisp%2Fsimple.el", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fgrok.example.com%2Fxref%2Femacs%2Fsrc", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fgrok.example.com%2Fxref%2Femacs%2F", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fnowhere.example.com%2F", **PROJECTS},
    {"url": "org-protocol://open-source:/https%3A%2F%2Forgmode.org%2Fworg%2Findex.html", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Findex.html"},
    {"url": "org-protocol://open-source?title=x", **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Forgmode.org%2Fworg%2Fx.html%3Fhttps%3A%2F%2Forgmode.org%2Fworg%2F",
     **PROJECTS},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2Fdoc%2Fa.html%2Fb.html",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "online_suffix": ".html", "working_suffix": ".org"}]}},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2F.html%2Fdoc%2Fa",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "online_suffix": ".html"}]}},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2Fdoc%2Fa",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "rewrites": [["doc/a", "x.org"]]}]}},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2Fdoc%2Fa",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "rewrites": [["\\(z\\)?doc/a", "x.org"]]}]}},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2Fdoc%2Fa%2F%2F",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "rewrites": [["\\(a\\)", "/x.org"], ["never", "y"]]}]}},
    {"url": "org-protocol://open-source?url=https%3A%2F%2Fex.org%2Fdoc%2F%22a%22",
     "options": {"projects": [{"name": "s", "base_url": "https://ex.org/doc/", "working_directory": "$DIR/",
                               "rewrites": [["\"q\"\\(\\\\\\)", "x"]]}]}},
]

if __name__ == "__main__":
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "protocol", "cases.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    save(path, reference(CASES))
    print(f"wrote {len(CASES)} cases to {path}")
