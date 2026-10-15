//! `org-entities-user`: the user's entities, in a test binary of their
//! own as they are the process's. What Org 9.8.10 makes of
//! `\myarrow{} and \alpha, \two, \my2 \myarrowx` with these entities:
//! `\myarrow{}` an entity shown as `→`, `\alpha` the user's `Ⓐ` rather
//! than Org's, `\two` an entity not shown composed (its character is two),
//! and neither `\my2` (digits) nor `\myarrowx` (another name) an entity.

use org_syntax::{SyntaxKind, TodoKeywords};

#[test]
fn user_entities_are_parsed_and_shown_as_org_does() {
    let entities = [("myarrow", "→"), ("alpha", "Ⓐ"), ("two", "22"), ("my2", "✗")];
    let owned: Vec<(String, String)> =
        entities.iter().map(|(name, utf8)| (name.to_string(), utf8.to_string())).collect();
    org_syntax::set_user_entities(&owned);
    let text = "\\myarrow{} and \\alpha, \\two, \\my2 \\myarrowx\n";
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    let found: Vec<(String, usize, usize)> = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::ENTITY)
        .map(|node| {
            let range = node.text_range();
            (node.text().to_string(), usize::from(range.start()) + 1, usize::from(range.end()) + 1)
        })
        .collect();
    // Emacs's positions, from 1.
    assert_eq!(
        found,
        [("\\myarrow{}".to_string(), 1, 11), ("\\alpha".to_string(), 16, 22), ("\\two".to_string(), 24, 28)]
    );
    assert_eq!(org_syntax::entity_utf8("alpha"), Some("Ⓐ"));
    let shown: Vec<(usize, usize, &str)> = org_core::pretty::entities(text);
    assert_eq!(shown, [(0, 10, "→"), (15, 21, "Ⓐ")]);
    let names = org_syntax::entity_names();
    assert!(names.contains(&"myarrow") && names.contains(&"beta"));
    assert_eq!(names.iter().filter(|name| **name == "alpha").count(), 1);
}
