"""Write server/org-core/src/lint_tables.rs from Emacs: the tables
`org-lint` reads (Babel's header arguments, every language's own, the
export options), with every Babel language loaded and the default export
backends, as `scripts/lint_reference.py` runs Emacs.

    ORG_LISP_DIR=/path/to/org-mode/lisp python3 scripts/update-lint-tables.py
"""

import json
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def rust_string(text):
    return json.dumps(text, ensure_ascii=False)


def values(value):
    if value == ":any":
        return "Values::Any"
    if value is None:
        return "Values::Unset"
    groups = ", ".join("&[" + ", ".join(rust_string(v) for v in group) + "]" for group in value)
    return f"Values::Groups(&[{groups}])"


def args(entries):
    return "&[\n" + "".join(f"        ({rust_string(name)}, {values(value)}),\n" for name, value in entries) + "    ]"


def main():
    org_path = ["-L", os.environ["ORG_LISP_DIR"]] if os.environ.get("ORG_LISP_DIR") else []
    with tempfile.TemporaryDirectory() as home:
        out = subprocess.run(
            ["emacs", "--batch", "-Q", *org_path, "-l", os.path.join(ROOT, "scripts", "lint-tables.el")],
            check=True,
            capture_output=True,
            text=True,
            env=dict(os.environ, HOME=home),
        ).stdout
    tables = json.loads(out)
    common = dict((name.lstrip(":"), value) for name, value in [])
    lines = [
        "//! Tables `org-lint` reads that Org keeps in Lisp: Babel's header",
        "//! arguments and each language's own, the export options of the",
        "//! default export backends. Written by `scripts/update-lint-tables.py`;",
        "//! do not edit.",
        "",
        "/// The values a header argument takes.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum Values {",
        "    /// Any value (`:any`).",
        "    Any,",
        "    /// None given (`(no-expand)`).",
        "    Unset,",
        "    /// Groups of values, one of each group at most; `:any` in a group",
        "    /// lets any value stand for it.",
        "    Groups(&'static [&'static [&'static str]]),",
        "}",
        "",
        "/// `org-babel-common-header-args-w-values`",
        f"pub const COMMON_HEADER_ARGS: &[(&str, Values)] = {args(tables['common'])};",
        "",
        "/// `org-babel-load-languages`, every language loaded.",
        "pub const LANGUAGES: &[&str] = &[" + ", ".join(rust_string(l) for l in tables["languages"]) + "];",
        "",
        "/// `org-babel-header-args:LANG`: the header arguments of a language.",
        "pub const LANGUAGE_HEADER_ARGS: &[(&str, &[(&str, Values)])] = &[",
    ]
    for language, entries in tables["header_args"].items():
        lines.append(f"    ({rust_string(language)}, {args(entries)}),")
    lines += [
        "];",
        "",
        "/// The `#+OPTIONS:` items of `org-export-options-alist` and of the",
        "/// registered backends.",
        "pub const OPTIONS_ITEMS: &[&str] = &[" + ", ".join(rust_string(o) for o in tables["options_items"]) + "];",
        "",
        "/// The keywords of `org-export-options-alist`.",
        "pub const COMMON_OPTIONS: &[&str] = &[" + ", ".join(rust_string(o) for o in tables["common_options"]) + "];",
        "",
        "/// The keywords of the registered backends' options, and their",
        "/// backends.",
        "pub const BACKEND_OPTIONS: &[(&str, &[&str])] = &[",
    ]
    for keyword, backends in tables["backend_options"]:
        lines.append(f"    ({rust_string(keyword)}, &[" + ", ".join(rust_string(b) for b in backends) + "]),")
    lines += [
        "];",
        "",
        "/// `org-default-properties`",
        "pub const DEFAULT_PROPERTIES: &[&str] = &[" + ", ".join(rust_string(p) for p in tables["default_properties"]) + "];",
        "",
    ]
    path = os.path.join(ROOT, "server", "org-core", "src", "lint_tables.rs")
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    subprocess.run(["rustfmt", "--edition", "2021", path], check=True)
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
