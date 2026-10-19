//! BibTeX entries as Org headlines, as `ol-bibtex.el` makes them: an
//! entry of the headline's properties (`org-bibtex-headline`), written
//! for each headline to a `.bib` file (`org-bibtex`) or for one to the
//! kill ring (`org-bibtex-export-to-kill-ring`); the properties of a new
//! entry asked for in turn (`org-bibtex-create`,
//! `org-bibtex-create-in-current-entry`), or those missing
//! (`org-bibtex-check`, `org-bibtex-check-all`); a BibTeX entry read as a
//! headline (`org-bibtex-yank`, `org-bibtex-import-from-file`). Checked
//! against Emacs in `tests/bibtex`.
//!
//! The questions are asked in turn: a command runs with the answers so
//! far and stops at the next question ([`Stop::Ask`]). The key that
//! `org-bibtex-autogen-keys` asks `bibtex-generate-autokey` for comes from
//! the caller.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

use crate::edit;
use crate::indent;
use crate::meta_return;
use crate::newline::Outcome;
use crate::outline::{Inheritance, Outline};
use crate::properties::{self, Change};
use crate::todo::headline_level;

/// The options of `ol-bibtex.el`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-bibtex-autogen-keys`: keys from `bibtex-generate-autokey`
    /// instead of asked for.
    pub autogen_keys: bool,
    /// `org-bibtex-prefix`: before the names of the properties written.
    pub prefix: Option<String>,
    /// `org-bibtex-treat-headline-as-title`: the headline stands for a
    /// missing title, which `org-bibtex-check` then does not ask for.
    pub treat_headline_as_title: bool,
    /// `org-bibtex-export-arbitrary-fields`: with a prefix, every property
    /// with it is a field.
    pub export_arbitrary_fields: bool,
    /// `org-bibtex-key-property`
    pub key_property: String,
    /// `org-bibtex-tags`: tags of new entries.
    pub tags: Vec<String>,
    /// `org-bibtex-tags-are-keywords`: the `keywords` field as tags, and
    /// back.
    pub tags_are_keywords: bool,
    /// `org-bibtex-no-export-tags`: tags that are no keywords.
    pub no_export_tags: Vec<String>,
    /// `org-bibtex-inherit-tags`: inherited tags are keywords too.
    pub inherit_tags: bool,
    /// `org-bibtex-type-property-name`
    pub type_property_name: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            autogen_keys: false,
            prefix: None,
            treat_headline_as_title: true,
            export_arbitrary_fields: false,
            key_property: "CUSTOM_ID".to_string(),
            tags: Vec::new(),
            tags_are_keywords: false,
            no_export_tags: Vec::new(),
            inherit_tags: false,
            type_property_name: "btype".to_string(),
        }
    }
}

/// A field an entry type asks for, or one of several.
#[derive(Debug, Clone, Copy)]
enum Field {
    One(&'static str),
    Either(&'static [&'static str]),
}

use Field::{Either, One};

