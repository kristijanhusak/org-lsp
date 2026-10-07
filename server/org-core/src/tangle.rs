//! Tangling, as Org's `org-babel-tangle` does it (`C-c C-v t`): the code
//! of the source blocks a file marks with `:tangle` written to their own
//! files.
//!
//! A block's header arguments are merged as `org-babel-get-src-block-info`
//! merges them: Babel's defaults, then the `header-args` and
//! `header-args:LANG` properties (inherited, from the file's
//! `#+PROPERTY:` lines and drawers too), then the block's line, then its
//! `#+HEADER:` lines, each over the one before. Of those, tangling reads
//! `:tangle`, `:noweb`, `:noweb-ref`, `:noweb-sep`, `:noweb-prefix`,
//! `:padline`, `:shebang`, `:tangle-mode`, `:mkdirp`, `:comments`,
//! `:prologue`, `:epilogue` and, for Emacs Lisp, `:var`.
//!
//! What needs Emacs is left out: Lisp in header arguments (but for
//! `(identity #o755)`), evaluated Noweb references (`<<name()>>`), and the
//! variables of languages Babel has not loaded by default.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use org_syntax::{Parse, SyntaxKind};

use crate::outline::Outline;

/// What tangling takes from the configuration.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// `org-src-preserve-indentation`
    pub preserve_indentation: bool,
}

/// A file tangling writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: PathBuf,
    pub text: String,
    /// The mode to set (`:tangle-mode`, or `755` with `:shebang`).
    pub mode: Option<u32>,
    /// Create the missing directories first (`:mkdirp`).
    pub make_directories: bool,
}

/// What tangling a file comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub files: Vec<File>,
    /// How many blocks were written.
    pub blocks: usize,
}

impl Outcome {
    /// Org's message: `Tangled 2 code blocks from notes.org`.
    pub fn message(&self, file_name: &str) -> String {
        format!("Tangled {} code block{} from {file_name}", self.blocks, if self.blocks == 1 { "" } else { "s" })
    }
}

/// `org-babel-tangle-lang-exts`: extensions of languages whose files do
/// not take the language's name.
const EXTENSIONS: &[(&str, &str)] = &[("emacs-lisp", "el"), ("elisp", "el"), ("bibtex", "bib")];

/// A source block of the file.
#[derive(Debug, Clone)]
struct Block {
    /// The offset of its `#+begin_src` line.
    begin: usize,
    lang: Option<String>,
    switches: String,
    /// The header arguments, merged.
    params: Params,
    name: Option<String>,
    /// The code, as `org-babel--normalize-body` gives it.
    body: String,
    /// The entry it is in.
    entry: Option<usize>,
    /// Under a commented or archived headline.
    skipped: bool,
}

/// Header arguments, by name with its colon (`:tangle`).
type Params = HashMap<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Text(String),
    Number(i64),
    /// No value at all (`:mkdirp` alone).
    None,
}

impl Value {
    fn text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }
}

fn param<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::text)
}

/// Tangle the file at `path`, whose text is `text`.
pub fn tangle(text: &str, parse: &Parse, path: &Path, settings: &Settings) -> Result<Outcome, String> {
    let outline = Outline::new(parse, text, "");
    let blocks = collect(text, parse, &outline, settings)?;
    let directory = path.parent().unwrap_or(Path::new(""));
    // `org-babel-tangle-collect-blocks`: by file, in the order they come.
    let mut by_file: Vec<(PathBuf, Vec<(usize, String)>)> = Vec::new();
    let mut counters: HashMap<Option<usize>, usize> = HashMap::new();
    let mut tangled = 0;
    for (index, block) in blocks.iter().enumerate() {
        // The block's number under its headline, for its comments.
        let counter = {
            let count = counters.entry(block.entry).or_insert(0);
            *count += 1;
            *count
        };
        if block.skipped {
            continue;
        }
        let tangle = param(&block.params, ":tangle").unwrap_or("no");
        if tangle == "no" || (block.lang.is_none() && tangle == "yes") {
            continue;
        }
        let Some(file) = tangled_file(path, directory, block.lang.as_deref(), tangle) else { continue };
        let spec = single_block(text, &outline, &blocks, index, counter, path, &file, settings)?;
        tangled += 1;
        match by_file.iter_mut().find(|(known, _)| *known == file) {
            Some((_, specs)) => specs.push((index, spec)),
            None => by_file.push((file, vec![(index, spec)])),
        }
    }
    let mut files = Vec::new();
    for (file, specs) in by_file {
        if same_file(&file, path) {
            return Err("Not allowed to tangle into the same file as self".to_string());
        }
        let mut out = String::new();
        let (mut mode, mut make_directories, mut she_banged) = (None, false, false);
        for (index, spec) in specs {
            let params = &blocks[index].params;
            let she_bang = param(params, ":shebang").filter(|value| !value.is_empty());
            let mut tangle_mode = match params.get(":tangle-mode") {
                Some(value) => Some(file_mode(value)?),
                None => None,
            };
            if she_bang.is_some() && tangle_mode.is_none() {
                tangle_mode = Some(0o755);
            }
            // The first mode given counts: Org sets them all, the first last.
            if mode.is_none() {
                mode = tangle_mode;
            }
            if params.get(":mkdirp").is_some_and(|value| value.text() != Some("no")) {
                make_directories = true;
            }
            if param(params, ":padline") != Some("no") && !out.is_empty() {
                out.push('\n');
            }
            if let Some(she_bang) = she_bang.filter(|_| !she_banged) {
                out.push_str(she_bang);
                out.push('\n');
                she_banged = true;
            }
            out.push_str(&spec);
        }
        files.push(File { path: file, text: out, mode, make_directories });
    }
    Ok(Outcome { files, blocks: tangled })
}

