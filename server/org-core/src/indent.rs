//! Indentation as Org computes it, under `org-adapt-indentation`:
//! `org-indent-line`, which `TAB` and `RET` run, `org-indent-region`, and
//! `org-fixup-indentation`, which moves an entry's text with its headline.
//!
//! Org works on elements (`org-element-at-point`) and their extents,
//! where the blank lines after an element belong to it. The elements are
//! read from the syntax tree into that form, and the indentation of a
//! line is computed from them as `org--get-expected-indentation` does.
//! `org-indent-region` reads them again after each change, as Emacs does,
//! since a line moved can change what the lines after it belong to.
//! Checked against Emacs in `tests/indent`.

use org_syntax::{ast, SyntaxKind::*, SyntaxNode, TodoKeywords};

/// `org-adapt-indentation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Adapt {
    /// `nil`: text is not indented to its headline.
    #[default]
    Off,
    /// `t`: the lines of an entry are indented to its headline, past the
    /// stars.
    All,
    /// `headline-data`: only the lines right below a headline (planning,
    /// drawers, clock lines) are.
    HeadlineData,
}

/// What indentation follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub adapt: Adapt,
    /// `org-log-into-drawer`: the drawer that counts as headline data.
    pub log_into_drawer: Option<String>,
    /// `org-src-preserve-indentation`: source blocks keep the indentation
    /// of their code.
    pub src_preserve_indentation: bool,
    /// `org-edit-src-content-indentation`: how far code is indented past
    /// its block.
    pub src_content_indentation: usize,
    /// `org-list-two-spaces-after-bullet-regexp`: the text of an item
    /// whose bullet it matches starts two columns after the bullet.
    pub two_spaces: Option<crate::list::BulletRegexp>,
    /// `org-property-format`: how a node property's key and value are
    /// written when the line is indented.
    pub property_format: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            adapt: Adapt::Off,
            log_into_drawer: None,
            src_preserve_indentation: false,
            src_content_indentation: 2,
            two_spaces: None,
            property_format: "%-10s %s".to_string(),
        }
    }
}

const TAB_WIDTH: usize = 8;

/// A change to the text: `start..end` replaced by `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// `text` with `edits`, which are in order and do not overlap.
pub fn apply(text: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for edit in edits {
        out.push_str(&text[at..edit.start]);
        out.push_str(&edit.text);
        at = edit.end;
    }
    out.push_str(&text[at..]);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Headline,
    Section,
    Planning,
    PropertyDrawer,
    NodeProperty,
    Drawer,
    Clock,
    Paragraph,
    PlainList,
    Item,
    Table,
    TableRow,
    SrcBlock,
    ExampleBlock,
    ExportBlock,
    CommentBlock,
    VerseBlock,
    GreaterBlock,
    DynamicBlock,
    Keyword,
    Comment,
    FixedWidth,
    HorizontalRule,
    FootnoteDefinition,
    LatexEnvironment,
    DiarySexp,
}

impl Kind {
    /// `org-element-greater-elements`
    fn is_greater(self) -> bool {
        matches!(
            self,
            Kind::Headline
                | Kind::Section
                | Kind::PropertyDrawer
                | Kind::Drawer
                | Kind::PlainList
                | Kind::Item
                | Kind::Table
                | Kind::GreaterBlock
                | Kind::DynamicBlock
                | Kind::FootnoteDefinition
        )
    }
}

#[derive(Debug, Clone)]
struct Element {
    kind: Kind,
    /// The first line, affiliated keywords included.
    begin: usize,
    /// The line after the affiliated keywords.
    post_affiliated: usize,
    /// Where the next element starts: the blank lines after this one
    /// belong to it.
    end: usize,
    contents: Option<(usize, usize)>,
    parent: Option<usize>,
    /// The level of a headline.
    level: usize,
    /// The name of a drawer.
    name: String,
    /// The element's `#+NAME:` (or a synonym), the last one.
    label: Option<String>,
    /// A source or example block whose code keeps its indentation (`-i`).
    preserve: bool,
}

/// A document read into elements, as `org-element-at-point` sees them.
struct Snapshot {
    text: String,
    elements: Vec<Element>,
    /// The elements in each element, and at the top.
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
    settings: Settings,
}

fn element_kind(node: &SyntaxNode) -> Option<Kind> {
    Some(match node.kind() {
        HEADLINE => Kind::Headline,
        SECTION => Kind::Section,
        PLANNING => Kind::Planning,
        PROPERTY_DRAWER => Kind::PropertyDrawer,
        NODE_PROPERTY => Kind::NodeProperty,
        DRAWER => Kind::Drawer,
        CLOCK => Kind::Clock,
        PARAGRAPH => Kind::Paragraph,
        LIST => Kind::PlainList,
        LIST_ITEM => Kind::Item,
        TABLE => Kind::Table,
        TABLE_ROW => Kind::TableRow,
        BLOCK => {
            let begin = node
                .children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == BLOCK_BEGIN)?;
            match begin.text().get(8..).unwrap_or("").to_ascii_uppercase().as_str() {
                "SRC" => Kind::SrcBlock,
                "EXAMPLE" => Kind::ExampleBlock,
                "EXPORT" => Kind::ExportBlock,
                "COMMENT" => Kind::CommentBlock,
                "VERSE" => Kind::VerseBlock,
                _ => Kind::GreaterBlock,
            }
        }
        DYNAMIC_BLOCK => Kind::DynamicBlock,
        KEYWORD if ast::is_affiliated_keyword(node) => return None,
        KEYWORD => Kind::Keyword,
        COMMENT => Kind::Comment,
        FIXED_WIDTH => Kind::FixedWidth,
        HORIZONTAL_RULE => Kind::HorizontalRule,
        FOOTNOTE_DEFINITION => Kind::FootnoteDefinition,
        LATEX_ENVIRONMENT => Kind::LatexEnvironment,
        DIARY_SEXP => Kind::DiarySexp,
        _ => return None,
    })
}

fn offset(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

/// The `#+NAME:` of an element (`org-element`'s `:name`): the last of its
/// affiliated `NAME` keywords or their synonyms.
fn affiliated_name(node: &SyntaxNode) -> Option<String> {
    let mut name = None;
    for keyword in node.children().take_while(ast::is_affiliated_keyword) {
        let token = |kind| {
            keyword
                .children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == kind)
                .map(|token| token.text().to_string())
        };
        let key = token(KEYWORD_KEY).unwrap_or_default().to_ascii_uppercase();
        let key = key.trim_start_matches("#+").trim_end_matches(':');
        if matches!(key, "NAME" | "DATA" | "LABEL" | "RESNAME" | "SOURCE" | "SRCNAME" | "TBLNAME") {
            name = Some(token(KEYWORD_VALUE).unwrap_or_default().trim().to_string());
        }
    }
    name
}

/// The elements of a text, to ask what is at a place, as
/// `org-element-at-point` answers.
pub struct Elements(Snapshot);

/// An element as `org-element-at-point` returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementAt {
    /// Its type, as `org-element-type` names it (`paragraph`,
    /// `src-block`…).
    pub kind: &'static str,
    /// Its first line, affiliated keywords included.
    pub begin: usize,
    /// The line after its affiliated keywords.
    pub post_affiliated: usize,
    /// Where the next element starts.
    pub end: usize,
    /// Its contents, for a greater element.
    pub contents: Option<(usize, usize)>,
}

