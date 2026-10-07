//! Citations, as Org's `oc.el` and its `basic` processor (`oc-basic.el`)
//! read them: the bibliography files of a document (`#+BIBLIOGRAPHY:`
//! and `org-cite-global-bibliography`), their entries (BibTeX, as
//! `bibtex.el` parses it with the `biblatex` dialect, or CSL-JSON), the
//! keys of a citation, an entry as the help echo shows it, its line in
//! the completion table, where following a key goes, and the keys close
//! to an unknown one.

use std::collections::HashMap;

/// The value of a field: text, or something else in a CSL-JSON file,
/// written as Lisp would print it, which Org refuses to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Other(String),
}

/// An entry: its key and its fields, as Org lists them (`type` and `id`
/// among them), field names in lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub fields: Vec<(String, Value)>,
}

impl Entry {
    /// `org-cite-basic--get-field`: `Ok(None)` when the entry has no such
    /// field, an error when its value is not text.
    pub fn field(&self, name: &str) -> Result<Option<&str>, String> {
        match self.fields.iter().find(|(field, _)| field == name).map(|(_, value)| value) {
            None => Ok(None),
            Some(Value::Text(text)) => Ok(Some(text)),
            Some(Value::Other(other)) => Err(format!("Non-string bibliography field value: {other}")),
        }
    }

    /// `org-cite-basic--get-author`: the author, or else the editor.
    fn author(&self) -> Result<Option<&str>, String> {
        Ok(match self.field("author")? {
            Some(author) => Some(author),
            None => self.field("editor")?,
        })
    }

    /// `org-cite-basic--get-year` without a suffix: the year, or the
    /// year a date starts with.
    fn year(&self) -> Result<Option<String>, String> {
        if let Some(year) = self.field("year")? {
            return Ok(Some(year.to_string()));
        }
        Ok(self.field("date")?.and_then(|date| {
            let digits: String = date.chars().take(4).collect();
            let after = date.chars().nth(4);
            (digits.chars().count() == 4
                && digits.chars().all(|c| c.is_ascii_digit())
                && after.is_none_or(|c| !c.is_ascii_digit()))
            .then_some(digits)
        }))
    }

    /// `org-cite-basic--print-entry` with the `plain` style, as the help
    /// echo of a key shows it: `Doe, Roe. Title, Publisher, 2020.`
    pub fn plain(&self) -> Result<String, String> {
        let author = self.author()?;
        let title = self.field("title")?;
        let mut from = None;
        for name in ["publisher", "journal", "institution", "school"] {
            if let Some(value) = self.field(name)? {
                from = Some(value);
                break;
            }
        }
        let year = self.year()?;
        let mut out = String::new();
        if let Some(author) = author {
            out.push_str(&shorten_names(author));
        }
        out.push_str(". ");
        out.push_str(title.unwrap_or(""));
        if let Some(from) = from {
            out.push_str(", ");
            out.push_str(from);
        }
        out.push_str(", ");
        out.push_str(year.as_deref().unwrap_or(""));
        out.push('.');
        Ok(out)
    }

    /// The entry's line in `org-cite-basic--key-completion-table`: the
    /// authors in 25 columns, the year in 4, the title.
    pub fn completion(&self) -> Result<String, String> {
        let author = match self.author()? {
            Some(author) => pad_to_width(&author.replace(" and ", "; "), 25),
            None => " ".repeat(25),
        };
        let year = self.year()?.unwrap_or_default();
        let title = self.field("title")?.unwrap_or("");
        Ok(format!("{author}  {year:>4}  {title}"))
    }
}

/// `truncate-string-to-width` with padding: `text` cut at `width`
/// columns, and padded with spaces to them.
fn pad_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = crate::text::char_width(c);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(&" ".repeat(width - used));
    out
}

/// `org-cite-basic--shorten-names`: the family names of `names` (`and`
/// between them), a name written `Family, Given` cut at its comma.
fn shorten_names(names: &str) -> String {
    names
        .split(" and ")
        .map(|name| if name.chars().count() == 1 { "" } else { name.split(", ").next().unwrap_or("") })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `org-string-distance`: the Levenshtein distance between two strings,
/// in characters.
pub fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// `org-cite-basic--close-keys`: the keys at most two edits away from
/// `key`.
pub fn close_keys<'a>(key: &str, keys: &'a [String]) -> Vec<&'a String> {
    keys.iter().filter(|known| distance(known, key) <= 2).collect()
}

/// A citation key in a document: where its `@` starts and where it ends,
/// and the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub start: usize,
    pub end: usize,
    pub key: String,
    /// The part of the citation the reference is, between semicolons
    /// (its prefix and suffix with it).
    pub part: (usize, usize),
}

/// Whether `c` may be in a key (`org-element-citation-key-re`).
fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || "-.:?!`'/*@+|(){}<>&_^$#%~".contains(c)
}

/// The references of the citation `text` (`[cite:…]`, starting at byte
/// `offset` of the document): the first key of each part between
/// semicolons.
pub fn references(text: &str, offset: usize) -> Vec<Reference> {
    let Some(colon) = text.find(':') else { return Vec::new() };
    let body_start = colon + 1;
    let body_end = text.len().saturating_sub(1).max(body_start);
    let mut out = Vec::new();
    let mut part_start = body_start;
    for part in text[body_start..body_end].split(';') {
        if let Some(at) = part.char_indices().find_map(|(index, c)| {
            (c == '@' && part[index + 1..].chars().next().is_some_and(is_key_char)).then_some(index)
        }) {
            let key_len: usize = part[at + 1..].chars().take_while(|c| is_key_char(*c)).map(char::len_utf8).sum();
            out.push(Reference {
                start: offset + part_start + at,
                end: offset + part_start + at + 1 + key_len,
                key: part[at + 1..at + 1 + key_len].to_string(),
                part: (offset + part_start, offset + part_start + part.len()),
            });
        }
        part_start += part.len() + 1;
    }
    out
}

/// The citation around byte `offset` of a document: where it starts and
/// its text.
pub fn citation_at(parse: &org_syntax::Parse, offset: usize) -> Option<(usize, String)> {
    let root = parse.syntax();
    let offset = org_syntax::TextSize::try_from(offset).ok()?;
    let token = match root.token_at_offset(offset) {
        org_syntax::TokenAtOffset::None => return None,
        org_syntax::TokenAtOffset::Single(token) => token,
        // Between two tokens, the one on the left when it is the end of a
        // citation, so that the end of a key counts.
        org_syntax::TokenAtOffset::Between(left, right) => {
            if left.parent_ancestors().any(|node| node.kind() == org_syntax::SyntaxKind::CITATION) {
                left
            } else {
                right
            }
        }
    };
    let citation = token.parent_ancestors().find(|node| node.kind() == org_syntax::SyntaxKind::CITATION)?;
    Some((usize::from(citation.text_range().start()), citation.text().to_string()))
}

/// The reference of a citation at byte `offset`: the one whose key the
/// offset is on or at the end of.
pub fn key_at(references: &[Reference], offset: usize) -> Option<&Reference> {
    references.iter().find(|reference| reference.start <= offset && offset <= reference.end)
}

/// The bibliography files `org-cite-list-bibliography-files` lists for a
/// document: the values of its `#+BIBLIOGRAPHY:` keywords, without
/// quotes around them, then `globals`; each once.
pub fn bibliography_files(keywords: &[String], globals: &[String]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for file in keywords.iter().map(|value| strip_quotes(value.trim())).chain(globals.iter().cloned()) {
        if !files.contains(&file) {
            files.push(file);
        }
    }
    files
}

/// The values of the `#+BIBLIOGRAPHY:` keywords of a document.
pub fn bibliography_keywords(text: &str, keywords: &org_syntax::TodoKeywords) -> Vec<String> {
    let parse = org_syntax::parse(text, keywords);
    crate::outline::keyword_values(&parse.syntax(), "BIBLIOGRAPHY")
}

/// `org-strip-quotes`
fn strip_quotes(text: &str) -> String {
    match text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) {
        Some(inner) => inner.to_string(),
        None => text.to_string(),
    }
}

