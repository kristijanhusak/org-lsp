//! Parse configuration: the TODO keywords that are valid in a document.

use std::collections::BTreeMap;

/// Whether a state change is recorded, as the keyword's definition asks:
/// `WAIT(w@/!)` records a note on entering and a time on leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logging {
    /// `!`: a line with the time.
    Time,
    /// `@`: a line with the time and a note.
    Note,
}

/// How the keywords of a sequence are used (the first word of an entry of
/// `org-todo-keywords`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpretation {
    /// `sequence` (`#+TODO:`, `#+SEQ_TODO:`): states the entry goes through.
    #[default]
    Sequence,
    /// `type` (`#+TYP_TODO:`): who or what the entry is for; cycling goes
    /// from any of them straight to the done state.
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Keyword {
    name: String,
    /// The key that selects it, as in `TODO(t)`.
    key: Option<char>,
    done: bool,
    /// Index of the first keyword of the sequence this one belongs to.
    head: usize,
    interpretation: Interpretation,
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
    /// Where each keyword is in `keywords`: looked up for every headline.
    index: BTreeMap<String, usize>,
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
        let mut out = TodoKeywords { keywords: Vec::new(), index: BTreeMap::new() };
        out.push_sequence(Interpretation::Sequence, words);
        out
    }

    /// Build from several sequences, as `org-todo-keywords` lists them.
    pub fn from_sequences<I, W, S>(sequences: I) -> Self
    where
        I: IntoIterator<Item = (Interpretation, W)>,
        W: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = TodoKeywords { keywords: Vec::new(), index: BTreeMap::new() };
        for (interpretation, words) in sequences {
            out.push_sequence(interpretation, words);
        }
        out
    }

    fn push_sequence<I, S>(&mut self, interpretation: Interpretation, words: I)
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
            self.index.insert(name.to_string(), self.keywords.len());
            self.keywords.push(Keyword {
                name: name.to_string(),
                key: *key,
                done: index >= first_done,
                head,
                interpretation,
                on_entering: *on_entering,
                on_leaving: *on_leaving,
            });
        }
    }

    /// Keywords declared in the document itself (`#+TODO:`, `#+SEQ_TODO:`,
    /// `#+TYP_TODO:`), or `None` when the document declares none.
    pub fn from_document(text: &str) -> Option<Self> {
        let mut out = TodoKeywords { keywords: Vec::new(), index: BTreeMap::new() };
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
            let interpretation =
                if key.eq_ignore_ascii_case("TYP_TODO") { Interpretation::Type } else { Interpretation::Sequence };
            out.push_sequence(interpretation, words);
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
        self.keywords.get(*self.index.get(word)?)
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

    /// The keywords of the sequence that starts with `head`, in order.
    pub fn sequence(&self, head: &str) -> Vec<&str> {
        let Some(index) = self.position(head) else { return Vec::new() };
        let head = self.keywords[index].head;
        self.keywords.iter().filter(|keyword| keyword.head == head).map(|keyword| keyword.name.as_str()).collect()
    }

    /// The first keyword of each sequence (`org-todo-heads`).
    pub fn heads(&self) -> Vec<&str> {
        self.keywords
            .iter()
            .enumerate()
            .filter(|(index, keyword)| keyword.head == *index)
            .map(|(_, keyword)| keyword.name.as_str())
            .collect()
    }

    /// The head of the sequence of `current` (`org-get-todo-sequence-head`):
    /// with no keyword, `remembered`, the sequence the entry was last in
    /// (the `org-todo-head` text property), or else the first keyword.
    fn sequence_head<'a>(&'a self, current: Option<&str>, remembered: Option<&str>) -> Option<&'a str> {
        let word = match current {
            Some(word) => Some(word),
            None => remembered.filter(|head| self.contains(head)),
        };
        match word.and_then(|word| self.head(word)) {
            Some(head) => Some(head),
            None => self.keywords.first().map(|keyword| keyword.name.as_str()),
        }
    }

    /// The state `org-todo` cycles to from `current` without fast
    /// selection, within the sequence: after its last done state no
    /// keyword, and from no keyword the head of the sequence the entry was
    /// last in (`remembered`), or the first keyword. A keyword of a `type`
    /// sequence goes to the sequence's first done state, unless the command
    /// is repeated (`repeated`), which steps on to the next one.
    pub fn cycle(&self, current: Option<&str>, remembered: Option<&str>, repeated: bool) -> Option<&str> {
        let Some(index) = current.and_then(|word| self.position(word)) else {
            return self.sequence_head(None, remembered);
        };
        let keyword = &self.keywords[index];
        let sequence: Vec<&Keyword> = self.keywords.iter().filter(|other| other.head == keyword.head).collect();
        if sequence.last().is_some_and(|last| last.name == keyword.name) {
            return None;
        }
        let next = self.keywords.get(index + 1).map(|keyword| keyword.name.as_str());
        if keyword.interpretation == Interpretation::Type && !repeated {
            let done = sequence.iter().find(|keyword| keyword.done).map(|keyword| keyword.name.as_str());
            return done
                .or_else(|| self.keywords.iter().find(|keyword| keyword.done).map(|keyword| keyword.name.as_str()));
        }
        next
    }

    /// The head of the next sequence after the one of `current` (or of
    /// `remembered`), back to the first after the last
    /// (`org-todo` with `nextset`).
    pub fn next_set(&self, current: Option<&str>, remembered: Option<&str>) -> Option<&str> {
        let heads = self.heads();
        let index = self.set_of(current, remembered).and_then(|head| heads.iter().position(|other| *other == head));
        index.and_then(|index| heads.get(index + 1)).or(heads.first()).copied()
    }

    /// The head of the sequence of `current`, or `remembered`; none for
    /// an entry with neither (`member` of `nil` in `org-todo-heads`).
    fn set_of<'a>(&'a self, current: Option<&str>, remembered: Option<&str>) -> Option<&'a str> {
        match current {
            Some(word) => self.head(word),
            None => remembered.and_then(|head| self.head(head)),
        }
    }

    /// The head of the sequence before the one of `current` (or of
    /// `remembered`), round to the last (`org-todo` with `previousset`).
    pub fn previous_set(&self, current: Option<&str>, remembered: Option<&str>) -> Option<&str> {
        let heads = self.heads();
        let index = self.set_of(current, remembered).and_then(|head| heads.iter().position(|other| *other == head));
        match index {
            Some(index) if index > 0 => Some(heads[index - 1]),
            _ => heads.last().copied(),
        }
    }

    /// Where `head`'s sequence is among them, from 1, and how many there
    /// are, for `Keyword-Set 2/3: …`.
    pub fn set_number(&self, head: &str) -> Option<(usize, usize)> {
        let heads = self.heads();
        heads.iter().position(|other| *other == head).map(|index| (index + 1, heads.len()))
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
        self.index.get(word).copied()
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
    fn cycles_within_a_sequence() {
        let text = "#+TODO: TODO NEXT | DONE CANCELLED\n#+TODO: REPORT BUG | FIXED\n#+TYP_TODO: Fred Sara | FINISHED\n";
        let kw = TodoKeywords::from_document(text).unwrap();
        assert_eq!(kw.heads(), ["TODO", "REPORT", "Fred"]);
        assert_eq!(kw.cycle(Some("NEXT"), None, false), Some("DONE"));
        assert_eq!(kw.cycle(Some("DONE"), None, false), Some("CANCELLED"));
        assert_eq!(kw.cycle(Some("CANCELLED"), None, false), None, "the last state of the sequence clears it");
        assert_eq!(kw.cycle(None, Some("REPORT"), false), Some("REPORT"), "back to the sequence it was in");
        assert_eq!(kw.cycle(None, None, false), Some("TODO"));
        assert_eq!(kw.cycle(Some("BUG"), None, false), Some("FIXED"));
        assert_eq!(kw.cycle(Some("FIXED"), None, false), None);
        // A type goes to done at once, and on to the next when repeated.
        assert_eq!(kw.cycle(Some("Fred"), None, false), Some("FINISHED"));
        assert_eq!(kw.cycle(Some("Fred"), None, true), Some("Sara"));
        assert_eq!(kw.cycle(Some("FINISHED"), None, false), None);
        // Shift-right still walks all keywords.
        assert_eq!(kw.next(Some("CANCELLED")), Some("REPORT"));
    }

    #[test]
    fn switches_sets() {
        let text = "#+TODO: TODO | DONE\n#+TODO: REPORT | FIXED\n#+TODO: A | B\n";
        let kw = TodoKeywords::from_document(text).unwrap();
        assert_eq!(kw.next_set(Some("DONE"), None), Some("REPORT"));
        assert_eq!(kw.next_set(Some("B"), None), Some("TODO"), "round to the first");
        assert_eq!(kw.next_set(None, Some("REPORT")), Some("A"));
        assert_eq!(kw.next_set(None, None), Some("TODO"), "none remembered: the first");
        assert_eq!(kw.previous_set(Some("TODO"), None), Some("A"));
        assert_eq!(kw.previous_set(Some("FIXED"), None), Some("TODO"));
        assert_eq!(kw.set_number("REPORT"), Some((2, 3)));
        assert_eq!(kw.sequence("REPORT"), ["REPORT", "FIXED"]);
        let kw = TodoKeywords::from_sequences([
            (Interpretation::Sequence, vec!["TODO", "|", "DONE"]),
            (Interpretation::Type, vec!["Me", "You", "|", "OK"]),
        ]);
        assert_eq!(kw.cycle(Some("You"), None, false), Some("OK"));
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
