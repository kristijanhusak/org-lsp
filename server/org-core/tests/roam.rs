//! org-roam's database against Emacs: `tests/roam/cases.json` is written
//! by `scripts/update-roam-fixtures.py`, files of an `org-roam-directory`
//! with the rows `org-roam-db-sync` gives them, the nodes a completion
//! offers and slugs. `ORG_ROAM_CASES` points the test at another file.

use std::collections::HashMap;
use std::path::Path;

use org_core::outline::{Inherit, Inheritance, Outline};
use org_core::roam::{self, Db};
use serde_json::{json, Value};

fn cases_path() -> std::path::PathBuf {
    match std::env::var_os("ORG_ROAM_CASES") {
        Some(path) => path.into(),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam/cases.json"),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// What org-roam records for a case, in the reference's shape.
fn run(case: &Value) -> Value {
    let options = &case["options"];
    let mut settings = roam::Settings::default();
    if let Some(exclude) = options.get("org_roam_file_exclude_regexp") {
        settings.exclude = strings(exclude);
    }
    if let Some(extensions) = options.get("org_roam_file_extensions") {
        settings.extensions = strings(extensions);
    }
    let mut inheritance = Inheritance::default();
    if let Some(excluded) = options.get("org_tags_exclude_from_inheritance") {
        inheritance.tags_excluded = strings(excluded);
    }
    match options.get("org_use_tag_inheritance") {
        Some(Value::Bool(false)) => inheritance.tags = Inherit::Nothing,
        Some(Value::String(pattern)) => inheritance.tags = Inherit::Matching(pattern.clone()),
        _ => {}
    }
    let files = case["files"].as_object().expect("files");
    let mut names: Vec<String> = files.keys().cloned().collect();
    roam::sync_order(&mut names);
    let mut db = Db::default();
    let mut texts: HashMap<String, String> = HashMap::new();
    for name in &names {
        if !roam::is_roam_file(name, &settings) {
            continue;
        }
        let text = files[name.as_str()].as_str().unwrap_or_default();
        let name = &name;
        let todo = org_syntax::TodoKeywords::from_document(text).unwrap_or_default();
        let parse = org_syntax::parse(text, &todo);
        let base = name.rsplit('/').next().unwrap_or(name);
        let stem = base.rfind('.').filter(|at| *at > 0).map_or(base, |at| &base[..at]);
        let outline = Outline::with_inheritance(&parse, text, stem, &inheritance);
        // An encrypted file cannot be read without its key.
        let encrypted = name.ends_with(".gpg") || name.ends_with(".age");
        let data = if encrypted { Err("encrypted".to_string()) } else { roam::scan(text, &parse, &outline, name) };
        db.add(name, data);
        texts.insert(name.to_string(), text.to_string());
    }
    let position = |file: &str, offset: usize| texts[file][..offset].chars().count() + 1;
    let null_if_empty = |list: &[String]| if list.is_empty() { Value::Null } else { json!(list) };
    let mut nodes: Vec<(&String, &roam::Node)> = db.nodes.iter().map(|(file, node)| (file, node)).collect();
    nodes.sort_by_key(|(file, node)| ((*file).clone(), node.offset));
    let order: HashMap<&str, usize> = nodes.iter().enumerate().map(|(at, (_, node))| (node.id.as_str(), at)).collect();
    let by_node = |id: &str| order.get(id).copied().unwrap_or(order.len());
    let mut files_rows: Vec<Value> = db.files.iter().map(|(file, title)| json!([file, title])).collect();
    files_rows.sort_by_key(|row| row[0].as_str().unwrap_or_default().to_string());
    let node_rows: Vec<Value> = nodes
        .iter()
        .map(|(file, node)| {
            let properties: Vec<Value> = node.properties.iter().map(|(key, value)| json!([key, value])).collect();
            json!([
                node.id,
                file,
                node.level,
                position(file, node.offset),
                node.todo,
                node.priority,
                node.scheduled,
                node.deadline,
                node.title,
                properties,
                null_if_empty(&node.olp),
            ])
        })
        .collect();
    let mut aliases = db.aliases.clone();
    aliases.sort_by_key(|(id, _)| by_node(id));
    let mut tags = db.tags.clone();
    tags.sort_by_key(|(id, _)| by_node(id));
    let mut refs = db.refs.clone();
    refs.sort_by_key(|reference| by_node(&reference.node));
    let mut links = db.links.clone();
    links.sort_by_key(|(file, link)| (link.source.clone(), position(file, link.offset)));
    let mut citations = db.citations.clone();
    citations.sort_by_key(|(file, citation)| (citation.node.clone(), position(file, citation.offset)));
    let slugs: Vec<String> = strings(&case["titles"]).iter().map(|title| roam::slugify(title)).collect();
    // Each file is newer than the one before it, in the order of their names.
    let mut by_name: Vec<&String> = files.keys().collect();
    by_name.sort();
    let mtime = |file: &str| by_name.iter().position(|name| *name == file).unwrap_or(0) as u64;
    let candidates: Vec<Value> = strings(&case["templates"])
        .iter()
        .map(|template| match db.candidates(template, 79, &mtime) {
            Ok(lines) => json!(lines.iter().map(|line| line.display.clone()).collect::<Vec<_>>()),
            Err(error) => json!(error),
        })
        .collect();
    json!({
        "files": files_rows,
        "nodes": node_rows,
        "aliases": aliases.iter().map(|(id, alias)| json!([id, alias])).collect::<Vec<_>>(),
        "tags": tags.iter().map(|(id, tag)| json!([id, tag])).collect::<Vec<_>>(),
        "refs": refs.iter().map(|r| json!([r.node, r.reference, r.kind])).collect::<Vec<_>>(),
        "links": links
            .iter()
            .map(|(file, link)| {
                json!([position(file, link.offset), link.source, link.dest, link.kind, null_if_empty(&link.outline), link.search])
            })
            .collect::<Vec<_>>(),
        "citations": citations
            .iter()
            .map(|(file, citation)| {
                json!([citation.node, citation.key, position(file, citation.offset), null_if_empty(&citation.outline)])
            })
            .collect::<Vec<_>>(),
        "slugs": slugs,
        "candidates": candidates,
    })
}

#[test]
fn roam_matches_emacs() {
    let path = cases_path();
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let Some(expected) = case["expected"].as_object() else { continue };
        let actual = run(case);
        for (table, rows) in expected {
            if actual[table.as_str()] != *rows {
                failures
                    .push(format!("case {index}, {table}:\n  emacs:   {rows}\n  org-lsp: {}", actual[table.as_str()]));
            }
        }
    }
    assert!(failures.is_empty(), "{} differences from Emacs:\n{}", failures.len(), failures.join("\n"));
}

/// A template of the cases, as the server takes it.
fn template_of(value: &Value) -> roam::capture::Template {
    use roam::capture::{Target, Template};
    let target = strings(&value["target"]);
    let text = |at: usize| target.get(at).cloned().unwrap_or_default();
    let target = match target.first().map(String::as_str) {
        Some("file+head") => Target::FileHead(text(1), text(2)),
        Some("file+olp") => Target::FileOlp(text(1), target[2..].to_vec()),
        Some("file+head+olp") => Target::FileHeadOlp(text(1), text(2), target[3..].to_vec()),
        Some("node") => Target::Node(text(1)),
        _ => Target::File(text(1)),
    };
    let kind = match value["type"].as_str() {
        Some("plain") => org_core::capture::Kind::Plain,
        Some("item") => org_core::capture::Kind::Item,
        Some("checkitem") => org_core::capture::Kind::CheckItem,
        Some("table-line") => org_core::capture::Kind::TableLine,
        _ => org_core::capture::Kind::Entry,
    };
    let body = match &value["template"] {
        Value::Array(lines) => lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"),
        other => other.as_str().unwrap_or_default().to_string(),
    };
    Template {
        kind,
        body,
        target,
        properties: org_core::capture::Properties {
            prepend: value["prepend"].as_bool().unwrap_or(false),
            ..Default::default()
        },
        unnarrowed: value["unnarrowed"].as_bool().unwrap_or(false),
    }
}

/// What a capture case makes: the files new or changed, and the prompts.
fn run_capture(case: &Value) -> Value {
    const CURSOR: char = '¦';
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    let directory = Path::new("/roam");
    let mut files: HashMap<String, String> = HashMap::new();
    let mut cursor = None;
    if let Some(given) = case["files"].as_object() {
        for (name, text) in given {
            let text = text.as_str().unwrap_or_default();
            if let Some(at) = text.find(CURSOR) {
                cursor = Some((name.clone(), at));
            }
            files.insert(name.clone(), text.replace(CURSOR, ""));
        }
    }
    let default = json!({ "key": "d", "description": "default", "type": "plain", "template": "%?",
        "target": ["file+head", "%<%Y%m%d%H%M%S>-${slug}.org", "#+title: ${title}\n"], "unnarrowed": true });
    let templates = case["templates"].as_array().cloned().unwrap_or_else(|| vec![default]);
    let keys = case["keys"].as_str().unwrap_or("");
    let chosen = templates.iter().find(|t| t["key"] == keys).unwrap_or(&templates[0]);
    let template = template_of(chosen);
    let title = case["title"].as_str().unwrap_or_default();
    let now = org_core::date::DateTime::new(org_core::date::Date::new(2026, 5, 18), 10, 30);
    let context = org_core::capture::Context {
        now,
        file: None,
        initial: String::new(),
        annotation: String::new(),
        link_props: Vec::new(),
        user_name: String::new(),
        clock_heading: String::new(),
        clock_link: String::new(),
        tags_column: -77,
        clipboard: String::new(),
    };
    let read_date = org_core::read_date::Settings::default();
    let levels = org_core::refile::Settings::default();
    let relative = |path: &Path| path.strip_prefix(directory).ok().map(|rest| rest.to_string_lossy().to_string());
    let read = |path: &Path| relative(path).and_then(|name| files.get(&name).cloned());
    let properties = |text: &str| org_core::properties::Context {
        keywords: org_syntax::TodoKeywords::from_document(text).unwrap_or_default(),
        category: "???".to_string(),
        ..Default::default()
    };
    let find_node = |_: &str| None;
    let given = strings(&case["answers"]);
    let mut answers: Vec<String> = Vec::new();
    let mut prompts: Vec<String> = Vec::new();
    let ready = loop {
        let request = roam::capture::Request {
            template: &template,
            title,
            id: ID,
            file: None,
            answers: &answers,
            context: &context,
            read_date: &read_date,
            directory,
            read: &read,
            properties: &properties,
            levels: &levels,
            find_node: &find_node,
        };
        match roam::capture::capture(&request) {
            Ok(roam::capture::Step::Ask(question)) => {
                let prompt = match &question {
                    org_core::capture::Question::Text { prompt, .. } => prompt.clone(),
                    other => format!("{other:?}"),
                };
                prompts.push(if prompt.ends_with(": ") { prompt } else { format!("{prompt}: ") });
                let Some(answer) = given.get(answers.len()) else { return json!("ERROR: no answer") };
                answers.push(answer.clone());
            }
            Ok(roam::capture::Step::Ready(ready)) => break ready,
            Err(error) => return json!(format!("ERROR: {error:?}")),
        }
    };
    let body = case["body"].as_str().unwrap_or_default();
    let shown = ready.placed.shown();
    let at = ready.placed.cursor - ready.placed.start;
    let edited = format!("{}{body}{}", &shown[..at], &shown[at..]);
    let finished = ready.placed.finish(&edited);
    let mut changed = serde_json::Map::new();
    changed.insert(relative(&ready.path).unwrap_or_default(), json!(finished));
    if case["finalize"] == "insert-link" {
        if let Some((name, offset)) = &cursor {
            let mut text = files[name].clone();
            text.insert_str(*offset, &format!("[[id:{}][{title}]]", ready.id));
            changed.insert(name.clone(), json!(text));
        }
    }
    json!({ "files": changed, "prompts": prompts })
}

#[test]
fn roam_capture_matches_emacs() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam/capture.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let actual = run_capture(case);
        if actual != case["expected"] {
            failures.push(format!(
                "case {index} ({}):\n  emacs:   {}\n  org-lsp: {actual}",
                case["title"], case["expected"]
            ));
        }
    }
    assert!(failures.is_empty(), "{} differences from Emacs:\n{}", failures.len(), failures.join("\n"));
}