/// `org-bibtex-types`: each type with its required and optional fields.
const TYPES: [(&str, &[Field], &[Field]); 14] = [
    (
        "article",
        &[One("author"), One("title"), One("journal"), One("year")],
        &[One("volume"), One("number"), One("pages"), One("month"), One("note"), One("doi")],
    ),
    (
        "book",
        &[Either(&["editor", "author"]), One("title"), One("publisher"), One("year")],
        &[
            Either(&["volume", "number"]),
            One("series"),
            One("address"),
            One("edition"),
            One("month"),
            One("note"),
            One("doi"),
        ],
    ),
    (
        "booklet",
        &[One("title")],
        &[
            One("author"),
            One("howpublished"),
            One("address"),
            One("month"),
            One("year"),
            One("note"),
            One("doi"),
            One("url"),
        ],
    ),
    (
        "conference",
        &[One("author"), One("title"), One("booktitle"), One("year")],
        &[
            One("editor"),
            One("pages"),
            One("organization"),
            One("publisher"),
            One("address"),
            One("month"),
            One("note"),
            One("doi"),
            One("url"),
        ],
    ),
    (
        "inbook",
        &[Either(&["author", "editor"]), One("title"), Either(&["chapter", "pages"]), One("publisher"), One("year")],
        &[
            One("crossref"),
            Either(&["volume", "number"]),
            One("series"),
            One("type"),
            One("address"),
            One("edition"),
            One("month"),
            One("note"),
            One("doi"),
        ],
    ),
    (
        "incollection",
        &[One("author"), One("title"), One("booktitle"), One("publisher"), One("year")],
        &[
            One("crossref"),
            One("editor"),
            Either(&["volume", "number"]),
            One("series"),
            One("type"),
            One("chapter"),
            One("pages"),
            One("address"),
            One("edition"),
            One("month"),
            One("note"),
            One("doi"),
        ],
    ),
    (
        "inproceedings",
        &[One("author"), One("title"), One("booktitle"), One("year")],
        &[
            One("crossref"),
            One("editor"),
            Either(&["volume", "number"]),
            One("series"),
            One("pages"),
            One("address"),
            One("month"),
            One("organization"),
            One("publisher"),
            One("note"),
            One("doi"),
        ],
    ),
    (
        "manual",
        &[One("title")],
        &[
            One("author"),
            One("organization"),
            One("address"),
            One("edition"),
            One("month"),
            One("year"),
            One("note"),
            One("doi"),
            One("url"),
        ],
    ),
    (
        "mastersthesis",
        &[One("author"), One("title"), One("school"), One("year")],
        &[One("type"), One("address"), One("month"), One("note"), One("doi"), One("url")],
    ),
    (
        "misc",
        &[],
        &[
            One("author"),
            One("title"),
            One("howpublished"),
            One("month"),
            One("year"),
            One("note"),
            One("doi"),
            One("url"),
        ],
    ),
    (
        "phdthesis",
        &[One("author"), One("title"), One("school"), One("year")],
        &[One("type"), One("address"), One("month"), One("note"), One("doi"), One("url")],
    ),
    (
        "proceedings",
        &[One("title"), One("year")],
        &[
            One("editor"),
            Either(&["volume", "number"]),
            One("series"),
            One("address"),
            One("month"),
            One("organization"),
            One("publisher"),
            One("note"),
            One("doi"),
        ],
    ),
    (
        "techreport",
        &[One("author"), One("title"), One("institution"), One("year")],
        &[One("type"), One("address"), One("month"), One("note"), One("doi"), One("url")],
    ),
    ("unpublished", &[One("author"), One("title"), One("note")], &[One("month"), One("year"), One("doi"), One("url")]),
];

/// The fields of `org-bibtex-fields`, those `org-bibtex-ask` knows.
const FIELDS: [&str; 26] = [
    "address",
    "annote",
    "author",
    "booktitle",
    "chapter",
    "crossref",
    "doi",
    "edition",
    "editor",
    "howpublished",
    "institution",
    "journal",
    "key",
    "month",
    "note",
    "number",
    "organization",
    "pages",
    "publisher",
    "school",
    "series",
    "title",
    "type",
    "url",
    "volume",
    "year",
];

/// The names of the entry types, as `org-bibtex-create` offers them.
pub fn type_names() -> Vec<String> {
    TYPES.iter().map(|(name, ..)| name.to_string()).collect()
}

/// The type `name` names, as Org looks it up: with regard to case.
fn entry_type(name: &str) -> Option<&'static (&'static str, &'static [Field], &'static [Field])> {
    TYPES.iter().find(|(kind, ..)| *kind == name)
}

/// What the commands need besides the text.
pub struct Env<'a> {
    pub settings: &'a Settings,
    /// How Org writes properties, tags and headings.
    pub edit: &'a edit::Settings,
    /// The TODO keywords of files without their own.
    pub keywords: &'a TodoKeywords,
    /// What entries inherit, for `org-get-tags`.
    pub inheritance: &'a Inheritance,
    /// How `org-indent-region` indents.
    pub indent: &'a indent::Settings,
    /// The file's name, its entries' category without one of their own.
    pub category: &'a str,
    /// The name of its buffer in Emacs, for Emacs's errors.
    pub buffer: &'a str,
}