/// `org-babel-effective-tangled-filename`
fn tangled_file(path: &Path, directory: &Path, lang: Option<&str>, tangle: &str) -> Option<PathBuf> {
    match tangle {
        "yes" => {
            let lang = lang?;
            let extension = EXTENSIONS.iter().find(|(name, _)| *name == lang).map_or(lang, |(_, extension)| extension);
            let stem = path.with_extension("");
            Some(PathBuf::from(format!("{}.{extension}", stem.display())))
        }
        "" => None,
        _ => Some(normalize(&directory.join(expand_home(tangle)))),
    }
}

/// `~/` as the home directory.
fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// A path with `.` and `..` resolved, as `expand-file-name` gives it.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn same_file(a: &Path, b: &Path) -> bool {
    let absolute = |path: &Path| std::path::absolute(path).map(|path| normalize(&path)).unwrap_or(path.to_path_buf());
    absolute(a) == absolute(b)
}

/// `org-babel-interpret-file-mode`
fn file_mode(value: &Value) -> Result<u32, String> {
    let invalid = || format!("File mode {} not recognized as a valid format", show(value));
    match value {
        Value::Number(number) => {
            let number = u32::try_from(*number).map_err(|_| invalid())?;
            if format!("{number:o}").len() == 3 {
                Ok(number)
            } else {
                Err(format!(
                    "{number:o} is not a valid file mode octal.  Did you give the decimal value {number} by mistake?"
                ))
            }
        }
        Value::Text(text) => {
            if let Some(octal) = text.strip_prefix('o').map(|rest| rest.strip_prefix('0').unwrap_or(rest)) {
                if octal.len() == 3 && octal.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                    return u32::from_str_radix(octal, 8).map_err(|_| invalid());
                }
            }
            if text.len() == 9 {
                // `rwxr-x---`
                let bits: Vec<char> = text.chars().collect();
                let allowed = ["r-", "w-", "xs-", "r-", "w-", "xs-", "r-", "w-", "x-"];
                if bits.iter().zip(allowed).all(|(bit, allowed)| allowed.contains(*bit)) {
                    let mut mode = 0;
                    for (index, bit) in bits.iter().enumerate() {
                        if *bit != '-' {
                            mode |= 1 << (8 - index);
                        }
                    }
                    return Ok(mode);
                }
            }
            symbolic_mode(text).ok_or_else(invalid)
        }
        Value::None => Err(invalid()),
    }
}

/// `file-modes-symbolic-to-number` on `org-babel-tangle-default-file-mode`
/// (`644`): `u+x`, `a=rw,u+x`.
fn symbolic_mode(text: &str) -> Option<u32> {
    let mut mode = 0o644;
    for clause in text.split(',') {
        let at = clause.find(['+', '-', '='])?;
        let (who, mut rest) = clause.split_at(at);
        if !who.chars().all(|c| "ugoa".contains(c)) {
            return None;
        }
        let mask = if who.is_empty() || who.contains('a') {
            0o777
        } else {
            who.chars()
                .map(|c| match c {
                    'u' => 0o700,
                    'g' => 0o070,
                    _ => 0o007,
                })
                .fold(0, |all, one| all | one)
        };
        while !rest.is_empty() {
            let operator = rest.chars().next()?;
            let end = rest[1..].find(['+', '-', '=']).map_or(rest.len(), |at| at + 1);
            let mut bits = 0;
            for c in rest[1..end].chars() {
                bits |= match c {
                    'r' => 0o444,
                    'w' => 0o222,
                    'x' | 'X' => 0o111,
                    's' | 't' => 0,
                    _ => return None,
                };
            }
            let bits = bits & mask;
            match operator {
                '+' => mode |= bits,
                '-' => mode &= !bits,
                _ => mode = (mode & !mask) | bits,
            }
            rest = &rest[end..];
        }
    }
    Some(mode)
}

fn show(value: &Value) -> String {
    match value {
        Value::Text(text) => format!("{text:?}"),
        Value::Number(number) => number.to_string(),
        Value::None => "nil".to_string(),
    }
}

// ----- the blocks

/// Every source block of the file, with what tangling needs of it.
fn collect(text: &str, parse: &Parse, outline: &Outline, settings: &Settings) -> Result<Vec<Block>, String> {
    let mut blocks = Vec::new();
    for node in parse.syntax().descendants().filter(|node| node.kind() == SyntaxKind::BLOCK) {
        let range = node.text_range();
        let (node_start, end) = (usize::from(range.start()), usize::from(range.end()));
        // The node starts with the block's affiliated keywords, if any.
        let Some(begin) = node.children_with_tokens().find(|child| child.kind() == SyntaxKind::BLOCK_BEGIN) else {
            continue;
        };
        let begin = usize::from(begin.text_range().start()).max(node_start);
        let start = text[..begin].rfind('\n').map_or(0, |at| at + 1);
        let first_end = text[start..end].find('\n').map_or(end, |at| start + at);
        let Some((lang, switches, parameters)) = begin_src(&text[start..first_end]) else { continue };
        // The `#+end_src` line: the last of the block's lines that is one.
        let content_start = (first_end + 1).min(end);
        let mut end_line = None;
        let mut at = content_start;
        for line in text[content_start..end].split_inclusive('\n') {
            if line.trim_start().to_ascii_lowercase().starts_with("#+end_src") {
                end_line = Some(at);
            }
            at += line.len();
        }
        let Some(end_line) = end_line else { continue };
        let (name, headers) = affiliated(text, start);
        let entry = outline.entry_at(start);
        let skipped = entry.is_some_and(|entry| {
            let entry = &outline.entries[entry];
            entry.commented || entry.archived
        });
        let value = unescape(&text[content_start..end_line]);
        let preserve = settings.preserve_indentation || has_switch(&switches, "-i");
        let body = value.strip_suffix('\n').unwrap_or(&value);
        let body = if preserve { body.to_string() } else { remove_indentation(body) };
        let params = merged_params(outline, entry, lang.as_deref(), &parameters, &headers)?;
        blocks.push(Block { begin: start, lang, switches, params, name, body, entry, skipped });
    }
    Ok(blocks)
}