impl Elements {
    pub fn new(text: &str) -> Elements {
        Elements(Snapshot::new(text, &Settings::default()))
    }

    /// `org-element-at-point` at `at`.
    pub fn at(&self, at: usize) -> Option<ElementAt> {
        let index = self.0.element_at(at)?;
        let element = &self.0.elements[index];
        let kind = match element.kind {
            Kind::Headline => "headline",
            Kind::Section => "section",
            Kind::Planning => "planning",
            Kind::PropertyDrawer => "property-drawer",
            Kind::NodeProperty => "node-property",
            Kind::Drawer => "drawer",
            Kind::Clock => "clock",
            Kind::Paragraph => "paragraph",
            Kind::PlainList => "plain-list",
            Kind::Item => "item",
            Kind::Table => "table",
            Kind::TableRow => "table-row",
            Kind::SrcBlock => "src-block",
            Kind::ExampleBlock => "example-block",
            Kind::ExportBlock => "export-block",
            Kind::CommentBlock => "comment-block",
            Kind::VerseBlock => "verse-block",
            Kind::GreaterBlock => match element.name.as_str() {
                "CENTER" => "center-block",
                "QUOTE" => "quote-block",
                _ => "special-block",
            },
            Kind::DynamicBlock => "dynamic-block",
            Kind::Keyword => "keyword",
            Kind::Comment => "comment",
            Kind::FixedWidth => "fixed-width",
            Kind::HorizontalRule => "horizontal-rule",
            Kind::FootnoteDefinition => "footnote-definition",
            Kind::LatexEnvironment => "latex-environment",
            Kind::DiarySexp => "diary-sexp",
        };
        Some(ElementAt {
            kind,
            begin: element.begin,
            post_affiliated: element.post_affiliated,
            end: element.end,
            contents: element.contents,
        })
    }
}

/// `org-element-at-point` for links: where the element at `at` begins,
/// its affiliated keywords included, and its `#+NAME:`.
pub fn element_at(text: &str, at: usize) -> Option<(usize, Option<String>)> {
    let snapshot = Snapshot::new(text, &Settings::default());
    let index = snapshot.element_at(at)?;
    let element = &snapshot.elements[index];
    Some((element.begin, element.label.clone()))
}

/// Whether `text` has the `-i` switch of a block that keeps its
/// indentation.
fn has_preserve_switch(node: &SyntaxNode) -> bool {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == BLOCK_PARAMETERS)
        .is_some_and(|token| token.text().split_whitespace().any(|word| word == "-i"))
}

impl Snapshot {
    fn new(text: &str, settings: &Settings) -> Self {
        let root = org_syntax::parse(text, &TodoKeywords::default()).syntax();
        let mut snapshot = Snapshot {
            text: text.to_string(),
            elements: Vec::new(),
            children: Vec::new(),
            roots: Vec::new(),
            settings: settings.clone(),
        };
        snapshot.collect(&root, None);
        snapshot
    }

    fn collect(&mut self, node: &SyntaxNode, parent: Option<usize>) {
        let mut children = node.children_with_tokens().peekable();
        while let Some(child) = children.next() {
            let Some(child) = child.into_node() else { continue };
            let Some(kind) = element_kind(&child) else { continue };
            let (begin, mut end) = offset(&child);
            // The blank lines after it.
            while let Some(blank) =
                children.peek().and_then(|next| next.as_token()).filter(|token| token.kind() == BLANK_LINE)
            {
                end = usize::from(blank.text_range().end());
                children.next();
            }
            let first = child
                .children_with_tokens()
                .find(|grandchild| grandchild.as_node().is_none_or(|node| !ast::is_affiliated_keyword(node)));
            let post_affiliated =
                first.map_or(begin, |first| self.line_start(usize::from(first.text_range().start())).max(begin));
            let level = if kind == Kind::Headline {
                ast::Headline::cast(child.clone()).map_or(0, |headline| headline.level())
            } else {
                0
            };
            let name = if kind == Kind::Drawer {
                child
                    .children_with_tokens()
                    .filter_map(|grandchild| grandchild.into_token())
                    .find(|token| token.kind() == DRAWER_BEGIN)
                    .map(|token| token.text().trim_matches(':').to_string())
                    .unwrap_or_default()
            } else if kind == Kind::GreaterBlock {
                // The type of a greater block: `CENTER`, `QUOTE`, or a
                // special block's.
                child
                    .children_with_tokens()
                    .filter_map(|grandchild| grandchild.into_token())
                    .find(|token| token.kind() == BLOCK_BEGIN)
                    .map(|token| token.text().get(8..).unwrap_or("").to_ascii_uppercase())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let preserve = matches!(kind, Kind::SrcBlock | Kind::ExampleBlock) && has_preserve_switch(&child);
            let label = affiliated_name(&child);
            let index = self.elements.len();
            match parent {
                Some(parent) => self.children[parent].push(index),
                None => self.roots.push(index),
            }
            self.children.push(Vec::new());
            self.elements.push(Element {
                kind,
                begin,
                post_affiliated,
                end,
                contents: None,
                parent,
                level,
                name,
                label,
                preserve,
            });
            let contents = self.contents(&child, kind, post_affiliated, offset(&child).1);
            self.elements[index].contents = contents;
            // Blank lines at the start of a block's contents are a
            // paragraph of their own to `org-element-at-point`.
            if let (Some((start, end)), Kind::GreaterBlock | Kind::DynamicBlock) = (contents, kind) {
                let first =
                    child.children().filter(|node| element_kind(node).is_some()).map(|node| offset(&node).0).next();
                let first = first.unwrap_or(end).min(end);
                if first > start {
                    self.children[index].push(self.elements.len());
                    self.children.push(Vec::new());
                    self.elements.push(Element {
                        kind: Kind::Paragraph,
                        begin: start,
                        post_affiliated: start,
                        end: first,
                        contents: None,
                        parent: Some(index),
                        level: 0,
                        name: String::new(),
                        label: None,
                        preserve: false,
                    });
                }
            }
            if kind.is_greater() {
                self.collect(&child, Some(index));
            }
        }
    }

    /// Where the contents of an element start and end.
    fn contents(
        &self,
        node: &SyntaxNode,
        kind: Kind,
        post_affiliated: usize,
        node_end: usize,
    ) -> Option<(usize, usize)> {
        let child_elements =
            || node.children().filter(|child| element_kind(child).is_some() || child.kind() == SECTION);
        let range = match kind {
            Kind::Headline => {
                let section_start =
                    node.children().find(|child| child.kind() == SECTION).map(|section| offset(&section).0);
                let after = self.next_line(post_affiliated);
                (section_start.unwrap_or(after).min(after), node_end)
            }
            Kind::Section => (offset(node).0, node_end),
            Kind::Drawer | Kind::PropertyDrawer | Kind::GreaterBlock | Kind::DynamicBlock | Kind::VerseBlock => {
                let mut start = self.next_line(post_affiliated);
                // A drawer's contents start at its first line with text.
                if kind == Kind::Drawer {
                    while start < self.text.len() && self.is_blank_line(start) {
                        start = self.next_line(start);
                    }
                }
                let end_line = node
                    .children_with_tokens()
                    .filter_map(|child| child.into_token())
                    .find(|token| matches!(token.kind(), DRAWER_END | BLOCK_END))
                    .map(|token| self.line_start(usize::from(token.text_range().start())))?;
                (start, end_line)
            }
            Kind::PlainList | Kind::Table => {
                let first = child_elements().next()?;
                let last = child_elements().last()?;
                (offset(&first).0, self.trim_blank_end(offset(&last).1))
            }
            Kind::Item | Kind::FootnoteDefinition => {
                let first = child_elements().next()?;
                (offset(&first).0, self.trim_blank_end(node_end))
            }
            _ => return None,
        };
        (range.0 < range.1).then_some(range)
    }

    // ----- text

    fn line_start(&self, at: usize) -> usize {
        self.text[..at.min(self.text.len())].rfind('\n').map_or(0, |newline| newline + 1)
    }

    fn line_end(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |newline| at + newline)
    }

