//! Completion, as Org's `pcomplete` offers it (`org-thing-at-point`), and
//! more for links:
//!
//! - `#+` keywords, and the values of `#+STARTUP:`; languages after
//!   `#+BEGIN_SRC`;
//! - in a link, after `[[`: link types and `#+LINK:` abbreviations; file
//!   names after `file:` (or `./`, `../`, `/`, `~/`), and that file's
//!   headlines after `::*`; this file's headlines after `*`; custom IDs
//!   after `#`;
//! - entities after a backslash (`\alpha`);
//! - TODO keywords for the first word after the stars, and tags after a
//!   colon in the headline's tags: the file's (`org-get-buffer-tags`),
//!   then those of the agenda files with
//!   `org-complete-tags-always-offer-all-agenda-tags`;
//! - after a colon at the start of a line, property names inside a
//!   property drawer, drawer names elsewhere.

use std::path::{Path, PathBuf};

use org_syntax::TodoKeywords;

/// What a completion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Tag,
    /// A `#+` keyword, a drawer or a property name.
    Name,
    /// A value, such as a `#+STARTUP:` option or a language.
    Value,
    LinkType,
    File,
    Folder,
    Heading,
    Entity,
    /// A citation key, from the bibliography.
    Citation,
}

/// One candidate: what the menu shows, and what it inserts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub kind: Kind,
    pub insert: String,
    /// More about it, next to the label.
    pub detail: Option<String>,
}

impl Candidate {
    fn new(label: impl Into<String>, kind: Kind) -> Candidate {
        let label = label.into();
        Candidate { insert: label.clone(), label, kind, detail: None }
    }

    fn inserting(label: impl Into<String>, kind: Kind, insert: impl Into<String>) -> Candidate {
        Candidate { label: label.into(), kind, insert: insert.into(), detail: None }
    }
}

/// What completion knows besides the text.
pub struct Context<'a> {
    pub keywords: &'a TodoKeywords,
    /// Tags offered after the file's own.
    pub more_tags: &'a [String],
    /// The directory of the file, for relative file names.
    pub directory: Option<&'a Path>,
    /// The entries of a directory: names, and whether each is one.
    pub list_directory: &'a dyn Fn(&Path) -> Vec<(String, bool)>,
    /// The text of another file, for its headlines.
    pub read_file: &'a dyn Fn(&Path) -> Option<String>,
    /// The keys of the bibliography, each with its line of the completion
    /// table (authors, year, title).
    pub citation_keys: &'a [(String, String)],
}

/// Whether the text before the cursor on its line (`before`) ends with
/// an `@` and the start of a key in a citation not closed yet: where the
/// key starts.
pub fn citation_key_start(before: &str) -> Option<usize> {
    let open = before.rfind("[cite")?;
    let inside = &before[open..];
    if inside.contains(']') || !inside.contains(':') {
        return None;
    }
    let is_key_char = |c: char| c.is_alphanumeric() || "-.:?!`'/*@+|(){}<>&_^$#%~".contains(c);
    let at = before.rfind('@')?;
    (at > open && before[at + 1..].chars().all(is_key_char)).then_some(at + 1)
}

/// Citation keys after `@` in a citation (`org-cite-insert`'s table): the
/// key, with its authors, year and title beside it.
fn citation(before: &str, context: &Context) -> Option<(usize, Vec<Candidate>)> {
    let start = citation_key_start(before)?;
    let candidates = context
        .citation_keys
        .iter()
        .map(|(key, line)| Candidate { detail: Some(line.clone()), ..Candidate::new(key.clone(), Kind::Citation) })
        .collect();
    Some((start, candidates))
}

/// The candidates at byte `offset` of `text`, and where the word they
/// replace starts.
pub fn complete(text: &str, offset: usize, context: &Context) -> Option<(usize, Vec<Candidate>)> {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |index| offset + index);
    let before = &text[start..offset];
    let after_blank = text[offset..end].trim().is_empty();
    let found = citation(before, context)
        .or_else(|| keyword_line(before, after_blank))
        .or_else(|| link(text, before, context))
        .or_else(|| entity(before))
        .or_else(|| headline(text, before, context))
        .or_else(|| drawer_or_property(text, start, before, after_blank))?;
    let (from, candidates) = found;
    Some((start + from, candidates))
}

