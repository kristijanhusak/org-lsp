//! `org-file-apps`: what opens the file of a `file:` link, as
//! `org-open-file` picks it. The editor stands for Emacs; the system's
//! application (`system`, `default`, `mailcap` and the systems' own
//! defaults) is the editor's way to open a file outside it.

use crate::agenda::matcher::translate_capturing;
use regex::{Regex, RegexBuilder};

/// What identifies files, the car of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// An extension (only letters and digits), or a regexp: on the file
    /// name, or with groups on the whole link.
    Pattern(String),
    /// `directory`
    Directory,
    /// `remote`
    Remote,
    /// `auto-mode`: the files Emacs has a mode for, all of them here.
    AutoMode,
    /// `system`: the command of `system` in a command.
    System,
    /// `t`: any other file.
    Other,
}

/// What opens them, the cdr of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum App {
    /// `emacs`: the editor.
    Emacs,
    /// `default`: the command of `t`.
    Default,
    /// `system`: the command of `system`.
    System,
    /// `mailcap`, and the systems' defaults: the system's application.
    Mailcap,
    /// A shell command, `%s` the file, `%1` to `%9` the groups.
    Command(String),
}

/// `org-file-apps` and the system's defaults after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileApps(pub Vec<(Selector, App)>);

impl FileApps {
    /// The entries as settings write them: a car of `directory`,
    /// `remote`, `auto-mode`, `system` or `t` is Emacs's symbol, as a cdr
    /// of `emacs`, `default`, `system` or `mailcap` is.
    pub fn from_pairs(pairs: &[(String, String)]) -> FileApps {
        FileApps(
            pairs
                .iter()
                .map(|(key, command)| {
                    let selector = match key.as_str() {
                        "directory" => Selector::Directory,
                        "remote" => Selector::Remote,
                        "auto-mode" => Selector::AutoMode,
                        "system" => Selector::System,
                        "t" => Selector::Other,
                        pattern => Selector::Pattern(pattern.to_string()),
                    };
                    let app = match command.as_str() {
                        "emacs" => App::Emacs,
                        "default" => App::Default,
                        "system" => App::System,
                        "mailcap" => App::Mailcap,
                        command => App::Command(command.to_string()),
                    };
                    (selector, app)
                })
                .collect(),
        )
    }
}

impl Default for FileApps {
    /// Org's default.
    fn default() -> Self {
        FileApps(vec![
            (Selector::AutoMode, App::Emacs),
            (Selector::Directory, App::Emacs),
            (Selector::Pattern(r"\.mm\'".to_string()), App::Default),
            (Selector::Pattern(r"\.x?html?\'".to_string()), App::Default),
            (Selector::Pattern(r"\.pdf\'".to_string()), App::Default),
        ])
    }
}

/// How the file is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opener {
    /// In the editor.
    Editor,
    /// By the system's application.
    System,
    /// By this shell command.
    Command(String),
}

/// The prefix argument of `org-open-at-point`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prefix {
    #[default]
    None,
    /// `C-u`: in the editor.
    Editor,
    /// `C-u C-u`: by the system's application.
    System,
}

/// The file at hand: its absolute path, what follows `::` in the link,
/// and what is there.
pub struct Target<'a> {
    pub file: &'a str,
    pub search: Option<&'a str>,
    pub exists: bool,
    pub directory: bool,
}

