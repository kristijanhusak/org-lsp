//! Parse configuration: the TODO keywords that are valid in a document.

/// An ordered set of TODO keywords, each marked as "done" or not.
///
/// The order is the cycling order. With several sequences (several `#+TODO:`
/// lines) the keywords are concatenated, matching `org-todo-keywords-1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoKeywords {
    keywords: Vec<(String, bool)>,
}

impl Default for TodoKeywords {
    fn default() -> Self {
        TodoKeywords::from_sequence(["TODO", "|", "DONE"])
    }
}

impl TodoKeywords {
    /// Build from one sequence written the Org way: `["TODO", "NEXT", "|", "DONE"]`.
    ///
    /// Without a `|` separator the last keyword is the only "done" state.
    /// Fast-access suffixes such as `TODO(t)` or `WAIT(w@/!)` are stripped.
    pub fn from_sequence<I, S>(words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = TodoKeywords { keywords: Vec::new() };
        out.push_sequence(words);
        out
    }

    fn push_sequence<I, S>(&mut self, words: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let words: Vec<String> =
            words.into_iter().map(|w| strip_fast_key(w.as_ref()).to_string()).filter(|w| !w.is_empty()).collect();
        let separator = words.iter().position(|w| w == "|");
        let names: Vec<&String> = words.iter().filter(|w| *w != "|").collect();
        let first_done = match separator {
            Some(index) => index,
            None => names.len().saturating_sub(1),
        };
        for (index, name) in names.into_iter().enumerate() {
            if self.keywords.iter().any(|(existing, _)| existing == name) {
                continue;
            }
            self.keywords.push((name.clone(), index >= first_done));
        }
    }

    /// Keywords declared in the document itself (`#+TODO:`, `#+SEQ_TODO:`,
    /// `#+TYP_TODO:`), or `None` when the document declares none.
    pub fn from_document(text: &str) -> Option<Self> {
        let mut out = TodoKeywords { keywords: Vec::new() };
        let mut found = false;
        for line in text.lines() {
            let line = line.trim_start();
            if !line.starts_with("#+") {
                continue;
            }
            let Some(colon) = line.find(':') else { continue };
            let key = &line[2..colon];
            if !["TODO", "SEQ_TODO", "TYP_TODO"].iter().any(|k| key.eq_ignore_ascii_case(k)) {
                continue;
            }
            let words: Vec<&str> = line[colon + 1..].split_whitespace().collect();
            if words.iter().all(|w| *w == "|") {
                continue;
            }
            found = true;
            out.push_sequence(words);
        }
        found.then_some(out)
    }

    pub fn is_empty(&self) -> bool {
        self.keywords.is_empty()
    }

    /// All keywords in cycling order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.keywords.iter().map(|(name, _)| name.as_str())
    }

    pub fn contains(&self, word: &str) -> bool {
        self.keywords.iter().any(|(name, _)| name == word)
    }

    /// Whether `word` is a keyword that marks an entry as finished.
    pub fn is_done(&self, word: &str) -> bool {
        self.keywords.iter().any(|(name, done)| *done && name == word)
    }

    /// The state after `current` when cycling forward; `None` means "no keyword".
    pub fn next(&self, current: Option<&str>) -> Option<&str> {
        match current.and_then(|word| self.position(word)) {
            None => self.keywords.first().map(|(name, _)| name.as_str()),
            Some(index) => self.keywords.get(index + 1).map(|(name, _)| name.as_str()),
        }
    }

    /// The state before `current` when cycling backward; `None` means "no keyword".
    pub fn previous(&self, current: Option<&str>) -> Option<&str> {
        match current.and_then(|word| self.position(word)) {
            None => self.keywords.last().map(|(name, _)| name.as_str()),
            Some(0) => None,
            Some(index) => self.keywords.get(index - 1).map(|(name, _)| name.as_str()),
        }
    }

    fn position(&self, word: &str) -> Option<usize> {
        self.keywords.iter().position(|(name, _)| name == word)
    }
}

fn strip_fast_key(word: &str) -> &str {
    match word.find('(') {
        Some(index) if word.ends_with(')') => &word[..index],
        _ => word,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sequence() {
        let kw = TodoKeywords::default();
        assert_eq!(kw.names().collect::<Vec<_>>(), ["TODO", "DONE"]);
        assert!(!kw.is_done("TODO"));
        assert!(kw.is_done("DONE"));
    }

    #[test]
    fn without_separator_last_is_done() {
        let kw = TodoKeywords::from_sequence(["A", "B", "C"]);
        assert!(!kw.is_done("B"));
        assert!(kw.is_done("C"));
    }

    #[test]
    fn cycles_through_none() {
        let kw = TodoKeywords::from_sequence(["TODO", "NEXT", "|", "DONE"]);
        assert_eq!(kw.next(None), Some("TODO"));
        assert_eq!(kw.next(Some("TODO")), Some("NEXT"));
        assert_eq!(kw.next(Some("DONE")), None);
        assert_eq!(kw.previous(None), Some("DONE"));
        assert_eq!(kw.previous(Some("TODO")), None);
        assert_eq!(kw.previous(Some("DONE")), Some("NEXT"));
    }

    #[test]
    fn reads_document_keywords() {
        let text = "#+TITLE: x\n#+TODO: TODO(t) WAIT(w@/!) | DONE(d!) CANCELLED\n#+seq_todo: A B\n* TODO x\n";
        let kw = TodoKeywords::from_document(text).unwrap();
        assert_eq!(kw.names().collect::<Vec<_>>(), ["TODO", "WAIT", "DONE", "CANCELLED", "A", "B"]);
        assert!(kw.is_done("CANCELLED"));
        assert!(!kw.is_done("A"));
        assert!(kw.is_done("B"));
        assert_eq!(TodoKeywords::from_document("* TODO x\n"), None);
    }
}