/// The language, switches and parameters of a `#+begin_src` line, as
/// `org-element-src-block-parser` reads them.
fn begin_src(line: &str) -> Option<(Option<String>, String, String)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let rest = trimmed.get(..11).filter(|start| start.eq_ignore_ascii_case("#+begin_src")).map(|_| &trimmed[11..])?;
    // `#+begin_srcx` is not one.
    if rest.starts_with(|c: char| !c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_end_matches([' ', '\t', '\r']);
    let mut words = rest;
    let lang = if words.starts_with(' ') {
        let after = words.trim_start_matches(' ');
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        let lang = &after[..end];
        words = &after[end..];
        (!lang.is_empty()).then(|| lang.to_string())
    } else {
        None
    };
    // Switches: ` -l "…"`, ` -i`, ` -k`, ` -r`, ` -n 10`, ` +n`.
    let mut switches_end = 0;
    loop {
        let rest = &words[switches_end..];
        let after = rest.trim_start_matches(' ');
        if after.len() == rest.len() {
            break;
        }
        let spaces = rest.len() - after.len();
        let length = if let Some(label) = after.strip_prefix("-l \"") {
            match label.rfind('"') {
                Some(close) if close > 0 => 4 + close + 1,
                _ => break,
            }
        } else if after.starts_with("-i") || after.starts_with("-k") || after.starts_with("-r") {
            2
        } else if after.starts_with("-n") || after.starts_with("+n") {
            let digits = after[2..].trim_start_matches(' ');
            let count = digits.bytes().take_while(u8::is_ascii_digit).count();
            if count > 0 {
                after.len() - digits.len() + count
            } else {
                2
            }
        } else {
            break;
        };
        switches_end += spaces + length;
    }
    let switches = words[..switches_end].to_string();
    let parameters = words[switches_end..].trim().to_string();
    Some((lang, switches, parameters))
}

fn has_switch(switches: &str, switch: &str) -> bool {
    switches.split_whitespace().any(|word| word == switch)
}

/// The `#+NAME:` and the `#+HEADER:` lines right above the block at
/// `start`, as `org-element` collects affiliated keywords: the last name
/// counts, and the header lines come last first.
fn affiliated(text: &str, start: usize) -> (Option<String>, Vec<String>) {
    let (mut name, mut headers) = (None, Vec::new());
    let mut line_end = start;
    while line_end > 0 {
        let line_start = text[..line_end - 1].rfind('\n').map_or(0, |at| at + 1);
        let line = text[line_start..line_end - 1].trim_start_matches([' ', '\t']);
        let Some(rest) = line.strip_prefix("#+") else { break };
        let Some((key, value)) = rest.split_once(':') else { break };
        let key = key.to_ascii_uppercase();
        let value = value.trim().to_string();
        match key.as_str() {
            "NAME" | "DATA" | "LABEL" | "RESNAME" | "SOURCE" | "SRCNAME" | "TBLNAME" => {
                // From the bottom up: the first seen is the last.
                name.get_or_insert(value);
            }
            "HEADER" | "HEADERS" => headers.push(value),
            "CAPTION" | "PLOT" | "RESULTS" | "RESULT" => {}
            key if key.starts_with("ATTR_") => {}
            _ => break,
        }
        line_end = line_start;
    }
    // Collected from the bottom: in the file's order, the last line over
    // the ones before it.
    headers.reverse();
    (name, headers)
}