/// The entries of a bibliography file, by its name (`.json`, `.bib`, or
/// `.bibtex`) and its text, as `org-cite-basic--parse-bibliography`
/// reads it; an error when Org cannot, and then the file has none.
pub fn parse_file(name: &str, text: &str) -> Result<Vec<Entry>, String> {
    let text = text.replace("\r\n", "\n");
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("json") => parse_json(&text),
        Some("bib") => parse_bibtex(&text, true),
        Some("bibtex") => parse_bibtex(&text, false),
        other => Err(format!("Unknown bibliography extension: \"{}\"", other.unwrap_or(""))),
    }
}

/// Where following `key` in the bibliography file `text` goes
/// (`org-cite-basic-goto`): the start of the line of its entry, or in a
/// CSL-JSON file the brace that opens it; `None` when Org finds no entry
/// (it then shows the file as it is).
pub fn entry_position(name: &str, text: &str, key: &str) -> Option<usize> {
    if name.ends_with(".json") {
        let mut from = 0;
        while let Some(at) = text[from..].find("\"id\":").map(|at| from + at) {
            let rest = text[at + 5..].trim_start_matches([' ', '\t']);
            // Case folded, as Emacs searches.
            let found = rest.strip_prefix('"').is_some_and(|rest| {
                rest.get(..key.len()).is_some_and(|found| found.to_lowercase() == key.to_lowercase())
                    && rest[key.len()..].starts_with('"')
            });
            if found {
                return text[..at].rfind('{');
            }
            from = at + 5;
        }
        return None;
    }
    // `bibtex-search-entry`, in the dialect a `.bib` file opens with.
    let bib = Bib { text, types: &BIBTEX_TYPES };
    let mut line = 0;
    loop {
        if let Some(head) = bib.head(line) {
            let rest = &text[head.key_start..];
            if rest.len() >= key.len()
                && rest.is_char_boundary(key.len())
                && rest[..key.len()].to_lowercase() == key.to_lowercase()
                && rest[key.len()..].trim_start_matches([' ', '\t', '\n']).starts_with([',', '='])
            {
                return Some(line);
            }
        }
        line = text[line..].find('\n').map(|at| line + at + 1)?;
    }
}

// ----- BibTeX

/// The entry types of the `biblatex` dialect (`bibtex-biblatex-entry-alist`).
const TYPES: [&str; 32] = [
    "Article",
    "Book",
    "MVBook",
    "InBook",
    "BookInBook",
    "SuppBook",
    "Booklet",
    "Collection",
    "MVCollection",
    "InCollection",
    "SuppCollection",
    "Dataset",
    "Manual",
    "Misc",
    "Online",
    "Patent",
    "Periodical",
    "SuppPeriodical",
    "Proceedings",
    "MVProceedings",
    "InProceedings",
    "Conference",
    "Reference",
    "MVReference",
    "InReference",
    "Report",
    "Software",
    "Thesis",
    "PhdThesis",
    "MastersThesis",
    "TechReport",
    "Unpublished",
];

/// The entry types of the `BibTeX` dialect (`bibtex-BibTeX-entry-alist`),
/// which `.bibtex` files are read with and keys are looked up with.
const BIBTEX_TYPES: [&str; 14] = [
    "Article",
    "InProceedings",
    "Conference",
    "InCollection",
    "InBook",
    "Proceedings",
    "Book",
    "Booklet",
    "PhdThesis",
    "MastersThesis",
    "TechReport",
    "Manual",
    "Unpublished",
    "Misc",
];

/// `bibtex-predefined-strings`
const PREDEFINED: [(&str, &str); 32] = [
    ("jan", "January"),
    ("feb", "February"),
    ("mar", "March"),
    ("apr", "April"),
    ("may", "May"),
    ("jun", "June"),
    ("jul", "July"),
    ("aug", "August"),
    ("sep", "September"),
    ("oct", "October"),
    ("nov", "November"),
    ("dec", "December"),
    ("acmcs", "ACM Computing Surveys"),
    ("acta", "Acta Informatica"),
    ("cacm", "Communications of the ACM"),
    ("ibmjrd", "IBM Journal of Research and Development"),
    ("ibmsj", "IBM Systems Journal"),
    ("ieeese", "IEEE Transactions on Software Engineering"),
    ("ieeetc", "IEEE Transactions on Computers"),
    ("ieeetcad", "IEEE Transactions on Computer-Aided Design of Integrated Circuits"),
    ("ipl", "Information Processing Letters"),
    ("jacm", "Journal of the ACM"),
    ("jcss", "Journal of Computer and System Sciences"),
    ("scp", "Science of Computer Programming"),
    ("sicomp", "SIAM Journal on Computing"),
    ("tcs", "Theoretical Computer Science"),
    ("tocs", "ACM Transactions on Computer Systems"),
    ("tods", "ACM Transactions on Database Systems"),
    ("tog", "ACM Transactions on Graphics"),
    ("toms", "ACM Transactions on Mathematical Software"),
    ("toois", "ACM Transactions on Office Information Systems"),
    ("toplas", "ACM Transactions on Programming Languages and Systems"),
];

/// `bibtex-reference-key`
fn is_reference_char(c: char) -> bool {
    c.is_alphanumeric() || "[].:;?!`'/*@+|()<>&_^$-".contains(c)
}

/// `bibtex-field-const`
fn is_const_char(c: char) -> bool {
    c.is_alphanumeric() || "[].:;?!`'/*@+=|<>&_^$-".contains(c)
}

/// `bibtex-field-name`
fn is_name_char(c: char) -> bool {
    !"\"#%'(),={} \t\n".contains(c)
}

fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n')
}

/// The head of an entry: `@Type{key`.
struct Head {
    /// The type as written, after the `@` (`bibtex-type-in-head`).
    kind: String,
    key: Option<String>,
    key_start: usize,
    /// The entry opens with a parenthesis.
    paren: bool,
    /// The end of the head.
    end: usize,
}

/// A field: its name and the bounds of its text.
struct Field {
    name: String,
    text_start: usize,
    text_end: usize,
    /// The end of the field, past the blanks after it.
    end: usize,
}

struct Bib<'a> {
    text: &'a str,
    /// The entry types of the dialect.
    types: &'a [&'a str],
}