    fn next_line(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |newline| at + newline + 1)
    }

    fn line(&self, at: usize) -> &str {
        let start = self.line_start(at);
        &self.text[start..self.line_end(start)]
    }

    fn is_blank_line(&self, at: usize) -> bool {
        self.line(at).bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'))
    }

    /// The start of the line after the last line with text before `end`.
    fn trim_blank_end(&self, end: usize) -> usize {
        let trimmed = self.text[..end].trim_end_matches([' ', '\t', '\r', '\n']).len();
        if trimmed == 0 {
            return 0;
        }
        // From the start of the last line with text: a byte back from
        // `trimmed` may be inside a character.
        self.next_line(self.line_start(trimmed))
    }

    /// `current-indentation` of the line at `at`.
    fn indentation(&self, at: usize) -> usize {
        width(self.line(at))
    }

    fn is_heading(&self, at: usize) -> bool {
        let line = self.line(at);
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        stars > 0 && line.as_bytes().get(stars) == Some(&b' ')
    }

    /// `org-list-item-body-column`: past the bullet and one space, or two
    /// when `org-list-two-spaces-after-bullet-regexp` matches the bullet.
    fn body_column(&self, item: usize) -> usize {
        let line = self.line(item);
        let blanks = line.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
        let bullet: String = line[blanks..].chars().take_while(|c| !c.is_whitespace()).collect();
        self.indentation(item) + bullet.chars().count() + spaces_after(&bullet, self.settings.two_spaces.as_ref())
    }

    // ----- elements

    fn kind(&self, element: Option<usize>) -> Option<Kind> {
        element.map(|index| self.elements[index].kind)
    }

    fn parent(&self, element: Option<usize>) -> Option<usize> {
        element.and_then(|index| self.elements[index].parent)
    }

    /// `org-element-at-point`: the innermost element at `at`, going into
    /// a greater element only within its contents. Blank lines at the
    /// start of a section stand for its headline.
    fn element_at(&self, at: usize) -> Option<usize> {
        let at = at.min(self.text.len().saturating_sub(1));
        let mut candidates = &self.roots;
        let mut found = None;
        // Siblings follow each other in the text: the one that holds `at`
        // is the last to begin at or before it, if it ends after.
        let holding = |candidates: &[usize]| {
            let after = candidates.partition_point(|index| self.elements[*index].begin <= at);
            let index = *candidates.get(after.checked_sub(1)?)?;
            (at < self.elements[index].end).then_some(index)
        };
        while let Some(index) = holding(candidates) {
            found = Some(index);
            let element = &self.elements[index];
            match element.contents {
                // At the start of a list or a table, the list or the table.
                Some((start, end))
                    if element.kind.is_greater()
                        && (start < at || (start == at && !matches!(element.kind, Kind::PlainList | Kind::Table)))
                        && at < end =>
                {
                    candidates = &self.children[index];
                }
                _ => break,
            }
        }
        match self.kind(found) {
            Some(Kind::Section) => self.parent(found),
            _ => found,
        }
    }

    /// The ancestors of an element, nearest last.
    fn ancestors(&self, index: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut parent = self.elements[index].parent;
        while let Some(current) = parent {
            out.push(current);
            parent = self.elements[current].parent;
        }
        out.reverse();
        out
    }

    /// `org--at-headline-data-p`
    fn at_headline_data(&self, at: usize, element: Option<usize>) -> bool {
        let Some(mut element) = element.or_else(|| self.element_at(at)) else { return false };
        if self.elements[element].kind == Kind::Headline
            || !self.ancestors(element).iter().any(|ancestor| self.elements[*ancestor].kind == Kind::Headline)
        {
            return false;
        }
        while let Some(parent) =
            self.elements[element].parent.filter(|parent| self.elements[*parent].kind != Kind::Section)
        {
            element = parent;
        }
        let element = &self.elements[element];
        match element.kind {
            Kind::Planning | Kind::PropertyDrawer => true,
            Kind::Drawer => self.settings.log_into_drawer.as_deref() == Some(element.name.as_str()),
            Kind::Clock => {
                let before = element.begin.saturating_sub(1);
                self.is_heading(before) || self.at_headline_data(before, None)
            }
            _ => false,
        }
    }

    /// `org--get-expected-indentation`: the column for the line at `line`
    /// in `element`, or for the contents of `element` with `contents`.
    fn expected(&self, element: Option<usize>, contents: bool, line: usize) -> usize {
        let kind = self.kind(element);
        if contents {
            return match kind {
                Some(Kind::DiarySexp | Kind::FootnoteDefinition) => 0,
                Some(Kind::Section) => self.expected(self.parent(element), true, line),
                Some(Kind::Headline) | None => match self.settings.adapt {
                    Adapt::Off => 0,
                    _ => element.map_or(0, |index| self.elements[index].level + 1),
                },
                Some(Kind::Item | Kind::PlainList) => {
                    self.body_column(self.elements[element.expect("an element")].post_affiliated)
                }
                Some(_) => self.indentation(self.elements[element.expect("an element")].begin),
            };
        }
        let line = self.line_start(line);
        match kind {
            Some(Kind::Headline) | None => {
                return if self.is_blank_line(line) { self.expected(element, true, line) } else { 0 };
            }
            Some(Kind::DiarySexp | Kind::FootnoteDefinition) => return 0,
            _ => {}
        }
        let index = element.expect("an element");
        let this = &self.elements[index];
        if line < this.begin {
            return self.expected(this.parent, true, line);
        }
        if line == this.begin {
            return self.first_line(index);
        }
        // Inside the element: from the line with text above.
        let above = self.text[..line].trim_end_matches([' ', '\t', '\r', '\n']).len();
        let above_line = self.line_start(above);
        if matches!(this.kind, Kind::FootnoteDefinition | Kind::PlainList)
            && self.text[above..line].matches('\n').count() > 2
        {
            return self.indentation(this.begin);
        }
        if above_line < this.begin {
            return self.expected(this.parent, true, line);
        }
        if above_line == this.post_affiliated {
            return self.expected(element, true, line);
        }
        let block = matches!(
            this.kind,
            Kind::CommentBlock | Kind::ExampleBlock | Kind::ExportBlock | Kind::SrcBlock | Kind::VerseBlock
        );
        if this.kind.is_greater() || block {
            let contents_end = this.contents.map(|(_, end)| end).unwrap_or_else(|| {
                let last = self.text[..this.end].trim_end_matches([' ', '\t', '\r', '\n']).len();
                self.line_start(last)
            });
            if contents_end <= line {
                if matches!(this.kind, Kind::FootnoteDefinition | Kind::Item | Kind::PlainList) {
                    let last = self.element_at(above);
                    return self.expected(last, self.kind(last) == Some(Kind::Item), line);
                }
                return self.indentation(this.begin);
            }
        }
        self.indentation(above_line)
    }

    /// The column of the first line of `element`: like the element before
    /// it, or like the contents of its parent.
    fn first_line(&self, element: usize) -> usize {
        let mut start = self.elements[element].begin;
        loop {
            if start == 0 {
                return 0;
            }
            let mut previous = self.element_at(start - 1);
            let mut parent = previous;
            while let Some(index) = parent.filter(|index| self.elements[*index].end <= start) {
                previous = Some(index);
                parent = self.elements[index].parent;
            }
            let Some(index) = previous else { return 0 };
            let found = &self.elements[index];
            if found.end > start {
                return self.expected(previous, true, start);
            }
            if found.kind == Kind::FootnoteDefinition {
                start = found.begin;
                continue;
            }
            if self.settings.adapt == Adapt::HeadlineData
                && !self.at_headline_data(start, Some(element))
                && (self.is_heading(start - 1) || self.at_headline_data(start - 1, previous))
            {
                return 0;
            }
            return if self.line_start(found.begin) == found.begin {
                self.indentation(found.begin)
            } else {
                self.expected(found.parent, true, start)
            };
        }
    }

    /// `org-indent-line`: the column for the line at `line`, or `None`
    /// when Org leaves it as it is.
    fn line_column(&self, line: usize) -> Option<usize> {
        let line = self.line_start(line);
        let element = self.element_at(line);
        if self.is_heading(line) {
            return None;
        }
        if self.settings.adapt == Adapt::HeadlineData && !self.at_headline_data(line, element) {
            let after_data = match element {
                Some(index) if self.elements[index].kind == Kind::Headline => true,
                Some(index) => {
                    let before = self.elements[index].begin.saturating_sub(1);
                    self.elements[index].begin > 0 && (self.is_heading(before) || self.at_headline_data(before, None))
                }
                None => false,
            };
            if after_data {
                return None;
            }
        }
        if let Some(index) = element {
            let this = &self.elements[index];
            let last_line = || {
                let last = self.text[..this.end].trim_end_matches([' ', '\t', '\r', '\n']).len();
                self.line_start(last)
            };
            match this.kind {
                Kind::PlainList | Kind::Item if line == this.post_affiliated => return None,
                Kind::LatexEnvironment if line >= this.post_affiliated && line <= last_line() => return None,
                // Code: past the block by `org-edit-src-content-indentation`
                // on an empty line, else as the language indents it, which
                // only an editor for the language knows.
                Kind::SrcBlock if line > this.post_affiliated && line < last_line() => {
                    let preserve = this.preserve || self.settings.src_preserve_indentation;
                    return (!preserve && self.line(line).is_empty())
                        .then(|| self.indentation(this.begin) + self.settings.src_content_indentation);
                }
                _ => {}
            }
        }
        Some(self.expected(element, false, line))
    }

    /// The text of the node property at `line` aligned as
    /// `org-property-format` (`%-10s %s`) has it, without indentation
    /// (`org--align-node-property`).
    fn align_property(&self, line: usize) -> Option<String> {
        let text = self.line(line).trim_start_matches([' ', '\t']);
        let rest = text.strip_prefix(':')?;
        let mut key_end = None;
        for (index, c) in rest.char_indices() {
            if c.is_whitespace() {
                break;
            }
            if c == ':' && rest[index + 1..].chars().next().is_none_or(char::is_whitespace) && index > 0 {
                key_end = Some(index + 1);
            }
        }
        let key_end = key_end?;
        let key = &text[..key_end + 1];
        let value = rest[key_end..].trim_matches([' ', '\t', '\r']);
        let formatted = crate::text::format_strings(&self.settings.property_format, &[key, value]);
        Some(crate::text::trim(&formatted).to_string())
    }

    /// The index of the line at `at`.
    fn line_index(&self, at: usize) -> usize {
        self.text[..at.min(self.text.len())].matches('\n').count()
    }

    /// The index of the first line that starts at or after `at`.
    fn line_index_after(&self, at: usize) -> usize {
        let index = self.line_index(at);
        if self.line_start(at) == at {
            index
        } else {
            index + 1
        }
    }
}

