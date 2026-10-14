//! `org-link-preview-region` (`C-c C-x C-v`): the links whose file can be
//! shown in place of the link, an image, and what each one shows.
//!
//! As Org 9.8 has it, a `file:` or `attachment:` link (or a bracket link to
//! a path) is previewable when its path names an image file
//! (`image-file-name-regexp`) and it has no description, or its
//! description is itself such a link with an explicit type, and nothing
//! else. With `include_linked`, links with a description are previewed
//! too.

use org_syntax::{Parse, SyntaxKind, SyntaxNode};

use crate::open::{self, Open};

/// A link to preview: where it is, and the file it names, as written
/// (relative to the text's own file) or, for an attachment, its name in
/// the entry's attachment directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub start: usize,
    pub end: usize,
    pub path: String,
    pub attachment: bool,
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
/// `global` is `org-link-abbrev-alist`.
pub fn previews(
    text: &str,
    parse: &Parse,
    global: &[(String, String)],
    include_linked: bool,
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
        let (path, attachment) = if !described || include_linked {
            let Some(link) = open::link_at(text, parse, start, global, &open::Settings::default()) else { continue };
            match link.open {
                Ok(Open::File { path, search: None, .. }) => (path, false),
                Ok(Open::Attachment(path)) if !path.contains("::") => (path, true),
                _ => continue,
            }
        } else {
            // A description that is a sole link with its type.
            let Some(inner) = sole_link(&node, text) else { continue };
            inner
        };
        if is_image(&path) {
            out.push(Preview { start, end, path, attachment });
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use org_syntax::TodoKeywords;

    fn found(text: &str, include_linked: bool) -> Vec<(String, bool)> {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        previews(text, &parse, &[], include_linked, 0, text.len())
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
}
