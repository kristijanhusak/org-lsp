//! Faces for particular TODO keywords, priorities and tags
//! (`org-todo-keyword-faces`, `org-priority-faces`, `org-tag-faces`), for
//! editors to apply over the server's highlighting.
//!
//! A face is written as Org writes it: a color, which is the foreground
//! (`org-faces-easy-properties`), or a property list such as
//! `:foreground blue :weight bold :underline t`. Editors get the
//! attributes, `{ "foreground", "background", "bold", "italic",
//! "underline", "strikethrough" }`, those that are set.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The attributes of one face, as JSON, or `None` when it sets nothing.
pub fn face(spec: &str) -> Option<Value> {
    let spec = spec.trim();
    let mut out = Map::new();
    if !spec.starts_with(':') {
        let color = spec.trim_matches('"');
        if color.is_empty() {
            return None;
        }
        out.insert("foreground".to_string(), json!(color));
        return Some(Value::Object(out));
    }
    let words: Vec<&str> = spec.split_whitespace().collect();
    let mut index = 0;
    while index < words.len() {
        let key = words[index];
        let value = words.get(index + 1).map(|value| value.trim_matches('"')).unwrap_or("");
        index += 2;
        let on = !matches!(value, "nil" | "normal" | "");
        match key {
            ":foreground" | ":fg" => {
                out.insert("foreground".to_string(), json!(value));
            }
            ":background" | ":bg" => {
                out.insert("background".to_string(), json!(value));
            }
            ":weight" => {
                out.insert(
                    "bold".to_string(),
                    json!(matches!(value, "bold" | "extra-bold" | "ultra-bold" | "heavy" | "black" | "semi-bold")),
                );
            }
            ":slant" => {
                out.insert("italic".to_string(), json!(matches!(value, "italic" | "oblique")));
            }
            ":underline" => {
                out.insert("underline".to_string(), json!(on));
            }
            ":strike-through" => {
                out.insert("strikethrough".to_string(), json!(on));
            }
            // An attribute editors cannot show; its value is skipped.
            _ if key.starts_with(':') => {}
            // A word that is no attribute: read on from the next one.
            _ => index -= 1,
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

/// The faces of the TODO keywords, of the priorities and of the tags,
/// `{ "todo": { KEYWORD: face }, "priority": { "A": face }, "tag": { TAG:
/// face } }`.
pub fn faces(
    todo: &BTreeMap<String, String>,
    priority: &BTreeMap<String, String>,
    tag: &BTreeMap<String, String>,
) -> Value {
    let read = |faces: &BTreeMap<String, String>| -> Map<String, Value> {
        faces.iter().filter_map(|(name, spec)| Some((name.clone(), face(spec)?))).collect()
    };
    json!({ "todo": read(todo), "priority": read(priority), "tag": read(tag) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_colors_and_property_lists() {
        assert_eq!(face("orange"), Some(json!({ "foreground": "orange" })));
        assert_eq!(face("\"#ff8800\""), Some(json!({ "foreground": "#ff8800" })));
        assert_eq!(
            face(":foreground blue :background \"white\" :weight bold :slant italic :underline t"),
            Some(
                json!({ "foreground": "blue", "background": "white", "bold": true, "italic": true, "underline": true })
            )
        );
        assert_eq!(
            face(":weight normal :box t :strike-through t"),
            Some(json!({ "bold": false, "strikethrough": true }))
        );
        assert_eq!(face(""), None);
        assert_eq!(face(":height 1.2"), None);
    }
}
