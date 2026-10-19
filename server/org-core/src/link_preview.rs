//! `org-link-preview-region` (`C-c C-x C-v`): the links whose file can be
//! shown in place of the link, an image, and what each one shows.
//!
//! As Org 9.8 has it, a `file:` or `attachment:` link (or a bracket link to
//! a path) is previewable when its path names an image file
//! (`image-file-name-regexp`) and it has no description, or its
//! description is itself such a link with an explicit type, and nothing
//! else. With `include_linked`, links with a description are previewed
//! too.
//!
//! With `remote`, an `http:` or `https:` link naming an image is
//! previewable as well, its image downloaded first: Org's
//! `org-display-remote-inline-images` is for remote files (TRAMP), which
//! org-lsp has not, and its URLs are what stands for them.

use org_syntax::{Parse, SyntaxKind, SyntaxNode};

use crate::open::{self, Open};

/// A link to preview: where it is, and the file it names, as written
/// (relative to the text's own file), for an attachment its name in the
/// entry's attachment directory, or for a remote image its URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub start: usize,
    pub end: usize,
    pub path: String,
    pub attachment: bool,
    pub remote: bool,
}

/// The extensions of `image-file-name-extensions`.
const IMAGE_EXTENSIONS: &[&str] =
    &["png", "jpeg", "jpg", "gif", "tiff", "tif", "xbm", "xpm", "pbm", "pgm", "ppm", "pnm", "svg", "webp"];

/// Whether `path` names an image file, by its extension.
pub fn is_image(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty() && IMAGE_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(extension))
    })
}

/// The previewable links between bytes `from` and `to` of `text`.
/// `global` is `org-link-abbrev-alist`; `remote`, whether the images of
/// URLs show (`org-display-remote-inline-images` not `skip`).
pub fn previews(
    text: &str,
    parse: &Parse,
    global: &[(String, String)],
    include_linked: bool,
    remote: bool,
    from: usize,
    to: usize,
) -> Vec<Preview> {
    let mut out = Vec::new();
    for node in parse.syntax().descendants() {
        if !matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK) {
            continue;
        }
        let range = node.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        if end <= from || start >= to {
            continue;
        }
        let described = has_description(&node);
        let (path, attachment, url) = if !described || include_linked {
            let Some(link) = open::link_at(text, parse, start, global, &open::Settings::default()) else { continue };
            match link.open {
                Ok(Open::File { path, search: None, .. }) => (path, false, false),
                Ok(Open::Attachment(path)) if !path.contains("::") => (path, true, false),
                Ok(Open::External(url)) if remote && is_url(&url) => (url, false, true),
                _ => continue,
            }
        } else {
            // A description that is a sole link with its type.
            let Some((path, attachment)) = sole_link(&node, text) else { continue };
            (path, attachment, false)
        };
        if is_image(&path) {
            out.push(Preview { start, end, path, attachment, remote: url });
        }
    }
    out
}

/// Whether `url` is one of the web, `http:` or `https:`.
fn is_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Whether a bracket link has a description.
fn has_description(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::LINK
        && node.children_with_tokens().any(|child| child.kind() == SyntaxKind::LINK_SEPARATOR)
}

/// The file of the link that is all of a bracket link's description, if
/// one is: `file:` or `attachment:` written out, plain or in angles.
fn sole_link(node: &SyntaxNode, text: &str) -> Option<(String, bool)> {
    let separator = node.children_with_tokens().find(|child| child.kind() == SyntaxKind::LINK_SEPARATOR)?;
    let close = node.children_with_tokens().find(|child| child.kind() == SyntaxKind::LINK_CLOSE)?;
    let description = &text[usize::from(separator.text_range().end())..usize::from(close.text_range().start())];
    let description = description.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')).unwrap_or(description);
    if description.chars().any(char::is_whitespace) {
        return None;
    }
    let (kind, path) = description.split_once(':')?;
    if path.contains("::") || path.is_empty() {
        return None;
    }
    match kind {
        "file" => Some((path.to_string(), false)),
        "attachment" => Some((path.to_string(), true)),
        _ => None,
    }
}

/// A number as Lisp has it: an integer or a float, which Org tells apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

impl Number {
    fn value(self) -> f64 {
        match self {
            Number::Integer(value) => value as f64,
            Number::Float(value) => value,
        }
    }
}

/// `org-image-actual-width`
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActualWidth {
    /// `t`: the image's own width.
    Actual,
    /// A number: that width.
    Fixed(Number),
    /// `nil`, or a list of a number: the `:width` of `#+ATTR_ORG:` or else
    /// of another `#+ATTR_…:`, else the list's number.
    Attributes(Option<Number>),
}

