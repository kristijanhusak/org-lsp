//! `man:` and `info:` links (`ol-man.el`, `ol-info.el`): what they name,
//! and where following them leads. Emacs shows a man page or an Info node
//! in a buffer of its own; the server writes the page as text in a file of
//! its own, and opens the GNU manuals on the web, as Org's export links
//! them (`org-info-export`).

use std::sync::LazyLock;

use regex::Regex;

/// `org-man-open`: the page of a `man:` link and the text to look for in
/// it, after `::`.
pub fn man_link(path: &str) -> (String, Option<String>) {
    match path.split_once("::") {
        Some((page, search)) => (page.to_string(), Some(search.to_string())),
        None => (path.to_string(), None),
    }
}

/// The arguments `man` gets for a page, as Emacs's `man` reads it
/// (`Man-translate-references`): blanks cleaned up, and `chmod(2V)`
/// turned into `2v chmod`.
pub fn man_arguments(page: &str) -> Vec<String> {
    static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
        let name = r"[-[:alnum:]_\u{ad}+][-[:alnum:]_.:\u{ad}+]*";
        Regex::new(&format!(r"\A({name}(?:[ \t]+{name})*)[ \t]*\(([0-9][a-zA-Z0-9+]*|[LNln])\)\z"))
            .expect("a valid regex")
    });
    // `Man-translate-cleanup`.
    let cleaned = page.trim_matches([' ', '\t', '\x0c', '\r', '\n']).replace(['\t', '\r', '\n'], " ");
    let mut cleaned = cleaned.as_str().to_string();
    while cleaned.contains("  ") {
        cleaned = cleaned.replace("  ", " ");
    }
    let translated = match REFERENCE.captures(&cleaned) {
        Some(found) => {
            let name: String = found[1].chars().filter(|c| !matches!(c, '\n' | '\t' | ' ')).collect();
            format!("{} {name}", found[2].to_lowercase())
        }
        None => cleaned,
    };
    translated.split(' ').filter(|word| !word.is_empty()).map(str::to_string).collect()
}

/// Why the arguments of a `man:` link are not given to `man`: one of them
/// is an option (`man:-l /etc/hostname` would read any file as a page).
/// Emacs passes them on; org-lsp gives a link pages and sections only.
pub fn man_refused(arguments: &[String]) -> Option<String> {
    let option = arguments.iter().find(|word| word.starts_with('-'))?;
    Some(format!("A man: link names pages, not options: {option}"))
}

/// What Emacs says when `man` finds no page (`Man-bgproc-sentinel`), the
/// options left out of the arguments (`Man-page-from-arguments`).
pub fn man_not_found(arguments: &[String]) -> String {
    let page: Vec<&str> = arguments.iter().map(String::as_str).filter(|word| !word.starts_with('-')).collect();
    format!("Can’t find the {} manpage", page.join(" "))
}

/// A man page as `man` writes it to a pipe, made plain text as `col -b`
/// makes it: a character and a backspace before another (bold, underlined)
/// leave the last.
pub fn plain_page(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\u{8}' {
            out.pop();
        } else {
            out.push(c);
        }
    }
    // And the colors some `man` programs write all the same.
    static ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("a valid regex"));
    ESCAPE.replace_all(&out, "").into_owned()
}

/// `org-man-open` with a search: the line (from 0) point is left on, the
/// line before the first match, found without regard to case
/// (`search-forward` with `case-fold-search`); else Emacs's error.
pub fn man_search_line(page: &str, search: &str) -> Result<usize, String> {
    let lower = page.to_lowercase();
    let wanted = search.to_lowercase();
    // Lowercase text may have other lengths: lines are counted in it.
    let at = lower.find(&wanted).ok_or_else(|| format!("Search failed: \"{search}\""))?;
    // Point after the match, then `forward-line -1`.
    let line = lower[..at + wanted.len()].matches('\n').count();
    Ok(line.saturating_sub(1))
}

