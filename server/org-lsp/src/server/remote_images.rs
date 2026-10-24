//! The images of `http:` and `https:` links (`org-display-remote-inline-images`):
//! downloaded into a private directory of the server and shown as the
//! images of local files are, once `org-resource-download-policy` and
//! `org-safe-remote-resources` allow it or the answer to
//! `org--confirm-resource-safe`'s question does. That question is an input
//! whose answer runs `org.link.preview` again; the answers so far go with
//! it, so that each URL is asked about once.
//!
//! As Emacs's `download` reads the file anew each time it shows, so the
//! image is downloaded each time here, into the same file for the same
//! URL; `cache`, which keeps the file in a buffer, keeps it here for the
//! session. The regexps the answers `!`, `d` and `f` add to
//! `org-safe-remote-resources` (by `customize-push-and-save` in Emacs) are
//! kept for the session, not saved.
//!
//! Emacs downloads while it waits. Here `curl` runs on a thread of its
//! own, as Emacs makes LaTeX previews on its own: the command answers
//! with the images of files, or nothing when there are none, and the
//! preview comes again in `org-lsp/images`, with all its images, once the
//! downloads are there. The previews download one after the other, and
//! one whose document changed or closed before its turn does not, nor
//! one asked again the same before its turn. An image larger than
//! [`LARGEST`] is not downloaded.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use lsp_types::{MessageType, Uri};
use org_core::link_preview::Preview;
use org_core::remote_resource::{self, Fetch, Policy};
use serde_json::{json, Value};

use super::{Context, Message, Private, Server, PREVIEW_IMAGES};
use crate::config::RemoteImages;
use crate::uri;

/// The largest image downloaded, as `curl --max-filesize` takes it.
pub(super) const LARGEST: &str = "20M";
/// [`LARGEST`] in bytes.
const LARGEST_BYTES: u64 = 20 << 20;

/// The most bytes of the name of a downloaded file, which ends as its URL
/// does: with the number before it and a partial download's ending
/// after, within the 255 bytes a file's name has.
const NAME_BYTES: usize = 200;

/// What the server knows of remote images.
pub(super) struct Remote {
    /// `org-display-remote-inline-images`
    pub(super) mode: RemoteImages,
    /// `org-resource-download-policy`
    pub(super) policy: Policy,
    /// `org-safe-remote-resources`, and the regexps the answers added.
    pub(super) safe: Vec<String>,
    /// The file each URL is downloaded to.
    files: HashMap<String, PathBuf>,
    /// Where the images are downloaded to.
    pub(super) directory: Private,
    /// How many files were named, which names the next one.
    named: u64,
    /// How many previews waited for downloads, which numbers the next.
    jobs: u64,
    /// The previews waiting for their downloads, by their number.
    waiting: HashMap<u64, Waiting>,
    /// The previews whose downloads have not started, the first first.
    queue: VecDeque<u64>,
    /// Whether the downloads of a preview are running: one at a time.
    downloading: bool,
    /// The downloads done, sent by their threads.
    done: (Sender<Downloaded>, Receiver<Downloaded>),
}

impl Remote {
    pub(super) fn new(mode: RemoteImages, policy: Policy, safe: Vec<String>) -> Remote {
        Remote {
            mode,
            policy,
            safe,
            files: HashMap::new(),
            directory: Private::new("images"),
            named: 0,
            jobs: 0,
            waiting: HashMap::new(),
            queue: VecDeque::new(),
            downloading: false,
            done: crossbeam_channel::unbounded(),
        }
    }

    /// Where the downloads say they are done, for the main loop.
    pub(super) fn downloads(&self) -> Receiver<Downloaded> {
        self.done.1.clone()
    }
}

/// The images of a preview's remote links: those there already, by the
/// start of their link, and the URLs to download first.
#[derive(Default)]
pub(super) struct RemoteFiles {
    pub(super) files: HashMap<usize, PathBuf>,
    pub(super) wanted: Vec<(usize, String)>,
}

