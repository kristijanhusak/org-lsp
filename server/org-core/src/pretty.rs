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

/// A sub- or superscript as `org-raise-scripts` finds it: where its `_`
/// or `^` is, the range of what follows (braces included), and whether it
/// is raised (`^`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Script {
    pub marker: usize,
    pub body: (usize, usize),
    pub sup: bool,
}

/// `org-match-sexp-depth`
const SEXP_DEPTH: usize = 3;

/// The sub- and superscripts Org raises with
/// `org-pretty-entities-include-sub-superscripts`: those
/// `org-match-substring-regexp` finds (with `braces_only`,
/// `org-match-substring-with-braces-regexp`, as `org-use-sub-superscripts`
/// `{}` has it), in order, but not on a comment or keyword line, nor in
/// fixed-width text, a verbatim block, emphasis, a link, a timestamp, a
/// footnote reference, tags or a keyword (`org-special-keyword`).
pub fn find_scripts(root: &org_syntax::SyntaxNode, text: &str, braces_only: bool) -> Vec<Script> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_start_matches([' ', '\t']);
        let skipped =
            trimmed.starts_with("# ") || trimmed.starts_with("#+") || trimmed == ":" || trimmed.starts_with(": ");
        if !skipped {
            // Where the search goes on from: after a match, or on its last
            // character when another `_` or `^` follows (`a_b^c`).
            let mut from = 0;
            for (at, c) in body.char_indices() {
                if !matches!(c, '_' | '^') || at == 0 || at <= from {
                    continue;
                }
                let before = body[..at].chars().next_back().unwrap();
                if before.is_whitespace() || at - before.len_utf8() < from {
                    continue;
                }
                let Some(end) = script_body(body, at + 1, braces_only) else { continue };
                let script =
                    Script { marker: line_start + at, body: (line_start + at + 1, line_start + end), sup: c == '^' };
                from = if body[end..].starts_with(['_', '^']) {
                    end - body[..end].chars().next_back().unwrap().len_utf8()
                } else {
                    end
                };
                if raised_here(root, script.body.0) {
                    out.push(script);
                }
            }
        }
        line_start += line.len();
    }
    out
}

/// Where the script whose text starts at `at` of `line` ends: a group in
/// braces or parentheses (nested `org-match-sexp-depth` deep at most),
/// `*`, or letters, digits, `.`, `,` and `\` ending with a letter or
/// digit, a sign before them.
fn script_body(line: &str, at: usize, braces_only: bool) -> Option<usize> {
    let rest = &line[at..];
    let group = |open: char, close: char| -> Option<usize> {
        let mut depth = 0;
        for (offset, c) in rest.char_indices() {
            if c == open {
                depth += 1;
                if depth > SEXP_DEPTH {
                    return None;
                }
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(at + offset + 1);
                }
            }
        }
        None
    };
    if rest.starts_with('{') {
        return group('{', '}');
    }
    if braces_only {
        return None;
    }
    if rest.starts_with('(') {
        return group('(', ')');
    }
    if rest.starts_with('*') {
        return Some(at + 1);
    }
    let sign = usize::from(rest.starts_with(['+', '-']));
    let mut end = None;
    for (offset, c) in rest[sign..].char_indices() {
        if c.is_alphanumeric() {
            end = Some(at + sign + offset + c.len_utf8());
        } else if !matches!(c, '.' | ',' | '\\') {
            break;
        }
    }
    end
}

