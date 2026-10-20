//! Following `man:` and `info:` links, and `org-ctags`.
//!
//! Emacs shows a man page (`org-man-open`) or an Info node
//! (`org-info-open`) in a buffer. Here `man`, or the `info` program, writes
//! it as text into a file of the server's, which the editor shows. The
//! manuals on the web (`org-info-other-documents`, Emacs's own) open there
//! instead, at the page Org's export links them to.
//!
//! With `org-ctags` in `org-modules`, a link to a headline, a target or a
//! custom ID goes through `org-ctags-open-link-functions` first: the tag of
//! that name in the `TAGS` file of the link's directory, a new topic, an
//! Org file of that name. Their questions are inputs of kind `key`, the
//! answers so far in the context (`answers`).

use std::path::{Path, PathBuf};

use lsp_types::request::ShowDocument;
use lsp_types::{Position, Range, ShowDocumentParams, TextEdit, Uri};
use org_core::ctags::{self, Outcome};
use org_core::manual;
use serde_json::{json, Value};

use super::{shell_output, Context, LineIndex, Pending, Refusal, Server};
use crate::uri;

/// A file name for a page: what is not a letter, a digit, `.` or `-` made
/// `_`.
fn page_file(name: &str) -> String {
    let name: String =
        name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    format!("{name}.txt")
}

/// Run `program` with `arguments`, its output as text when it ends well;
/// stopped when it takes too long ([`crate::process::PROGRAM_TIME`]).
fn run(program: &str, arguments: &[&str]) -> std::io::Result<Option<String>> {
    let mut command = std::process::Command::new(program);
    command
        .args(arguments)
        // One page, as text: no pager, no colors, the width Emacs's `man`
        // gives a frame of 80 columns.
        .env("MANWIDTH", "80")
        .env("MANPAGER", "cat")
        .env("PAGER", "cat")
        .env("GROFF_NO_SGR", "1")
        .env_remove("MAN_KEEP_FORMATTING")
        .stdin(std::process::Stdio::null());
    let output = crate::process::output(&mut command, crate::process::PROGRAM_TIME)?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((output.status.success() && !text.trim().is_empty()).then_some(text))
}

impl Server {
    /// `org-man-open`: the page `org_man_command` writes, shown at the
    /// line before the first match of the search, if any.
    pub(super) fn open_man(&mut self, page: &str, search: Option<&str>) -> Result<Value, Refusal> {
        let arguments = manual::man_arguments(page);
        if let Some(refused) = manual::man_refused(&arguments) {
            return Err(refused.into());
        }
        let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let program = self.man_command.clone();
        let written = run(&program, &words).map_err(|error| Refusal::Failed(format!("{program}: {error}")))?;
        let text = manual::plain_page(&written.ok_or_else(|| manual::man_not_found(&arguments))?);
        let line = match search {
            Some(search) => manual::man_search_line(&text, search)?,
            None => 0,
        };
        let path = self.manual_directory.path().join(page_file(&format!("Man {}", arguments.join(" "))));
        self.show_page(&path, &text, line)
    }

    /// `org-info-open`: a manual on the web at its node's anchor; another
    /// as the `info` program writes the node, else the entry of the index
    /// of that name, as `org-info-follow-link` looks for it.
    pub(super) fn open_info(&mut self, file: &str, node: &str) -> Result<Value, Refusal> {
        if manual::on_the_web(file, &self.info_other_documents) {
            let url = manual::info_url(file, node, &self.info_other_documents);
            let target = uri::from_url(&url).ok_or_else(|| format!("Cannot open {url}"))?;
            self.show_external(target);
            return Ok(Value::Null);
        }
        let info = |arguments: &[&str]| {
            run("info", &[&["--file", file], arguments, &["--output", "-"]].concat())
                .map_err(|error| Refusal::Failed(format!("info: {error}")))
        };
        let index = format!("--index-search={node}");
        let text = match info(&["--node", node])? {
            Some(text) => text,
            None => match info(&[index.as_str()])? {
                Some(text) => text,
                None if info(&["--node", "Top"])?.is_some() => return Err(manual::info_node_missing(node).into()),
                None => return Err(manual::info_missing(file).into()),
            },
        };
        let path = self.manual_directory.path().join(page_file(&format!("({file}) {node}")));
        self.show_page(&path, &text, 0)
    }

    /// A page in a file of the server's, shown at `line`.
    fn show_page(&mut self, path: &Path, text: &str, line: usize) -> Result<Value, Refusal> {
        let target = self.own_file_show(path, text, false, true)?;
        let at = Position::new(line as u32, 0);
        let params = ShowDocumentParams {
            uri: target,
            external: None,
            take_focus: Some(true),
            selection: Some(Range::new(at, at)),
        };
        self.send_request::<ShowDocument>(params, None);
        Ok(Value::Null)
    }