/// `org-unescape-code-in-string`: the first comma of lines starting with
/// `,*`, `,#+`, `,,*` and `,,#+` goes.
fn unescape(code: &str) -> String {
    code.split_inclusive('\n')
        .map(|line| {
            let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
            let commas = line[indent..].bytes().take_while(|b| *b == b',').count();
            let after = &line[indent + commas..];
            if commas > 0 && (after.starts_with('*') || after.starts_with("#+")) {
                format!("{}{}", &line[..indent], &line[indent + 1..])
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// The column after `text`'s leading blanks, tabs to the next multiple of
/// eight.
fn indentation(line: &str) -> (usize, usize) {
    let mut column = 0;
    for (at, c) in line.char_indices() {
        match c {
            ' ' => column += 1,
            '\t' => column = (column / 8 + 1) * 8,
            _ => return (column, at),
        }
    }
    (column, line.len())
}

/// `org-remove-indentation`: the indentation all lines with text share,
/// removed; the text as it is when a line has none.
pub(crate) fn remove_indentation(code: &str) -> String {
    let mut least = usize::MAX;
    for line in code.split('\n') {
        let (column, at) = indentation(line);
        if at < line.len() {
            if column == 0 {
                return code.to_string();
            }
            least = least.min(column);
        }
    }
    let mut out = Vec::new();
    for line in code.split('\n') {
        let (column, at) = indentation(line);
        if column < least {
            // Only blank lines are less indented: they are emptied.
            out.push(String::new());
            continue;
        }
        // Up to column `least`, a tab across it turned into spaces.
        let mut current = 0;
        let mut cut = String::new();
        for (index, c) in line[..at].char_indices() {
            let next = if c == '\t' { (current / 8 + 1) * 8 } else { current + 1 };
            if next > least {
                cut = " ".repeat(next - least);
                cut.push_str(&line[index + c.len_utf8()..]);
                break;
            }
            current = next;
            if current == least {
                cut = line[index + c.len_utf8()..].to_string();
                break;
            }
        }
        out.push(cut);
    }
    out.join("\n")
}

/// `org-trim`: blanks and newlines at both ends go; with `keep_lead`, at
/// the end only.
fn trim(text: &str, keep_lead: bool) -> String {
    let blank = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let text = text.trim_end_matches(blank);
    if keep_lead { text } else { text.trim_start_matches(blank) }.to_string()
}

// ----- header arguments

/// `org-babel-default-header-args`, and the defaults of Emacs Lisp.
fn defaults(lang: Option<&str>) -> Params {
    let mut params: Params = [
        (":session", "none"),
        (":results", "replace"),
        (":exports", "code"),
        (":cache", "no"),
        (":noweb", "no"),
        (":hlines", "no"),
        (":tangle", "no"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), Value::Text(value.to_string())))
    .collect();
    if lang == Some("emacs-lisp") {
        params.insert(":lexical".to_string(), Value::Text("yes".to_string()));
    }
    params
}

/// The header arguments of a block, merged as `org-babel-merge-params`
/// merges them: the later over the earlier, `:var` gathered.
fn merged_params(
    outline: &Outline,
    entry: Option<usize>,
    lang: Option<&str>,
    parameters: &str,
    headers: &[String],
) -> Result<Params, String> {
    let mut params = defaults(lang);
    let mut layers = vec![outline.entry_property(entry, "header-args", true)];
    if let Some(lang) = lang {
        layers.push(outline.entry_property(entry, &format!("header-args:{lang}"), true));
    }
    layers.push(Some(parameters.to_string()));
    layers.extend(headers.iter().cloned().map(Some));
    for layer in layers.into_iter().flatten() {
        for (key, value) in parse_header_arguments(&layer)? {
            if key == ":var" {
                let vars = match params.remove(":var") {
                    Some(Value::Text(earlier)) => format!("{earlier}\u{0}{}", value.text().unwrap_or_default()),
                    _ => value.text().unwrap_or_default().to_string(),
                };
                params.insert(key, Value::Text(vars));
            } else {
                params.insert(key, value);
            }
        }
    }
    Ok(params)
}

/// `org-babel-parse-header-arguments`: `:key value` pairs, split where a
/// colon follows a blank outside brackets and quotes.
fn parse_header_arguments(text: &str) -> Result<Vec<(String, Value)>, String> {
    parse_header_arguments_with(text, false)
}

/// `org-babel-parse-header-arguments` with NO-EVAL: Lisp is left as it
/// is written.
pub(crate) fn parse_header_arguments_no_eval(text: &str) -> Vec<(String, Value)> {
    parse_header_arguments_with(text, true).unwrap_or_default()
}

fn parse_header_arguments_with(text: &str, no_eval: bool) -> Result<Vec<(String, Value)>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut parts: Vec<String> = Vec::new();
    let mut partial = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == ':' && at > 0 && matches!(chars[at - 1], ' ' | '\t') {
            partial.pop();
            if !partial.is_empty() {
                parts.push(std::mem::take(&mut partial));
            }
            at += 1;
            continue;
        }
        if c == '(' || c == '[' {
            // To the balanced closing bracket, if there is one.
            let mut openings = vec![c];
            let mut end = at + 1;
            while end < chars.len() && !openings.is_empty() {
                match chars[end] {
                    open @ ('(' | '[') => openings.push(open),
                    ']' if openings.last() == Some(&'[') => {
                        openings.pop();
                    }
                    ')' if openings.last() == Some(&'(') => {
                        openings.pop();
                    }
                    _ => {}
                }
                end += 1;
            }
            if openings.is_empty() {
                partial.extend(&chars[at..end]);
                at = end;
            } else {
                partial.push(c);
                at += 1;
            }
            continue;
        }
        if c == '"' && (at == 0 || chars[at - 1] != '\\') {
            // To the closing quote.
            let mut end = at + 1;
            while end < chars.len() && !(chars[end] == '"' && chars[end - 1] != '\\') {
                end += 1;
            }
            let end = (end + 1).min(chars.len());
            partial.extend(&chars[at..end]);
            at = end;
            continue;
        }
        partial.push(c);
        at += 1;
    }
    if !partial.is_empty() {
        parts.push(partial);
    }
    let mut out = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        let part = if index == 0 { part } else { format!(":{part}") };
        let blank = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c');
        let part = part.trim_start_matches(blank);
        match part.find(blank) {
            Some(space) if !part[space..].trim_start_matches(blank).is_empty() => {
                let key = part[..space].to_string();
                let value = part[space..].trim_start_matches(blank).trim_end_matches(blank);
                let value = if no_eval { read_no_eval(value) } else { read(&key, value)? };
                out.push((key.clone(), value));
            }
            _ => {
                let key = part.trim_end_matches(blank);
                if !key.is_empty() {
                    out.push((key.to_string(), Value::None));
                }
            }
        }
    }
    Ok(out)
}

