//! Whether Org may download a remote resource, as `org.el` decides
//! (`org--should-fetch-remote-resource-p`): by `org-resource-download-policy`,
//! the patterns of `org-safe-remote-resources`
//! (`org--safe-remote-resource-p`), and else by asking
//! (`org--confirm-resource-safe`), whose answers may mark the resource, its
//! domain or the file asking as safe from then on.

use crate::agenda::matcher::emacs_regex;

/// `org-resource-download-policy`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// `t`: always download.
    Always,
    /// `prompt`: ask about a resource not considered safe.
    #[default]
    Prompt,
    /// `safe`: only the resources considered safe.
    Safe,
    /// `nil`: never download.
    Never,
}

/// What [`should_fetch`] decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetch {
    Yes,
    No,
    /// The user is to be asked first ([`confirm`]).
    Ask,
}

/// `org--should-fetch-remote-resource-p`: whether `uri` may be downloaded
/// for the Org file `file` (its true name, as `file-truename` gives it),
/// by `policy` and the regexps `safe`.
pub fn should_fetch(policy: Policy, uri: &str, file: Option<&str>, safe: &[String]) -> Fetch {
    if policy == Policy::Always || safe_remote_resource(uri, file, safe) {
        Fetch::Yes
    } else if policy == Policy::Prompt {
        Fetch::Ask
    } else {
        Fetch::No
    }
}

/// `org--safe-remote-resource-p`: whether one of the regexps `safe`
/// matches `uri`, or the file asking for it as a `file://` URI. Matched as
/// `string-match-p` does, without regard to case.
pub fn safe_remote_resource(uri: &str, file: Option<&str>, safe: &[String]) -> bool {
    let file_uri = file.map(|file| format!("file://{file}"));
    safe.iter().any(|pattern| {
        emacs_regex(pattern).is_some_and(|regex| {
            regex.is_match(uri) || file_uri.as_deref().is_some_and(|file_uri| regex.is_match(file_uri))
        })
    })
}

/// The domain of `uri`, as `org--confirm-resource-safe` reads it: the
/// scheme, the user if any, and the host, without the port or path.
pub fn domain(uri: &str) -> Option<String> {
    static DOMAIN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"https?://(?:[^@/\n]+@)?(?:www\.)?[^:/?\n]+").expect("valid regexp")
    });
    DOMAIN.find(uri).map(|found| found.as_str().to_string())
}

/// What `org--confirm-resource-safe` asks: the text of its
/// `*Org Remote Resource*` buffer, the prompt read with it, and the keys
/// offered in the buffer with what each does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub text: String,
    pub prompt: String,
    pub choices: Vec<(char, String)>,
}

/// The question `org--confirm-resource-safe` asks about `uri`, for the
/// file `file` (its true name), if any.
pub fn confirm(uri: &str, file: Option<&str>) -> Question {
    let domain = domain(uri);
    let mut choices = vec![('!', "to download this resource, and permanently mark it as safe.".to_string())];
    if let Some(domain) = &domain {
        choices.push(('d', format!("to download this resource, and mark the domain ({domain}) as safe.")));
    }
    if let Some(file) = file {
        choices
            .push(('f', format!("to download this resource, and permanently mark all resources in {file} as safe.")));
    }
    choices.push(('y', "to download this resource, just this once.".to_string()));
    choices.push(('n', "to skip this resource.".to_string()));
    let mut text = format!(
        "An org-mode document would like to download {uri}, which is not considered safe.\n\n\
         Do you want to download this?  You can type\n"
    );
    for (key, label) in &choices {
        text.push_str(&format!(" {key} {label}\n"));
    }
    // The window is taken to show the whole buffer: no "C-v/M-v to scroll".
    let prompt = format!("Please type y, n{}, d, or !: ", if file.is_some() { ", f" } else { "" });
    Question { text, prompt, choices }
}

/// Whether the answer `key` to [`confirm`] downloads the resource: `y`,
/// `!`, `d`, `f` and a space do.
pub fn answer_fetches(key: char) -> bool {
    matches!(key, 'y' | '!' | 'd' | ' ' | 'f')
}

/// The regexp the answer `key` adds to `org-safe-remote-resources`, if it
/// adds one: for `!` the resource, for `d` its domain, for `f` the file
/// asking (or the resource, without a file).
pub fn safe_pattern(key: char, uri: &str, file: Option<&str>) -> Option<String> {
    match key {
        'd' => domain(uri).map(|domain| format!("\\`{}\\(?:/\\|\\'\\)", regexp_quote(&domain))),
        '!' | 'f' => {
            let what = match (key, file) {
                ('f', Some(file)) => format!("file://{file}"),
                _ => uri.to_string(),
            };
            Some(format!("\\`{}\\'", regexp_quote(&what)))
        }
        _ => None,
    }
}

/// `regexp-quote`
fn regexp_quote(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "[*.\\?+^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