/// The node lookup of `org-roam-node-from-title-or-alias`, over `db`.
fn lookup(db: &Db, name: &str) -> Result<Option<String>, String> {
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

/// What an editing case leaves: the file's text, and the message or the
/// error.
fn run_edit(case: &Value) -> Value {
    use org_core::roam::edit;
    const CURSOR: char = '¦';
    let files = case["files"].as_object().expect("files");
    let (name, text, at) = files
        .iter()
        .find_map(|(name, text)| {
            let text = text.as_str()?;
            text.find(CURSOR).map(|at| (name.clone(), text.replace(CURSOR, ""), at))
        })
        .expect("a cursor");
    let mut db = Db::default();
    let mut names: Vec<String> = files.keys().cloned().collect();
    roam::sync_order(&mut names);
    for file in &names {
        let body =
            if *file == name { text.clone() } else { files[file.as_str()].as_str().unwrap_or_default().to_string() };
        let todo = org_syntax::TodoKeywords::from_document(&body).unwrap_or_default();
        let parse = org_syntax::parse(&body, &todo);
        let outline = Outline::new(&parse, &body, "x");
        db.add(file, roam::scan(&body, &parse, &outline, file));
    }
    let todo = org_syntax::TodoKeywords::from_document(&text).unwrap_or_default();
    let parse = org_syntax::parse(&text, &todo);
    let stem = name.rsplit('/').next().unwrap_or(&name).trim_end_matches(".org").to_string();
    let outline = Outline::new(&parse, &text, &stem);
    let context =
        org_core::properties::Context { keywords: todo.clone(), category: stem.clone(), ..Default::default() };
    let settings = org_core::tags::Settings { auto_align: true, tags_column: -77, ..Default::default() };
    let arg = &case["arg"];
    let text_arg = arg.as_str().unwrap_or_default();
    let list_arg = strings(arg);
    let done = |result: Result<(String, Option<String>), String>| match result {
        Ok((text, said)) => json!({ "text": text, "message": said }),
        Err(error) => json!({ "text": text, "error": error }),
    };
    let command = case["command"].as_str().unwrap_or_default();
    if command == "replace-links" {
        let (edited, error) = edit::replace_links(&text, &parse, &|name| lookup(&db, name));
        return match error {
            Some(error) => json!({ "text": edited, "error": error }),
            None => json!({ "text": edited, "message": null }),
        };
    }
    let point = match edit::node_at(&text, at, &context) {
        // Where the database has the node: a position of its file, which
        // Emacs takes in this one when the node is in another.
        Ok(found) => match db.nodes.iter().find(|(_, node)| node.id == found.id) {
            Some((file, node)) if *file == name => node.offset,
            Some((file, node)) => {
                let chars = files[file.as_str()].as_str().unwrap_or_default()[..node.offset].chars().count();
                text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
            }
            None => found.point,
        },
        Err(refusal) => return done(Err(refusal.message().to_string())),
    };
    // A deliberate difference: when the database does not have the node
    // (its file cannot be read, or has changed since), Emacs's
    // `org-set-tags` changes the line the cursor is on as if it were the
    // headline; org-lsp changes the headline.
    let heading = org_core::properties::heading_at(&text, point);
    if command.starts_with("tag") && heading.is_some_and(|heading| heading != point) {
        return json!("not compared");
    }
    let property = |name: &str, remove: bool| {
        if remove {
            edit::property_remove(&text, point, name, text_arg, &context)
        } else {
            edit::property_add(&text, point, name, text_arg, &context).map(|text| (text, None))
        }
    };
    done(match command {
        "alias-add" => property("ROAM_ALIASES", false),
        "alias-remove" => property("ROAM_ALIASES", true),
        "ref-add" => property("ROAM_REFS", false),
        "ref-remove" => property("ROAM_REFS", true),
        "tag-add" => Ok((edit::tag_add(&text, &outline, point, &list_arg, &settings), None)),
        "tag-remove" => edit::tag_remove(&text, &outline, point, &list_arg, &settings)
            .map(|text| (text, None))
            .map_err(|refusal| refusal.message().to_string()),
        other => Err(format!("unknown command {other}")),
    })
}

#[test]
fn roam_edits_match_emacs() {
    let path = match std::env::var_os("ORG_ROAM_EDIT_CASES") {
        Some(path) => path.into(),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam/edit.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let actual = run_edit(case);
        if actual != case["expected"] && actual != "not compared" {
            failures.push(format!(
                "case {index} ({} {}):\n  emacs:   {}\n  org-lsp: {actual}",
                case["command"], case["arg"], case["expected"]
            ));
        }
    }
    assert!(failures.is_empty(), "{} differences from Emacs:\n{}", failures.len(), failures.join("\n"));
}

/// What org-roam's buffer shows for the node at the cursor: its title and
/// text, as Emacs shows them.
fn run_buffer(case: &Value) -> Value {
    use org_core::roam::{buffer, edit};
    const CURSOR: char = '¦';
    let given = case["files"].as_object().expect("files");
    let mut files: HashMap<String, String> = HashMap::new();
    let mut cursor = None;
    for (name, text) in given {
        let text = text.as_str().unwrap_or_default();
        if let Some(at) = text.find(CURSOR) {
            cursor = Some((name.clone(), at));
        }
        files.insert(name.clone(), text.replace(CURSOR, ""));
    }
    let (name, at) = cursor.expect("a cursor");
    let mut names: Vec<String> = files.keys().cloned().collect();
    roam::sync_order(&mut names);
    let settings = roam::Settings::default();
    let mut db = Db::default();
    for file in names.iter().filter(|name| roam::is_roam_file(name, &settings)) {
        let text = &files[file];
        let todo = org_syntax::TodoKeywords::from_document(text).unwrap_or_default();
        let parse = org_syntax::parse(text, &todo);
        let outline = Outline::new(&parse, text, "x");
        db.add(file, roam::scan(text, &parse, &outline, file));
    }
    let text = &files[&name];
    let context = org_core::properties::Context {
        keywords: org_syntax::TodoKeywords::from_document(text).unwrap_or_default(),
        ..Default::default()
    };
    // `org-roam-node-at-point` without ASSERT.
    let Ok(found) = edit::node_at(text, at, &context) else { return Value::Null };
    let kinds: Vec<buffer::Kind> = strings(&case["sections"])
        .iter()
        .map(|name| match name.as_str() {
            "reflinks" => buffer::Kind::Reflinks,
            "unlinked-references" => buffer::Kind::UnlinkedReferences,
            _ => buffer::Kind::Backlinks,
        })
        .collect();
    let kinds = if kinds.is_empty() { vec![buffer::Kind::Backlinks, buffer::Kind::Reflinks] } else { kinds };
    // What ripgrep searches: every Org file of the directory.
    let mut all: Vec<String> = files
        .keys()
        .filter(|name| [".org", ".org.gpg", ".org.age"].iter().any(|ext| name.ends_with(ext)))
        .cloned()
        .collect();
    all.sort();
    let read = |file: &str| files.get(file).cloned();
    let sections = buffer::sections(&db, &found.id, &kinds, &read, &all);
    let title = db.nodes.iter().find(|(_, node)| node.id == found.id).map(|(_, node)| node.title.clone());
    json!({ "title": title, "text": buffer::plain(&sections) })
}

#[test]
fn roam_buffer_matches_emacs() {
    let path = match std::env::var_os("ORG_ROAM_BUFFER_CASES") {
        Some(path) => path.into(),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam/buffer.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let actual = run_buffer(case);
        if actual != case["expected"] {
            failures.push(format!("case {index}:\n  emacs:   {}\n  org-lsp: {actual}", case["expected"]));
        }
    }
    assert!(failures.is_empty(), "{} differences from Emacs:\n{}", failures.len(), failures.join("\n"));
}
