//! org-roam's capture (`org-roam-capture-`) of a new node: a template
//! of `org-roam-capture-templates` filled with the node's title and
//! slug (`${title}`, `${slug}`, `${…}` asked for), its target made
//! (`file+head` writes the head of a new file), the node's `ID` put
//! where it goes, then the body placed as `org-capture` places it.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::capture::{self, Filled, Question};
use crate::properties;
use crate::read_date::Settings as ReadDateSettings;
use crate::refile::Settings as LevelSettings;
use crate::Refusal;

/// Where a template puts the node (`:target`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `(file PATH)`
    File(String),
    /// `(file+head PATH HEAD)`: HEAD starts a new file.
    FileHead(String, String),
    /// `(file+olp PATH OLP)`: under the outline path, made when missing.
    FileOlp(String, Vec<String>),
    /// `(file+head+olp PATH HEAD OLP)`
    FileHeadOlp(String, String, Vec<String>),
    /// `(node TITLE-OR-ID)`: in an existing node.
    Node(String),
}

/// A template of `org-roam-capture-templates`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub kind: capture::Kind,
    /// The template of the text, `%?` and all.
    pub body: String,
    pub target: Target,
    pub properties: capture::Properties,
    /// `:unnarrowed`: the capture buffer shows the whole file.
    pub unnarrowed: bool,
}

/// What a capture of a node needs besides its template.
pub struct Request<'a> {
    pub template: &'a Template,
    /// The new node's title and `ID`.
    pub title: &'a str,
    pub id: &'a str,
    /// The file of a node that exists, which is the target's file
    /// whatever the template says (`org-roam-capture--target-truepath`).
    pub file: Option<&'a Path>,
    /// The answers to the questions asked so far, in order.
    pub answers: &'a [String],
    /// What `org-capture-fill-template` fills in.
    pub context: &'a capture::Context,
    pub read_date: &'a ReadDateSettings,
    /// `org-roam-directory`
    pub directory: &'a Path,
    /// The text of a file, or `None` when there is no such file.
    pub read: &'a dyn Fn(&Path) -> Option<String>,
    /// How to read and change the properties of a file's text.
    pub properties: &'a dyn Fn(&str) -> properties::Context,
    pub levels: &'a LevelSettings,
    /// How a list item goes in.
    pub lists: &'a crate::list::Settings,
    /// The file and headline (or 0 for the file) of the node with this
    /// `ID`, title or alias.
    pub find_node: &'a dyn Fn(&str) -> Option<(PathBuf, usize)>,
}

/// Where a capture stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A question to ask before going on; its answer comes after the
    /// others.
    Ask(Question),
    Ready(Ready),
}

/// A capture ready to be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    /// The target file, and whether it is new.
    pub path: PathBuf,
    pub new_file: bool,
    /// The target with the template in it, and what the capture buffer
    /// shows of it.
    pub placed: capture::Placed,
    /// The node's `ID`: the one given, or the target's own.
    pub id: String,
}

/// The answers to the questions of a capture, taken in order: those of
/// `${…}` once per name, then those of each template's `%^{…}`.
struct Answers<'a> {
    list: &'a [String],
    used: usize,
    /// `org-roam-capture--info`: what each `${…}` was answered.
    named: Vec<(String, String)>,
}

