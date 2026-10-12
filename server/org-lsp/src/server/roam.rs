//! org-roam, when `org_roam_enabled` is on: the nodes of the files of
//! `org_roam_directory`, as `org-roam-db-sync` records them, offered by
//! their titles and aliases to visit (`org-roam-node-find`), to link to
//! (`org-roam-node-insert`) or to capture to (`org-roam-capture`). A title
//! no node has makes a new node with `org_roam_capture_templates`
//! (`org-roam-capture-`), in a capture buffer as `org.capture` shows one.
//!
//! The server reads the files again when they change, in the editor or on
//! disk, instead of keeping org-roam's SQLite database.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lsp_types::request::ShowDocument;
use lsp_types::{Position, Range, ShowDocumentParams, TextDocumentIdentifier, Uri};
use org_core::roam::{self, capture as roam_capture, Candidate, Db, FileData};
use org_core::{capture, edit, link, refile, Refusal};
use serde::Deserialize;
use serde_json::{json, Value};

pub mod buffer;
mod node_edit;

use super::{template_choices, template_text, Capturing, Pending, Server};
use crate::config::{CaptureTemplateOption, RoamSettings};
use crate::line_index::LineIndex;
use crate::{commands, uri};

/// What the commands say while org-roam is off.
const OFF: &str = "org-roam is off: set org_roam_enabled to use it";

/// The window's width for `org-roam-node-display-template`, as Emacs has
/// it in a frame 80 columns wide.
const WIDTH: usize = 79;

/// org-roam's settings, and what its files gave when last read.
pub(super) struct Roam {
    settings: RoamSettings,
    /// `org_roam_directory`, with `~/` expanded.
    directory: PathBuf,
    files: HashMap<PathBuf, Scanned>,
    /// Where the org-roam buffer's files go.
    pub(super) buffer_directory: PathBuf,
    /// The node the persistent buffer shows.
    buffer_node: Option<String>,
}

/// A file as org-roam last read it.
struct Scanned {
    stamp: Stamp,
    data: Result<FileData, String>,
}

/// What a file was when it was read: its text in the editor, or its time
/// and size on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stamp {
    Open(u64),
    Disk(Option<SystemTime>, u64),
}

impl Roam {
    pub fn new(settings: RoamSettings) -> Roam {
        let directory = expand(&settings.directory);
        let buffer_directory = super::server_directory("roam");
        Roam { settings, directory, files: HashMap::new(), buffer_directory, buffer_node: None }
    }
}

/// A path with `~/` standing for the home directory.
pub(super) fn expand(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    match (path.strip_prefix("~/").or(if path == "~" { Some("") } else { None }), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// The files under `directory`, by their names relative to it with `/`
/// between directories (`org-roam--directory-files-recursively`, which
/// follows links to directories).
pub(super) fn list_files(directory: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let relative = format!("{prefix}{name}");
        let path = entry.path();
        if path.is_dir() {
            if depth < 64 {
                list_files(&path, &format!("{relative}/"), depth + 1, out);
            }
        } else {
            out.push(relative);
        }
    }
}

/// A file's modification time, in nanoseconds, for sorting.
fn nanos(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |since| since.as_nanos() as u64)
}

/// What a roam command is run with: where, and the answers so far.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RoamContext {
    text_document: Option<TextDocumentIdentifier>,
    position: Option<Position>,
    /// The selection, Emacs's active region.
    range: Option<Range>,
    input: Option<String>,
    /// What `input` answers: `node`, the node read; `keys`, the key of a
    /// template; `capture`, a question of the template.
    step: Option<String>,
    /// The node read: its title, and its `ID` when it exists.
    title: Option<String>,
    id: Option<String>,
    /// The keys of the template picked so far.
    keys: Option<String>,
    /// The answers to the template's questions so far.
    answers: Option<Vec<String>>,
    /// What the editor would paste (`%c`, `%x`).
    clipboard: Option<String>,
    /// For `org.roam.buffer.toggle`: whether the editor shows the buffer.
    visible: Option<bool>,
    /// The count typed before the key (`C-u` is 4).
    count: Option<i64>,
}

