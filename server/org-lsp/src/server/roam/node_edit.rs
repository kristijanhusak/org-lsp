//! The node at the cursor changed: its aliases, tags and references
//! (`org-roam-alias-add` and the like), and `roam:` links: followed
//! (`org-roam-link-follow-link`), turned into `id:` links before a file is
//! saved (`org-roam-link-replace-all`), and completed with the titles of
//! the nodes (`org-roam-complete-link-at-point`,
//! `org-roam-complete-everywhere`).

use lsp_types::{Range, TextEdit, Uri};
use org_core::roam::{edit, Db};
use org_core::{edit as org_edit, Refusal};
use serde_json::{json, Value};

use super::RoamContext;
use crate::server::Server;
use crate::{commands, uri};

/// `org-roam-node-from-title-or-alias`: the `ID` of the node with this
/// title or alias, or why there is no one node.
pub(super) fn node_named(db: &Db, name: &str) -> Result<Option<String>, String> {
    let mut ids: Vec<&str> =
        db.nodes.iter().filter(|(_, node)| node.title == name).map(|(_, node)| node.id.as_str()).collect();
    ids.extend(db.aliases.iter().filter(|(_, alias)| alias == name).map(|(id, _)| id.as_str()));
    ids.sort();
    ids.dedup();
    match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(id.to_string())),
        _ => Err(format!("Multiple nodes exist with title or alias \"{name}\"")),
    }
}

/// `org-roam--get-titles`: every title and alias, once.
fn titles(db: &Db) -> Vec<String> {
    let mut titles: Vec<String> = db
        .nodes
        .iter()
        .map(|(_, node)| node.title.clone())
        .chain(db.aliases.iter().map(|(_, alias)| alias.clone()))
        .collect();
    titles.sort();
    titles.dedup();
    titles
}

