//! `org-num-mode`: the numbers Org shows before headlines, `1.`, `1.2.`
//! and so on (`org-num--number-region`), with the options that leave
//! headlines out.

use crate::outline::{complex_heading_parts, Outline};
use org_syntax::TodoKeywords;

/// The options of `org-num-mode`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `org-num-max-level`: the deepest level numbered, all when none.
    pub max_level: Option<usize>,
    /// `org-num-skip-commented`
    pub skip_commented: bool,
    /// `org-num-skip-footnotes`: the headline `org-footnote-section` names.
    pub skip_footnotes: bool,
    /// `org-num-skip-tags`
    pub skip_tags: Vec<String>,
    /// `org-num-skip-unnumbered`: the `UNNUMBERED` property.
    pub skip_unnumbered: bool,
}

/// `org-footnote-section`, as Org has it.
const FOOTNOTE_SECTION: &str = "Footnotes";

/// A numbered headline: where its title starts (after the stars and the
/// space, where Emacs's overlay ends), and what shows there, `1.2 `, or
/// nothing for a headline left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Number {
    pub at: usize,
    pub shown: Option<String>,
}

/// `org-num--number-region` over the whole text: a [`Number`] for each
/// headline down to `org-num-max-level`.
pub fn numbers(text: &str, outline: &Outline, todo: &TodoKeywords, settings: &Settings) -> Vec<Number> {
    // `org-num--headline-regexp`: deeper headlines are not seen at all.
    let deepest = settings.max_level.map(|max| if outline.odd_levels { 2 * max - 1 } else { max });
    // `org-num--numbering`, deepest level first, and
    // `org-num--skip-level`.
    let mut numbering: Vec<u64> = Vec::new();
    let mut skip_level: Option<usize> = None;
    let mut found = Vec::new();
    for entry in &outline.entries {
        if deepest.is_some_and(|deepest| entry.level > deepest) {
            continue;
        }
        let level = crate::structure::reduced_level(entry.level, outline.odd_levels);
        let line = &text[entry.offset..entry.line_end];
        let skip = skipped(line, entry.level, todo, entry, settings);
        let shown = if skip_level.is_some_and(|skipped| level > skipped) {
            None
        } else if skip {
            skip_level = Some(level);
            None
        } else {
            skip_level = None;
            let last = numbering.len();
            numbering = if numbering.is_empty() {
                std::iter::once(1).chain(std::iter::repeat_n(0, level - 1)).collect()
            } else if level == last {
                std::iter::once(numbering[0] + 1).chain(numbering[1..].iter().copied()).collect()
            } else if level < last {
                let suffix = &numbering[last - level..];
                std::iter::once(suffix[0] + 1).chain(suffix[1..].iter().copied()).collect()
            } else {
                std::iter::once(1)
                    .chain(std::iter::repeat_n(0, level - last - 1))
                    .chain(numbering.iter().copied())
                    .collect()
            };
            // `org-num-default-format`
            let parts: Vec<String> = numbering.iter().rev().map(u64::to_string).collect();
            Some(format!("{} ", parts.join(".")))
        };
        let at = (entry.offset + entry.level + 1).min(entry.line_end);
        found.push(Number { at, shown });
    }
    found
}

/// `org-num--skip-value`: whether the headline `line` is left out, with
/// its subtree.
fn skipped(line: &str, stars: usize, todo: &TodoKeywords, entry: &crate::outline::Entry, settings: &Settings) -> bool {
    let parts = complex_heading_parts(line, stars, todo);
    let title = parts.title.map(|(start, end)| &line[start..end]);
    let tags: Vec<&str> = parts
        .tags
        .map(|(start, end)| line[start..end].split(':').filter(|tag| !tag.is_empty()).collect())
        .unwrap_or_default();
    (settings.skip_footnotes && title == Some(FOOTNOTE_SECTION))
        || (settings.skip_commented && title.is_some_and(|title| title == "COMMENT" || title.starts_with("COMMENT ")))
        || tags.iter().any(|tag| settings.skip_tags.iter().any(|skipped| skipped == tag))
        || (settings.skip_unnumbered && entry.properties.iter().any(|(key, _)| key == "UNNUMBERED"))
}
