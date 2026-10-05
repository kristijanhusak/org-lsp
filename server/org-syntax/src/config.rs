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
    /// The key that selects it, as in `TODO(t)`.
    key: Option<char>,
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

/// A keyword as written: its name, the key that selects it, and what to
/// log on entering and on leaving the state.
type Definition<'a> = (&'a str, Option<char>, Option<Logging>, Option<Logging>);

/// Split `WAIT(w@/!)` into the name, the selection key and what to log on
/// entering and on leaving the state.
fn keyword_definition(word: &str) -> Definition<'_> {
    let Some(open) = word.find('(').filter(|_| word.ends_with(')')) else { return (word, None, None, None) };
    let options = &word[open + 1..word.len() - 1];
    let logging = |c: Option<char>| match c {
        Some('!') => Some(Logging::Time),
        Some('@') => Some(Logging::Note),
        _ => None,
    };
    // An optional selection key, then `!` or `@`, then `/` and the same
    // for leaving.
    let mut chars = options.chars().peekable();
    let key = chars.next_if(|c| !matches!(c, '!' | '@' | '/'));
    let entering = logging(chars.peek().copied());
    if entering.is_some() {
        chars.next();
    }
    let leaving = if chars.next() == Some('/') { logging(chars.next()) } else { None };
    (&word[..open], key, entering, leaving)
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
        let definitions: Vec<Definition> =
            words.iter().map(|word| keyword_definition(word)).filter(|(name, ..)| !name.is_empty()).collect();
        let separator = definitions.iter().position(|(name, ..)| *name == "|");
        let names: Vec<_> = definitions.iter().filter(|(name, ..)| *name != "|").collect();
        let first_done = match separator {
            Some(index) => index,
            None => names.len().saturating_sub(1),
        };
        let head = self.keywords.len();
        for (index, (name, key, on_entering, on_leaving)) in names.into_iter().enumerate() {
            if self.contains(name) {
                continue;
            }
            self.keywords.push(Keyword {
                name: name.to_string(),
                key: *key,
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

    /// The keys of fast TODO selection (`org-todo-key-alist`), in
    /// keyword order, or none when no keyword was given a key
    /// (`org-todo-key-trigger`). A keyword without a key gets one as
    /// `org-assign-fast-keys` picks it: the first letter of its name, in
    /// lower case, that no other keyword uses, or else the first unused of
    /// `0`, `1`, `2`…
    pub fn fast_keys(&self) -> Vec<(&str, char)> {
        if self.keywords.iter().all(|keyword| keyword.key.is_none()) {
            return Vec::new();
        }
        let mut keys: Vec<Option<char>> = self.keywords.iter().map(|keyword| keyword.key).collect();
        let mut alternative = '0';
        for index in 0..keys.len() {
            if keys[index].is_some() {
                continue;
            }
            // Keys taken: those assigned so far, and those given later.
            let used = |c: char, keys: &[Option<char>]| keys.contains(&Some(c));
            let name = self.keywords[index].name.to_lowercase();
            let mut letters = name.chars().peekable();
            letters.next_if_eq(&'@');
            let letter = letters.find(|c| !used(*c, &keys));
            keys[index] = Some(letter.unwrap_or_else(|| {
                while used(alternative, &keys) {
                    alternative = char::from_u32(u32::from(alternative) + 1).unwrap_or(alternative);
                }
                alternative
            }));
        }
        self.keywords.iter().zip(keys).map(|(keyword, key)| (keyword.name.as_str(), key.unwrap_or('0'))).collect()
    }

    /// The keyword `key` selects (`org-fast-todo-selection`), preferring
    /// one in the sequence of `current` when keys are shared.
    pub fn fast_key(&self, key: char, current: Option<&str>) -> Option<&str> {
        let keys = self.fast_keys();
        let head = current.and_then(|word| self.get(word)).map(|keyword| keyword.head);
        let in_sequence = |name: &str| head.is_some() && self.get(name).map(|keyword| keyword.head) == head;
        let found = keys.iter().find(|(name, k)| *k == key && in_sequence(name));
        found.or_else(|| keys.iter().find(|(_, k)| *k == key)).map(|(name, _)| *name)
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

    #[test]
    fn fast_selection_keys() {
        assert!(TodoKeywords::default().fast_keys().is_empty(), "no key given");
        let text = "#+TODO: TODO(t) NEXT WAIT(w@/!) | DONE(d!) DELEGATED\n#+TODO: TASK | TOLD(t)\n";
        let kw = TodoKeywords::from_document(text).unwrap();
        assert_eq!(
            kw.fast_keys(),
            [
                ("TODO", 't'),
                ("NEXT", 'n'),
                ("WAIT", 'w'),
                ("DONE", 'd'),
                ("DELEGATED", 'e'),
                ("TASK", 'a'),
                ("TOLD", 't')
            ]
        );
        assert_eq!(kw.fast_key('t', Some("TASK")), Some("TOLD"), "the sequence of the current keyword first");
        assert_eq!(kw.fast_key('t', Some("NEXT")), Some("TODO"));
        assert_eq!(kw.fast_key('t', None), Some("TODO"));
        assert_eq!(kw.fast_key('x', None), None);
        let kw = TodoKeywords::from_sequence(["A(a)", "AB", "BA"]);
        assert_eq!(kw.fast_keys(), [("A", 'a'), ("AB", 'b'), ("BA", '0')]);
    }
}
