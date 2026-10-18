//! `org-ctags`: the `<<targets>>` of the Org files of a directory as tags
//! of a `TAGS` file, made by `ctags`, and links that are not found in
//! their file followed to them (`org-open-link-functions`).
//!
//! What following a link does is a list of functions tried in turn, as
//! `org-ctags-open-link-functions` names them, until one says it is done:
//! [`follow`] runs them, with the answers given so far to the questions
//! they ask, and says what to do next.

/// A tag of a `TAGS` file (Emacs's etags format, which `ctags -e` writes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Its name.
    pub name: String,
    /// The file it is in, as the `TAGS` file names it: relative to the
    /// directory of the `TAGS` file, or absolute.
    pub file: String,
    /// Its line, from 1.
    pub line: usize,
}

/// The tags of a `TAGS` file: after each form feed, a line `FILE,SIZE`,
/// then a line per tag, `TEXT DEL NAME ^A LINE,POS`, or without a name,
/// which is then the end of the text (an implicit name).
pub fn read_tags(text: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    for section in text.split('\x0c').skip(1) {
        let mut lines = section.strip_prefix('\n').unwrap_or(section).split('\n');
        let Some(header) = lines.next() else { continue };
        let Some((file, _)) = header.rsplit_once(',') else { continue };
        for line in lines {
            let Some((pattern, rest)) = line.split_once('\x7f') else { continue };
            let (name, place) = match rest.split_once('\x01') {
                Some((name, place)) => (name.to_string(), place),
                None => (implicit_name(pattern), rest),
            };
            let number = place.split(',').next().and_then(|number| number.parse().ok());
            if let Some(line) = number {
                tags.push(Tag { name, file: file.to_string(), line });
            }
        }
    }
    tags
}

/// The name a tag without one has: the symbol its text ends with.
fn implicit_name(pattern: &str) -> String {
    let pattern = pattern.trim_end_matches([' ', '\t']);
    let start = pattern.rfind(['\n', ' ', '(', ')', '{', '}', '=', ';']).map_or(0, |at| at + 1);
    pattern[start..].to_string()
}

/// `xref-find-definitions` with the etags backend, as `org-ctags-find-tag`
/// calls it: the first tag of this name, without regard to case
/// (`tags-case-fold-search`).
pub fn find<'a>(tags: &'a [Tag], name: &str) -> Option<&'a Tag> {
    let wanted = name.to_lowercase();
    tags.iter().find(|tag| tag.name.to_lowercase() == wanted)
}

/// `org-ctags-tag-regexp`: what `ctags` makes a tag of in an Org file.
pub const TAG_REGEXP: &str = "/<<([^<>]+)>>/\\1/d,definition/";

/// `org-ctags-create-tags`: the shell command that makes `DIRECTORY/TAGS`
/// with `ctags` (`org-ctags-path-to-ctags`), for the files of `directory`
/// (with its slash at the end) and of its subdirectories.
pub fn create_command(ctags: &str, directory: &str) -> String {
    let directory = crate::file_apps::shell_quote(directory);
    format!(
        "{ctags} --langdef=orgmode --langmap=orgmode:.org --regex-orgmode={} -f {directory}TAGS -e -R {directory}*",
        crate::file_apps::shell_quote(TAG_REGEXP)
    )
}

/// `capitalize`: each word with its first letter upper case, the others
/// lower case. Letters and digits make words.
pub fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_word = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if in_word {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            in_word = true;
        } else {
            out.push(c);
            in_word = false;
        }
    }
    out
}

/// `org-ctags-new-topic-template` for `name`: `%t` is its name,
/// capitalized.
pub fn topic(template: &str, name: &str) -> String {
    template.replace("%t", &capitalize(name))
}

/// `org-ctags-append-topic`: a new topic at the end of `text`, after two
/// newlines, and where point goes, two lines below the end of the line
/// four characters before the end.
pub fn append_topic(text: &str, name: &str, template: &str) -> (String, usize) {
    let mut out = format!("{text}\n\n{}", topic(template, name));
    // `backward-char 4`, `end-of-line`, `forward-line 2`.
    let mut point = out.len();
    for _ in 0..4 {
        point = out[..point].char_indices().next_back().map_or(0, |(at, _)| at);
    }
    point = out[point..].find('\n').map_or(out.len(), |at| point + at);
    for _ in 0..2 {
        match out[point..].find('\n') {
            Some(at) => point += at + 1,
            None => point = out.len(),
        }
    }
    let point = point.min(out.len());
    out.shrink_to_fit();
    (out, point)
}

/// The functions of `org-ctags-open-link-functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    /// `org-ctags-find-tag`
    FindTag,
    /// `org-ctags-ask-rebuild-tags-file-then-find-tag`
    AskRebuildThenFindTag,
    /// `org-ctags-rebuild-tags-file-then-find-tag`
    RebuildThenFindTag,
    /// `org-ctags-ask-append-topic`
    AskAppendTopic,
    /// `org-ctags-append-topic`
    AppendTopic,
    /// `org-ctags-ask-visit-buffer-or-file`
    AskVisitBufferOrFile,
    /// `org-ctags-visit-buffer-or-file`
    VisitBufferOrFile,
    /// `org-ctags-fail-silently`
    FailSilently,
}

