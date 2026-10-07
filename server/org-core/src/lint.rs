//! `org-lint`: syntax mistakes in an Org text, as Org's checkers find
//! them, each with the place Org reports it at, its trust (`"nil"`,
//! `"low"`, `"high"`), its message and the checker's name.
//!
//! The checkers run in the order `org-lint--checkers` holds them (the
//! last one registered first), and their reports are sorted by place, as
//! `org-lint--generate-reports` sorts them.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use org_syntax::{ast, Parse, SyntaxKind, SyntaxNode};
use regex::{Regex, RegexBuilder};

use crate::indent::Elements;

/// A report of a checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Where it is.
    pub offset: usize,
    /// How much it can be trusted, as Org says: `"nil"` (no level given),
    /// `"low"` or `"high"`.
    pub trust: &'static str,
    pub message: String,
    /// The checker's name (`invalid-fuzzy-link`).
    pub checker: &'static str,
}

/// What the checkers need to know besides the text.
pub struct Context<'a> {
    /// Whether a file exists, given its name as the link or keyword
    /// writes it (relative to the text's directory, or with `~`).
    pub file_exists: &'a dyn Fn(&str) -> bool,
}

/// A checker's reports: places and messages.
type Found = Vec<(usize, String)>;

/// A footnote reference: its node, its label, `standard` or `inline`.
type Reference = (SyntaxNode, Option<String>, &'static str);

/// A footnote definition: its node and its label.
type Definition = (SyntaxNode, Option<String>);

/// The text, its tree and its elements, as the checkers read them.
struct Doc<'a> {
    text: &'a str,
    parse: &'a Parse,
    root: SyntaxNode,
    elements: Elements,
    context: &'a Context<'a>,
}

fn range(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == kind)
        .map(|token| token.text().to_string())
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].rfind('\n').map_or(0, |found| found + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |found| at + found)
}

/// Where an element starts once its affiliated keywords are past.
fn post_affiliated(text: &str, node: &SyntaxNode) -> usize {
    let first = node
        .children_with_tokens()
        .find(|child| child.as_node().is_none_or(|child| !ast::is_affiliated_keyword(child)));
    let begin = range(node).0;
    first.map_or(begin, |first| line_start(text, usize::from(first.text_range().start())).max(begin))
}

/// A keyword's key, upper case, as `org-element` reads it.
fn keyword_key(node: &SyntaxNode) -> String {
    let key = token(node, SyntaxKind::KEYWORD_KEY).unwrap_or_default();
    let key = key.trim_start().trim_start_matches("#+");
    let key = key.strip_suffix(':').unwrap_or(key);
    // `#+CAPTION[short]:`
    let key = key.split('[').next().unwrap_or(key);
    key.to_ascii_uppercase()
}

fn keyword_value(node: &SyntaxNode) -> String {
    token(node, SyntaxKind::KEYWORD_VALUE).unwrap_or_default().trim().to_string()
}

/// The keywords that are elements of their own, not affiliated ones.
fn keywords(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    standalone_keywords(root).filter(|node| keyword_key(node) != "CALL")
}

/// The keyword lines that are elements, `#+CALL:` among them (a
/// `babel-call` to `org-element`).
fn standalone_keywords(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    root.descendants().filter(|node| node.kind() == SyntaxKind::KEYWORD && !ast::is_affiliated_keyword(node))
}

/// The `#+CALL:` lines.
fn babel_calls(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    standalone_keywords(root).filter(|node| keyword_key(node) == "CALL")
}

