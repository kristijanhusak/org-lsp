//! Cutting, copying and pasting subtrees against Emacs.
//!
//! `tests/subtree/cases.json` is written by
//! `scripts/update-subtree-fixtures.py`: a document, where the cursor is,
//! the command, and the document, cursor and what was copied after it, as
//! Emacs leaves them.

use std::path::Path;

use org_core::indent::{self, Adapt};
use org_core::structure::{self, Settings};
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn cuts_copies_and_pastes_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/subtree/cases.json"));
    let path = std::env::var("ORG_SUBTREE_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let op = case["op"].as_str().unwrap();
        let adapt = match case["adapt"].as_str() {
            Some("t") => Adapt::All,
            Some("headline-data") => Adapt::HeadlineData,
            _ => Adapt::Off,
        };
        let settings = Settings {
            tags_column: -77,
            indent: indent::Settings { adapt, ..indent::Settings::default() },
            allow_promoting_top_level: false,
            odd_levels: false,
        };
        let count = case["count"].as_u64().unwrap_or(1) as usize;
        // The headlines folded after it, and what Org says.
        let mut folded: Vec<usize> = Vec::new();
        let mut said: Option<String> = None;
        let outcome = match op {
            "cut" | "copy" => {
                structure::copy_subtree(text, point, count, op == "cut").and_then(|(outcome, copied, _)| {
                    let Some(paste) = case["paste"].as_u64() else { return Ok((outcome, Some(copied))) };
                    // `org-paste-subtree` of what was just cut or copied folds it
                    // again when it was folded (`org-subtree-clip-folded`).
                    let at = byte(&outcome.text, paste as usize);
                    let (pasted, _) = structure::paste_subtree(&outcome.text, at, &copied, None, &settings)?;
                    if case["fold"] == true {
                        folded.push(pasted.cursor);
                    }
                    Ok((pasted, Some(copied)))
                })
            }
            // Where Org yanks the text as it is, the editor's own paste.
            "yank" => {
                let kill = case["kill"].as_str().unwrap();
                let adjusted = case["adjusted"] != false;
                let fold = case["folded"] != false;
                match structure::yank_subtree(text, point, kill, &settings, adjusted, fold) {
                    Some(yanked) => yanked.map(|yanked| {
                        folded = yanked.folded;
                        said = yanked.message;
                        (yanked.outcome, None)
                    }),
                    None => {
                        let text = format!("{}{kill}{}", &text[..point], &text[point..]);
                        Ok((org_core::newline::Outcome { text, cursor: point + kill.len() }, None))
                    }
                }
            }
            _ => {
                let level = case["level"].as_u64().map(|level| level as usize);
                structure::paste_subtree(text, point, case["kill"].as_str().unwrap(), level, &settings)
                    .map(|(outcome, _)| (outcome, None))
            }
        };
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        let name = format!("{op} {} {:?}", case["count"], case["kill"].as_str().unwrap_or(""));
        match (outcome, expected.get("error")) {
            (Err(_), Some(_)) => {}
            (Err(refusal), None) => {
                failures.push(format!("{name} {}\n  refused {}", show(text, point), refusal.message()))
            }
            (Ok((outcome, _)), Some(error)) => failures.push(format!(
                "{name} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok((outcome, copied)), None) => {
                let want = expected["text"].as_str().unwrap();
                let cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                let want_copied = expected["kill"].as_str().filter(|kill| *kill != "json-null");
                if outcome.text != want || outcome.cursor != cursor || copied.as_deref() != want_copied {
                    failures.push(format!(
                        "{name} {}\n  expected {} copied {want_copied:?}\n  got      {} copied {copied:?}",
                        show(text, point),
                        show(want, cursor),
                        show(&outcome.text, outcome.cursor)
                    ));
                    continue;
                }
                // The lines that show with their end hidden: the headlines
                // folded, those with text to hide.
                if let Some(want_folds) = expected.get("folds").and_then(Value::as_array) {
                    let after = &outcome.text;
                    let starts: Vec<usize> =
                        std::iter::once(0).chain(after.match_indices('\n').map(|(at, _)| at + 1)).collect();
                    let want_folds: Vec<usize> = want_folds
                        .iter()
                        .filter_map(Value::as_u64)
                        .filter_map(|line| starts.get(line as usize).copied())
                        .filter(|heading| hides_text(after, *heading))
                        .collect();
                    let got_folds: Vec<usize> =
                        folded.iter().copied().filter(|heading| hides_text(after, *heading)).collect();
                    if got_folds != want_folds {
                        failures.push(format!(
                            "{name} {}\n  expected folds at {want_folds:?}\n  got         {got_folds:?}",
                            show(text, point)
                        ));
                    }
                }
                // What Org says last; "Mark set" is `yank`'s, about the
                // mark, which org-lsp has none of.
                if let Some(want_said) = expected.get("message").filter(|_| op == "yank") {
                    let want_said = want_said.as_str().filter(|said| *said != "Mark set");
                    let yanked = case["adjusted"] != false || case["folded"] != false;
                    if yanked && structure::is_subtree(case["kill"].as_str().unwrap()) && said.as_deref() != want_said {
                        failures.push(format!(
                            "{name} {}\n  expected message {want_said:?}\n  got      {said:?}",
                            show(text, point)
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

/// Whether folding the subtree of the headline at `heading` hides text:
/// more than blank lines after its line and before the next headline of
/// its level or above. (Emacs hides blank lines too, and the newline at
/// the end of the text, which an editor's folds do not.)
fn hides_text(text: &str, heading: usize) -> bool {
    // `org-outline-regexp`: stars and a blank.
    let level = |at: usize| {
        let stars = text[at..].bytes().take_while(|b| *b == b'*').count();
        (stars > 0 && text.as_bytes().get(at + stars) == Some(&b' ')).then_some(stars)
    };
    let Some(own) = level(heading) else { return false };
    let Some(newline) = text[heading..].find('\n') else { return false };
    let mut at = heading + newline + 1;
    let start = at;
    while at < text.len() {
        if level(at).is_some_and(|found| found <= own) {
            break;
        }
        at = text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1);
    }
    !text[start..at].trim().is_empty()
}
