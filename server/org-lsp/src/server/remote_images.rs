//! The images of `http:` and `https:` links (`org-display-remote-inline-images`):
//! downloaded into a private directory of the server and shown as the
//! images of local files are, once `org-resource-download-policy` and
//! `org-safe-remote-resources` allow it or the answer to
//! `org--confirm-resource-safe`'s question does. That question is an input
//! whose answer runs `org.link.preview` again; the answers so far go with
//! it, so that each URL is asked about once.
//!
//! As Emacs's `download` reads the file anew each time it shows, so the
//! image is downloaded each time here; `cache`, which keeps the file in a
//! buffer, keeps it here for the session. The regexps the answers `!`, `d`
//! and `f` add to `org-safe-remote-resources` (by `customize-push-and-save`
//! in Emacs) are kept for the session, not saved.

use std::collections::HashMap;
use std::path::PathBuf;

use lsp_types::{MessageType, Uri};
use org_core::link_preview::Preview;
use org_core::remote_resource::{self, Fetch, Policy};
use serde_json::{json, Value};

use super::{Context, Private, Server};
use crate::config::RemoteImages;
use crate::uri;

/// What the server knows of remote images.
pub(super) struct Remote {
    /// `org-display-remote-inline-images`
    pub(super) mode: RemoteImages,
    /// `org-resource-download-policy`
    pub(super) policy: Policy,
    /// `org-safe-remote-resources`, and the regexps the answers added.
    pub(super) safe: Vec<String>,
    /// The file of each URL downloaded, with `cache`.
    cache: HashMap<String, PathBuf>,
    /// Where the images are downloaded to.
    pub(super) directory: Private,
    /// How many were downloaded, which names the next one.
    downloads: u64,
}

impl Remote {
    pub(super) fn new(mode: RemoteImages, policy: Policy, safe: Vec<String>) -> Remote {
        Remote { mode, policy, safe, cache: HashMap::new(), directory: Private::new("images"), downloads: 0 }
    }
}

impl Server {
    /// The files of the remote images among `found` of the file `uri`, by
    /// the start of their link, those that may not be downloaded left
    /// out; or the question to ask first, as `{ "input" }`. When the file
    /// opens (`startup`) nothing is asked, and what would be asked about is
    /// left out.
    pub(super) fn remote_images(
        &mut self,
        uri: &Uri,
        found: &[Preview],
        context: &Context,
        startup: bool,
    ) -> Result<HashMap<usize, PathBuf>, Value> {
        let mut files = HashMap::new();
        if self.remote.mode == RemoteImages::Skip || !found.iter().any(|found| found.remote) {
            return Ok(files);
        }
        // The file asking, as `file-truename` names it.
        let file =
            uri::to_path(uri).map(|path| std::fs::canonicalize(&path).unwrap_or(path).to_string_lossy().into_owned());
        let file = file.as_deref();
        // The answers so far, and the one just given.
        let mut answers: serde_json::Map<String, Value> =
            context.answers.as_deref().and_then(|answers| serde_json::from_str(answers).ok()).unwrap_or_default();
        if let (Some(asked), Some(input)) = (context.step.as_deref(), context.input.as_deref()) {
            let key = input.chars().next().unwrap_or('n');
            if let Some(pattern) = remote_resource::safe_pattern(key, asked, file) {
                if !self.remote.safe.contains(&pattern) {
                    self.remote.safe.push(pattern);
                }
            }
            answers.insert(asked.to_string(), json!(key.to_string()));
        }
        // What may be downloaded, before any is.
        let mut wanted = Vec::new();
        let mut refused = Vec::new();
        for found in found.iter().filter(|found| found.remote) {
            let url = found.path.as_str();
            let fetch = match remote_resource::should_fetch(self.remote.policy, url, file, &self.remote.safe) {
                Fetch::Ask => match answers.get(url).and_then(Value::as_str).and_then(|key| key.chars().next()) {
                    Some(key) => remote_resource::answer_fetches(key),
                    None if startup => continue,
                    None => return Err(self.remote_question(url, file, &answers)),
                },
                Fetch::Yes => true,
                Fetch::No => false,
            };
            if fetch {
                wanted.push((found.start, url));
            } else if !refused.contains(&url) {
                refused.push(url);
            }
        }
        for url in refused {
            if !startup {
                // What `org-file-contents` says of a resource it does not download.
                self.show_message(
                    MessageType::INFO,
                    format!("The remote resource {url:?} is considered unsafe, and will not be downloaded."),
                );
            }
        }
        let mut fetched: HashMap<&str, Option<PathBuf>> = HashMap::new();
        for (start, url) in wanted {
            let path = match fetched.get(url) {
                Some(path) => path.clone(),
                None => {
                    let path = self.download_image(url);
                    if path.is_none() && !startup {
                        self.show_message(MessageType::INFO, format!("Unable to fetch file from {url:?}"));
                    }
                    fetched.insert(url, path.clone());
                    path
                }
            };
            if let Some(path) = path {
                files.insert(start, path);
            }
        }
        Ok(files)
    }

    /// `org--confirm-resource-safe`'s question about `url`, as an input of
    /// kind `key`: the first line of its buffer and its prompt, the keys
    /// it lists with what each does.
    fn remote_question(&self, url: &str, file: Option<&str>, answers: &serde_json::Map<String, Value>) -> Value {
        let question = remote_resource::confirm(url, file);
        let first = question.text.lines().next().unwrap_or_default();
        let choices: Vec<Value> =
            question.choices.iter().map(|(key, label)| json!({ "key": key.to_string(), "label": label })).collect();
        json!({ "input": {
            "kind": "key",
            "prompt": format!("{first} {}", question.prompt),
            "value": "",
            "choices": choices,
            "context": { "step": url, "answers": Value::Object(answers.clone()).to_string() },
        } })
    }

    /// The image of `url` as a file: the one downloaded before with
    /// `cache`, else downloaded now (with `curl`), or none when it cannot
    /// be.
    fn download_image(&mut self, url: &str) -> Option<PathBuf> {
        if self.remote.mode == RemoteImages::Cache {
            if let Some(path) = self.remote.cache.get(url).filter(|path| path.is_file()) {
                return Some(path.clone());
            }
        }
        self.remote.downloads += 1;
        // The name ends as the URL's does, for editors that tell an image's
        // kind by its extension.
        let name: String = url
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = self.remote.directory.path().join(format!("{}-{name}", self.remote.downloads));
        let status = std::process::Command::new("curl")
            .args(["--silent", "--fail", "--location", "--max-time", "30"])
            .args(["--proto", "=http,https", "--proto-redir", "=http,https"])
            .arg("--output")
            .arg(&path)
            .arg("--")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if !status.is_ok_and(|status| status.success()) || !path.is_file() {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        if self.remote.mode == RemoteImages::Cache {
            self.remote.cache.insert(url.to_string(), path.clone());
        }
        Some(path)
    }
}
