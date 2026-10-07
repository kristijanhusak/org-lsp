//! The agenda's filters (`org-agenda-filter-apply`): lines are hidden by
//! their tags (`\`), their category (`<`), a regexp on their text (`=`),
//! their effort (`_`), or their top headline (`^`); `/` sets several at
//! once from a string (`org-agenda-filter`). Only lines of entries are
//! hidden: headers, days and the time grid stay.

use super::matcher::emacs_regex;
use super::{Line, View};
use crate::outline::Outline;

/// The filters of an agenda view: each a list of `+` or `-` and a value,
/// as Org keeps them (`org-agenda-tag-filter` and the others).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// `+work`, `-home`; `+` alone for any tag.
    pub tag: Vec<String>,
    /// Whether group tags stand for their groups (all but `C-u C-u C-u \`).
    pub expand_tags: bool,
    pub category: Vec<String>,
    pub regexp: Vec<String>,
    /// `+<0:30`, `->1:00`, `+=2:00`.
    pub effort: Vec<String>,
    /// The top headline, and whether its lines are the ones hidden.
    pub top_headline: Option<(String, bool)>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.tag.is_empty()
            && self.category.is_empty()
            && self.regexp.is_empty()
            && self.effort.is_empty()
            && self.top_headline.is_none()
    }

    /// What the filters are, as Emacs's mode line shows them.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        if !self.category.is_empty() {
            out.push_str(&format!("[{}]", self.category.join("")));
        }
        out.push_str(&self.tag.join(""));
        out.push_str(&self.effort.join(""));
        for regexp in &self.regexp {
            let (sign, rest) = regexp.split_at(regexp.chars().next().map_or(0, char::len_utf8));
            out.push_str(&format!("{sign}/{rest}/"));
        }
        // Emacs's mode line leaves the top headline out; it is said here
        // all the same.
        if let Some((headline, negative)) = &self.top_headline {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&format!("{}^{headline}", if *negative { "-" } else { "+" }));
        }
        out
    }
}

/// What a line of an entry is filtered by.
struct LineFacts<'a> {
    tags: Vec<&'a str>,
    category: &'a str,
    txt: &'a str,
    /// `effort-minutes`, or none.
    effort: Option<f64>,
    top: Option<String>,
}

/// Hide the lines of `view` that the filters keep out. `files` are those
/// the view was built from; `groups` the group tags of the agenda files.
pub fn apply(view: &mut View, files: &[&Outline], filters: &Filters, groups: &[(String, Vec<String>)]) {
    if filters.is_empty() {
        return;
    }
    let tag_form = tag_matcher(&filters.tag, filters.expand_tags, groups);
    let regexps: Vec<(bool, Option<regex::Regex>)> =
        filters.regexp.iter().map(|filter| (filter.starts_with('-'), emacs_regex(&filter[1..]))).collect();
    let efforts: Vec<(char, f64)> = filters.effort.iter().filter_map(|filter| effort_form(filter)).collect();
    view.lines.retain(|line| {
        let Some(facts) = facts(line, files) else { return true };
        // Tags.
        if !filters.tag.is_empty() && !tag_form.iter().all(|form| form.holds(&facts.tags)) {
            return false;
        }
        // Categories: any of several `+` ones, else all.
        if !filters.category.is_empty() {
            let positive = filters.category.iter().filter(|filter| filter.starts_with('+')).count();
            let holds = |filter: &String| {
                let equal = filter[1..] == *facts.category;
                if filter.starts_with('-') {
                    !equal
                } else {
                    equal
                }
            };
            let kept =
                if positive > 1 { filters.category.iter().any(holds) } else { filters.category.iter().all(holds) };
            if !kept {
                return false;
            }
        }
        // Regexps.
        for (negated, regex) in &regexps {
            let matched = regex.as_ref().is_some_and(|regex| regex.is_match(facts.txt));
            if matched == *negated {
                return false;
            }
        }
        // Efforts: none counts as the highest (`org-agenda-sort-noeffort-is-high`).
        // Org's form leaves the sign out: `-<1:00` keeps what `+<1:00` does.
        for (op, value) in &efforts {
            let effort = facts.effort.unwrap_or(f64::from(i32::MAX));
            let holds = match op {
                '<' => effort <= *value,
                '>' => effort >= *value,
                _ => effort == *value,
            };
            if !holds {
                return false;
            }
        }
        // The top headline.
        if let Some((headline, negative)) = &filters.top_headline {
            if let Some(top) = &facts.top {
                if (top == headline) == *negative {
                    return false;
                }
            }
        }
        true
    });
}

