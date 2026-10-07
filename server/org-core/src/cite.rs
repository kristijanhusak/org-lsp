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
