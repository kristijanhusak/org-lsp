"""Show timestamps in Emacs with `org-display-custom-times` and record
what each one shows (`org-display-custom-time`): the ranges of the text
covered and the string in their place.

A case is a JSON object:

    {
      "text": "<2026-10-05 Mon>",           an Org document
      "formats": ["%d.%m.%Y", "%d.%m.%Y %H:%M"]   `org-timestamp-custom-formats`
                                            (Org's default when left out)
    }

`reference(cases)` adds `expected`: a list of `[start, end, shown]`,
start and end byte offsets. Set ORG_LISP_DIR to the `lisp` directory of
an Org checkout to use that version instead of the one bundled with Emacs.
"""

import json
import os
import subprocess
import tempfile

DRIVER = r"""
(require 'org)
(require 'json)
(defun org-lsp-custom-times-case (in out formats)
  (with-temp-buffer
    (insert-file-contents in)
    (org-mode)
    (let ((org-display-custom-times t)
          (org-timestamp-custom-formats (or formats org-timestamp-custom-formats))
          (found nil))
      (goto-char (point-min))
      (while (org-activate-dates (point-max)))
      (let ((pos (point-min)))
        (while (< pos (point-max))
          (let ((next (next-single-property-change pos 'display nil (point-max)))
                (shown (get-text-property pos 'display)))
            (when (stringp shown)
              (push (vector (1- pos) (1- next) (substring-no-properties shown)) found))
            (setq pos next))))
      (with-temp-file out (insert (json-encode (vconcat (nreverse found))))))))
"""


def lisp_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, cases):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=1)
        handle.write("\n")


def to_bytes(text, offset):
    return len(text[:offset].encode("utf-8"))


def reference(cases):
    """Fill in `expected` for every case."""
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as directory:
        forms = [DRIVER]
        outputs = []
        for index, case in enumerate(cases):
            source = os.path.join(directory, f"in-{index}.org")
            target = os.path.join(directory, f"out-{index}.json")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write(case["text"])
            outputs.append((case, target))
            formats = case.get("formats")
            lisp = f"(cons {lisp_string(formats[0])} {lisp_string(formats[1])})" if formats else "nil"
            forms.append(f"(org-lsp-custom-times-case {lisp_string(source)} {lisp_string(target)} {lisp})")
        driver = os.path.join(directory, "driver.el")
        with open(driver, "w", encoding="utf-8") as handle:
            handle.write("\n".join(forms))
        env = dict(os.environ, LC_ALL="C.UTF-8", LANG="C.UTF-8", TZ="UTC")
        subprocess.run(["emacs", "--batch", "-Q", *org_path, "-l", driver], check=True, env=env,
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, timeout=600)
        for case, target in outputs:
            with open(target, encoding="utf-8") as handle:
                found = json.load(handle)
            text = case["text"]
            case["expected"] = [[to_bytes(text, start), to_bytes(text, end), shown] for start, end, shown in found]
    return cases


TEXTS = [
    "<2026-10-05 Mon>",
    "[2026-10-05 Mon 10:30]",
    "* TODO Call\nSCHEDULED: <2026-10-05 Mon 09:00 +1w> DEADLINE: <2026-10-09 Fri -3d>\n",
    "<2026-10-05 Mon>--<2026-10-07 Wed>\n[2026-10-05 Mon 10:00]--[2026-10-05 Mon 12:30]\n",
    "<2026-10-05 Mon 10:00-11:30>\n<2026-10-05 Mon 10:00-11:30 .+2d/4d>\n",
    "CLOCK: [2026-10-05 Mon 09:00]--[2026-10-05 Mon 10:15] =>  1:15\n",
    "<2026-10-05>\n<2026-10-05 10:00>\n<2026-02-29 Sun>\n",
    "[[<2026-10-05 Mon>]] and [<2026-10-05 Mon>] in text <2026-1-5 Mon>\n",
    "Déjà <2026-10-05 lun. 08:05 ++1m> fin\n",
    "<2026-10-05 Mon 9:05>\n",
]
FORMATS = [None, ["%d.%m.%Y", "%d.%m.%Y %H:%M"], ["<%A, %B %-d %Y>", "<%a %b %e %Y %I:%M %p>"], ["[%F]", "%F %R"]]


def default_cases():
    return [{"text": text, "formats": formats} if formats else {"text": text} for formats in FORMATS for text in TEXTS]


if __name__ == "__main__":
    import sys

    ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    PATH = os.path.join(ROOT, "tests", "custom-times", "cases.json")
    cases = default_cases() if "--new" in sys.argv or not os.path.exists(PATH) else load(PATH)
    save(PATH, reference(cases))
    print(f"updated {len(cases)} cases")
