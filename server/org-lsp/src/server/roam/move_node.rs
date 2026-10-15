//! `org-roam-refile` and `org-roam-extract-subtree`: the subtree at the
//! cursor moved under another node, or to a file of its own where it is
//! the file's node.

use std::path::PathBuf;

use lsp_types::{
    DeleteFile, DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, ResourceOp,
    TextDocumentEdit, TextEdit, Uri, WorkspaceEdit,
};
use org_core::roam::edit;
use org_core::{edit as org_edit, link, refile, Refusal};
use serde_json::{json, Value};

use super::{expand, RoamContext};
use crate::server::{Pending, Server};
use crate::uri;

/// The Org file a command runs in: its URI, text, version and the cursor.
type Here = (Uri, String, i32, usize);

impl Server {
    fn roam_here(&self, context: &RoamContext) -> Result<Here, Refusal> {
        let not_here = || Refusal::NotHere("This command works in Org files".to_string());
        let uri = context.text_document.as_ref().ok_or_else(not_here)?.uri.clone();
        if self.is_view(&uri) {
            return Err(not_here());
        }
        let document = self.document(&uri).ok_or_else(not_here)?;
        let offset = document.index.offset(&document.text, context.position.unwrap_or_default(), self.client.encoding);
        Ok((uri, document.text.clone(), document.version, offset))
    }