impl Env<'_> {
    fn properties(&self, text: &str) -> properties::Context {
        edit::property_context(&org_syntax::parse(text, self.keywords), self.edit, self.category)
    }

    /// `org-bibtex-get`: the value of `property` in the entry at `at`, or
    /// else of the property with the prefix, trimmed.
    fn get(&self, text: &str, at: usize, property: &str) -> Option<String> {
        let context = self.properties(text);
        let value = properties::local_value(text, at, &context);
        let upper = property.to_uppercase();
        let prefix = self.settings.prefix.as_deref().unwrap_or("");
        value(&upper).or_else(|| value(&format!("{prefix}{upper}"))).map(|value| trim(&value).to_string())
    }

    /// The name `org-bibtex-put` writes `property` as: in capitals, with
    /// the prefix but for the key's.
    fn put_name(&self, property: &str) -> String {
        let name = property.to_uppercase();
        if name == self.settings.key_property {
            name
        } else {
            format!("{}{name}", self.settings.prefix.as_deref().unwrap_or(""))
        }
    }
}

/// `org-trim`
fn trim(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\n', '\r'])
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

/// Where a position `at` of `old` is in `new`, as a marker of Emacs's
/// follows the text around it: before what changed it stays.
fn follow(old: &str, new: &str, at: usize) -> usize {
    let (prefix, suffix) = crate::text::common_ends(old, new);
    if at <= prefix {
        at
    } else if at >= old.len() - suffix {
        at + new.len() - old.len()
    } else {
        prefix
    }
}

/// `nth 4` of `org-heading-components` on the headline at `heading`: its
/// text without keyword, priority and tags (`COMMENT` stays).
fn heading_title(text: &str, heading: usize, keywords: &TodoKeywords) -> Option<String> {
    let line = &text[heading..line_end(text, heading)];
    let level = headline_level(line)?;
    let parse_keywords = org_syntax::parse(text, keywords).todo_keywords().clone();
    crate::outline::complex_heading_title(line, level, &parse_keywords).map(|(start, end)| line[start..end].to_string())
}

/// The tags `org-get-tags` gives the headline at `heading`, its own or
/// with those it inherits.
fn tags_of(text: &str, heading: usize, local: bool, env: &Env) -> Vec<String> {
    let parse = org_syntax::parse(text, env.keywords);
    let outline = Outline::with_inheritance(&parse, text, env.category, env.inheritance);
    let Some(entry) = outline.entry_at(heading) else { return Vec::new() };
    outline.entries[entry].tags.iter().filter(|tag| !local || !tag.inherited).map(|tag| tag.name.clone()).collect()
}

/// `org-entry-properties` with `standard`: the properties of the entry's
/// drawer, the last first, `NAME+` values joined to theirs, and
/// `CATEGORY` in front when the drawer has none.
fn standard_properties(text: &str, at: usize, env: &Env) -> Vec<(String, String)> {
    static PROPERTY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[ \t]*:(\S+):(?:[ \t]+(.*?))?[ \t]*$").expect("a valid regex"));
    let mut props: Vec<(String, String)> = Vec::new();
    if let Some((begin, end)) = properties::block(text, at) {
        let mut seen: Vec<String> = Vec::new();
        for line in text[begin..end].lines() {
            let Some(captures) = PROPERTY.captures(line) else { continue };
            let key = captures[1].to_uppercase();
            let value = captures.get(2).map_or("", |value| value.as_str()).to_string();
            let (base, extends) = match key.strip_suffix('+') {
                Some(base) => (base.to_string(), true),
                None => (key.clone(), false),
            };
            if properties::SPECIAL.iter().any(|special| special.eq_ignore_ascii_case(&base)) {
                continue;
            }
            if extends {
                // `org--update-property-plist`
                let previous = props.iter().position(|(name, _)| *name == base).map(|index| props.remove(index).1);
                let joined = match previous {
                    Some(previous) => format!("{previous} {value}"),
                    None => value,
                };
                props.insert(0, (base, joined));
            } else if !seen.contains(&key) {
                seen.push(key.clone());
                match props.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(&key)) {
                    Some((_, old)) => *old = format!("{value} {old}"),
                    None => props.insert(0, (key, value)),
                }
            }
        }
    }
    if !props.iter().any(|(name, _)| name == "CATEGORY") {
        let parse = org_syntax::parse(text, env.keywords);
        let outline = Outline::with_inheritance(&parse, text, env.category, env.inheritance);
        let category = match properties::heading_at(text, at).and_then(|heading| outline.entry_at(heading)) {
            Some(entry) => outline.entries[entry].category.clone(),
            None => outline.category.clone(),
        };
        props.insert(0, ("CATEGORY".to_string(), category));
    }
    props
}