/// Candidates in the case of what was typed: lower case when it is.
fn in_case(typed: &str, name: &str) -> String {
    if typed.chars().any(char::is_lowercase) {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

// ----- #+ keywords

/// `org-options-keywords`
const OPTIONS: &[&str] = &[
    "ARCHIVE",
    "AUTHOR",
    "BIBLIOGRAPHY",
    "BIND",
    "CATEGORY",
    "CITE_EXPORT",
    "COLUMNS",
    "CREATOR",
    "DATE",
    "DESCRIPTION",
    "DRAWERS",
    "EMAIL",
    "EXCLUDE_TAGS",
    "FILETAGS",
    "INCLUDE",
    "INDEX",
    "KEYWORDS",
    "LANGUAGE",
    "MACRO",
    "OPTIONS",
    "PROPERTY",
    "PRINT_BIBLIOGRAPHY",
    "PRIORITIES",
    "SELECT_TAGS",
    "SEQ_TODO",
    "SETUPFILE",
    "STARTUP",
    "TAGS",
    "TITLE",
    "TODO",
    "TYP_TODO",
    "EXPORT_FILE_NAME",
];

/// `org-element-affiliated-keywords`
const AFFILIATED: &[&str] = &[
    "CAPTION", "DATA", "HEADER", "HEADERS", "LABEL", "NAME", "PLOT", "RESNAME", "RESULT", "RESULTS", "SOURCE",
    "SRCNAME", "TBLNAME",
];

/// The blocks of `org-block-keywords`.
const BLOCKS: &[&str] = &["CENTER", "COMMENT", "EXAMPLE", "EXPORT", "QUOTE", "SRC", "VERSE"];

/// `org-startup-options`
const STARTUP: &[&str] = &[
    "fold",
    "overview",
    "nofold",
    "showall",
    "show2levels",
    "show3levels",
    "show4levels",
    "show5levels",
    "showeverything",
    "content",
    "indent",
    "noindent",
    "num",
    "nonum",
    "hidestars",
    "showstars",
    "odd",
    "oddeven",
    "align",
    "noalign",
    "shrink",
    "descriptivelinks",
    "literallinks",
    "inlineimages",
    "noinlineimages",
    "linkpreviews",
    "nolinkpreviews",
    "latexpreview",
    "nolatexpreview",
    "customtime",
    "logdone",
    "lognotedone",
    "nologdone",
    "lognoteclock-out",
    "nolognoteclock-out",
    "logrepeat",
    "lognoterepeat",
    "logdrawer",
    "nologdrawer",
    "logstatesreversed",
    "nologstatesreversed",
    "nologrepeat",
    "logreschedule",
    "lognotereschedule",
    "nologreschedule",
    "logredeadline",
    "lognoteredeadline",
    "nologredeadline",
    "logrefile",
    "lognoterefile",
    "nologrefile",
    "fninline",
    "nofninline",
    "fnlocal",
    "fnauto",
    "fnprompt",
    "fnconfirm",
    "fnplain",
    "fnadjust",
    "nofnadjust",
    "fnanon",
    "constcgs",
    "constSI",
    "noptag",
    "hideblocks",
    "nohideblocks",
    "hidedrawers",
    "nohidedrawers",
    "beamer",
    "entitiespretty",
    "entitiesplain",
];

/// The languages of `org-babel-load-languages`.
const LANGUAGES: &[&str] = &[
    "awk",
    "C",
    "R",
    "calc",
    "clojure",
    "css",
    "ditaa",
    "dot",
    "emacs-lisp",
    "eshell",
    "forth",
    "fortran",
    "gnuplot",
    "groovy",
    "haskell",
    "java",
    "js",
    "julia",
    "latex",
    "lilypond",
    "lisp",
    "lua",
    "makefile",
    "maxima",
    "ocaml",
    "octave",
    "org",
    "perl",
    "processing",
    "plantuml",
    "python",
    "ruby",
    "sass",
    "scheme",
    "screen",
    "sed",
    "shell",
    "sql",
    "sqlite",
];

fn keyword_line(before: &str, after_blank: bool) -> Option<(usize, Vec<Candidate>)> {
    let indent = before.len() - before.trim_start_matches([' ', '\t']).len();
    let rest = before[indent..].strip_prefix("#+").or_else(|| before[indent..].strip_prefix("#+"))?;
    // A keyword's value.
    if let Some(colon) = rest.find(':') {
        let name = &rest[..colon];
        let value = &rest[colon + 1..];
        let word_start = value.rfind([' ', '\t']).map_or(0, |at| at + 1);
        let word = &value[word_start..];
        let from = indent + 2 + colon + 1 + word_start;
        if name.eq_ignore_ascii_case("STARTUP") {
            let used: Vec<&str> = value.split_whitespace().collect();
            let candidates = STARTUP
                .iter()
                .filter(|option| !used[..used.len().saturating_sub(usize::from(!word.is_empty()))].contains(option))
                .map(|option| Candidate::new(*option, Kind::Value))
                .collect();
            return Some((from, candidates));
        }
        return None;
    }
    // The language of a source block.
    if let Some(arguments) = rest
        .strip_prefix("BEGIN_SRC ")
        .or_else(|| rest.strip_prefix("begin_src "))
        .filter(|arguments| !arguments.contains([' ', '\t']))
    {
        let from = before.len() - arguments.len();
        return Some((from, LANGUAGES.iter().map(|language| Candidate::new(*language, Kind::Value)).collect()));
    }
    // A keyword's name, with nothing after it.
    if !after_blank || !rest.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let mut candidates = Vec::new();
    let name = |name: &str, insert: &str| Candidate::inserting(in_case(rest, name), Kind::Name, in_case(rest, insert));
    for option in OPTIONS {
        candidates.push(name(&format!("{option}:"), &format!("{option}: ")));
    }
    for keyword in AFFILIATED {
        candidates.push(name(&format!("{keyword}:"), &format!("{keyword}: ")));
    }
    for block in BLOCKS {
        let space = if matches!(*block, "EXPORT" | "SRC") { " " } else { "" };
        candidates.push(name(&format!("BEGIN_{block}"), &format!("BEGIN_{block}{space}")));
        candidates.push(name(&format!("END_{block}"), &format!("END_{block}")));
        candidates.push(name(&format!("ATTR_{block}:"), &format!("ATTR_{block}: ")));
    }
    Some((indent + 2, candidates))
}

// ----- links

/// `org-link-parameters`: the link types Org knows.
const LINK_TYPES: &[&str] = &[
    "attachment",
    "bbdb",
    "bibtex",
    "docview",
    "doi",
    "elisp",
    "eshell",
    "eww",
    "file",
    "file+emacs",
    "file+sys",
    "gnus",
    "help",
    "http",
    "https",
    "id",
    "info",
    "irc",
    "mailto",
    "man",
    "mhe",
    "news",
    "rmail",
    "shell",
    "shortdoc",
    "w3m",
];

fn link(text: &str, before: &str, context: &Context) -> Option<(usize, Vec<Candidate>)> {
    // The target of a link that is still open: after `[[`, before `]`.
    let open = before.rfind("[[")?;
    let target = &before[open + 2..];
    if target.contains(']') {
        return None;
    }
    let from = open + 2;
    if let Some(heading) = target.strip_prefix('*') {
        let _ = heading;
        return Some((from + 1, headings(text, context.keywords)));
    }
    if target.starts_with('#') {
        return Some((from + 1, custom_ids(text)));
    }
    let path = target
        .strip_prefix("file:")
        .or_else(|| ["./", "../", "/", "~/"].iter().any(|prefix| target.starts_with(prefix)).then_some(target));
    if let Some(path) = path {
        let path_from = from + (target.len() - path.len());
        // `file:notes.org::*`: that file's headlines.
        if let Some((file, search)) = path.split_once("::") {
            let heading = search.strip_prefix('*')?;
            let _ = heading;
            let found = resolve(file, context.directory)?;
            let other = (context.read_file)(&found)?;
            let keywords = TodoKeywords::from_document(&other).unwrap_or_else(|| context.keywords.clone());
            return Some((path_from + file.len() + 3, headings(&other, &keywords)));
        }
        let (directory, _name) = match path.rfind('/') {
            Some(slash) => (&path[..=slash], &path[slash + 1..]),
            None => ("", path),
        };
        let listed = resolve(if directory.is_empty() { "." } else { directory }, context.directory)?;
        let mut entries = (context.list_directory)(&listed);
        entries.sort();
        let candidates = entries
            .into_iter()
            .map(|(name, folder)| {
                if folder {
                    Candidate::inserting(format!("{name}/"), Kind::Folder, format!("{name}/"))
                } else {
                    Candidate::new(name, Kind::File)
                }
            })
            .collect();
        return Some((path_from + directory.len(), candidates));
    }
    if target.contains(':') {
        return None;
    }
    // A link type or an abbreviation (`#+LINK: name url`).
    let mut names: Vec<String> = LINK_TYPES.iter().map(|name| name.to_string()).collect();
    for line in text.lines() {
        let line = line.trim_start();
        let Some(value) = line.get(..7).filter(|start| start.eq_ignore_ascii_case("#+LINK:")).map(|_| &line[7..])
        else {
            continue;
        };
        if let Some(name) = value.split_whitespace().next() {
            if !names.iter().any(|known| known == name) {
                names.push(name.to_string());
            }
        }
    }
    Some((
        from,
        names
            .into_iter()
            .map(|name| Candidate::inserting(format!("{name}:"), Kind::LinkType, format!("{name}:")))
            .collect(),
    ))
}

/// A file name as written in a link: from the home directory (`~/`), the
/// root, or the directory of the file.
fn resolve(name: &str, directory: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = name.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        return Some(PathBuf::from(home).join(rest));
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(directory?.join(path))
}

/// `org-link-heading-search-string` of every headline: its title without
/// keyword, priority, `COMMENT`, tags or statistics cookies.
fn headings(text: &str, keywords: &TodoKeywords) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for line in text.lines() {
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        if stars == 0 || !line[stars..].starts_with([' ', '\t']) {
            continue;
        }
        let mut title = line[stars..].trim();
        if let Some(word) = title.split([' ', '\t']).next() {
            if keywords.names().any(|keyword| keyword == word) {
                title = title[word.len()..].trim_start();
            }
        }
        if title.starts_with("[#") {
            if let Some(end) = title.find(']') {
                title = title[end + 1..].trim_start();
            }
        }
        if let Some(rest) = title.strip_prefix("COMMENT").filter(|rest| rest.is_empty() || rest.starts_with(' ')) {
            title = rest.trim_start();
        }
        if let Some((rest, tags)) = title.rsplit_once([' ', '\t']) {
            if tags.len() > 1 && tags.starts_with(':') && tags.ends_with(':') {
                title = rest;
            }
        }
        let words: Vec<&str> = title.split_whitespace().filter(|word| !is_cookie(word)).collect();
        let heading = words.join(" ");
        if !heading.is_empty() && !out.iter().any(|known| known.label == heading) {
            out.push(Candidate::new(heading, Kind::Heading));
        }
    }
    out
}

/// A statistics cookie: `[1/3]`, `[50%]`.
fn is_cookie(word: &str) -> bool {
    let Some(inner) = word.strip_prefix('[').and_then(|word| word.strip_suffix(']')) else { return false };
    let digits = |text: &str| text.chars().all(|c| c.is_ascii_digit());
    match inner.split_once('/') {
        Some((done, total)) => digits(done) && digits(total),
        None => inner.strip_suffix('%').is_some_and(digits),
    }
}

/// The `CUSTOM_ID` properties of the file.
fn custom_ids(text: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let Some(rest) = line.get(..11).filter(|start| start.eq_ignore_ascii_case(":CUSTOM_ID:")).map(|_| &line[11..])
        else {
            continue;
        };
        let id = rest.trim();
        if !id.is_empty() && !out.iter().any(|known| known.label == id) {
            out.push(Candidate::new(id, Kind::Heading));
        }
    }
    out
}