/// The width of the leading blanks of `text`, tabs to 8 columns.
pub fn width(text: &str) -> usize {
    text.bytes().take_while(|b| matches!(b, b' ' | b'\t')).fold(0, |column, b| {
        if b == b'\t' {
            (column / TAB_WIDTH + 1) * TAB_WIDTH
        } else {
            column + 1
        }
    })
}

/// `indent-line-to` with `indent-tabs-mode` off: `line` indented to
/// `column`, keeping what it can of the blanks it has.
fn indent_line_to(line: &str, column: usize) -> String {
    let blanks = line.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let (prefix, rest) = line.split_at(blanks);
    let current = width(prefix);
    if current == column {
        return line.to_string();
    }
    if current < column {
        let mut prefix = prefix;
        if column - current / TAB_WIDTH * TAB_WIDTH >= TAB_WIDTH {
            prefix = prefix.trim_end_matches(' ');
        }
        return format!("{prefix}{}{rest}", " ".repeat(column - width(prefix)));
    }
    // Back to `column`; a tab that spans it becomes spaces.
    let mut kept = String::new();
    let mut at = 0;
    for c in prefix.chars() {
        let next = if c == '\t' { (at / TAB_WIDTH + 1) * TAB_WIDTH } else { at + 1 };
        if next > column {
            kept.push_str(&" ".repeat(column - at));
            break;
        }
        kept.push(c);
        at = next;
        if at == column {
            break;
        }
    }
    format!("{kept}{rest}")
}

fn is_blank(line: &str) -> bool {
    line.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'))
}

/// `org-indent-region` as Emacs runs it: the lines change as it goes, and
/// each element is read from the document as it is by then. Changing
/// indentation can change what a line belongs to (a list moved left can
/// take in the paragraph after it). Lines are neither added nor removed,
/// so line numbers stand for Emacs's markers.
struct Pass {
    lines: Vec<String>,
    settings: Settings,
    /// The parse of the top-level subtree being indented, made again
    /// after a change.
    window: Option<Window>,
    changed: bool,
    /// The whole text as one part: what the parts must come to.
    #[cfg(test)]
    whole: bool,
}

/// The parse of lines `first` up to `end`: a top-level headline's
/// subtree, or the text before the first headline. Org parses each such
/// part on its own, so the elements there are those of the whole text,
/// and a change is parsed again in its part alone rather than in the
/// whole document, once for each line indented.
struct Window {
    first: usize,
    end: usize,
    snapshot: Snapshot,
    /// Where each of its lines starts in `snapshot`'s text.
    starts: Vec<usize>,
}

/// What `org-indent-region` needs of an element, in lines.
struct Found {
    kind: Kind,
    has_contents: bool,
    preserve: bool,
    column: usize,
    begin: usize,
    post_affiliated: usize,
    end: usize,
    contents_begin: usize,
    contents_end: usize,
}

impl Pass {
    fn new(text: &str, settings: &Settings) -> Self {
        Pass {
            lines: text.split('\n').map(str::to_string).collect(),
            settings: settings.clone(),
            window: None,
            changed: false,
            #[cfg(test)]
            whole: false,
        }
    }

