//! The files the agenda is built from.
//!
//! `org_agenda_files` lists files, directories and glob patterns, like Org's
//! `org-agenda-files`. Without it, the Org files under the workspace
//! folders are used. Files are read from disk and read again when their
//! modification time changes; a document that is open in the editor is
//! taken from the editor instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use org_core::outline::{Inheritance, Outline};
use org_syntax::{Parse, TodoKeywords};

use crate::line_index::LineIndex;

/// Directory levels searched below a workspace folder.
const MAX_DEPTH: usize = 8;
/// Files taken from workspace folders at most.
const MAX_FILES: usize = 2000;

/// A file read from disk.
#[derive(Debug)]
pub struct Source {
    pub text: String,
    pub index: LineIndex,
    pub parse: Parse,
    pub outline: Outline,
}

impl Source {
    pub fn new(text: String, name: &str, todo: &TodoKeywords, inheritance: &Inheritance) -> Source {
        let parse = org_syntax::parse(&text, todo);
        let outline = Outline::with_inheritance(&parse, &text, name, inheritance);
        Source { index: LineIndex::new(&text), text, parse, outline }
    }
}

struct Cached {
    modified: Option<SystemTime>,
    len: u64,
    source: Rc<Source>,
}

#[derive(Default)]
pub struct Workspace {
    /// `org_agenda_files`, as configured.
    patterns: Vec<String>,
    folders: Vec<PathBuf>,
    /// `org-agenda-file-regexp`: which files of a directory listed are
    /// agenda files; unset, the `.org` files that are not hidden.
    pub file_regexp: Option<regex::Regex>,
    cache: HashMap<PathBuf, Cached>,
    /// What entries inherit, for the outlines of the files read.
    pub inheritance: Inheritance,
}

fn is_org_file(path: &Path) -> bool {
    let hidden = path.file_name().is_none_or(|name| name.to_string_lossy().starts_with('.'));
    !hidden && path.extension().is_some_and(|extension| extension == "org")
}

fn has_wildcard(text: &str) -> bool {
    text.contains(['*', '?'])
}

/// Whether `name` matches `pattern`, where `*` stands for any run of
/// characters and `?` for one.
fn matches(pattern: &str, name: &str) -> bool {
    let (pattern, name): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    // Positions in `pattern` that the name read so far can have reached.
    let mut reached = vec![false; pattern.len() + 1];
    reached[0] = true;
    let spread = |reached: &mut Vec<bool>| {
        for at in 0..pattern.len() {
            if reached[at] && pattern[at] == '*' {
                reached[at + 1] = true;
            }
        }
    };
    spread(&mut reached);
    for c in name {
        let mut next = vec![false; pattern.len() + 1];
        for at in 0..pattern.len() {
            if reached[at] {
                match pattern[at] {
                    '*' => next[at] = true,
                    '?' => next[at + 1] = true,
                    literal if literal == c => next[at + 1] = true,
                    _ => {}
                }
            }
        }
        spread(&mut next);
        reached = next;
    }
    reached[pattern.len()]
}

fn sorted_entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(directory).into_iter().flatten().flatten().map(|entry| entry.path()).collect();
    entries.sort();
    entries
}

fn is_hidden(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// The Org files directly in `directory`, as `org-agenda-files` reads a
/// directory.
fn org_files_in(directory: &Path, regexp: Option<&regex::Regex>) -> Vec<PathBuf> {
    let wanted = |path: &Path| match regexp {
        // `org-agenda-file-regexp`, against the file's name.
        Some(regexp) => path.file_name().is_some_and(|name| regexp.is_match(&name.to_string_lossy())),
        None => is_org_file(path),
    };
    sorted_entries(directory).into_iter().filter(|path| path.is_file() && wanted(path)).collect()
}

/// The Org files below `directory`, skipping hidden directories and the
/// usual dependency and build directories.
fn org_files_below(directory: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for path in sorted_entries(directory) {
        if out.len() >= MAX_FILES {
            return;
        }
        if path.is_dir() {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            if depth < MAX_DEPTH && !is_hidden(&path) && !matches!(name.as_str(), "node_modules" | "target") {
                org_files_below(&path, depth + 1, out);
            }
        } else if is_org_file(&path) {
            out.push(path);
        }
    }
}

/// The paths matching the glob components `parts` below `base`.
/// `seen`: the directories each `**` went into (by how much of the
/// pattern is left), through their links, so that a link to a directory
/// above is not followed round and round.
fn glob(base: &Path, parts: &[&str], out: &mut Vec<PathBuf>, seen: &mut std::collections::HashSet<(PathBuf, usize)>) {
    let Some((part, rest)) = parts.split_first() else {
        out.push(base.to_path_buf());
        return;
    };
    if *part == "**" {
        // Zero or more directories, each once.
        if !seen.insert((std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf()), parts.len())) {
            return;
        }
        glob(base, rest, out, seen);
        for path in sorted_entries(base) {
            if path.is_dir() && !is_hidden(&path) {
                glob(&path, parts, out, seen);
            }
        }
    } else if has_wildcard(part) {
        for path in sorted_entries(base) {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            if matches(part, &name) && (part.starts_with('.') || !name.starts_with('.')) {
                glob(&path, rest, out, seen);
            }
        }
    } else {
        glob(&base.join(part), rest, out, seen);
    }
}

