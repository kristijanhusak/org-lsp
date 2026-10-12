//! The org-roam buffer, as an Org file of the server's own: the persistent
//! one (`org-roam-buffer-toggle`), which follows the node the cursor is in
//! as the editor reports its moves (`org-lsp/cursor`), and dedicated ones
//! for a node each (`org-roam-buffer-display-dedicated`). Its items link to
//! the places they come from, a line of their files.

use std::path::{Path, PathBuf};

use lsp_types::request::ShowDocument;
use lsp_types::{Position, ShowDocumentParams, Uri};
use org_core::roam::{buffer, edit, slugify};
use org_core::Refusal;
use serde_json::{json, Value};

use super::RoamContext;
use crate::server::Server;
use crate::{commands, uri};

/// The name of the persistent buffer's file; a dedicated one's starts with
/// it too.
pub const FILE_PREFIX: &str = "org-roam-buffer";

impl Server {
    /// The file of the persistent buffer, or of the dedicated buffer of a
    /// node.
    fn roam_buffer_file(&self, dedicated: Option<&str>) -> Option<PathBuf> {
        let roam = self.roam.as_ref()?;
        let name = match dedicated {
            Some(title) => format!("{FILE_PREFIX}-{}.org", slugify(title)),
            None => format!("{FILE_PREFIX}.org"),
        };
        Some(roam.buffer_directory.path().join(name))
    }

    /// Whether `uri` is a file of the org-roam buffer.
    pub(in crate::server) fn is_roam_buffer(&self, uri: &Uri) -> bool {
        let (Some(roam), Some(path)) = (self.roam.as_ref(), uri::to_path(uri)) else { return false };
        roam.buffer_directory.made().is_some_and(|directory| path.parent() == Some(directory))
    }

    /// The buffer's text for the node `id`: its sections, as
    /// `org_roam_mode_sections` lists them, as an Org document.
    fn roam_buffer_text(&mut self, id: &str) -> Result<String, Refusal> {
        let (db, _) = self.roam_db()?;
        let roam = self.roam.as_ref().ok_or(super::OFF)?;
        let kinds = roam.settings.sections.clone();
        let directory = roam.directory.clone();
        let extensions = roam.settings.files.extensions.clone();
        // What ripgrep searches for unlinked references: the files of each
        // extension, encrypted too, but not hidden ones.
        let mut names = Vec::new();
        super::list_files(&directory, "", 0, &mut names);
        let files: Vec<String> = names
            .iter()
            .filter(|name| !name.split('/').any(|part| part.starts_with('.')))
            .filter(|name| {
                extensions.iter().any(|extension| {
                    [format!(".{extension}"), format!(".{extension}.gpg"), format!(".{extension}.age")]
                        .iter()
                        .any(|ending| name.ends_with(ending.as_str()))
                })
            })
            .map(|name| directory.join(name).to_string_lossy().to_string())
            .collect();
        let read = |file: &str| -> Option<String> {
            let path = Path::new(file);
            let open = self.documents.iter().find_map(|(key, document)| {
                let uri = key.parse::<Uri>().ok()?;
                (uri::to_path(&uri).as_deref() == Some(path)).then(|| document.text.clone())
            });
            open.or_else(|| std::fs::read_to_string(path).ok())
        };
        let sections = buffer::sections(&db, id, &kinds, &read, &files);
        let title =
            db.nodes.iter().find(|(_, node)| node.id == id).map_or(String::new(), |(_, node)| node.title.clone());
        let line_of = |file: &str, offset: usize| {
            read(file).map_or(1, |text| text[..offset.min(text.len())].matches('\n').count() + 1)
        };
        Ok(buffer::org(&title, &sections, &line_of))
    }

    /// Show `text` in the buffer's file `path`: written there, and given
    /// to the editor when it has the file open, then saved. With `show`,
    /// the editor is asked to show it, the cursor staying where it is
    /// unless with `focus`.
    pub(in crate::server) fn own_file_show(
        &mut self,
        path: &Path,
        text: &str,
        show: bool,
        focus: bool,
    ) -> Result<Uri, Refusal> {
        let uri = uri::from_path(path).ok_or("Cannot name the file")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        match self.document(&uri).map(|document| (document.text.clone(), document.version)) {
            Some((old, version)) if old != text => {
                let edits = self.text_edits(&old, text);
                let edit = self.workspace_edit(&uri, Some(version), edits);
                let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
                // Saved once applied, whatever `auto_save` says: the file is
                // the server's.
                let request = self.new_request(
                    <lsp_types::request::ApplyWorkspaceEdit as lsp_types::request::Request>::METHOD,
                    serde_json::to_value(params).unwrap_or(Value::Null),
                    Some(crate::server::Pending::Edit),
                );
                self.saving.insert(request.id.clone(), vec![uri.clone()]);
                let _ = self.sender.send(lsp_server::Message::Request(request));
            }
            Some(_) => {}
            None => std::fs::write(path, text).map_err(|error| error.to_string())?,
        }
        if show {
            let start = Position::new(0, 0);
            let params = ShowDocumentParams {
                uri: uri.clone(),
                external: None,
                take_focus: Some(focus),
                selection: Some(lsp_types::Range::new(start, start)),
            };
            self.send_request::<ShowDocument>(params, None);
        }
        Ok(uri)
    }