/// `org-bibtex-headline`: the BibTeX entry of the headline at `at`, or
/// nothing when it has no type.
pub fn headline(text: &str, at: usize, env: &Env) -> Option<String> {
    let settings = env.settings;
    let id = env.get(text, at, &settings.key_property);
    let kind = env.get(text, at, &settings.type_property_name)?;
    let heading = properties::heading_at(text, at);
    let tags: Vec<String> = match heading {
        Some(heading) if settings.tags_are_keywords => tags_of(text, heading, !settings.inherit_tags, env)
            .into_iter()
            .filter(|tag| !settings.tags.contains(tag) && !settings.no_export_tags.contains(tag))
            .collect(),
        _ => Vec::new(),
    };
    let pairs: Vec<(String, String)> = match settings.prefix.as_deref() {
        Some(prefix) if settings.export_arbitrary_fields => {
            let matcher = crate::agenda::matcher::emacs_regex(prefix);
            let type_key = format!("{prefix}{}", settings.type_property_name).to_lowercase();
            standard_properties(text, at, env)
                .into_iter()
                .filter_map(|(key, value)| {
                    let matcher = matcher.as_ref()?;
                    (matcher.is_match(&key) && type_key != key.to_lowercase())
                        .then(|| (matcher.replace_all(&key, "").to_lowercase(), value))
                })
                .collect()
        }
        _ => {
            let fields = entry_type(&kind).map(|(_, required, optional)| required.iter().chain(optional.iter()));
            let names = fields.into_iter().flatten().flat_map(|field| match field {
                One(name) => vec![*name],
                Either(names) => names.to_vec(),
            });
            names
                .filter_map(|name| {
                    let value = env.get(text, at, name).or_else(|| {
                        (name == "title").then(|| heading.and_then(|h| heading_title(text, h, env.keywords)))?
                    });
                    value.map(|value| (name.to_string(), value))
                })
                .collect()
        }
    };
    let body: Vec<String> = pairs.iter().map(|(name, value)| format!("  {name}={{{value}}}")).collect();
    let mut entry = format!("@{kind}{{{},\n{}\n}}\n", id.as_deref().unwrap_or("nil"), body.join(",\n"));
    if !tags.is_empty() {
        static KEYWORDS: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)keywords.*=.*\{(.*)\}").expect("a valid regex"));
        let joined = tags.join(", ");
        match KEYWORDS.captures(&entry).and_then(|captures| captures.get(1)).map(|group| group.end()) {
            Some(end) => entry.insert_str(end, &format!(", {joined}")),
            None => {
                let at = entry.find(",\n").map_or(0, |at| at + 2);
                entry.insert_str(at, "  keywords={},\n");
                entry.insert_str(at + 12, &joined);
            }
        }
    }
    Some(entry)
}

/// `org-bibtex`: the entries of the headlines with a type, as the file
/// is written, and how many.
pub fn export(text: &str, env: &Env) -> (String, usize) {
    let entries: Vec<String> =
        headings(text, env).into_iter().filter_map(|heading| headline(text, heading, env)).collect();
    (entries.join("\n"), entries.len())
}

/// The headlines `org-map-entries` goes through, in order.
fn headings(text: &str, env: &Env) -> Vec<usize> {
    let parse = org_syntax::parse(text, env.keywords);
    Outline::new(&parse, text, env.category).entries.iter().map(|entry| entry.offset).collect()
}

/// A question a command asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    /// The text the answer starts with.
    pub initial: Option<String>,
    /// What `completing-read` offers; none for `read-from-minibuffer`.
    pub completions: Vec<String>,
    /// The answer is meant to be one of them: Org knows no other (the
    /// type, one of two fields). An empty answer is no default then.
    pub select: bool,
}

/// Why a command stopped before it was done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// It asks this, after the answers it was given.
    Ask(Question),
    /// It failed, with Emacs's message.
    Failed(String),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Failed(message)
    }
}

/// `bibtex-generate-autokey` on an entry (`org-bibtex-autogen-keys`).
pub type Autokey<'a> = &'a dyn Fn(&str) -> Result<String, String>;

/// A command running: the text and point as it changes them, and the
/// answers it reads in turn.
struct Run<'a> {
    env: &'a Env<'a>,
    text: String,
    point: usize,
    answers: &'a [String],
    asked: usize,
    autokey: Autokey<'a>,
}