    /// `run-hook-with-args-until-success` on `org-open-link-functions` with
    /// org-ctags's, for the link to `name` in `text`, the file `uri`: what
    /// to answer the command with when one of them was done (or asks), or
    /// none for the link to be followed as without them.
    pub(super) fn ctags_follow(
        &mut self,
        context: &Context,
        uri: &Uri,
        text: &str,
        name: &str,
    ) -> Result<Option<Value>, Refusal> {
        let Some(settings) = self.ctags.clone() else { return Ok(None) };
        // The answers so far, `y` or `n` each, then the one given now; `r`
        // once the tags table was made again, which is done once.
        let mut answers = String::new();
        if context.step.as_deref() == Some("ctags") {
            answers = context.answers.clone().unwrap_or_default();
            answers.push(if matches!(context.input.as_deref(), Some("y" | "Y" | "yes")) { 'y' } else { 'n' });
        }
        let directory = uri::to_path(uri).and_then(|path| path.parent().map(PathBuf::from));
        let named = directory.as_ref().map(|directory| format!("{}/", directory.display()));
        let given: Vec<bool> = answers.chars().filter(|c| *c != 'r').map(|c| c == 'y').collect();
        let mut rebuilt = answers.contains('r');
        let outcome = {
            let context = ctags::Context {
                functions: &settings.functions,
                answers: &given,
                directory: named.as_deref(),
                text,
                template: &settings.template,
            };
            let mut tags = |rebuild: bool| {
                let Some(directory) = &directory else { return Vec::new() };
                if rebuild && !rebuilt {
                    // `org-ctags-create-tags`, whose exit code Emacs does not
                    // look at.
                    let command = ctags::create_command(&settings.program, &format!("{}/", directory.display()));
                    let _ = shell_output(&command, Some(directory));
                    rebuilt = true;
                }
                std::fs::read(directory.join("TAGS"))
                    .map(|bytes| ctags::read_tags(&String::from_utf8_lossy(&bytes)))
                    .unwrap_or_default()
            };
            let exists = |file: &str| Path::new(file).is_file();
            ctags::follow(name, &context, &mut tags, &exists)
        };
        if rebuilt && !answers.contains('r') {
            answers.push('r');
        }
        let show = |server: &mut Server, target: Uri, line: u32| {
            let at = Position::new(line, 0);
            let selection = Some(Range::new(at, at));
            let params = ShowDocumentParams { uri: target, external: None, take_focus: Some(true), selection };
            server.send_request::<ShowDocument>(params, None);
        };
        match outcome {
            Outcome::Pass => return Ok(None),
            Outcome::Done => {}
            Outcome::Ask(prompt) => {
                return Ok(Some(json!({ "input": {
                    "kind": "key",
                    "prompt": prompt,
                    "value": "",
                    "choices": [{ "key": "y", "label": "yes" }, { "key": "n", "label": "no" }],
                    "context": { "step": "ctags", "answers": answers, "replies": context.replies },
                } })));
            }
            // `xref-find-definitions`: the start of the tag's line.
            Outcome::Found(tag) => {
                let file = match &directory {
                    Some(directory) if !Path::new(&tag.file).is_absolute() => directory.join(&tag.file),
                    _ => PathBuf::from(&tag.file),
                };
                let target = uri::from_path(&file).ok_or("Cannot open the file")?;
                show(self, target, tag.line.saturating_sub(1) as u32);
            }
            Outcome::Append(new, point) => {
                let version = self.document(uri).map(|document| document.version);
                let position = LineIndex::new(&new).position(&new, point, self.client.encoding);
                let edits = self.text_edits(text, &new);
                let edit = self.workspace_edit(uri, version, edits);
                let pending =
                    Pending::EditAndShow { uri: uri.clone(), position, folds: Value::Null, other_window: false };
                self.message = Some(format!("Adding topic in buffer {}", buffer_name(uri)));
                self.apply_edit(lsp_types::ApplyWorkspaceEditParams { label: None, edit }, pending);
            }
            Outcome::Visit(file) => {
                let target = uri::from_path(Path::new(&file)).ok_or("Cannot open the file")?;
                self.message = Some(format!("Opening existing org file ‘{file:?}’..."));
                show(self, target, 0);
            }
            // `org-ctags-open-file`: the file made, the topic in it.
            Outcome::Create(file, new) => {
                let target = uri::from_path(Path::new(&file)).ok_or("Cannot open the file")?;
                let position = LineIndex::new(&new).position(&new, new.len(), self.client.encoding);
                self.message = Some("Inserted new file text OK".to_string());
                if self.client.create_files {
                    let operations = vec![
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Create(lsp_types::CreateFile {
                            uri: target.clone(),
                            options: None,
                            annotation_id: None,
                        })),
                        lsp_types::DocumentChangeOperation::Edit(lsp_types::TextDocumentEdit {
                            text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                                uri: target.clone(),
                                version: None,
                            },
                            edits: vec![lsp_types::OneOf::Left(TextEdit { range: Range::default(), new_text: new })],
                        }),
                    ];
                    let edit = lsp_types::WorkspaceEdit {
                        document_changes: Some(lsp_types::DocumentChanges::Operations(operations)),
                        ..Default::default()
                    };
                    let pending =
                        Pending::EditAndShow { uri: target, position, folds: Value::Null, other_window: false };
                    self.apply_edit(lsp_types::ApplyWorkspaceEditParams { label: None, edit }, pending);
                } else {
                    // An editor that cannot make files gets it written.
                    std::fs::write(&file, new).map_err(|error| Refusal::Failed(error.to_string()))?;
                    show(self, target, position.line);
                }
            }
        }
        Ok(Some(Value::Null))
    }

    /// `org-ctags-create-tags` (`org.ctags.create`): the `TAGS` file of the
    /// directory of the file, made by `org_ctags_path_to_ctags`.
    pub(super) fn ctags_create(&mut self, context: &Context) -> Result<Value, Refusal> {
        let settings = self.ctags.clone().ok_or("org-ctags is off: `org-ctags` is not in `org_modules`")?;
        let file = uri::to_path(&context.text_document.uri).ok_or("Assertion failed: (buffer-file-name)")?;
        let directory = file.parent().map(PathBuf::from).ok_or("Assertion failed: (buffer-file-name)")?;
        let command = ctags::create_command(&settings.program, &format!("{}/", directory.display()));
        shell_output(&command, Some(&directory)).map_err(|error| Refusal::Failed(format!("{command}: {error}")))?;
        Ok(Value::Null)
    }

    /// `org-ctags-find-tag-interactive` (`org.ctags.find`): a topic asked
    /// for among the tags of the directory's `TAGS` file; one of them is
    /// shown, another goes through `org_ctags_open_link_functions`.
    pub(super) fn ctags_find(&mut self, context: &Context) -> Result<Value, Refusal> {
        if self.ctags.is_none() {
            return Err("org-ctags is off: `org-ctags` is not in `org_modules`".into());
        }
        let uri = context.text_document.uri.clone();
        let directory = uri::to_path(&uri).and_then(|path| path.parent().map(PathBuf::from));
        let tags = directory
            .as_ref()
            .and_then(|directory| std::fs::read(directory.join("TAGS")).ok())
            .map(|bytes| ctags::read_tags(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default();
        // The topic, typed now or with the first answer.
        let topic = match (context.replies.as_ref().and_then(|replies| replies.first()), &context.input) {
            (Some(topic), _) => topic.clone(),
            (None, Some(topic)) if !topic.is_empty() => topic.clone(),
            _ => {
                let mut names: Vec<String> = tags.iter().map(|tag| tag.name.clone()).collect();
                names.sort();
                names.dedup();
                return Ok(json!({ "input": {
                    "kind": "text", "prompt": "Topic: ", "value": "", "completions": names,
                } }));
            }
        };
        if context.replies.is_none() {
            if let Some(tag) = tags.iter().find(|tag| tag.name == topic) {
                let file = match &directory {
                    Some(directory) if !Path::new(&tag.file).is_absolute() => directory.join(&tag.file),
                    _ => PathBuf::from(&tag.file),
                };
                let target = uri::from_path(&file).ok_or("Cannot open the file")?;
                let at = Position::new(tag.line.saturating_sub(1) as u32, 0);
                let params = ShowDocumentParams {
                    uri: target,
                    external: None,
                    take_focus: Some(true),
                    selection: Some(Range::new(at, at)),
                };
                self.send_request::<ShowDocument>(params, None);
                return Ok(Value::Null);
            }
        }
        let text = self.document(&uri).map(|document| document.text.clone()).unwrap_or_default();
        let mut context = context.clone();
        context.replies = Some(vec![topic.clone()]);
        Ok(self.ctags_follow(&context, &uri, &text, &topic)?.unwrap_or(Value::Null))
    }
}

/// The name Emacs gives the buffer of a file: its name.
fn buffer_name(uri: &Uri) -> String {
    uri::to_path(uri)
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_default()
}