fn facts<'a>(line: &'a Line, files: &[&'a Outline]) -> Option<LineFacts<'a>> {
    let item = line.item.as_ref()?;
    let source = item.source?;
    let outline = files.get(source.file)?;
    let entry = outline.entries.get(source.entry)?;
    let effort = entry
        .properties
        .iter()
        .find(|(name, _)| name == "EFFORT")
        .and_then(|(_, value)| crate::clock::duration_to_minutes(value));
    Some(LineFacts {
        tags: entry.tags.iter().map(|tag| tag.name.as_str()).collect(),
        category: &entry.category,
        txt: &item.text[item.heading..],
        effort,
        top: top_headline(outline, source.entry),
    })
}

/// `org-find-top-headline`: the title of the entry's topmost ancestor,
/// a statistics cookie at its start taken out.
pub fn top_headline(outline: &Outline, index: usize) -> Option<String> {
    let mut top = index;
    while let Some(parent) = outline.entries.get(top)?.parent {
        top = parent;
    }
    let entry = &outline.entries[top];
    let line = &outline.text[entry.offset..entry.line_end];
    let (start, end) = crate::outline::complex_heading_title(line, entry.level, &outline.todo)?;
    let title = &line[start..end];
    static COOKIE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^\[[0-9]+/[0-9]+\] *|^\[%[0-9]+\] *").unwrap());
    Some(COOKIE.replace(title, "").into_owned())
}

/// One term of the tag filter: any (`+`) or all (`-`) of its tags.
struct TagForm {
    negated: bool,
    /// `None` for a line with any tag.
    tests: Vec<TagTest>,
}

enum TagTest {
    Any,
    Tag(String),
    Regex(Option<regex::Regex>),
}

impl TagForm {
    fn holds(&self, tags: &[&str]) -> bool {
        let test = |test: &TagTest| match test {
            TagTest::Any => !tags.is_empty(),
            TagTest::Tag(name) => tags.contains(&name.as_str()),
            TagTest::Regex(regex) => regex.as_ref().is_some_and(|regex| tags.iter().any(|tag| regex.is_match(tag))),
        };
        if self.negated {
            self.tests.iter().all(|t| !test(t))
        } else {
            self.tests.iter().any(test)
        }
    }
}

/// `org-agenda-filter-make-matcher` for tags: each term with its group
/// expanded (`org-agenda-filter-expand-tags`).
fn tag_matcher(filter: &[String], expand: bool, groups: &[(String, Vec<String>)]) -> Vec<TagForm> {
    let mut seen: Vec<&String> = Vec::new();
    let mut forms = Vec::new();
    for term in filter {
        if seen.contains(&term) {
            continue;
        }
        seen.push(term);
        let negated = term.starts_with('-');
        let tag = term.trim_start_matches(['+', '-']);
        let tags: Vec<String> = if expand && !groups.is_empty() && !tag.is_empty() {
            expand_tag(tag, groups)
        } else {
            vec![tag.to_string()]
        };
        let tests = tags
            .iter()
            .map(|tag| {
                if tag.is_empty() {
                    TagTest::Any
                } else if tag.len() > 2 && tag.starts_with('{') && tag.ends_with('}') {
                    TagTest::Regex(emacs_regex(&tag[1..tag.len() - 1]))
                } else {
                    TagTest::Tag(tag.clone())
                }
            })
            .collect();
        forms.push(TagForm { negated, tests });
    }
    forms
}

/// `(org-tags-expand tag t)`: the tag and those of its group.
fn expand_tag(tag: &str, groups: &[(String, Vec<String>)]) -> Vec<String> {
    let mut expanded: Vec<String> = Vec::new();
    let mut pending = vec![tag.to_string()];
    while let Some(next) = pending.pop() {
        if expanded.contains(&next) {
            continue;
        }
        if let Some((_, members)) = groups.iter().find(|(key, _)| *key == next) {
            pending.extend(members.iter().rev().cloned());
        }
        expanded.push(next);
    }
    expanded
}

/// `org-agenda-filter-effort-form`: the operator and the minutes; the
/// sign before them goes unread.
fn effort_form(filter: &str) -> Option<(char, f64)> {
    let rest = &filter[1..];
    let op = rest.chars().next()?;
    let value = crate::clock::duration_to_minutes(&rest[op.len_utf8()..]).unwrap_or(0.0);
    Some((op, value))
}

/// The tags of the view's lines, with the group tags they belong to
/// (`org-agenda-get-represented-tags`).
pub fn represented_tags(view: &View, files: &[&Outline], groups: &[(String, Vec<String>)]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for line in &view.lines {
        if let Some(facts) = facts(line, files) {
            for tag in facts.tags {
                if !tags.iter().any(|known| known == tag) {
                    tags.push(tag.to_string());
                }
            }
        }
    }
    let mut out = tags.clone();
    for tag in &tags {
        for (key, members) in groups {
            if members.contains(tag) {
                out.insert(0, key.clone());
            }
        }
    }
    out
}

/// The categories of the view's lines, those with a `-` in double quotes
/// (`org-agenda-get-represented-categories`).
pub fn represented_categories(view: &View, files: &[&Outline]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in &view.lines {
        if let Some(facts) = facts(line, files) {
            let category = if facts.category.contains('-') {
                format!("\"{}\"", facts.category)
            } else {
                facts.category.to_string()
            };
            if !out.contains(&category) {
                out.push(category);
            }
        }
    }
    out
}

/// `org-agenda-filter` (`/`): the filters a string sets, as
/// `+work-John<0:10-/plot/`. With `negate` (`C-u /`) each is turned
/// around; a string starting with `++` or `+-` adds to `current`.
pub fn parse(
    string: &str,
    negate: bool,
    current: &Filters,
    tags: &[String],
    categories: &[String],
) -> (Filters, Vec<String>) {
    let mut string = string.to_string();
    let keep = string.starts_with("++") || string.starts_with("+-");
    if keep {
        string.remove(0);
    }
    // `org-agenda-filter-remove-all` comes first: the top headline goes too.
    let mut filters = if keep { current.clone() } else { Filters::default() };
    filters.expand_tags = true;
    filters.top_headline = None;
    // A `-` inside double quotes is no operator.
    static QUOTED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#""([^"]*)-([^"]*)""#).unwrap());
    let string = QUOTED.replace_all(&string, "\"$1~~~$2\"").into_owned();
    static TERM: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[ \t]*([-+])?(?:([^-+<>=/ \t]+)|([<>=][0-9:]+)|(/([^/]+)/?))").unwrap()
    });
    let mut ignored = Vec::new();
    let mut rest = string.as_str();
    let push = |list: &mut Vec<String>, value: String| {
        if !list.contains(&value) {
            list.push(value);
        }
    };
    while let Some(found) = TERM.captures(rest) {
        let mut sign = found.get(1).map_or("+", |sign| sign.as_str()).to_string();
        if negate {
            sign = if sign == "+" { "-".to_string() } else { "+".to_string() };
        }
        if let Some(word) = found.get(2) {
            let word = word.as_str().replace("~~~", "-");
            if tags.contains(&word) {
                push(&mut filters.tag, format!("{sign}{word}"));
            } else if categories.contains(&word) {
                push(&mut filters.category, format!("{sign}{}", word.trim_matches('"')));
            } else {
                ignored.push(format!("`{sign}{word}' filter ignored because tag/category is not represented"));
            }
        } else if let Some(effort) = found.get(3) {
            push(&mut filters.effort, format!("{sign}{}", effort.as_str()));
        } else if let Some(regexp) = found.get(5) {
            push(&mut filters.regexp, format!("{sign}{}", regexp.as_str()));
        }
        rest = &rest[found.get(0).expect("a match").end()..];
    }
    (filters, ignored)
}

/// The string `/` starts with: the filters there are, without the first
/// `+` of the effort and regexp ones.
pub fn current_string(filters: &Filters) -> String {
    let effort = filters.effort.first().map_or(String::new(), |e| e.strip_prefix('+').unwrap_or(e).to_string());
    let regexp = filters.regexp.first().map_or(String::new(), |r| r.strip_prefix('+').unwrap_or(r).to_string());
    let regexp = if regexp.is_empty() { String::new() } else { format!("/{regexp}/") };
    format!("{}{}{effort}{regexp}", filters.category.join(""), filters.tag.join(""))
}
