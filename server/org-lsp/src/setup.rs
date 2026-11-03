//! `#+SETUPFILE:`: the in-buffer settings a document takes from other
//! files, as Emacs's `org-collect-keywords` (`org--collect-keywords-1`)
//! reads them. Each setup file's keyword lines are read in order, a setup
//! file it names read in its place, and a file already being read (a
//! setup file naming itself, or one that names it) is not read again.
//!
//! Unlike Emacs, which reads a setup file's settings where its
//! `#+SETUPFILE:` line is, they come before the document's own; and a
//! setup file on the web (`https://…`) is not read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use org_syntax::Setting;

/// Keywords whose values are file names, made absolute from the setup
/// file's directory (`org-collect-keywords`'s DIRECTORY): the documents
/// that read them are elsewhere.
const FILE_NAMES: &[&str] = &["BIBLIOGRAPHY"];

/// A file read: when it was last changed, and its text.
type Read = (Option<SystemTime>, String);

/// The files read, by path: read again when they change.
static FILES: LazyLock<Mutex<HashMap<PathBuf, Read>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The settings of the setup files of `text`, the document at `path`
/// (relative setup files are found from its directory; with none, from
/// the current directory).
pub fn settings(path: Option<&Path>, text: &str) -> Vec<Setting> {
    if !mentions_setup_file(text) {
        return Vec::new();
    }
    let directory = path.and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    let mut chain: Vec<PathBuf> = path.map(|path| vec![path.to_path_buf()]).unwrap_or_default();
    let mut out = Vec::new();
    for (key, value) in keyword_lines(text) {
        if key.eq_ignore_ascii_case("SETUPFILE") {
            read(&directory, &value, &mut chain, &mut out);
        }
    }
    out
}

/// The keyword lines Org reads for `text`, the document at `path`
/// (`org-collect-keywords`): those of its setup files, then its own.
pub fn keywords(path: Option<&Path>, text: &str) -> Vec<Setting> {
    let mut all = settings(path, text);
    all.extend(keyword_lines(text).into_iter().filter(|(key, _)| key != "SETUPFILE"));
    all
}

/// Whether `text` may have a `#+SETUPFILE:` line: most documents have
/// none, and are read no further.
fn mentions_setup_file(text: &str) -> bool {
    text.as_bytes().windows(12).any(|window| window.eq_ignore_ascii_case(b"#+setupfile:"))
}

/// The settings of the setup file `value` names, from `directory`, and of
/// the setup files it names, into `out`.
fn read(directory: &Path, value: &str, chain: &mut Vec<PathBuf>, out: &mut Vec<Setting>) {
    let name = strip_quotes(value.trim());
    if name.is_empty() || name.contains("://") {
        return;
    }
    let path = crate::config::expand_home(name, &|name| std::env::var(name).ok());
    let path = if path.is_absolute() { path } else { directory.join(path) };
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    if chain.contains(&path) {
        return;
    }
    let Some(text) = contents(&path) else { return };
    let here = path.parent().map(Path::to_path_buf).unwrap_or_default();
    chain.push(path);
    for (key, value) in keyword_lines(&text) {
        if key.eq_ignore_ascii_case("SETUPFILE") {
            read(&here, &value, chain, out);
        } else if FILE_NAMES.iter().any(|name| key.eq_ignore_ascii_case(name)) && !value.is_empty() {
            let file = strip_quotes(&value);
            let file = if Path::new(file).is_absolute() { PathBuf::from(file) } else { here.join(file) };
            out.push((key, file.display().to_string()));
        } else {
            out.push((key, value));
        }
    }
    chain.pop();
}

/// The text of the file at `path`, read again when it has changed.
fn contents(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok();
    let mut files = FILES.lock().ok()?;
    if let Some((when, text)) = files.get(path) {
        if *when == modified && modified.is_some() {
            return Some(text.clone());
        }
    }
    let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned();
    files.insert(path.to_path_buf(), (modified, text.clone()));
    Some(text)
}

/// `org-strip-quotes`
fn strip_quotes(value: &str) -> &str {
    value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value)
}

/// The keyword lines of `text` (`#+KEY: value`), the key upper case and
/// the value trimmed, leaving out those in blocks whose lines are not
/// Org's (source, example, export, comment and verse blocks).
fn keyword_lines(text: &str) -> Vec<Setting> {
    let mut lines = Vec::new();
    let mut inside: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_start_matches([' ', '\t']);
        if let Some(end) = &inside {
            if line.get(..end.len()).is_some_and(|start| start.eq_ignore_ascii_case(end)) {
                inside = None;
            }
            continue;
        }
        if line.get(..8).is_some_and(|start| start.eq_ignore_ascii_case("#+begin_")) {
            let name: String = line[8..].chars().take_while(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
            if matches!(name.as_str(), "src" | "example" | "export" | "comment" | "verse") {
                inside = Some(format!("#+end_{name}"));
            }
            continue;
        }
        let Some(rest) = line.strip_prefix("#+") else { continue };
        let Some(colon) = rest.find(':') else { continue };
        let key = &rest[..colon];
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        lines.push((key.to_ascii_uppercase(), rest[colon + 1..].trim().to_string()));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(directory: &Path, name: &str, text: &str) -> PathBuf {
        let path = directory.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn reads_setup_files_in_order_and_once() {
        let directory = std::env::temp_dir().join(format!("org-lsp-setup-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("sub")).unwrap();
        write(&directory, "setup.org", "#+TODO: FOO | BAR\n#+SETUPFILE: sub/more.org\n#+STARTUP: overview\n");
        write(
            &directory.join("sub"),
            "more.org",
            "#+TAGS: a b\n#+SETUPFILE: ../setup.org\n#+BIBLIOGRAPHY: refs.bib\n#+begin_src org\n#+TODO: NO\n#+end_src\n",
        );
        let document = directory.join("notes.org");
        let found = settings(Some(&document), "#+setupfile: \"./setup.org\"\n* item\n");
        let refs = directory.join("sub").canonicalize().unwrap().join("refs.bib").display().to_string();
        let expected: Vec<Setting> = vec![
            ("TODO".into(), "FOO | BAR".into()),
            ("TAGS".into(), "a b".into()),
            ("BIBLIOGRAPHY".into(), refs),
            ("STARTUP".into(), "overview".into()),
        ];
        assert_eq!(found, expected);
        assert!(settings(Some(&document), "* no setup file\n").is_empty());
        assert!(settings(Some(&document), "#+SETUPFILE: missing.org\n").is_empty());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