/// `org-open-file`: what opens `target`, or Org's error. A file that is not
/// there opens only in Emacs, unless `non_existing`
/// (`org-open-non-existing-files`).
pub fn open(apps: &FileApps, target: &Target<'_>, prefix: Prefix, non_existing: bool) -> Result<Opener, String> {
    // The system's own, after the user's (`org-file-apps-gnu`).
    let mut all = apps.0.clone();
    all.extend([(Selector::Remote, App::Emacs), (Selector::System, App::Mailcap), (Selector::Other, App::Mailcap)]);
    let of = |selector: &Selector| all.iter().find(|(key, _)| key == selector).map(|(_, app)| app.clone());
    let file = target.file;
    let link = match target.search {
        Some(search) => format!("{file}::{search}"),
        None => file.to_string(),
    };
    let lower = file.to_lowercase();
    let mut groups: Vec<String> = Vec::new();
    let app = match prefix {
        Prefix::System => of(&Selector::System),
        Prefix::Editor => Some(App::Emacs),
        Prefix::None => {
            let directory = if target.directory { of(&Selector::Directory) } else { None };
            directory
                .or_else(|| {
                    // A regexp with groups and a command using them: on the
                    // whole link.
                    all.iter().find_map(|(key, app)| {
                        let Selector::Pattern(pattern) = key else { return None };
                        let App::Command(command) = app else { return None };
                        if !has_group(pattern) || !uses_group(command) {
                            return None;
                        }
                        let found = regex(pattern, true)?.captures(&link)?;
                        groups = (1..found.len().min(10))
                            .map(|index| found.get(index).map_or(String::new(), |group| group.as_str().to_string()))
                            .collect();
                        Some(app.clone())
                    })
                })
                .or_else(|| {
                    let locator = |(key, app): &&(Selector, App)| match (key, app) {
                        (Selector::Pattern(pattern), App::Command(command)) => {
                            has_group(pattern) && uses_group(command)
                        }
                        _ => false,
                    };
                    let mut patterns = all.iter().filter(|entry| !locator(entry)).filter_map(|(key, app)| match key {
                        Selector::Pattern(pattern) => Some((file_pattern(pattern), app.clone())),
                        _ => None,
                    });
                    let found =
                        patterns.find(|(pattern, _)| regex(pattern, false).is_some_and(|re| re.is_match(&lower)));
                    found.map(|(_, app)| app).or_else(|| of(&Selector::AutoMode).map(|_| App::Emacs))
                })
                .or_else(|| {
                    // The extension, as the key is written.
                    let extension = extension(&lower)?;
                    all.iter().find_map(|(key, app)| match key {
                        Selector::Pattern(pattern) if *pattern == extension => Some(app.clone()),
                        _ => None,
                    })
                })
                .or_else(|| of(&Selector::Other))
        }
    };
    let app = match app {
        Some(App::System) => of(&Selector::System),
        Some(App::Default) => of(&Selector::Other),
        app => app,
    };
    // Only Emacs visits a file that is not there, unless
    // `org-open-non-existing-files`.
    if app != Some(App::Emacs) && !target.exists && !non_existing {
        return Err(format!("No such file: {file}"));
    }
    Ok(match app {
        Some(App::Command(command)) => Opener::Command(format_command(&command, file, &groups)?),
        Some(App::Mailcap) => Opener::System,
        // `system` or `default` again, or nothing: Emacs visits the file.
        _ => Opener::Editor,
    })
}

/// `regexp-opt-depth` above 0: a group that is not shy.
fn has_group(pattern: &str) -> bool {
    pattern.match_indices(r"\(").any(|(at, _)| !pattern[at + 2..].starts_with('?'))
}

fn uses_group(command: &str) -> bool {
    command.as_bytes().windows(2).any(|pair| pair[0] == b'%' && pair[1].is_ascii_digit())
}

/// `org--file-apps-regexp-alist`: an extension as a regexp on the end of
/// the file name; anything with other characters is a regexp already.
fn file_pattern(key: &str) -> String {
    if key.chars().any(|c| !(c.is_alphanumeric() || c == '_')) {
        key.to_string()
    } else {
        format!(r"\.{key}\'")
    }
}

/// An Emacs regexp, matched without regard to case.
fn regex(pattern: &str, capturing: bool) -> Option<Regex> {
    let translated = if capturing { translate_capturing(pattern) } else { crate::agenda::matcher::translate(pattern) };
    RegexBuilder::new(&translated).case_insensitive(true).build().ok()
}