/// `org-babel-read` with NO-EVAL: a number, a string in quotes, or the
/// text as it is.
fn read_no_eval(cell: &str) -> Value {
    if let Ok(number) = cell.parse::<i64>() {
        return Value::Number(number);
    }
    if cell.starts_with('(') || cell.starts_with('\'') || cell.starts_with('`') {
        return Value::Text(cell.to_string());
    }
    match read(":", cell) {
        Ok(value) => value,
        Err(_) => Value::Text(cell.to_string()),
    }
}

/// `org-babel-read`: a number, a string in quotes, or the text. Lisp is
/// read only as `(identity #o755)`.
fn read(key: &str, cell: &str) -> Result<Value, String> {
    if let Ok(number) = cell.parse::<i64>() {
        return Ok(Value::Number(number));
    }
    if cell.starts_with('(') || cell.starts_with('\'') || cell.starts_with('`') {
        let octal = cell
            .strip_prefix("(identity #o")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|digits| i64::from_str_radix(digits.trim(), 8).ok());
        return match octal {
            Some(number) => Ok(Value::Number(number)),
            None if matches!(key, ":tangle" | ":tangle-mode" | ":shebang" | ":noweb-ref" | ":noweb") => {
                Err(format!("Cannot evaluate the Emacs Lisp of {key}: {cell}"))
            }
            None => Ok(Value::Text(cell.to_string())),
        };
    }
    let trimmed = cell.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // A single string: no unescaped quote inside.
        let bytes = inner.as_bytes();
        if !(1..bytes.len()).any(|at| bytes[at] == b'"' && bytes[at - 1] != b'\\') && !inner.starts_with('"') {
            return Ok(Value::Text(lisp_string(inner)));
        }
    }
    Ok(Value::Text(cell.to_string()))
}

/// The text of an Emacs Lisp string's insides: `\n`, `\t`, `\"`, `\\`.
fn lisp_string(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\x1b'),
            Some('\n') | None => {}
            Some(other) => out.push(other),
        }
    }
    out
}

// ----- one block

/// `org-babel-tangle-single-block` and `org-babel-spec-to-string`: what
/// a block adds to its file.
#[allow(clippy::too_many_arguments)]
fn single_block(
    text: &str,
    outline: &Outline,
    blocks: &[Block],
    index: usize,
    counter: usize,
    path: &Path,
    file: &Path,
    settings: &Settings,
) -> Result<String, String> {
    let block = &blocks[index];
    let params = &block.params;
    let noweb = param(params, ":noweb").unwrap_or("no");
    let mut body = if noweb_p(params, Context::Tangle) {
        if noweb.split_whitespace().any(|value| value == "strip-tangle") {
            strip_references(&block.body)
        } else {
            let mut expander = Expander { text, outline, blocks, path, file, by_ref: None };
            expander.expand(index, Context::Tangle)?
        }
    } else {
        block.body.clone()
    };
    if !params.contains_key(":no-expand") {
        body = expand_body(block.lang.as_deref(), &body, params);
    }
    if has_switch(&block.switches, "-r") {
        body = remove_coderefs(&body, &block.switches);
    }
    let preserve = settings.preserve_indentation || has_switch(&block.switches, "-i");
    let body = if preserve { trim(&body, true) } else { trim(&remove_indentation(&body), false) };

    let comments = param(params, ":comments").unwrap_or("no");
    let syntax = comment_syntax(block.lang.as_deref());
    let mut out = String::new();
    // The text commented, then `end-of-line` and a newline: after a text
    // that ends with one, an empty line.
    let comment = |out: &mut String, text: &str| {
        if comments != "no" && !text.trim().is_empty() {
            out.push_str(&comment_region(text, syntax));
            out.push('\n');
        }
    };
    if matches!(comments, "both" | "org") {
        comment(&mut out, &remove_indentation(&org_comment(text, outline, block)));
    }
    let link = matches!(comments, "both" | "link" | "yes" | "noweb");
    let (begin, end) = link_comments(text, outline, blocks, index, counter, path, file);
    if link {
        comment(&mut out, &begin);
    }
    out.push_str(&body);
    out.push('\n');
    if link {
        comment(&mut out, &end);
    }
    Ok(out)
}

/// `org-babel-expand-body:emacs-lisp` and `org-babel-expand-body:generic`
/// without variables: Babel loads only Emacs Lisp by default, and other
/// languages' variables need theirs.
fn expand_body(lang: Option<&str>, body: &str, params: &Params) -> String {
    if lang == Some("emacs-lisp") {
        let vars: Vec<String> =
            param(params, ":var").map(|vars| vars.split('\u{0}').filter_map(elisp_var).collect()).unwrap_or_default();
        if vars.is_empty() {
            return body.to_string();
        }
        return format!("(let ({})\n{body}\n)", vars.join("\n      "));
    }
    let mut parts = Vec::new();
    if let Some(prologue) = param(params, ":prologue") {
        parts.push(prologue.to_string());
    }
    parts.push(body.to_string());
    if let Some(epilogue) = param(params, ":epilogue") {
        parts.push(epilogue.to_string());
    }
    parts.join("\n")
}

/// `NAME=VALUE` of `:var` as Emacs Lisp's `let` binds it: `(x '1)`.
fn elisp_var(assignment: &str) -> Option<String> {
    let (name, value) = assignment.split_once('=')?;
    let (name, value) = (name.trim(), value.trim());
    // A number or a string, as `%S` prints them; anything else refers to
    // a block or a table, which needs Emacs.
    let quoted = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
    if value.parse::<f64>().is_err() && !quoted {
        return None;
    }
    Some(format!("({name} '{value})"))
}