/// A capture of org-roam's: the node's `ID`, and what is done once it is
/// filed (`org-roam-capture--finalize`).
pub(super) struct Captured {
    id: String,
    finalize: Finalize,
}

enum Finalize {
    /// `org-roam-capture`: nothing more.
    Nothing,
    /// `org-roam-node-find`: the node is shown.
    FindFile,
    /// `org-roam-node-insert`: a link to the node goes where the capture
    /// started, in place of the region.
    InsertLink { uri: Uri, text: String, start: usize, end: usize, description: String },
}

impl Server {
    /// The `org.roam.*` commands.
    pub(super) fn roam_command(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        if self.roam.is_none() {
            return Err(OFF.into());
        }
        if matches!(id, commands::ROAM_BUFFER_TOGGLE | commands::ROAM_BUFFER_DEDICATED | commands::ROAM_BUFFER_REFRESH)
        {
            return self.roam_buffer_command(id, context);
        }
        if matches!(
            id,
            commands::ROAM_ALIAS_ADD
                | commands::ROAM_ALIAS_REMOVE
                | commands::ROAM_TAG_ADD
                | commands::ROAM_TAG_REMOVE
                | commands::ROAM_REF_ADD
                | commands::ROAM_REF_REMOVE
        ) {
            return self.roam_edit(id, context);
        }
        let (db, candidates) = self.roam_nodes()?;
        if id == commands::ROAM_NODE_RANDOM {
            // `seq-random-elt` over the lines offered: a node with aliases
            // comes up more often.
            if candidates.is_empty() {
                return Err("Sequence cannot be empty".into());
            }
            let pick = (nanos(Some(SystemTime::now())) / 1000) as usize % candidates.len();
            return self.roam_visit(&db, &candidates[pick].id);
        }
        let region = self.roam_region(context);
        if id == commands::ROAM_NODE_INSERT && region.is_none() {
            let document = context.text_document.as_ref().ok_or("This command works in Org files")?;
            if self.document(&document.uri).is_none() || self.is_view(&document.uri) {
                return Err(Refusal::NotHere("This command works in Org files".to_string()));
            }
        }
        match context.step.as_deref() {
            // `org-roam-node-read`: the region is the initial input of
            // `org-roam-node-insert`.
            None => {
                let initial = region.as_ref().map(|(_, _, text)| text.clone()).unwrap_or_default();
                let completions: Vec<&str> = candidates.iter().map(|candidate| candidate.display.as_str()).collect();
                Ok(json!({ "input": {
                    "kind": "text",
                    "prompt": "Node",
                    "value": initial,
                    "completions": completions,
                    "select": true,
                    "context": { "step": "node" },
                } }))
            }
            Some("node") => {
                let answer = context.input.clone().unwrap_or_default();
                let found = candidates
                    .iter()
                    .find(|candidate| candidate.display == answer)
                    .or_else(|| candidates.iter().find(|candidate| candidate.display.trim_end() == answer.trim_end()));
                match (id, found) {
                    (commands::ROAM_NODE_FIND, Some(candidate)) => self.roam_visit(&db, &candidate.id),
                    (commands::ROAM_NODE_INSERT, Some(candidate)) => {
                        let description = region.as_ref().map_or(candidate.title.clone(), |(_, _, text)| text.clone());
                        self.roam_insert_link(context, &candidate.id, &description)
                    }
                    (_, found) => {
                        let title = found.map_or(answer.clone(), |candidate| candidate.title.clone());
                        let existing = found.map(|candidate| candidate.id.clone());
                        self.roam_capture(id, context, &db, &title, existing.as_deref())
                    }
                }
            }
            Some(_) => {
                let title = context.title.clone().unwrap_or_default();
                self.roam_capture(id, context, &db, &title, context.id.as_deref())
            }
        }
    }

    /// org-roam's database as `org-roam-db-sync` would make it now, and
    /// the lines that offer its nodes (`org-roam-node-read--completions`).
    fn roam_nodes(&mut self) -> Result<(Db, Vec<Candidate>), Refusal> {
        let template = self.roam.as_ref().ok_or(OFF)?.settings.display_template.clone();
        let (db, mtimes) = self.roam_db()?;
        let candidates = db.candidates(&template, WIDTH, &|file| mtimes.get(file).copied().unwrap_or(0))?;
        Ok((db, candidates))
    }