// ----- entities

fn entity(before: &str) -> Option<(usize, Vec<Candidate>)> {
    // Letters, then digits, after a backslash.
    let digits = before.len() - before.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let letters_end = before.len() - digits;
    let letters = letters_end - before[..letters_end].trim_end_matches(|c: char| c.is_ascii_alphabetic()).len();
    let backslash = letters_end - letters;
    if backslash == 0 || before.as_bytes()[backslash - 1] != b'\\' {
        return None;
    }
    let candidates = org_syntax::entity_names().iter().map(|name| Candidate::new(*name, Kind::Entity)).collect();
    Some((backslash, candidates))
}

// ----- headlines: keywords and tags

fn headline(text: &str, before: &str, context: &Context) -> Option<(usize, Vec<Candidate>)> {
    let stars = before.bytes().take_while(|b| *b == b'*').count();
    if stars == 0 || before.as_bytes().get(stars) != Some(&b' ') {
        return None;
    }
    let rest = &before[stars..];
    // The first word after the stars: a TODO keyword.
    let word = rest.trim_start_matches(' ');
    if !word.contains([' ', '\t', ':']) {
        let words = context
            .keywords
            .names()
            .filter(|name| name.starts_with(word))
            .map(|name| Candidate::new(name, Kind::Keyword));
        return Some((before.len() - word.len(), words.collect()));
    }
    // A tag: the part after the last colon of a tag group that starts
    // after a blank and runs to the cursor.
    let colon = before.rfind(':')?;
    let group_start = before[..=colon].rfind([' ', '\t']).map(|blank| blank + 1)?;
    let group = &before[group_start..];
    let tag = &before[colon + 1..];
    let valid = |s: &str| s.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%' | ':'));
    if !group.starts_with(':') || !valid(group) {
        return None;
    }
    let present: Vec<&str> = group.split(':').filter(|tag| !tag.is_empty()).collect();
    let mut tags = buffer_tags(text);
    let mut more: Vec<String> = context.more_tags.iter().filter(|tag| !tags.contains(tag)).cloned().collect();
    tags.append(&mut more);
    tags.retain(|candidate| candidate.starts_with(tag) && !present.contains(&candidate.as_str()));
    Some((before.len() - tag.len(), tags.into_iter().map(|tag| Candidate::new(tag, Kind::Tag)).collect()))
}