/// `org-src-coderef-regexp`: `(ref:NAME)` at the end of lines, or the
/// format of `-l "…"`, gone.
fn remove_coderefs(body: &str, switches: &str) -> String {
    let format = switches
        .find("-l \"")
        .and_then(|at| {
            let rest = &switches[at + 4..];
            rest.find('"').map(|end| &rest[..end])
        })
        .unwrap_or("(ref:%s)");
    let Some((before, after)) = format.split_once("%s") else { return body.to_string() };
    body.split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if let Some(start) = trimmed.strip_suffix(after).and_then(|rest| rest.rfind(before)) {
                let label = &trimmed[start + before.len()..trimmed.len() - after.len()];
                let valid = label.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    && label.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '));
                if valid {
                    return trimmed[..start].trim_end_matches([' ', '\t']).to_string();
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ----- noweb

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Tangle,
}

/// `org-babel-noweb-p`
fn noweb_p(params: &Params, context: Context) -> bool {
    let allowed: &[&str] = match context {
        Context::Tangle => &["yes", "tangle", "no-export", "strip-export", "strip-tangle"],
    };
    param(params, ":noweb").unwrap_or("").split_whitespace().any(|value| allowed.contains(&value))
}

/// A Noweb reference: `<<name>>`, with no blank at either end inside.
fn references(body: &str) -> Vec<(usize, usize, &str)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = body[from..].find("<<").map(|at| from + at) {
        let inside = start + 2;
        let line_end = body[inside..].find('\n').map_or(body.len(), |at| inside + at);
        // The shortest name up to `>>` on the line.
        let mut found_one = None;
        let mut search = inside;
        while let Some(close) = body[search..line_end].find(">>").map(|at| search + at) {
            let name = &body[inside..close];
            if !name.is_empty() && !name.starts_with([' ', '\t']) && !name.ends_with([' ', '\t']) {
                found_one = Some((start, close + 2, name));
                break;
            }
            search = close + 1;
        }
        match found_one {
            Some((start, end, name)) => {
                found.push((start, end, name));
                from = end;
            }
            None => from = start + 1,
        }
    }
    found
}

/// `:noweb strip-tangle`: the references go.
fn strip_references(body: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (start, end, _) in references(body) {
        out.push_str(&body[last..start]);
        last = end;
    }
    out.push_str(&body[last..]);
    out
}

struct Expander<'a> {
    text: &'a str,
    outline: &'a Outline,
    blocks: &'a [Block],
    path: &'a Path,
    file: &'a Path,
    /// The blocks by `:noweb-ref`, once looked for.
    by_ref: Option<HashMap<String, Vec<usize>>>,
}

impl Expander<'_> {
    /// `org-babel-expand-noweb-references` on the block `index`.
    fn expand(&mut self, index: usize, context: Context) -> Result<String, String> {
        let block = &self.blocks[index];
        let params = &block.params;
        let comment = param(params, ":comments") == Some("noweb");
        let prefix_lines = match params.get(":noweb-prefix") {
            None => true,
            Some(value) => !matches!(value.text(), Some("no") | Some("nil")) && *value != Value::None,
        };
        let body = block.body.clone();
        let mut out = String::new();
        let mut last = 0;
        for (start, end, id) in references(&body) {
            let line_start = body[..start].rfind('\n').map_or(0, |at| at + 1).max(last);
            let prefix = &body[line_start..start];
            out.push_str(&body[last..start]);
            let expansion = if id.contains('(') && id.ends_with(')') {
                return Err(format!("Cannot evaluate the code block of {id} without Emacs"));
            } else {
                self.reference(id, context, comment)?
            };
            if prefix_lines {
                out.push_str(&expansion.split(['\n', '\r']).collect::<Vec<_>>().join(&format!("\n{prefix}")));
            } else {
                out.push_str(&expansion);
            }
            last = end;
        }
        out.push_str(&body[last..]);
        Ok(out)
    }

    /// The text a reference stands for.
    fn reference(&mut self, id: &str, context: Context, comment: bool) -> Result<String, String> {
        // A headline by CUSTOM_ID or ID: its text.
        if let Some(body) = self.headline_body(id) {
            return Ok(body);
        }
        // The first block named so, unless it is under a commented headline.
        let named = self.blocks.iter().position(|block| block.name.as_deref().is_some_and(|name| name == id));
        if let Some(named) = named {
            let commented = self.blocks[named].entry.is_some_and(|entry| self.outline.entries[entry].commented);
            if !commented {
                return self.expand_body(named, context, comment);
            }
        }
        // The blocks with `:noweb-ref` ID, outside commented headlines.
        let blocks = self.blocks;
        let outline = self.outline;
        let by_ref = self.by_ref.get_or_insert_with(|| {
            let mut by_ref: HashMap<String, Vec<usize>> = HashMap::new();
            for (index, block) in blocks.iter().enumerate() {
                if block.entry.is_some_and(|entry| outline.entries[entry].commented) {
                    continue;
                }
                if let Some(name) = param(&block.params, ":noweb-ref") {
                    by_ref.entry(name.to_string()).or_default().push(index);
                }
            }
            by_ref
        });
        let Some(found) = by_ref.get(id).cloned() else { return Ok(String::new()) };
        let mut out = String::new();
        for (position, block) in found.iter().enumerate() {
            out.push_str(&self.expand_body(*block, context, comment)?);
            if position + 1 < found.len() {
                out.push_str(param(&self.blocks[*block].params, ":noweb-sep").unwrap_or("\n"));
            }
        }
        Ok(out)
    }

    /// A referenced block's text: expanded in its turn if its `:noweb`
    /// says so, between link comments with `:comments noweb`.
    fn expand_body(&mut self, index: usize, context: Context, comment: bool) -> Result<String, String> {
        let body = if noweb_p(&self.blocks[index].params, context) {
            self.expand(index, context)?
        } else {
            self.blocks[index].body.clone()
        };
        if !comment {
            return Ok(body);
        }
        let syntax = comment_syntax(self.blocks[index].lang.as_deref());
        let counter = self.blocks[..=index].iter().filter(|block| block.entry == self.blocks[index].entry).count();
        let (begin, end) = link_comments(self.text, self.outline, self.blocks, index, counter, self.path, self.file);
        Ok(format!(
            "{}\n{body}\n{}\n",
            trim(&comment_region(&begin, syntax), false),
            trim(&comment_region(&end, syntax), false)
        ))
    }

    /// `org-babel-ref-goto-headline-id` and `org-babel-ref-headline-body`:
    /// the text of the entry with `id` as CUSTOM_ID or ID, after its
    /// planning line and property drawer, to the end of its subtree.
    fn headline_body(&self, id: &str) -> Option<String> {
        let found = self.outline.entries.iter().position(|entry| {
            entry.properties.iter().any(|(key, value)| (key == "CUSTOM_ID" || key == "ID") && value == id)
        })?;
        let entry = &self.outline.entries[found];
        let end = self
            .outline
            .entries
            .iter()
            .skip(found + 1)
            .find(|other| other.level <= entry.level)
            .map_or(self.text.len(), |other| other.offset);
        let mut start = (entry.line_end + 1).min(end);
        let skip = |line_start: usize| {
            let line_end = self.text[line_start..end].find('\n').map_or(end, |at| line_start + at + 1);
            (line_start, line_end)
        };
        // A planning line, then a property drawer.
        let (_, next) = skip(start);
        let line = self.text[start..next].trim_start();
        if ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|word| line.starts_with(word)) {
            start = next;
        }
        if self.text[start..end].trim_start().to_ascii_uppercase().starts_with(":PROPERTIES:") {
            while start < end {
                let (_, next) = skip(start);
                let done = self.text[start..next].trim().eq_ignore_ascii_case(":END:");
                start = next;
                if done {
                    break;
                }
            }
        }
        let body = &self.text[start..end];
        Some(body.strip_suffix('\n').unwrap_or(body).to_string())
    }
}