impl ActualWidth {
    /// The value of the property `ORG-IMAGE-ACTUAL-WIDTH`, as `read` reads
    /// it; anything else Org takes for the image's own width.
    pub fn read(value: &str) -> ActualWidth {
        let value = value.trim();
        if value == "nil" || value == "()" {
            return ActualWidth::Attributes(None);
        }
        if let Some(inner) = value.strip_prefix('(').and_then(|rest| rest.strip_suffix(')')) {
            let first = inner.split_whitespace().next();
            return ActualWidth::Attributes(first.and_then(read_number));
        }
        read_number(value).map_or(ActualWidth::Actual, ActualWidth::Fixed)
    }
}

/// A number written as Lisp reads one, or none.
fn read_number(text: &str) -> Option<Number> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let number = string_to_number(text);
    (digits.chars().any(|c| c.is_ascii_digit())).then_some(number)
}

/// `string-to-number`: the number the text starts with, 0 when none; a
/// float when it has a fraction.
fn string_to_number(text: &str) -> Number {
    let text = text.trim_start_matches([' ', '\t']);
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    let whole = rest.bytes().take_while(u8::is_ascii_digit).count();
    let after = &rest[whole..];
    let fraction = after
        .strip_prefix('.')
        .map(|digits| digits.bytes().take_while(u8::is_ascii_digit).count())
        .filter(|count| *count > 0);
    match fraction {
        Some(count) => {
            let number: f64 = rest[..whole + 1 + count].parse().unwrap_or(0.0);
            Number::Float(sign * number)
        }
        None => Number::Integer(rest[..whole].parse::<i64>().map_or(0, |number| number * sign as i64)),
    }
}

/// How wide an image shows: in pixels, or as a part of the width of the
/// text in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageWidth {
    Pixels(f64),
    Fraction(f64),
}

/// `org-display-inline-image--width`: how wide the image of the link at
/// `start` shows, by `setting` (`org-image-actual-width`, or the entry's
/// `ORG-IMAGE-ACTUAL-WIDTH` property), or none for its own width.
pub fn image_width(parse: &Parse, start: usize, setting: ActualWidth) -> Option<ImageWidth> {
    match setting {
        ActualWidth::Actual => None,
        ActualWidth::Fixed(number) => Some(ImageWidth::Pixels(number.value())),
        ActualWidth::Attributes(default) => {
            let attributes = paragraph_attributes(parse, start);
            let width_of = |name: &str| {
                attributes.iter().find(|(key, _)| key == name).and_then(|(_, value)| read_attribute(value, ":width"))
            };
            let org = width_of("ATTR_ORG");
            let width = if !unreadable(org.as_deref()) {
                org
            } else {
                attributes
                    .iter()
                    .filter(|(key, _)| key != "ATTR_ORG")
                    .map(|(_, value)| read_attribute(value, ":width"))
                    .find(|width| !unreadable(width.as_deref()))
                    .flatten()
            };
            let number = match width.as_deref() {
                Some("t") => None,
                width if unreadable(width) => default,
                Some(width) if PERCENT.is_match(width) => {
                    Some(Number::Float(string_to_number(width.trim_start_matches('[')).value() / 100.0))
                }
                Some(width) => Some(string_to_number(width)),
                None => default,
            };
            number.map(|number| match number {
                Number::Float(part) if (0.0..=2.0).contains(&part) => ImageWidth::Fraction(part),
                number => ImageWidth::Pixels(number.value()),
            })
        }
    }
}

/// [`image_width`] for the link at `start` of the text of `outline`, by
/// the entry's own `ORG-IMAGE-ACTUAL-WIDTH` when it has one
/// (`org-property-or-variable-value`), else by `setting`.
pub fn image_width_in(
    outline: &crate::outline::Outline,
    parse: &Parse,
    start: usize,
    setting: ActualWidth,
) -> Option<ImageWidth> {
    let property = outline.entry_property(outline.entry_at(start), "ORG-IMAGE-ACTUAL-WIDTH", false);
    let setting = property.filter(|value| !value.trim().is_empty()).map_or(setting, |value| ActualWidth::read(&value));
    image_width(parse, start, setting)
}

/// `\`[[+]?[0-9.]+%`, as Org writes it: `[` or `+`, then a percentage.
static PERCENT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[\[+]?[0-9.]+%").expect("valid regexp"));

/// A width Org cannot read (`width-unreadable?`): a lone number of pixels,
/// a percentage, a fraction, or `t`; a float that is not a percentage is
/// read only between 0 and 2.
fn unreadable(value: Option<&str>) -> bool {
    static READABLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\+?(?:[0-9]+(?:px)?$|[0-9.]+(%)|[0-9]*\.[0-9]+)").expect("valid regexp")
    });
    let Some(value) = value else { return true };
    if value == "t" {
        return false;
    }
    let Some(captures) = READABLE.captures(value) else { return true };
    match string_to_number(value) {
        Number::Float(number) => captures.get(1).is_none() && !(0.0..=2.0).contains(&number),
        Number::Integer(_) => false,
    }
}