    /// The parse of the part of the text `line` is in, as it is now.
    fn window(&mut self, line: usize) -> &Window {
        let top = |text: &String| is_heading_line(text);
        let first =
            (0..=line.min(self.lines.len().saturating_sub(1))).rev().find(|at| top(&self.lines[*at])).unwrap_or(0);
        let end = (first + 1..self.lines.len()).find(|at| top(&self.lines[*at])).unwrap_or(self.lines.len());
        #[cfg(test)]
        let (first, end) = if self.whole { (0, self.lines.len()) } else { (first, end) };
        let current = self.window.as_ref().is_some_and(|window| window.first == first && window.end == end);
        if self.changed || !current {
            // The newline before the next headline ends the part's last
            // element, as it does in the whole text.
            let mut text = self.lines[first..end].join("\n");
            if end < self.lines.len() {
                text.push('\n');
            }
            let mut starts = Vec::with_capacity(end - first);
            let mut at = 0;
            for line in &self.lines[first..end] {
                starts.push(at);
                at += line.len() + 1;
            }
            let snapshot = Snapshot::new(&text, &self.settings);
            self.window = Some(Window { first, end, snapshot, starts });
            self.changed = false;
        }
        self.window.as_ref().expect("made above")
    }

    /// `indent-line-to`: a line already at `column` stays as it is.
    fn set(&mut self, line: usize, column: usize) {
        let text = indent_line_to(&self.lines[line], column);
        if text != self.lines[line] {
            self.lines[line] = text;
            self.changed = true;
        }
    }

    /// `column` for the lines with text from `from` up to `to`; where it
    /// stopped.
    fn indent_lines(&mut self, from: usize, to: usize, column: usize) -> usize {
        let to = to.min(self.lines.len());
        for line in from..to {
            if !is_blank(&self.lines[line]) {
                self.set(line, column);
            }
        }
        to.max(from)
    }

    /// `indent-rigidly`: the lines from `from` up to `to` moved by
    /// `offset` columns. Blank lines lose their blanks.
    fn shift(&mut self, from: usize, to: usize, offset: isize) {
        for line in from..to.min(self.lines.len()) {
            if is_blank(&self.lines[line]) {
                if !self.lines[line].is_empty() {
                    self.lines[line].clear();
                    self.changed = true;
                }
            } else {
                let text = &self.lines[line];
                let column = (width(text) as isize + offset).max(0) as usize;
                let blanks = text.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
                let shifted = format!("{}{}", " ".repeat(column), &text[blanks..]);
                if shifted != *text {
                    self.lines[line] = shifted;
                    self.changed = true;
                }
            }
        }
    }

    /// `org--align-node-property` on `line`.
    fn align(&mut self, line: usize) {
        let window = self.window(line);
        let start = window.starts[line - window.first];
        let Some(aligned) = window.snapshot.align_property(start) else { return };
        let text = &self.lines[line];
        let blanks = text.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
        if text[blanks..] != aligned {
            self.lines[line] = format!("{}{aligned}", &text[..blanks]);
            self.changed = true;
        }
    }

    /// The element at the start of `line` and what `org-indent-region`
    /// needs of it.
    fn found(&mut self, line: usize) -> Option<Found> {
        let window = self.window(line);
        let (snapshot, base) = (&window.snapshot, window.first);
        let start = window.starts[line - base];
        let index = snapshot.element_at(start)?;
        let element = &snapshot.elements[index];
        let column = snapshot.expected(Some(index), false, start);
        let last_line = || {
            let last = snapshot.text[..element.end].trim_end_matches([' ', '\r', '\t', '\n']).len();
            snapshot.line_index(last)
        };
        let post_affiliated = snapshot.line_index(element.post_affiliated);
        let contents_begin = match (element.contents, element.kind) {
            (None, _) => post_affiliated + 1,
            // Contents that may start on the first line: from the next
            // line with text.
            (Some(_), Kind::FootnoteDefinition | Kind::Item | Kind::PlainList) => {
                let line_end = snapshot.line_end(element.post_affiliated);
                let rest = &snapshot.text[line_end..];
                let skipped = line_end + rest.len() - rest.trim_start_matches([' ', '\r', '\t', '\n']).len();
                if skipped >= snapshot.text.len() {
                    snapshot.line_index_after(skipped)
                } else {
                    snapshot.line_index(skipped)
                }
            }
            (Some((begin, _)), _) => snapshot.line_index_after(begin),
        };
        let contents_end = match element.contents {
            Some((_, end)) => snapshot.line_index_after(end),
            None => last_line(),
        };
        // In lines of the whole text.
        Some(Found {
            kind: element.kind,
            has_contents: element.contents.is_some(),
            preserve: element.preserve,
            column,
            begin: base + snapshot.line_index(element.begin),
            post_affiliated: base + post_affiliated,
            end: base + snapshot.line_index_after(element.end),
            contents_begin: base + contents_begin,
            contents_end: base + contents_end,
        })
    }

    fn is_heading(&self, line: usize) -> bool {
        let text = &self.lines[line];
        let stars = text.bytes().take_while(|b| *b == b'*').count();
        stars > 0 && text.as_bytes().get(stars) == Some(&b' ')
    }

    /// `org-indent-region` over the lines from `start` up to `end`.
    fn region(&mut self, start: usize, end: usize) {
        let count = self.lines.len() - usize::from(self.lines.last().is_some_and(String::is_empty));
        let mut at = start;
        while at < count && is_blank(&self.lines[at]) {
            at += 1;
        }
        while at < end.min(count) {
            let before = at;
            if self.is_heading(at) {
                at += 1;
                continue;
            }
            let Some(found) = self.found(at) else {
                at += 1;
                continue;
            };
            let column = found.column;
            match found.kind {
                // Indented as one block.
                kind if matches!(kind, Kind::ExportBlock | Kind::LatexEnvironment)
                    || (kind == Kind::ExampleBlock && !(found.preserve || self.settings.src_preserve_indentation)) =>
                {
                    let offset = column as isize - width(&self.lines[at]) as isize;
                    if offset != 0 {
                        self.shift(found.begin, found.end, offset);
                    }
                    at = found.end;
                }
                // Indented line by line.
                kind if matches!(kind, Kind::Paragraph | Kind::Table | Kind::TableRow)
                    || !(found.has_contents || matches!(kind, Kind::ExampleBlock | Kind::SrcBlock)) =>
                {
                    if kind == Kind::NodeProperty {
                        self.align(at);
                    }
                    at = self.indent_lines(at, found.end.min(end), column);
                }
                // The lines before the contents, the contents, the lines
                // after them.
                kind => {
                    match kind {
                        Kind::PlainList => {
                            let offset = column as isize - width(&self.lines[at]) as isize;
                            if offset != 0 {
                                self.shift(found.begin, found.end, offset);
                            }
                            at = found.contents_begin;
                        }
                        Kind::Item => at = found.contents_begin,
                        _ => at = self.indent_lines(at, found.contents_begin.min(end), column),
                    }
                    if at < end {
                        match kind {
                            Kind::ExampleBlock | Kind::VerseBlock => {}
                            Kind::SrcBlock => {
                                self.code(at, found.contents_end.min(end), found.post_affiliated, found.preserve)
                            }
                            _ => self.region(at, found.contents_end.min(end)),
                        }
                        at = found.contents_end.min(end);
                        if at < end {
                            at = self.indent_lines(at, found.end.min(end), column);
                        }
                    }
                }
            }
            if at <= before {
                at = before + 1;
            }
        }
    }

