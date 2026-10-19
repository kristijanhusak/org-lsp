//! org-protocol URLs against Emacs.
//!
//! `tests/protocol/cases.json` is written by
//! `scripts/update-protocol-fixtures.py`: a URL, the options of
//! `org-protocol.el`, the files there are, and what Org does with the URL
//! (`org-protocol-check-filename-for-protocol` and its handlers): the link
//! stored, the capture started, the file opened, what it says, or its
//! error. `ORG_PROTOCOL_CASES` names another file of cases (the fuzzer's).

use std::path::{Path, PathBuf};

use org_core::protocol::{self, FileState, Options, Project, Request};
use serde_json::{json, Value};

/// What Org does, in the form the cases record it.
fn outcome(url: &str, options: &Options, directory: &Path) -> Value {
    let mut out = json!({
        "handler": null, "result": null, "stored": [], "kill": null, "capture": null, "messages": [],
    });
    let request = match protocol::parse(url, options) {
        Ok(Some(request)) => request,
        Ok(None) => {
            out["result"] = json!(protocol::expand_file_name(url));
            return out;
        }
        Err(refused) => {
            out["error"] = json!(refused.error);
            out["handler"] = json!(refused.called.then(|| refused.handler.name()));
            if refused.handler == protocol::Handler::Capture {
                out["messages"] = json!(["Greedy org-protocol handler.  Killing client."]);
            }
            return out;
        }
    };
    match request {
        Request::StoreLink { url, title } => {
            out["handler"] = json!("store-link");
            out["stored"] = json!([[url, title]]);
            out["kill"] = json!(url);
            out["messages"] = json!([protocol::store_link_message(&url)]);
        }
        Request::Capture(capture) => {
            out["handler"] = json!("capture");
            // The message of `:kill-client`, which org-lsp does not say:
            // there is no client to kill.
            out["messages"] = json!(["Greedy org-protocol handler.  Killing client.", protocol::CAPTURED]);
            out["stored"] = json!(capture.stored.iter().map(|(url, title)| json!([url, title])).collect::<Vec<_>>());
            let mut plist: Vec<Value> = capture.props.iter().map(|(key, value)| json!([key, value])).collect();
            let query: Vec<Value> = capture.query.iter().map(|(key, value)| json!([key, value])).collect();
            plist.push(json!([":query", query]));
            out["capture"] = json!({ "template": capture.template, "plist": plist });
        }
        Request::OpenSource { url } => {
            out["handler"] = json!("open-source");
            let state = |name: &str| {
                let path = match name.strip_prefix("~/") {
                    Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
                    None => directory.join(name),
                };
                match std::fs::metadata(&path) {
                    Ok(_) => FileState::Readable,
                    Err(_) => FileState::Missing,
                }
            };
            let opened = protocol::open_source(&url, &options.projects, &state);
            out["result"] = json!(opened.file);
            out["messages"] = json!(opened.messages);
            if let Some(error) = opened.error {
                out["error"] = json!(error);
            }
        }
    }
    out
}

fn options(case: &Value, directory: &str) -> Options {
    let given: Value =
        serde_json::from_str(&case["options"].to_string().replace("$DIR", directory)).unwrap_or(Value::Null);
    let text = |value: &Value| value.as_str().map(str::to_string);
    Options {
        default_template_key: text(&given["default_template_key"]),
        data_separator: text(&given["data_separator"]),
        projects: given["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|project| Project {
                name: text(&project["name"]).unwrap_or_default(),
                base_url: text(&project["base_url"]).unwrap_or_default(),
                working_directory: text(&project["working_directory"]).unwrap_or_default(),
                online_suffix: text(&project["online_suffix"]),
                working_suffix: text(&project["working_suffix"]),
                rewrites: project["rewrites"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|rewrite| (text(&rewrite[0]).unwrap_or_default(), text(&rewrite[1]).unwrap_or_default()))
                    .collect(),
            })
            .collect(),
    }
}

#[test]
fn reads_urls_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/protocol/cases.json"));
    let path = std::env::var("ORG_PROTOCOL_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scratch = std::env::temp_dir().join(format!("org-core-protocol-{}", std::process::id()));
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let directory = scratch.join(format!("case-{index}"));
        std::fs::create_dir_all(&directory).unwrap();
        for name in case["files"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let path = directory.join(name);
            if name.ends_with('/') {
                std::fs::create_dir_all(&path).unwrap();
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "").unwrap();
            }
        }
        let shown = directory.display().to_string();
        let got = outcome(case["url"].as_str().unwrap(), &options(case, &shown), &directory);
        let got: Value = serde_json::from_str(&got.to_string().replace(&shown, "$DIR")).unwrap();
        if got != case["expected"] {
            failures.push(format!("{}\n  expected {}\n  got      {}", case["url"], case["expected"], got));
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