/// A node property's key, as written, and its value.
fn node_property(node: &SyntaxNode) -> (String, Option<String>) {
    let key = token(node, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
    let key = key.trim().trim_start_matches(':').trim_end_matches(':').trim_end_matches('+').to_string();
    let value = token(node, SyntaxKind::PROPERTY_VALUE).map(|value| value.trim().to_string());
    (key, value.filter(|value| !value.is_empty()))
}

/// A block's type as written (`src`, `QUOTE`) and its parameters.
fn block_type(node: &SyntaxNode) -> (String, String) {
    let begin = token(node, SyntaxKind::BLOCK_BEGIN).unwrap_or_default();
    let name = begin.trim_start().get(8..).unwrap_or("").to_string();
    let parameters = token(node, SyntaxKind::BLOCK_PARAMETERS).unwrap_or_default().trim().to_string();
    (name, parameters)
}

/// The `#+NAME:` of an element (or a synonym), the last one.
fn element_name(node: &SyntaxNode) -> Option<String> {
    let mut name = None;
    for keyword in node.children().take_while(ast::is_affiliated_keyword) {
        if matches!(
            keyword_key(&keyword).as_str(),
            "NAME" | "DATA" | "LABEL" | "RESNAME" | "SOURCE" | "SRCNAME" | "TBLNAME"
        ) {
            name = Some(keyword_value(&keyword));
        }
    }
    name
}

/// The first value of an element's `#+RESULTS:` (its `[key]` aside).
fn element_results(node: &SyntaxNode) -> Option<String> {
    node.children()
        .take_while(ast::is_affiliated_keyword)
        .filter(|keyword| matches!(keyword_key(keyword).as_str(), "RESULTS" | "RESULT"))
        .last()
        .map(|keyword| keyword_value(&keyword))
}

fn is_element(node: &SyntaxNode) -> bool {
    use SyntaxKind::*;
    match node.kind() {
        KEYWORD => !ast::is_affiliated_keyword(node),
        HEADLINE | SECTION | PLANNING | PROPERTY_DRAWER | NODE_PROPERTY | DRAWER | BLOCK | DYNAMIC_BLOCK | COMMENT
        | FIXED_WIDTH | HORIZONTAL_RULE | TABLE | TABLE_ROW | LIST | LIST_ITEM | FOOTNOTE_DEFINITION | CLOCK
        | LATEX_ENVIRONMENT | DIARY_SEXP | PARAGRAPH => true,
        _ => false,
    }
}

/// `org-split-string` on blanks, as `split-string` with its default.
fn words(text: &str) -> Vec<String> {
    text.split([' ', '\t', '\n', '\r', '\x0b', '\x0c']).filter(|word| !word.is_empty()).map(str::to_string).collect()
}

/// `%S` of a string: in quotes, with `"` and `\` escaped.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `org-lint--collect-duplicates`: every place of a key seen more than
/// once.
fn duplicates<K: Eq + std::hash::Hash + Clone>(items: Vec<(K, usize)>, message: impl Fn(&K) -> String) -> Found {
    let mut first: HashMap<K, usize> = HashMap::new();
    let mut originals: Vec<K> = Vec::new();
    let mut out = Vec::new();
    for (key, at) in items {
        match first.get(&key) {
            Some(_) => {
                if !originals.contains(&key) {
                    originals.push(key.clone());
                }
                out.push((at, message(&key)));
            }
            None => {
                first.insert(key, at);
            }
        }
    }
    for key in originals {
        out.push((first[&key], message(&key)));
    }
    out
}

impl Doc<'_> {
    fn at(&self, at: usize) -> Option<&'static str> {
        self.elements.at(at).map(|element| element.kind)
    }

    // ----- checkers

    fn misplaced_heading(&self) -> Found {
        static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^*\n\r ,]\*\*+ ").expect("a valid regex"));
        HEADING
            .find_iter(self.text)
            .filter(|found| matches!(self.at(found.end()), Some("paragraph" | "headline")))
            .map(|found| (found.start(), "Possibly misplaced heading line".to_string()))
            .collect()
    }

    fn duplicate_custom_id(&self) -> Found {
        let items = self
            .root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY)
            .filter_map(|node| {
                let (key, value) = node_property(&node);
                (key.eq_ignore_ascii_case("CUSTOM_ID")).then(|| value.map(|value| (value, range(&node).0)))?
            })
            .collect();
        duplicates(items, |key| format!("Duplicate CUSTOM_ID property \"{key}\""))
    }

    fn duplicate_name(&self) -> Found {
        let items = self
            .root
            .descendants()
            .filter(is_element)
            .filter_map(|node| {
                let name = element_name(&node)?;
                // The line of the keyword that names it.
                let begin = range(&node).0;
                let mut at = begin;
                let pattern =
                    RegexBuilder::new(&format!(r"(?m)^[ \t]*#\+[A-Za-z]+:[ \t]*{}[ \t]*$", regex::escape(&name)))
                        .case_insensitive(true)
                        .build()
                        .ok()?;
                if let Some(found) = pattern.find_at(self.text, begin) {
                    at = found.start();
                }
                Some((name, at))
            })
            .collect();
        duplicates(items, |key| format!("Duplicate NAME \"{key}\""))
    }

    fn duplicate_target(&self) -> Found {
        let items = self
            .root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::TARGET)
            .map(|node| {
                let text = node.text().to_string();
                let value = text.trim_start_matches("<<").trim_end_matches(">>");
                (words(value), range(&node).0)
            })
            .collect();
        duplicates(items, |key| format!("Duplicate target <<{}>>", key.join(" ")))
    }

    fn duplicate_footnote_definition(&self) -> Found {
        let items = self
            .root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FOOTNOTE_DEFINITION)
            .filter_map(|node| {
                let label = footnote_label(&token(&node, SyntaxKind::FOOTNOTE_LABEL)?)?;
                Some((label, post_affiliated(self.text, &node)))
            })
            .collect();
        duplicates(items, |key| format!("Duplicate footnote definition \"{key}\""))
    }

    fn orphaned_affiliated_keywords(&self) -> Found {
        const AFFILIATED: [&str; 11] = [
            "CAPTION", "DATA", "HEADER", "HEADERS", "LABEL", "NAME", "PLOT", "RESNAME", "SOURCE", "SRCNAME", "TBLNAME",
        ];
        keywords(&self.root)
            .filter_map(|node| {
                let key = keyword_key(&node);
                let attr = key.strip_prefix("ATTR_").is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                });
                (attr || AFFILIATED.contains(&key.as_str()))
                    .then(|| (post_affiliated(self.text, &node), format!("Orphaned affiliated keyword: \"{key}\"")))
            })
            .collect()
    }

    fn regular_keyword_before_affiliated(&self) -> Found {
        keywords(&self.root)
            .filter_map(|node| {
                let (begin, end) = range(&node);
                // No blank line after it.
                if end < self.text.len() && self.text[end..].starts_with(['\n']) {
                    return None;
                }
                let next = self.elements.at(end)?;
                (next.begin != begin && next.begin < next.post_affiliated).then(|| {
                    (
                        begin,
                        format!(
                            "Independent keyword {} may be confused with affiliated keywords below",
                            keyword_key(&node)
                        ),
                    )
                })
            })
            .collect()
    }

    fn obsolete_affiliated_keywords(&self) -> Found {
        static OBSOLETE: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r"(?m)^[ \t]*#\+(DATA|LABEL|RESNAME|SOURCE|SRCNAME|TBLNAME|RESULT|HEADERS):")
                .case_insensitive(true)
                .build()
                .expect("a valid regex")
        });
        let mut out = Vec::new();
        for captures in OBSOLETE.captures_iter(self.text) {
            let (Some(whole), Some(key)) = (captures.get(0), captures.get(1)) else { continue };
            let key = key.as_str().to_ascii_uppercase();
            let affiliated = self.elements.at(whole.end()).is_some_and(|element| whole.end() < element.post_affiliated);
            if affiliated {
                let instead = match key.as_str() {
                    "HEADERS" => "HEADER",
                    "RESULT" => "RESULTS",
                    _ => "NAME",
                };
                out.push((
                    line_start(self.text, whole.start()),
                    format!("Obsolete affiliated keyword: \"{key}\".  Use \"{instead}\" instead"),
                ));
            }
        }
        out.reverse();
        out
    }

    fn deprecated_export_blocks(&self) -> Found {
        const DEPRECATED: [&str; 10] =
            ["ASCII", "BEAMER", "HTML", "LATEX", "MAN", "MARKDOWN", "MD", "ODT", "ORG", "TEXINFO"];
        self.blocks()
            .filter_map(|(node, name, _)| {
                DEPRECATED.iter().any(|deprecated| deprecated.eq_ignore_ascii_case(&name)).then(|| {
                    (
                        post_affiliated(self.text, &node),
                        format!("Deprecated syntax for export block.  Use \"BEGIN_EXPORT {name}\" instead"),
                    )
                })
            })
            .collect()
    }

    fn deprecated_header_syntax(&self) -> Found {
        let deprecated: Vec<&str> = crate::lint_tables::COMMON_HEADER_ARGS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| *name != "dir")
            .collect();
        let mut out = Vec::new();
        for node in self.root.descendants() {
            match node.kind() {
                SyntaxKind::KEYWORD if !ast::is_affiliated_keyword(&node) && keyword_key(&node) == "PROPERTY" => {
                    let value = keyword_value(&node);
                    let found = deprecated.iter().find(|name| {
                        value.len() > name.len()
                            && value[..name.len()].eq_ignore_ascii_case(name)
                            && value[name.len()..].starts_with([' ', '\t'])
                    });
                    if let Some(name) = found {
                        out.push((
                            range(&node).0,
                            format!("Deprecated syntax for \"{}\".  Use header-args instead", &value[..name.len()]),
                        ));
                    }
                }
                SyntaxKind::NODE_PROPERTY => {
                    let key = token(&node, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
                    let key = key.trim().trim_start_matches(':').trim_end_matches(':');
                    if deprecated.iter().any(|name| name.eq_ignore_ascii_case(key)) {
                        out.push((
                            range(&node).0,
                            format!("Deprecated syntax for \"{key}\".  Use :header-args: instead"),
                        ));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The blocks: the node, its type as written and its parameters.
    fn blocks(&self) -> impl Iterator<Item = (SyntaxNode, String, String)> + '_ {
        self.root.descendants().filter(|node| node.kind() == SyntaxKind::BLOCK).map(|node| {
            let (name, parameters) = block_type(&node);
            (node, name, parameters)
        })
    }

    fn missing_language_in_src_block(&self) -> Found {
        self.blocks()
            .filter(|(_, name, parameters)| name.eq_ignore_ascii_case("src") && parameters.is_empty())
            .map(|(node, _, _)| (post_affiliated(self.text, &node), "Missing language in source block".to_string()))
            .collect()
    }

    fn missing_backend_in_export_block(&self) -> Found {
        self.blocks()
            .filter(|(_, name, parameters)| name.eq_ignore_ascii_case("export") && parameters.is_empty())
            .map(|(node, _, _)| (post_affiliated(self.text, &node), "Missing backend in export block".to_string()))
            .collect()
    }

    fn deprecated_category_setup(&self) -> Found {
        keywords(&self.root)
            .filter(|node| keyword_key(node) == "CATEGORY")
            .skip(1)
            .map(|node| {
                (
                    post_affiliated(self.text, &node),
                    "Spurious CATEGORY keyword.  Set :CATEGORY: property instead".to_string(),
                )
            })
            .collect()
    }

    fn links(&self) -> Vec<LinkInfo> {
        self.root
            .descendants()
            .filter(|node| matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK))
            .map(|node| link_info(&node))
            .collect()
    }

    fn invalid_coderef_link(&self) -> Found {
        self.links()
            .into_iter()
            .filter(|link| link.kind == "coderef")
            .filter(|link| crate::open::search(self.text, self.parse, &format!("({})", link.path)).is_err())
            .map(|link| (link.start, format!("Unknown coderef \"{}\"", link.path)))
            .collect()
    }

    fn invalid_custom_id_link(&self) -> Found {
        let ids = self.headline_ids();
        self.links()
            .into_iter()
            .filter(|link| link.kind == "custom-id" && !ids.contains(&link.path))
            .map(|link| (link.start, format!("Unknown custom ID \"{}\"", link.path)))
            .collect()
    }

    /// The `ID` and `CUSTOM_ID` of the headlines.
    fn headline_ids(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for headline in self.root.descendants().filter(|node| node.kind() == SyntaxKind::HEADLINE) {
            let Some(drawer) = headline
                .children()
                .find(|child| child.kind() == SyntaxKind::SECTION)
                .and_then(|section| section.children().find(|child| child.kind() == SyntaxKind::PROPERTY_DRAWER))
                .or_else(|| headline.children().find(|child| child.kind() == SyntaxKind::PROPERTY_DRAWER))
            else {
                continue;
            };
            for property in drawer.children().filter(|child| child.kind() == SyntaxKind::NODE_PROPERTY) {
                let (key, value) = node_property(&property);
                if let Some(value) = value {
                    if key.eq_ignore_ascii_case("ID") || key.eq_ignore_ascii_case("CUSTOM_ID") {
                        out.insert(value);
                    }
                }
            }
        }
        out
    }

    fn invalid_fuzzy_link(&self) -> Found {
        // `org-export-search-cells` of the targets, the named elements and
        // the headlines.
        let mut cells: HashSet<(&'static str, Vec<String>)> = HashSet::new();
        let upper = |words: Vec<String>| words.into_iter().map(|word| word.to_uppercase()).collect::<Vec<_>>();
        static COOKIE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\[[0-9]*(?:%|/[0-9]*)\]").expect("a valid regex"));
        for node in self.root.descendants() {
            match node.kind() {
                SyntaxKind::HEADLINE => {
                    let raw = headline_raw_value(&node);
                    let title = upper(words(&COOKIE.replace_all(&raw, " ")));
                    cells.insert(("headline", title.clone()));
                    cells.insert(("other", title));
                }
                SyntaxKind::TARGET => {
                    let text = node.text().to_string();
                    let value = text.trim_start_matches("<<").trim_end_matches(">>").to_string();
                    cells.insert(("target", upper(words(&value))));
                }
                _ if is_element(&node) => {
                    if let Some(name) = element_name(&node).or_else(|| element_results(&node)) {
                        cells.insert(("other", words(&name)));
                    }
                }
                _ => {}
            }
        }
        self.links()
            .into_iter()
            .filter(|link| link.kind == "fuzzy")
            .filter(|link| {
                let path = &link.path;
                let wanted: Vec<(&'static str, Vec<String>)> = match path.strip_prefix('*') {
                    Some(rest) => vec![("headline", upper(words(rest)))],
                    None => {
                        let search = words(path);
                        vec![
                            ("target", search.clone()),
                            ("other", search.clone()),
                            ("target", upper(search.clone())),
                            ("other", upper(search)),
                        ]
                    }
                };
                !wanted.iter().any(|cell| cells.contains(cell))
            })
            .map(|link| {
                let shown = link.path.strip_prefix('*').unwrap_or(&link.path);
                (link.start, format!("Unknown fuzzy location \"{shown}\""))
            })
            .collect()
    }

    fn confusing_brackets(&self) -> Found {
        self.links()
            .into_iter()
            .filter(|link| {
                // After the link and the blanks that follow it.
                let rest = &self.text[link.end..];
                rest.trim_start_matches([' ', '\t']).starts_with(']')
            })
            .map(|link| (link.start, "Trailing ']' after link end".to_string()))
            .collect()
    }

    fn brackets_inside_description(&self) -> Found {
        self.links()
            .into_iter()
            .filter_map(|link| {
                let (start, end) = link.description?;
                let description = &self.text[start..end];
                let count: i64 = description
                    .chars()
                    .map(|c| match c {
                        '[' => 1,
                        ']' => -1,
                        _ => 0,
                    })
                    .sum();
                (count > 0)
                    .then(|| (link.start, format!("No closing ']' matches '[' in link description: {description}")))
            })
            .collect()
    }

    fn special_property_in_properties_drawer(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY)
            .filter_map(|node| {
                let key = token(&node, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
                let key = key.trim().trim_start_matches(':').trim_end_matches(':').to_string();
                crate::properties::SPECIAL
                    .iter()
                    .any(|special| special.eq_ignore_ascii_case(&key))
                    .then(|| (range(&node).0, format!("Special property \"{key}\" found in a properties drawer")))
            })
            .collect()
    }

    fn obsolete_properties_drawer(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::DRAWER)
            .filter(|node| token(node, SyntaxKind::DRAWER_BEGIN).is_some_and(|begin| begin.trim() == ":PROPERTIES:"))
            .map(|node| (post_affiliated(self.text, &node), "Incorrect contents for PROPERTIES drawer".to_string()))
            .collect()
    }

    fn invalid_effort_property(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY)
            .filter_map(|node| {
                let (key, value) = node_property(&node);
                let value = value?;
                (key == "EFFORT" && !crate::properties::is_duration(&value))
                    .then(|| (range(&node).0, format!("Invalid effort duration format: {}", quoted(&value))))
            })
            .collect()
    }

    fn invalid_id_property(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY)
            .filter_map(|node| {
                let (key, value) = node_property(&node);
                let value = value?;
                (key == "ID" && value.contains("::"))
                    .then(|| (range(&node).0, format!("IDs should not include \"::\": {}", quoted(&value))))
            })
            .collect()
    }

    fn link_to_local_file(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK))
            .filter_map(|node| {
                let link = link_info(&node);
                if link.kind != "file" {
                    return None;
                }
                let file = substitute_env(&link.path);
                if (self.context.file_exists)(&file) {
                    return None;
                }
                let in_description = node.ancestors().skip(1).any(|ancestor| ancestor.kind() == SyntaxKind::LINK);
                let message = if in_description {
                    format!("Link to non-existent image file {} in description", quoted(&file))
                } else {
                    format!("Link to non-existent local file {}", quoted(&file))
                };
                Some((link.start, message))
            })
            .collect()
    }

    fn non_existent_setupfile_parameter(&self) -> Found {
        keywords(&self.root)
            .filter(|node| keyword_key(node) == "SETUPFILE")
            .filter_map(|node| {
                let value = keyword_value(&node);
                let file = unbracket_quotes(&value);
                (!is_url(&file) && !(self.context.file_exists)(&file))
                    .then(|| (range(&node).0, format!("Non-existent setup file {}", quoted(&file))))
            })
            .collect()
    }

    fn obsolete_include_markup(&self) -> Found {
        static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r#"\A(?:".+"|\S+)[ \t]+(ASCII|BEAMER|HTML|LATEX|MAN|MARKDOWN|MD|ODT|ORG|TEXINFO)"#)
                .case_insensitive(true)
                .build()
                .expect("a valid regex")
        });
        keywords(&self.root)
            .filter(|node| keyword_key(node) == "INCLUDE")
            .filter_map(|node| {
                let value = keyword_value(&node);
                let markup = MARKUP.captures(&value)?.get(1)?.as_str().to_string();
                Some((
                    post_affiliated(self.text, &node),
                    format!("Obsolete markup \"{markup}\" in INCLUDE keyword.  Use \"export {markup}\" instead"),
                ))
            })
            .collect()
    }

    fn footnotes(&self) -> (Vec<Reference>, Vec<Definition>) {
        let references = self
            .root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FOOTNOTE_REFERENCE)
            .map(|node| {
                let text = node.text().to_string();
                let inner = text.trim_start_matches('[').trim_end_matches(']');
                let rest = inner.strip_prefix("fn:").unwrap_or(inner);
                let (label, kind) = match rest.find(':') {
                    Some(0) => (None, "inline"),
                    Some(colon) => (Some(rest[..colon].to_string()), "inline"),
                    None => (Some(rest.to_string()), "standard"),
                };
                (node, label, kind)
            })
            .collect();
        let definitions = self
            .root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FOOTNOTE_DEFINITION)
            .map(|node| {
                let label = token(&node, SyntaxKind::FOOTNOTE_LABEL).and_then(|label| footnote_label(&label));
                (node, label)
            })
            .collect();
        (references, definitions)
    }

    fn undefined_footnote_reference(&self) -> Found {
        let (references, definitions) = self.footnotes();
        let mut defined: Vec<String> = definitions.iter().filter_map(|(_, label)| label.clone()).collect();
        defined.extend(
            references.iter().filter(|(_, _, kind)| *kind == "inline").filter_map(|(_, label, _)| label.clone()),
        );
        references
            .iter()
            .filter(|(_, label, kind)| {
                *kind == "standard" && label.as_ref().is_some_and(|label| !defined.contains(label))
            })
            .map(|(node, label, _)| {
                (range(node).0, format!("Missing definition for footnote [{}]", label.clone().unwrap_or_default()))
            })
            .collect()
    }

    fn unreferenced_footnote_definition(&self) -> Found {
        let (references, definitions) = self.footnotes();
        let referenced: Vec<String> = references.iter().filter_map(|(_, label, _)| label.clone()).collect();
        definitions
            .iter()
            .filter_map(|(node, label)| {
                let label = label.as_ref()?;
                (!referenced.contains(label)).then(|| {
                    (post_affiliated(self.text, node), format!("No reference for footnote definition [{label}]"))
                })
            })
            .collect()
    }

    fn invalid_keyword_syntax(&self) -> Found {
        static KEYWORD: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)^[ \t]*#\+([^\s:]*)(?: |$)").expect("a valid regex"));
        static EXCEPTION: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r"\A[ \t]*#\+(?:CAPTION|RESULTS)(\[.*\])?:(?: |$)")
                .case_insensitive(true)
                .build()
                .expect("a valid regex")
        });
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(captures) = KEYWORD.captures_at(self.text, from) {
            let (Some(whole), Some(name)) = (captures.get(0), captures.get(1)) else { break };
            // `re-search-forward` goes on from the end of the match, or a
            // character further when it was empty.
            from = whole.end().max(whole.start() + 1);
            let name = name.as_str();
            let lower = name.to_ascii_lowercase();
            let line = &self.text[line_start(self.text, whole.start())..line_end(self.text, whole.start())];
            if lower.starts_with("begin") || lower.starts_with("end") || EXCEPTION.is_match(line) {
                continue;
            }
            out.push((whole.start(), format!("Possible missing colon in keyword \"{name}\"")));
            if from >= self.text.len() {
                break;
            }
        }
        out.reverse();
        out
    }

    fn invalid_block(&self) -> Found {
        static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r"(?m)^[ \t]*#\+(BEGIN|END)(?::|_\S*)?[ \t]*")
                .case_insensitive(true)
                .build()
                .expect("a valid regex")
        });
        let mut out = Vec::new();
        for captures in BLOCK.captures_iter(self.text) {
            let (Some(whole), Some(kind)) = (captures.get(0), captures.get(1)) else { continue };
            let start = line_start(self.text, whole.start());
            let end = line_end(self.text, start);
            let name = self.text[start..end].trim_matches([' ', '\t', '\n', '\r']).to_string();
            if kind.as_str().eq_ignore_ascii_case("END") && whole.end() != end {
                out.push((start, format!("Invalid block closing line \"{name}\"")));
            } else if !matches!(
                self.at(whole.end()),
                Some(
                    "center-block"
                        | "comment-block"
                        | "dynamic-block"
                        | "example-block"
                        | "export-block"
                        | "quote-block"
                        | "special-block"
                        | "src-block"
                        | "verse-block"
                )
            ) {
                out.push((start, format!("Possible incomplete block \"{name}\"")));
            }
        }
        out.reverse();
        out
    }

    fn misplaced_planning_info(&self) -> Found {
        static PLANNING: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r"(?m)^[ \t]*(?:CLOSED|DEADLINE|SCHEDULED):")
                .case_insensitive(true)
                .build()
                .expect("a valid regex")
        });
        let mut out: Found = PLANNING
            .find_iter(self.text)
            .filter(|found| {
                !matches!(
                    self.at(found.end()),
                    Some("comment-block" | "example-block" | "export-block" | "planning" | "src-block" | "verse-block")
                )
            })
            .map(|found| (line_start(self.text, found.start()), "Misplaced planning info line".to_string()))
            .collect();
        out.reverse();
        out
    }

    fn incomplete_drawer(&self) -> Found {
        static DRAWER: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)^[ \t]*:((?:\w|[-_])+):[ \t]*$").expect("a valid regex"));
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = DRAWER.find_at(self.text, from) {
            from = found.end().max(found.start() + 1);
            let name = found.as_str().trim_matches([' ', '\t']).to_string();
            let Some(element) = self.elements.at(found.end()) else { continue };
            match element.kind {
                "drawer" => {
                    if let Some((_, end)) = element.contents {
                        let mut inner = found.end();
                        while let Some(nested) = DRAWER.find_at(self.text, inner).filter(|nested| nested.end() <= end) {
                            inner = nested.end().max(nested.start() + 1);
                            let nested_name = nested.as_str().trim_matches([' ', '\t']).to_string();
                            out.push((
                                line_start(self.text, nested.start()),
                                format!("Possible misleading drawer entry {}", quoted(&nested_name)),
                            ));
                        }
                    }
                    from = from.max(element.end);
                }
                "property-drawer" => from = from.max(element.end),
                "comment-block" | "example-block" | "export-block" | "src-block" | "verse-block" => {}
                _ => out.push((
                    line_start(self.text, found.start()),
                    format!("Possible incomplete drawer {}", quoted(&name)),
                )),
            }
            if from >= self.text.len() {
                break;
            }
        }
        out.reverse();
        out
    }

    fn indented_diary_sexp(&self) -> Found {
        static SEXP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]+%%\(").expect("a valid regex"));
        let mut out: Found = SEXP
            .find_iter(self.text)
            .filter(|found| {
                !matches!(
                    self.at(found.end()),
                    Some(
                        "comment-block" | "diary-sexp" | "example-block" | "export-block" | "src-block" | "verse-block"
                    )
                )
            })
            .map(|found| (line_start(self.text, found.start()), "Possible indented diary-sexp".to_string()))
            .collect();
        out.reverse();
        out
    }

    fn quote_section(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::HEADLINE)
            .filter(|node| {
                let raw = headline_raw_value(node);
                raw.starts_with("QUOTE ") || raw.starts_with("COMMENT QUOTE ")
            })
            .map(|node| (range(&node).0, "Deprecated QUOTE section".to_string()))
            .collect()
    }

    fn file_application(&self) -> Found {
        self.links()
            .into_iter()
            .filter_map(|link| {
                let application = link.raw.split(':').next()?.strip_prefix("file+")?.to_string();
                Some((link.start, format!("Deprecated \"file+{application}\" link type")))
            })
            .collect()
    }

    fn percent_encoding_link_escape(&self) -> Found {
        static PERCENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"%(..)?").expect("a valid regex"));
        self.links()
            .into_iter()
            .filter(|link| link.bracket)
            .filter(|link| {
                let path = &link.path;
                let all_known = PERCENT.captures_iter(path).all(|captures| {
                    captures.get(1).is_some_and(|code| matches!(code.as_str(), "25" | "5B" | "5D" | "20"))
                });
                all_known && path.contains('%')
            })
            .map(|link| (link.start, "Link escaped with obsolete percent-encoding syntax".to_string()))
            .collect()
    }

    fn spurious_colons(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::HEADLINE)
            .filter(|node| {
                let line = headline_line(self.text, node);
                crate::text::tag_group(line).is_some_and(|(_, start)| {
                    let group = line[start..].trim_end_matches([' ', '\t']);
                    group.trim_matches(':').split(':').any(str::is_empty)
                })
            })
            .map(|node| (range(&node).0, "Tags contain a spurious colon".to_string()))
            .collect()
    }

    fn non_existent_bibliography(&self) -> Found {
        keywords(&self.root)
            .filter(|node| keyword_key(node) == "BIBLIOGRAPHY")
            .filter_map(|node| {
                let file = strip_quotes(&keyword_value(&node));
                (!(self.context.file_exists)(&file))
                    .then(|| (range(&node).0, format!("Non-existent bibliography {}", quoted(&file))))
            })
            .collect()
    }

    fn missing_print_bibliography(&self) -> Found {
        let cites = self.root.descendants().any(|node| node.kind() == SyntaxKind::CITATION);
        let printed = keywords(&self.root).any(|node| keyword_key(&node) == "PRINT_BIBLIOGRAPHY");
        if cites && !printed {
            vec![(self.text.len(), "Possibly missing \"PRINT_BIBLIOGRAPHY\" keyword".to_string())]
        } else {
            Vec::new()
        }
    }

    fn mismatched_planning_repeaters(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::PLANNING)
            .filter_map(|node| {
                let text = node.text().to_string();
                let repeater = |keyword: &str| -> Option<(String, i64, char)> {
                    let at = text.find(keyword)?;
                    let stamp = text[at + keyword.len()..].trim_start();
                    let close = stamp.find(['>', ']'])?;
                    repeater_of(&stamp[..close])
                };
                let (scheduled, deadline) = (repeater("SCHEDULED:")?, repeater("DEADLINE:")?);
                let counted = |mark: &str| mark == "+" || mark == "++";
                (counted(&scheduled.0)
                    && counted(&deadline.0)
                    && scheduled.1 > 0
                    && deadline.1 > 0
                    && scheduled != deadline)
                    .then(|| (range(&node).0, "Different repeaters in SCHEDULED and DEADLINE timestamps.".to_string()))
            })
            .collect()
    }

    fn planning_inactive(&self) -> Found {
        self.root
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::PLANNING)
            .filter_map(|node| {
                let text = node.text().to_string();
                let inactive = |keyword: &str| {
                    text.find(keyword).is_some_and(|at| text[at + keyword.len()..].trim_start().starts_with('['))
                };
                if inactive("SCHEDULED:") {
                    Some((range(&node).0, "Inactive timestamp in SCHEDULED will not appear in agenda.".to_string()))
                } else if inactive("DEADLINE:") {
                    Some((range(&node).0, "Inactive timestamp in DEADLINE will not appear in agenda.".to_string()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// The Babel calls, inline calls, inline source blocks and source
    /// blocks, in order, with their headers.
    fn babel_datums(&self) -> Vec<Datum> {
        let mut out = Vec::new();
        for node in self.root.descendants() {
            match node.kind() {
                SyntaxKind::KEYWORD if !ast::is_affiliated_keyword(&node) && keyword_key(&node) == "CALL" => {
                    let call = parse_call(self.text, &node);
                    out.push(Datum {
                        at: post_affiliated(self.text, &node),
                        language: None,
                        parts: vec![call.inside_header, call.end_header],
                        call: true,
                    });
                }
                SyntaxKind::INLINE_BABEL_CALL => {
                    let call = parse_inline_call(&node.text().to_string());
                    out.push(Datum {
                        at: range(&node).0,
                        language: None,
                        parts: vec![call.inside_header, call.end_header],
                        call: true,
                    });
                }
                SyntaxKind::INLINE_SRC_BLOCK => {
                    let text = node.text().to_string();
                    let rest = text.strip_prefix("src_").unwrap_or(&text);
                    let language_end = rest.find(['[', '{']).unwrap_or(rest.len());
                    let parameters = paired(rest, language_end, '[').map(|(inside, _)| inside.trim().to_string());
                    out.push(Datum {
                        at: range(&node).0,
                        language: Some(rest[..language_end].to_string()),
                        parts: vec![parameters.filter(|parameters| not_blank(parameters))],
                        call: false,
                    });
                }
                SyntaxKind::BLOCK => {
                    let (name, parameters) = block_type(&node);
                    if !name.eq_ignore_ascii_case("src") {
                        continue;
                    }
                    let (language, parameters) = src_parts(&parameters);
                    let mut parts = vec![parameters];
                    parts.extend(element_headers(&node).into_iter().map(Some));
                    out.push(Datum { at: post_affiliated(self.text, &node), language, parts, call: false });
                }
                _ => {}
            }
        }
        out
    }

    fn invalid_babel_call_block(&self) -> Found {
        babel_calls(&self.root)
            .filter_map(|node| {
                let call = parse_call(self.text, &node);
                let at = post_affiliated(self.text, &node);
                if call.call.is_none() {
                    Some((at, "Invalid syntax in babel call block".to_string()))
                } else if call.end_header.as_ref().is_some_and(|end| end.starts_with('[') && end.ends_with(']')) {
                    Some((at, "Babel call's end header must not be wrapped within brackets".to_string()))
                } else {
                    None
                }
            })
            .collect()
    }

    fn wrong_header_argument(&self) -> Found {
        use crate::lint_tables::{COMMON_HEADER_ARGS, LANGUAGES};
        let mut out = Vec::new();
        let mut verify = |at: usize, language: Option<&str>, headers: Vec<(String, Option<String>, bool)>| {
            let languages: Vec<&str> = match language {
                Some(language) => vec![language],
                None => LANGUAGES.to_vec(),
            };
            let allowed = |name: &str| {
                COMMON_HEADER_ARGS.iter().any(|(known, _)| *known == name)
                    || languages
                        .iter()
                        .filter_map(|language| language_args(language))
                        .any(|args| args.iter().any(|(known, _)| *known == name))
            };
            for (key, _, _) in headers {
                match key.strip_prefix(':') {
                    None => out.push((at, format!("Missing colon in header argument \"{key}\""))),
                    Some(name) if allowed(name) => {}
                    Some(_) => out.push((at, format!("Unknown header argument \"{key}\""))),
                }
            }
        };
        for node in self.root.descendants() {
            match node.kind() {
                SyntaxKind::KEYWORD if !ast::is_affiliated_keyword(&node) && keyword_key(&node) == "PROPERTY" => {
                    static HEADER_ARGS: LazyLock<Regex> = LazyLock::new(|| {
                        Regex::new(r"\Aheader-args(?::(\S+?))?\+ *|\Aheader-args(?::(\S+))? *").expect("a valid regex")
                    });
                    let value = keyword_value(&node);
                    if let Some(captures) = HEADER_ARGS.captures(&value) {
                        let language =
                            captures.get(1).or_else(|| captures.get(2)).map(|found| found.as_str().to_string());
                        let rest = value[captures.get(0).map_or(0, |found| found.end())..].to_string();
                        verify(post_affiliated(self.text, &node), language.as_deref(), headers_of(Some(&rest)));
                    }
                }
                SyntaxKind::NODE_PROPERTY => {
                    static HEADER_ARGS: LazyLock<Regex> = LazyLock::new(|| {
                        RegexBuilder::new(r"\AHEADER-ARGS(?::(\S+?))?\+|\AHEADER-ARGS(?::(\S+))?")
                            .case_insensitive(true)
                            .build()
                            .expect("a valid regex")
                    });
                    let key = token(&node, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
                    let key = key.trim().trim_start_matches(':').trim_end_matches(':').to_string();
                    if let Some(captures) = HEADER_ARGS.captures(&key) {
                        let language =
                            captures.get(1).or_else(|| captures.get(2)).map(|found| found.as_str().to_string());
                        let value = token(&node, SyntaxKind::PROPERTY_VALUE).map(|value| value.trim().to_string());
                        verify(range(&node).0, language.as_deref(), headers_of(value.as_deref()));
                    }
                }
                _ => {}
            }
        }
        for datum in self.babel_datums() {
            let headers = datum.parts.iter().flat_map(|part| headers_of(part.as_deref())).collect();
            let language = if datum.call { None } else { datum.language.clone() };
            if datum.call {
                verify(datum.at, None, headers);
            } else {
                verify(datum.at, language.as_deref(), headers);
            }
        }
        out.reverse();
        out
    }

    fn empty_header_argument(&self) -> Found {
        let mut out = Vec::new();
        for datum in self.babel_datums() {
            for part in &datum.parts {
                for (key, value, _) in headers_of(part.as_deref()) {
                    if value.is_none() {
                        out.push((datum.at, format!("Empty value in header argument \"{key}\"")));
                    }
                }
            }
        }
        out.reverse();
        out
    }

    fn wrong_header_value(&self) -> Found {
        use crate::lint_tables::{Values, COMMON_HEADER_ARGS};
        let mut out = Vec::new();
        for datum in self.babel_datums() {
            let language = if datum.call { None } else { datum.language.clone() };
            let joined: Vec<String> = datum.parts.iter().map(|part| part.clone().unwrap_or_default()).collect();
            let joined = joined.join(" ");
            for (key, value, number) in headers_of(Some(joined.trim())) {
                let name = key.get(1..).unwrap_or("");
                let allowed = language
                    .as_deref()
                    .and_then(language_args)
                    .into_iter()
                    .flatten()
                    .chain(COMMON_HEADER_ARGS.iter())
                    .find(|(known, _)| *known == name)
                    .map(|(_, values)| *values);
                let Some(Values::Groups(groups)) = allowed else { continue };
                let values: Vec<Option<String>> = match (&value, number) {
                    (Some(text), false) => text.split_whitespace().map(|word| Some(word.to_string())).collect(),
                    (other, _) => vec![other.clone()],
                };
                let mut used: Vec<(usize, String)> = Vec::new();
                for v in values {
                    let shown = v.clone().unwrap_or_else(|| "nil".to_string());
                    let mut valid = false;
                    let mut forbidden = false;
                    for (index, group) in groups.iter().enumerate() {
                        let member = !number && v.as_deref().is_some_and(|v| group.contains(&v));
                        if !member {
                            if group.contains(&":any") {
                                valid = true;
                                used.push((index, shown.clone()));
                            }
                        } else if let Some((_, earlier)) = used.iter().find(|(used, _)| *used == index) {
                            out.push((
                                datum.at,
                                format!("Forbidden combination in header \"{key}\": {earlier}, {shown}"),
                            ));
                            forbidden = true;
                            break;
                        } else {
                            used.push((index, shown.clone()));
                            valid = true;
                        }
                    }
                    if !valid && !forbidden {
                        out.push((datum.at, format!("Unknown value \"{shown}\" for header \"{key}\"")));
                    }
                }
            }
        }
        out.reverse();
        out
    }

    fn unknown_options_item(&self) -> Found {
        static ITEM: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(.+?):(\(.*?\)|\S+)?[ \t]*").expect("a valid regex"));
        let mut out = Vec::new();
        for node in keywords(&self.root).filter(|node| keyword_key(node) == "OPTIONS") {
            let value = keyword_value(&node);
            let at = post_affiliated(self.text, &node);
            for captures in ITEM.captures_iter(&value) {
                let item = captures.get(1).map_or("", |found| found.as_str());
                if !crate::lint_tables::OPTIONS_ITEMS.contains(&item) {
                    out.push((at, format!("Unknown OPTIONS item \"{item}\"")));
                }
                if captures.get(2).is_none() {
                    out.push((at, format!("Missing value for option item {}", quoted(item))));
                }
            }
        }
        out.reverse();
        out
    }

    fn misspelled_export_option(&self) -> Found {
        use crate::lint_tables::{BACKEND_OPTIONS, COMMON_OPTIONS, DEFAULT_PROPERTIES};
        let mut out = Vec::new();
        for node in self.root.descendants().filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY) {
            let key = token(&node, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
            let key = key.trim().trim_start_matches(':').trim_end_matches(':').to_string();
            let common = COMMON_OPTIONS.contains(&key.as_str());
            let backends = BACKEND_OPTIONS.iter().find(|(option, _)| *option == key).map(|(_, backends)| *backends);
            if !(common || backends.is_some()) || DEFAULT_PROPERTIES.contains(&key.as_str()) {
                continue;
            }
            let place = match backends.filter(|_| !common) {
                Some([one]) => format!(" in {one} export backend"),
                Some(many) => format!(" in ({}) export backends", many.join(" ")),
                None => String::new(),
            };
            out.push((
                range(&node).0,
                format!(
                    "Potentially misspelled {}export option \"{key}\"{place}.  Consider \"EXPORT_{key}\".",
                    // `(when … "global ")` formats as "nil" otherwise.
                    if common { "global " } else { "nil" }
                ),
            ));
        }
        out.reverse();
        out
    }

    fn beamer_frame(&self) -> Found {
        static FRAME: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\\(?:begin|end)\{orgframe\}").expect("a valid regex"));
        let mut out: Found = FRAME
            .find_iter(self.text)
            .map(|found| {
                (
                    found.start(),
                    "Beamer frame name may cause error when exporting.  Consider customizing `org-beamer-frame-environment'."
                        .to_string(),
                )
            })
            .collect();
        out.reverse();
        out
    }
}

/// Where the brackets opened at `at` close
/// (`org-element--parse-paired-brackets`): their contents and the
/// place after them.
fn paired(text: &str, at: usize, open: char) -> Option<(String, usize)> {
    let close = if open == '[' { ']' } else { ')' };
    if !text[at..].starts_with(open) {
        return None;
    }
    let mut depth = 0;
    for (index, c) in text[at..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some((text[at + 1..at + index].to_string(), at + index + 1));
            }
        } else if c == '\n' {
            return None;
        }
    }
    None
}

/// A Babel call (`#+CALL:`, `call_name()`) as `org-element` reads it: its
/// name, its inside header, its arguments, its end header.
struct Call {
    call: Option<String>,
    inside_header: Option<String>,
    end_header: Option<String>,
}

/// `org-element-babel-call-parser` on the `#+CALL:` line at `node`. The
/// name runs up to a bracket or the next line; with no name on the line,
/// what follows is read from the next one, as Org reads it.
fn parse_call(text: &str, node: &SyntaxNode) -> Call {
    let start = post_affiliated(text, node);
    let line_end = line_end(text, start);
    let before_blank = (line_end + 1).min(text.len());
    let colon = text[start..line_end].find(':').map_or(line_end, |colon| start + colon + 1);
    let mut at = colon + text[colon..line_end].len() - text[colon..line_end].trim_start_matches([' ', '\t']).len();
    let name_start = at;
    while at < before_blank && !matches!(text.as_bytes()[at], b'[' | b']' | b'(' | b')') {
        at += 1;
    }
    let call = Some(text[name_start..at].to_string()).filter(|call| not_blank(call));
    let inside_header = paired(text, at, '[').map(|(inside, after)| {
        at = after;
        inside
    });
    if let Some((_, after)) = paired(text, at, '(') {
        at = after;
    }
    let end = text[at..].find('\n').map_or(text.len(), |found| at + found);
    let end_header = Some(text[at..end].trim().to_string()).filter(|end| not_blank(end));
    Call { call, inside_header, end_header }
}

/// An inline call (`call_name[header](arguments)[header]`).
fn parse_inline_call(text: &str) -> Call {
    let rest = text.strip_prefix("call_").unwrap_or(text);
    let name_end = rest.find(['[', '(']).unwrap_or(rest.len());
    let mut at = name_end;
    let inside_header = paired(rest, at, '[').map(|(inside, after)| {
        at = after;
        inside
    });
    if let Some((_, after)) = paired(rest, at, '(') {
        at = after;
    }
    let end_header = paired(rest, at, '[').map(|(inside, _)| inside).filter(|end| not_blank(end));
    Call { call: Some(rest[..name_end].to_string()), inside_header, end_header }
}

fn not_blank(text: &str) -> bool {
    text.chars().any(|c| !c.is_whitespace())
}

/// A source block's language, switches and parameters, as
/// `org-element-src-block-parser` reads its first line.
fn src_parts(parameters: &str) -> (Option<String>, Option<String>) {
    // Language: the first word.
    let trimmed = parameters.trim_start();
    let language_end = trimmed.find([' ', '\t']).unwrap_or(trimmed.len());
    let language = Some(trimmed[..language_end].to_string()).filter(|language| !language.is_empty());
    let mut rest = &trimmed[language_end..];
    // Switches, then the parameters.
    static SWITCHES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\A(?: +(?:-(?:l ".+"|[ikr])|[-+]n(?: *[0-9]+)?))+"#).expect("a valid regex"));
    if let Some(found) = SWITCHES.find(rest) {
        rest = &rest[found.end()..];
    }
    let parameters = Some(rest.trim().to_string()).filter(|parameters| not_blank(parameters));
    (language, parameters)
}

/// The `#+HEADER:` lines of an element, as `org-element` keeps them:
/// the last first.
fn element_headers(node: &SyntaxNode) -> Vec<String> {
    let mut headers: Vec<String> = node
        .children()
        .take_while(ast::is_affiliated_keyword)
        .filter(|keyword| matches!(keyword_key(keyword).as_str(), "HEADER" | "HEADERS"))
        .map(|keyword| keyword_value(&keyword))
        .collect();
    headers.reverse();
    headers
}

/// A header argument's value as `org-lint` sees it: text, a number, or
/// none.
fn header_value(value: &crate::tangle::Value) -> Option<String> {
    match value {
        crate::tangle::Value::Text(text) => Some(text.clone()),
        crate::tangle::Value::Number(number) => Some(number.to_string()),
        crate::tangle::Value::None => None,
    }
}

/// The header arguments of a string, as `org-babel-parse-header-arguments`
/// reads them without evaluating: each key with its value.
fn headers_of(text: Option<&str>) -> Vec<(String, Option<String>, bool)> {
    let Some(text) = text.filter(|text| not_blank(text)) else { return Vec::new() };
    crate::tangle::parse_header_arguments_no_eval(text)
        .into_iter()
        .map(|(key, value)| {
            let number = matches!(value, crate::tangle::Value::Number(_));
            (key, header_value(&value), number)
        })
        .collect()
}

/// `org-babel-header-args:LANG`
fn language_args(language: &str) -> Option<&'static [(&'static str, crate::lint_tables::Values)]> {
    crate::lint_tables::LANGUAGE_HEADER_ARGS.iter().find(|(name, _)| *name == language).map(|(_, args)| *args)
}

/// A datum of the header checkers: where it is reported, its language,
/// and its headers as separate strings.
struct Datum {
    at: usize,
    language: Option<String>,
    parts: Vec<Option<String>>,
    /// A `#+CALL:` or an inline call: any language's arguments go.
    call: bool,
}

/// The repeater of a timestamp's text: its mark (`+`, `++`, `.+`), value
/// and unit.
fn repeater_of(stamp: &str) -> Option<(String, i64, char)> {
    static REPEATER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\.\+|\+\+|\+)([0-9]+)([hdwmy])").expect("a valid regex"));
    let captures = REPEATER.captures(stamp)?;
    Some((captures[1].to_string(), captures[2].parse().ok()?, captures[3].chars().next()?))
}