impl Run<'_> {
    fn ask(&mut self, prompt: &str, initial: Option<String>, completions: Vec<String>) -> Result<String, Stop> {
        let Some(answer) = self.answers.get(self.asked) else {
            let select = matches!(prompt, "Type: " | "Field: ");
            return Err(Stop::Ask(Question { prompt: prompt.to_string(), initial, completions, select }));
        };
        self.asked += 1;
        Ok(answer.clone())
    }

    fn get(&self, property: &str) -> Option<String> {
        self.env.get(&self.text, self.point, property)
    }

    fn change(&mut self, text: String) {
        self.point = follow(&self.text, &text, self.point);
        self.text = text;
    }

    /// A change that writes a line again (`org-entry-put` and
    /// `org--align-node-property` replace the whole line): a point in what
    /// changed goes to the start of that line.
    fn change_line(&mut self, text: String) {
        let (prefix, suffix) = crate::text::common_ends(&self.text, &text);
        self.point = if self.point > prefix && self.point < self.text.len() - suffix {
            self.text[..prefix].rfind('\n').map_or(0, |newline| newline + 1)
        } else {
            follow(&self.text, &text, self.point)
        };
        self.text = text;
    }

    fn insert(&mut self, what: &str) {
        self.text.insert_str(self.point, what);
        self.point += what.len();
    }

    /// `org-bibtex-put`: `org-set-property`, or with `raw` the line
    /// inserted at the point.
    fn put(&mut self, property: &str, value: Option<&str>, raw: bool) -> Result<(), Stop> {
        let name = self.env.put_name(property);
        if raw {
            self.insert(&format!(":{name}: {}\n", value.unwrap_or("nil")));
            return Ok(());
        }
        let context = self.env.properties(&self.text);
        // Without a value, `org-set-property` asks for one
        // (`org-read-property-value`).
        let value = match value {
            Some(value) => value.to_string(),
            None => {
                let question = properties::value_question(&self.text, self.point, &name, &context)?;
                let typed = self.ask(&question.prompt, None, question.collection.clone())?;
                question.answer(&typed)
            }
        };
        if let Change::Text(text) = properties::set(&self.text, self.point, &name, &value, &context)? {
            self.change_line(text);
        }
        Ok(())
    }

    /// `org-bibtex-ask`: the value of `field`, nothing for an empty
    /// answer.
    fn ask_field(&mut self, field: &str) -> Result<Option<String>, Stop> {
        if !FIELDS.contains(&field) {
            return Err(Stop::Failed(format!("Field::{field} is not known")));
        }
        let answer = self.ask(&format!("{field}: "), None, Vec::new())?;
        Ok((!answer.is_empty()).then_some(answer))
    }

    /// `org-toggle-tag` with `on`.
    fn tag(&mut self, tag: &str) {
        let Some(heading) = properties::heading_at(&self.text, self.point) else { return };
        // `org--get-local-tags`, which keeps an empty tag (`:a::b:`).
        let line = &self.text[heading..line_end(&self.text, heading)];
        let mut current: Vec<String> = match crate::text::tag_group(line) {
            Some((_, start)) => {
                let group = line[start..].trim_end_matches([' ', '\t']);
                group[1..group.len() - 1].split(':').map(String::from).collect()
            }
            None => Vec::new(),
        };
        if !current.iter().any(|known| known == tag) {
            current.push(tag.to_string());
        }
        let edit = self.env.edit;
        let settings = crate::tags::Settings { tags_column: edit.todo.tags_column, ..edit.tags.clone() };
        let text = crate::tags::set_tags(&self.text, heading, &current, &settings);
        // `org-set-tags` deletes from the end of the title on, blanks and
        // tags, and inserts them again: a point there goes to the end of
        // the title.
        let end = line_end(&self.text, heading);
        let tags_from = crate::text::tag_group(line).map_or(line.len(), |(_, start)| start);
        let title_end = heading + line[..tags_from].trim_end_matches([' ', '\t']).len();
        let point = self.point;
        self.change(text);
        if point > title_end && point <= end {
            self.point = title_end;
        }
    }

    /// `org-insert-heading` at the point.
    fn insert_heading(&mut self) {
        let keywords = org_syntax::parse(&self.text, self.env.keywords).todo_keywords().clone();
        let outcome = meta_return::insert_heading_here(&self.text, self.point, &keywords, &self.env.edit.insert);
        self.text = outcome.text;
        self.point = outcome.cursor;
    }

    /// `org-indent-region` from `start` to the point.
    fn indent_region(&mut self, start: usize) {
        let line = self.text[..self.point].rfind('\n').map_or(0, |newline| newline + 1);
        let on_property = line < self.point && properties::property_at(&self.text, self.point).is_some();
        let indentation = self.text[line..].len() - self.text[line..].trim_start_matches([' ', '\t']).len();
        let indentation = self.text[line..line + indentation].to_string();
        let edits = indent::indent_region(&self.text, start, self.point, self.env.indent);
        let mut text = indent::apply(&self.text, &edits);
        self.point = self.point + text.len() - self.text.len();
        // A property line the point is in is aligned first, which takes
        // the end of the region (a marker at the point) to the start of
        // the line: it keeps its indentation, and the point goes there.
        if on_property {
            let start = text[..self.point].rfind('\n').map_or(0, |newline| newline + 1);
            let end = text[start..].find('\n').map_or(text.len(), |newline| start + newline);
            let aligned = format!("{indentation}{}", text[start..end].trim_start_matches([' ', '\t']));
            text.replace_range(start..end, &aligned);
            self.point = start;
        }
        self.text = text;
    }

    /// `org-bibtex-autokey`: the key, generated or asked for.
    fn autokey(&mut self) -> Result<(), Stop> {
        let key = if self.env.settings.autogen_keys {
            let entry = headline(&self.text, self.point, self.env).unwrap_or_default();
            (self.autokey)(&entry)?
        } else {
            self.ask("id: ", None, Vec::new())?
        };
        let property = self.env.settings.key_property.clone();
        self.put(&property, Some(&key), false)
    }

    /// `org-bibtex-fleshout`: the fields of `kind` the entry lacks, asked
    /// for, the title only without `treat_title`, the optional ones with
    /// `optional`; then the key.
    fn fleshout(&mut self, kind: &str, optional: bool, treat_title: bool) -> Result<(), Stop> {
        let Some((_, required, optionals)) = entry_type(kind) else { return Ok(()) };
        let fields = required
            .iter()
            .filter(|field| !(treat_title && matches!(field, One("title"))))
            .chain(optionals.iter().filter(|_| optional));
        for field in fields {
            let name = match field {
                One(name) => name.to_string(),
                Either(names) => match names.iter().find(|name| self.get(name).is_some()) {
                    Some(present) => present.to_string(),
                    None => {
                        let offered = names.iter().map(|name| name.to_string()).collect();
                        self.ask("Field: ", None, offered)?.to_lowercase()
                    }
                },
            };
            if self.get(&name).is_none() {
                if let Some(value) = self.ask_field(&name)? {
                    self.put(&name, Some(&value), false)?;
                }
            }
        }
        if self.get(&self.env.settings.key_property.clone()).is_none() {
            self.autokey()?;
        }
        Ok(())
    }

    fn done(self) -> Outcome {
        Outcome { text: self.text, cursor: self.point }
    }
}