    /// The code of a source block as Org writes it back once the
    /// language has indented it: `org-edit-src-content-indentation` past
    /// the block, the code's own indentation kept.
    fn code(&mut self, from: usize, to: usize, block: usize, preserve: bool) {
        if preserve || self.settings.src_preserve_indentation {
            return;
        }
        let to = to.min(self.lines.len());
        let Some(least) =
            (from..to).filter(|line| !is_blank(&self.lines[*line])).map(|line| width(&self.lines[line])).min()
        else {
            return;
        };
        let offset = (width(&self.lines[block]) + self.settings.src_content_indentation) as isize - least as isize;
        self.shift(from, to, offset);
    }

    /// The edits from `original` to the lines now.
    fn edits(&self, original: &str) -> Vec<Edit> {
        let mut edits = Vec::new();
        let mut start = 0;
        for (old, new) in original.split('\n').zip(&self.lines) {
            if old != new {
                let old_blanks = old.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
                let new_blanks = new.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
                edits.push(if old[old_blanks..] == new[new_blanks..] {
                    Edit { start, end: start + old_blanks, text: new[..new_blanks].to_string() }
                } else {
                    Edit { start, end: start + old.len(), text: new.clone() }
                });
            }
            start += old.len() + 1;
        }
        edits
    }
}

/// `org-indent-line`: the column Org gives the line at `at`, or `None`
/// when it leaves the line as it is.
pub fn line_column(text: &str, at: usize, settings: &Settings) -> Option<usize> {
    Snapshot::new(text, settings).line_column(at)
}

/// `org-indent-line` on the line at `at`: the edit that indents it (and
/// aligns a node property), or none when it stays as it is.
pub fn indent_line(text: &str, at: usize, settings: &Settings) -> Option<Edit> {
    let snapshot = Snapshot::new(text, settings);
    let start = snapshot.line_start(at);
    let column = snapshot.line_column(start)?;
    let line = snapshot.line(start);
    let blanks = line.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let property = snapshot.kind(snapshot.element_at(start)) == Some(Kind::NodeProperty);
    let aligned = if property { snapshot.align_property(start) } else { None };
    let indented = indent_line_to(line, column);
    let indentation = &indented[..indented.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count()];
    let edit = match aligned {
        Some(aligned) => Edit { start, end: start + line.len(), text: format!("{indentation}{aligned}") },
        None => Edit { start, end: start + blanks, text: indentation.to_string() },
    };
    (text[edit.start..edit.end] != edit.text).then_some(edit)
}

/// `org-indent-region` over the lines from `start` up to `end`.
pub fn indent_region(text: &str, start: usize, end: usize, settings: &Settings) -> Vec<Edit> {
    region_in_parts(text, start, end, settings, true)
}

/// `indent_region`, the text parsed in parts or, for the tests, whole.
fn region_in_parts(text: &str, start: usize, end: usize, settings: &Settings, parts: bool) -> Vec<Edit> {
    let mut pass = Pass::new(text, settings);
    #[cfg(test)]
    {
        pass.whole = !parts;
    }
    #[cfg(not(test))]
    let _ = parts;
    let first = text[..start.min(text.len())].matches('\n').count();
    let end_at = end.min(text.len());
    let last = text[..end_at].matches('\n').count() + usize::from(end_at > 0 && text.as_bytes()[end_at - 1] != b'\n');
    // The empty line after a final newline is the end of the buffer,
    // where `org-indent-region` stops.
    let lines = pass.lines.len() - usize::from(text.ends_with('\n'));
    let last = if end >= text.len() { lines } else { last.min(lines) };
    pass.region(first, last);
    pass.edits(text)
}

fn is_heading_line(line: &str) -> bool {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    stars > 0 && line.as_bytes().get(stars) == Some(&b' ')
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// `org-footnote-definition-re` at the start of `line`.
fn is_footnote_start(line: &str) -> bool {
    line.strip_prefix("[fn:").is_some_and(|rest| {
        let label = rest.bytes().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')).count();
        label > 0 && rest.as_bytes().get(label) == Some(&b']')
    })
}

/// `org-fixup-indentation`: the lines of `entry`, a headline and its text
/// up to the next headline, after the headline moved `diff` columns of
/// stars (`org-demote` is 1, `org-promote` -1). The planning line and
/// property drawer are indented again for the new level, a `LOGBOOK`
/// drawer right below them moves by `diff`, and with `adapt` `t` so does
/// the rest, unless that would change the document's structure.
pub fn fixup_entry(entry: &str, diff: isize, settings: &Settings) -> String {
    let mut text = entry.to_string();
    let starts = |text: &str| -> Vec<usize> {
        std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).filter(|at| *at < text.len()).collect()
    };
    let line_at = |text: &str, at: usize| -> String { text[at..].split('\n').next().unwrap_or("").to_string() };
    let mut line = 1;
    // The planning line, by `org-indent-line`.
    let lines = starts(&text);
    if let Some(&at) = lines.get(line) {
        let body = line_at(&text, at);
        let trimmed = body.trim_start_matches([' ', '\t']);
        if ["CLOSED:", "DEADLINE:", "SCHEDULED:"].iter().any(|keyword| starts_with_ignore_case(trimmed, keyword)) {
            if let Some(edit) = indent_line(&text, at, settings) {
                text = apply(&text, &[edit]);
            }
            line += 1;
        }
    }
    // The property drawer, by `org-indent-region`.
    let lines = starts(&text);
    if let Some(&at) = lines.get(line) {
        if line_at(&text, at).trim().eq_ignore_ascii_case(":PROPERTIES:") {
            let mut end = line + 1;
            while end < lines.len() {
                let body = line_at(&text, lines[end]);
                let trimmed = body.trim();
                if trimmed.eq_ignore_ascii_case(":END:") {
                    break;
                }
                let is_property = trimmed.len() > 2
                    && trimmed.starts_with(':')
                    && trimmed[1..].split_whitespace().next().is_some_and(|key| key.ends_with(':') && key.len() > 1);
                if !is_property {
                    end = lines.len();
                    break;
                }
                end += 1;
            }
            if end < lines.len() {
                let stop = lines[end] + line_at(&text, lines[end]).len();
                text = apply(&text, &indent_region(&text, at, stop, settings));
                line = end + 1;
            }
        }
    }
    // A `LOGBOOK` drawer: moved by `diff`, a character at a time.
    let lines = starts(&text);
    if let Some(&at) = lines.get(line) {
        if line_at(&text, at).trim_matches([' ', '\t']).eq_ignore_ascii_case(":LOGBOOK:") {
            let end = (line + 1..lines.len())
                .find(|index| line_at(&text, lines[*index]).trim().eq_ignore_ascii_case(":END:"));
            if let Some(end) = end {
                if width(&line_at(&text, at)) as isize + diff >= 0 {
                    let mut out = String::new();
                    let mut previous = 0;
                    for &start in &lines[line..=end] {
                        out.push_str(&text[previous..start]);
                        let body = line_at(&text, start);
                        if diff >= 0 {
                            out.push_str(&" ".repeat(diff as usize));
                            previous = start;
                        } else {
                            let cut: usize = body.chars().take(diff.unsigned_abs()).map(char::len_utf8).sum();
                            previous = start + cut;
                        }
                    }
                    out.push_str(&text[previous..]);
                    text = out;
                }
                line = end + 1;
            }
        }
    }
    if diff == 0 || settings.adapt != Adapt::All {
        return text;
    }
    // The rest, by `diff`: footnote definitions, headlines, blank lines
    // and the code of blocks that keep their indentation stay.
    let snapshot = Snapshot::new(&text, settings);
    let lines = starts(&text);
    let skip_to = |index: usize, kinds: &[Kind]| -> Option<usize> {
        let at = lines[index];
        let element = snapshot.element_at(at)?;
        let found = &snapshot.elements[element];
        (kinds.contains(&found.kind) && found.begin == at).then_some(found.end)
    };
    let preserved_block = |index: usize| -> Option<usize> {
        let body = line_at(&text, lines[index]);
        let trimmed = body.trim_start_matches([' ', '\t']);
        if !(starts_with_ignore_case(trimmed, "#+BEGIN_EXAMPLE") || starts_with_ignore_case(trimmed, "#+BEGIN_SRC")) {
            return None;
        }
        let element = snapshot.element_at(lines[index])?;
        let found = &snapshot.elements[element];
        let preserve = found.preserve || settings.src_preserve_indentation;
        (matches!(found.kind, Kind::SrcBlock | Kind::ExampleBlock) && preserve).then(|| {
            let last = text[..found.end].trim_end_matches([' ', '\r', '\t', '\n']).len();
            snapshot.line_index(last)
        })
    };
    let line_of = |at: usize| lines.iter().position(|start| *start >= at).unwrap_or(lines.len());
    // Moving left must not reach column 0 with text that would become a
    // headline or a footnote definition, nor go past it.
    if diff < 0 {
        let wanted = diff.unsigned_abs();
        let mut index = line;
        while index < lines.len() {
            let body = line_at(&text, lines[index]);
            if is_blank(&body) || is_heading_line(&body) {
                index += 1;
                continue;
            }
            if is_footnote_start(&body) {
                if let Some(end) = skip_to(index, &[Kind::FootnoteDefinition]) {
                    index = line_of(end);
                    continue;
                }
            }
            let column = width(&body);
            let rest = body.trim_start_matches([' ', '\t']);
            if column < wanted || (column == wanted && (is_heading_line(rest) || is_footnote_start(rest))) {
                return text;
            }
            // The code of a block that keeps its indentation is not
            // looked at; its closing line is.
            index = preserved_block(index).filter(|last| *last > index).unwrap_or(index + 1);
        }
    }
    let mut new_lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut index = line;
    while index < lines.len() {
        let body = line_at(&text, lines[index]);
        if is_footnote_start(&body) {
            if let Some(end) = skip_to(index, &[Kind::FootnoteDefinition]) {
                index = line_of(end);
                continue;
            }
        }
        if is_heading_line(&body) || is_blank(&body) {
            index += 1;
            continue;
        }
        new_lines[index] = indent_line_to(&body, (width(&body) as isize + diff).max(0) as usize);
        index = preserved_block(index).filter(|last| *last > index).unwrap_or(index + 1);
    }
    new_lines.join("\n")
}