/// The attributes of the paragraph the link at `start` is in, its
/// affiliated `#+ATTR_…:` keywords: each name upper-cased with its values
/// joined by a space, in the order they first come.
fn paragraph_attributes(parse: &Parse, start: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let token = parse.syntax().token_at_offset(org_syntax::TextSize::from(start as u32)).right_biased();
    let Some(paragraph) =
        token.and_then(|token| token.parent_ancestors().find(|node| node.kind() == SyntaxKind::PARAGRAPH))
    else {
        return out;
    };
    for keyword in paragraph.children().take_while(org_syntax::ast::is_affiliated_keyword) {
        let token = |kind| {
            keyword
                .children_with_tokens()
                .filter_map(|child| child.into_token())
                .find(|token| token.kind() == kind)
                .map(|token| token.text().to_string())
        };
        let key = token(SyntaxKind::KEYWORD_KEY).unwrap_or_default().to_ascii_uppercase();
        let key = key.trim_start_matches("#+").trim_end_matches(':').to_string();
        if !key.starts_with("ATTR_") {
            continue;
        }
        let value = token(SyntaxKind::KEYWORD_VALUE).unwrap_or_default().trim().to_string();
        match out.iter_mut().find(|(name, _)| *name == key) {
            Some((_, values)) => {
                values.push(' ');
                values.push_str(&value);
            }
            None => out.push((key, value)),
        }
    }
    out
}

/// `org-export-read-attribute`: the value of `property` (`:width`) in
/// `attributes`, a list of `:key value` pairs; none for an empty value or
/// `nil`, and `""` for a value of double quotes alone.
fn read_attribute(attributes: &str, property: &str) -> Option<String> {
    static KEY: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?:^|[ \t]+)(:[-a-zA-Z0-9_]+)(?:[ \t]+|$)").expect("valid regexp")
    });
    let prepare = |value: &str| -> Option<String> {
        if value.is_empty() || value == "nil" {
            return None;
        }
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') && value.bytes().all(|b| b == b'"') {
            return Some(value[1..value.len() - 1].to_string());
        }
        Some(value.to_string())
    };
    let mut rest = attributes;
    let mut key: Option<&str> = None;
    loop {
        let found = KEY.captures(rest);
        let value_end = found.as_ref().map_or(rest.len(), |captures| captures.get(0).map_or(0, |m| m.start()));
        if key == Some(property) {
            return prepare(&rest[..value_end]);
        }
        let captures = found?;
        let (name, end) = (captures.get(1)?, captures.get(0)?.end());
        key = Some(&rest[name.start()..name.end()]);
        // An empty match at the end would go on for ever.
        if end == 0 && rest.is_empty() {
            return None;
        }
        rest = &rest[end..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use org_syntax::TodoKeywords;

    fn found(text: &str, include_linked: bool) -> Vec<(String, bool)> {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        previews(text, &parse, &[], include_linked, false, 0, text.len())
            .into_iter()
            .map(|preview| (text[preview.start..preview.end].to_string(), preview.attachment))
            .collect()
    }

    #[test]
    fn links_to_images_without_description() {
        let text = "[[file:a.png]] [[./b.JPG]] [[file:c.txt]] [[https://x.org/d.png]]\n\
                    [[file:e.png][E]] [[attachment:f.svg]] file:g.gif [[file:h.png::x]]\n";
        assert_eq!(
            found(text, false),
            [
                ("[[file:a.png]]".to_string(), false),
                ("[[./b.JPG]]".to_string(), false),
                ("[[attachment:f.svg]]".to_string(), true),
                ("file:g.gif".to_string(), false),
            ]
        );
        let linked = found(text, true);
        assert!(linked.contains(&("[[file:e.png][E]]".to_string(), false)), "{linked:?}");
    }

    #[test]
    fn a_description_that_is_a_sole_image_link() {
        let text = "[[https://example.com][file:thumb.png]] [[https://example.com][thumb.png]]\n";
        assert_eq!(found(text, false), [("[[https://example.com][file:thumb.png]]".to_string(), false)]);
    }

    #[test]
    fn remote_images_when_they_show() {
        let text = "[[https://x.org/d.png]] http://x.org/e.GIF <https://x.org/f.svg> [[https://x.org/g.png?s=1]]\n\
                    [[https://x.org/h.png][H]] [[ftp://x.org/i.png]] [[https://x.org/page]]\n";
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let found = |remote| -> Vec<(String, bool)> {
            previews(text, &parse, &[], false, remote, 0, text.len())
                .into_iter()
                .map(|preview| (preview.path, preview.remote))
                .collect()
        };
        assert_eq!(found(false), []);
        assert_eq!(
            found(true),
            [
                ("https://x.org/d.png".to_string(), true),
                ("http://x.org/e.GIF".to_string(), true),
                ("https://x.org/f.svg".to_string(), true),
            ]
        );
    }
}