    /// `org.roam.buffer.toggle`, `org.roam.buffer.dedicated` and
    /// `org.roam.buffer.refresh`.
    pub(super) fn roam_buffer_command(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        match id {
            // `org-roam-buffer-toggle`: hidden when the editor shows it,
            // else shown for the node at the cursor.
            commands::ROAM_BUFFER_TOGGLE => {
                let path = self.roam_buffer_file(None).ok_or(super::OFF)?;
                let uri = uri::from_path(&path).ok_or("Cannot name the org-roam buffer")?;
                let visible = context.visible.unwrap_or_else(|| self.document(&uri).is_some());
                if visible {
                    if let Some(roam) = self.roam.as_mut() {
                        roam.buffer_node = None;
                    }
                    return Ok(json!({ "close": uri.as_str() }));
                }
                let node = self.roam_node_at(context);
                let text = match &node {
                    Some(id) => self.roam_buffer_text(id)?,
                    None => String::new(),
                };
                if let Some(roam) = self.roam.as_mut() {
                    roam.buffer_node = node;
                }
                self.own_file_show(&path, &text, true, false)?;
                Ok(Value::Null)
            }
            // `org-roam-buffer-display-dedicated`: for a node read, or with
            // a count (`C-u`) the node at the cursor.
            commands::ROAM_BUFFER_DEDICATED => {
                let node = if context.count.is_some() {
                    Some(self.roam_node_at(context).ok_or("No node at point")?)
                } else {
                    None
                };
                let (db, candidates) = self.roam_nodes()?;
                let id = match node {
                    Some(id) => id,
                    None => {
                        let Some(answer) = context.input.clone().filter(|_| context.step.as_deref() == Some("node"))
                        else {
                            let completions: Vec<&str> =
                                candidates.iter().map(|candidate| candidate.display.as_str()).collect();
                            return Ok(json!({ "input": {
                                "kind": "text",
                                "prompt": "Node",
                                "value": "",
                                "completions": completions,
                                "select": true,
                                "context": { "step": "node" },
                            } }));
                        };
                        // `require-match`: only a node offered.
                        let found = candidates
                            .iter()
                            .find(|candidate| candidate.display == answer)
                            .or_else(|| {
                                candidates.iter().find(|candidate| candidate.display.trim_end() == answer.trim_end())
                            })
                            .ok_or("No match")?;
                        found.id.clone()
                    }
                };
                let title = db
                    .nodes
                    .iter()
                    .find(|(_, node)| node.id == id)
                    .map_or(String::new(), |(_, node)| node.title.clone());
                let path = self.roam_buffer_file(Some(&title)).ok_or(super::OFF)?;
                let text = self.roam_buffer_text(&id)?;
                self.own_file_show(&path, &text, true, false)?;
                Ok(Value::Null)
            }
            // `org-roam-buffer-refresh`: the persistent buffer again, for its
            // node, with what the files say now.
            _ => {
                let path = self.roam_buffer_file(None).ok_or(super::OFF)?;
                let node = self.roam.as_ref().and_then(|roam| roam.buffer_node.clone());
                let Some(node) = node else { return Ok(Value::Null) };
                let text = self.roam_buffer_text(&node)?;
                self.own_file_show(&path, &text, false, false)?;
                Ok(Value::Null)
            }
        }
    }

    /// The `ID` of the node at the cursor of a command, if any
    /// (`org-roam-node-at-point`).
    fn roam_node_at(&self, context: &RoamContext) -> Option<String> {
        let document = self.document(&context.text_document.as_ref()?.uri)?;
        let offset = document.index.offset(&document.text, context.position?, self.client.encoding);
        let property_context = org_core::edit::property_context(&document.parse, &self.edit_settings, "");
        edit::node_at(&document.text, offset, &property_context).ok().map(|found| found.id)
    }

    /// `org-lsp/cursor`: the cursor moved in an Org file. While the editor
    /// shows the persistent buffer, it follows the node the cursor is in,
    /// in org-roam's files (`org-roam-buffer-persistent-redisplay`).
    pub(in crate::server) fn roam_cursor(&mut self, params: Value) {
        let Some(path) = self.roam_buffer_file(None) else { return };
        let Some(buffer_uri) = uri::from_path(&path) else { return };
        if self.document(&buffer_uri).is_none() {
            return;
        }
        let Ok(context) = serde_json::from_value::<RoamContext>(params) else { return };
        let Some(document) = context.text_document.as_ref() else { return };
        if !self.is_roam_file(&document.uri) {
            return;
        }
        let Some(node) = self.roam_node_at(&context) else { return };
        if self.roam.as_ref().and_then(|roam| roam.buffer_node.as_deref()) == Some(node.as_str()) {
            return;
        }
        let Ok(text) = self.roam_buffer_text(&node) else { return };
        if let Some(roam) = self.roam.as_mut() {
            roam.buffer_node = Some(node);
        }
        if let Err(refusal) = self.own_file_show(&path, &text, false, false) {
            self.show_message(lsp_types::MessageType::WARNING, refusal.message());
        }
    }
}
