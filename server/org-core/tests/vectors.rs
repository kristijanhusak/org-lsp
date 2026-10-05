//! The date helpers against values computed by Org itself.
//!
//! `tests/vectors/*.tsv` are written by `scripts/gen-vectors.el`. Each row
//! holds an input and what the Org function returned for it.

use std::path::Path;

use org_core::timestr::{
    closest_date, parse_time_string, plain_time_range, stamp_time_range, time_of_day, time_string_to_day, warning_days,
    Prefer,
};

fn rows(name: &str) -> Vec<Vec<String>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let rows: Vec<Vec<String>> = text.lines().map(|line| line.split('\t').map(str::to_string).collect()).collect();
    assert!(!rows.is_empty());
    rows
}

/// Report every row that differs, not just the first.
fn check(name: &str, mut actual: impl FnMut(&[String]) -> String) {
    let mut failures = Vec::new();
    for row in rows(name) {
        let (inputs, expected) = row.split_at(row.len() - expected_columns(name));
        let got = actual(inputs);
        let expected = expected.join("\t");
        if got != expected {
            failures.push(format!("{inputs:?}\n  expected {expected}\n  actual   {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} rows differ in {name}:\n{}",
        failures.len(),
        failures[..failures.len().min(25)].join("\n")
    );
}

fn expected_columns(name: &str) -> usize {
    match name {
        "to-day.tsv" => 2,
        "times.tsv" => 3,
        _ => 1,
    }
}

/// Render an optional string the way `%S` prints it.
fn lisp_string(text: Option<&str>) -> String {
    text.map_or("nil".to_string(), |text| format!("\"{text}\""))
}

#[test]
fn closest_date_matches_org() {
    check("closest-date.tsv", |row| {
        let prefer = match row[2].as_str() {
            "past" => Prefer::Past,
            "future" => Prefer::Future,
            _ => Prefer::Nearest,
        };
        closest_date(&row[0], row[1].parse().unwrap(), prefer).unwrap().to_string()
    });
}

#[test]
fn time_string_to_day_matches_org() {
    check("to-day.tsv", |row| {
        let day = time_string_to_day(&row[0]).unwrap();
        let time = match parse_time_string(&row[0]).unwrap().time {
            Some((hour, minute)) => format!("({hour} {minute})"),
            None => "(nil nil)".to_string(),
        };
        format!("{day}\t{time}")
    });
}

#[test]
fn warning_days_match_org() {
    check("wdays.tsv", |row| warning_days(&row[0], row[1].parse().unwrap(), row[2] == "1", row[3] == "1").to_string());
}

#[test]
fn times_of_day_match_org() {
    check("times.tsv", |row| {
        // The input is wrapped in `|` so that blanks survive.
        let text = &row[0][1..row[0].len() - 1];
        // Emacs reports character offsets.
        let chars = |byte: usize| text[..byte].chars().count();
        let military = time_of_day(text).map_or("nil".to_string(), |time| time.military().to_string());
        let plain = plain_time_range(text).map_or("nil".to_string(), |(first, second)| {
            let end = second.map_or(first.end, |second| second.end);
            format!(
                "({} {} {} {})",
                chars(first.start),
                chars(end),
                lisp_string(Some(&text[first.start..first.end])),
                lisp_string(second.map(|second| &text[second.start..second.end])),
            )
        });
        let stamp = stamp_time_range(text).map_or("nil".to_string(), |stamp| {
            format!(
                "({} {} {} {})",
                chars(stamp.matched.0),
                chars(stamp.matched.1),
                lisp_string(Some(&text[stamp.start.0..stamp.start.1])),
                lisp_string(stamp.end.map(|(start, end)| &text[start..end])),
            )
        });
        format!("{military}\t{plain}\t{stamp}")
    });
}