/// What the `#+STARTUP:` lines of a document say about how it shows:
/// `indent` or `noindent`, `hidestars` or `showstars`. The last word wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Startup {
    pub indent: Option<bool>,
    pub hide_stars: Option<bool>,
}

pub fn startup(text: &str) -> Startup {
    let mut out = Startup::default();
    for word in crate::text::startup_words(text) {
        match word.to_ascii_lowercase().as_str() {
            "indent" => out.indent = Some(true),
            "noindent" => out.indent = Some(false),
            "hidestars" => out.hide_stars = Some(true),
            "showstars" => out.hide_stars = Some(false),
            _ => {}
        }
    }
    out
}

/// How a document shows: `org-indent-mode` and `org-hide-leading-stars`,
/// from the options and the document's `#+STARTUP:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    /// `org-startup-indented`
    pub startup_indented: bool,
    /// `org-hide-leading-stars`
    pub hide_leading_stars: bool,
    /// org-bullets is on: it hides the leading stars whatever the rest says.
    pub bullets: bool,
    /// `org-indent-mode-turns-on-hiding-stars`
    pub indent_hides_stars: bool,
    /// `org-indent-mode-turns-off-org-adapt-indentation`
    pub indent_turns_off_adapt: bool,
    /// `org-indent-indentation-per-level`
    pub per_level: usize,
}

impl Default for Display {
    fn default() -> Self {
        Display {
            startup_indented: false,
            hide_leading_stars: false,
            bullets: false,
            indent_hides_stars: true,
            indent_turns_off_adapt: true,
            per_level: 2,
        }
    }
}

impl Display {
    /// Whether `org-indent-mode` is on in `text`.
    pub fn indented(&self, text: &str) -> bool {
        startup(text).indent.unwrap_or(self.startup_indented)
    }

    /// Whether the leading stars of headlines are hidden in `text`:
    /// `org-indent-mode` hides them (`org-indent-mode-turns-on-hiding-stars`).
    pub fn hides_stars(&self, text: &str) -> bool {
        let startup = startup(text);
        self.bullets
            || (self.indent_hides_stars && startup.indent.unwrap_or(self.startup_indented))
            || startup.hide_stars.unwrap_or(self.hide_leading_stars)
    }

    /// `org-adapt-indentation` in `text`: `org-indent-mode` turns it off,
    /// unless it is `headline-data`
    /// (`org-indent-mode-turns-off-org-adapt-indentation`).
    pub fn adapt(&self, text: &str, adapt: Adapt) -> Adapt {
        if self.indent_turns_off_adapt && adapt != Adapt::HeadlineData && self.indented(text) {
            Adapt::Off
        } else {
            adapt
        }
    }
}

/// How `org-indent-mode` shows a line: `prefix` columns before it
/// (`line-prefix`), `wrap` columns before the lines it wraps into
/// (`wrap-prefix`), and how many of a headline's stars are hidden
/// (`org-hide-leading-stars`: all but the last).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePrefix {
    pub line: usize,
    pub prefix: usize,
    pub wrap: usize,
    pub hidden_stars: usize,
}

/// The spaces `org-list-item-body-column` counts after `bullet`.
fn spaces_after(bullet: &str, two_spaces: Option<&crate::list::BulletRegexp>) -> usize {
    if two_spaces.is_some_and(|regexp| regexp.is_match(bullet)) {
        2
    } else {
        1
    }
}