/// `org-roam-capture-`: the template filled and placed, or the next
/// question.
pub fn capture(request: &Request) -> Result<Step, Refusal> {
    let mut answers = Answers { list: request.answers, used: 0, named: Vec::new() };
    let template = request.template;
    macro_rules! fill {
        ($text:expr, $newline:expr) => {
            match fill_template($text, request, &mut answers, $newline)? {
                Ok(text) => text,
                Err(question) => return Ok(Step::Ask(question)),
            }
        };
    }
    // `org-roam-capture--setup-target-location`
    let path_of = |text: String| -> PathBuf {
        let path = text.trim();
        let expanded = match path.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
            None => PathBuf::from(path),
        };
        request.directory.join(expanded)
    };
    let (path, mut text, new_file, point, olp): (PathBuf, String, bool, usize, Option<Vec<String>>) =
        match &template.target {
            Target::File(path) | Target::FileOlp(path, _) => {
                let path = match request.file {
                    Some(file) => file.to_path_buf(),
                    None => path_of(fill!(path, false)),
                };
                let existing = (request.read)(&path);
                let olp = match &template.target {
                    Target::FileOlp(_, olp) => Some(olp.clone()),
                    _ => None,
                };
                (path, existing.clone().unwrap_or_default(), existing.is_none(), 0, olp)
            }
            Target::FileHead(path, head) | Target::FileHeadOlp(path, head, _) => {
                let path = match request.file {
                    Some(file) => file.to_path_buf(),
                    None => path_of(fill!(path, false)),
                };
                let existing = (request.read)(&path);
                let text = match &existing {
                    Some(text) => text.clone(),
                    None => fill!(head, true),
                };
                let olp = match &template.target {
                    Target::FileHeadOlp(_, _, olp) => Some(olp.clone()),
                    _ => None,
                };
                (path, text, existing.is_none(), 0, olp)
            }
            Target::Node(name) => {
                let (path, point) = (request.find_node)(name)
                    .ok_or_else(|| Refusal::Failed(format!("No node with title or id \"{name}\"")))?;
                let text = (request.read)(&path).unwrap_or_default();
                (path, text, false, point, None)
            }
        };
    // The `ID` goes where the target starts; with an outline path, that
    // is still the start of the file.
    let id_point = point;
    let mut point = point;
    if let Some(olp) = olp {
        let mut filled = Vec::new();
        for heading in &olp {
            filled.push(fill!(heading, false));
        }
        let (edited, at) = find_or_create_olp(&text, &filled, &(request.properties)(&text))?;
        text = edited;
        point = at;
    }
    // The node's `ID`: the target's own, or the new one put there.
    let context = (request.properties)(&text);
    let id = match properties::entry_get(&text, id_point, "ID", &context) {
        Some(id) => id,
        None => {
            let before = text.len();
            text = properties::put(&text, id_point, "ID", request.id, &context).map_err(Refusal::Failed)?;
            // The drawer goes in before the capture's place.
            if point > id_point {
                point += text.len() - before;
            }
            request.id.to_string()
        }
    };
    let point = adjust_point(&text, point, template.kind, template.properties.prepend, &(request.properties)(&text));
    // The body: filled by org-roam, then again by `org-capture`.
    let body = fill!(&template.body, false);
    let (filled, _) = capture::fill_counting(&body, request.context, &[], request.read_date, None)?;
    let Filled::Text(body) = filled else {
        return Err(Refusal::Failed("A template asks after org-roam filled it".to_string()));
    };
    let keywords = (request.properties)(&text).keywords;
    let mut placed = capture::place(
        &text,
        &keywords,
        &capture::Target::Exact(point),
        template.kind,
        &body,
        &template.properties,
        request.levels,
        request.lists,
    )?;
    if template.unnarrowed {
        placed.unnarrow();
    }
    Ok(Step::Ready(Ready { path, new_file, placed, id }))
}

/// The `${name}` and `${name=default}` of a template.
static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{([^}]+)\}").expect("a valid regex"));

/// `org-roam-capture--fill-template`: `${…}` replaced by the node's
/// title, `ID` or slug, or what the user answers for each other name;
/// then `org-capture-fill-template`, and the blanks the template ended
/// with put back (a newline at least, with `ensure_newline`). `Err` is
/// the question to ask first.
fn fill_template(
    template: &str,
    request: &Request,
    answers: &mut Answers,
    ensure_newline: bool,
) -> Result<Result<String, Question>, Refusal> {
    let mut question = None;
    let mut failed = None;
    let replaced = FIELD.replace_all(template, |found: &regex::Captures| {
        if question.is_some() || failed.is_some() {
            return String::new();
        }
        // `(.+)=(.+)`: the last `=` splits a name from its default.
        let (name, default) = match found[1].rsplit_once('=') {
            Some((name, default)) if !name.is_empty() && !default.is_empty() => (name, Some(default)),
            _ => (&found[1], None),
        };
        match name {
            "title" | "formatted" => return request.title.to_string(),
            "id" => return request.id.to_string(),
            "slug" => return super::slugify(request.title),
            // The node's other fields, which a new node has none of.
            "file" | "file-title" | "file-hash" | "file-atime" | "file-mtime" | "level" | "point" | "todo"
            | "priority" | "scheduled" | "deadline" | "properties" | "olp" | "tags" | "aliases" | "refs"
            | "category" => {
                failed = Some("peculiar error".to_string());
                return String::new();
            }
            _ => {}
        }
        if let Some((_, value)) = answers.named.iter().find(|(known, _)| known == name) {
            return value.clone();
        }
        match answers.list.get(answers.used) {
            Some(answer) => {
                // The default is what the prompt starts with.
                answers.used += 1;
                answers.named.push((name.to_string(), answer.clone()));
                answer.clone()
            }
            None => {
                question = Some(Question::Text {
                    prompt: format!("{name}: "),
                    default: default.unwrap_or("").to_string(),
                    choices: Vec::new(),
                });
                String::new()
            }
        }
    });
    if let Some(message) = failed {
        return Err(Refusal::Failed(message));
    }
    if let Some(question) = question {
        return Ok(Err(question));
    }
    let trailing = template.len() - template.trim_end_matches([' ', '\t', '\n']).len();
    let mut whitespace = template[template.len() - trailing..].to_string();
    let rest = &answers.list[answers.used.min(answers.list.len())..];
    let (filled, used) = capture::fill_counting(&replaced, request.context, rest, request.read_date, None)?;
    answers.used += used;
    let filled = match filled {
        Filled::Text(text) => text,
        Filled::Ask(question) => return Ok(Err(question)),
    };
    if ensure_newline && whitespace.is_empty() {
        whitespace = "\n".to_string();
    }
    Ok(Ok(format!("{}{whitespace}", filled.trim_end_matches('\n'))))
}