/// `org-info--link-file-node`: the Info file and node of an `info:` link,
/// `dir` and `Top` when not given.
pub fn info_file_node(path: &str) -> (String, String) {
    static FILE_NODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A([^#:]*)(?:[#:]:?(.*))?\z").expect("a valid regex"));
    let trim = |text: &str| text.trim_matches([' ', '\t', '\n', '\r']).to_string();
    let Some(found) = FILE_NODE.captures(path) else { return ("dir".to_string(), "Top".to_string()) };
    let file = found.get(1).map(|file| trim(file.as_str())).filter(|file| !file.is_empty());
    let node = found.get(2).map(|node| trim(node.as_str())).filter(|node| !node.is_empty());
    (file.unwrap_or_else(|| "dir".to_string()), node.unwrap_or_else(|| "Top".to_string()))
}

/// `org-info-emacs-documents`: the manuals of Emacs on the web.
pub const EMACS_DOCUMENTS: &[&str] = &[
    "ada-mode",
    "auth",
    "autotype",
    "bovine",
    "calc",
    "ccmode",
    "cl",
    "dbus",
    "dired-x",
    "ebrowse",
    "ede",
    "ediff",
    "edt",
    "efaq-w32",
    "efaq",
    "eglot",
    "eieio",
    "eintr",
    "elisp",
    "emacs-gnutls",
    "emacs-mime",
    "emacs",
    "epa",
    "erc",
    "ert",
    "eshell",
    "eudc",
    "eww",
    "flymake",
    "forms",
    "gnus",
    "htmlfontify",
    "idlwave",
    "ido",
    "info",
    "mairix-el",
    "message",
    "mh-e",
    "modus-themes",
    "newsticker",
    "nxml-mode",
    "octave-mode",
    "org",
    "pcl-cvs",
    "pgg",
    "rcirc",
    "reftex",
    "remember",
    "sasl",
    "sc",
    "semantic",
    "ses",
    "sieve",
    "smtpmail",
    "speedbar",
    "srecode",
    "todo-mode",
    "tramp",
    "transient",
    "url",
    "use-package",
    "vhdl-mode",
    "vip",
    "viper",
    "vtable",
    "widget",
    "wisent",
    "woman",
];

/// `org-info-other-documents` by default: other manuals on the web.
pub fn default_other_documents() -> Vec<(String, String)> {
    [
        ("dir", "https://www.gnu.org/manual/manual.html"),
        ("libc", "https://www.gnu.org/software/libc/manual/html_mono/libc.html"),
        ("make", "https://www.gnu.org/software/make/manual/make.html"),
    ]
    .iter()
    .map(|(file, url)| (file.to_string(), url.to_string()))
    .collect()
}

/// `org-info-map-html-url`: the page of the Info manual `file` on the web,
/// as `other` (`org-info-other-documents`) names it or as the Emacs
/// manuals are; else `FILE.html`, which Org's export puts next to the
/// exported file.
pub fn map_html_url(file: &str, other: &[(String, String)]) -> String {
    if let Some((_, url)) = other.iter().find(|(name, _)| name == file) {
        return url.clone();
    }
    if EMACS_DOCUMENTS.contains(&file) {
        return format!("https://www.gnu.org/software/emacs/manual/html_mono/{file}.html");
    }
    format!("{file}.html")
}

/// Whether the manual `file` is on the web, at [`map_html_url`].
pub fn on_the_web(file: &str, other: &[(String, String)]) -> bool {
    other.iter().any(|(name, _)| name == file) || EMACS_DOCUMENTS.contains(&file)
}

/// `org-info--expand-node-name`: an Info node as the anchor of its HTML
/// page (Texinfo's "HTML Xref Node Name Expansion").
pub fn expand_node_name(node: &str) -> String {
    let mut out = String::new();
    let mut blanks = false;
    for c in node.trim_matches([' ', '\t', '\n', '\r']).chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            if !blanks {
                out.push('-');
            }
            blanks = true;
            continue;
        }
        blanks = false;
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push_str(&format!("_{:04x}", c as u32));
        }
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "g_t");
    }
    out
}

/// The page of an Info node as Org's export links it (`org-info-export`):
/// the manual's page and the node's anchor. Following an `info:` link to a
/// manual on the web ([`on_the_web`]) opens it.
pub fn info_url(file: &str, node: &str, other: &[(String, String)]) -> String {
    format!("{}#{}", map_html_url(file, other), expand_node_name(node))
}

/// What Emacs says of an Info file that is not there (`Info-find-node`).
pub fn info_missing(file: &str) -> String {
    format!("Info file ‘{file}’ does not exist; consider installing it")
}

/// What Org says of a node that is neither a node nor an index entry
/// (`org-info-follow-link`).
pub fn info_node_missing(node: &str) -> String {
    format!("Could not find ’{node}’ node or index entry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn man_references_become_section_and_name() {
        assert_eq!(man_arguments("printf(3)"), ["3", "printf"]);
        assert_eq!(man_arguments("  ls   1 "), ["ls", "1"]);
        assert_eq!(man_arguments("chmod(2V)"), ["2v", "chmod"]);
        assert_eq!(man_not_found(&man_arguments("-a ls")), "Can’t find the ls manpage");
    }

    #[test]
    fn man_links_give_no_options() {
        for page in ["ls", "ls(1)", "5 crontab", "chmod(2V)", "git-log"] {
            assert_eq!(man_refused(&man_arguments(page)), None, "{page}");
        }
        assert_eq!(
            man_refused(&man_arguments("-l /etc/hostname")),
            Some("A man: link names pages, not options: -l".to_string())
        );
        assert!(man_refused(&man_arguments("ls --html=firefox")).is_some());
    }

    #[test]
    fn pages_lose_their_overstrikes() {
        assert_eq!(plain_page("N\u{8}NA\u{8}AME\n_\u{8}x"), "NAME\nx");
    }
}