/// `org-list-item-body-column` of `line` if it is a list item
/// (`org-item-re`), else `None`.
fn item_body_column(line: &str, two_spaces: Option<&crate::list::BulletRegexp>) -> Option<usize> {
    let blanks = line.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let rest = &line[blanks..];
    let bullet = match rest.bytes().next()? {
        b'-' | b'+' => 1,
        // A star at the start of the line is a headline's.
        b'*' if blanks > 0 => 1,
        b'0'..=b'9' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    if !matches!(rest.as_bytes().get(bullet), None | Some(b' ' | b'\t')) {
        return None;
    }
    Some(width(line) + bullet + spaces_after(&rest[..bullet], two_spaces))
}

/// `org-indent-add-properties`: how each line of `text` shows with
/// `org-indent-mode` (`indent`) and with hidden leading stars
/// (`hide_stars`). `adapt` `headline-data` lines up the planning lines,
/// drawers and clock lines of an entry with its headline's stars. Lines
/// shown as they are, are left out. `two_spaces` is
/// `org-list-two-spaces-after-bullet-regexp`, `per_level`
/// `org-indent-indentation-per-level`.
pub fn virtual_indentation(
    text: &str,
    indent: bool,
    hide_stars: bool,
    adapt: Adapt,
    two_spaces: Option<&crate::list::BulletRegexp>,
    per_level: usize,
) -> Vec<LinePrefix> {
    // `org-indent--compute-prefixes`: a headline of level `n` has
    // `(per_level - 1) * (n - 1)` columns before its stars, and the text
    // below it as many plus `n` and the boundary character, so that with
    // 2 per level its title and its text start at `2n`. With 0 there are
    // no prefixes.
    let indentation = |level: usize| if level <= 1 { 0 } else { (per_level.saturating_sub(1)) * (level - 1) };
    let heading_prefix = |level: usize| if per_level == 0 { 0 } else { indentation(level) };
    let text_prefix = |level: usize| {
        if per_level == 0 || level == 0 {
            0
        } else {
            level + indentation(level) + 1
        }
    };
    let mut out = Vec::new();
    let mut level = 0;
    // Planning lines (and the blank lines after them, which belong to
    // them) and clock lines are found by their element, as Org does.
    let snapshot = (indent && adapt == Adapt::HeadlineData)
        .then(|| Snapshot::new(text, &Settings { adapt, ..Settings::default() }));
    let element_is = |at: usize, kinds: &[Kind]| {
        snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.element_at(at).is_some_and(|index| kinds.contains(&snapshot.elements[index].kind))
        })
    };
    // The empty line after a final line break is no line.
    let lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(text).split('\n').collect();
    let mut start = 0;
    for (number, line) in lines.into_iter().enumerate() {
        let at = start;
        start += line.len() + 1;
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        let heading = stars > 0 && line.as_bytes().get(stars) == Some(&b' ');
        let (prefix, wrap, hidden) = if heading {
            level = stars;
            let prefix = heading_prefix(stars);
            (prefix, prefix + stars + 1, if hide_stars { stars - 1 } else { 0 })
        } else if let Some(body) = item_body_column(line, two_spaces) {
            (text_prefix(level), text_prefix(level) + body, 0)
        } else {
            let trimmed = line.trim_start_matches([' ', '\t']);
            let property = trimmed.trim_end().eq_ignore_ascii_case(":PROPERTIES:")
                || trimmed.trim_end().eq_ignore_ascii_case(":END:")
                || (trimmed.starts_with(':')
                    && trimmed[1..].split_whitespace().next().is_some_and(|key| key.len() > 1 && key.ends_with(':')));
            let data = adapt == Adapt::HeadlineData
                && indent
                && (property
                    || element_is(at, &[Kind::Planning])
                    || (starts_with_ignore_case(trimmed, "CLOCK:") && element_is(at, &[Kind::Clock])));
            if data {
                let prefix = heading_prefix(level);
                (prefix, prefix + level + 1, 0)
            } else {
                let prefix = text_prefix(level);
                (prefix, prefix + width(line), 0)
            }
        };
        let (prefix, wrap) = if indent { (prefix, wrap) } else { (0, 0) };
        if prefix > 0 || wrap > 0 || hidden > 0 {
            out.push(LinePrefix { line: number, prefix, wrap, hidden_stars: hidden });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With `org-list-two-spaces-after-bullet-regexp`, the text of an item
    /// whose bullet it matches lines up two columns after the bullet, as
    /// `org-indent-line` puts it in Emacs; so does `org-indent-mode`.
    #[test]
    fn items_with_two_spaces_after_the_bullet() {
        let settings = Settings { two_spaces: crate::list::BulletRegexp::new("[0-9]"), ..Settings::default() };
        let text = "1. a\n b\n- c\n d\n";
        let edits = indent_region(text, 0, text.len(), &settings);
        assert_eq!(apply(text, &edits), "1. a\n    b\n- c\n  d\n");
        let wraps: Vec<usize> = virtual_indentation(text, true, false, Adapt::Off, settings.two_spaces.as_ref(), 2)
            .into_iter()
            .filter(|line| line.line == 0 || line.line == 2)
            .map(|line| line.wrap)
            .collect();
        assert_eq!(wraps, [4, 2]);
    }

    /// `org-property-format`: a property line indented is written as the
    /// format makes it, trimmed, as `org-indent-line` writes it in Emacs.
    #[test]
    fn writes_properties_as_org_property_format_says() {
        let cases = [
            ("%-10s %s", ":FOO:", ":FOO:"),
            ("%-10s %s", ":FOO: bar", ":FOO:      bar"),
            ("%-10s %s", ":LONGPROPERTYNAME: x  y ", ":LONGPROPERTYNAME: x  y"),
            ("%-10s %s", "   :A+:   v", ":A+:       v"),
            ("%s %s", "   :A+:   v", ":A+: v"),
            ("%-15s %s", ":FOO: bar", ":FOO:           bar"),
            ("%15s %s", ":FOO: bar", ":FOO: bar"),
            ("%s: %s", ":FOO:", ":FOO::"),
            ("%s: %s", ":FOO: bar", ":FOO:: bar"),
            ("%.3s %s", ":FOO:", ":FO"),
            ("%.3s %s", ":LONGPROPERTYNAME: x  y ", ":LO x  y"),
            ("%-5s|%s", ":FOO:", ":FOO:|"),
            ("%-5s|%s", "   :A+:   v", ":A+: |v"),
        ];
        for (format, line, wanted) in cases {
            let text = format!("* h\n:PROPERTIES:\n{line}\n:END:\n");
            let settings = Settings { property_format: format.to_string(), ..Settings::default() };
            let at = text.find(line).unwrap();
            let edit = indent_line(&text, at, &settings);
            let got = apply(&text, &edit.into_iter().collect::<Vec<_>>());
            assert_eq!(got.split('\n').nth(2).unwrap(), wanted, "{format:?} {line:?}");
        }
    }

    /// `org-indent-mode-turns-on-hiding-stars` and
    /// `org-indent-mode-turns-off-org-adapt-indentation` off: a file shown
    /// indented keeps its stars and `org-adapt-indentation`.
    #[test]
    fn indent_mode_leaves_stars_and_adaptation_when_told() {
        let indented = "#+STARTUP: indent\n* a\n";
        let display = Display::default();
        assert!(display.hides_stars(indented));
        assert_eq!(display.adapt(indented, Adapt::All), Adapt::Off);
        let display = Display { indent_hides_stars: false, indent_turns_off_adapt: false, ..Display::default() };
        assert!(!display.hides_stars(indented));
        assert_eq!(display.adapt(indented, Adapt::All), Adapt::All);
        let display = Display { hide_leading_stars: true, ..display };
        assert!(display.hides_stars(indented));
    }

    /// Parsing each part on its own gives what parsing the whole text
    /// gives, for every Org file of the tests.
    #[test]
    fn indents_in_parts_as_in_the_whole() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests");
        let mut files = Vec::new();
        let mut directories = vec![root];
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                } else if path.extension().is_some_and(|extension| extension == "org") {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 20, "{} files", files.len());
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for adapt in [Adapt::Off, Adapt::All, Adapt::HeadlineData] {
                let settings = Settings { adapt, ..Settings::default() };
                let parts = region_in_parts(&text, 0, text.len(), &settings, true);
                let whole = region_in_parts(&text, 0, text.len(), &settings, false);
                assert_eq!(parts, whole, "{} with {adapt:?}", path.display());
            }
        }
    }
}