impl Bib<'_> {
    fn skip(&self, at: usize, what: impl Fn(char) -> bool) -> usize {
        at + self.text[at..].chars().take_while(|c| what(*c)).map(char::len_utf8).sum::<usize>()
    }

    /// `bibtex-entry-maybe-empty-head` at the line starting at `line`,
    /// for an entry of a known type.
    fn head(&self, line: usize) -> Option<Head> {
        let text = self.text;
        let at = self.skip(line, |c| c == ' ' || c == '\t');
        text[at..].strip_prefix('@')?;
        let type_start = at + 1;
        let name_start = self.skip(type_start, |c| c == ' ' || c == '\t');
        // The type that has the opening delimiter after it.
        let (name_end, paren) = self.types.iter().find_map(|kind| {
            let end = name_start + kind.len();
            let candidate = text.get(name_start..end)?;
            if !candidate.eq_ignore_ascii_case(kind) {
                return None;
            }
            let delimiter = self.skip(end, |c| c == ' ' || c == '\t');
            match text[delimiter..].chars().next()? {
                '{' => Some((end, false)),
                '(' => Some((end, true)),
                _ => None,
            }
        })?;
        let open = self.skip(name_end, |c| c == ' ' || c == '\t');
        let key_start = self.skip(open + 1, is_blank);
        let key_end = self.skip(key_start, is_reference_char);
        Some(Head {
            kind: text[type_start..name_end].to_string(),
            key: (key_end > key_start).then(|| text[key_start..key_end].to_string()),
            key_start,
            paren,
            end: key_end,
        })
    }

    /// `bibtex-parse-field-string` at `at`: the end of a string in braces
    /// or quotes; an error when its braces do not balance.
    fn string_end(&self, at: usize) -> Result<Option<usize>, String> {
        let text = self.text;
        match text[at..].chars().next() {
            Some('"') => {
                let mut escaped = false;
                for (index, c) in text[at + 1..].char_indices() {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        return Ok(Some(at + 1 + index + 1));
                    }
                }
                Err("Unbalanced parentheses".to_string())
            }
            Some('{') => {
                let mut depth = 0;
                for (index, c) in text[at..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Ok(Some(at + index + 1));
                            }
                        }
                        _ => {}
                    }
                }
                Err("Unbalanced parentheses".to_string())
            }
            _ => Ok(None),
        }
    }

    /// `bibtex-parse-field-text`: the end of a field's text (constants and
    /// strings joined with `#`), and the end past the blanks after it.
    fn field_text(&self, at: usize) -> Result<Option<(usize, usize)>, String> {
        let mut at = at;
        loop {
            let after_const = self.skip(at, is_const_char);
            if after_const > at {
                at = after_const;
            } else if let Some(end) = self.string_end(at)? {
                at = end;
            } else {
                return Ok(None);
            }
            let blanks = self.skip(at, is_blank);
            if self.text[blanks..].starts_with('#') {
                at = self.skip(blanks + 1, is_blank);
            } else {
                return Ok(Some((at, blanks)));
            }
        }
    }

    /// `bibtex-parse-field` at `at`: `, name = text`.
    fn field(&self, at: usize) -> Result<Option<Field>, String> {
        let text = self.text;
        if !text[at..].starts_with(',') {
            return Ok(None);
        }
        let name_start = self.skip(at + 1, is_blank);
        match text[name_start..].chars().next() {
            Some(c) if is_name_char(c) && !c.is_ascii_digit() => {}
            _ => return Ok(None),
        }
        let name_end = self.skip(name_start, is_name_char);
        let equal = self.skip(name_end, is_blank);
        if !text[equal..].starts_with('=') {
            return Ok(None);
        }
        let text_start = self.skip(equal + 1, is_blank);
        Ok(self.field_text(text_start)?.map(|(text_end, end)| Field {
            name: text[name_start..name_end].to_string(),
            text_start,
            text_end,
            end,
        }))
    }

    /// `bibtex-valid-entry` at the line starting at `line`: the end of
    /// the entry, which must have a key.
    fn valid_entry(&self, line: usize) -> Result<Option<usize>, String> {
        let Some(head) = self.head(line).filter(|head| head.key.is_some()) else { return Ok(None) };
        let mut at = self.skip(head.end, is_blank);
        while let Some(field) = self.field(at)? {
            at = field.end;
        }
        let text = self.text;
        let after_comma = if text[at..].starts_with(',') { at + 1 } else { at };
        let close = self.skip(after_comma, is_blank);
        let closer = if head.paren { ')' } else { '}' };
        Ok(text[close..].starts_with(closer).then_some(close + 1))
    }

    /// `bibtex-parse-string` (or with `preamble` `bibtex-parse-preamble`)
    /// at the line starting at `line`: the name, the bounds of the text
    /// and the end of the entry.
    fn string_entry(&self, line: usize, preamble: bool) -> Result<Option<(String, usize, usize, usize)>, String> {
        let text = self.text;
        let at = self.skip(line, |c| c == ' ' || c == '\t');
        let Some(rest) = text[at..].strip_prefix('@') else { return Ok(None) };
        let word = if preamble { "Preamble" } else { "String" };
        let name_start = at + 1 + (rest.len() - rest.trim_start_matches([' ', '\t']).len());
        if !text.get(name_start..name_start + word.len()).is_some_and(|found| found.eq_ignore_ascii_case(word)) {
            return Ok(None);
        }
        let open = self.skip(name_start + word.len(), |c| c == ' ' || c == '\t');
        if !text[open..].starts_with(['{', '(']) {
            return Ok(None);
        }
        let mut at = self.skip(open + 1, is_blank);
        let mut name = String::new();
        if !preamble {
            let key_end = self.skip(at, is_reference_char);
            if key_end == at {
                return Ok(None);
            }
            name = text[at..key_end].to_string();
            let equal = self.skip(key_end, is_blank);
            if !text[equal..].starts_with('=') {
                return Ok(None);
            }
            at = self.skip(equal + 1, is_blank);
        }
        let Some((text_end, _)) = self.field_text(at)? else { return Ok(None) };
        let close = self.skip(text_end, is_blank);
        Ok(text[close..].starts_with(['}', ')']).then_some((name, at, text_end, close + 1)))
    }

    /// The start of each line that starts with `@`, after blanks.
    fn at_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        let mut line = 0;
        loop {
            let at = self.skip(line, |c| c == ' ' || c == '\t');
            if self.text[at..].starts_with('@') {
                lines.push(line);
            }
            match self.text[line..].find('\n') {
                Some(end) => line += end + 1,
                None => return lines,
            }
        }
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |index| index + 1)
    }

    /// `bibtex-validate`: whether every entry of a known type is well
    /// formed and no key comes twice.
    fn validate(&self) -> Result<bool, String> {
        let text = self.text;
        let mut point = 0;
        for line in self.at_lines() {
            if line < point && self.line_start(point) != line {
                continue;
            }
            if line < point {
                continue;
            }
            if let Some(end) = self.valid_entry(line)? {
                point = end;
            } else if let Some((.., end)) = self.string_entry(line, false)? {
                point = end;
            } else if let Some((.., end)) = self.string_entry(line, true)? {
                point = end;
            } else {
                // `bibtex-any-valid-entry-type`: a known type, `String`
                // or `Preamble`, whatever follows.
                let at = self.skip(line, |c| c == ' ' || c == '\t') + 1;
                let at = self.skip(at, |c| c == ' ' || c == '\t');
                let known =
                    self.types.iter().chain(&["String", "Preamble"]).any(|kind| {
                        text.get(at..at + kind.len()).is_some_and(|found| found.eq_ignore_ascii_case(kind))
                    });
                if known {
                    return Ok(false);
                }
                point = line + 1;
            }
        }
        let mut keys: Vec<String> = Vec::new();
        for start in self.entries()? {
            let key = self.head(start).and_then(|head| head.key).unwrap_or_default();
            if keys.contains(&key) {
                return Ok(false);
            }
            keys.push(key);
        }
        Ok(true)
    }

    /// `bibtex-map-entries`: the start of each valid entry.
    fn entries(&self) -> Result<Vec<usize>, String> {
        let text = self.text;
        let mut starts = Vec::new();
        let mut pos = 0;
        loop {
            // `bibtex-skip-to-valid-entry`
            let mut line = self.line_start(pos);
            let found = loop {
                if line >= text.len() {
                    break None;
                }
                if let Some(end) = self.valid_entry(line)? {
                    break Some((line, end));
                }
                match text[line..].find('\n') {
                    Some(newline) => {
                        let next = line + newline + 1;
                        // To the next line that starts with `@`.
                        match self.at_lines().into_iter().find(|start| *start >= next) {
                            Some(start) => line = start,
                            None => break None,
                        }
                    }
                    None => break None,
                }
            };
            let Some((start, end)) = found else { return Ok(starts) };
            if starts.last() == Some(&start) {
                pos = text[start..].find('\n').map_or(text.len(), |at| start + at + 1);
                if pos >= text.len() {
                    return Ok(starts);
                }
                continue;
            }
            starts.push(start);
            pos = end;
        }
    }

    /// The content of a field's text: its strings without their
    /// delimiters, its constants expanded, joined.
    fn content(&self, start: usize, end: usize, strings: &mut Strings) -> Result<String, String> {
        let text = self.text;
        let mut out = String::new();
        let mut at = start;
        while at < end {
            let after_const = self.skip(at, is_const_char).min(end);
            if after_const > at {
                let name = &text[at..after_const];
                out.push_str(&strings.expand(name)?.unwrap_or_else(|| name.to_string()));
                at = after_const;
            } else if let Some(string_end) = self.string_end(at)? {
                out.push_str(&text[at + 1..string_end - 1]);
                at = string_end;
            } else {
                break;
            }
            let blanks = self.skip(at, is_blank);
            if text[blanks..].starts_with('#') {
                at = self.skip(blanks + 1, is_blank);
            }
        }
        Ok(out)
    }
}

