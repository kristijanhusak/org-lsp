//! How a view line is shown: the text with links folded, and which parts
//! of it carry which style.
//!
//! In the Emacs agenda a link shows as its description, and tags are
//! aligned for that display. [`display`] produces the same text, so the
//! columns line up in any editor.

use super::item::hidden_ranges;
use super::{Face, ItemKind, Line, LineKind};
use crate::text::tag_group;

/// What a part of a displayed line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The title of the view.
    Structure,
    Date {
        today: bool,
        weekend: bool,
    },
    /// Text of an entry or grid line that has no style of its own.
    Face(Face),
    Keyword {
        done: bool,
    },
    Priority,
    Tags,
    Link,
}

/// A styled byte range of a displayed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// A link in a displayed line: where its text is, and what it points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Displayed {
    pub text: String,
    /// In order, without overlap. Parts without a style are left out.
    pub segments: Vec<Segment>,
    pub links: Vec<Link>,
}

/// Where a priority cookie starts in `text`, at or after `from`.
fn priority_cookie(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    text[from..].match_indices("[#").find_map(|(at, _)| {
        let at = from + at;
        let len = match bytes[at + 2..] {
            [value, b']', ..] if value.is_ascii_uppercase() || value.is_ascii_digit() => 4,
            [b'1'..=b'5', b'0'..=b'9', b']', ..] | [b'6', b'0'..=b'4', b']', ..] => 5,
            _ => return None,
        };
        Some((at, at + len))
    })
}

/// The displayed form of a view line.
pub fn display(line: &Line) -> Displayed {
    let raw = line.text.as_str();
    let item = match (&line.kind, &line.item) {
        (LineKind::Header | LineKind::Separator, _) => return whole(raw, Style::Structure),
        (LineKind::Date { today, weekend, .. }, _) => {
            return whole(raw, Style::Date { today: *today, weekend: *weekend })
        }
        (LineKind::Item, Some(item)) => item,
        (LineKind::Item, None) => return Displayed { text: raw.to_string(), ..Displayed::default() },
    };

    // Styled parts in raw offsets: keyword, priority, links, tags.
    let mut parts: Vec<Segment> = Vec::new();
    let heading = item.heading.min(raw.len());
    if item.keyword_len > 0 {
        let dots = raw[heading..].bytes().take_while(|b| *b == b'.').count();
        let start = heading + dots;
        parts.push(Segment { start, end: start + item.keyword_len, style: Style::Keyword { done: item.keyword_done } });
    }
    if item.kind != ItemKind::Grid {
        if let Some((start, end)) = priority_cookie(raw, heading) {
            parts.push(Segment { start, end, style: Style::Priority });
        }
    }
    let hidden = hidden_ranges(raw);
    // Hidden ranges come in pairs: the opening part and the closing brackets.
    let mut raw_links = Vec::new();
    for pair in hidden.chunks(2) {
        if let [open, close] = pair {
            let target = if open.1 - open.0 > 2 { &raw[open.0 + 2..open.1 - 2] } else { &raw[open.1..close.0] };
            raw_links.push((open.1, close.0, target.to_string()));
            parts.push(Segment { start: open.1, end: close.0, style: Style::Link });
        }
    }
    if let Some((_, tags_start)) = tag_group(raw) {
        let end = raw.trim_end_matches([' ', '\t']).len();
        parts.push(Segment { start: tags_start, end, style: Style::Tags });
    }
    parts.sort_by_key(|part| part.start);
    // The keyword, the cookie and a link may not overlap; the first wins.
    let mut kept: Vec<Segment> = Vec::new();
    for part in parts {
        if kept.last().is_none_or(|last| last.end <= part.start) {
            kept.push(part);
        }
    }

    // Fill the gaps with the face of the line.
    let mut segments = Vec::new();
    let mut at = 0;
    let base = (item.face != Face::Default).then_some(Style::Face(item.face));
    for part in kept {
        if let Some(style) = base.filter(|_| part.start > at) {
            segments.push(Segment { start: at, end: part.start, style });
        }
        at = part.end;
        segments.push(part);
    }
    if let Some(style) = base.filter(|_| raw.len() > at) {
        segments.push(Segment { start: at, end: raw.len(), style });
    }

    // Fold the links: drop the hidden ranges and shift what follows.
    let shift = |offset: usize| -> usize {
        let removed: usize =
            hidden.iter().map(|(start, end)| end.min(&offset).saturating_sub(*start.min(&offset))).sum();
        offset - removed
    };
    let mut text = String::with_capacity(raw.len());
    let mut copied = 0;
    for (start, end) in &hidden {
        text.push_str(&raw[copied..*start]);
        copied = *end;
    }
    text.push_str(&raw[copied..]);
    let segments = segments
        .into_iter()
        .map(|segment| Segment { start: shift(segment.start), end: shift(segment.end), style: segment.style })
        .filter(|segment| segment.end > segment.start)
        .collect();
    let links = raw_links
        .into_iter()
        .map(|(start, end, target)| Link { start: shift(start), end: shift(end), target })
        .collect();
    Displayed { text, segments, links }
}

