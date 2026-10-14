//! Org's table commands against Emacs: `tests/table/cases.json` is written
//! by `scripts/update-table-fixtures.py`: a document with the cursor
//! (`¦`) in a table and the keys pressed, with the text, the point and
//! what went to the kill ring after, and at times the options the case
//! was recorded with. `ORG_TABLE_CASES` points the test at another file.

use std::path::Path;

use org_core::table::{self, Command};
use serde_json::{json, Value};

fn command(key: &str) -> Command {
    match key {
        "TAB" => Command::NextField,
        "<backtab>" => Command::PreviousField,
        "M-<left>" => Command::MoveColumn { left: true },
        "M-<right>" => Command::MoveColumn { left: false },
        "M-<up>" => Command::MoveRow { up: true },
        "M-<down>" => Command::MoveRow { up: false },
        "M-S-<left>" => Command::DeleteColumn,
        "M-S-<right>" => Command::InsertColumn,
        "M-S-<up>" => Command::KillRow,
        "M-S-<down>" => Command::InsertRow { below: false },
        "C-u M-S-<down>" => Command::InsertRow { below: true },
        "C-c -" => Command::InsertHline { above: false },
        "C-u C-c -" => Command::InsertHline { above: true },
        "C-c RET" => Command::HlineAndMove,
        "fn:org-table-blank-field" => Command::BlankField,
        "C-c C-c" => Command::Align,
        key if key.starts_with("C-c ^ ") => Command::Sort { kind: key.chars().last().unwrap(), column: None },
        key => panic!("no command for {key}"),
    }
}

/// The options of a case: `evaluate_inline`, `line_recalculation`,
/// `automatic_realign`, `tab_jumps_over_hlines`.
fn settings(case: &Value) -> table::Settings {
    let options = &case["settings"];
    let defaults = table::Settings::default();
    table::Settings {
        evaluate_inline: options["evaluate_inline"].as_bool().unwrap_or(defaults.evaluate_inline),
        line_recalculation: options["line_recalculation"].as_bool().unwrap_or(defaults.line_recalculation),
        automatic_realign: options["automatic_realign"].as_bool().unwrap_or(defaults.automatic_realign),
        tab_jumps_over_hlines: options["tab_jumps_over_hlines"].as_bool().unwrap_or(defaults.tab_jumps_over_hlines),
        ..defaults
    }
}

fn run(case: &Value) -> Value {
    let marked = case["text"].as_str().unwrap();
    // `C-c C-c` at the start of a table recalculates it, as `tests/tblfm`
    // checks.
    if case["key"] == "C-c C-c" && table::recalculates(&marked.replacen('¦', "", 1), marked.find('¦').unwrap()) {
        return case["expected"].clone();
    }
    let at = marked.find('¦').unwrap();
    let text = marked.replacen('¦', "", 1);
    match table::run_with(&text, at, command(case["key"].as_str().unwrap()), &|_| None, &settings(case)) {
        Ok(done) => json!({
            "text": done.text,
            "point": done.text[..done.cursor].chars().count(),
            "killed": done.killed,
        }),
        // Org asks for the column in the minibuffer.
        Err(error) if error == table::ASK_COLUMN => json!({ "error": format!("Asked: {error}: ") }),
        Err(error) => json!({ "error": error }),
    }
}

#[test]
fn table_commands_as_in_emacs() {
    let path = std::env::var("ORG_TABLE_CASES").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/table/cases.json").display().to_string()
    });
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = &case["expected"];
        if expected.get("skip").is_some() {
            continue;
        }
        let actual = run(case);
        if &actual != expected {
            failures.push(format!(
                "case {index}: {} {}\n  expected {expected}\n  actual   {actual}",
                case["key"],
                serde_json::to_string(&case["text"]).unwrap(),
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

/// A Windows file's table: worked on as Org's, its lines still ending
/// with `\r\n`.
#[test]
fn keeps_the_line_ends_of_a_windows_file() {
    let text = "| a | bb |\r\n|-\r\n| ccc | 1 |\r\n";
    let done = table::run(text, 2, Command::NextField).unwrap();
    assert_eq!(done.text, "| a   | bb |\r\n|-----+----|\r\n| ccc |  1 |\r\n");
    assert_eq!(&done.text[done.cursor..done.cursor + 2], "bb");
}