/// An image of a preview: where it goes and how wide, and its file or the
/// URL it is downloaded from.
pub(super) struct Item {
    pub(super) range: Value,
    pub(super) width: Option<(&'static str, Value)>,
    pub(super) source: Source,
}

pub(super) enum Source {
    File(PathBuf),
    /// A remote image, by the start of its link.
    Remote(usize),
}

/// A preview whose remote images are being downloaded.
struct Waiting {
    uri: Uri,
    version: i32,
    preview: Value,
    items: Vec<Item>,
    files: HashMap<usize, PathBuf>,
    wanted: Vec<(usize, String)>,
    startup: bool,
    /// The URLs to download and their files, until the downloads start.
    urls: Vec<(String, PathBuf)>,
}

/// The downloads of a preview, done: each URL's file, or none.
pub(super) struct Downloaded {
    job: u64,
    files: Vec<(String, Option<PathBuf>)>,
}

/// The items of a preview, with the files of their remote images: those
/// whose file is there.
pub(super) fn finish(items: &[Item], remote: &HashMap<usize, PathBuf>) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| {
            let path = match &item.source {
                Source::File(path) => path.clone(),
                Source::Remote(start) => remote.get(start)?.clone(),
            };
            // `a/./b.png` as `a/b.png`.
            let path: PathBuf = path.components().collect();
            let path = Some(path).filter(|path| path.is_file())?;
            let mut value = json!({ "range": item.range, "path": path.to_string_lossy() });
            if let Some((key, width)) = &item.width {
                value[*key] = width.clone();
            }
            Some(value)
        })
        .collect()
}

