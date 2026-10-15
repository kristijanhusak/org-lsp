//! `org-roam-graph`: org-roam's nodes and the links between them as a
//! Graphviz graph (`org-roam-graph--dot`), all of them or those connected
//! to a node (`org-roam-graph--connected-component`).

use std::collections::{HashMap, HashSet};

use super::Db;
use crate::text::{char_width, width};

/// The options of `org-roam-graph`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-roam-graph-extra-config`: the graph's attributes.
    pub extra: Vec<(String, String)>,
    /// `org-roam-graph-edge-extra-config`
    pub edge: Vec<(String, String)>,
    /// `org-roam-graph-node-extra-config`: the attributes of the nodes of
    /// each link type (`id` for org-roam's nodes).
    pub node: Vec<(String, Vec<(String, String)>)>,
    /// `org-roam-graph-link-hidden-types`
    pub hidden: Vec<String>,
    /// `org-roam-graph-max-title-length`
    pub max_title_length: usize,
    /// `org-roam-graph-shorten-titles`
    pub shorten: Shorten,
}

/// The values of `org-roam-graph-shorten-titles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shorten {
    /// `truncate`: cut, with `...` at the end.
    Truncate,
    /// `wrap`: over lines (`org-roam-word-wrap`).
    Wrap,
    /// `nil`: as they are.
    No,
}

impl Default for Settings {
    fn default() -> Self {
        let pairs = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
        };
        let link = pairs(&[
            ("style", "rounded,filled"),
            ("fillcolor", "#EEEEEE"),
            ("color", "#C9C9C9"),
            ("fontcolor", "#0A97A6"),
        ]);
        Settings {
            extra: Vec::new(),
            edge: Vec::new(),
            node: vec![
                (
                    "id".to_string(),
                    pairs(&[
                        ("style", "bold,rounded,filled"),
                        ("fillcolor", "#EEEEEE"),
                        ("color", "#C9C9C9"),
                        ("fontcolor", "#111111"),
                    ]),
                ),
                ("http".to_string(), link.clone()),
                ("https".to_string(), link),
            ],
            hidden: vec!["file".to_string()],
            max_title_length: 100,
            shorten: Shorten::Truncate,
        }
    }
}

/// A row of `links`: its source, destination and type.
pub type Edge = (String, String, String);

/// `SELECT DISTINCT source, dest, type FROM links`, in the table's order.
fn edges(db: &Db, keep: impl Fn(&str, &str) -> bool) -> Vec<Edge> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (_, link) in &db.links {
        let edge = (link.source.clone(), link.dest.clone(), link.kind.clone());
        if keep(&edge.0, &edge.1) && seen.insert(edge.clone()) {
            out.push(edge);
        }
    }
    out
}

/// `org-roam-graph--connected-component`: the links from or to the nodes
/// reachable from `id` along links either way, within `distance - 1`
/// steps, or any number of steps when `distance` is 0 (from the nodes
/// next to `id`, so that it is among them only when it has links).
pub fn connected_component(db: &Db, id: &str, distance: usize) -> Vec<Edge> {
    let mut neighbours: HashMap<&str, Vec<&str>> = HashMap::new();
    for (_, link) in &db.links {
        neighbours.entry(&link.source).or_default().push(&link.dest);
        neighbours.entry(&link.dest).or_default().push(&link.source);
    }
    let next = |node: &str| neighbours.get(node).cloned().unwrap_or_default();
    let mut found: HashSet<&str> = HashSet::new();
    if distance == 0 {
        let mut todo: Vec<&str> = next(id);
        while let Some(node) = todo.pop() {
            if found.insert(node) {
                todo.extend(next(node));
            }
        }
    } else {
        // A trace of `distance` nodes at most, the start the first.
        found.insert(id);
        let mut layer = vec![id];
        for _ in 1..distance {
            let mut further = Vec::new();
            for node in layer {
                for other in next(node) {
                    if found.insert(other) {
                        further.push(other);
                    }
                }
            }
            layer = further;
        }
    }
    edges(db, |source, dest| found.contains(source) || found.contains(dest))
}