/// The line of a headline, without its line break.
fn headline_line<'t>(text: &'t str, node: &SyntaxNode) -> &'t str {
    let start = range(node).0;
    &text[start..line_end(text, start)]
}

/// A headline's `:raw-value`: its title as written, without the
/// keyword, the priority and the tags.
fn headline_raw_value(node: &SyntaxNode) -> String {
    ast::Headline::cast(node.clone()).map(|headline| headline.title_text().trim().to_string()).unwrap_or_default()
}

/// The label of `[fn:label]` or `[fn:label:…]`.
fn footnote_label(text: &str) -> Option<String> {
    let inner = text.trim().trim_start_matches('[').trim_end_matches(']');
    let rest = inner.strip_prefix("fn:")?;
    let label = rest.split(':').next().unwrap_or(rest);
    (!label.is_empty()).then(|| label.to_string())
}

/// A link as `org-element-link-parser` reads it.
struct LinkInfo {
    start: usize,
    end: usize,
    /// `file`, `https`, `fuzzy`, `custom-id`, `coderef`…
    kind: String,
    /// The path, without the type and without a file's search option.
    path: String,
    raw: String,
    bracket: bool,
    description: Option<(usize, usize)>,
}

fn link_info(node: &SyntaxNode) -> LinkInfo {
    let (start, end) = range(node);
    let bracket = node.kind() == SyntaxKind::LINK;
    let raw_path = token(node, SyntaxKind::LINK_PATH).unwrap_or_default();
    let raw = if bracket { crate::link::unescape(&raw_path) } else { raw_path };
    // The description: between `][` and `]]`.
    let description = if bracket {
        let separator = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .find(|token| token.kind() == SyntaxKind::LINK_SEPARATOR)
            .map(|token| usize::from(token.text_range().end()));
        let close = node
            .children_with_tokens()
            .filter_map(|child| child.into_token())
            .filter(|token| token.kind() == SyntaxKind::LINK_CLOSE)
            .last()
            .map(|token| usize::from(token.text_range().start()));
        separator.zip(close).filter(|(from, to)| from <= to)
    } else {
        None
    };
    let typed = raw.split_once(':').filter(|(kind, _)| {
        org_syntax::link_types().iter().any(|known| known.eq_ignore_ascii_case(kind)) || kind.starts_with("file+")
    });
    let (kind, path) = match typed {
        Some((kind, rest)) => {
            let kind = kind.to_ascii_lowercase();
            let kind = if kind.starts_with("file+") { "file".to_string() } else { kind };
            // A file's search option is not part of its path.
            let rest = if kind == "file" { rest.split("::").next().unwrap_or(rest) } else { rest };
            // `file:///home` is `/home`.
            let rest = if kind == "file" && rest.starts_with("//") {
                format!("/{}", rest.trim_start_matches('/'))
            } else {
                rest.to_string()
            };
            (kind, rest)
        }
        None if bracket
            && (raw.starts_with('/') || raw.starts_with("./") || raw.starts_with("../") || raw.starts_with('~')) =>
        {
            ("file".to_string(), raw.split("::").next().unwrap_or(&raw).to_string())
        }
        None if bracket && raw.starts_with('#') => ("custom-id".to_string(), raw[1..].to_string()),
        None if bracket && raw.starts_with('(') && raw.ends_with(')') => {
            ("coderef".to_string(), raw[1..raw.len() - 1].to_string())
        }
        None => ("fuzzy".to_string(), raw.clone()),
    };
    LinkInfo { start, end, kind, path, raw, bracket, description }
}