fn whole(text: &str, style: Style) -> Displayed {
    let segments = if text.is_empty() { Vec::new() } else { vec![Segment { start: 0, end: text.len(), style }] };
    Displayed { text: text.to_string(), segments, links: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agenda::{Item, Source};

    fn item_line(text: &str, heading: usize, keyword_len: usize, face: Face) -> Line {
        Line {
            text: text.to_string(),
            kind: LineKind::Item,
            item: Some(Item {
                text: text.to_string(),
                heading,
                time_of_day: None,
                priority: 0,
                kind: ItemKind::Scheduled,
                face,
                source: Some(Source { file: 0, entry: 0 }),
                marker: None,
                keyword_len,
                keyword_done: false,
            }),
        }
    }

    fn describe(shown: &Displayed) -> Vec<(&str, Style)> {
        shown.segments.iter().map(|segment| (&shown.text[segment.start..segment.end], segment.style)).collect()
    }

    #[test]
    fn an_entry_line_is_split_into_its_parts() {
        let line = item_line(
            "  work:       Scheduled:  TODO [#A] Call [[https://x.org][site]] now    :a:b:",
            26,
            4,
            Face::Scheduled,
        );
        let shown = display(&line);
        assert_eq!(shown.text, "  work:       Scheduled:  TODO [#A] Call site now    :a:b:");
        let face = Style::Face(Face::Scheduled);
        assert_eq!(
            describe(&shown),
            [
                ("  work:       Scheduled:  ", face),
                ("TODO", Style::Keyword { done: false }),
                (" ", face),
                ("[#A]", Style::Priority),
                (" Call ", face),
                ("site", Style::Link),
                (" now    ", face),
                (":a:b:", Style::Tags),
            ]
        );
        assert_eq!(shown.links, [Link { start: 41, end: 45, target: "https://x.org".to_string() }]);
    }

    #[test]
    fn plain_lines_keep_only_their_parts() {
        let line = item_line("  work:       TODO [[target]]", 14, 4, Face::Default);
        let shown = display(&line);
        assert_eq!(shown.text, "  work:       TODO target");
        assert_eq!(describe(&shown), [("TODO", Style::Keyword { done: false }), ("target", Style::Link)]);
        assert_eq!(shown.links[0].target, "target");
    }

    #[test]
    fn headers_and_dates_are_one_segment() {
        let header = Line { text: "Week-agenda (W21):".to_string(), kind: LineKind::Header, item: None };
        assert_eq!(describe(&display(&header)), [("Week-agenda (W21):", Style::Structure)]);
        let date = Line {
            text: "Monday     18 May 2026 W21".to_string(),
            kind: LineKind::Date { day: 1, today: true, weekend: false },
            item: None,
        };
        assert_eq!(display(&date).segments[0].style, Style::Date { today: true, weekend: false });
    }
}
