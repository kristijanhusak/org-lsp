//! An Org document as HTML, as Org's HTML export would have it in the
//! main: headings, paragraphs, lists, tables, blocks, links and markup.

use org_syntax::SyntaxKind::*;
use org_syntax::{SyntaxElement, SyntaxNode};

use crate::{highlight, Links, Page, Target};

/// The export options of a file (`#+OPTIONS:`) the pages follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// `^:`: sub- and superscripts always (`t`), with braces only (`{}`),
    /// or never (`nil`).
    pub scripts: Scripts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scripts {
    Always,
    Braces,
    Never,
}

impl Options {
    pub fn read(root: &SyntaxNode) -> Options {
        let mut options = Options { scripts: Scripts::Always };
        for node in root.descendants().filter(|node| node.kind() == KEYWORD) {
            let (key, value) = keyword_parts(&node);
            if key != "options" {
                continue;
            }
            for item in value.split_whitespace() {
                match item {
                    "^:{}" => options.scripts = Scripts::Braces,
                    "^:nil" => options.scripts = Scripts::Never,
                    "^:t" => options.scripts = Scripts::Always,
                    _ => {}
                }
            }
        }
        options
    }
}

/// The key of a `#+KEY: VALUE` line, in lower case, and its value.
fn keyword_parts(node: &SyntaxNode) -> (String, String) {
    let mut key = String::new();
    let mut value = String::new();
    for token in node.children_with_tokens().filter_map(|child| child.into_token()) {
        match token.kind() {
            KEYWORD_KEY => key = token.text().trim_start_matches("#+").trim_end_matches(':').to_lowercase(),
            KEYWORD_VALUE => value.push_str(token.text()),
            _ => {}
        }
    }
    (key, value.trim().to_string())
}

/// A section of a page for the search: under which heading, and its text.
pub struct Section {
    pub id: Option<String>,
    pub heading: Option<String>,
    pub text: String,
}

/// The body of `page`, and its sections for the search.
pub fn render(page: &Page, links: &mut Links) -> (String, Vec<Section>) {
    let mut renderer = Renderer { page, links, out: String::new(), sections: Vec::new() };
    for child in page.root.children() {
        match child.kind() {
            SECTION => {
                renderer.section_text(&child, None, None);
                renderer.elements(&child);
            }
            HEADLINE => renderer.headline(&child),
            _ => {}
        }
    }
    (renderer.out, renderer.sections)
}

struct Renderer<'a, 'b> {
    page: &'a Page,
    links: &'a mut Links<'b>,
    out: String,
    sections: Vec<Section>,
}

impl Renderer<'_, '_> {
    fn headline(&mut self, node: &SyntaxNode) {
        if skipped(node) {
            return;
        }
        let offset = usize::from(node.text_range().start());
        let heading = self.page.headings.iter().find(|heading| heading.offset == offset).cloned();
        let headline = org_syntax::ast::Headline::cast(node.clone()).unwrap();
        let level = (headline.level() + 1).min(6);
        let id = heading.as_ref().map(|heading| heading.id.clone()).unwrap_or_default();
        let mut title = String::new();
        if let Some(keyword) = headline.todo_keyword() {
            title.push_str(&format!("{} ", escape(keyword.text())));
        }
        if let Some(node) = headline.title() {
            title.push_str(&self.objects(&node));
        }
        for tag in headline.tags() {
            title.push_str(&format!(" <span class=\"tag\">{}</span>", escape(&tag)));
        }
        self.out.push_str(&format!(
            "<h{level} id=\"{id}\">{title}<a class=\"anchor\" href=\"#{id}\" aria-label=\"Link to this heading\">#</a></h{level}>\n"
        ));
        let text = heading.as_ref().map(|heading| heading.text.clone());
        for child in node.children() {
            match child.kind() {
                SECTION => {
                    self.section_text(&child, Some(id.clone()), text.clone());
                    self.elements(&child);
                }
                HEADLINE => self.headline(&child),
                _ => {}
            }
        }
    }