// ----- comments

/// How a language's mode comments a line: what comes before and after.
#[derive(Debug, Clone, Copy)]
struct CommentSyntax {
    start: &'static str,
    end: &'static str,
}

/// The comment syntax of the modes Emacs has for a language.
fn comment_syntax(lang: Option<&str>) -> CommentSyntax {
    let syntax = |start, end| CommentSyntax { start, end };
    match lang.unwrap_or("").to_ascii_lowercase().as_str() {
        "emacs-lisp" | "elisp" | "lisp" | "scheme" | "clojure" | "racket" | "fennel" => syntax(";; ", ""),
        "c" | "css" => syntax("/* ", " */"),
        "html" | "xml" | "nxml" | "mhtml" => syntax("<!-- ", " -->"),
        "latex" | "tex" => syntax("%% ", ""),
        "octave" => syntax("## ", ""),
        "matlab" | "erlang" | "prolog" => syntax("% ", ""),
        "sql" | "sqlite" | "lua" | "haskell" | "ada" => syntax("-- ", ""),
        "js" | "javascript" | "java" | "cpp" | "c++" | "rust" | "go" | "typescript" | "ts" | "kotlin" | "scala"
        | "swift" | "php" | "dart" | "groovy" | "csharp" | "d" | "objc" | "dot" | "zig" => syntax("// ", ""),
        "ocaml" | "fsharp" => syntax("(* ", " *)"),
        "fortran" => syntax("c$$$", ""),
        _ => syntax("# ", ""),
    }
}

/// `comment-region` in the language's mode: each line with text
/// commented, the comment starting at the indentation they share.
fn comment_region(text: &str, syntax: CommentSyntax) -> String {
    let indent = text
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    text.split_inclusive('\n')
        .map(|line| {
            let (content, newline) = match line.strip_suffix('\n') {
                Some(content) => (content, "\n"),
                None => (line, ""),
            };
            if content.trim().is_empty() {
                line.to_string()
            } else {
                format!("{}{}{}{}{newline}", &content[..indent], syntax.start, &content[indent..], syntax.end)
            }
        })
        .collect()
}

/// The link comments around a block (`org-babel-tangle-comment-format-beg`
/// and `-end`): `[[file:notes.org::*Heading][Heading:1]]` and
/// `Heading:1 ends here`.
fn link_comments(
    text: &str,
    outline: &Outline,
    blocks: &[Block],
    index: usize,
    counter: usize,
    path: &Path,
    file: &Path,
) -> (String, String) {
    let block = &blocks[index];
    let heading = block.entry.map(|entry| &outline.entries[entry]);
    let source_name = match (&block.name, heading) {
        (Some(name), _) => name.clone(),
        (None, Some(entry)) => format!("{}:{counter}", heading_title(entry, outline)),
        (None, None) => format!("No heading:{counter}"),
    };
    // `org-link-precise-link-target`
    let search = match (&block.name, heading) {
        (Some(name), _) => name.clone(),
        (None, Some(entry)) => match entry.properties.iter().find(|(key, _)| key == "CUSTOM_ID") {
            Some((_, id)) => format!("#{id}"),
            None => format!("*{}", crate::link::normalize_string(&clean_heading(entry, outline), false)),
        },
        (None, None) => {
            let line_end = text[block.begin..].find('\n').map_or(text.len(), |at| block.begin + at);
            crate::link::normalize_string(&text[block.begin..line_end], true)
        }
    };
    let relative = relative_path(path, file.parent().unwrap_or(Path::new("")));
    let link = format!("file:{relative}::{search}");
    (format!("[[{link}][{source_name}]]"), format!("{source_name} ends here"))
}