    /// org-roam's database as `org-roam-db-sync` would make it now, and
    /// when each file was modified.
    fn roam_db(&mut self) -> Result<(Db, HashMap<String, u64>), Refusal> {
        let roam = self.roam.as_ref().ok_or(OFF)?;
        let (directory, settings) = (roam.directory.clone(), roam.settings.files.clone());
        let mut names = Vec::new();
        list_files(&directory, "", 0, &mut names);
        names.retain(|name| roam::is_roam_file(name, &settings));
        roam::sync_order(&mut names);
        // The files open in the editor, by path: the editor may spell a
        // URI differently.
        let open: HashMap<PathBuf, String> = self
            .documents
            .keys()
            .filter_map(|key| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, key.clone())))
            .collect();
        let mut db = Db::default();
        let mut mtimes: HashMap<String, u64> = HashMap::new();
        let mut seen = Vec::new();
        for name in names {
            let path = directory.join(&name);
            let file = path.to_string_lossy().to_string();
            let metadata = std::fs::metadata(&path).ok();
            let modified = metadata.as_ref().and_then(|metadata| metadata.modified().ok());
            mtimes.insert(file.clone(), nanos(modified));
            let key = open.get(&path);
            let stamp = match key.and_then(|key| self.documents.get(key)) {
                Some(document) => {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    document.text.hash(&mut hasher);
                    Stamp::Open(hasher.finish())
                }
                None => Stamp::Disk(modified, metadata.as_ref().map_or(0, |metadata| metadata.len())),
            };
            let roam = self.roam.as_ref().ok_or(OFF)?;
            let data = match roam.files.get(&path).filter(|scanned| scanned.stamp == stamp) {
                Some(scanned) => scanned.data.clone(),
                None => {
                    // An encrypted file cannot be read without its key.
                    let data = if name.ends_with(".gpg") || name.ends_with(".age") {
                        Err("encrypted".to_string())
                    } else {
                        let uri = key.and_then(|key| key.parse::<Uri>().ok()).or_else(|| uri::from_path(&path));
                        uri.and_then(|uri| {
                            self.with_source(&uri, |text, _, parse, outline, _| roam::scan(text, parse, outline, &file))
                        })
                        .unwrap_or_else(|| Err("unreadable".to_string()))
                    };
                    if let Some(roam) = self.roam.as_mut() {
                        roam.files.insert(path.clone(), Scanned { stamp, data: data.clone() });
                    }
                    data
                }
            };
            db.add(&file, data);
            seen.push(path);
        }
        if let Some(roam) = self.roam.as_mut() {
            roam.files.retain(|path, _| seen.contains(path));
        }
        Ok((db, mtimes))
    }

    /// `org-roam-node-visit`: the node's file shown, at the node.
    fn roam_visit(&mut self, db: &Db, id: &str) -> Result<Value, Refusal> {
        let (file, node) = db.nodes.iter().find(|(_, node)| node.id == id).ok_or("No such node")?;
        let path = PathBuf::from(file);
        let uri = self.uri_for_path(&path).ok_or("Cannot name the node's file")?;
        let encoding = self.client.encoding;
        let offset = node.offset;
        let position = self
            .with_source(&uri, |text, index, _, _, _| index.position(text, offset.min(text.len()), encoding))
            .unwrap_or_default();
        let params = ShowDocumentParams {
            uri,
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(position, position)),
        };
        self.send_request::<ShowDocument>(params, None);
        Ok(Value::Null)
    }

    /// The URI of a file: the one the editor has it open under, if it does.
    fn uri_for_path(&self, path: &Path) -> Option<Uri> {
        let open = self.documents.keys().find_map(|key| {
            let uri = key.parse::<Uri>().ok()?;
            (uri::to_path(&uri).as_deref() == Some(path)).then_some(uri)
        });
        open.or_else(|| uri::from_path(path))
    }

    /// The region a roam command was run on, as byte offsets of its
    /// document, and its text as a link's description would show it
    /// (`org-link-display-format`).
    fn roam_region(&self, context: &RoamContext) -> Option<(usize, usize, String)> {
        let range = context.range.filter(|range| range.start != range.end)?;
        let document = self.document(&context.text_document.as_ref()?.uri)?;
        let encoding = self.client.encoding;
        let start = document.index.offset(&document.text, range.start, encoding);
        let end = document.index.offset(&document.text, range.end, encoding);
        let (start, end) = (start.min(end), start.max(end));
        Some((start, end, refile::link_display_format(&document.text[start..end])))
    }

    /// `org-roam-node-insert` of a node that exists: `[[id:ID][title]]`
    /// at the cursor, or in place of the region, the cursor after it.
    fn roam_insert_link(&mut self, context: &RoamContext, id: &str, description: &str) -> Result<Value, Refusal> {
        let uri = context.text_document.as_ref().ok_or("This command works in Org files")?.uri.clone();
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or("This file is not open")?;
        let (text, version) = (document.text.clone(), document.version);
        let (start, end) = match self.roam_region(context) {
            Some((start, end, _)) => (start, end),
            None => {
                let at = document.index.offset(&text, context.position.unwrap_or_default(), encoding);
                (at, at)
            }
        };
        let link = link::make_string(&format!("id:{id}"), Some(description))?;
        let edited = format!("{}{link}{}", &text[..start], &text[end..]);
        let position = LineIndex::new(&edited).position(&edited, start + link.len(), encoding);
        let edits = self.text_edits(&text, &edited);
        let edit = self.workspace_edit(&uri, Some(version), edits);
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::EditAndMove { uri, position });
        Ok(Value::Null)
    }

    /// `org-roam-capture-`: a template picked (by its keys unless there is
    /// only one), its questions answered, and the capture buffer shown.
    /// `existing` is the `ID` of a node that exists, whose file the
    /// capture goes to.
    fn roam_capture(
        &mut self,
        command: &str,
        context: &RoamContext,
        db: &Db,
        title: &str,
        existing: Option<&str>,
    ) -> Result<Value, Refusal> {
        let roam = self.roam.as_ref().ok_or(OFF)?;
        let templates = roam.settings.templates.clone();
        let directory = roam.directory.clone();
        // What the next question's answer is sent back with.
        let mut state = json!({ "title": title });
        if let Some(id) = existing {
            state["id"] = json!(id);
        }
        let mut keys = context.keys.clone().unwrap_or_default();
        if context.step.as_deref() == Some("keys") {
            keys.push_str(context.input.as_deref().unwrap_or(""));
        }
        let chosen = if templates.len() == 1 && keys.is_empty() {
            templates.first()
        } else {
            templates.iter().find(|template| template.key == keys && !template.is_group())
        };
        let Some(option) = chosen.cloned() else {
            let next = template_choices(&templates, &keys);
            if next.is_empty() {
                return Err(
                    if keys.is_empty() { "No capture templates" } else { "No capture template for this key" }.into()
                );
            }
            state["step"] = json!("keys");
            state["keys"] = json!(keys);
            return Ok(json!({ "input": {
                "kind": "key",
                "prompt": "Select a capture template",
                "value": "",
                "choices": next,
                "context": state,
            } }));
        };
        let template = roam_template(&option)?;
        let mut answers = context.answers.clone().unwrap_or_default();
        if context.step.as_deref() == Some("capture") {
            answers.push(context.input.clone().unwrap_or_default());
        }
        let region = self.roam_region(context);
        let document = context.text_document.as_ref().map(|document| document.uri.clone());
        let clipboard = context.clipboard.clone().unwrap_or_default();
        let mut fill_context =
            self.capture_fill_context(document.as_ref(), context.position, self.date_time(), clipboard);
        fill_context.initial = region.as_ref().map(|(_, _, text)| text.clone()).unwrap_or_default();
        // The node that exists: its file, which the capture goes to.
        let node_of = |id: &str| db.nodes.iter().find(|(_, node)| node.id == id);
        let file = existing.and_then(node_of).map(|(file, _)| PathBuf::from(file));
        // `org-roam-node-from-id`, then `org-roam-node-from-title-or-alias`.
        let find_node = |name: &str| {
            let by_alias = || {
                let id = db.aliases.iter().find(|(_, alias)| alias == name).map(|(id, _)| id.as_str())?;
                node_of(id)
            };
            node_of(name)
                .or_else(|| db.nodes.iter().find(|(_, node)| node.title == name))
                .or_else(by_alias)
                .map(|(file, node)| (PathBuf::from(file), node.offset))
        };
        // The files as the editor has them, when it has them open.
        let open: HashMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(key, document)| Some((uri::to_path(&key.parse::<Uri>().ok()?)?, document.text.clone())))
            .collect();
        let read =
            |path: &Path| -> Option<String> { open.get(path).cloned().or_else(|| std::fs::read_to_string(path).ok()) };
        let (todo, edit_settings) = (self.todo.clone(), self.edit_settings.clone());
        let properties = |text: &str| -> org_core::properties::Context {
            edit::property_context(&org_syntax::parse(text, &todo), &edit_settings, "")
        };
        let levels = refile::Settings { adapt: self.adapt_in(""), ..self.refile.2 };
        let id = link::new_uuid();
        let request = roam_capture::Request {
            template: &template,
            title,
            id: existing.unwrap_or(&id),
            file: file.as_deref(),
            answers: &answers,
            context: &fill_context,
            read_date: &self.read_date,
            directory: &directory,
            read: &read,
            properties: &properties,
            levels: &levels,
            find_node: &find_node,
        };
        let ready = match roam_capture::capture(&request)? {
            roam_capture::Step::Ask(question) => {
                state["step"] = json!("capture");
                state["keys"] = json!(option.key);
                state["answers"] = json!(answers);
                let prompt = match question {
                    capture::Question::Text { prompt, .. } => prompt,
                    capture::Question::Date { prompt, .. } => prompt.unwrap_or_else(|| "Date".to_string()),
                    capture::Question::Tags { prompt, .. } => prompt.unwrap_or_else(|| "Tags".to_string()),
                    capture::Question::Clipboard { .. } => "Clipboard/kill value".to_string(),
                    capture::Question::Property { name, .. } => format!("{name} value"),
                };
                let prompt = prompt.strip_suffix(": ").unwrap_or(&prompt).to_string();
                return Ok(json!({ "input": { "kind": "text", "prompt": prompt, "value": "", "context": state } }));
            }
            roam_capture::Step::Ready(ready) => ready,
        };
        let target = self.uri_for_path(&ready.path).ok_or("Cannot name the target file")?;
        let snapshot = if ready.new_file { String::new() } else { read(&ready.path).unwrap_or_default() };
        let finalize = match command {
            commands::ROAM_NODE_FIND => Finalize::FindFile,
            commands::ROAM_NODE_INSERT => {
                let uri = document.clone().ok_or("This command works in Org files")?;
                let document = self.document(&uri).ok_or("This file is not open")?;
                let text = document.text.clone();
                let (start, end) = match &region {
                    Some((start, end, _)) => (*start, *end),
                    None => {
                        let at =
                            document.index.offset(&text, context.position.unwrap_or_default(), self.client.encoding);
                        (at, at)
                    }
                };
                let description = region.map_or(title.to_string(), |(_, _, text)| text);
                Finalize::InsertLink { uri, text, start, end, description }
            }
            _ => Finalize::Nothing,
        };
        let uri = self.show_capture(&target, &ready.placed)?;
        let unnarrowed = template.unnarrowed;
        self.capture = Some(Capturing {
            uri,
            target,
            snapshot,
            placed: ready.placed,
            spec: (capture::Target::File, template.kind, template.properties.clone()),
            clock: None,
            jump: false,
            unnarrowed,
            roam: Some(Captured { id: ready.id, finalize }),
        });
        Ok(Value::Null)
    }

    /// `org-roam-capture--finalize`, once the capture is filed in
    /// `target`, whose text is now `done`.
    pub(super) fn roam_finalize(&mut self, captured: &Captured, target: &Uri, done: &str) {
        match &captured.finalize {
            Finalize::Nothing => {}
            // `org-roam-capture--finalize-find-file`: the target shown, at
            // its node.
            Finalize::FindFile => {
                let id = &captured.id;
                let parse = org_syntax::parse(done, &self.todo);
                let offset = org_core::open::find_id(&parse, id).unwrap_or(0);
                let position = LineIndex::new(done).position(done, offset, self.client.encoding);
                let params = ShowDocumentParams {
                    uri: target.clone(),
                    external: None,
                    take_focus: Some(true),
                    selection: Some(Range::new(position, position)),
                };
                self.send_request::<ShowDocument>(params, None);
            }
            // `org-roam-capture--finalize-insert-link`: where the capture
            // started, wherever edits since have moved it.
            Finalize::InsertLink { uri, text, start, end, description } => {
                let Some(document) = self.document(uri) else { return };
                let (now, version) = (document.text.clone(), document.version);
                let (start, end) = (moved(text, &now, *start), moved(text, &now, *end).max(moved(text, &now, *start)));
                let Ok(link) = link::make_string(&format!("id:{}", captured.id), Some(description)) else { return };
                let edited = format!("{}{link}{}", &now[..start], &now[end..]);
                let edits = self.text_edits(&now, &edited);
                let edit = self.workspace_edit(uri, Some(version), edits);
                let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
                self.apply_edit(params, Pending::Edit);
            }
        }
    }
}