/// `org-roam-capture-find-or-create-olp`: the headline at the outline
/// path `olp`, each level found below the one before or added at its
/// end. Fails when a level's title is not unique.
fn find_or_create_olp(text: &str, olp: &[String], context: &properties::Context) -> Result<(String, usize), Refusal> {
    let mut text = text.to_string();
    let (mut start, mut end) = (0, text.len());
    let (mut lmin, mut lmax) = (1, 1);
    let mut found = 0;
    for heading in olp {
        let matches = |text: &str, from: usize, to: usize| -> Vec<(usize, usize)> {
            let outline = crate::outline::Outline::new(&org_syntax::parse(text, &context.keywords), text, "");
            outline
                .entries
                .iter()
                .filter(|entry| entry.offset >= from && entry.offset < to)
                .filter(|entry| entry.level >= lmin && entry.level <= lmax)
                .filter(|entry| {
                    let line = &text[entry.offset..entry.line_end];
                    crate::outline::complex_heading_title(line, entry.level, &context.keywords)
                        .is_some_and(|(a, b)| line[a..b] == *heading)
                })
                .map(|entry| (entry.offset, entry.level))
                .collect()
        };
        let mut hits = matches(&text, start, end);
        if hits.len() > 1 {
            return Err(Refusal::Failed(format!("Heading not unique on level {lmax}: {heading}")));
        }
        if hits.is_empty() {
            // A headline of the level wanted, at the end of the parent.
            let mut insertion = String::new();
            if end > 0 && !text[..end].ends_with('\n') {
                insertion.push('\n');
            }
            insertion.push_str(&"*".repeat(lmin));
            insertion.push(' ');
            insertion.push_str(heading);
            let at = end;
            text.insert_str(at, &insertion);
            end = at + insertion.len();
            hits = matches(&text, start, end);
        }
        let Some(&(offset, level)) = hits.first() else {
            return Err(Refusal::Failed(format!("Heading not found on level {lmax}: {heading}")));
        };
        found = offset;
        lmin = level + 1;
        lmax = lmin;
        start = offset;
        end = crate::refile::subtree_end(&text, offset);
    }
    Ok((text, found))
}

/// `org-roam-capture--adjust-point-for-capture-type`: for plain text,
/// the end of the entry at `point` (the text before the first headline
/// at the start of the file), or with `:prepend` the start of its text.
fn adjust_point(text: &str, point: usize, kind: capture::Kind, prepend: bool, context: &properties::Context) -> usize {
    if kind != capture::Kind::Plain {
        return point;
    }
    let outline = crate::outline::Outline::new(&org_syntax::parse(text, &context.keywords), text, "");
    let next_heading =
        |from: usize| outline.entries.iter().find(|entry| entry.offset > from).map_or(text.len(), |entry| entry.offset);
    if point == 0 && !outline.entries.first().is_some_and(|entry| entry.offset == 0) {
        if !prepend {
            return next_heading(0);
        }
        // Past the drawers, keywords and comments that start the file.
        static META: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^(?:[ \t]*#\+\S*:.*|[ \t]*#(?: .*)?|[ \t]*:\S+:.*|[ \t]*-{5,}[ \t]*)$").expect("a valid regex")
        });
        let mut at = 0;
        while at < text.len() {
            let end = text[at..].find('\n').map_or(text.len(), |found| at + found + 1);
            if !META.is_match(text[at..end].trim_end_matches('\n')) {
                break;
            }
            at = end;
        }
        return at;
    }
    if prepend {
        return capture::end_of_meta_data(text, point, true);
    }
    next_heading(point)
}
