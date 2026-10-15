//! `org-num-mode`: the numbers before headlines, as Emacs shows them for
//! the cases of `tests/num/cases.json` (`scripts/num_reference.py`).

use org_core::num::{numbers, Settings};
use org_core::outline::Outline;
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

#[test]
fn numbers_headlines_as_emacs_does() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/num/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let options = &case["options"];
        let flag = |name: &str| options[name].as_bool().unwrap_or(false);
        let settings = Settings {
            max_level: options["org-num-max-level"].as_u64().map(|level| level as usize),
            skip_commented: flag("org-num-skip-commented"),
            skip_footnotes: flag("org-num-skip-footnotes"),
            skip_tags: options["org-num-skip-tags"]
                .as_array()
                .map(|tags| tags.iter().map(|tag| tag.as_str().unwrap().to_string()).collect())
                .unwrap_or_default(),
            skip_unnumbered: flag("org-num-skip-unnumbered"),
        };
        let keywords = TodoKeywords::default();
        let parse = org_syntax::parse(text, &keywords);
        let outline = Outline::new(&parse, text, "num");
        let got: Vec<Value> = numbers(text, &outline, &keywords, &settings)
            .into_iter()
            .map(|number| {
                let line = text[..number.at].matches('\n').count();
                let column = number.at - text[..number.at].rfind('\n').map_or(0, |at| at + 1);
                json!([line, column, number.shown])
            })
            .collect();
        if Value::from(got.clone()) != case["expected"] {
            failures.push(format!("{text:?} {options}:\n  got   {}\n  Emacs {}", Value::from(got), case["expected"]));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