/// `substitute-env-in-file-name`: `$VAR` and `${VAR}` replaced.
fn substitute_env(text: &str) -> String {
    static VAR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\$(?:\{([^}]+)\}|([A-Za-z_][A-Za-z0-9_]*))").expect("a valid regex"));
    VAR.replace_all(text, |captures: &regex::Captures| {
        let name = captures.get(1).or_else(|| captures.get(2)).map_or("", |found| found.as_str());
        std::env::var(name).unwrap_or_else(|_| captures[0].to_string())
    })
    .into_owned()
}

/// `org-unbracket-string "\"" "\""`
fn unbracket_quotes(text: &str) -> String {
    match text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) {
        Some(inner) => inner.to_string(),
        None => text.to_string(),
    }
}

/// `org-strip-quotes`
fn strip_quotes(text: &str) -> String {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        text[1..text.len() - 1].to_string()
    } else {
        text.to_string()
    }
}

/// `org-url-p`
fn is_url(text: &str) -> bool {
    static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A[a-zA-Z][-a-zA-Z0-9+.]*://").expect("a valid regex"));
    URL.is_match(text)
}

/// The checkers, in the order `org-lint--checkers` holds them, with
/// their trust.
type Checker<'d> = (&'static str, &'static str, fn(&Doc<'d>) -> Found);

