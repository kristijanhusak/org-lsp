//! Moving about an Org document as Org's motion commands do:
//! `org-next-visible-heading` (`C-c C-n`), `org-previous-visible-heading`
//! (`C-c C-p`), `org-forward-heading-same-level` (`C-c C-f`),
//! `org-backward-heading-same-level` (`C-c C-b`), `outline-up-heading`
//! (`C-c C-u`), `org-next-link` (`C-c C-x C-n`) and `org-previous-link`
//! (`C-c C-x C-p`).
//!
//! Headings are found as Org finds them here, by the regexp `^\*+ `, so
//! that a line of stars in a block counts too. Everything is taken to be
//! visible: the folds are the editor's. Offsets are bytes.

use org_syntax::Parse;

/// What to move by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    NextHeading,
    PreviousHeading,
    ForwardSameLevel,
    BackwardSameLevel,
    Up,
    NextLink,
    PreviousLink,
}

/// Where the cursor goes, and what Org says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub offset: usize,
    pub message: Option<String>,
    /// For the link search: nothing was found, so that the same command
    /// right after wraps around.
    pub failed: bool,
}

impl Moved {
    fn to(offset: usize) -> Moved {
        Moved { offset, message: None, failed: false }
    }
}

/// Move from `offset` by `motion`, `count` times (backwards when
/// negative). `failed_before` says that the same link search ran last and
/// found nothing, so that this one wraps around.
pub fn run(
    text: &str,
    parse: &Parse,
    offset: usize,
    motion: Motion,
    count: i64,
    failed_before: bool,
) -> Result<Moved, String> {
    let offset = offset.min(text.len());
    Ok(match motion {
        Motion::NextHeading => Moved::to(next_visible_heading(text, offset, count)),
        Motion::PreviousHeading => Moved::to(next_visible_heading(text, offset, -count)),
        Motion::ForwardSameLevel => Moved::to(forward_same_level(text, offset, count)),
        Motion::BackwardSameLevel => Moved::to(forward_same_level(text, offset, -count)),
        Motion::Up => Moved::to(up_heading(text, offset, count)?),
        Motion::NextLink => next_link(text, parse, offset, false, failed_before),
        Motion::PreviousLink => next_link(text, parse, offset, true, failed_before),
    })
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |found| found + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |found| at + found)
}

/// The level of the heading line starting at `start` (`^\*+ `), if it is
/// one.
fn heading_level(text: &str, start: usize) -> Option<usize> {
    let line = &text[start..];
    let stars = line.bytes().take_while(|&byte| byte == b'*').count();
    (stars > 0 && line.as_bytes().get(stars) == Some(&b' ')).then_some(stars)
}

/// The starts of the heading lines, in order.
fn headings(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut start = 0;
    loop {
        if heading_level(text, start).is_some() {
            starts.push(start);
        }
        match text[start..].find('\n') {
            Some(found) => start += found + 1,
            None => break,
        }
    }
    starts
}

/// `re-search-forward` of `^\*+ ` from `from`: the start of the first
/// heading line at or after it.
fn search_forward(text: &str, from: usize) -> Option<usize> {
    headings(text).into_iter().find(|&start| start >= from)
}

/// `re-search-backward` of `^\*+ ` from `from`: the start of the last
/// heading line before it.
fn search_backward(text: &str, from: usize) -> Option<usize> {
    headings(text).into_iter().rev().find(|&start| start < from)
}

/// The end of the `^\*+ ` match at the heading line `start`.
fn match_end(text: &str, start: usize) -> usize {
    start + heading_level(text, start).unwrap_or(0) + 1
}

/// `org-next-visible-heading`
fn next_visible_heading(text: &str, offset: usize, count: i64) -> usize {
    let mut arg = count;
    let mut point = if arg < 0 { line_start(text, offset) } else { line_end(text, offset) };
    while arg < 0 {
        match search_backward(text, point) {
            Some(start) => {
                point = start;
                arg += 1;
            }
            None => {
                point = 0;
                break;
            }
        }
    }
    while arg > 0 {
        match search_forward(text, point) {
            Some(start) => {
                point = match_end(text, start);
                arg -= 1;
            }
            None => {
                point = text.len();
                break;
            }
        }
    }
    if arg > 0 {
        text.len()
    } else {
        line_start(text, point)
    }
}