/// `org-get-buffer-tags`: the tags of every headline, sorted, each once.
fn buffer_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with('*'))
        .filter_map(|line| {
            let trimmed = line.trim_end();
            let group = trimmed.rsplit([' ', '\t']).next()?;
            (group.len() > 1 && group.starts_with(':') && group.ends_with(':')).then_some(group)
        })
        .flat_map(|group| group.split(':').filter(|tag| !tag.is_empty()).map(str::to_string))
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

// ----- drawers and properties

/// `:NAME:` on a line of its own, `NAME` being a drawer's.
fn drawer_name(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix(':')?.strip_suffix(':')?;
    (!inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))).then_some(inner)
}

/// `:NAME: value`: a property line, and its name.
fn property_name(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix(':')?;
    let end = rest.find(':')?;
    let name = &rest[..end];
    let after = &rest[end + 1..];
    let name = name.strip_suffix('+').unwrap_or(name);
    (!name.is_empty() && !name.contains(char::is_whitespace) && (after.is_empty() || after.starts_with([' ', '\t'])))
        .then_some(name)
}

fn drawer_or_property(
    text: &str,
    line_start: usize,
    before: &str,
    after_blank: bool,
) -> Option<(usize, Vec<Candidate>)> {
    let indent = before.len() - before.trim_start_matches([' ', '\t']).len();
    let typed = before[indent..].strip_prefix(':')?;
    if !after_blank || typed.contains(char::is_whitespace) {
        return None;
    }
    let from = indent + 1;
    // Inside a property drawer: the lines above, up to `:PROPERTIES:`,
    // are properties.
    let mut present = Vec::new();
    let mut in_properties = false;
    for line in text[..line_start].lines().rev() {
        if drawer_name(line).is_some_and(|name| name.eq_ignore_ascii_case("PROPERTIES")) {
            in_properties = true;
            break;
        }
        match property_name(line) {
            Some(name) if !name.eq_ignore_ascii_case("END") => present.push(name.to_uppercase()),
            _ => break,
        }
    }
    let name =
        |label: &str, insert: &str, kind| Candidate::inserting(in_case(typed, label), kind, in_case(typed, insert));
    if in_properties {
        // `org-buffer-property-keys`: every property of the file, but not
        // those this drawer has already.
        let mut keys: Vec<String> = Vec::new();
        let mut inside = false;
        for line in text.lines() {
            match drawer_name(line).map(str::to_uppercase).as_deref() {
                Some("PROPERTIES") => inside = true,
                Some("END") => inside = false,
                _ if inside => {
                    if let Some(key) = property_name(line).map(str::to_uppercase) {
                        if !keys.contains(&key) && !present.contains(&key) {
                            keys.push(key);
                        }
                    }
                }
                _ => {}
            }
        }
        keys.sort();
        let mut candidates: Vec<Candidate> =
            keys.iter().map(|key| name(&format!("{key}:"), &format!("{key}: "), Kind::Name)).collect();
        candidates.push(name("END:", "END:", Kind::Name));
        return Some((from, candidates));
    }
    // Drawer names: `PROPERTIES`, `LOGBOOK` and those of the file.
    let mut names = vec!["PROPERTIES".to_string(), "LOGBOOK".to_string()];
    for line in text.lines() {
        if let Some(found) = drawer_name(line).map(str::to_uppercase) {
            if found != "END" && !names.contains(&found) {
                names.push(found);
            }
        }
    }
    names.push("END".to_string());
    Some((from, names.iter().map(|drawer| name(&format!("{drawer}:"), &format!("{drawer}:"), Kind::Name)).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(keywords: &'a TodoKeywords, more: &'a [String]) -> Context<'a> {
        Context {
            keywords,
            more_tags: more,
            directory: Some(Path::new("/notes")),
            list_directory: &|path: &Path| match path.to_str() {
                Some("/notes/.") | Some("/notes") => {
                    vec![("b.org".into(), false), ("a.org".into(), false), ("sub".into(), true)]
                }
                Some("/notes/sub/") => vec![("deep.org".into(), false)],
                _ => Vec::new(),
            },
            read_file: &|path: &Path| {
                (path == Path::new("/notes/a.org")).then(|| "* TODO First [1/2] :x:\n** Second\n".to_string())
            },
            citation_keys: &[],
        }
    }

    /// The candidates where `¦` is, and where the replaced word starts.
    fn at_with(marked: &str, more: &[String]) -> Option<(usize, Vec<String>)> {
        let offset = marked.find('¦').unwrap();
        let text = marked.replacen('¦', "", 1);
        let keywords = TodoKeywords::from_document(&text).unwrap_or_default();
        complete(&text, offset, &context(&keywords, more))
            .map(|(start, items)| (start, items.into_iter().map(|item| item.insert).collect()))
    }

    fn at(marked: &str) -> Option<(usize, Vec<String>)> {
        at_with(marked, &[])
    }

    #[test]
    fn keywords_after_the_stars() {
        assert_eq!(at("* ¦"), Some((2, vec!["TODO".into(), "DONE".into()])));
        assert_eq!(at("** D¦"), Some((3, vec!["DONE".into()])));
        assert_eq!(at("#+TODO: NEXT | DONE\n* N¦"), Some((22, vec!["NEXT".into()])));
        assert_eq!(at("text ¦"), None);
        assert_eq!(at("*bold¦"), None);
    }

    #[test]
    fn tags_of_the_file() {
        let file = "* a :work:home:\n* b :@phone:\n* TODO c :¦\n";
        assert_eq!(at(file), Some((39, vec!["@phone".into(), "home".into(), "work".into()])));
        let file = "* a :work:home:\n* b :wo¦\n";
        assert_eq!(at(file), Some((21, vec!["work".into()])));
        let file = "* a :work:home:\n* b :home:¦\n";
        assert_eq!(at(file), Some((26, vec!["work".into()])));
        assert_eq!(at("* a :work:\n* b text¦"), None);
    }

    #[test]
    fn then_the_agenda_tags() {
        let more = ["home".to_string(), "work".to_string(), "@phone".to_string()];
        assert_eq!(at_with("* a :work:\n* b :¦\n", &more).unwrap().1, ["work", "home", "@phone"]);
    }

    #[test]
    fn keywords_after_hash_plus() {
        let (start, found) = at("#+TI¦").unwrap();
        assert_eq!(start, 2);
        assert!(found.contains(&"TITLE: ".to_string()) && found.contains(&"BEGIN_SRC ".to_string()));
        assert!(found.contains(&"BEGIN_QUOTE".to_string()) && found.contains(&"ATTR_SRC: ".to_string()));
        // In the case typed.
        assert!(at("  #+ti¦").unwrap().1.contains(&"title: ".to_string()));
        // Not with text after the cursor, nor in a value.
        assert_eq!(at("#+TI¦TLE: x"), None);
        assert_eq!(at("#+TITLE: x¦"), None);
    }

    #[test]
    fn values_of_startup_and_languages() {
        let (start, found) = at("#+STARTUP: overview ind¦").unwrap();
        assert_eq!(start, 20);
        assert!(found.contains(&"indent".to_string()) && !found.contains(&"overview".to_string()));
        let (start, found) = at("#+begin_src py¦").unwrap();
        assert_eq!(start, 12);
        assert!(found.contains(&"python".to_string()));
        assert_eq!(at("#+begin_src python :resul¦"), None);
    }

    #[test]
    fn links() {
        let (start, found) = at("See [[ht¦").unwrap();
        assert_eq!(start, 6);
        assert!(found.contains(&"https:".to_string()) && found.contains(&"file:".to_string()));
        let found = at("#+LINK: gh https://github.com/%s\n[[¦").unwrap().1;
        assert!(found.contains(&"gh:".to_string()));
        assert_eq!(at("[[https://x.org][desc¦"), None);
        assert_eq!(at("[[x]] and ¦"), None);
    }

    #[test]
    fn file_names_and_their_headlines() {
        assert_eq!(at("[[file:¦"), Some((7, vec!["a.org".into(), "b.org".into(), "sub/".into()])));
        assert_eq!(at("[[file:sub/de¦"), Some((11, vec!["deep.org".into()])));
        assert_eq!(at("[[./¦").unwrap().0, 4);
        assert_eq!(at("[[file:a.org::*¦"), Some((15, vec!["First".into(), "Second".into()])));
    }

    #[test]
    fn headlines_and_custom_ids_of_the_file() {
        let file = "* TODO [#A] Write it [50%] :tag:\n:PROPERTIES:\n:CUSTOM_ID: writing\n:END:\n* COMMENT Old\n[[*W¦";
        assert_eq!(at(file).unwrap().1, ["Write it", "Old"]);
        assert_eq!(at(&file.replace("[[*W¦", "[[#¦")).unwrap().1, ["writing"]);
    }

    #[test]
    fn entities() {
        let (start, found) = at("Greek \\alp¦").unwrap();
        assert_eq!(start, 7);
        assert!(found.contains(&"alpha".to_string()));
        assert_eq!(at("no backslash alp¦"), None);
    }

    #[test]
    fn drawers_and_properties() {
        let (start, found) = at("* a\n:¦").unwrap();
        assert_eq!(start, 5);
        assert_eq!(found, ["PROPERTIES:", "LOGBOOK:", "END:"]);
        assert_eq!(at("* a\n:NOTES:\n:END:\n* b\n  :lo¦").unwrap().1, ["properties:", "logbook:", "notes:", "end:"]);
        let file = "* a\n:PROPERTIES:\n:EFFORT: 1\n:OWNER: me\n:END:\n* b\n:PROPERTIES:\n:OWNER: you\n:¦";
        assert_eq!(at(file).unwrap().1, ["EFFORT: ", "END:"]);
        assert_eq!(at("* a\n:PROPERTIES: x¦"), None);
    }
}