/// `bibtex-strings`, made when a constant is first expanded: the
/// predefined strings, then the file's `@String`s, the first of a name
/// counting. Only a `.bib` file can give its strings (`bibtex-string-files`
/// names files by that extension).
struct Strings<'a> {
    text: &'a str,
    bib: bool,
    table: Option<Vec<(String, String)>>,
}

impl Strings<'_> {
    fn expand(&mut self, name: &str) -> Result<Option<String>, String> {
        if self.table.is_none() {
            if !self.bib {
                return Err("BibTeX strings file not found".to_string());
            }
            let bib = Bib { text: self.text, types: &TYPES };
            let mut table: Vec<(String, String)> =
                PREDEFINED.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
            for line in bib.at_lines() {
                if let Some((name, start, end, _)) = bib.string_entry(line, false)? {
                    // A constant in a string definition sends Emacs round
                    // in circles, until it gives up.
                    let mut at = start;
                    let mut value = String::new();
                    while at < end {
                        if bib.skip(at, is_const_char) > at {
                            return Err("Lisp nesting exceeds `max-lisp-eval-depth'".to_string());
                        }
                        let Some(string_end) = bib.string_end(at)? else { break };
                        value.push_str(&self.text[at + 1..string_end - 1]);
                        at = bib.skip(string_end, is_blank);
                        if self.text[at..].starts_with('#') {
                            at = bib.skip(at + 1, is_blank);
                        }
                    }
                    if !table.iter().any(|(known, _)| known.eq_ignore_ascii_case(&name)) {
                        table.push((name, value));
                    }
                }
            }
            self.table = Some(table);
        }
        let table = self.table.as_ref().expect("the strings");
        Ok(table.iter().find(|(known, _)| known.to_lowercase() == name.to_lowercase()).map(|(_, value)| value.clone()))
    }
}

/// `org-cite-basic--parse-bibtex`: the entries of a BibTeX file, `bib`
/// for a `.bib` file, read with the `biblatex` dialect (and whose
/// `@String`s can be expanded); a `.bibtex` file with the `BibTeX` one.
pub fn parse_bibtex(text: &str, bib: bool) -> Result<Vec<Entry>, String> {
    let parser = Bib { text, types: if bib { &TYPES } else { &BIBTEX_TYPES } };
    if !parser.validate()? {
        return Err("Malformed bibliography".to_string());
    }
    let mut strings = Strings { text, bib, table: None };
    let mut entries: Vec<Entry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for start in parser.entries()? {
        let Some(head) = parser.head(start) else { continue };
        let key = head.key.clone().unwrap_or_default();
        let mut fields =
            vec![("type".to_string(), Value::Text(head.kind.clone())), ("id".to_string(), Value::Text(key.clone()))];
        let mut at = parser.skip(head.end, is_blank);
        while let Some(field) = parser.field(at)? {
            let mut name = field.name.clone();
            let lower = name.to_lowercase();
            if (lower.starts_with("opt") || lower.starts_with("alt")) && !lower.starts_with("option") {
                name = name[3..].to_string();
            }
            let value = parser.content(field.text_start, field.text_end, &mut strings)?;
            fields.push((name.to_lowercase(), Value::Text(collapse_blanks(&value))));
            at = field.end;
        }
        let entry = Entry { key: key.clone(), fields };
        match index.get(&key) {
            Some(at) => entries[*at] = entry,
            None => {
                index.insert(key, entries.len());
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

/// Runs of blanks as one space, as Org reads a BibTeX value.
fn collapse_blanks(text: &str) -> String {
    let mut out = String::new();
    let mut blank = false;
    for c in text.chars() {
        if is_blank(c) {
            if !blank {
                out.push(' ');
            }
            blank = true;
        } else {
            out.push(c);
            blank = false;
        }
    }
    out
}

// ----- CSL-JSON

/// A JSON value with the order of its object's members kept.
#[derive(Debug, Clone)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(key, _)| key == name).map(|(_, value)| value),
            _ => None,
        }
    }

    /// The value as Lisp prints it, for an error.
    fn lisp(&self) -> String {
        match self {
            Json::Null | Json::Bool(false) => ":json-false".to_string(),
            Json::Bool(true) => "t".to_string(),
            Json::Number(number) => number.clone(),
            Json::Text(text) => format!("{text:?}"),
            Json::Array(items) => format!("({})", items.iter().map(Json::lisp).collect::<Vec<_>>().join(" ")),
            Json::Object(members) => format!(
                "({})",
                members.iter().map(|(key, value)| format!("({key} . {})", value.lisp())).collect::<Vec<_>>().join(" ")
            ),
        }
    }
}

struct JsonReader<'a> {
    text: &'a [u8],
    at: usize,
}

impl JsonReader<'_> {
    fn blank(&mut self) {
        while self.at < self.text.len() && matches!(self.text[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.blank();
        let error = || "JSON readtable error".to_string();
        match *self.text.get(self.at).ok_or_else(error)? {
            b'{' => {
                self.at += 1;
                let mut members = Vec::new();
                self.blank();
                if self.text.get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                loop {
                    self.blank();
                    let Json::Text(key) = self.value()? else { return Err(error()) };
                    self.blank();
                    if self.text.get(self.at) != Some(&b':') {
                        return Err(error());
                    }
                    self.at += 1;
                    let value = self.value()?;
                    members.push((key, value));
                    self.blank();
                    match self.text.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Json::Object(members));
                        }
                        _ => return Err(error()),
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.blank();
                if self.text.get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.blank();
                    match self.text.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Json::Array(items));
                        }
                        _ => return Err(error()),
                    }
                }
            }
            b'"' => {
                self.at += 1;
                let mut out: Vec<u8> = Vec::new();
                loop {
                    let byte = *self.text.get(self.at).ok_or_else(error)?;
                    self.at += 1;
                    match byte {
                        b'"' => break,
                        b'\\' => {
                            let escape = *self.text.get(self.at).ok_or_else(error)?;
                            self.at += 1;
                            match escape {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'r' => out.push(b'\r'),
                                b'b' => out.push(8),
                                b'f' => out.push(12),
                                b'u' => {
                                    let hex =
                                        std::str::from_utf8(self.text.get(self.at..self.at + 4).ok_or_else(error)?)
                                            .map_err(|_| error())?;
                                    let mut code = u32::from_str_radix(hex, 16).map_err(|_| error())?;
                                    self.at += 4;
                                    if (0xD800..0xDC00).contains(&code)
                                        && self.text.get(self.at..self.at + 2) == Some(b"\\u")
                                    {
                                        let low = std::str::from_utf8(&self.text[self.at + 2..self.at + 6])
                                            .map_err(|_| error())?;
                                        let low = u32::from_str_radix(low, 16).map_err(|_| error())?;
                                        code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                        self.at += 6;
                                    }
                                    let c = char::from_u32(code).unwrap_or('\u{fffd}');
                                    let mut buffer = [0; 4];
                                    out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                                }
                                other => out.push(other),
                            }
                        }
                        other => out.push(other),
                    }
                }
                Ok(Json::Text(String::from_utf8_lossy(&out).into_owned()))
            }
            b't' if self.text[self.at..].starts_with(b"true") => {
                self.at += 4;
                Ok(Json::Bool(true))
            }
            b'f' if self.text[self.at..].starts_with(b"false") => {
                self.at += 5;
                Ok(Json::Bool(false))
            }
            b'n' if self.text[self.at..].starts_with(b"null") => {
                self.at += 4;
                Ok(Json::Null)
            }
            _ => {
                let start = self.at;
                while self.at < self.text.len()
                    && matches!(self.text[self.at], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                {
                    self.at += 1;
                }
                if self.at == start {
                    return Err(error());
                }
                let number = std::str::from_utf8(&self.text[start..self.at]).map_err(|_| error())?;
                Ok(Json::Number(lisp_number(number)))
            }
        }
    }
}

/// A JSON number as Lisp prints it.
fn lisp_number(number: &str) -> String {
    if !number.contains(['.', 'e', 'E']) {
        return number.trim_start_matches('+').to_string();
    }
    match number.parse::<f64>() {
        Ok(value) => {
            let mut written = format!("{value}");
            if !written.contains(['.', 'e', 'i', 'N']) {
                written.push_str(".0");
            }
            written
        }
        Err(_) => number.to_string(),
    }
}