/// Whether Org raises a script whose text starts at `at`: not in what
/// font-lock gives another face (emphasis, links, timestamps, footnote
/// references, tags, keywords) nor in a verbatim block.
fn raised_here(root: &org_syntax::SyntaxNode, at: usize) -> bool {
    use org_syntax::SyntaxKind::*;
    let Some(token) = root.token_at_offset(org_syntax::TextSize::from(at as u32)).right_biased() else { return true };
    if matches!(token.kind(), PROPERTY_KEY | PLANNING_KEYWORD | CLOCK_KEYWORD) {
        return false;
    }
    !token.parent_ancestors().any(|node| match node.kind() {
        BOLD | ITALIC | UNDERLINE | STRIKE_THROUGH | CODE | VERBATIM | LINK | PLAIN_LINK | ANGLE_LINK | TIMESTAMP
        | FOOTNOTE_REFERENCE | TAGS | FIXED_WIDTH | COMMENT => true,
        BLOCK => {
            let name = node.first_token().map(|first| first.text().to_ascii_lowercase()).unwrap_or_default();
            let name = name.trim_start().trim_start_matches("#+begin_");
            ["src", "example", "export", "comment"].iter().any(|verbatim| name.starts_with(verbatim))
        }
        _ => false,
    })
}

/// The sub- and superscripts shown in Unicode, where every character of
/// their text has a raised or lowered form (`x²`, `H₂O`), as an editor
/// cannot raise text as Emacs does: the range from the `_` or `^` to the
/// end, and what shows in its place. The others are left as they are.
pub fn scripts(root: &org_syntax::SyntaxNode, text: &str, braces_only: bool) -> Vec<(usize, usize, String)> {
    find_scripts(root, text, braces_only)
        .into_iter()
        .filter_map(|script| {
            let body = &text[script.body.0..script.body.1];
            let inner = body.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')).unwrap_or(body);
            let shown: Option<String> =
                inner.chars().map(|c| if script.sup { superscript(c) } else { subscript(c) }).collect();
            shown.filter(|shown| !shown.is_empty()).map(|shown| (script.marker, script.body.1, shown))
        })
        .collect()
}

fn superscript(c: char) -> Option<char> {
    const FROM: &str = "0123456789+-=()abcdefghijklmnoprstuvwxyzABDEGHIJKLMNOPRTUVW";
    const TO: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖʳˢᵗᵘᵛʷˣʸᶻᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁⱽᵂ";
    FROM.chars().position(|from| from == c).and_then(|index| TO.chars().nth(index))
}

