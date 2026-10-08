//! `org-pretty-entities`: `\\alpha` shown as `α`, as `org-fontify-entities`
//! composes the entities Org knows into their UTF-8 character.

/// The entities shown as their character: the range each covers, `{}`
/// after it included, and the character. As Org: `\\there4`, `\\sup1`
/// to `3`, the fractions and names of letters, ended by the end of a line,
/// `{}` or anything not a letter; not on a comment line, nor the `\\_ `
/// family, nor one whose character is more than one.
pub fn entities(text: &str) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let comment = trimmed.starts_with("# ") || trimmed.trim_end_matches(['\n', '\r']) == "#";
        if !comment {
            let mut at = 0;
            while let Some(found) = line[at..].find('\\') {
                let start = at + found;
                at = start + 1;
                let Some((name_end, end)) = entity_at(line, start + 1) else { continue };
                let name = &line[start + 1..name_end];
                let Some(utf8) = org_syntax::entity_utf8(name) else { continue };
                if utf8.chars().count() != 1 {
                    continue;
                }
                out.push((line_start + start, line_start + end, utf8));
                // The search goes on from the character after the name.
                at = name_end.max(start + 1);
            }
        }
        line_start += line.len();
    }
    out
}

/// The name at `at` of `line` and where the composed text ends: after
/// `{}` when it follows, else after the name; none when what follows the
/// name is a letter.
fn entity_at(line: &str, at: usize) -> Option<(usize, usize)> {
    let rest = &line[at..];
    let ended = |end: usize| -> Option<usize> {
        let after = &line[end..];
        if after.is_empty() || after.starts_with('\n') {
            Some(end)
        } else if after.starts_with("{}") {
            Some(end + 2)
        } else if after.chars().next().is_some_and(|c| !c.is_alphabetic()) {
            Some(end)
        } else {
            None
        }
    };
    // The alternatives in order: `there4`, `sup1` to `3`, the fractions,
    // then letters.
    let special = ["there4", "sup1", "sup2", "sup3", "frac12", "frac14", "frac32", "frac34"];
    for name in special {
        if rest.starts_with(name) {
            if let Some(end) = ended(at + name.len()) {
                return Some((at + name.len(), end));
            }
        }
    }
    let letters = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters == 0 {
        return None;
    }
    ended(at + letters).map(|end| (at + letters, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_as_org_shows_them() {
        let shown = |text: &str| -> Vec<(String, &str)> {
            entities(text).into_iter().map(|(start, end, utf8)| (text[start..end].to_string(), utf8)).collect()
        };
        assert_eq!(shown("\\alpha and \\beta{}x"), [("\\alpha".to_string(), "α"), ("\\beta{}".to_string(), "β")]);
        assert_eq!(shown("\\alphax \\nosuch \\to"), [("\\to".to_string(), "→")]);
        assert_eq!(shown("# \\alpha\n\\there4 \\sup2."), [("\\there4".to_string(), "∴"), ("\\sup2".to_string(), "²")]);
        assert_eq!(shown("\\alpha\\beta"), [("\\alpha".to_string(), "α"), ("\\beta".to_string(), "β")]);
        // More than one character: left as it is.
        assert_eq!(shown("\\Pr"), []);
    }
}