    /// The text of a section, for the search.
    fn section_text(&mut self, section: &SyntaxNode, id: Option<String>, heading: Option<String>) {
        let mut text = String::new();
        for child in section.children() {
            match child.kind() {
                PARAGRAPH | LIST | TABLE | BLOCK | FIXED_WIDTH | DRAWER | FOOTNOTE_DEFINITION => {
                    text.push_str(&plain(&child));
                    text.push(' ');
                }
                _ => {}
            }
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() || heading.is_some() {
            self.sections.push(Section { id, heading, text });
        }
    }

    /// The elements of a section, a list item or a block.
    fn elements(&mut self, parent: &SyntaxNode) {
        for child in parent.children() {
            self.element(&child);
        }
    }

    fn element(&mut self, node: &SyntaxNode) {
        match node.kind() {
            PARAGRAPH => self.paragraph(node),
            LIST => self.list(node),
            TABLE => self.table(node),
            BLOCK => self.block(node),
            FIXED_WIDTH => {
                let lines: Vec<String> = node
                    .children_with_tokens()
                    .filter_map(|child| child.into_token())
                    .filter(|token| token.kind() == FIXED_WIDTH_TEXT)
                    .map(|token| {
                        let line = token.text().trim_start();
                        line.strip_prefix(": ").or_else(|| line.strip_prefix(':')).unwrap_or(line).to_string()
                    })
                    .collect();
                self.out.push_str(&format!("<pre class=\"example\">{}</pre>\n", escape(&lines.join("\n"))));
            }
            HORIZONTAL_RULE => self.out.push_str("<hr>\n"),
            DRAWER => {
                let name = node
                    .children_with_tokens()
                    .filter_map(|child| child.into_token())
                    .find(|token| token.kind() == DRAWER_BEGIN)
                    .map(|token| token.text().trim().trim_matches(':').to_uppercase())
                    .unwrap_or_default();
                if name != "LOGBOOK" {
                    self.elements(node);
                }
            }
            FOOTNOTE_DEFINITION => {
                let label = node
                    .children_with_tokens()
                    .filter_map(|child| child.into_token())
                    .find(|token| token.kind() == FOOTNOTE_LABEL)
                    .map(|token| token.text().to_string())
                    .unwrap_or_default();
                let label = footnote_label(&label);
                self.out.push_str(&format!("<div class=\"footnote\" id=\"fn.{label}\"><sup>{}</sup> ", escape(&label)));
                self.elements(node);
                self.out.push_str("</div>\n");
            }
            LATEX_ENVIRONMENT => {
                self.out.push_str(&format!("<pre class=\"latex\">{}</pre>\n", escape(&node.text().to_string())));
            }
            DYNAMIC_BLOCK => self.elements(node),
            // `#+HTML:` goes into the page as it is, as Org's HTML export
            // puts it.
            KEYWORD => {
                let (key, value) = keyword_parts(node);
                if key == "html" {
                    self.out.push_str(&value);
                    self.out.push('\n');
                }
            }
            // Other keywords, comments, planning, property drawers, clocks
            // and diary sexps are not shown.
            _ => {}
        }
    }

    fn paragraph(&mut self, node: &SyntaxNode) {
        // Affiliated keywords: `#+CAPTION:` and `#+ATTR_HTML:`.
        let mut caption = None;
        let mut attributes = String::new();
        for keyword in node.children().filter(|child| child.kind() == KEYWORD) {
            let (key, value) = keyword_parts(&keyword);
            match key.as_str() {
                "caption" => caption = Some(value),
                "attr_html" => attributes = html_attributes(&value),
                _ => {}
            }
        }
        let content: Vec<SyntaxElement> = node
            .children_with_tokens()
            .filter(|child| child.kind() != KEYWORD)
            .filter(|child| !matches!(child.kind(), WHITESPACE | NEWLINE) && !is_blank_text(child))
            .collect();
        // A link to an image alone in its paragraph is a figure.
        if let [SyntaxElement::Node(link)] = content.as_slice() {
            if link.kind() == LINK && !has_description(link) {
                if let Target::Image(src) = self.links.resolve(&link_path(link)) {
                    // The caption is the text an image stands for, unless
                    // `#+ATTR_HTML:` gives one.
                    let alt = match attributes.contains(" alt=\"") {
                        true => String::new(),
                        false => format!(" alt=\"{}\"", escape(&caption.clone().unwrap_or_default())),
                    };
                    self.out.push_str(&format!(
                        "<figure><img src=\"{}\"{alt} loading=\"lazy\"{attributes}>{}</figure>\n",
                        escape(&src),
                        caption
                            .map(|caption| format!("<figcaption>{}</figcaption>", escape(&caption)))
                            .unwrap_or_default()
                    ));
                    return;
                }
            }
        }
        let body = self.objects(node);
        let body = body.trim();
        if !body.is_empty() {
            self.out.push_str(&format!("<p>{body}</p>\n"));
        }
    }

    fn list(&mut self, node: &SyntaxNode) {
        let items: Vec<SyntaxNode> = node.children().filter(|child| child.kind() == LIST_ITEM).collect();
        let descriptive = items.iter().any(|item| item.children().any(|child| child.kind() == ITEM_TAG));
        let first_bullet = items.first().and_then(bullet).unwrap_or_default();
        let ordered = first_bullet.ends_with(['.', ')']);
        if descriptive {
            self.out.push_str("<dl>\n");
            for item in &items {
                let tag = item.children().find(|child| child.kind() == ITEM_TAG);
                let term = tag.as_ref().map(|tag| self.objects(tag)).unwrap_or_default();
                self.out.push_str(&format!("<dt>{}{term}</dt>\n<dd>", checkbox(item)));
                self.item_body(item);
                self.out.push_str("</dd>\n");
            }
            self.out.push_str("</dl>\n");
            return;
        }
        let start = first_bullet.trim_end_matches(['.', ')']).parse::<u64>().ok().filter(|start| *start != 1);
        let tag = if ordered { "ol" } else { "ul" };
        let start = start.map(|start| format!(" start=\"{start}\"")).unwrap_or_default();
        let class = if items.iter().any(|item| !checkbox(item).is_empty()) { " class=\"checklist\"" } else { "" };
        self.out.push_str(&format!("<{tag}{start}{class}>\n"));
        for item in &items {
            self.out.push_str(&format!("<li>{}", checkbox(item)));
            self.item_body(item);
            self.out.push_str("</li>\n");
        }
        self.out.push_str(&format!("</{tag}>\n"));
    }

    /// What an item holds: a lone paragraph without its `<p>`.
    fn item_body(&mut self, item: &SyntaxNode) {
        let elements: Vec<SyntaxNode> =
            item.children().filter(|child| !matches!(child.kind(), ITEM_TAG | KEYWORD)).collect();
        let paragraphs = elements.iter().filter(|child| child.kind() == PARAGRAPH).count();
        for element in &elements {
            if element.kind() == PARAGRAPH && paragraphs == 1 {
                let content = self.objects(element);
                self.out.push_str(content.trim());
            } else {
                self.element(element);
            }
        }
    }

    fn table(&mut self, node: &SyntaxNode) {
        let rows: Vec<SyntaxNode> = node.children().filter(|child| child.kind() == TABLE_ROW).collect();
        let is_rule = |row: &SyntaxNode| row.children_with_tokens().any(|child| child.kind() == TABLE_RULE);
        // Rows of width cookies (`<10>`, `<r>`) are not shown.
        let is_cookies = |row: &SyntaxNode| {
            let cells: Vec<String> =
                row.children().filter(|c| c.kind() == TABLE_CELL).map(|c| c.text().to_string()).collect();
            !cells.is_empty()
                && cells.iter().all(|cell| {
                    let cell = cell.trim();
                    cell.is_empty()
                        || (cell.starts_with('<')
                            && cell.ends_with('>')
                            && cell[1..cell.len() - 1].chars().all(|c| c.is_ascii_digit() || "lrc".contains(c)))
                })
                && cells.iter().any(|cell| !cell.trim().is_empty())
        };
        let data: Vec<&SyntaxNode> = rows.iter().filter(|row| !is_cookies(row)).collect();
        // Rows above the first rule are the header, when rows follow it.
        let first_rule = data.iter().position(|row| is_rule(row));
        let header = match first_rule {
            Some(at) if at > 0 && data[at..].iter().any(|row| !is_rule(row)) => at,
            _ => 0,
        };
        self.out.push_str("<div class=\"table\"><table>\n");
        if header > 0 {
            self.out.push_str("<thead>\n");
            for row in &data[..header] {
                self.row(row, "th");
            }
            self.out.push_str("</thead>\n");
        }
        self.out.push_str("<tbody>\n");
        for row in &data[header..] {
            if !is_rule(row) {
                self.row(row, "td");
            }
        }
        self.out.push_str("</tbody>\n</table></div>\n");
    }

    fn row(&mut self, row: &SyntaxNode, cell_tag: &str) {
        self.out.push_str("<tr>");
        for cell in row.children().filter(|child| child.kind() == TABLE_CELL) {
            let content = self.objects(&cell);
            self.out.push_str(&format!("<{cell_tag}>{}</{cell_tag}>", content.trim()));
        }
        self.out.push_str("</tr>\n");
    }

    fn block(&mut self, node: &SyntaxNode) {
        let mut kind = String::new();
        let mut parameters = String::new();
        for token in node.children_with_tokens().filter_map(|child| child.into_token()) {
            match token.kind() {
                BLOCK_BEGIN => kind = token.text().trim().to_lowercase().trim_start_matches("#+begin_").to_string(),
                BLOCK_PARAMETERS => parameters = token.text().trim().to_string(),
                _ => {}
            }
        }
        match kind.as_str() {
            "src" | "example" => {
                let language = if kind == "src" {
                    parameters.split_whitespace().next().unwrap_or("").to_string()
                } else {
                    String::new()
                };
                let code = block_contents(node);
                let label =
                    if language.is_empty() { String::new() } else { format!(" data-lang=\"{}\"", escape(&language)) };
                self.out.push_str(&format!(
                    "<div class=\"code\"{label}><pre><code>{}</code></pre></div>\n",
                    highlight::code(&language, &code)
                ));
            }
            "verse" => {
                let text = block_contents(node);
                self.out.push_str(&format!("<p class=\"verse\">{}</p>\n", escape(&text).replace('\n', "<br>\n")));
            }
            "export" => {
                if parameters.split_whitespace().next().is_some_and(|backend| backend.eq_ignore_ascii_case("html")) {
                    self.out.push_str(&block_contents(node));
                }
            }
            "comment" => {}
            "quote" => {
                self.out.push_str("<blockquote>\n");
                self.elements(node);
                self.out.push_str("</blockquote>\n");
            }
            "center" => {
                self.out.push_str("<div class=\"center\">\n");
                self.elements(node);
                self.out.push_str("</div>\n");
            }
            // Special blocks (`#+begin_note`): a box named by its kind.
            other => {
                self.out.push_str(&format!("<div class=\"block {}\">\n", escape(other)));
                self.elements(node);
                self.out.push_str("</div>\n");
            }
        }
    }

    /// The objects of a paragraph, a title, a table cell or a link's
    /// description.
    fn objects(&mut self, parent: &SyntaxNode) -> String {
        let mut out = String::new();
        for child in parent.children_with_tokens() {
            match child {
                SyntaxElement::Token(token) => match token.kind() {
                    MARKER => {}
                    TEXT => out.push_str(&text(token.text())),
                    _ => out.push_str(&escape(token.text())),
                },
                SyntaxElement::Node(node) => out.push_str(&self.object(&node)),
            }
        }
        out
    }

    fn object(&mut self, node: &SyntaxNode) -> String {
        match node.kind() {
            BOLD => format!("<strong>{}</strong>", self.objects(node)),
            ITALIC => format!("<em>{}</em>", self.objects(node)),
            UNDERLINE => format!("<span class=\"underline\">{}</span>", self.objects(node)),
            STRIKE_THROUGH => format!("<del>{}</del>", self.objects(node)),
            CODE | VERBATIM => format!("<code>{}</code>", escape(&marked_contents(node))),
            SUBSCRIPT | SUPERSCRIPT => {
                let braced =
                    node.children_with_tokens().any(|child| child.kind() == MARKER && child.to_string() == "{");
                let shown = match self.page.options.scripts {
                    Scripts::Always => true,
                    Scripts::Braces => braced,
                    Scripts::Never => false,
                };
                if shown {
                    let tag = if node.kind() == SUBSCRIPT { "sub" } else { "sup" };
                    format!("<{tag}>{}</{tag}>", self.objects(node))
                } else {
                    escape(&node.text().to_string())
                }
            }
            LINK => self.link(node),
            PLAIN_LINK => {
                let path = node.text().to_string();
                self.anchor(&path, escape(&path))
            }
            ANGLE_LINK => {
                let path: String = node
                    .children_with_tokens()
                    .filter(|child| !matches!(child.kind(), ANGLE_OPEN | ANGLE_CLOSE))
                    .map(|child| child.to_string())
                    .collect();
                self.anchor(&path, escape(&path))
            }
            TIMESTAMP => format!("<span class=\"timestamp\">{}</span>", escape(&node.text().to_string())),
            ENTITY => {
                let raw = node.text().to_string();
                let name = raw.trim_start_matches('\\').trim_end_matches("{}");
                org_syntax::entity_utf8(name).map_or_else(|| escape(&raw), escape)
            }
            LATEX_FRAGMENT => format!("<code class=\"latex\">{}</code>", escape(&node.text().to_string())),
            TARGET => {
                let name = plain(node);
                format!("<span id=\"{}\"></span>", escape(&name))
            }
            RADIO_TARGET => self.objects(node),
            FOOTNOTE_REFERENCE => {
                let raw = node.text().to_string();
                let label = footnote_label(raw.trim_start_matches('[').trim_end_matches(']'));
                format!("<sup class=\"footref\"><a href=\"#fn.{0}\">{0}</a></sup>", escape(&label))
            }
            EXPORT_SNIPPET => {
                let raw = node.text().to_string();
                let inner = raw.trim_start_matches("@@").trim_end_matches("@@");
                match inner.split_once(':') {
                    Some((backend, value)) if backend.eq_ignore_ascii_case("html") => value.to_string(),
                    _ => String::new(),
                }
            }
            MACRO | INLINE_SRC_BLOCK | INLINE_BABEL_CALL | CITATION | STATISTICS_COOKIE => {
                format!("<code>{}</code>", escape(&node.text().to_string()))
            }
            _ => self.objects(node),
        }
    }

    fn link(&mut self, node: &SyntaxNode) -> String {
        let path = link_path(node);
        if !has_description(node) {
            return match self.links.resolve(&path) {
                Target::Image(src) => format!("<img src=\"{}\" alt=\"\" loading=\"lazy\">", escape(&src)),
                Target::Href(href) => format!("<a href=\"{}\">{}</a>", escape(&href), escape(&shown_path(&path))),
            };
        }
        let mut description = String::new();
        let mut inside = false;
        for child in node.children_with_tokens() {
            match child.kind() {
                LINK_SEPARATOR => inside = true,
                LINK_CLOSE => inside = false,
                _ if inside => match child {
                    SyntaxElement::Token(token) if token.kind() == TEXT => description.push_str(&text(token.text())),
                    SyntaxElement::Token(token) if token.kind() == MARKER => {}
                    SyntaxElement::Token(token) => description.push_str(&escape(token.text())),
                    SyntaxElement::Node(node) => description.push_str(&self.object(&node)),
                },
                _ => {}
            }
        }
        self.anchor(&path, description)
    }

    fn anchor(&mut self, path: &str, content: String) -> String {
        match self.links.resolve(path) {
            Target::Href(href) | Target::Image(href) => {
                let external = href.starts_with("http://") || href.starts_with("https://");
                let class = if external && !href.starts_with(crate::REPOSITORY) { " class=\"external\"" } else { "" };
                format!("<a href=\"{}\"{class}>{content}</a>", escape(&href))
            }
        }
    }
}

/// A link's path as shown without a description: `file:` left out.
fn shown_path(path: &str) -> String {
    path.strip_prefix("file:").unwrap_or(path).to_string()
}

fn link_path(node: &SyntaxNode) -> String {
    node.children_with_tokens()
        .filter(|child| child.kind() == LINK_PATH)
        .map(|child| child.to_string())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_description(node: &SyntaxNode) -> bool {
    node.children_with_tokens().any(|child| child.kind() == LINK_SEPARATOR)
}

fn is_blank_text(child: &SyntaxElement) -> bool {
    matches!(child, SyntaxElement::Token(token) if token.kind() == TEXT && token.text().trim().is_empty())
}

fn bullet(item: &SyntaxNode) -> Option<String> {
    item.children_with_tokens().find(|child| child.kind() == BULLET).map(|child| child.to_string().trim().to_string())
}

fn checkbox(item: &SyntaxNode) -> String {
    let Some(state) = item.children_with_tokens().find(|child| child.kind() == CHECKBOX).map(|child| child.to_string())
    else {
        return String::new();
    };
    let (checked, label) = match state.as_str() {
        "[X]" | "[x]" => (" checked", "done"),
        "[-]" => ("", "partly done"),
        _ => ("", "not done"),
    };
    format!("<input type=\"checkbox\" disabled{checked} aria-label=\"{label}\"> ")
}

/// `:width 400 :alt Text` of `#+ATTR_HTML:` as attributes.
fn html_attributes(value: &str) -> String {
    let mut out = String::new();
    let mut parts = value.split(" :").map(|part| part.trim_start_matches(':'));
    for part in parts.by_ref() {
        if let Some((name, value)) = part.split_once(' ') {
            if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !name.is_empty() {
                out.push_str(&format!(" {name}=\"{}\"", escape(value.trim())));
            }
        }
    }
    out
}

/// The text of a code or verbatim object, between its markers.
fn marked_contents(node: &SyntaxNode) -> String {
    let text = node.text().to_string();
    let mut chars = text.chars();
    chars.next();
    chars.next_back();
    chars.as_str().to_string()
}

/// The lines of a block between its begin and end lines, without the
/// indentation they share and with Org's commas taken off (`,*`, `,#+`).
fn block_contents(node: &SyntaxNode) -> String {
    let text = node.text().to_string();
    let start = text.find('\n').map_or(text.len(), |at| at + 1);
    let end = node
        .children_with_tokens()
        .find(|child| child.kind() == BLOCK_END)
        .map(|child| usize::from(child.text_range().start() - node.text_range().start()))
        .unwrap_or(text.len());
    let body = &text[start.min(end)..end];
    let lines: Vec<&str> = body.lines().collect();
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .iter()
        .map(|line| {
            let line = if line.len() >= indent { &line[indent..] } else { line.trim_start() };
            let content = line.trim_start();
            let lead = &line[..line.len() - content.len()];
            match content.strip_prefix(',') {
                Some(rest)
                    if rest.starts_with('*')
                        || rest.starts_with("#+")
                        || rest.starts_with(",*")
                        || rest.starts_with(",#+") =>
                {
                    format!("{lead}{rest}")
                }
                _ => line.to_string(),
            }
        })
        .collect();
    while out.last().is_some_and(|line| line.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn footnote_label(raw: &str) -> String {
    let label = raw.trim_start_matches("fn:");
    label.split(':').next().unwrap_or(label).to_string()
}

/// Text of a paragraph: a `\\` at the end of a line breaks it.
fn text(raw: &str) -> String {
    escape(raw).replace("\\\\\n", "<br>\n")
}

/// Whether a headline is left out: commented, `:noexport:`, or under one
/// that is.
pub fn skipped(node: &SyntaxNode) -> bool {
    std::iter::successors(Some(node.clone()), |node| node.parent()).filter(|node| node.kind() == HEADLINE).any(|node| {
        let headline = org_syntax::ast::Headline::cast(node).unwrap();
        headline.is_commented() || headline.tags().iter().any(|tag| tag == "noexport")
    })
}

/// A headline's `CUSTOM_ID` property.
pub fn custom_id(headline: &SyntaxNode) -> Option<String> {
    let section = headline.children().find(|child| child.kind() == SECTION)?;
    let drawer = section.children().find(|child| child.kind() == PROPERTY_DRAWER)?;
    drawer.children().filter(|child| child.kind() == NODE_PROPERTY).find_map(|property| {
        let mut key = None;
        let mut value = None;
        for token in property.children_with_tokens().filter_map(|child| child.into_token()) {
            match token.kind() {
                PROPERTY_KEY => key = Some(token.text().trim_matches(':').to_uppercase()),
                PROPERTY_VALUE => value = Some(token.text().trim().to_string()),
                _ => {}
            }
        }
        (key.as_deref() == Some("CUSTOM_ID")).then_some(value).flatten()
    })
}

/// Whether `<<id>>` or `#+NAME: id` is in the document.
pub fn has_target(root: &SyntaxNode, id: &str) -> bool {
    root.descendants().any(|node| match node.kind() {
        TARGET => plain(&node) == id,
        KEYWORD => {
            let (key, value) = keyword_parts(&node);
            key == "name" && value == id
        }
        _ => false,
    })
}

/// The text a reader sees, without markup.
pub fn plain(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Token(token) => match token.kind() {
                MARKER | LINK_OPEN | LINK_SEPARATOR | LINK_CLOSE | ANGLE_OPEN | ANGLE_CLOSE | BLOCK_BEGIN
                | BLOCK_END | BLOCK_PARAMETERS | KEYWORD_KEY | DRAWER_BEGIN | DRAWER_END | PROPERTY_KEY
                | TABLE_RULE | PIPE | BULLET | TAG_SEPARATOR | STARS => {}
                LINK_PATH if has_description(node) => {}
                NEWLINE | BLANK_LINE => out.push(' '),
                _ => out.push_str(token.text()),
            },
            SyntaxElement::Node(child) => match child.kind() {
                CODE | VERBATIM => out.push_str(&marked_contents(&child)),
                ENTITY => {
                    let raw = child.text().to_string();
                    let name = raw.trim_start_matches('\\').trim_end_matches("{}");
                    out.push_str(org_syntax::entity_utf8(name).unwrap_or(&raw));
                }
                KEYWORD | PROPERTY_DRAWER | COMMENT => {}
                _ => {
                    out.push_str(&plain(&child));
                    if matches!(child.kind(), TABLE_CELL | LIST_ITEM | PARAGRAPH) {
                        out.push(' ');
                    }
                }
            },
        }
    }
    out
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(text: &str) -> String {
        let root = org_syntax::parse(text, &org_syntax::TodoKeywords::default()).syntax();
        let block = root.descendants().find(|node| node.kind() == BLOCK).unwrap();
        block_contents(&block)
    }

    #[test]
    fn blocks_lose_their_shared_indentation_and_commas() {
        assert_eq!(contents("  #+begin_src org\n  ,* a\n    b\n  #+end_src\n"), "* a\n  b");
        assert_eq!(contents("#+begin_example\nx\n\n#+end_example\n"), "x");
    }

    #[test]
    fn plain_text_drops_the_markup() {
        let root = org_syntax::parse("* The ~foo~ [[https://x][link]] \\alpha\n", &Default::default()).syntax();
        let title = root.descendants().find(|node| node.kind() == TITLE).unwrap();
        assert_eq!(plain(&title), "The foo link α");
    }

    #[test]
    fn attributes_of_attr_html() {
        assert_eq!(html_attributes(":width 400 :alt Two words"), " width=\"400\" alt=\"Two words\"");
    }
}
