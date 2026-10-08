//! `org-contextualize-keys`: which capture templates
//! (`org-capture-templates-contexts`) and agenda commands
//! (`org-agenda-custom-commands-contexts`) a place offers. A context names
//! a key, the key whose template or command it stands for there (itself
//! when none), and rules; the key is offered where one of its contexts has
//! a rule that holds, and not at all where none does. Keys without a
//! context are always offered.

use regex::{Regex, RegexBuilder};

/// Where the command runs: the file of the buffer (none in the agenda),
/// its major mode and its name.
#[derive(Debug, Clone, Copy)]
pub struct Here<'a> {
    pub file: Option<&'a str>,
    pub mode: &'a str,
    pub buffer: &'a str,
}

/// What a rule looks at, and whether it wants a match or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    InFile,
    InMode,
    InBuffer,
    NotInFile,
    NotInMode,
    NotInBuffer,
}

impl Kind {
    /// The kind of `in_file`, `not-in-mode` and so on.
    pub fn named(name: &str) -> Option<Kind> {
        Some(match name.replace('_', "-").as_str() {
            "in-file" => Kind::InFile,
            "in-mode" => Kind::InMode,
            "in-buffer" => Kind::InBuffer,
            "not-in-file" => Kind::NotInFile,
            "not-in-mode" => Kind::NotInMode,
            "not-in-buffer" => Kind::NotInBuffer,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub kind: Kind,
    /// Matched as `string-match` does with `case-fold-search`: ignoring
    /// case.
    pub regex: Regex,
}

impl Rule {
    /// A rule of `kind` on `pattern` (the syntax of the `regex` crate).
    pub fn new(kind: Kind, pattern: &str) -> Result<Rule, String> {
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|error| format!("Invalid context regexp {pattern:?}: {error}"))?;
        Ok(Rule { kind, regex })
    }

    /// Whether it holds `here` (`org-contextualize-validate-key`): a rule
    /// on the file holds in no buffer without one.
    fn holds(&self, here: &Here) -> bool {
        let matches = |text: &str| self.regex.is_match(text);
        match self.kind {
            Kind::InFile => here.file.is_some_and(matches),
            Kind::InMode => matches(here.mode),
            Kind::InBuffer => matches(here.buffer),
            Kind::NotInFile => here.file.is_some_and(|file| !matches(file)),
            Kind::NotInMode => !matches(here.mode),
            Kind::NotInBuffer => !matches(here.buffer),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    pub key: String,
    /// The key whose template or command `key` stands for where this
    /// context holds: `key` itself when it names none.
    pub use_key: String,
    pub rules: Vec<Rule>,
}

/// The `items` offered `here`, in their order: each kept, left out, or
/// standing for another item under its own key (that other item then not
/// offered under its own).
pub fn contextualize<T: Clone>(
    items: &[T],
    key_of: impl Fn(&T) -> &str,
    rekey: impl Fn(&T, &str) -> T,
    contexts: &[Context],
    here: &Here,
) -> Result<Vec<T>, String> {
    let mut kept: Vec<T> = Vec::new();
    let mut replacements: Vec<&str> = Vec::new();
    for item in items {
        let key = key_of(item);
        if !contexts.iter().any(|context| context.key == key) {
            kept.push(item.clone());
            continue;
        }
        let valid: Vec<&Context> = contexts
            .iter()
            .filter(|context| context.key == key && context.rules.iter().any(|rule| rule.holds(here)))
            .collect();
        if valid.is_empty() {
            continue;
        }
        match valid.iter().find(|context| context.key != context.use_key) {
            None => kept.push(item.clone()),
            Some(replacement) => {
                let used = items.iter().find(|other| key_of(other) == replacement.use_key).ok_or_else(|| {
                    format!("Undefined key `{}' as contextual replacement for `{key}'", replacement.use_key)
                })?;
                replacements.push(&replacement.use_key);
                kept.push(rekey(used, key));
            }
        }
    }
    kept.retain(|item| !replacements.contains(&key_of(item)));
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against `org-contextualize-keys` in Emacs, over `a`, `b`, `c`, `q`.
    #[test]
    fn offers_what_emacs_offers() {
        let items: Vec<(String, String)> =
            ["a", "b", "c", "q"].iter().map(|key| (key.to_string(), key.to_uppercase())).collect();
        let context = |key: &str, use_key: &str, rules: &[(Kind, &str)]| Context {
            key: key.to_string(),
            use_key: use_key.to_string(),
            rules: rules.iter().map(|(kind, pattern)| Rule::new(*kind, pattern).unwrap()).collect(),
        };
        let org = |file: &'static str, buffer: &'static str| Here { file: Some(file), mode: "org-mode", buffer };
        let agenda = Here { file: None, mode: "org-agenda-mode", buffer: "*Org Agenda*" };
        let offered = |contexts: Vec<Context>, here: Here| {
            contextualize(&items, |item| &item.0, |item, key| (key.to_string(), item.1.clone()), &contexts, &here)
                .map(|kept| kept.iter().map(|(key, body)| format!("{key}{body}")).collect::<Vec<_>>().join(" "))
        };
        let cases = [
            (vec![context("a", "a", &[(Kind::InFile, r"\.org$")])], org("/h/x.org", "x.org"), "aA bB cC qQ"),
            (vec![context("a", "a", &[(Kind::InFile, r"\.org$")])], org("/h/x.txt", "x.txt"), "bB cC qQ"),
            (vec![context("a", "q", &[(Kind::InMode, "org-mode")])], org("/h/x.org", "x.org"), "aQ bB cC"),
            (vec![context("a", "a", &[(Kind::NotInFile, "work")])], org("/work/x.org", "x.org"), "bB cC qQ"),
            (vec![context("a", "a", &[(Kind::NotInFile, "work")])], org("/home/x.org", "x.org"), "aA bB cC qQ"),
            (
                vec![context("a", "b", &[(Kind::InFile, "x")]), context("a", "c", &[(Kind::InFile, "x")])],
                org("/h/x.org", "x.org"),
                "aB cC qQ",
            ),
            (vec![context("a", "a", &[(Kind::InFile, r"\.ORG")])], org("/h/x.org", "x.org"), "aA bB cC qQ"),
            (vec![context("a", "a", &[(Kind::InBuffer, "notes")])], org("/h/x.org", "notes.org"), "aA bB cC qQ"),
            (vec![context("a", "a", &[(Kind::InFile, "x")])], agenda, "bB cC qQ"),
            (vec![context("a", "a", &[(Kind::NotInFile, "x")])], agenda, "bB cC qQ"),
            (
                vec![context("a", "b", &[(Kind::InMode, "org")]), context("c", "c", &[(Kind::InMode, "text")])],
                org("/h/x.org", "x.org"),
                "aB qQ",
            ),
            (
                vec![context("a", "b", &[(Kind::InMode, "text"), (Kind::InFile, "x")])],
                org("/h/x.org", "x.org"),
                "aB cC qQ",
            ),
        ];
        for (index, (contexts, here, expected)) in cases.into_iter().enumerate() {
            assert_eq!(offered(contexts, here), Ok(expected.to_string()), "case {index}");
        }
        let undefined = vec![context("a", "zz", &[(Kind::InMode, "org")])];
        assert_eq!(
            offered(undefined, org("/h/x.org", "x.org")),
            Err("Undefined key `zz' as contextual replacement for `a'".to_string())
        );
    }
}
