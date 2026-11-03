//! Tangling against Emacs.
//!
//! `tests/tangle/cases.json` holds Org files and what Emacs's
//! `org-babel-tangle` wrote for them (`scripts/update-tangle-fixtures.py`).
//! `ORG_TANGLE_CASES` points the test at other cases.

use std::path::Path;

use org_core::tangle::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

/// The tangling settings a case gives, as the server reads them.
fn settings(given: &Value) -> Settings {
    let defaults = Settings::default();
    let text = |key: &str| given[key].as_str().map(str::to_string);
    Settings {
        preserve_indentation: given["org_src_preserve_indentation"].as_bool().unwrap_or(false),
        lang_exts: given["org_babel_tangle_lang_exts"]
            .as_object()
            .map(|exts| exts.iter().map(|(lang, ext)| (lang.clone(), ext.as_str().unwrap().to_string())).collect())
            .unwrap_or_default(),
        relative_links: given["org_babel_tangle_use_relative_file_links"].as_bool().unwrap_or(true),
        comment_format: (
            text("org_babel_tangle_comment_format_beg").unwrap_or(defaults.comment_format.0),
            text("org_babel_tangle_comment_format_end").unwrap_or(defaults.comment_format.1),
        ),
        uncomment_comments: given["org_babel_tangle_uncomment_comments"].as_bool().unwrap_or(false),
        process_comment_text: text("org_babel_process_comment_text").unwrap_or(defaults.process_comment_text),
        default_file_mode: text("org_babel_tangle_default_file_mode")
            .and_then(|mode| u32::from_str_radix(&mode, 8).ok())
            .unwrap_or(defaults.default_file_mode),
        noweb_wrap: (
            text("org_babel_noweb_wrap_start").unwrap_or(defaults.noweb_wrap.0),
            text("org_babel_noweb_wrap_end").unwrap_or(defaults.noweb_wrap.1),
        ),
    }
}

#[test]
fn tangles_as_emacs_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/tangle/cases.json");
    let path = std::env::var("ORG_TANGLE_CASES").map_or(path, Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let directory = Path::new("/case");
    let mut failures = Vec::new();
    for case in &cases {
        let files = case["files"].as_object().unwrap();
        let mut names: Vec<&String> = files.keys().filter(|name| name.ends_with(".org")).collect();
        names.sort();
        let text = files[names[0].as_str()].as_str().unwrap();
        let settings = settings(&case["settings"]);
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let actual = match tangle::tangle(text, &parse, &directory.join(names[0]), &settings) {
            Ok(outcome) => {
                let relative = |path: &Path| path.strip_prefix(directory).unwrap().display().to_string();
                json!({
                    "files": outcome.files.iter().map(|file| (relative(&file.path), json!(file.text))).collect::<serde_json::Map<_, _>>(),
                    "modes": outcome.files.iter().filter_map(|file| file.mode.filter(|mode| *mode != 0o644).map(|mode| (relative(&file.path), json!(format!("{mode:o}"))))).collect::<serde_json::Map<_, _>>(),
                    "message": outcome.message(names[0]),
                })
            }
            Err(error) => json!({ "error": error }),
        };
        if actual != case["expected"] {
            failures.push(format!(
                "\n{}:\nexpected: {}\nactual:   {}",
                case["name"].as_str().unwrap_or("?"),
                serde_json::to_string_pretty(&case["expected"]).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap()
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:{}", failures.len(), cases.len(), failures.join("\n"));
}

#[test]
fn refuses_a_block_that_refers_to_itself() {
    let text = "#+name: a\n#+begin_src sh :noweb yes :tangle out.sh\n<<a>>\n#+end_src\n";
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    let tangled = tangle::tangle(text, &parse, Path::new("/case/a.org"), &Settings::default());
    assert_eq!(tangled.err().as_deref(), Some("Lisp nesting exceeds `max-lisp-eval-depth'"));
}

/// A block tangled, and under it a chain of `references` named blocks,
/// each `<<cN>>` the next one; the last is `end`.
fn chain(references: usize) -> String {
    let mut text = "#+begin_src sh :tangle out.sh :noweb yes\n<<c1>>\n#+end_src\n".to_string();
    for n in 1..=references {
        let body = if n < references { format!("<<c{}>>", n + 1) } else { "end".to_string() };
        text.push_str(&format!("#+name: c{n}\n#+begin_src sh :noweb yes\n{body}\n#+end_src\n"));
    }
    text
}

fn tangled(text: &str) -> Result<String, String> {
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    tangle::tangle(text, &parse, Path::new("/case/a.org"), &Settings::default())
        .map(|outcome| outcome.files.into_iter().map(|file| file.text).collect())
}

/// Emacs's Lisp stack (`max-lisp-eval-depth`) holds a chain of 524
/// references, and says it is full past them; a longer one used to
/// overflow the server's stack.
#[test]
fn refuses_references_deeper_than_emacs_goes() {
    assert_eq!(tangled(&chain(524)).as_deref(), Ok("end\n"));
    for references in [525, 30_000] {
        let refused = tangled(&chain(references)).unwrap_err();
        assert_eq!(refused, "Lisp nesting exceeds `max-lisp-eval-depth'", "{references}");
    }
}

/// References to a block twice over, at each level: each block is
/// expanded once, and past [`tangle::LARGEST`] bytes the expansion stops.
#[test]
fn refuses_references_that_expand_past_the_largest() {
    let doubling = |levels: usize, last: &str| {
        let mut text = "#+begin_src sh :tangle out.sh :noweb yes\n<<b1>>\n#+end_src\n".to_string();
        for n in 1..=levels {
            let body = if n < levels { format!("<<b{0}>><<b{0}>>", n + 1) } else { last.to_string() };
            text.push_str(&format!("#+name: b{n}\n#+begin_src sh :noweb yes\n{body}\n#+end_src\n"));
        }
        text
    };
    // 2^7 times the last block.
    assert_eq!(tangled(&doubling(8, "ab")), Ok(format!("{}\n", "ab".repeat(128))));
    // 2^59 times an empty one: nothing, at once.
    assert_eq!(tangled(&doubling(60, "")).as_deref(), Ok("\n"));
    // 2^39 times ten bytes.
    let refused = tangled(&doubling(40, "0123456789")).unwrap_err();
    assert_eq!(refused, "Noweb references expand to more than 64 MiB");
}

#[test]
fn leaves_a_comment_function_of_the_users_to_emacs() {
    let text = "* H\ntext\n#+begin_src sh :tangle out.sh :comments org\necho 1\n#+end_src\n";
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    let settings = Settings { process_comment_text: "my-comment-filter".to_string(), ..Settings::default() };
    let refused = tangle::tangle(text, &parse, Path::new("/case/a.org"), &settings).unwrap_err();
    assert!(tangle::needs_emacs(&refused), "{refused}");
}
