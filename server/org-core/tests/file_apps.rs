//! `org-file-apps`: what `org-open-file` opens a link's file with, as
//! Emacs picked it for the cases of `tests/file-apps/cases.json`
//! (`scripts/file_apps_reference.py`). The system's application stands
//! for `xdg-open %s` there.

use org_core::file_apps::{open, shell_quote, FileApps, Opener, Prefix, Target};
use serde_json::{json, Value};

fn apps(value: &Value) -> FileApps {
    let Some(pairs) = value.as_array() else { return FileApps::default() };
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|pair| (pair[0].as_str().unwrap().to_string(), pair[1].as_str().unwrap().to_string()))
        .collect();
    FileApps::from_pairs(&pairs)
}

#[test]
fn files_open_as_emacs_opens_them() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/file-apps/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let dir = "{dir}";
    let existing = ["a.pdf", "b.html", "c.txt", "D.PDF", "e.tar.gz", "notes.org", "x.xyz", "it's here.pdf", "sub"];
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["file"].as_str().unwrap();
        let directory = name == "sub" || name == "wiki";
        // `org-open-directory-means-index-dot-org`, as the server does it:
        // the directory's `index.org`, still a directory's.
        let index = directory && case["vars"]["org-open-directory-means-index-dot-org"] == true;
        let file = if index { format!("{dir}/{name}/index.org") } else { format!("{dir}/{name}") };
        let target = Target {
            file: &file,
            search: case.get("search").and_then(Value::as_str),
            exists: if index { name == "wiki" } else { existing.contains(&name) },
            directory,
        };
        let prefix = match case.get("prefix").and_then(Value::as_u64) {
            Some(4) => Prefix::Editor,
            Some(16) => Prefix::System,
            _ => Prefix::None,
        };
        let non_existing = case["vars"]["org-open-non-existing-files"].as_bool().unwrap_or(false);
        let got = match open(&apps(&case["apps"]), &target, prefix, non_existing) {
            Ok(Opener::Editor) => json!({ "emacs": true }),
            Ok(Opener::System) => json!({ "command": format!("xdg-open {}", shell_quote(&file)) }),
            Ok(Opener::Command(command)) => json!({ "command": command }),
            Err(message) => json!({ "error": message }),
        };
        // The braces of `{dir}` are quoted in a command as a file name's.
        let got = Value::from(got.to_string().replace(r"\\{dir\\}", "{dir}"));
        let expected = Value::from(case["expected"].to_string());
        if got != expected {
            failures.push(format!(
                "{} {name} {:?} {:?}: {got}, Emacs {expected}",
                case["apps"],
                case.get("search"),
                case.get("prefix")
            ));
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