impl Function {
    /// The function of this name.
    pub fn named(name: &str) -> Option<Function> {
        Some(match name {
            "org-ctags-find-tag" => Function::FindTag,
            "org-ctags-ask-rebuild-tags-file-then-find-tag" => Function::AskRebuildThenFindTag,
            "org-ctags-rebuild-tags-file-then-find-tag" => Function::RebuildThenFindTag,
            "org-ctags-ask-append-topic" => Function::AskAppendTopic,
            "org-ctags-append-topic" => Function::AppendTopic,
            "org-ctags-ask-visit-buffer-or-file" => Function::AskVisitBufferOrFile,
            "org-ctags-visit-buffer-or-file" => Function::VisitBufferOrFile,
            "org-ctags-fail-silently" => Function::FailSilently,
            _ => return None,
        })
    }
}

/// The default of `org-ctags-open-link-functions`.
pub const DEFAULT_FUNCTIONS: &[Function] =
    &[Function::FindTag, Function::AskRebuildThenFindTag, Function::AskAppendTopic];

/// The default of `org-ctags-new-topic-template`.
pub const DEFAULT_TEMPLATE: &str = "* <<%t>>\n\n\n\n\n\n";

/// What following a link to `name` has to work with.
pub struct Context<'a> {
    /// The functions to try, in order.
    pub functions: &'a [Function],
    /// The answers to the questions asked so far, yes or no.
    pub answers: &'a [bool],
    /// The directory of the link's file, with its slash at the end
    /// (`file-name-directory`); none for a buffer without a file.
    pub directory: Option<&'a str>,
    /// The text of the link's file.
    pub text: &'a str,
    /// `org-ctags-new-topic-template`
    pub template: &'a str,
}

/// What the functions do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Go to this tag.
    Found(Tag),
    /// Ask this, yes or no (`y-or-n-p`), then run them again with the
    /// answer.
    Ask(String),
    /// The text with a new topic at its end, and where point goes.
    Append(String, usize),
    /// Open this Org file, which is there.
    Visit(String),
    /// Make this Org file with this text, and go to its end.
    Create(String, String),
    /// Done, nothing more to do (`org-ctags-fail-silently`).
    Done,
    /// None of them was done: Org follows the link as it would without
    /// them.
    Pass,
}

/// `run-hook-with-args-until-success` on `org-open-link-functions` with
/// org-ctags's functions. `tags` reads the `TAGS` file of the directory,
/// after making it again first when it is given true; `exists` says
/// whether a file is there.
pub fn follow(
    name: &str,
    context: &Context,
    tags: &mut dyn FnMut(bool) -> Vec<Tag>,
    exists: &dyn Fn(&str) -> bool,
) -> Outcome {
    let mut answers = context.answers.iter().copied();
    // Asks, or takes the answer given already.
    macro_rules! ask {
        ($prompt:expr) => {
            match answers.next() {
                Some(answer) => answer,
                None => return Outcome::Ask($prompt),
            }
        };
    }
    for function in context.functions {
        match function {
            Function::FindTag => {
                if let Some(tag) = find(&tags(false), name) {
                    return Outcome::Found(tag.clone());
                }
            }
            Function::AskRebuildThenFindTag | Function::RebuildThenFindTag => {
                let Some(directory) = context.directory else { continue };
                if *function == Function::AskRebuildThenFindTag
                    && !ask!(format!("Tag ‘{name}’ not found.  Rebuild table ‘{directory}/TAGS’ and look again?"))
                {
                    continue;
                }
                if let Some(tag) = find(&tags(true), name) {
                    return Outcome::Found(tag.clone());
                }
            }
            Function::AskAppendTopic | Function::AppendTopic => {
                if *function == Function::AskAppendTopic
                    && !ask!(format!("Topic ‘{name}’ not found; append to end of buffer?"))
                {
                    continue;
                }
                let (text, point) = append_topic(context.text, name, context.template);
                return Outcome::Append(text, point);
            }
            Function::AskVisitBufferOrFile | Function::VisitBufferOrFile => {
                let file = match context.directory {
                    Some(directory) if !name.starts_with('/') => format!("{directory}{name}.org"),
                    _ => format!("{name}.org"),
                };
                if exists(&file) {
                    return Outcome::Visit(file);
                }
                if *function == Function::AskVisitBufferOrFile && ask!(format!("File ‘{name}.org’ not found; create?"))
                {
                    return Outcome::Create(file, topic(context.template, name));
                }
            }
            Function::FailSilently => return Outcome::Done,
        }
    }
    Outcome::Pass
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_files_are_read() {
        let text =
            "\x0c\nb.org,40\nSome <<foo>>\x7ffoo\x013,3\n* <<bar>>\x7fbar\x014,18\n\x0c\nc.el,10\n(defun baz\x7f2,5\n";
        let tags = read_tags(text);
        assert_eq!(tags.len(), 3);
        assert_eq!(find(&tags, "FOO"), Some(&Tag { name: "foo".into(), file: "b.org".into(), line: 3 }));
        assert_eq!(find(&tags, "baz").map(|tag| tag.line), Some(2));
        assert_eq!(find(&tags, "fo"), None);
    }

    #[test]
    fn words_are_capitalized_as_emacs_does() {
        assert_eq!(capitalize("it's a fooBar foo-bar"), "It'S A Foobar Foo-Bar");
        assert_eq!(capitalize("été x2y tag_name"), "Été X2y Tag_Name");
    }
}
