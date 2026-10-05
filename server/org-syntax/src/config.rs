//! Parse configuration: the TODO keywords that are valid in a document.

/// Whether a state change is recorded, as the keyword's definition asks:
/// `WAIT(w@/!)` records a note on entering and a time on leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logging {
    /// `!`: a line with the time.
    Time,
    /// `@`: a line with the time and a note.
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Keyword {
    name: String,
    done: bool,
    /// Index of the first keyword of the sequence this one belongs to.
    head: usize,
    on_entering: Option<Logging>,
    on_leaving: Option<Logging>,
}

/// An ordered set of TODO keywords, each marked as "done" or not.
///
/// The order is the cycling order. With several sequences (several `#+TODO:`
/// lines) the keywords are concatenated, matching `org-todo-keywords-1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoKeywords {
    keywords: Vec<Keyword>,
}

impl Default for TodoKeywords {
    fn default() -> Self {
        TodoKeywords::from_sequence(["TODO", "|", "DONE"])
    }
}

/// Split `WAIT(w@/!)` into the name and what to log on entering and on
/// leaving the state.
fn keyword_definition(word: &str) -> (&str, Option<Logging>, Option<Logging>) {
    let Some(open) = word.find('(').filter(|_| word.ends_with(')')) else { return (word, None, None) };
    let options = &word[open + 1..word.len() - 1];
    let logging = |c: Option<char>| match c {
        Some('!') => Some(Logging::Time),
        Some('@') => Some(Logging::Note),
        _ => None,
    };
    // An optional selection key, then `!` or `@`, then `/` and the same
    // for leaving.
    let mut chars = options.chars().peekable();
    if chars.peek().is_some_and(|c| !matches!(c, '!' | '@' | '/')) {
        chars.next();
    }
    let entering = logging(chars.peek().copied());
    if entering.is_some() {
        chars.next();
    }
    let leaving = if chars.next() == Some('/') { logging(chars.next()) } else { None };
    (&word[..open], entering, leaving)
}

impl TodoKeywords {
    /// Build from one sequence written the Org way: `["TODO", "NEXT", "|", "DONE"]`.
    ///
    /// Without a `|` separator the last keyword is the only "done" state.
    /// A keyword may carry a selection key and logging marks, as in
    /// `TODO(t)` or `WAIT(w@/!)`.
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
        let words: Vec<String> = words.into_iter().map(|w| w.as_ref().to_string()).collect();
        let definitions: Vec<(&str, Option<Logging>, Option<Logging>)> =
            words.iter().map(|word| keyword_definition(word)).filter(|(name, ..)| !name.is_empty()).collect();
        let separator = definitions.iter().position(|(name, ..)| *name == "|");
        let names: Vec<_> = definitions.iter().filter(|(name, ..)| *name != "|").collect();
        let first_done = match separator {
            Some(index) => index,
            None => names.len().saturating_sub(1),
        };
        let head = self.keywords.len();
        for (index, (name, on_entering, on_leaving)) in names.into_iter().enumerate() {
            if self.contains(name) {
                continue;
            }
            self.keywords.push(Keyword {
                name: name.to_string(),
                done: index >= first_done,
                head,
                on_entering: *on_entering,
                on_leaving: *on_leaving,
            });
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
        self.keywords.iter().map(|keyword| keyword.name.as_str())
    }

    fn get(&self, word: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|keyword| keyword.name == word)
    }

    pub fn contains(&self, word: &str) -> bool {
        self.get(word).is_some()
    }

    /// Whether `word` is a keyword that marks an entry as finished.
    pub fn is_done(&self, word: &str) -> bool {
        self.get(word).is_some_and(|keyword| keyword.done)
    }

    /// The first keyword of the sequence `word` belongs to.
    pub fn head(&self, word: &str) -> Option<&str> {
        let keyword = self.get(word)?;
        self.keywords.get(keyword.head).map(|head| head.name.as_str())
    }

    /// What to record when an entry enters the state `word`.
    pub fn logs_entering(&self, word: &str) -> Option<Logging> {
        self.get(word)?.on_entering
    }

    /// What to record when an entry leaves the state `word`, unless the
    /// new state records something itself.
    pub fn logs_leaving(&self, word: &str) -> Option<Logging> {
        self.get(word)?.on_leaving
    }

    /// Whether any keyword asks for state changes to be recorded.
    pub fn logs_any(&self) -> bool {
        self.keywords.iter().any(|keyword| keyword.on_entering.is_some() || keyword.on_leaving.is_some())
    }

    /// The state after `current` when cycling forward; `None` means "no keyword".
    pub fn next(&self, current: Option<&str>) -> Option<&str> {
        match current.and_then(|word| self.position(word)) {
            None => self.keywords.first().map(|keyword| keyword.name.as_str()),
            Some(index) => self.keywords.get(index + 1).map(|keyword| keyword.name.as_str()),
        }
    }

    /// The state before `current` when cycling backward; `None` means "no keyword".
    pub fn previous(&self, current: Option<&str>) -> Option<&str> {
        match current.and_then(|word| self.position(word)) {
            None => self.keywords.last().map(|keyword| keyword.name.as_str()),
            Some(0) => None,
            Some(index) => self.keywords.get(index - 1).map(|keyword| keyword.name.as_str()),
        }
    }

    fn position(&self, word: &str) -> Option<usize> {
        self.keywords.iter().position(|keyword| keyword.name == word)
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

    #[test]
    fn logging_marks_and_sequence_heads() {
        let text = "#+TODO: TODO(t) WAIT(w@/!) | DONE(d!) GONE(@)\n#+TODO: A(!/@) | B(/!)\n";
        let kw = TodoKeywords::from_document(text).unwrap();
        assert_eq!((kw.logs_entering("TODO"), kw.logs_leaving("TODO")), (None, None));
        assert_eq!((kw.logs_entering("WAIT"), kw.logs_leaving("WAIT")), (Some(Logging::Note), Some(Logging::Time)));
        assert_eq!(kw.logs_entering("DONE"), Some(Logging::Time));
        assert_eq!(kw.logs_entering("GONE"), Some(Logging::Note));
        assert_eq!((kw.logs_entering("A"), kw.logs_leaving("A")), (Some(Logging::Time), Some(Logging::Note)));
        assert_eq!((kw.logs_entering("B"), kw.logs_leaving("B")), (None, Some(Logging::Time)));
        assert_eq!(kw.head("GONE"), Some("TODO"));
        assert_eq!(kw.head("B"), Some("A"));
        assert_eq!(kw.head("nope"), None);
        assert!(kw.logs_any() && !TodoKeywords::default().logs_any());
    }
}
