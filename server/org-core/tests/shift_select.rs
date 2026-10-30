//! Where Org's shifted cursor keys act and where they select, with each
//! value of `org-support-shift-select`, against Emacs.
//!
//! `tests/shift-select/cases.json` is written by
//! `scripts/shift_select_reference.py`: a text and, for each value and
//! command, a letter for each position: `o` where Org acts, `s` where the
//! key selects, `e` where Org says it does not apply.

use std::path::Path;

use org_core::edit::{shift_acts, shift_control_acts, Direction, ShiftSelect};
use serde_json::Value;

#[test]
fn shift_keys_select_where_emacs_selects() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/shift-select/cases.json"));
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        for (value, select) in
            [("nil", ShiftSelect::Never), ("t", ShiftSelect::Outside), ("always", ShiftSelect::Always)]
        {
            let commands = [
                ("org-shiftup", Some(Direction::Up)),
                ("org-shiftdown", Some(Direction::Down)),
                ("org-shiftleft", Some(Direction::Left)),
                ("org-shiftright", Some(Direction::Right)),
                ("org-shiftcontrolright", None),
            ];
            for (command, direction) in commands {
                let expected = case["expected"][value][command].as_str().unwrap();
                for (point, letter) in expected.chars().enumerate() {
                    let acts = match direction {
                        Some(direction) => shift_acts(text, point, direction, select, true),
                        None => shift_control_acts(text, point, select),
                    };
                    let actual = match (acts, select) {
                        (true, _) => 'o',
                        (false, ShiftSelect::Never) => 'e',
                        (false, _) => 's',
                    };
                    if actual != letter {
                        failures.push(format!(
                            "{value} {command} at {point} ({:?}): {actual}, Emacs {letter}",
                            &text[point..text.len().min(point + 8)]
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}