/// `org-back-to-heading`: the heading line the cursor is in.
fn back_to_heading(text: &str, offset: usize) -> Option<usize> {
    let start = line_start(text, offset);
    if heading_level(text, start).is_some() {
        return Some(start);
    }
    search_backward(text, start)
}

/// `org-forward-heading-same-level`
fn forward_same_level(text: &str, offset: usize, count: i64) -> usize {
    let backward = count < 0;
    let Some(heading) = back_to_heading(text, offset) else {
        // Before the first heading.
        if backward {
            return 0;
        }
        // `outline-next-heading`
        let from = if offset == line_start(text, offset) && offset < text.len() { offset + 1 } else { offset };
        return search_forward(text, from).unwrap_or(text.len());
    };
    let level = heading_level(text, heading).unwrap_or(1);
    let mut point = if backward { heading } else { line_end(text, heading) };
    let mut result = point;
    let mut left = count.unsigned_abs();
    while left > 0 {
        let found = if backward { search_backward(text, point) } else { search_forward(text, point) };
        let Some(start) = found else { break };
        point = if backward { start } else { match_end(text, start) };
        let found_level = heading_level(text, start).unwrap_or(0);
        if found_level < level {
            left = 0;
        } else if found_level == level {
            left -= 1;
            result = point;
        }
    }
    line_start(text, result)
}

/// `outline-up-heading`
fn up_heading(text: &str, offset: usize, count: i64) -> Result<usize, String> {
    let mut point = back_to_heading(text, offset).ok_or("Before first heading")?;
    let mut start_level = heading_level(text, point).unwrap_or(1);
    if start_level <= 1 {
        return Err("Already at top level of the outline".to_string());
    }
    let mut arg = count;
    while start_level > 1 && arg > 0 && point > 0 {
        let mut level = start_level;
        while !(level < start_level || point == 0) {
            point = search_backward(text, point).unwrap_or(0);
            // Off any heading (the start of the file), Org reads the level
            // of whatever stars are there.
            level = heading_level(text, point).unwrap_or(0);
        }
        start_level = level;
        arg -= 1;
    }
    Ok(point)
}

/// `org-next-link`, or `org-previous-link` when `backward`.
fn next_link(text: &str, parse: &Parse, offset: usize, backward: bool, failed_before: bool) -> Moved {
    let links = crate::open::links(text, parse, &[], &crate::open::Settings::default());
    let mut point = offset;
    let mut message = None;
    if failed_before {
        if backward {
            point = text.len();
            message = Some("Link search wrapped back to end of buffer".to_string());
        } else {
            point = 0;
            message = Some("Link search wrapped back to beginning of buffer".to_string());
        }
    } else if backward {
        // In a link: from its start.
        if let Some(link) = links.iter().find(|link| link.start <= point && point < link.end) {
            point = link.start;
        }
    } else if let Some(link) = links.iter().find(|link| link.start == point) {
        // On a link: past it.
        point = link.end;
    } else if let Some(link) = links.iter().find(|link| link.start < point && point == typed_path(text, link)) {
        // At the path of a bracket or angle link that has a type, Org sees
        // a plain link there, and searches on from past it.
        point = link.path.1;
    } else if let Some(link) = links.iter().find(|link| link.start < point && point < typed_path(text, link)) {
        // Just inside a bracket or angle link whose path has a type (`[[https:…`),
        // Org's search finds that path as a plain link, and the link it is
        // in: the cursor goes back to its start.
        return Moved { offset: link.start, message: None, failed: false };
    }
    let found = if backward {
        links.iter().rev().find(|link| link.start < point)
    } else {
        links.iter().find(|link| link.start >= point)
    };
    match found {
        Some(link) => Moved { offset: link.start, message, failed: false },
        None => Moved { offset, message: Some("No further link found".to_string()), failed: true },
    }
}

/// Where a plain link would start in `link`'s path: after `[[` or `<` when
/// its path begins with a link type (`https:`); else its start, which no
/// cursor inside it is at or before.
fn typed_path(text: &str, link: &crate::open::Link) -> usize {
    let opening = if link.bracket {
        2
    } else if text.as_bytes().get(link.start) == Some(&b'<') {
        1
    } else {
        return link.start;
    };
    let typed = link.raw.split_once(':').is_some_and(|(kind, _)| org_syntax::link_types().contains(&kind));
    if typed {
        link.start + opening
    } else {
        link.start
    }
}