/// The error of `org-back-to-heading` before the first headline, which
/// says where the line of `at` starts.
fn before_first_headline(text: &str, at: usize, env: &Env) -> Stop {
    let start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let position = text[..start].chars().count() + 1;
    Stop::Failed(format!("Before first headline at position {position} in buffer {}", env.buffer))
}

/// `org-bibtex-create`, or with `here` `org-bibtex-create-in-current-entry`:
/// a new heading at the cursor (or the entry at the cursor) with the
/// type asked for and its fields, the optional ones too with `optional`.
pub fn create(
    text: &str,
    at: usize,
    optional: bool,
    here: bool,
    answers: &[String],
    autokey: Autokey,
    env: &Env,
) -> Result<Outcome, Stop> {
    let mut run = Run { env, text: text.to_string(), point: at, answers, asked: 0, autokey };
    let settings = env.settings;
    let initial = if here { run.get(&settings.type_property_name) } else { None };
    let kind = run.ask("Type: ", initial, type_names())?;
    if entry_type(&kind).is_none() {
        return Err(Stop::Failed(format!("Type::{kind} is not known")));
    }
    if here {
        run.point = properties::heading_at(&run.text, run.point).ok_or_else(|| before_first_headline(text, at, env))?;
    } else {
        run.insert_heading();
        let Some(title) = run.ask_field("title")? else {
            return Err(Stop::Failed("Wrong type argument: char-or-string-p, nil".to_string()));
        };
        run.insert(&title);
        run.put("TITLE", Some(&title), false)?;
    }
    run.put(&settings.type_property_name, Some(&kind), false)?;
    run.fleshout(&kind, optional, !here)?;
    for tag in &settings.tags {
        run.tag(tag);
    }
    Ok(run.done())
}

