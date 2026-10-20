//! Colors for the code in the docs: a small scanner for the languages
//! they show (comments, strings, numbers, keywords, keys), and Org
//! itself through org-lsp's parser.

use org_syntax::SyntaxKind::*;
use org_syntax::{SyntaxElement, SyntaxNode, TodoKeywords};

use crate::html::escape;

/// `code` of `language` as HTML, its parts in `tok-…` spans.
pub fn code(language: &str, code: &str) -> String {
    let language = language.to_lowercase();
    if language == "org" {
        return org(code);
    }
    match spec(&language) {
        Some(spec) => scan(&spec, code),
        None => escape(code),
    }
}

struct Spec {
    /// Comments to the end of the line.
    line_comments: &'static [&'static str],
    /// A comment that only starts a line (Vim's `"`).
    line_start_comment: Option<&'static str>,
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    keywords: &'static [&'static str],
    constants: &'static [&'static str],
    /// A name before `=` (TOML) or a string before `:` (JSON) is a key.
    keys: Keys,
    /// `$NAME` and `${…}` are variables (shells).
    dollar_variables: bool,
    /// `:keyword` and `'symbol` (Lisp).
    lisp_symbols: bool,
}

#[derive(PartialEq, Eq)]
enum Keys {
    None,
    BeforeEquals,
    StringBeforeColon,
}

const LUA: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local", "not", "or",
    "repeat", "return", "then", "until", "while", "require",
];
const SHELL: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "function",
    "export", "local", "return", "set", "cd", "echo", "exit", "sudo",
];
const VIM: &[&str] = &[
    "let",
    "set",
    "setlocal",
    "call",
    "function",
    "function!",
    "endfunction",
    "if",
    "else",
    "elseif",
    "endif",
    "for",
    "endfor",
    "while",
    "endwhile",
    "return",
    "autocmd",
    "augroup",
    "nnoremap",
    "nmap",
    "inoremap",
    "imap",
    "xnoremap",
    "vnoremap",
    "packadd",
    "execute",
    "silent",
    "plug",
    "Plug",
    "source",
    "filetype",
    "syntax",
];
const LISP: &[&str] = &[
    "defun",
    "defvar",
    "defcustom",
    "setq",
    "setq-default",
    "let",
    "let*",
    "lambda",
    "if",
    "when",
    "unless",
    "cond",
    "progn",
    "require",
    "use-package",
    "add-to-list",
    "add-hook",
    "with-eval-after-load",
    "and",
    "or",
    "not",
];
const PYTHON: &[&str] = &[
    "def", "class", "return", "if", "elif", "else", "for", "while", "in", "import", "from", "as", "with", "try",
    "except", "finally", "raise", "pass", "lambda", "and", "or", "not", "is", "yield", "print",
];

fn spec(language: &str) -> Option<Spec> {
    let base = Spec {
        line_comments: &[],
        line_start_comment: None,
        block_comment: None,
        quotes: &['"', '\''],
        keywords: &[],
        constants: &["true", "false"],
        keys: Keys::None,
        dollar_variables: false,
        lisp_symbols: false,
    };
    Some(match language {
        "lua" => Spec {
            line_comments: &["--"],
            block_comment: Some(("--[[", "]]")),
            keywords: LUA,
            constants: &["true", "false", "nil"],
            ..base
        },
        "sh" | "bash" | "shell" | "zsh" | "console" => {
            Spec { line_comments: &["#"], keywords: SHELL, dollar_variables: true, ..base }
        }
        "toml" => Spec { line_comments: &["#"], keys: Keys::BeforeEquals, ..base },
        "json" | "jsonc" | "json5" => Spec {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"'],
            constants: &["true", "false", "null"],
            keys: Keys::StringBeforeColon,
            ..base
        },
        "vim" | "viml" | "vimscript" => Spec {
            line_start_comment: Some("\""),
            quotes: &['\''],
            keywords: VIM,
            constants: &["v:true", "v:false"],
            ..base
        },
        "emacs-lisp" | "elisp" | "lisp" => Spec {
            line_comments: &[";"],
            quotes: &['"'],
            keywords: LISP,
            constants: &["t", "nil"],
            lisp_symbols: true,
            ..base
        },
        "python" | "py" => {
            Spec { line_comments: &["#"], keywords: PYTHON, constants: &["True", "False", "None"], ..base }
        }
        "js" | "javascript" | "ts" | "typescript" => Spec {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\'', '`'],
            keywords: &[
                "const", "let", "var", "function", "return", "if", "else", "for", "while", "import", "export", "from",
                "new", "async", "await", "class",
            ],
            constants: &["true", "false", "null", "undefined"],
            ..base
        },
        _ => return None,
    })
}

