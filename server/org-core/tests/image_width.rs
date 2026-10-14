//! `org-display-inline-image--width`: how wide each link's image shows,
//! as Emacs answered for the cases of `tests/link/image-width.json`
//! (`scripts/image_width_reference.py`), the window 1000 pixels wide.

use org_core::link_preview::{image_width_in, ActualWidth, ImageWidth, Number};
use serde_json::Value;

fn setting(value: &Value) -> ActualWidth {
    let number = |value: &Value| match value.as_i64() {
        Some(integer) => Number::Integer(integer),
        None => Number::Float(value.as_f64().expect("a number")),
    };
    match value {
        Value::Bool(true) => ActualWidth::Actual,
        Value::Bool(false) | Value::Null => ActualWidth::Attributes(None),
        Value::Array(items) => ActualWidth::Attributes(items.first().map(number)),
        value => ActualWidth::Fixed(number(value)),
    }
}

#[test]
fn widths_as_emacs_has_them() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/link/image-width.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let parse = org_syntax::parse(text, &Default::default());
        let outline = org_core::outline::Outline::new(&parse, text, "test.org");
        let links: Vec<usize> = parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == org_syntax::SyntaxKind::LINK)
            .map(|node| usize::from(node.text_range().start()))
            .collect();
        let found: Vec<Value> = links
            .iter()
            .map(|start| match image_width_in(&outline, &parse, *start, setting(&case["setting"])) {
                None => Value::Null,
                Some(ImageWidth::Pixels(pixels)) => serde_json::json!(pixels),
                Some(ImageWidth::Fraction(part)) => serde_json::json!((part * 1000.0).round()),
            })
            .collect();
        let expected: Vec<Value> = case["widths"].as_array().unwrap().clone();
        let same = found.len() == expected.len()
            && found.iter().zip(&expected).all(|(a, b)| match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => a == b,
            });
        if !same {
            failures.push(format!("{text:?} with {}: {found:?}, Emacs {expected:?}", case["setting"]));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