fn checkers<'d>() -> Vec<Checker<'d>> {
    vec![
        ("planning-inactive", "high", Doc::planning_inactive),
        ("beamer-frame", "low", Doc::beamer_frame),
        ("missing-print-bibliography", "nil", Doc::missing_print_bibliography),
        ("non-existent-bibliography", "nil", Doc::non_existent_bibliography),
        ("spurious-colons", "nil", Doc::spurious_colons),
        ("percent-encoding-link-escape", "low", Doc::percent_encoding_link_escape),
        ("file-application", "nil", Doc::file_application),
        ("quote-section", "low", Doc::quote_section),
        ("indented-diary-sexp", "low", Doc::indented_diary_sexp),
        ("incomplete-drawer", "low", Doc::incomplete_drawer),
        ("misplaced-planning-info", "low", Doc::misplaced_planning_info),
        ("mismatched-planning-repeaters", "low", Doc::mismatched_planning_repeaters),
        ("invalid-block", "low", Doc::invalid_block),
        ("invalid-keyword-syntax", "low", Doc::invalid_keyword_syntax),
        ("unreferenced-footnote-definition", "nil", Doc::unreferenced_footnote_definition),
        ("undefined-footnote-reference", "nil", Doc::undefined_footnote_reference),
        ("invalid-id-property", "nil", Doc::invalid_id_property),
        ("invalid-effort-property", "nil", Doc::invalid_effort_property),
        ("obsolete-properties-drawer", "nil", Doc::obsolete_properties_drawer),
        ("special-property-in-properties-drawer", "nil", Doc::special_property_in_properties_drawer),
        ("misspelled-export-option", "low", Doc::misspelled_export_option),
        ("unknown-options-item", "low", Doc::unknown_options_item),
        ("obsolete-include-markup", "low", Doc::obsolete_include_markup),
        ("non-existent-setupfile-parameter", "low", Doc::non_existent_setupfile_parameter),
        ("link-to-local-file", "low", Doc::link_to_local_file),
        ("unclosed-brackets-in-link-description", "low", Doc::brackets_inside_description),
        ("trailing-bracket-after-link", "low", Doc::confusing_brackets),
        ("invalid-fuzzy-link", "nil", Doc::invalid_fuzzy_link),
        ("invalid-custom-id-link", "nil", Doc::invalid_custom_id_link),
        ("invalid-coderef-link", "nil", Doc::invalid_coderef_link),
        ("deprecated-category-setup", "nil", Doc::deprecated_category_setup),
        ("empty-header-argument", "low", Doc::empty_header_argument),
        ("wrong-header-value", "low", Doc::wrong_header_value),
        ("wrong-header-argument", "nil", Doc::wrong_header_argument),
        ("invalid-babel-call-block", "nil", Doc::invalid_babel_call_block),
        ("missing-backend-in-export-block", "nil", Doc::missing_backend_in_export_block),
        ("missing-language-in-src-block", "nil", Doc::missing_language_in_src_block),
        ("deprecated-header-syntax", "low", Doc::deprecated_header_syntax),
        ("deprecated-export-blocks", "low", Doc::deprecated_export_blocks),
        ("obsolete-affiliated-keywords", "nil", Doc::obsolete_affiliated_keywords),
        ("combining-keywords-with-affiliated", "low", Doc::regular_keyword_before_affiliated),
        ("orphaned-affiliated-keywords", "low", Doc::orphaned_affiliated_keywords),
        ("duplicate-footnote-definition", "nil", Doc::duplicate_footnote_definition),
        ("duplicate-target", "nil", Doc::duplicate_target),
        ("duplicate-name", "nil", Doc::duplicate_name),
        ("duplicate-custom-id", "nil", Doc::duplicate_custom_id),
        ("misplaced-heading", "low", Doc::misplaced_heading),
    ]
}

/// The names of the checkers.
pub fn checker_names() -> Vec<&'static str> {
    checkers().into_iter().map(|(name, _, _)| name).collect()
}

/// `org-lint` on `text`: the reports of every checker, by place.
pub fn lint(text: &str, parse: &Parse, context: &Context) -> Vec<Report> {
    let doc = Doc { text, parse, root: parse.syntax(), elements: Elements::new(text), context };
    let mut out = Vec::new();
    for (name, trust, check) in checkers() {
        for (offset, message) in check(&doc) {
            out.push(Report { offset, trust, message, checker: name });
        }
    }
    // `sort` is stable: at the same place, the checkers' order.
    out.sort_by_key(|report| report.offset);
    out
}