fn span(class: &str, text: &str) -> String {
    format!("<span class=\"tok-{class}\">{}</span>", escape(text))
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn scan(spec: &Spec, code: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    let mut line_start = true;
    while at < code.len() {
        let rest = &code[at..];
        let c = rest.chars().next().unwrap();
        if c == '\n' {
            out.push('\n');
            at += 1;
            line_start = true;
            continue;
        }
        if c == ' ' || c == '\t' {
            out.push(c);
            at += 1;
            continue;
        }
        let starts_line = std::mem::replace(&mut line_start, false);
        let line_end = rest.find('\n').map_or(code.len(), |end| at + end);
        if let Some((open, close)) = spec.block_comment {
            if let Some(inside) = rest.strip_prefix(open) {
                let end = inside.find(close).map_or(code.len(), |end| at + open.len() + end + close.len());
                out.push_str(&span("comment", &code[at..end]));
                at = end;
                continue;
            }
        }
        let line_comment = spec.line_comments.iter().any(|start| rest.starts_with(start))
            || (starts_line && spec.line_start_comment.is_some_and(|start| rest.starts_with(start)));
        // `#` in a shell word (`a#b`) or `$#` is not a comment.
        if line_comment && !(spec.dollar_variables && !out.is_empty() && !out.ends_with([' ', '\t', '\n', '>'])) {
            out.push_str(&span("comment", &code[at..line_end]));
            at = line_end;
            continue;
        }
        if spec.quotes.contains(&c) {
            let end = string_end(code, at, c, line_end);
            let text = &code[at..end];
            let after = code[end..].trim_start_matches([' ', '\t']);
            let class =
                if spec.keys == Keys::StringBeforeColon && after.starts_with(':') { "property" } else { "string" };
            out.push_str(&span(class, text));
            at = end;
            continue;
        }
        if spec.dollar_variables && c == '$' {
            let end = if rest.starts_with("${") {
                rest.find('}').map_or(line_end, |end| at + end + 1)
            } else {
                at + 1 + rest[1..].find(|c: char| !is_word(c)).unwrap_or(rest.len() - 1)
            };
            if end > at + 1 {
                out.push_str(&span("variable", &code[at..end]));
                at = end;
                continue;
            }
        }
        if spec.lisp_symbols && (c == ':' || c == '\'') && rest[1..].starts_with(|c: char| is_word(c) || c == '-') {
            let end = at + 1 + rest[1..].find(|c: char| !(is_word(c) || c == '-')).unwrap_or(rest.len() - 1);
            out.push_str(&span("constant", &code[at..end]));
            at = end;
            continue;
        }
        if c.is_ascii_digit() && !out.ends_with(is_word) {
            let end =
                at + rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).unwrap_or(rest.len());
            out.push_str(&span("number", &code[at..end]));
            at = end;
            continue;
        }
        if spec.keys == Keys::BeforeEquals && starts_line && c == '[' {
            out.push_str(&span("heading", &code[at..line_end]));
            at = line_end;
            continue;
        }
        let word_char = |c: char| {
            is_word(c) || (spec.lisp_symbols && c == '-') || c == ':' && spec.constants.iter().any(|k| k.contains(':'))
        };
        // TOML's bare keys go on over `-` (`org-attach-method = "mv"`).
        let word_goes_on = |c: char| word_char(c) || (spec.keys == Keys::BeforeEquals && c == '-');
        if word_char(c) || (spec.keywords.iter().any(|k| k.ends_with('!')) && c == '!') {
            let mut end = at + rest.find(|c: char| !word_goes_on(c)).unwrap_or(rest.len());
            if code[end..].starts_with('!') && spec.keywords.contains(&format!("{}!", &code[at..end]).as_str()) {
                end += 1;
            }
            let word = &code[at..end];
            let after = code[end..].trim_start_matches([' ', '\t']);
            let class = if spec.keys == Keys::BeforeEquals && after.starts_with('=') && !after.starts_with("==") {
                Some("property")
            } else if spec.keywords.contains(&word) {
                Some("keyword")
            } else if spec.constants.contains(&word) {
                Some("constant")
            } else if after.starts_with('(') && spec.keys == Keys::None && !spec.lisp_symbols {
                Some("function")
            } else {
                None
            };
            match class {
                Some(class) => out.push_str(&span(class, word)),
                None => out.push_str(&escape(word)),
            }
            at = end;
            continue;
        }
        out.push_str(&escape(&c.to_string()));
        at += c.len_utf8();
    }
    out
}