/// `org-bibtex-check`: the fields the entry at the cursor lacks for its
/// type, asked for, the optional ones too with `optional`.
pub fn check(
    text: &str,
    at: usize,
    optional: bool,
    answers: &[String],
    autokey: Autokey,
    env: &Env,
) -> Result<Outcome, Stop> {
    // `org-narrow-to-subtree`
    if properties::heading_at(text, at).is_none() {
        return Err(Stop::Failed("Before first heading".to_string()));
    }
    let mut run = Run { env, text: text.to_string(), point: at, answers, asked: 0, autokey };
    if let Some(kind) = run.get(&env.settings.type_property_name) {
        run.fleshout(&kind, optional, env.settings.treat_headline_as_title)?;
    }
    Ok(run.done())
}

/// `org-bibtex-check-all`: `org-bibtex-check` on every headline.
pub fn check_all(text: &str, at: usize, answers: &[String], autokey: Autokey, env: &Env) -> Result<Outcome, Stop> {
    let mut run = Run { env, text: text.to_string(), point: at, answers, asked: 0, autokey };
    let count = headings(text, env).len();
    for index in 0..count {
        let Some(heading) = headings(&run.text, env).get(index).copied() else { break };
        // `org-map-entries` keeps the point where it was.
        let kept = run.point;
        run.point = heading;
        let before = run.text.clone();
        let outcome = if let Some(kind) = run.get(&env.settings.type_property_name) {
            run.fleshout(&kind, false, env.settings.treat_headline_as_title)
        } else {
            Ok(())
        };
        run.point = follow(&before, &run.text, kept);
        outcome?;
    }
    Ok(run.done())
}

/// An entry read from BibTeX: its fields in order, named in lower case,
/// `type` and `key` among them.
pub type Entry = Vec<(String, String)>;

/// `org-bibtex-read` on the entry `bibtex-beginning-of-entry` finds from
/// `at` in `text`: nothing when none of a known type starts there.
fn read_at(text: &str, at: usize) -> Result<Option<(usize, Entry)>, String> {
    let start = crate::cite::beginning_of_entry(text, at);
    let Some(fields) = crate::cite::parse_entry(text, start)? else { return Ok(None) };
    let mut entry = Vec::new();
    for (name, value) in fields {
        let name = match name.to_lowercase().as_str() {
            "=type=" => "type".to_string(),
            "=key=" => "key".to_string(),
            other => other.to_string(),
        };
        let Some(value) = value else { return Err("Wrong type argument: arrayp, nil".to_string()) };
        entry.push((name, clean_space(strip_delimiters(&value))));
    }
    Ok(Some((start, entry)))
}

/// What `org-bibtex-read` strips: enclosing `"…"`, then `{…}`.
fn strip_delimiters(text: &str) -> &str {
    let mut text = text;
    for (open, close) in [('"', '"'), ('{', '}')] {
        if text.chars().count() > 1 && text.starts_with(open) && text.ends_with(close) {
            text = &text[1..text.len() - 1];
        }
    }
    text
}

/// White space, newlines among it, as one space.
fn clean_space(text: &str) -> String {
    static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("a valid regex"));
    SPACE.replace_all(text, " ").into_owned()
}

/// `org-bibtex-read` on the last entry of `text`, which `org-bibtex-yank`
/// reads from the kill ring.
pub fn read_last(text: &str) -> Result<Option<Entry>, String> {
    Ok(read_at(text, text.len())?.map(|(_, entry)| entry))
}

/// `org-bibtex-read-buffer`: the entries of `text`, in order.
pub fn read_all(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut at = text.len();
    while at > 0 {
        at -= text[..at].chars().next_back().map_or(1, char::len_utf8);
        let found = read_at(text, at)?;
        let start = crate::cite::beginning_of_entry(text, at);
        if let Some((_, entry)) = found {
            entries.insert(0, entry);
        }
        at = start;
    }
    Ok(entries)
}

