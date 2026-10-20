//! Org's options: each `defcustom` of Org 9.8.10, the file it is in, and
//! what org-lsp does with it, as `scripts/org-options.json` says (the
//! Options section of `docs/parity.org` is written from it). The server
//! reads it for `emacs_options`, which sets in Emacs those Emacs reads.

use std::sync::LazyLock;

use serde_json::Value;

/// An option of Org.
pub struct OrgOption {
    /// Its name, `org-attach-method`.
    pub name: String,
    /// The file it is in, `org-attach.el`.
    pub file: String,
    /// What org-lsp does with it: `setting`, `emacs`, `lisp`, `missing`,
    /// `other`, `absent` or `emacs-only`.
    pub status: String,
    /// What an option done another way (`other`) is done by.
    pub note: Option<String>,
}

impl OrgOption {
    /// The feature its file provides, which Emacs loads it with
    /// (`org-attach`).
    pub fn feature(&self) -> &str {
        self.file.strip_suffix(".el").unwrap_or(&self.file)
    }
}

static OPTIONS: LazyLock<Vec<OrgOption>> = LazyLock::new(|| {
    let files: serde_json::Map<String, Value> =
        serde_json::from_str(include_str!("../../../scripts/org-options.json")).expect("org-options.json is JSON");
    let mut options = Vec::new();
    for (file, names) in files {
        for (name, value) in names.as_object().into_iter().flatten() {
            let (status, note) = match value {
                Value::Array(parts) => (parts.first(), parts.get(1).and_then(Value::as_str).map(str::to_string)),
                status => (Some(status), None),
            };
            let status = status.and_then(Value::as_str).unwrap_or_default().to_string();
            options.push(OrgOption { name: name.clone(), file: file.clone(), status, note });
        }
    }
    options
});

/// Every option of Org.
pub fn all() -> &'static [OrgOption] {
    &OPTIONS
}

/// The option `name`.
pub fn find(name: &str) -> Option<&'static OrgOption> {
    OPTIONS.iter().find(|option| option.name == name)
}

/// The option whose name is closest to `name`, a few letters away at
/// most: what a misspelt name was meant to be.
pub fn closest(name: &str) -> Option<&'static str> {
    let most = (name.chars().count() / 4).clamp(1, 3);
    OPTIONS
        .iter()
        .map(|option| (distance(name, &option.name), option.name.as_str()))
        .filter(|(distance, _)| *distance <= most)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, name)| name)
}

/// How many characters to insert, delete or replace to make `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let replaced = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = replaced.min(row[j] + 1).min(previous + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_each_option_and_its_file() {
        let method = find("org-attach-method").unwrap();
        assert_eq!((method.feature(), method.status.as_str()), ("org-attach", "emacs"));
        assert_eq!(find("org-log-done").unwrap().status, "setting");
        assert!(find("org-tab-follows-link").unwrap().note.is_some());
        assert!(all().len() > 1000);
    }

    #[test]
    fn finds_the_name_meant() {
        assert_eq!(closest("org-atach-method"), Some("org-attach-method"));
        assert_eq!(closest("org-export-with-tocs"), Some("org-export-with-toc"));
        assert_eq!(closest("nothing-like-it"), None);
    }
}