impl Server {
    /// The files of the remote images among `found` of the file `uri`, by
    /// the start of their link, those that may not be downloaded left
    /// out, and those to download; or the question to ask first, as
    /// `{ "input" }`. When the file opens (`startup`) nothing is asked,
    /// and what would be asked about is left out.
    pub(super) fn remote_images(
        &mut self,
        uri: &Uri,
        found: &[Preview],
        context: &Context,
        startup: bool,
    ) -> Result<RemoteFiles, Value> {
        let mut remote = RemoteFiles::default();
        if self.remote.mode == RemoteImages::Skip || !found.iter().any(|found| found.remote) {
            return Ok(remote);
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
            if !fetch {
                if !refused.contains(&url) {
                    refused.push(url);
                }
                continue;
            }
            // With `cache`, the file downloaded before.
            let kept =
                self.remote.files.get(url).filter(|path| self.remote.mode == RemoteImages::Cache && path.is_file());
            match kept {
                Some(path) => {
                    remote.files.insert(found.start, path.clone());
                }
                None => remote.wanted.push((found.start, url.to_string())),
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
        Ok(remote)
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

    /// Download the images `remote` wants for `preview` of the document
    /// `uri` at `version`, on a thread of their own once those of the
    /// previews before are done; once they are there, the preview with
    /// `items` goes to the client in `org-lsp/images`.
    pub(super) fn download_images(
        &mut self,
        uri: Uri,
        version: i32,
        preview: Value,
        items: Vec<Item>,
        remote: RemoteFiles,
        startup: bool,
    ) {
        let mut urls: Vec<(String, PathBuf)> = Vec::new();
        for (_, url) in &remote.wanted {
            if !urls.iter().any(|(known, _)| known == url) {
                urls.push((url.clone(), self.image_file(url)));
            }
        }
        self.remote.jobs += 1;
        let job = self.remote.jobs;
        let RemoteFiles { files, wanted } = remote;
        self.remote.waiting.insert(job, Waiting { uri, version, preview, items, files, wanted, startup, urls });
        self.remote.queue.push_back(job);
        self.next_download();
    }

    /// Start the downloads of the next preview whose document is as it
    /// was, unless those of another are running; those of the previews
    /// before it, whose document changed or closed, are left out, and so
    /// is one asked again the same before its turn: the later one shows
    /// what it would.
    fn next_download(&mut self) {
        if self.remote.downloading {
            return;
        }
        while let Some(job) = self.remote.queue.pop_front() {
            let Some(waiting) = self.remote.waiting.get(&job) else { continue };
            if self.document(&waiting.uri).map(|document| document.version) != Some(waiting.version) {
                self.remote.waiting.remove(&job);
                continue;
            }
            let asked_again =
                self.remote.queue.iter().filter_map(|later| self.remote.waiting.get(later)).any(|later| {
                    later.uri == waiting.uri
                        && later.version == waiting.version
                        && later.startup == waiting.startup
                        && later.preview == waiting.preview
                        && later.urls == waiting.urls
                });
            if asked_again {
                self.remote.waiting.remove(&job);
                continue;
            }
            let urls = self.remote.waiting.get_mut(&job).map(|waiting| std::mem::take(&mut waiting.urls));
            let sender = self.remote.done.0.clone();
            // With `cache`, a file downloaded for a preview before is kept.
            let cache = self.remote.mode == RemoteImages::Cache;
            std::thread::spawn(move || {
                let files = urls
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(index, (url, path))| {
                        let downloaded = (cache && path.is_file()) || download(&url, &path, &format!("{job}-{index}"));
                        (url, downloaded.then_some(path))
                    })
                    .collect();
                let _ = sender.send(Downloaded { job, files });
            });
            self.remote.downloading = true;
            return;
        }
    }

    /// The downloads of a preview are done: those of the next start.
    pub(super) fn images_downloaded(&mut self, done: Downloaded) {
        self.remote.downloading = false;
        self.preview_downloaded(done);
        self.next_download();
    }

    /// While the document of the preview whose downloads are `done` is as
    /// it was, what could not be downloaded is said, and the preview goes
    /// to the client.
    fn preview_downloaded(&mut self, done: Downloaded) {
        let Some(mut waiting) = self.remote.waiting.remove(&done.job) else { return };
        if self.document(&waiting.uri).map(|document| document.version) != Some(waiting.version) {
            return;
        }
        for (url, path) in &done.files {
            if path.is_none() && !waiting.startup {
                self.show_message(MessageType::INFO, format!("Unable to fetch file from {url:?}"));
            }
        }
        let fetched: HashMap<&str, &PathBuf> =
            done.files.iter().filter_map(|(url, path)| Some((url.as_str(), path.as_ref()?))).collect();
        for (start, url) in &waiting.wanted {
            if let Some(path) = fetched.get(url.as_str()) {
                waiting.files.insert(*start, (*path).clone());
            }
        }
        waiting.preview["items"] = json!(finish(&waiting.items, &waiting.files));
        let notification = lsp_server::Notification::new(PREVIEW_IMAGES.to_string(), waiting.preview);
        let _ = self.sender.send(Message::Notification(notification));
    }

    /// The file the image of `url` is downloaded to: the same each time.
    /// Its name ends as the URL's does, for editors that tell an image's
    /// kind by its extension.
    fn image_file(&mut self, url: &str) -> PathBuf {
        if let Some(path) = self.remote.files.get(url) {
            return path.clone();
        }
        self.remote.named += 1;
        let path = self.remote.directory.path().join(format!("{}-{}", self.remote.named, file_name(url)));
        self.remote.files.insert(url.to_string(), path.clone());
        path
    }
}

/// The end of the name of the file `url` is downloaded to: the last part
/// of the URL, what is not a letter, a digit, `.`, `-` or `_` made `_`,
/// its start cut to [`NAME_BYTES`] bytes, its extension kept.
fn file_name(url: &str) -> String {
    let name: String = url
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if name.len() <= NAME_BYTES {
        return name;
    }
    // Only ASCII is left, a byte each.
    let extension = name.rfind('.').map(|at| &name[at..]).filter(|extension| extension.len() <= 16).unwrap_or("");
    format!("{}{extension}", &name[..NAME_BYTES - extension.len()])
}

/// Download `url` with `curl` into `path`: into a file beside it named
/// with `part` first, then renamed over it, so that the image shown from
/// it before stays whole until the new one is there. Whether it was: not
/// when it is larger than [`LARGEST`], which a `curl` before 8.4 finds
/// out only from a `Content-Length`. The URL is as it is: `{a,b}` and
/// `[1-3]` are no patterns of URLs to download (`--globoff`).
fn download(url: &str, path: &std::path::Path, part: &str) -> bool {
    let mut partial = path.as_os_str().to_owned();
    partial.push(format!(".part-{part}"));
    let partial = PathBuf::from(partial);
    let status = std::process::Command::new("curl")
        .args(["--silent", "--fail", "--location", "--globoff", "--max-time", "30", "--max-filesize", LARGEST])
        .args(["--proto", "=http,https", "--proto-redir", "=http,https"])
        .arg("--output")
        .arg(&partial)
        .arg("--")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let whole = std::fs::metadata(&partial).is_ok_and(|file| file.is_file() && file.len() <= LARGEST_BYTES);
    let renamed = status.is_ok_and(|status| status.success()) && whole && std::fs::rename(&partial, path).is_ok();
    if !renamed {
        let _ = std::fs::remove_file(&partial);
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_files_as_their_urls_end() {
        assert_eq!(file_name("https://example.com/a/x[1-3]%20b.png"), "x_1-3__20b.png");
        // Within the length of a file's name, keeping the extension.
        let long = format!("https://example.com/{}.jpeg", "é".repeat(300));
        let name = file_name(&long);
        assert_eq!(name.len(), NAME_BYTES);
        assert!(name.starts_with("___") && name.ends_with("_.jpeg"), "{name}");
        let dotless = file_name(&format!("https://example.com/{}", "x".repeat(400)));
        assert_eq!(dotless, "x".repeat(NAME_BYTES));
    }
}
