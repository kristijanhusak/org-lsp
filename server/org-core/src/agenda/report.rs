//! Headlines as the clock shows them: titles without keyword, priority,
//! `COMMENT` and tags, links as their descriptions, and shortened as
//! clock tables shorten them. The agenda's clock report itself is a clock
//! table (`clocktable::agenda_report`).

use super::item::hidden_ranges;

/// `org-get-heading t t t t`: the title without keyword, priority,
/// `COMMENT` and tags.
pub(crate) fn heading_title(head: &str, todo: Option<&str>) -> String {
    let mut text = head;
    if let Some(todo) = todo {
        text = text.strip_prefix(todo).map_or(text, str::trim_start);
    }
    if text.starts_with("[#") {
        if let Some(end) = text.find(']') {
            text = text[end + 1..].trim_start();
        }
    }
    if let Some(rest) = text.strip_prefix("COMMENT").filter(|rest| rest.is_empty() || rest.starts_with(' ')) {
        text = rest.trim_start();
    }
    if let Some((blanks, _)) = crate::text::tag_group(text) {
        text = &text[..blanks];
    }
    text.trim_end().to_string()
}

/// The headline as Emacs's mode line shows a clocked entry
/// (`org-clock--mode-line-heading`): the title, its links as their
/// descriptions.
pub fn plain_heading(head: &str, todo: Option<&str>) -> String {
    display_format(&heading_title(head, todo)).trim().to_string()
}

/// Statistics cookies out (`\[[0-9]*\(?:%\|/[0-9]*\)\]`).
pub(crate) fn without_cookies(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let tail = &after[digits..];
        let cookie_end = if let Some(stripped) = tail.strip_prefix("%]") {
            Some(after.len() - stripped.len())
        } else if let Some(stripped) = tail.strip_prefix('/') {
            let more = stripped.bytes().take_while(u8::is_ascii_digit).count();
            stripped[more..].starts_with(']').then(|| digits + 1 + more + 1)
        } else {
            None
        };
        match cookie_end {
            Some(length) => rest = &after[length..],
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `org-link--normalize-string`: cookies out, blanks packed.
pub(crate) fn normalize(text: &str) -> String {
    without_cookies(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `org-link-display-format`: links as their descriptions.
pub(crate) fn display_format(text: &str) -> String {
    let hidden = hidden_ranges(text);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, end) in &hidden {
        out.push_str(&text[copied..*start]);
        copied = *end;
    }
    out.push_str(&text[copied..]);
    out
}

/// `org-shorten-string`
pub(crate) fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    // Up to `max - 4` characters and a non-blank, then a blank or the end.
    let limit = max.saturating_sub(4).max(1);
    let mut best = None;
    for end in (2..=limit + 1).rev() {
        if end > chars.len() {
            continue;
        }
        let last = chars[end - 1];
        let follows = chars.get(end).copied();
        if last != ' ' && matches!(follows, None | Some(' ')) {
            best = Some(end);
            break;
        }
    }
    match best {
        Some(end) => format!("{}...", chars[..end].iter().collect::<String>()),
        None => format!("{}...", chars[..max.saturating_sub(3)].iter().collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_as_org_does() {
        assert_eq!(shorten("short", 40), "short");
        assert_eq!(shorten("a very long headline that goes on and on and on", 20), "a very long...");
        assert_eq!(shorten("unbreakablewordthatgoesonandon", 10), "unbreak...");
    }

    #[test]
    fn cookies_and_links() {
        assert_eq!(without_cookies("Plan [1/3] it [50%] [x]"), "Plan  it  [x]");
        assert_eq!(plain_heading("TODO Call [[https://x.org][them]] :a:", Some("TODO")), "Call them");
    }
}