/// The path of `path` from `directory`, as `file-relative-name` gives it.
fn relative_path(path: &Path, directory: &Path) -> String {
    let absolute = |path: &Path| std::path::absolute(path).map(|path| normalize(&path)).unwrap_or(path.to_path_buf());
    let (path, directory) = (absolute(path), absolute(directory));
    let (mut a, mut b) = (path.components().peekable(), directory.components().peekable());
    while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
        if x != y {
            break;
        }
        a.next();
        b.next();
    }
    let mut out: Vec<String> = b.map(|_| "..".to_string()).collect();
    out.extend(a.map(|component| component.as_os_str().to_string_lossy().into_owned()));
    out.join("/")
}

/// `org-heading-components`' title: the headline without its stars,
/// keyword, priority and tags.
fn heading_title(entry: &crate::outline::Entry, outline: &Outline) -> String {
    let line = format!("{} {}", "*".repeat(entry.level), entry.head);
    match crate::outline::complex_heading_title(&line, entry.level, &outline.todo) {
        Some((start, end)) => line[start..end].to_string(),
        None => String::new(),
    }
}

/// `org-get-heading t t t t`: the title without a COMMENT keyword either.
fn clean_heading(entry: &crate::outline::Entry, outline: &Outline) -> String {
    let title = heading_title(entry, outline);
    match title.strip_prefix("COMMENT") {
        Some(rest) if rest.is_empty() || rest.starts_with(' ') => rest.trim_start().to_string(),
        _ => title,
    }
}

/// The Org text before a block (`:comments org`, `both`): from its
/// headline's title, or the end of the block before it, to the block.
fn org_comment(text: &str, outline: &Outline, block: &Block) -> String {
    let heading_start = block.entry.map_or(0, |entry| {
        let entry = &outline.entries[entry];
        entry.offset + entry.level + 1
    });
    let previous_end = previous_block_end(text, block.begin).unwrap_or(0);
    let start = heading_start.max(previous_end).min(block.begin);
    text[start..block.begin].to_string()
}

/// Where the last whole source block before `offset` ends, after its
/// `#+end_src`, as a backward search for `org-babel-src-block-regexp`
/// finds it.
fn previous_block_end(text: &str, offset: usize) -> Option<usize> {
    let before = &text[..offset];
    let mut at = before.len();
    while let Some(start) = before[..at].rfind("#+end_src").or_else(|| before[..at].rfind("#+END_SRC")) {
        let line_start = before[..start].rfind('\n').map_or(0, |at| at + 1);
        if before[line_start..start].trim().is_empty() {
            return Some(start + "#+end_src".len());
        }
        at = start;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_shared_indentation_as_org_does() {
        assert_eq!(remove_indentation("  a\n    b\n\n  c"), "a\n  b\n\nc");
        assert_eq!(remove_indentation("a\n  b"), "a\n  b");
        assert_eq!(remove_indentation("\ta\n  \tb"), "a\nb");
        assert_eq!(remove_indentation("    a\n  \n    b"), "a\n\nb");
    }

    #[test]
    fn reads_header_arguments() {
        let parsed =
            parse_header_arguments(":tangle   \"a b.txt\"   :padline   no :mkdirp :noweb-sep \"\\n\\n\"").unwrap();
        assert_eq!(
            parsed,
            [
                (":tangle".to_string(), Value::Text("a b.txt".to_string())),
                (":padline".to_string(), Value::Text("no".to_string())),
                (":mkdirp".to_string(), Value::None),
                (":noweb-sep".to_string(), Value::Text("\n\n".to_string())),
            ]
        );
        assert_eq!(parse_header_arguments(":tangle-mode (identity #o755)").unwrap()[0].1, Value::Number(0o755));
    }

    #[test]
    fn reads_begin_src_lines() {
        assert_eq!(
            begin_src("#+begin_src python -r -l \"[[%s]]\" :tangle yes"),
            Some((Some("python".to_string()), " -r -l \"[[%s]]\"".to_string(), ":tangle yes".to_string()))
        );
        assert_eq!(
            begin_src("  #+BEGIN_SRC :tangle yes"),
            Some((Some(":tangle".to_string()), String::new(), "yes".to_string()))
        );
        assert_eq!(begin_src("#+begin_srcx"), None);
    }

    #[test]
    fn file_modes() {
        assert_eq!(file_mode(&Value::Text("o600".to_string())), Ok(0o600));
        assert_eq!(file_mode(&Value::Text("rwxr-x---".to_string())), Ok(0o750));
        assert_eq!(file_mode(&Value::Text("u+x".to_string())), Ok(0o744));
        assert_eq!(file_mode(&Value::Text("a=r,u+w".to_string())), Ok(0o644));
        assert_eq!(file_mode(&Value::Number(0o755)), Ok(0o755));
    }
}