/// The extension `org-open-file` takes: after the first dot, letters and
/// digits, and `.gz` after them.
fn extension(file: &str) -> Option<String> {
    let re = Regex::new(r"^.*?\.([a-zA-Z0-9]+(\.gz)?)$").expect("a valid regex");
    re.captures(file).map(|found| found[1].to_string())
}

/// `shell-quote-argument` for the shell commands run in: `cmd` on
/// Windows, a POSIX shell elsewhere.
pub fn shell_quote(argument: &str) -> String {
    shell_quote_for(argument, cfg!(windows))
}

/// `shell-quote-argument` as on Windows (`windows-nt` with
/// `w32-shell-dos-semantics`, for `cmd`) or as on a POSIX shell.
pub fn shell_quote_for(argument: &str, windows: bool) -> String {
    if windows {
        shell_quote_w32(argument)
    } else {
        shell_quote_posix(argument)
    }
}

/// `shell-quote-argument` on Windows: quoted for `CommandLineToArgvW`
/// (the backslashes before a double quote and at the end of a line
/// doubled, the quote escaped), then put in double quotes; with a `%`, a
/// `!` or a double quote in it, those quotes and each of `cmd`'s special
/// characters come after a caret.
fn shell_quote_w32(argument: &str) -> String {
    let mut quoted = String::new();
    let mut backslashes = 0;
    for c in argument.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        let doubled = if matches!(c, '"' | '\n') { backslashes * 2 } else { backslashes };
        quoted.extend(std::iter::repeat_n('\\', doubled));
        if c == '"' {
            quoted.push('\\');
        }
        quoted.push(c);
        backslashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    if !quoted.contains(['%', '!', '"']) {
        return format!("\"{quoted}\"");
    }
    let mut out = String::from("^\"");
    for c in quoted.chars() {
        if matches!(c, '%' | '!' | '(' | ')' | '"' | '<' | '>' | '&' | '|' | '^') {
            out.push('^');
        }
        out.push(c);
    }
    out.push_str("^\"");
    out
}

/// `shell-quote-argument` on a POSIX shell.
fn shell_quote_posix(argument: &str) -> String {
    if argument.is_empty() {
        return "''".to_string();
    }
    let mut out = String::new();
    for c in argument.chars() {
        if c == '\n' {
            out.push_str("'\n'");
        } else if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
    out
}

/// `org--open-file-format-command`: `%s` the file and `%1` to `%9` the
/// groups, each quoted for the shell (quotes around them dropped), `\x`
/// the character `x`.
fn format_command(command: &str, file: &str, groups: &[String]) -> Result<String, String> {
    let chars: Vec<char> = command.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\\' {
            if let Some(next) = chars.get(at + 1) {
                out.push(*next);
            }
            at += 2;
            continue;
        }
        let quote = (c == '\'' || c == '"').then_some(c);
        let percent = if quote.is_some() { at + 1 } else { at };
        if chars.get(percent) != Some(&'%') {
            out.push(c);
            at += 1;
            continue;
        }
        let key = chars.get(percent + 1).copied();
        let value = match key {
            Some('s') => Some(file.to_string()),
            Some(digit @ '1'..='9') => groups.get(digit as usize - '1' as usize).cloned(),
            _ => None,
        };
        let Some(value) = value else {
            let end = (percent + 2).min(chars.len());
            return Err(format!("Invalid format ‘{}’", chars[at..end].iter().collect::<String>()));
        };
        let quoted = shell_quote(&value);
        let mut end = percent + 2;
        match quote {
            Some(quote) if chars.get(end) == Some(&quote) => {
                end += 1;
                out.push_str(&quoted);
            }
            Some(quote) => {
                out.push(quote);
                out.push_str(&quoted);
            }
            None => out.push_str(&quoted),
        }
        at = end;
    }
    Ok(out)
}