impl Workspace {
    pub fn new(patterns: Vec<String>, folders: Vec<PathBuf>) -> Workspace {
        Workspace { patterns, folders, file_regexp: None, cache: HashMap::new(), inheritance: Inheritance::default() }
    }

    /// The workspace folders.
    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// Whether the agenda has a place to look for files.
    pub fn is_configured(&self) -> bool {
        !self.patterns.is_empty() || !self.folders.is_empty()
    }

    /// The agenda files, in the order the agenda lists their entries.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.patterns.is_empty() {
            for folder in &self.folders {
                org_files_below(folder, 0, &mut files);
            }
        }
        for pattern in &self.patterns {
            for path in self.expand(pattern) {
                if path.is_dir() {
                    files.extend(org_files_in(&path, self.file_regexp.as_ref()));
                } else if path.is_file() && (is_org_file(&path) || !has_wildcard(pattern)) {
                    files.push(path);
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        files.retain(|path| seen.insert(path.clone()));
        files
    }

    /// The files `org_agenda_files` names that are not there: each entry
    /// without wildcards that names nothing, as a path.
    pub fn missing(&self) -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        self.patterns
            .iter()
            .filter(|pattern| !has_wildcard(pattern) && !self.expand(pattern).iter().any(|path| path.exists()))
            .map(|pattern| match (pattern.strip_prefix("~/"), &home) {
                (Some(rest), Some(home)) => home.join(rest),
                _ if Path::new(pattern).is_absolute() => PathBuf::from(pattern),
                _ => self.folders.first().map_or_else(|| PathBuf::from(pattern), |folder| folder.join(pattern)),
            })
            .collect()
    }

    /// The Org files `pattern` names, as for the agenda files.
    pub fn org_files(&self, pattern: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for path in self.expand(pattern) {
            if path.is_dir() {
                files.extend(org_files_in(&path, self.file_regexp.as_ref()));
            } else if path.is_file() {
                files.push(path);
            }
        }
        files
    }

    /// What `pattern` names: a path, or the matches of a glob pattern.
    /// `~` is the home directory; a relative pattern is tried in every
    /// workspace folder.
    fn expand(&self, pattern: &str) -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let pattern = pattern.replace('\\', "/");
        let (bases, rest): (Vec<PathBuf>, &str) = match (pattern.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => (vec![home.clone()], rest),
            _ if Path::new(&pattern).is_absolute() => {
                // Keep the root (`/` or `C:/`) as the base.
                let root_len = pattern.find('/').map_or(pattern.len(), |index| index + 1);
                (vec![PathBuf::from(&pattern[..root_len])], &pattern[root_len..])
            }
            _ => (self.folders.clone(), pattern.as_str()),
        };
        let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty() && *part != ".").collect();
        let mut out = Vec::new();
        for base in bases {
            glob(&base, &parts, &mut out, &mut std::collections::HashSet::new());
        }
        out
    }

    /// The file at `path`, read again if it changed on disk.
    pub fn read(&mut self, path: &Path, todo: &TodoKeywords) -> Option<Rc<Source>> {
        let metadata = std::fs::metadata(path).ok()?;
        let (modified, len) = (metadata.modified().ok(), metadata.len());
        if let Some(cached) = self.cache.get(path) {
            if cached.modified == modified && cached.len == len && modified.is_some() {
                return Some(cached.source.clone());
            }
        }
        let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned();
        let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let source = Rc::new(Source::new(text, &name, todo, &self.inheritance));
        self.cache.insert(path.to_path_buf(), Cached { modified, len, source: source.clone() });
        Some(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(matches("*.org", "notes.org"));
        assert!(matches("*", ""));
        assert!(matches("a?c*", "abcdef"));
        assert!(matches("*日本*", "x日本y"));
        assert!(!matches("*.org", "notes.org_archive"));
        assert!(!matches("a?c", "ac"));
        assert!(matches("**", "anything"));
    }

    fn tree(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("org-lsp-workspace-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for file in ["a.org", "b.txt", ".hidden.org", "sub/c.org", "sub/deep/d.org", ".git/e.org", "sub/f.org_archive"]
        {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "* TODO x\n").unwrap();
        }
        root
    }

    fn names(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
        files.iter().map(|path| path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")).collect()
    }

    /// `org-agenda-file-regexp` for the files of a directory, as
    /// `directory-files` matches their names: with regard to case.
    #[test]
    fn lists_a_directory_s_files_by_org_agenda_file_regexp() {
        let root = std::env::temp_dir().join(format!("org-lsp-workspace-regexp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for file in ["a.org", ".hidden.org", "b.txt", "c.org_archive", "D.ORG", "notes.md", "e.org.bak"] {
            std::fs::write(root.join(file), "* TODO x\n").unwrap();
        }
        // As Emacs lists them, `(org-agenda-files)` of the directory.
        for (regexp, expected) in [
            (r"\`[^.].*\.org\'", vec!["a.org"]),
            (r"\.txt\'", vec!["b.txt"]),
            (r"\(org\|md\)\'", vec![".hidden.org", "a.org", "notes.md"]),
        ] {
            let mut workspace = Workspace::new(vec![root.display().to_string()], vec![]);
            workspace.file_regexp = org_core::agenda::matcher::emacs_regex_with_case(regexp);
            assert_eq!(names(&root, workspace.files()), expected, "{regexp}");
        }
        // A file named that is not there is missing; one that is, a
        // directory or a pattern are not.
        let missing = Workspace::new(
            vec![root.join("gone.org").display().to_string(), root.display().to_string(), "*.org".to_string()],
            vec![],
        );
        assert_eq!(missing.missing(), [root.join("gone.org")]);
    }

    #[test]
    fn finds_agenda_files() {
        let root = tree("files");
        let at = |pattern: &str| format!("{}/{pattern}", root.display());
        let files =
            |patterns: Vec<String>, folders: Vec<PathBuf>| names(&root, Workspace::new(patterns, folders).files());
        // Without patterns: everything below the workspace folders.
        assert_eq!(files(vec![], vec![root.clone()]), ["a.org", "sub/c.org", "sub/deep/d.org"]);
        // A directory: its own Org files only.
        assert_eq!(files(vec![at("")], vec![]), ["a.org"]);
        assert_eq!(files(vec![at("sub")], vec![]), ["sub/c.org"]);
        // A file is taken whatever its name.
        assert_eq!(files(vec![at("b.txt"), at("missing.org")], vec![]), ["b.txt"]);
        // Glob patterns only yield Org files.
        assert_eq!(files(vec![at("**/*")], vec![]), ["a.org", "sub/c.org", "sub/deep/d.org"]);
        assert_eq!(files(vec![at("*/*.org")], vec![]), ["sub/c.org"]);
        assert_eq!(files(vec![at("sub/**/d*")], vec![]), ["sub/deep/d.org"]);
        // Relative patterns resolve in the workspace folders; no duplicates.
        assert_eq!(files(vec!["sub".to_string(), "sub/*.org".to_string()], vec![root.clone()]), ["sub/c.org"]);
        // A link to a directory above: each file once.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("sub/up")).unwrap();
            assert_eq!(files(vec![at("**/*.org")], vec![]), ["a.org", "sub/c.org", "sub/deep/d.org"]);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rereads_a_file_that_changed() {
        let root = tree("reread");
        let path = root.join("a.org");
        let todo = TodoKeywords::default();
        let mut workspace = Workspace::new(vec![], vec![root.clone()]);
        let first = workspace.read(&path, &todo).unwrap();
        assert_eq!(first.outline.entries.len(), 1);
        assert!(Rc::ptr_eq(&first, &workspace.read(&path, &todo).unwrap()));
        std::fs::write(&path, "* TODO x\n* TODO y\n").unwrap();
        assert_eq!(workspace.read(&path, &todo).unwrap().outline.entries.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