/// `org-roam-graph--dot`: the graph of `edges` (all the links when there
/// are none), and with `all_nodes` the nodes without links too.
pub fn dot(db: &Db, edges_given: Option<Vec<Edge>>, all_nodes: bool, settings: &Settings) -> String {
    let edges = match edges_given {
        Some(edges) if !edges.is_empty() => edges,
        _ => edges(db, |_, _| true),
    };
    // The nodes by `ID`, the last of an `ID` winning (`puthash`), in the
    // order of the table.
    let mut titles: HashMap<&str, &str> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for (_, node) in &db.nodes {
        if titles.insert(&node.id, &node.title).is_none() {
            order.push(&node.id);
        }
    }
    let option = |(key, value): &(String, String)| format!("{key}=\"{value}\"");
    let mut out = String::from("digraph \"org-roam\" {\n");
    for (key, value) in &settings.extra {
        out.push_str(&format!("{key}={value};\n"));
    }
    out.push_str(&format!(" edge [{}];\n", settings.edge.iter().map(option).collect::<Vec<_>>().join(",")));
    let format_node = |id: &str, kind: &str| {
        let mut properties: Vec<(String, String)> =
            settings.node.iter().find(|(name, _)| name == kind).map(|(_, pairs)| pairs.clone()).unwrap_or_default();
        match titles.get(id) {
            Some(title) => {
                let title = quote(title);
                let shortened = quote(&match settings.shorten {
                    Shorten::Truncate => truncate(&title, settings.max_title_length),
                    Shorten::Wrap => word_wrap(&title, settings.max_title_length),
                    Shorten::No => title.clone(),
                });
                properties.push(("label".to_string(), shortened));
                let url = format!("org-protocol://roam-node?node={}", crate::open::hexify(id));
                properties.push(("URL".to_string(), url));
                properties.push(("tooltip".to_string(), xml_escape(&title)));
            }
            None => {
                properties.push(("label".to_string(), format!("{kind}:{id}")));
                if kind == "http" || kind == "https" {
                    properties.push(("URL".to_string(), xml_escape(&format!("{kind}:{id}"))));
                }
            }
        }
        format!("\"{id}\" [{}];\n", properties.iter().map(option).collect::<Vec<_>>().join(","))
    };
    let mut seen: HashSet<&str> = HashSet::new();
    for (source, dest, kind) in &edges {
        if settings.hidden.contains(kind) {
            continue;
        }
        for (node, node_kind) in [(source.as_str(), "id"), (dest.as_str(), kind.as_str())] {
            if seen.insert(node) {
                out.push_str(&format_node(node, node_kind));
            }
        }
        out.push_str(&format!("  \"{}\" -> \"{}\";\n", xml_escape(source), xml_escape(dest)));
    }
    if all_nodes {
        for id in order {
            if !seen.contains(id) {
                out.push_str(&format_node(id, "id"));
            }
        }
    }
    out.push('}');
    out
}

/// `org-roam-quote-string`
fn quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `xml-escape-string`
fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// `truncate-string-to-width` to `length` columns with `...` as the
/// ellipsis: the characters that fit with it, when the text does not.
fn truncate(text: &str, length: usize) -> String {
    if width(text) <= length {
        return text.to_string();
    }
    let room = length.saturating_sub(3);
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        if used + char_width(c) > room {
            break;
        }
        used += char_width(c);
        out.push(c);
    }
    out.push_str("...");
    out
}

/// `org-roam-word-wrap`: `fill-region` with `fill-column` `length`: words
/// on a line while it stays within as many columns, a line broken
/// between any two wide characters too.
fn word_wrap(text: &str, length: usize) -> String {
    // Each piece that may start a line, and whether a space comes before it.
    let mut pieces: Vec<(bool, String)> = Vec::new();
    let mut space = false;
    for c in text.chars() {
        if c == ' ' || c == '\t' || c == '\n' {
            space = true;
            continue;
        }
        let wide = char_width(c) > 1;
        match pieces.last_mut() {
            Some((_, piece)) if !space && !wide && !piece.chars().last().is_some_and(|last| char_width(last) > 1) => {
                piece.push(c)
            }
            _ => pieces.push((space, c.to_string())),
        }
        space = false;
    }
    let mut out = String::new();
    let mut column = 0;
    for (spaced, piece) in pieces {
        let gap = usize::from(spaced && column > 0);
        if column > 0 && column + gap + width(&piece) > length {
            out.push('\n');
            column = 0;
        } else if gap == 1 {
            out.push(' ');
            column += 1;
        }
        out.push_str(&piece);
        column += width(&piece);
    }
    out
}
