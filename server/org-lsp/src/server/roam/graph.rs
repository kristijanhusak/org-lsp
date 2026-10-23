//! `org-roam-graph`: the nodes and their links drawn by Graphviz, and the
//! picture shown.

use std::path::{Path, PathBuf};

use lsp_types::request::ShowDocument;
use lsp_types::ShowDocumentParams;
use org_core::roam::{edit, graph};
use org_core::{edit as org_edit, Refusal};
use serde_json::{json, Value};

use super::{RoamContext, OFF};
use crate::server::Server;
use crate::{port, uri};

impl Server {
    /// `org-roam-graph`: every node without a count; with one, the nodes
    /// connected to the node at the cursor, as many steps away as it says
    /// (`C-u`, 4, any number of steps). The graph is drawn by
    /// `org_roam_graph_executable` and shown by `org_roam_graph_viewer`,
    /// or by the editor as the system opens the file.
    pub(super) fn roam_graph(&mut self, context: &RoamContext) -> Result<Value, Refusal> {
        let settings = self.roam.as_ref().ok_or(OFF)?.settings.clone();
        let db = self.roam_db()?.0;
        let edges = match context.count {
            None => None,
            Some(count) => {
                let (uri, text, _, offset) = self.roam_here(context)?;
                let property_context = {
                    let document = self.document(&uri).ok_or("This file is not open")?;
                    org_edit::property_context(&document.parse, &self.edit_settings, &uri::file_stem(&uri))
                };
                let node = edit::node_at(&text, offset, &property_context)?;
                // `C-u` is a cons in Emacs: no limit.
                let distance = if count == 4 { 0 } else { count.unsigned_abs() as usize };
                Some(graph::connected_component(&db, &node.id, distance))
            }
        };
        let all_nodes = edges.is_none();
        let dot = graph::dot(&db, edges, all_nodes, &settings.graph);
        let executable = settings.graph_executable.as_str();
        let found = if Path::new(executable).components().count() > 1 {
            Some(PathBuf::from(executable)).filter(|path| path.is_file())
        } else {
            port::on_path(executable)
        };
        let Some(executable) = found else {
            return Err(format!(
                "Cannot find executable \"{executable}\" to generate the graph.  \
                 Please adjust ‘org-roam-graph-executable’"
            )
            .into());
        };
        let directory = self.roam.as_ref().ok_or(OFF)?.graph_directory.path().to_path_buf();
        let source = directory.join("graph.dot");
        let picture = directory.join(format!("graph.{}", settings.graph_filetype));
        std::fs::write(&source, dot).map_err(|error| format!("Cannot write {}: {error}", source.display()))?;
        let mut command = std::process::Command::new(&executable);
        command
            .arg(&source)
            .arg("-T")
            .arg(&settings.graph_filetype)
            .arg("-o")
            .arg(&picture)
            .stdin(std::process::Stdio::null());
        // A graph too big to draw in time is given up.
        let drawn = crate::process::output(&mut command, crate::process::PROGRAM_TIME, crate::process::PROGRAM_SIZE)
            .map_err(|error| format!("Cannot run {}: {error}", executable.display()))?;
        // Emacs shows nothing when Graphviz fails; org-lsp says why.
        if !drawn.status.success() {
            return Err(String::from_utf8_lossy(&drawn.stderr).trim().to_string().into());
        }
        // `org-roam-graph--open`
        match &settings.graph_viewer {
            Some(viewer) => {
                let viewer = port::on_path(viewer)
                    .or_else(|| Some(PathBuf::from(viewer)).filter(|path| path.is_file()))
                    .ok_or_else(|| format!("Executable not found: \"{viewer}\""))?;
                let mut child = std::process::Command::new(viewer)
                    .arg(&picture)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|error| format!("Failed to open org-roam graph: {error}"))?;
                std::thread::spawn(move || child.wait());
            }
            None => {
                let uri = uri::from_path(&picture).ok_or("Cannot name the graph's file")?;
                let params = ShowDocumentParams { uri, external: Some(true), take_focus: None, selection: None };
                self.send_request::<ShowDocument>(params, None);
            }
        }
        Ok(json!({ "message": "(org-roam) building graph" }))
    }
}