/// `org-cite-basic--parse-json`
pub fn parse_json(text: &str) -> Result<Vec<Entry>, String> {
    let mut reader = JsonReader { text: text.as_bytes(), at: 0 };
    let items = match reader.value()? {
        Json::Array(items) => items,
        _ => return Err("Wrong type argument: listp".to_string()),
    };
    let mut entries: Vec<Entry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for item in items {
        let Json::Object(members) = &item else { continue };
        let key = match item.get("id") {
            Some(Json::Text(id)) => id.clone(),
            Some(other) => other.lisp(),
            None => "nil".to_string(),
        };
        let mut fields = Vec::new();
        for (name, value) in members {
            let field = match (name.as_str(), value) {
                ("author" | "editor", Json::Array(people)) => {
                    let names: Vec<String> = people
                        .iter()
                        .map(|person| {
                            let part = |name: &str| match person.get(name) {
                                Some(Json::Text(text)) => text.clone(),
                                _ => String::new(),
                            };
                            format!("{} {}", part("family"), part("given"))
                        })
                        .collect();
                    (name.clone(), Value::Text(names.join(" and ")))
                }
                ("issued", value) => {
                    let date = value.get("date-parts").or_else(|| value.get("literal")).or_else(|| value.get("raw"));
                    let year = match date {
                        Some(Json::Array(parts)) => match parts.first() {
                            Some(Json::Array(first)) => match first.first() {
                                Some(Json::Number(number)) => number.clone(),
                                Some(Json::Text(text)) => text.clone(),
                                Some(other) => {
                                    return Err(format!(
                                        "First element of CSL-JSON date-parts should be a number or string, got {}",
                                        other.lisp()
                                    ))
                                }
                                None => return Err("Unknown CSL-JSON date format".to_string()),
                            },
                            _ => return Err("Unknown CSL-JSON date format".to_string()),
                        },
                        Some(Json::Text(text)) => first_year(text),
                        _ => return Err(format!("Unknown CSL-JSON date format: {}", value.lisp())),
                    };
                    ("year".to_string(), Value::Text(year))
                }
                (_, Json::Text(text)) => (name.clone(), Value::Text(text.clone())),
                (_, other) => (name.clone(), Value::Other(other.lisp())),
            };
            fields.push(field);
        }
        let entry = Entry { key: key.clone(), fields };
        match index.get(&key) {
            Some(at) => entries[*at] = entry,
            None => {
                index.insert(key, entries.len());
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

/// The first four digits of a date, or the date as it is.
fn first_year(date: &str) -> String {
    let chars: Vec<char> = date.chars().collect();
    (0..chars.len().saturating_sub(3))
        .find(|at| chars[*at..*at + 4].iter().all(char::is_ascii_digit))
        .map_or_else(|| date.to_string(), |at| chars[at..at + 4].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_keys_of_a_citation() {
        let keys: Vec<String> =
            references("[cite/t:see @doe; @roe p. 3; and @x @y;]", 0).into_iter().map(|r| r.key).collect();
        assert_eq!(keys, ["doe", "roe", "x"]);
        let reference = &references("[cite:@doe]", 10)[0];
        assert_eq!((reference.start, reference.end), (16, 20));
    }

    #[test]
    fn measures_edits() {
        assert_eq!(distance("knuth1984", "knuth1985"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
    }
}

// ----- inserting and editing citations

/// A citation as `org-element-citation-parser` delimits it, in byte
/// offsets of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub begin: usize,
    /// After the blanks that follow it.
    pub end: usize,
    /// Just after its closing bracket.
    pub closing: usize,
    /// The colon after `cite` and the style.
    pub colon: usize,
    pub contents_begin: usize,
    pub contents_end: usize,
    /// A prefix common to the references (before the first `;`).
    pub prefix: bool,
    /// A suffix common to them (after the last `;`).
    pub suffix: bool,
    pub references: Vec<CitationReference>,
}

/// A reference of a citation (`org-element-citation-reference-parser`):
/// from the end of the one before to its `;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationReference {
    pub begin: usize,
    pub end: usize,
    /// The key with its `@`.
    pub key_start: usize,
    pub key_end: usize,
    pub key: String,
}

/// The first key (`@` and its characters) in `text[from..to]`: its start
/// and end.
fn find_key(text: &str, from: usize, to: usize) -> Option<(usize, usize)> {
    let mut at = from;
    while let Some(found) = text[at..to].find('@').map(|index| at + index) {
        let end =
            found + 1 + text[found + 1..to].chars().take_while(|c| is_key_char(*c)).map(char::len_utf8).sum::<usize>();
        if end > found + 1 {
            return Some((found, end));
        }
        at = found + 1;
    }
    None
}

/// The citation that starts at byte `begin` of `text`, as Org parses it.
pub fn parse_citation(text: &str, begin: usize) -> Option<Citation> {
    let rest = text[begin..].strip_prefix("[cite")?;
    let mut at = begin + 5;
    if let Some(style) = rest.strip_prefix('/') {
        let len: usize =
            style.chars().take_while(|c| c.is_alphanumeric() || "/_-".contains(*c)).map(char::len_utf8).sum();
        if len == 0 {
            return None;
        }
        at += 1 + len;
    }
    if !text[at..].starts_with(':') {
        return None;
    }
    let colon = at;
    let start = colon + 1 + text[colon + 1..].chars().take_while(|c| matches!(c, ' ' | '\t' | '\n')).count();
    // The matching bracket.
    let mut depth = 0;
    let mut closing = None;
    for (index, c) in text[begin..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    closing = Some(begin + index + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let closing = closing?;
    let (_, first_key_end) = find_key(text, start, closing)?;
    let end = closing + text[closing..].chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    let (prefix, contents_begin) = match text[start..first_key_end].rfind(';') {
        None => (false, start),
        Some(semicolon) => (semicolon > 0, start + semicolon + 1),
    };
    let last = text[..closing - 1].trim_end_matches([' ', '\r', '\t', '\n']).len();
    let (suffix, contents_end) = match text[first_key_end..last].rfind(';').map(|index| first_key_end + index) {
        Some(semicolon) if find_key(text, semicolon, last).is_none() => (semicolon + 1 < last, semicolon + 1),
        _ => (false, last),
    };
    let mut references = Vec::new();
    let mut point = contents_begin;
    while point < contents_end {
        let Some((key_start, key_end)) = find_key(text, point, contents_end) else { break };
        let end = match text[key_end..contents_end].find(';') {
            Some(index) => key_end + index + 1,
            None => contents_end,
        };
        references.push(CitationReference {
            begin: point,
            end,
            key_start,
            key_end,
            key: text[key_start + 1..key_end].to_string(),
        });
        point = end;
    }
    Some(Citation { begin, end, closing, colon, contents_begin, contents_end, prefix, suffix, references })
}

/// A change to the text: what replaces `start..end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// What `org-cite-insert` does at a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Insert {
    /// Delete (the citation or the reference): these changes, in order.
    Delete(Vec<Change>),
    /// Ask for a style, which replaces `start..end` (`/style`, or nothing).
    Style { start: usize, end: usize },
    /// Ask for one key, which goes in as `place` says. On a reference,
    /// no key aborts; on the affixes of a citation, Org puts in a bare `@`.
    Key { place: KeyPlace, reference: bool },
    /// Ask for keys until an empty answer, then with `style` for a style,
    /// and insert a new citation at `at`.
    New { at: usize, style: bool },
}

/// Where a key asked for goes in an existing citation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyPlace {
    /// Before a reference: `@key;` at its start.
    Before(usize),
    /// After a reference: at its end, `@key;` when a `;` is before, else
    /// `;@key`.
    After { at: usize, semicolon: bool },
    /// In place of a key.
    Replace { start: usize, end: usize },
}

impl KeyPlace {
    /// The change that puts `key` in.
    pub fn change(&self, key: &str) -> Change {
        match self {
            KeyPlace::Before(at) => Change { start: *at, end: *at, text: format!("@{key};") },
            KeyPlace::After { at, semicolon: true } => Change { start: *at, end: *at, text: format!("@{key};") },
            KeyPlace::After { at, semicolon: false } => Change { start: *at, end: *at, text: format!(";@{key}") },
            KeyPlace::Replace { start, end } => Change { start: *start, end: *end, text: format!("@{key}") },
        }
    }
}

/// The text of a new citation (`[cite/style:@a; @b]`) of `keys` in the
/// order they were answered, which Org puts last first.
pub fn new_citation(keys: &[String], style: &str) -> String {
    let style = if style.contains(|c: char| !c.is_whitespace()) { format!("/{style}") } else { String::new() };
    let keys: Vec<String> = keys.iter().rev().map(|key| format!("@{key}")).collect();
    format!("[cite{style}:{}]", keys.join("; "))
}

/// The styles `oc-basic` offers (`org-cite-supported-styles`), the
/// default as the empty string.
pub const STYLES: [&str; 7] = ["author", "noauthor", "nocite", "note", "numeric", "text", "nil"];

/// What `org-element-context` finds at a place, as far as inserting a
/// citation goes: a citation (and the reference in it), or something else
/// with whether `org-cite--allowed-p` lets a citation in there.
enum Place {
    Citation(Citation, Option<usize>),
    Allowed(bool),
}

/// What an object is, for finding the one at a place.
#[derive(Clone)]
enum ObjectKind {
    Citation(Citation),
    /// The reference of the citation the search went into.
    Reference(usize),
    Cell,
    Link,
    Other,
}

/// An object, as `org-element--object-lex` finds them.
#[derive(Clone)]
struct Object {
    kind: ObjectKind,
    begin: usize,
    /// With the blanks after it.
    end: usize,
    contents: Option<(usize, usize)>,
}

/// The objects of `text[start..end]`, in order.
fn objects_in(text: &str, start: usize, end: usize) -> Vec<Object> {
    use org_syntax::SyntaxKind as K;
    let node = org_syntax::parse_objects(&text[start..end]);
    let mut objects = Vec::new();
    for child in node.children() {
        let begin = start + usize::from(child.text_range().start());
        let strict_end = start + usize::from(child.text_range().end());
        let end =
            (strict_end + text[strict_end..end].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count()).min(end);
        let tokens: Vec<_> = child.children_with_tokens().filter_map(|c| c.into_token()).collect();
        let token_start = |t: &org_syntax::SyntaxToken| start + usize::from(t.text_range().start());
        let token_end = |t: &org_syntax::SyntaxToken| start + usize::from(t.text_range().end());
        let (kind, contents) = match child.kind() {
            K::CITATION => match parse_citation(text, begin) {
                Some(mut citation) => {
                    // Its blanks end with the region (a table cell's contents).
                    citation.end = citation.end.min(end);
                    let contents = (citation.contents_begin, citation.contents_end);
                    let end = citation.end;
                    objects.push(Object { kind: ObjectKind::Citation(citation), begin, end, contents: Some(contents) });
                    continue;
                }
                None => (ObjectKind::Other, None),
            },
            K::BOLD | K::ITALIC | K::UNDERLINE | K::STRIKE_THROUGH => {
                (ObjectKind::Other, Some((begin + 1, strict_end - 1)))
            }
            K::RADIO_TARGET => (ObjectKind::Other, Some((begin + 3, strict_end - 3))),
            K::LINK => {
                let separator = tokens.iter().find(|t| t.kind() == K::LINK_SEPARATOR);
                let close = tokens.iter().rev().find(|t| t.kind() == K::LINK_CLOSE);
                let contents = match (separator, close) {
                    (Some(separator), Some(close)) => Some((token_end(separator), token_start(close))),
                    _ => None,
                };
                (ObjectKind::Link, contents)
            }
            K::SUBSCRIPT | K::SUPERSCRIPT => {
                let markers: Vec<_> = tokens.iter().filter(|t| t.kind() == K::MARKER).collect();
                let contents = if markers.len() >= 3 {
                    Some((token_end(markers[1]), token_start(markers[markers.len() - 1])))
                } else {
                    markers.first().map(|marker| (token_end(marker), strict_end))
                };
                (ObjectKind::Other, contents)
            }
            K::FOOTNOTE_REFERENCE => {
                let open = tokens.iter().find(|t| t.kind() == K::FOOTNOTE_OPEN);
                let close = tokens.iter().rev().find(|t| t.kind() == K::FOOTNOTE_CLOSE);
                let contents = match (open, close) {
                    (Some(open), Some(close)) if open.text().matches(':').count() >= 2 => {
                        Some((token_end(open), token_start(close)))
                    }
                    _ => None,
                };
                (ObjectKind::Other, contents)
            }
            _ => (ObjectKind::Other, None),
        };
        objects.push(Object { kind, begin, end, contents });
    }
    objects
}

/// The cells of a table row whose contents are `text[start..end]`
/// (`org-element-table-cell-parser`).
fn cells(text: &str, start: usize, end: usize) -> Vec<Object> {
    let mut cells = Vec::new();
    let mut at = start;
    while at < end {
        let rest = &text[at..end];
        let blanks = rest.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
        let (cell_end, inner_end) = match rest.find('|') {
            Some(bar) => (at + bar + 1, at + bar),
            None => (end, end),
        };
        let contents_begin = (at + blanks).min(inner_end);
        let contents_end = contents_begin + text[contents_begin..inner_end].trim_end_matches([' ', '\t']).len();
        cells.push(Object {
            kind: ObjectKind::Cell,
            begin: at,
            end: cell_end,
            contents: Some((contents_begin, contents_end)),
        });
        at = cell_end;
    }
    cells
}

/// The objects in an object.
fn children(text: &str, object: &Object) -> Vec<Object> {
    let Some((start, end)) = object.contents else { return Vec::new() };
    match &object.kind {
        ObjectKind::Citation(citation) => citation
            .references
            .iter()
            .enumerate()
            .map(|(index, r)| Object { kind: ObjectKind::Reference(index), begin: r.begin, end: r.end, contents: None })
            .collect(),
        _ => objects_in(text, start, end),
    }
}

/// The object at `pos` among `objects`, which `text[..region_end]` holds,
/// as `org-element-context` finds it: going into objects whose contents
/// hold the place, and taking one that ends right at it when nothing
/// starts there.
fn object_at(
    text: &str,
    pos: usize,
    mut objects: Vec<Object>,
    mut region_end: usize,
) -> Option<(Object, Option<Object>)> {
    let blank_before = text[..pos].ends_with([' ', '\t']);
    let mut parent: Option<Object> = None;
    let mut last: Option<Object> = None;
    loop {
        let mut into = None;
        for object in objects {
            if object.begin > pos {
                return last.or(parent.clone()).map(|found| (found, parent));
            }
            if object.end <= pos && object.end != region_end {
                if object.end == pos && !blank_before {
                    last = Some(object);
                }
                continue;
            }
            if let Some((start, end)) = object.contents {
                if pos >= start && (pos < end || (pos == end && (pos == region_end || !blank_before))) {
                    into = Some(object);
                    break;
                }
            }
            return Some((object, parent));
        }
        let Some(object) = into else { return last.or(parent.clone()).map(|found| (found, parent)) };
        objects = children(text, &object);
        region_end = object.contents.map_or(region_end, |(_, end)| end);
        parent = Some(object);
    }
}

/// What there is at `pos` among the objects of `text[start..end]`: a
/// citation, another object (and whether a citation may go in there), or
/// nothing.
fn place_in(text: &str, pos: usize, objects: Vec<Object>, end: usize) -> Option<Place> {
    let (object, parent) = object_at(text, pos, objects, end)?;
    Some(match object.kind {
        ObjectKind::Citation(citation) => Place::Citation(citation, None),
        ObjectKind::Reference(index) => match parent.map(|parent| parent.kind) {
            Some(ObjectKind::Citation(citation)) => Place::Citation(citation, Some(index)),
            _ => Place::Allowed(false),
        },
        kind => {
            let strict_end = text[..object.end].trim_end_matches([' ', '\r', '\t', '\n']).len();
            let in_contents = object.contents.is_some_and(|(start, end)| pos >= start && pos <= end);
            Place::Allowed(if pos >= strict_end || pos == object.begin {
                true
            } else {
                match kind {
                    ObjectKind::Link => false,
                    ObjectKind::Cell => {
                        let back = text[..pos].trim_end_matches([' ', '\t']).len();
                        object.contents.is_some_and(|(_, end)| back <= end)
                    }
                    _ => in_contents,
                }
            })
        }
    })
}

/// The groups of `org-complex-heading-regexp` on a headline line: the end
/// of the stars, of the keyword and of the priority, the title, and the
/// start of the tags.
struct Heading {
    stars_end: usize,
    todo_end: Option<usize>,
    priority_end: Option<usize>,
    title: Option<(usize, usize)>,
    tags_start: Option<usize>,
}

fn heading(line: &str, todo: &org_syntax::TodoKeywords) -> Option<Heading> {
    let keywords: Vec<String> = todo.names().map(regex::escape).collect();
    let todo = if keywords.is_empty() { String::new() } else { format!("(?: +(?P<todo>{}))?", keywords.join("|")) };
    let pattern = format!(
        r"^(?P<stars>\*+){todo}(?: +(?P<priority>\[#(?:[A-Z]|[0-9]|[1-5][0-9]|6[0-4])\]))?(?: +(?P<title>.*?))??(?:[ \t]+(?P<tags>:[\w@#%:]+:))?[ \t]*$"
    );
    let captures = regex::Regex::new(&pattern).ok()?.captures(line)?;
    Some(Heading {
        stars_end: captures.name("stars")?.end(),
        todo_end: captures.name("todo").map(|m| m.end()),
        priority_end: captures.name("priority").map(|m| m.end()),
        title: captures.name("title").map(|m| (m.start(), m.end())),
        tags_start: captures.name("tags").map(|m| m.start()),
    })
}

/// The start and end of the line around `pos`, without its newline.
fn line_around(text: &str, pos: usize) -> (usize, usize) {
    let start = text[..pos].rfind('\n').map_or(0, |index| index + 1);
    let end = text[pos..].find('\n').map_or(text.len(), |index| pos + index);
    (start, end)
}

/// `org-element--affiliated-re`.
static AFFILIATED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)^[ \t]*#\+(?:(CAPTION|DATA|HEADERS?|LABEL|NAME|PLOT|RESNAME|RESULTS?|SOURCE|SRCNAME|TBLNAME)(?:\[(.*)\])?|(ATTR_[-_A-Za-z0-9]+)):[ \t]*",
    )
    .unwrap()
});
/// A caption line, as `org-cite--allowed-p` looks back for it.
static CAPTION: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^[ \t]*#\+CAPTION:").unwrap());
/// The end of an inline task.
static INLINETASK_END: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^\*+ END[ \t]*$").unwrap());

/// An affiliated keyword's line: its name, the optional value in
/// brackets, and the end of the keyword.
struct Affiliated {
    name: String,
    dual: Option<(usize, usize)>,
    end: usize,
}

fn affiliated(line: &str) -> Option<Affiliated> {
    let captures = AFFILIATED.captures(line)?;
    let name = captures.get(1).or(captures.get(3))?.as_str().to_uppercase();
    Some(Affiliated { name, dual: captures.get(2).map(|m| (m.start(), m.end())), end: captures.get(0)?.end() })
}

/// Whether a block is a verse block.
fn is_verse(block: &org_syntax::SyntaxNode) -> bool {
    block
        .children_with_tokens()
        .find(|child| child.kind() == org_syntax::SyntaxKind::BLOCK_BEGIN)
        .and_then(|begin| begin.into_token())
        .is_some_and(|begin| begin.text().to_lowercase().starts_with("#+begin_verse"))
}

/// Whether a keyword is affiliated to the element it is in: one of the
/// keywords before the element's own lines.
fn is_affiliated(keyword: &org_syntax::SyntaxNode) -> bool {
    use org_syntax::SyntaxKind as K;
    let Some(parent) = keyword.parent() else { return false };
    if matches!(parent.kind(), K::SECTION | K::DOCUMENT) {
        return false;
    }
    let mut previous = keyword.prev_sibling_or_token();
    while let Some(element) = previous {
        if element.kind() != K::KEYWORD {
            return false;
        }
        previous = element.prev_sibling_or_token();
    }
    true
}

/// What there is at byte `pos`: `org-element-context`, and then
/// `org-cite--allowed-p` unless it is a citation.
fn place(text: &str, parse: &org_syntax::Parse, pos: usize) -> Place {
    use org_syntax::SyntaxKind as K;
    let root = parse.syntax();
    let Ok(at) = org_syntax::TextSize::try_from(pos) else { return Place::Allowed(false) };
    let token = match root.token_at_offset(at) {
        org_syntax::TokenAtOffset::None => return Place::Allowed(true),
        org_syntax::TokenAtOffset::Single(token) => token,
        org_syntax::TokenAtOffset::Between(_, right) => right,
    };
    let eob = pos == text.len();
    // A blank line, or the end of a document that ends with a newline:
    // the element before has it, and only a verse block keeps a citation
    // out of there.
    if token.kind() == K::BLANK_LINE || (eob && (text.is_empty() || text.ends_with('\n'))) {
        let mut previous =
            if token.kind() == K::BLANK_LINE { token.prev_sibling_or_token() } else { Some(token.clone().into()) };
        while let Some(element) = previous.clone() {
            if element.kind() == K::BLANK_LINE {
                previous = element.prev_sibling_or_token();
                continue;
            }
            break;
        }
        let verse = previous.and_then(|element| {
            let node = element.as_node().cloned().or_else(|| element.as_token().and_then(|token| token.parent()))?;
            node.ancestors().find(|node| node.kind() == K::BLOCK)
        });
        return Place::Allowed(!verse.is_some_and(|block| is_verse(&block)));
    }
    let node_range =
        |node: &org_syntax::SyntaxNode| (usize::from(node.text_range().start()), usize::from(node.text_range().end()));
    for node in token.parent_ancestors() {
        match node.kind() {
            K::KEYWORD if is_affiliated(&node) => {
                let (line_start, line_end) = line_around(text, pos);
                let line = &text[line_start..line_end];
                let caption = CAPTION.find(line);
                let allowed = caption.is_some_and(|m| pos >= line_start + m.end());
                if let Some(Affiliated { name, dual, end }) = affiliated(line) {
                    if name == "CAPTION" {
                        let region = if pos >= line_start + end {
                            Some((line_start + end, line_end))
                        } else {
                            dual.map(|(start, end)| (line_start + start, line_start + end))
                                .filter(|(start, end)| pos >= *start && pos < *end)
                        };
                        if let Some((start, end)) = region {
                            if let Some(place) = place_in(text, pos, objects_in(text, start, end), end) {
                                return place;
                            }
                        }
                    }
                }
                return Place::Allowed(allowed);
            }
            K::PARAGRAPH => {
                let (_, end) = node_range(&node);
                let post = node
                    .children()
                    .filter(|child| child.kind() == K::KEYWORD)
                    .last()
                    .map_or(usize::from(node.text_range().start()), |keyword| node_range(&keyword).1);
                if pos >= post && (pos < end || (pos == end && eob)) {
                    if let Some(place) = place_in(text, pos, objects_in(text, post, end), end) {
                        return place;
                    }
                }
                return Place::Allowed(true);
            }
            K::BLOCK => {
                if !is_verse(&node) {
                    return Place::Allowed(eob);
                }
                let begin = node.children_with_tokens().find(|child| child.kind() == K::BLOCK_BEGIN);
                let start = begin.map_or(node_range(&node).0, |begin| usize::from(begin.text_range().start()));
                let end = node_range(&node).1;
                let block = &text[start..end];
                let contents_begin = block.find('\n').map_or(end, |index| start + index + 1);
                let end_line = block.trim_end_matches(['\n', ' ', '\t']).rfind('\n').map_or(start, |index| start + index + 1);
                let contents_end = end_line.max(contents_begin);
                if pos >= contents_begin && pos < contents_end {
                    if let Some(place) = place_in(text, pos, objects_in(text, contents_begin, contents_end), contents_end) {
                        return place;
                    }
                    return Place::Allowed(true);
                }
                return Place::Allowed(false);
            }
            K::TABLE_ROW => {
                let (start, _) = node_range(&node);
                let (_, line_end) = line_around(text, start);
                let line = &text[start..line_end];
                if line.trim_start().starts_with("|-") {
                    return Place::Allowed(eob);
                }
                let Some(bar) = line.find('|') else { return Place::Allowed(eob) };
                let contents_begin = start + bar + 1;
                let contents_end = text[..line_end].trim_end_matches([' ', '\t']).len().max(contents_begin);
                if pos >= contents_begin && (pos < contents_end || (pos == contents_end && eob)) {
                    if let Some(place) = place_in(text, pos, cells(text, contents_begin, contents_end), contents_end) {
                        return place;
                    }
                }
                return Place::Allowed(eob);
            }
            K::LIST_ITEM => {
                let (start, _) = node_range(&node);
                let (line_start, line_end) = line_around(text, start);
                if let Some(tag) = node.children().find(|child| child.kind() == K::ITEM_TAG) {
                    let (tag_start, tag_end) = node_range(&tag);
                    if pos >= tag_start && pos < tag_end && pos <= line_end {
                        if let Some(place) = place_in(text, pos, objects_in(text, tag_start, tag_end), tag_end) {
                            return place;
                        }
                    }
                }
                let (pos_line, _) = line_around(text, pos);
                let indentation = text[pos_line..].chars().take_while(|c| matches!(c, ' ' | '\t')).count();
                let checkbox = node.children_with_tokens().any(|child| child.kind() == K::CHECKBOX);
                return Place::Allowed(pos > line_start + indentation + if checkbox { 5 } else { 1 });
            }
            K::HEADLINE => {
                let (start, _) = node_range(&node);
                let (_, line_end) = line_around(text, start);
                let line = &text[start..line_end];
                let Some(heading) = heading(line, parse.todo_keywords()) else { return Place::Allowed(false) };
                if let Some((title_start, title_end)) = heading.title {
                    let mut from = start + title_start;
                    if text[from..].starts_with("COMMENT") {
                        from += 7;
                    }
                    let end = start + title_end;
                    if pos >= from && pos <= end && from < end {
                        if let Some(place) = place_in(text, pos, objects_in(text, from, end), end) {
                            return place;
                        }
                    }
                }
                let end_line = INLINETASK_END.is_match(line);
                let first = heading.title.map(|(title, _)| title).or(heading.priority_end).or(heading.todo_end);
                let first = start + first.unwrap_or(heading.stars_end + 1);
                let before_tags = heading.tags_start.is_none_or(|tags| pos < start + tags);
                return Place::Allowed(!end_line && pos >= first && before_tags);
            }
            K::FOOTNOTE_DEFINITION => {
                return Place::Allowed(text[pos..].starts_with(|c: char| c.is_whitespace()));
            }
            K::SECTION | K::DOCUMENT => return Place::Allowed(true),
            K::DRAWER
            | K::PROPERTY_DRAWER
            | K::NODE_PROPERTY
            | K::PLANNING
            | K::KEYWORD
            | K::DYNAMIC_BLOCK
            | K::COMMENT
            | K::FIXED_WIDTH
            | K::HORIZONTAL_RULE
            | K::TABLE
            | K::CLOCK
            | K::LATEX_ENVIRONMENT
            | K::DIARY_SEXP
            // Any other element: only past its end, which is the end of
            // the document when it has no newline after.
            | K::LIST => return Place::Allowed(eob),
            _ => {}
        }
    }
    Place::Allowed(true)
}

/// `org-cite-insert` (`C-c C-x @`) at byte `offset`, with the prefix
/// argument `arg`: what to do there, as `org-cite-make-insert-processor`
/// decides with the `basic` processor.
pub fn insert(text: &str, parse: &org_syntax::Parse, offset: usize, arg: bool) -> Result<Insert, String> {
    let (citation, reference) = match place(text, parse, offset) {
        Place::Citation(citation, reference) => (citation, reference),
        Place::Allowed(true) => return Ok(Insert::New { at: offset, style: arg }),
        Place::Allowed(false) => return Err("Cannot insert a citation here".to_string()),
    };
    if let Some(index) = reference {
        let reference = &citation.references[index];
        if arg {
            return Ok(Insert::Delete(delete_reference(text, &citation, index, offset)));
        }
        let place = if reference.key_start >= offset {
            KeyPlace::Before(reference.begin)
        } else if reference.key_end <= offset {
            KeyPlace::After { at: reference.end, semicolon: text[..reference.end].ends_with(';') }
        } else {
            KeyPlace::Replace { start: reference.key_start, end: reference.key_end }
        };
        return Ok(Insert::Key { place, reference: true });
    }
    let boundaries_end = text[..citation.end].trim_end_matches([' ', '\t']).len();
    if offset > citation.begin && offset < boundaries_end {
        if arg {
            return Ok(Insert::Delete(delete_citation(text, &citation, offset)));
        }
        if offset <= citation.colon {
            return Ok(Insert::Style { start: citation.begin + 5, end: citation.colon });
        }
        let (first, last) = (citation.references.first(), citation.references.last());
        let place = if offset < citation.contents_begin {
            KeyPlace::Before(first.map_or(citation.contents_begin, |first| first.begin))
        } else {
            let at = last.map_or(citation.contents_end, |last| last.end);
            KeyPlace::After { at, semicolon: text[..at].ends_with(';') }
        };
        return Ok(Insert::Key { place, reference: false });
    }
    // On the citation's start or the blanks after it: a new one.
    Ok(Insert::New { at: offset, style: arg })
}

/// `org-cite-delete-citation` on a citation, from byte `offset`: it goes
/// with the blanks around it, as much as keeps the line as it should be
/// (the line of `offset`, as Org takes it).
fn delete_citation(text: &str, citation: &Citation, offset: usize) -> Vec<Change> {
    let begin = citation.begin;
    let end = text[..citation.end].trim_end_matches([' ', '\t']).len();
    let before_blank = text[..begin].trim_end_matches([' ', '\t']).len();
    let after_blank = citation.end;
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |index| offset + index);
    let first = before_blank == line_start;
    let last = after_blank == line_end;
    match (first, last) {
        (true, true) => {
            let next = (line_end + 1).min(text.len());
            vec![Change { start: line_start, end: next, text: String::new() }]
        }
        (true, false) => vec![Change { start: begin, end: after_blank, text: String::new() }],
        (false, true) => {
            vec![Change { start: before_blank.min(line_end), end: line_end.max(before_blank), text: String::new() }]
        }
        (false, false) => {
            let mut changes = vec![Change { start: before_blank, end, text: String::new() }];
            if after_blank == end {
                changes.push(Change { start: before_blank, end: before_blank, text: " ".to_string() });
            }
            changes
        }
    }
}

