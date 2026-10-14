//! How a view line is shown: the text with links folded, and which parts
//! of it carry which style.
//!
//! In the Emacs agenda a link shows as its description, and tags are
//! aligned for that display. [`display`] produces the same text, so the
//! columns line up in any editor.

use super::item::hidden_ranges;
use super::{Face, Line, LineKind};
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
    Priority(Rank),
    Tags,
    Link,
    /// A clock report's table.
    Table,
    /// A day of a habit's consistency graph.
    Habit(super::habit::Face),
}

/// Where a priority stands among those of its file: the highest shows in
/// bold, the lowest in italic (`org-agenda-fontify-priorities`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    Highest,
    Lowest,
    Other,
}

/// How a line shows its priority cookies (`org-agenda-fontify-priorities`
/// not `nil`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fontify {
    /// `t`: from the cookie to the end of the line; else the cookie alone.
    pub lines: bool,
    /// `org-priority-highest` and `org-priority-lowest` of the entry's
    /// file, which the line carries; as bytes, for no cookie's value is
    /// more than `z`.
    pub file: (u8, u8),
    /// Their global values, for a cookie that ends the line: Emacs reads
    /// the file's ones after the cookie, where such a line has nothing.
    pub global: (u8, u8),
}

impl Fontify {
    /// A priority as [`Fontify`] keeps it: one no cookie has stays one.
    pub fn byte(priority: i64) -> u8 {
        u8::try_from(priority).unwrap_or(u8::MAX)
    }
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

/// The priority cookies of `text`, as `org-priority-regexp` finds them in
/// the agenda, ignoring case: where each starts and ends, where its match
/// ends (a blank after it included), and its value
/// (`org-priority-to-value`).
fn priority_cookies(text: &str) -> Vec<(usize, usize, usize, i64)> {
    let bytes = text.as_bytes();
    let mut cookies = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("[#").map(|at| from + at) {
        let digit = |b: u8| i64::from(b - b'0');
        let found = match bytes[at + 2..] {
            [value, b']', ..] if value.is_ascii_alphabetic() => Some((4, i64::from(value))),
            [value, b']', ..] if value.is_ascii_digit() => Some((4, digit(value))),
            [tens @ b'1'..=b'5', ones @ b'0'..=b'9', b']', ..] | [tens @ b'6', ones @ b'0'..=b'4', b']', ..] => {
                Some((5, 10 * digit(tens) + digit(ones)))
            }
            _ => None,
        };
        let Some((len, value)) = found else {
            from = at + 2;
            continue;
        };
        let end = at + len;
        let matched = if bytes.get(end) == Some(&b' ') { end + 1 } else { end };
        cookies.push((at, end, matched, value));
        from = matched;
    }
    cookies
}

/// The displayed form of a view line.
pub fn display(line: &Line) -> Displayed {
    let raw = line.text.as_str();
    let item = match (&line.kind, &line.item) {
        (LineKind::Header | LineKind::Separator, _) => return whole(raw, Style::Structure),
        (LineKind::Blank, _) => return whole(raw, Style::Structure),
        // org-super-agenda's header face inherits `org-agenda-structure`.
        (LineKind::Group { faced }, _) => {
            let faced = (*faced).min(raw.len());
            let mut shown = whole(&raw[faced..], Style::Structure);
            shown.text = raw.to_string();
            for segment in &mut shown.segments {
                segment.start += faced;
                segment.end += faced;
            }
            return shown;
        }
        (LineKind::Table, _) => return table(raw),
        (LineKind::Date { today, weekend, .. }, _) => {
            return whole(raw, Style::Date { today: *today, weekend: *weekend })
        }
        (LineKind::Item, Some(item)) => item,
        (LineKind::Item, None) | (LineKind::EntryText, _) => {
            return Displayed { text: raw.to_string(), ..Displayed::default() }
        }
    };

    // Styled parts in raw offsets: keyword, priority, links, tags.
    let mut parts: Vec<Segment> = Vec::new();
    let heading = item.heading.min(raw.len());
    if item.keyword_len > 0 {
        let dots = raw[heading..].bytes().take_while(|b| *b == b'.').count();
        let start = heading + dots;
        parts.push(Segment { start, end: start + item.keyword_len, style: Style::Keyword { done: item.keyword_done } });
    }
    // `org-agenda-fontify-priorities`: each cookie, and with `t` the rest
    // of the line from the first one, where nothing else has a style.
    let mut priority_line = None;
    if let Some(fontify) = item.fontify {
        for (start, end, matched, value) in priority_cookies(raw) {
            let (highest, lowest) = if matched < raw.len() { fontify.file } else { fontify.global };
            let rank = if value == i64::from(lowest) {
                Rank::Lowest
            } else if value == i64::from(highest) {
                Rank::Highest
            } else {
                Rank::Other
            };
            parts.push(Segment { start, end, style: Style::Priority(rank) });
            if fontify.lines {
                priority_line.get_or_insert((start, rank));
            }
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
    // The days of a habit's graph, over whatever they stand on.
    if let Some((start, faces)) = &item.graph {
        let mut at = *start;
        for face in faces {
            let Some(c) = raw[at.min(raw.len())..].chars().next() else { break };
            parts.retain(|part| part.end <= at || part.start >= at + c.len_utf8());
            parts.push(Segment { start: at, end: at + c.len_utf8(), style: Style::Habit(*face) });
            at += c.len_utf8();
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

    // Fill the gaps with the face of the line, or after a priority cookie
    // with `t`, the cookie's.
    let mut segments = Vec::new();
    let mut at = 0;
    let base = (item.face != Face::Default).then_some(Style::Face(item.face));
    let fill = |segments: &mut Vec<Segment>, from: usize, to: usize| {
        if to <= from {
            return;
        }
        match priority_line {
            Some((start, rank)) if to > start => {
                if let Some(style) = base.filter(|_| start > from) {
                    segments.push(Segment { start: from, end: start, style });
                }
                segments.push(Segment { start: from.max(start), end: to, style: Style::Priority(rank) });
            }
            _ => {
                if let Some(style) = base {
                    segments.push(Segment { start: from, end: to, style });
                }
            }
        }
    };
    for part in kept {
        fill(&mut segments, at, part.start);
        at = part.end;
        segments.push(part);
    }
    fill(&mut segments, at, raw.len());

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

/// A table line: its links folded, the rest in the table's style.
fn table(raw: &str) -> Displayed {
    let hidden = hidden_ranges(raw);
    let mut text = String::with_capacity(raw.len());
    let mut segments = Vec::new();
    let mut links = Vec::new();
    let mut copied = 0;
    for pair in hidden.chunks(2) {
        let [open, close] = pair else { continue };
        let target = if open.1 - open.0 > 2 { &raw[open.0 + 2..open.1 - 2] } else { &raw[open.1..close.0] };
        if open.0 > copied {
            segments.push(Segment { start: text.len(), end: text.len() + open.0 - copied, style: Style::Table });
        }
        text.push_str(&raw[copied..open.0]);
        let start = text.len();
        text.push_str(&raw[open.1..close.0]);
        segments.push(Segment { start, end: text.len(), style: Style::Link });
        links.push(Link { start, end: text.len(), target: target.to_string() });
        copied = close.1;
    }
    if raw.len() > copied {
        segments.push(Segment { start: text.len(), end: text.len() + raw.len() - copied, style: Style::Table });
    }
    text.push_str(&raw[copied..]);
    Displayed { text, segments, links }
}

fn whole(text: &str, style: Style) -> Displayed {
    let segments = if text.is_empty() { Vec::new() } else { vec![Segment { start: 0, end: text.len(), style }] };
    Displayed { text: text.to_string(), segments, links: Vec::new() }
}

/// A line marked for a bulk action (`org-agenda-bulk-mark`): its first two
/// characters shown as `mark` and a blank, `mark` in the face of a TODO
/// keyword, as Emacs's overlay shows them.
pub fn mark(displayed: &mut Displayed, mark: &str) {
    let cut = displayed.text.char_indices().nth(2).map_or(displayed.text.len(), |(at, _)| at);
    let prefix = format!("{mark} ");
    let moved = |at: usize| if at >= cut { at - cut + prefix.len() } else { prefix.len().min(at) };
    displayed.text = format!("{prefix}{}", &displayed.text[cut..]);
    for segment in &mut displayed.segments {
        segment.start = moved(segment.start).max(prefix.len());
        segment.end = moved(segment.end).max(prefix.len());
    }
    displayed.segments.retain(|segment| segment.start < segment.end);
    for link in &mut displayed.links {
        link.start = moved(link.start).max(prefix.len());
        link.end = moved(link.end).max(prefix.len());
    }
    displayed.segments.insert(0, Segment { start: 0, end: mark.len(), style: Style::Keyword { done: false } });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agenda::{Item, ItemKind, Source};

    fn item_line(text: &str, heading: usize, keyword_len: usize, face: Face) -> Line {
        Line {
            text: text.to_string(),
            kind: LineKind::Item,
            item: Some(Item {
                text: text.to_string(),
                heading,
                time_of_day: None,
                priority: 0,
                base_priority: 0,
                kind: ItemKind::Scheduled,
                face,
                source: Some(Source { file: 0, entry: 0 }),
                marker: None,
                keyword_len,
                keyword_done: false,
                habit: None,
                graph: None,
                ts_date: None,
                log: None,
                ts_type: "",
                own_tags_only: false,
                fontify: Some(Fontify { lines: false, file: (65, 67), global: (65, 67) }),
            }),
        }
    }

    fn fontified(text: &str, fontify: Option<Fontify>) -> Vec<(String, Style)> {
        let mut line = item_line(text, 2, 0, Face::Default);
        line.item.as_mut().unwrap().fontify = fontify;
        let shown = display(&line);
        describe(&shown).into_iter().map(|(text, style)| (text.to_string(), style)).collect()
    }

    #[test]
    fn priorities_fontified_as_org_agenda_fontify_priorities_says() {
        let cookies = Fontify { lines: false, file: (65, 67), global: (65, 67) };
        // Every cookie, ignoring case; the highest in bold, the lowest in
        // italic.
        assert_eq!(
            fontified("  [#A] x [#c] y [#C] :t:", Some(cookies)),
            [
                ("[#A]".to_string(), Style::Priority(Rank::Highest)),
                ("[#c]".to_string(), Style::Priority(Rank::Other)),
                ("[#C]".to_string(), Style::Priority(Rank::Lowest)),
                (":t:".to_string(), Style::Tags),
            ]
        );
        // `t`: the rest of the line too, where nothing else has a style.
        let lines = Fontify { lines: true, ..cookies };
        assert_eq!(
            fontified("  a [#C] b :t:", Some(lines)),
            [
                ("[#C]".to_string(), Style::Priority(Rank::Lowest)),
                (" b ".to_string(), Style::Priority(Rank::Lowest)),
                (":t:".to_string(), Style::Tags),
            ]
        );
        assert_eq!(fontified("  a [#A] b", None), []);
        // The file's priorities (`#+PRIORITIES: 1 5 3`), but for a cookie
        // that ends the line, the global ones.
        let own = Fontify { lines: false, file: (1, 5), global: (65, 67) };
        assert_eq!(fontified("  a [#1] b", Some(own)), [("[#1]".to_string(), Style::Priority(Rank::Highest))]);
        assert_eq!(fontified("  a [#1]", Some(own)), [("[#1]".to_string(), Style::Priority(Rank::Other))]);
        assert_eq!(fontified("  a [#A]", Some(own)), [("[#A]".to_string(), Style::Priority(Rank::Highest))]);
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
                ("[#A]", Style::Priority(Rank::Highest)),
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