    /// `org-roam-refile`: asks for a node (`org-roam-node-read` with
    /// REQUIRE-MATCH), and moves the subtree at the cursor under it, a
    /// level below it, after its subtree. Before the first headline the
    /// whole file goes, and the file with it.
    pub(super) fn roam_refile(&mut self, context: &RoamContext) -> Result<Value, Refusal> {
        let (uri, text, version, offset) = self.roam_here(context)?;
        let (db, candidates) = self.roam_nodes()?;
        let Some(answer) = context.input.clone().filter(|_| context.step.as_deref() == Some("node")) else {
            let completions: Vec<&str> = candidates.iter().map(|candidate| candidate.display.as_str()).collect();
            return Ok(json!({ "input": {
                "kind": "text",
                "prompt": "Node",
                "value": "",
                "completions": completions,
                "select": true,
                "context": { "step": "node" },
            } }));
        };
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.display == answer)
            .or_else(|| candidates.iter().find(|candidate| candidate.display.trim_end() == answer.trim_end()))
            .ok_or("No match")?;
        let (file, node) = db.nodes.iter().find(|(_, node)| node.id == candidate.id).ok_or("No such node")?;
        let property_context = {
            let document = self.document(&uri).ok_or("This file is not open")?;
            org_edit::property_context(&document.parse, &self.edit_settings, &uri::file_stem(&uri))
        };
        if edit::node_at(&text, offset, &property_context).is_ok_and(|at| at.id == node.id) {
            return Err("Target is the same as current node".into());
        }
        let to = (node.level > 0).then_some(node.offset);
        let target_uri = self.uri_for_path(&PathBuf::from(file)).ok_or("Cannot name the node's file")?;
        let same = uri::to_path(&target_uri) == uri::to_path(&uri);
        let target = if same {
            None
        } else {
            let read = self.with_source(&target_uri, |text, _, _, _, version| (text.to_string(), version));
            Some(read.ok_or("Cannot read the node's file")?)
        };
        let keywords = self.document(&uri).map(|document| document.parse.todo_keywords().clone()).unwrap_or_default();
        let structure = self.structure_settings(&text);
        let into = target.as_ref().map_or(text.as_str(), |(text, _)| text.as_str());
        let settings = refile::Settings { adapt: self.adapt_in(into), ..self.refile.2 };
        let refiled = edit::refile(
            &text,
            offset,
            target.as_ref().map(|(text, _)| text.as_str()),
            to,
            &keywords,
            &structure,
            &self.edit_settings.tags,
            &settings,
        )?;
        let mut edits = Vec::new();
        if let (Some(changed), Some((now, target_version))) = (&refiled.target, &target) {
            edits.push((target_uri, *target_version, self.text_edits(now, changed)));
        }
        // A file left empty is deleted, as org-roam deletes it, when the
        // editor can delete files; else it stays, empty.
        let delete = refiled.source.is_empty() && self.client.delete_files;
        if !delete {
            edits.push((uri.clone(), Some(version), self.text_edits(&text, &refiled.source)));
        }
        edits.retain(|(_, _, edits)| !edits.is_empty());
        for (uri, _, _) in &edits {
            self.edited.insert(uri.as_str().to_string());
        }
        let edit = if delete {
            self.edits_and_delete(edits, uri)
        } else if edits.is_empty() {
            return Ok(Value::Null);
        } else {
            self.workspace_edits(edits)
        };
        let params = lsp_types::ApplyWorkspaceEditParams { label: None, edit };
        self.apply_edit(params, Pending::Edit);
        Ok(Value::Null)
    }

    /// Edits of documents, then a file deleted.
    fn edits_and_delete(&self, edits: Vec<(Uri, Option<i32>, Vec<TextEdit>)>, deleted: Uri) -> WorkspaceEdit {
        let mut operations: Vec<DocumentChangeOperation> = edits
            .into_iter()
            .map(|(uri, version, edits)| {
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version },
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
            })
            .collect();
        operations.push(DocumentChangeOperation::Op(ResourceOp::Delete(DeleteFile { uri: deleted, options: None })));
        WorkspaceEdit { document_changes: Some(DocumentChanges::Operations(operations)), ..Default::default() }
    }

    /// `org-roam-extract-subtree`: asks for the file (`Extract node to: `,
    /// `org_roam_extract_new_file_path` offered), gives the subtree at the
    /// cursor an `ID` unless it has one, and moves it to that file, its
    /// headline the file's node (`org-roam-promote-entire-buffer`).
    pub(super) fn roam_extract(&mut self, context: &RoamContext) -> Result<Value, Refusal> {
        let (uri, text, version, offset) = self.roam_here(context)?;
        let heading = edit::extracted_heading(&text, offset)?;
        let (property_context, keywords) = {
            let document = self.document(&uri).ok_or("This file is not open")?;
            let property_context =
                org_edit::property_context(&document.parse, &self.edit_settings, &uri::file_stem(&uri));
            (property_context, document.parse.todo_keywords().clone())
        };
        let roam = self.roam.as_ref().ok_or(super::OFF)?;
        let directory = roam.directory.clone();
        let Some(answer) = context.input.clone().filter(|_| context.step.as_deref() == Some("file")) else {
            let id = org_core::properties::entry_get(&text, heading, "ID", &property_context).unwrap_or_default();
            let template = roam.settings.extract_new_file_path.clone();
            let value = edit::extract_file_name(&template, &text, heading, &id, &keywords, self.date_time());
            return Ok(json!({ "input": {
                "kind": "text",
                "prompt": "Extract node to: ",
                "value": value,
                "context": { "step": "file" },
            } }));
        };
        // Relative to `org_roam_directory`, as `expand-file-name` takes it.
        let given = expand(answer.trim());
        let path = if given.is_absolute() { given } else { directory.join(given) };
        let mut new_id = self.id_maker();
        let (made, _) = link::id_get_create(&text, heading, &property_context, &mut new_id).map_err(Refusal::Failed)?;
        let with_id = made.map_or_else(|| text.clone(), |(text, _)| text);
        if path.exists() {
            // The `ID` stays, as Emacs saves it before it asks.
            if with_id != text {
                self.send_texts(vec![(uri, Some(version), text, with_id)]);
            }
            return Err(format!("{} exists. Aborting", path.display()).into());
        }
        let structure = self.structure_settings(&text);
        let (cut, extracted) =
            edit::extract_subtree(&with_id, heading, &keywords, &structure, &self.edit_settings.tags)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("Cannot make {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, &extracted).map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
        self.send_texts(vec![(uri, Some(version), text, cut)]);
        Ok(Value::Null)
    }
}