/// The tags of `completing-read-multiple` with `crm-separator` set to
/// colons with blanks around them, as `org-roam-tag-add` reads them.
fn read_tags(answer: &str) -> Vec<String> {
    answer
        .split(':')
        .map(|tag| tag.trim_matches([' ', '\t']))
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

impl Server {
    /// Whether `uri` is one of org-roam's files (`org-roam-file-p`).
    pub(in crate::server) fn is_roam_file(&self, uri: &Uri) -> bool {
        let (Some(roam), Some(path)) = (self.roam.as_ref(), uri::to_path(uri)) else { return false };
        let Ok(relative) = path.strip_prefix(&roam.directory) else { return false };
        let relative =
            relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        org_core::roam::is_roam_file(&relative, &roam.settings.files)
    }

    /// `org-roam-alias-add`, `org-roam-alias-remove`, `org-roam-tag-add`,
    /// `org-roam-tag-remove`, `org-roam-ref-add` and `org-roam-ref-remove`
    /// on the node at the cursor. What they ask comes back with `step`
    /// `value`.
    pub(super) fn roam_edit(&mut self, id: &str, context: &RoamContext) -> Result<Value, Refusal> {
        let not_here = || Refusal::NotHere("This command works in Org files".to_string());
        let uri = context.text_document.as_ref().ok_or_else(not_here)?.uri.clone();
        if self.is_view(&uri) {
            return Err(not_here());
        }
        let encoding = self.client.encoding;
        let document = self.document(&uri).ok_or_else(not_here)?;
        let (text, version) = (document.text.clone(), document.version);
        let offset = document.index.offset(&text, context.position.unwrap_or_default(), encoding);
        let answer = context.input.clone().filter(|_| context.step.as_deref() == Some("value"));
        let ask = |prompt: &str, completions: Vec<String>, separator: Option<&str>| {
            let mut input = json!({ "kind": "text", "prompt": prompt, "value": "", "context": { "step": "value" } });
            if !completions.is_empty() {
                input["completions"] = json!(completions);
            }
            if let Some(separator) = separator {
                input["separator"] = json!(separator);
            }
            Ok(json!({ "input": input }))
        };
        let db = self.roam_db()?.0;
        // Those that add ask first, then look for the node.
        if answer.is_none() {
            match id {
                commands::ROAM_ALIAS_ADD => return ask("Alias", Vec::new(), None),
                commands::ROAM_REF_ADD => return ask("Ref", Vec::new(), None),
                // `org-roam-tag-completions`
                commands::ROAM_TAG_ADD => {
                    let mut tags: Vec<String> = db.tags.iter().map(|(_, tag)| tag.clone()).collect();
                    tags.sort();
                    tags.dedup();
                    for entry in &self.edit_settings.tags.alist {
                        if let org_core::tags::TagEntry::Tag(name, _) = entry {
                            if !tags.contains(name) {
                                tags.push(name.clone());
                            }
                        }
                    }
                    return ask("Tag", tags, Some(":"));
                }
                _ => {}
            }
        }
        let property_context = {
            let document = self.document(&uri).ok_or_else(not_here)?;
            org_edit::property_context(&document.parse, &self.edit_settings, &uri::file_stem(&uri))
        };
        let node = edit::node_at(&text, offset, &property_context)?;
        // Where the database has the node, when it has it: a position of
        // its file.
        let path = uri::to_path(&uri).map(|path| path.to_string_lossy().to_string()).unwrap_or_default();
        let point = match db.nodes.iter().find(|(_, found)| found.id == node.id) {
            Some((file, found)) if *file == path => found.offset.min(text.len()),
            _ => node.heading,
        };
        let outline = self.with_source(&uri, |_, _, _, outline, _| outline.clone()).ok_or("Cannot read the file")?;
        let values = |name: &str| edit::property_values(&text, point, name, &property_context);
        let (edited, said) = match id {
            commands::ROAM_ALIAS_ADD | commands::ROAM_REF_ADD => {
                let name = if id == commands::ROAM_ALIAS_ADD { "ROAM_ALIASES" } else { "ROAM_REFS" };
                let value = answer.unwrap_or_default();
                (edit::property_add(&text, point, name, &value, &property_context)?, None)
            }
            commands::ROAM_ALIAS_REMOVE | commands::ROAM_REF_REMOVE => {
                let name = if id == commands::ROAM_ALIAS_REMOVE { "ROAM_ALIASES" } else { "ROAM_REFS" };
                let Some(value) = answer else { return ask("Remove", values(name)?, None) };
                edit::property_remove(&text, point, name, &value, &property_context)?
            }
            commands::ROAM_TAG_ADD => {
                let tags = read_tags(&answer.unwrap_or_default());
                (edit::tag_add(&text, &outline, point, &tags, &self.edit_settings.tags), None)
            }
            commands::ROAM_TAG_REMOVE => {
                let current = edit::node_tags(&text, &outline, point).unwrap_or_default();
                let Some(answer) = answer else {
                    // Refused before anything is asked when there is none.
                    edit::tag_remove(&text, &outline, point, &[], &self.edit_settings.tags)?;
                    return ask("Tag", current, Some(":"));
                };
                (edit::tag_remove(&text, &outline, point, &read_tags(&answer), &self.edit_settings.tags)?, None)
            }
            _ => return Err(format!("Unknown command {id}").into()),
        };
        if edited != text {
            self.send_texts(vec![(uri, Some(version), text, edited)]);
        }
        Ok(match said {
            Some(message) => json!({ "message": message }),
            None => Value::Null,
        })
    }

    /// `org-roam-link-follow-link`: the node with the title or alias of a
    /// `roam:` link visited, the link first turned into an `id:` link
    /// (`org_roam_link_auto_replace`); a node made for a title no node
    /// has.
    pub(in crate::server) fn roam_follow(&mut self, uri: &Uri, offset: usize, title: &str) -> Result<Value, Refusal> {
        // Without org-roam, Org takes it for a fuzzy link.
        if self.roam.is_none() {
            return Err(format!("No match for fuzzy expression: roam:{title}").into());
        }
        let db = self.roam_db()?.0;
        let Some(id) = node_named(&db, title)? else {
            // Its questions come back to `org.open` with the title.
            let context = RoamContext { title: Some(title.to_string()), ..Default::default() };
            return self.roam_capture(commands::ROAM_NODE_FIND, &context, &db, title, None);
        };
        if self.roam.as_ref().is_some_and(|roam| roam.settings.link_auto_replace) {
            if let Some(document) = self.document(uri) {
                let (text, version) = (document.text.clone(), document.version);
                let link = edit::roam_links(&text, &document.parse)
                    .into_iter()
                    .find(|(start, end, _, _)| (*start..=*end).contains(&offset));
                if let Some((start, end, path, description)) = link {
                    let shown = description.unwrap_or(path);
                    if let Ok(made) = org_core::link::make_string(&format!("id:{id}"), Some(&shown)) {
                        let edited = format!("{}{made}{}", &text[..start], &text[end..]);
                        self.send_texts(vec![(uri.clone(), Some(version), text, edited)]);
                    }
                }
            }
        }
        self.roam_visit(&db, &id)
    }

    /// Before one of org-roam's files is saved: its `roam:` links to nodes
    /// turned into `id:` links (`org-roam-link-replace-all`), when
    /// `org_roam_link_auto_replace` is on. The new text, if it changes.
    pub(in crate::server) fn roam_before_save(&mut self, uri: &Uri, text: &str) -> Option<String> {
        if !self.roam.as_ref().is_some_and(|roam| roam.settings.link_auto_replace) || !self.is_roam_file(uri) {
            return None;
        }
        if !text.contains("[[roam:") {
            return None;
        }
        let db = self.roam_db().ok()?.0;
        let parse = org_syntax::parse(text, &self.todo);
        let (edited, error) = edit::replace_links(text, &parse, &|name| node_named(&db, name));
        if let Some(error) = error {
            self.show_message(lsp_types::MessageType::WARNING, error);
        }
        (edited != text).then_some(edited)
    }

    /// The titles of the nodes, for a link being typed in one of
    /// org-roam's files, `[[title]]` or `[[roam:title]]`, which completing
    /// turns into `[[id:ID][title]]`; with
    /// `org_roam_completion_everywhere`, for the word at the cursor too,
    /// which becomes such a link.
    pub(in crate::server) fn roam_completion(&mut self, uri: &Uri, offset: usize) -> Vec<lsp_types::CompletionItem> {
        if !self.is_roam_file(uri) {
            return Vec::new();
        }
        let Some(document) = self.document(uri) else { return Vec::new() };
        let text = document.text.clone();
        let index = crate::line_index::LineIndex::new(&text);
        let encoding = self.client.encoding;
        let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
        let line_end = text[offset..].find('\n').map_or(text.len(), |at| offset + at);
        let in_src = org_core::roam::edit::in_src_block(&document.parse, offset);
        let everywhere = self.roam.as_ref().is_some_and(|roam| roam.settings.completion_everywhere);
        // `org-roam-bracket-completion-re` around the cursor: `[[`, maybe
        // `roam:`, the title, `]]`.
        let link = {
            let before = &text[line_start..offset];
            before.rfind("[[").and_then(|open| {
                let open = line_start + open;
                let uri_start = open + 2;
                let title_start = if text[uri_start..].starts_with("roam:") { uri_start + 5 } else { uri_start };
                let rest = &text[title_start..line_end];
                let close = rest.find("]]").map(|at| title_start + at);
                let title_end = close.unwrap_or(offset);
                let title = &text[title_start..title_end];
                let valid = title_start <= offset
                    && offset <= title_end
                    && !title.contains(['[', ']'])
                    && !text[title_start..offset].contains("]");
                valid.then_some((uri_start, title_start, title_end, close.is_some()))
            })
        };
        let word = || {
            let is_word = |c: char| c.is_alphanumeric();
            let start = text[line_start..offset]
                .char_indices()
                .rev()
                .take_while(|(_, c)| is_word(*c))
                .last()
                .map_or(offset, |(at, _)| line_start + at);
            let end =
                offset + text[offset..line_end].chars().take_while(|c| is_word(*c)).map(char::len_utf8).sum::<usize>();
            (start < end).then_some((start, end))
        };
        let (replace_start, filter_start, replace_end, wrap) = match link {
            Some((uri_start, title_start, title_end, closed)) if !in_src => {
                (uri_start, title_start, if closed { title_end + 2 } else { title_end }, false)
            }
            None if everywhere && !in_src && !org_core::roam::edit::in_link(&document.parse, offset) => match word() {
                Some((start, end)) => (start, start, end, true),
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let Ok((db, _)) = self.roam_db() else { return Vec::new() };
        let position = |at: usize| index.position(&text, at, encoding);
        titles(&db)
            .into_iter()
            .filter_map(|title| {
                let id = node_named(&db, &title).ok().flatten()?;
                let new_text = if wrap { format!("[[id:{id}][{title}]]") } else { format!("id:{id}][{title}]]") };
                // The title is what is typed; the link's start goes too.
                let additional = (replace_start < filter_start).then(|| {
                    vec![TextEdit {
                        range: Range::new(position(replace_start), position(filter_start)),
                        new_text: String::new(),
                    }]
                });
                Some(lsp_types::CompletionItem {
                    label: title.clone(),
                    kind: Some(lsp_types::CompletionItemKind::REFERENCE),
                    detail: Some("org-roam node".to_string()),
                    filter_text: Some(title),
                    text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit {
                        range: Range::new(position(filter_start), position(replace_end)),
                        new_text,
                    })),
                    additional_text_edits: additional,
                    ..Default::default()
                })
            })
            .collect()
    }
}