/// Where `offset` of `old` is in `new`, the text changed in one place: a
/// marker's place after the change.
fn moved(old: &str, new: &str, offset: usize) -> usize {
    let prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    let longest = old.len().min(new.len()) - prefix;
    let suffix = old.bytes().rev().zip(new.bytes().rev()).take(longest).take_while(|(a, b)| a == b).count();
    let at = if offset <= prefix {
        offset
    } else if offset >= old.len() - suffix {
        offset + new.len() - old.len()
    } else {
        new.len() - suffix
    };
    let mut at = at.min(new.len());
    while !new.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// A template of `org_roam_capture_templates`, as org-roam takes it.
fn roam_template(option: &CaptureTemplateOption) -> Result<roam_capture::Template, Refusal> {
    use roam_capture::Target;
    let kind = match option.kind.as_deref() {
        None | Some("entry") => capture::Kind::Entry,
        Some("plain") => capture::Kind::Plain,
        Some("item") => capture::Kind::Item,
        Some("checkitem") => capture::Kind::CheckItem,
        Some("table-line") => capture::Kind::TableLine,
        Some(other) => return Err(format!("Capture templates of type `{other}` are not supported").into()),
    };
    let part = |at: usize| option.target.get(at).cloned().unwrap_or_default();
    let rest = |from: usize| option.target.get(from..).unwrap_or_default().to_vec();
    let target = match option.target.first().map(String::as_str) {
        Some("file") => Target::File(part(1)),
        Some("file+head") => Target::FileHead(part(1), part(2)),
        Some("file+olp") => Target::FileOlp(part(1), rest(2)),
        Some("file+head+olp") => Target::FileHeadOlp(part(1), part(2), rest(3)),
        Some("node") => Target::Node(part(1)),
        Some(other) => return Err(format!("Invalid capture target specification: {other}").into()),
        None => return Err("Template needs to specify `:target'".into()),
    };
    Ok(roam_capture::Template {
        kind,
        body: template_text(option),
        target,
        properties: capture::Properties {
            prepend: option.prepend.unwrap_or(false),
            empty_lines: option.empty_lines,
            empty_lines_before: option.empty_lines_before,
            empty_lines_after: option.empty_lines_after,
            table_line_pos: option.table_line_pos.clone(),
        },
        unnarrowed: option.unnarrowed == Some(true),
    })
}

#[cfg(test)]
mod tests {
    use super::moved;

    #[test]
    fn a_place_moves_with_the_text_before_it() {
        assert_eq!(moved("abc def", "abc def", 4), 4);
        assert_eq!(moved("abc def", "xx abc def", 4), 7);
        assert_eq!(moved("abc def", "abc def yy", 4), 4);
        assert_eq!(moved("abc def", "abc zzz", 5), 7);
    }
}