/// Where a string that opens at `at` with `quote` ends: after its closing
/// quote, a backslash escaping the next character; at the end of the line
/// when it does not close there (except Lua's and TOML's long strings,
/// which are rare in the docs).
fn string_end(code: &str, at: usize, quote: char, line_end: usize) -> usize {
    let mut escaped = false;
    for (offset, c) in code[at + quote.len_utf8()..].char_indices() {
        let position = at + quote.len_utf8() + offset;
        if position >= line_end && quote != '`' {
            return line_end;
        }
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            return position + c.len_utf8();
        }
    }
    code.len()
}

/// Org, colored by what org-lsp's parser makes of it.
fn org(code: &str) -> String {
    let root = org_syntax::parse(code, &TodoKeywords::default()).syntax();
    let mut out = String::new();
    org_node(&root, None, &mut out);
    out
}

fn org_node(node: &SyntaxNode, inherited: Option<&str>, out: &mut String) {
    let own = match node.kind() {
        TITLE => Some("heading"),
        BOLD | ITALIC | UNDERLINE | STRIKE_THROUGH => Some("emphasis"),
        CODE | VERBATIM | INLINE_SRC_BLOCK => Some("string"),
        LINK | PLAIN_LINK | ANGLE_LINK => Some("link"),
        TIMESTAMP => Some("date"),
        COMMENT => Some("comment"),
        PLANNING | CLOCK => Some("meta"),
        _ => None,
    };
    let class = own.or(inherited);
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Node(child) => org_node(&child, class, out),
            SyntaxElement::Token(token) => {
                let token_class = match token.kind() {
                    STARS => Some("heading"),
                    TODO_KEYWORD => Some("keyword"),
                    PRIORITY | COOKIE_TEXT => Some("number"),
                    TAG | COLON => Some("tag"),
                    KEYWORD_KEY | BLOCK_BEGIN | BLOCK_END | DRAWER_BEGIN | DRAWER_END | PLANNING_KEYWORD
                    | CLOCK_KEYWORD => Some("meta"),
                    PROPERTY_KEY => Some("property"),
                    BLOCK_PARAMETERS => Some("constant"),
                    COMMENT_TEXT => Some("comment"),
                    BULLET | CHECKBOX => Some("keyword"),
                    PIPE | TABLE_RULE => Some("punctuation"),
                    _ => None,
                };
                match token_class.or(class) {
                    Some(class) if !token.text().trim().is_empty() => out.push_str(&span(class, token.text())),
                    _ => out.push_str(&escape(token.text())),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::code;

    #[test]
    fn lua_comments_strings_and_keywords() {
        assert_eq!(
            code("lua", "local x = 'a' -- note"),
            "<span class=\"tok-keyword\">local</span> x = <span class=\"tok-string\">'a'</span> <span class=\"tok-comment\">-- note</span>"
        );
    }

    #[test]
    fn json_keys_and_values() {
        assert_eq!(
            code("json", "{ \"a\": true }"),
            "{ <span class=\"tok-property\">&quot;a&quot;</span>: <span class=\"tok-constant\">true</span> }"
        );
    }

    #[test]
    fn toml_keys_and_tables() {
        assert_eq!(
            code("toml", "[x]\nkey = 1"),
            "<span class=\"tok-heading\">[x]</span>\n<span class=\"tok-property\">key</span> = <span class=\"tok-number\">1</span>"
        );
        assert_eq!(
            code("toml", "org-attach-method = -1"),
            "<span class=\"tok-property\">org-attach-method</span> = -<span class=\"tok-number\">1</span>"
        );
    }

    #[test]
    fn org_by_the_parser() {
        assert_eq!(
            code("org", "* TODO a"),
            "<span class=\"tok-heading\">*</span> <span class=\"tok-keyword\">TODO</span> <span class=\"tok-heading\">a</span>"
        );
    }

    #[test]
    fn unknown_languages_are_plain() {
        assert_eq!(code("rust", "a < b"), "a &lt; b");
    }
}
