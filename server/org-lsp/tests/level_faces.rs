//! The level faces of headlines (`org-get-level-face`): which of
//! `org-level-1` to `org-level-8` the stars and title of each headline
//! take with `org-n-level-faces`, `org-cycle-level-faces` and
//! `org-odd-levels-only`, and the lines whose face goes on to the edge of
//! the window with `org-fontify-whole-heading-line`, as Emacs fontifies
//! the texts of `tests/level-faces/cases.json`
//! (`scripts/level_faces_reference.py`).

use org_lsp::highlight::{heading_lines, modifier, tokens_with, Conceal, Fontify, TokenType};
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

/// A headline's line and the levels of the faces of its stars, its title
/// and its newline.
type Faces = (usize, Option<usize>, Option<usize>, Option<usize>);

/// The level of a token's level modifier, if it has one.
fn level(modifiers: u32) -> Option<usize> {
    (0..8).find(|bit| modifiers & (1 << bit) != 0).map(|bit| bit + 1)
}

#[test]
fn headlines_take_the_level_faces_emacs_gives_them() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/level-faces/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let fontify = Fontify {
            n_level_faces: case.get("n").and_then(Value::as_u64).unwrap_or(8) as usize,
            cycle_level_faces: case.get("cycle").and_then(Value::as_bool).unwrap_or(true),
            odd_levels: org_core::structure::odd_levels(
                text,
                case.get("odd").and_then(Value::as_bool).unwrap_or(false),
            ),
            whole_heading_line: case.get("whole").and_then(Value::as_bool).unwrap_or(false),
            ..Fontify::default()
        };
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let tokens = tokens_with(&parse.syntax(), parse.todo_keywords(), Conceal::default(), fontify);
        let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect();
        let line_of = |offset: usize| starts.partition_point(|start| *start <= offset) - 1;
        let whole: Vec<(usize, usize)> = heading_lines(&parse.syntax(), text, &fontify);
        let mut got = Vec::new();
        for line in 0..starts.len() {
            let on_line = |ty: TokenType| {
                tokens.iter().filter(move |token| token.ty == ty && line_of(usize::from(token.range.start())) == line)
            };
            let Some(stars) = on_line(TokenType::Stars).next() else { continue };
            // The title's tokens that keep the level: a done or archived
            // headline puts its own look in its place.
            let titles: Vec<usize> = on_line(TokenType::Headline)
                .filter(|token| token.modifiers & modifier::LEVELS != 0)
                .filter_map(|token| level(token.modifiers))
                .collect();
            let title = titles.first().copied().filter(|first| titles.iter().all(|level| level == first));
            let newline = whole.iter().find(|(at, _)| *at == line).map(|(_, face)| *face);
            got.push((line, level(stars.modifiers), title, newline));
        }
        let expected: Vec<Faces> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let at = |index: usize| row[index].as_u64().map(|value| value as usize);
                (at(0).unwrap(), at(1), at(2), at(3))
            })
            .collect();
        // Emacs's title is the face of the first character after the
        // stars, perhaps a TODO keyword's, which has no level here.
        let matches = got.len() == expected.len()
            && got.iter().zip(&expected).all(|(got, emacs)| {
                got.0 == emacs.0 && got.1 == emacs.1 && got.3 == emacs.3 && (got.2.is_none() || got.2 == emacs.2)
            });
        if !matches {
            let options = json!({ "n": case.get("n"), "cycle": case.get("cycle"), "odd": case.get("odd"), "whole": case.get("whole") });
            failures.push(format!("{text:?} {options}:\n  got   {got:?}\n  Emacs {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
