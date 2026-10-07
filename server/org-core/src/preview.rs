//! What an entry shows of itself in a popup, such as a hover over its line
//! in the agenda: its headline and its text up to the next heading, each
//! drawer folded to its first line as Emacs shows a folded drawer
//! (`:PROPERTIES:...`).

use std::sync::LazyLock;

use regex::Regex;

use crate::todo::headline_level;

/// `org-drawer-regexp`
static DRAWER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:([\w-]+):[ \t]*$").expect("valid"));

/// The tags at the end of a headline, and the blanks before them.
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+(:[\w@#%:]+:)[ \t]*$").expect("valid"));

/// The entry whose headline starts at byte `heading` of `text`.
pub fn entry(text: &str, heading: usize) -> String {
    let lines: Vec<&str> = text[heading..].lines().collect();
    let Some(headline) = lines.first() else { return String::new() };
    // The tags right after the title: a popup has no column to align to.
    let mut out = vec![TAGS.replace(headline, " $1").into_owned()];
    let body: Vec<&str> = lines[1..].iter().copied().take_while(|line| headline_level(line).is_none()).collect();
    let mut at = 0;
    while at < body.len() {
        let line = body[at];
        let end = DRAWER
            .captures(line)
            .filter(|found| !found[1].eq_ignore_ascii_case("END"))
            .and_then(|_| body[at + 1..].iter().position(|line| is_end(line)));
        match end {
            Some(end) => {
                out.push(format!("{}...", line.trim_end()));
                at += end + 2;
            }
            None => {
                out.push(line.trim_end().to_string());
                at += 1;
            }
        }
    }
    while out.len() > 1 && out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn is_end(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case(":END:")
}

/// `text` as a fenced Org block of Markdown, the fence longer than any run
/// of backquotes in it.
pub fn markdown(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}org\n{text}\n{fence}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_drawers_and_stops_at_the_next_heading() {
        let text = "* Before\n** TODO Write the report                                  :work:\nSCHEDULED: <2026-05-18 Mon>\n:PROPERTIES:\n:ID: 1\n:END:\n:LOGBOOK:\n- Note taken\n:END:\nThe text.\n:NOTES:\nNo end, no drawer.\n\n*** Sub\nsub text\n";
        let heading = text.find("** TODO").unwrap();
        assert_eq!(
            entry(text, heading),
            "** TODO Write the report :work:\nSCHEDULED: <2026-05-18 Mon>\n:PROPERTIES:...\n:LOGBOOK:...\nThe text.\n:NOTES:\nNo end, no drawer."
        );
        assert_eq!(entry("* Alone", 0), "* Alone");
    }

    #[test]
    fn fences_longer_than_what_is_inside() {
        assert_eq!(markdown("* A"), "```org\n* A\n```");
        assert_eq!(markdown("````\ncode"), "`````org\n````\ncode\n`````");
    }
}
