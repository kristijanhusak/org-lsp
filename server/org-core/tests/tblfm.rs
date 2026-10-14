//! Table formulas against Emacs.
//!
//! `tests/tblfm/cases.json` holds Org files with a table and what Emacs's
//! `org-table-recalculate` made of them (`scripts/update-tblfm-fixtures.py`).
//! `ORG_TBLFM_CASES` points the test at other cases.

use std::path::Path;

use org_core::tblfm::{self, Mode};
use serde_json::{json, Value};

/// The options of a case, as `tblfm::Settings`: `constants`,
/// `cross_hline` (`true`, `false`, `"error"`), `create_columns`,
/// `duration_format`, `padding` and `float_format` (`["fix", 2]`) and
/// `angle_mode`, which its `emacs` bindings give Emacs.
fn settings(case: &Value) -> tblfm::Settings {
    use org_core::calc::FloatFormat;
    use tblfm::{CrossHline, DurationFormat};
    let options = &case["settings"];
    let mut settings = tblfm::Settings::default();
    if let Some(constants) = options["constants"].as_object() {
        settings.constants =
            constants.iter().map(|(name, value)| (name.clone(), value.as_str().unwrap().to_string())).collect();
    }
    settings.cross_hline = match &options["cross_hline"] {
        Value::Bool(false) => CrossHline::Stop,
        Value::String(word) if word == "error" => CrossHline::Error,
        _ => CrossHline::Cross,
    };
    settings.create_columns = options["create_columns"].as_bool().unwrap_or(false);
    settings.duration_format = match options["duration_format"].as_str() {
        Some("seconds") => DurationFormat::Seconds,
        Some("minutes") => DurationFormat::Minutes,
        Some("days") => DurationFormat::Days,
        Some("hh:mm") => DurationFormat::HoursMinutes,
        _ => DurationFormat::Hours,
    };
    settings.hour_zero_padding = options["padding"].as_bool().unwrap_or(true);
    if let Some([kind, digits]) = options["float_format"].as_array().map(Vec::as_slice) {
        let digits = digits.as_u64().unwrap() as usize;
        settings.calc.float_format = match kind.as_str().unwrap() {
            "fix" => FloatFormat::Fix(digits),
            "sci" => FloatFormat::Sci(digits),
            _ => FloatFormat::Float(digits),
        };
    }
    if let Some(angle) = options["angle_mode"].as_str() {
        settings.calc.radians = angle == "rad";
    }
    settings
}

/// Emacs's curved quotes in messages, as the server writes them.
fn plain(message: &str) -> String {
    message.replace(['‘', '’'], "'").replace('`', "'")
}

#[test]
fn recalculates_as_emacs_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/tblfm/cases.json");
    let path = std::env::var("ORG_TBLFM_CASES").map_or(path, Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let marked = case["text"].as_str().unwrap();
        let offset = marked.find('¦').or_else(|| marked.find('|')).unwrap();
        let text = marked.replacen('¦', "", 1);
        let mode = match case["command"].as_str() {
            Some("line") => Mode::Line,
            Some("iterate") => Mode::Iterate,
            _ => Mode::All,
        };
        let actual = match tblfm::recalculate(&text, offset, mode, &|_| None, &settings(case)) {
            Ok(outcome) => json!({ "text": outcome.text }),
            Err(error) => json!({ "error": plain(&error) }),
        };
        let mut expected = case["expected"].clone();
        if let Some(error) = expected["error"].as_str() {
            expected = json!({ "error": plain(error) });
        }
        if actual != expected {
            failures.push(format!(
                "\n{}:\nexpected: {}\nactual:   {}",
                case["name"].as_str().unwrap_or("?"),
                serde_json::to_string_pretty(&expected).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap()
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:{}", failures.len(), cases.len(), failures.join("\n"));
}

/// Formulas that once held the server for ever, or asked for gigabytes.
#[test]
fn stops_on_formulas_that_never_end() {
    let recalculate = |text: &str| tblfm::recalculate(text, 0, Mode::All, &|_| None, &Default::default());
    let error = recalculate("| 1 |\n| 2 |\n#+TBLFM: @2$1=@+999999999999$1\n").unwrap_err();
    assert!(error.contains("leads outside table"), "{error}");
    assert!(recalculate("| @2$1 |\n| @1$1 |\n| x |\n#+TBLFM: @3$1=@1$1+1\n").is_err());
    let wide = recalculate("| 1 |\n| 2 |\n#+TBLFM: @2$1=@1$1;%9999999999d\n").unwrap().text;
    assert!(wide.len() < 10_000, "{} bytes", wide.len());
    let precise = recalculate("| 1 |\n| 2 |\n#+TBLFM: @2$1=@1$1/3;f999999999\n").unwrap().text;
    assert!(precise.len() < 10_000, "{} bytes", precise.len());
}