fn subscript(c: char) -> Option<char> {
    const FROM: &str = "0123456789+-=()aehijklmnoprstuvx";
    const TO: &str = "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ";
    FROM.chars().position(|from| from == c).and_then(|index| TO.chars().nth(index))
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

    /// What Emacs raises (`org-raise-scripts`), as `(text start, text
    /// end, sup)` from 0, for each case.
    fn found(text: &str) -> Vec<(usize, usize, bool)> {
        let parse = org_syntax::parse(text, &Default::default());
        find_scripts(&parse.syntax(), text, false).into_iter().map(|s| (s.body.0, s.body.1, s.sup)).collect()
    }

    #[test]
    fn finds_scripts_where_org_raises_them() {
        assert_eq!(
            found("x^2 and H_2O and a_{bc} and e^{i\\pi}\n"),
            [(2, 3, true), (10, 12, false), (19, 23, false), (30, 36, true)]
        );
        assert_eq!(
            found("a_b^c x^-1 y^(n+1) z^* w_{a{b}c} 10^3.5 _no ^no\n"),
            [
                (2, 3, false),
                (4, 5, true),
                (8, 10, true),
                (13, 18, true),
                (21, 22, true),
                (25, 32, false),
                (36, 39, true)
            ]
        );
        assert_eq!(found("# x^2 comment\n#+TITLE: x^2\n[[https://a.b/x_y][l_x]] *b_x* ~c_x~ =d_x=\n"), []);
        assert_eq!(found("| a_1 | b^2 |\n"), [(4, 5, false), (10, 11, true)]);
        assert_eq!(found("#+begin_src c\nx_y\n#+end_src\n#+begin_example\nx_y\n#+end_example\n"), []);
        assert_eq!(
            found("* H_2 :t_a:\nSCHEDULED: <2024-01-01 Mon>\n:PROPERTIES:\n:X_Y: a_b\n:END:\nfile_name.txt https://x.y/a_b\n"),
            [(4, 5, false), (61, 62, false), (74, 82, false)]
        );
        assert_eq!(
            found("#+begin_quote\nq_1\n#+end_quote\n#+begin_verse\nv_1\n#+end_verse\n#+begin_center\nc_1\n#+end_center\n#+begin_export html\ne_1\n#+end_export\n"),
            [(16, 17, false), (46, 47, false), (77, 78, false)]
        );
        assert_eq!(
            found("$x_1$ \\(y_1\\) \\[z_1\\] src_c{a_b} [fn:a_b] <2024-01-01 Mon> <<t_1>> {{{m_1}}} [cite:@k_1] 1_000 foo__bar\n"),
            [
                (3, 4, false),
                (10, 11, false),
                (18, 19, false),
                (26, 27, false),
                (30, 31, false),
                (63, 64, false),
                (72, 73, false),
                (86, 87, false),
                (91, 94, false),
                (100, 103, false)
            ]
        );
        assert_eq!(
            found(":LOGBOOK:\nl_1\n:END:\n- i_1 :: d_1\n  #+TBLFM: $1=a_1\nCLOSED: [2024-01-01 Mon 10:00] s_1\n"),
            [(12, 13, false), (24, 25, false), (31, 32, false), (84, 85, false)]
        );
        assert_eq!(found("\\begin{equation}\nx_1\n\\end{equation}\n: f_1\n#+RESULTS:\n: r_1\n"), [(19, 20, false)]);
    }

    #[test]
    fn shows_scripts_in_unicode_where_it_can() {
        let shown = |text: &str| -> Vec<(String, String)> {
            let parse = org_syntax::parse(text, &Default::default());
            scripts(&parse.syntax(), text, false)
                .into_iter()
                .map(|(start, end, shown)| (text[start..end].to_string(), shown))
                .collect()
        };
        let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
        };
        assert_eq!(
            shown("x^2 H_2O a_{bc} y^(n+1) e^{i\\pi} z^* f_{max}\n"),
            pairs(&[("^2", "²"), ("^(n+1)", "⁽ⁿ⁺¹⁾"), ("_{max}", "ₘₐₓ")])
        );
        // `H_2O`: `2O` has no lowered `O`.
        assert_eq!(shown("H_2 O\n"), pairs(&[("_2", "₂")]));
        // With `org-use-sub-superscripts` `{}`, braces only.
        let parse = org_syntax::parse("x^2 y^{2}\n", &Default::default());
        assert_eq!(scripts(&parse.syntax(), "x^2 y^{2}\n", true), [(5, 9, "²".to_string())]);
    }
}

/// org-bullets (`org-bullets-mode`): the last star of each line that
/// starts with stars and a space (`^\\*+ `), shown as the bullet of its
/// level, `bullets` repeating (`org-bullets-level-char`). The stars before
/// it are hidden, which the caller does as `org-hide-leading-stars` does.
pub fn bullets(text: &str, bullets: &[String]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    if bullets.is_empty() {
        return out;
    }
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let stars = line.bytes().take_while(|byte| *byte == b'*').count();
        if stars > 0 && line.as_bytes().get(stars) == Some(&b' ') {
            out.push((start + stars - 1, start + stars, bullets[(stars - 1) % bullets.len()].clone()));
        }
        start += line.len();
    }
    out
}

#[cfg(test)]
mod bullet_tests {
    use super::bullets;

    #[test]
    fn shows_the_last_star_as_the_bullet_of_its_level() {
        let list: Vec<String> = ["◉", "○", "✸", "✿"].iter().map(|bullet| bullet.to_string()).collect();
        let text = "* One\n** Two\n***** Five\n*not\n**\tTab\ntext * star\n";
        assert_eq!(bullets(text, &list), [(0, 1, "◉".to_string()), (7, 8, "○".to_string()), (17, 18, "◉".to_string())]);
        assert!(bullets(text, &[]).is_empty());
    }
}
