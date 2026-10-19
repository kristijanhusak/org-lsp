//! `org-duration-from-minutes` and `org-duration-to-minutes`, as Emacs
//! answered for the formats, units, numbers and texts of
//! `tests/duration/cases.json` (`scripts/duration_reference.py`).

use org_core::duration::{from_minutes_with, to_minutes_with, Format, Special, Unit};
use regex::Regex;
use serde_json::Value;

/// `(("min" . 1) ("h" . 60) ...)`
fn units(lisp: &str) -> Vec<Unit> {
    let pair = Regex::new(r#"\("([^"]+)" \. ([0-9.]+)\)"#).unwrap();
    pair.captures_iter(lisp)
        .map(|found| Unit {
            name: found[1].to_string(),
            minutes: found[2].parse().unwrap(),
            float: found[2].contains('.'),
        })
        .collect()
}

/// `h:mm`, `h:mm:ss` or a list of `("d" . nil)`, `(special . …)` and `compact`.
fn format(lisp: &str) -> Format {
    match lisp {
        "h:mm" => return Format::HMm,
        "h:mm:ss" => return Format::HMmSs,
        _ => {}
    }
    let unit = Regex::new(r#"\("([^"]+)" \. (nil|t)\)"#).unwrap();
    let special = Regex::new(r"\(special \. ([^)]+)\)").unwrap();
    Format::Units {
        units: unit.captures_iter(lisp).map(|found| (found[1].to_string(), &found[2] == "t")).collect(),
        special: special.captures(lisp).map(|found| match &found[1] {
            "h:mm" => Special::HMm,
            "h:mm:ss" => Special::HMmSs,
            digits => Special::Precision(digits.parse().unwrap()),
        }),
        compact: lisp.contains(" compact"),
    }
}

#[test]
fn durations_as_emacs_writes_and_reads_them() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/duration/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let units = units(case["units"].as_str().unwrap());
        let format = format(case["format"].as_str().unwrap());
        for (minutes, expected) in case["minutes"].as_array().unwrap().iter().zip(case["written"].as_array().unwrap()) {
            let written = from_minutes_with(minutes.as_f64().unwrap(), &format, &units, false);
            if written != expected.as_str().unwrap() {
                failures.push(format!(
                    "{} {}: {minutes} written {written:?}, Emacs {expected}",
                    case["units"], case["format"]
                ));
            }
        }
        for (text, expected) in case["texts"].as_array().unwrap().iter().zip(case["read"].as_array().unwrap()) {
            let read = to_minutes_with(text.as_str().unwrap(), &units, false);
            let same = match (read, expected.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                (None, None) => true,
                _ => false,
            };
            if !same {
                failures.push(format!("{}: {text} read {read:?}, Emacs {expected}", case["units"]));
            }
        }
    }
    assert!(failures.is_empty(), "{} differ:\n{}", failures.len(), failures.join("\n"));
}
