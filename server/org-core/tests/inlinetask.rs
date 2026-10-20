//! Inline tasks against Emacs with `org-inlinetask` loaded.
//!
//! `tests/inlinetask/cases.json` is written by
//! `scripts/update-inlinetask-fixtures.py`: a document, where the cursor
//! is (and the region's other end), the keys, and the document and cursor
//! after them, as Emacs leaves them. `TAB` is the editor's, by the folding
//! ranges; the server's tests check it.

use std::path::Path;

use org_core::indent::{self, Adapt};
use org_core::motion::{self, Motion};
use org_core::structure::{self, Settings};
use org_core::{inlinetask, Refusal};
use org_syntax::TodoKeywords;
use serde_json::Value;

/// The options of inline tasks hold for the process: the tests take
/// turns.
static OPTIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

/// The string bound to `variable` in the case's Lisp, if it is.
fn bound<'a>(lisp: &'a str, variable: &str) -> Option<&'a str> {
    let rest = lisp.split(&format!("({variable} ")).nth(1)?;
    Some(rest.split(')').next()?.trim_matches('"'))
}

#[test]
fn inline_tasks_work_as_in_emacs() {
    let _turn = OPTIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/inlinetask/cases.json"));
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keywords = TodoKeywords::default();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &cases {
        let key = case["key"].as_str().unwrap();
        if key.starts_with("TAB") {
            continue;
        }
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let lisp = case["emacs"].as_str().unwrap_or("");
        inlinetask::set_min_level(Some(
            bound(lisp, "org-inlinetask-min-level").map_or(15, |level| level.parse().unwrap()),
        ));
        let odd = bound(lisp, "org-odd-levels-only") == Some("t") || text.contains("#+STARTUP: odd");
        org_core::structure::set_odd_levels_default(bound(lisp, "org-odd-levels-only") == Some("t"));
        let adapt = if bound(lisp, "org-adapt-indentation") == Some("t") { Adapt::All } else { Adapt::Off };
        let settings = Settings {
            tags_column: -77,
            indent: indent::Settings { adapt, ..indent::Settings::default() },
            allow_promoting_top_level: false,
            odd_levels: odd,
        };
        let region = case["mark"].as_u64().map(|mark| {
            let mark = byte(text, mark as usize);
            (mark.min(point), mark.max(point))
        });
        let state = bound(lisp, "org-inlinetask-default-state");
        let parse = org_syntax::parse(text, &keywords);
        let moved = |motion, count| {
            motion::run(text, &parse, point, motion, count, false)
                .map(|moved| org_core::newline::Outcome { text: text.to_string(), cursor: moved.offset })
                .map_err(Refusal::Failed)
        };
        let outcome = match key {
            "C-c C-x t" => inlinetask::insert_task(text, point, region, state),
            "C-u C-c C-x t" => inlinetask::insert_task(text, point, region, None),
            "M-<left>" => structure::change_levels(text, point, -1, false, &keywords, &settings),
            "M-<right>" => structure::change_levels(text, point, 1, false, &keywords, &settings),
            "M-S-<left>" => structure::change_levels(text, point, -1, true, &keywords, &settings),
            "M-S-<right>" => structure::change_levels(text, point, 1, true, &keywords, &settings),
            "C-c C-n" => moved(Motion::NextHeading, 1),
            "C-u 2 C-c C-n" => moved(Motion::NextHeading, 2),
            "C-c C-p" => moved(Motion::PreviousHeading, 1),
            _ => panic!("unknown key {key}"),
        };
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        let name = format!("{key} {lisp}");
        match (outcome, expected.get("error")) {
            (Err(refusal), Some(error)) => {
                compared += 1;
                if refusal.message() != error.as_str().unwrap() {
                    failures.push(format!(
                        "{name} {}\n  expected error {error}\n  got {}",
                        show(text, point),
                        refusal.message()
                    ));
                }
            }
            (Err(refusal), None) => {
                failures.push(format!("{name} {}\n  refused {}", show(text, point), refusal.message()))
            }
            (Ok(outcome), Some(error)) => failures.push(format!(
                "{name} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok(outcome), None) => {
                compared += 1;
                let want = expected["text"].as_str().unwrap();
                let cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                if outcome.text != want || outcome.cursor != cursor {
                    failures.push(format!(
                        "{name} {}\n  expected {}\n  got      {}",
                        show(text, point),
                        show(want, cursor),
                        show(&outcome.text, outcome.cursor)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
    assert!(compared > 200, "only {compared} cases compared");
}

/// `org-inlinetask-fontify` hides the stars of an inline task but the
/// last two, as `org-hide` paints them in Emacs, with or without
/// `org-hide-leading-stars`; `org-indent-mode` keeps the text after it at
/// its entry's level (`org-indent-add-properties`).
#[test]
fn hides_the_stars_of_inline_tasks() {
    let _turn = OPTIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    inlinetask::set_min_level(Some(15));
    let stars = "*".repeat(15);
    let text = format!("** A\n{stars} Task\nbody\n{stars} END\nafter\n");
    let shown = |indent: bool, hide: bool| -> Vec<[usize; 4]> {
        indent::virtual_indentation(&text, indent, hide, Adapt::Off, None, 2)
            .into_iter()
            .map(|line| [line.line, line.prefix, line.wrap, line.hidden_stars])
            .collect()
    };
    assert_eq!(shown(false, false), [[1, 0, 0, 13], [3, 0, 0, 13]]);
    assert_eq!(shown(false, true), [[0, 0, 0, 1], [1, 0, 0, 13], [3, 0, 0, 13]]);
    assert_eq!(shown(true, true), [[0, 1, 4, 1], [1, 14, 30, 13], [2, 4, 4, 0], [3, 14, 30, 13], [4, 4, 4, 0]]);
    inlinetask::set_show_first_star(true);
    assert_eq!(shown(false, false), Vec::<[usize; 4]>::new());
    inlinetask::set_show_first_star(false);
}

/// With inline tasks on, the tests of each line take the shortest task's
/// stars found once for the text: an outline of many headlines builds,
/// and the stars of a long text are looked over, in linear time.
#[test]
fn many_lines_are_looked_over_fast() {
    let _turn = OPTIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    inlinetask::set_min_level(Some(15));
    let stars = "*".repeat(15);
    let text = "* a\nb\n".repeat(4000) + &format!("{stars} Task :x:\nbody\n{stars} END\n");
    let start = std::time::Instant::now();
    let parse = org_syntax::parse(&text, &TodoKeywords::default());
    let outline = org_core::outline::Outline::new(&parse, &text, "notes");
    assert_eq!(outline.entries.len(), 4002);
    assert_eq!(outline.entries[4001].tags.iter().map(|tag| tag.name.as_str()).collect::<Vec<_>>(), ["x"]);
    assert!(inlinetask::hides_stars(&text));
    assert!(!inlinetask::hides_stars(&"* a\nb\n".repeat(4000)));
    // Generous: this takes a fraction of a second in a debug build.
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "took {:?}", start.elapsed());
}