/// `org-cite-delete-citation` on the reference `index` of `citation`.
fn delete_reference(text: &str, citation: &Citation, index: usize, offset: usize) -> Vec<Change> {
    let reference = &citation.references[index];
    if citation.references.len() == 1 {
        return delete_citation(text, citation, offset);
    }
    if reference.begin == citation.contents_begin && !citation.prefix {
        let start = text[..reference.begin].trim_end_matches([' ', '\t']).len();
        return vec![Change { start, end: reference.end, text: String::new() }];
    }
    if reference.end == citation.contents_end && !citation.suffix {
        let boundaries_end = text[..citation.end].trim_end_matches([' ', '\t']).len();
        return vec![Change { start: reference.begin - 1, end: boundaries_end - 1, text: String::new() }];
    }
    vec![Change { start: reference.begin, end: reference.end, text: String::new() }]
}

/// The text with `changes` made, in order.
pub fn apply(text: &str, changes: &[Change]) -> String {
    let mut text = text.to_string();
    for change in changes {
        text.replace_range(change.start..change.end, &change.text);
    }
    text
}

/// The prompt for the next key of a new citation, after `keys`.
pub fn keys_prompt(keys: &[String]) -> String {
    if keys.is_empty() {
        "Key (empty input exits): ".to_string()
    } else {
        format!("Key (empty input exits) {}: ", keys.join(";"))
    }
}