/// `org-tag--invalid-char-re` taken out of a keyword, blanks made `_`.
fn keyword_tag(keyword: &str) -> String {
    static BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").expect("a valid regex"));
    BLANKS
        .replace_all(keyword, "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%'))
        .collect()
}

impl Run<'_> {
    /// `org-bibtex-write`: `entry` as a new heading at the point, or with
    /// `here` added to the entry at the point; indented unless
    /// `noindent`.
    fn write(&mut self, entry: &Entry, here: bool, noindent: bool) -> Result<(), Stop> {
        let value = |field: &str| entry.iter().find(|(name, _)| name == field).map(|(_, value)| value.as_str());
        let raw = !here;
        if !here {
            self.insert_heading();
            // `org-bibtex-headline-format-default`
            let Some(title) = value("title") else {
                return Err(Stop::Failed("Wrong type argument: char-or-string-p, nil".to_string()));
            };
            self.insert(title);
            self.insert("\n:PROPERTIES:\n");
        }
        self.put("TITLE", value("title"), raw)?;
        let kind = value("type").map(str::to_lowercase);
        let type_property = self.env.settings.type_property_name.clone();
        self.put(&type_property, kind.as_deref(), raw)?;
        for (name, field) in entry {
            match name.as_str() {
                "title" | "type" => {}
                "key" => {
                    let key = self.env.settings.key_property.clone();
                    self.put(&key, Some(field), raw)?;
                }
                "keywords" if self.env.settings.tags_are_keywords => {
                    static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(", *").expect("a valid regex"));
                    for keyword in SEPARATOR.split(field) {
                        self.tag(&keyword_tag(keyword));
                    }
                }
                _ => self.put(name, Some(field), raw)?,
            }
        }
        if !here {
            self.insert(":END:\n");
        }
        for tag in self.env.settings.tags.clone() {
            self.tag(&tag);
        }
        if !noindent {
            let start = properties::heading_at(&self.text, self.point)
                .ok_or_else(|| before_first_headline(&self.text, self.point, self.env))?;
            self.indent_region(start);
        }
        Ok(())
    }
}

/// `org-bibtex-yank`: the last BibTeX entry of `clipboard` as a heading
/// at the cursor, or with `here` added to the entry at the cursor, where
/// an entry without a title asks for one (`answers`).
pub fn yank(
    text: &str,
    at: usize,
    clipboard: &str,
    here: bool,
    answers: &[String],
    env: &Env,
) -> Result<Outcome, Stop> {
    let Some(entry) = read_last(clipboard)? else {
        return Err(Stop::Failed("Yanked text does not appear to contain a BibTeX entry".to_string()));
    };
    let mut run = Run { env, text: text.to_string(), point: at, answers, asked: 0, autokey: &|_| Ok(String::new()) };
    run.write(&entry, here, false)?;
    Ok(run.done())
}

/// `org-bibtex-import-from-file`: the entries of `bib`, a file's text, as
/// headings after the cursor, and how many were read. Emacs reads the file
/// as raw bytes (`find-file-noselect` with RAWFILE), and puts what is not
/// ASCII in as those bytes; the file is UTF-8 here.
pub fn import(text: &str, at: usize, bib: &str, env: &Env) -> Result<(Outcome, usize), Stop> {
    static PROPERTY_END: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?mi)^[ \t]*:END:[ \t]*$").expect("a valid regex"));
    let entries = read_all(bib)?;
    let mut run =
        Run { env, text: text.to_string(), point: at, answers: &[], asked: 0, autokey: &|_| Ok(String::new()) };
    for entry in &entries {
        // `save-excursion`
        let (before, kept) = (run.text.clone(), run.point);
        run.write(entry, false, true)?;
        run.point = follow(&before, &run.text, kept);
        let Some(end) = PROPERTY_END.find_at(&run.text, run.point).map(|found| found.end()) else {
            return Err(Stop::Failed("Search failed: org-property-end-re".to_string()));
        };
        run.point = end;
        run.insert("\n");
    }
    // From where the point was, as a number.
    run.indent_region(at.min(run.text.len()));
    Ok((run.done(), entries.len()))
}
