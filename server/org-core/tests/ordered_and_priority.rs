//! `org-track-ordered-property-with-tag` and
//! `org-priority-enable-commands`, against what Org 9.8.10 makes of the
//! same text.

use org_core::date::{Date, DateTime};
use org_core::edit;
use org_core::priority::Action;
use org_syntax::TodoKeywords;

fn run(command: &edit::Command, text: &str, at: usize, settings: &edit::Settings) -> Result<String, String> {
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
    let call = edit::Call { count: None, repeated: false, now, note: None, skip_note: false, time_only: false };
    edit::run(command, text, &parse, at, &call, settings)
        .map(|outcome| outcome.text)
        .map_err(|refusal| refusal.message().to_string())
}

fn tracked(tag: &str) -> edit::Settings {
    edit::Settings { ordered_tag: Some(tag.to_string()), ..edit::Settings::default() }
}

#[test]
fn the_ordered_tag_follows_the_property() {
    let toggle = |text: &str, at: usize, settings: &edit::Settings| run(&edit::Command::Ordered, text, at, settings);
    let ordered = tracked("ORDERED");
    let pad = |headline: &str, tags: &str| format!("{headline}{}{tags}", " ".repeat(77 - headline.len() - tags.len()));
    let drawer = ":PROPERTIES:\n:ORDERED:  t\n:END:\n";
    assert_eq!(
        toggle("* Task\nbody\n", 2, &ordered).unwrap(),
        format!("{}\n{drawer}body\n", pad("* Task", ":ORDERED:"))
    );
    assert_eq!(
        toggle("* Task :work:\nbody\n", 2, &ordered).unwrap(),
        format!("{}\n{drawer}body\n", pad("* Task", ":work:ORDERED:"))
    );
    assert_eq!(
        toggle(&format!("* Task :work:ORDERED:\n{drawer}body\n"), 2, &ordered).unwrap(),
        format!("{}\nbody\n", pad("* Task", ":work:"))
    );
    assert_eq!(
        toggle(&format!("* Task :ORDERED:work:\n{drawer}"), 2, &ordered).unwrap(),
        format!("{}\n", pad("* Task", ":work:"))
    );
    assert_eq!(toggle(&format!("* Task\n{drawer}"), 2, &ordered).unwrap(), "* Task\n");
    // A tag that is there already stays where it is.
    assert_eq!(
        toggle("* Task :ORDERED:\nbody\n", 2, &ordered).unwrap(),
        format!("{}\n{drawer}body\n", pad("* Task", ":ORDERED:"))
    );
    // A tag of the user's, sorted as `org-tags-sort-function` says.
    let mut sorted = tracked("seq");
    sorted.tags.sort = Some(org_core::tags::Sort::Ascending);
    assert_eq!(
        toggle("* Task :zz:\nbody\n", 9, &sorted).unwrap(),
        format!("{}\n{drawer}body\n", pad("* Task", ":seq:zz:"))
    );
    // `org-tags-column` 0: one space before the tags.
    let mut column = tracked("ORDERED");
    column.todo.tags_column = 0;
    assert_eq!(toggle("* Task\nbody\n", 2, &column).unwrap(), format!("* Task :ORDERED:\n{drawer}body\n"));
    // On the entry the cursor is in.
    assert_eq!(
        toggle("* Task\n** Sub\n", 13, &tracked("seq")).unwrap(),
        format!("* Task\n{}\n{drawer}", pad("** Sub", ":seq:"))
    );
    // Without the option, no tag.
    assert_eq!(toggle("* Task\n", 2, &edit::Settings::default()).unwrap(), format!("* Task\n{drawer}"));
}

#[test]
fn priority_commands_refuse_when_disabled() {
    let disabled = edit::Settings { priority_commands: false, ..edit::Settings::default() };
    let text = "* TODO a\n";
    for command in [
        edit::Command::Priority(Action::Up),
        edit::Command::Priority(Action::Set(i64::from(b'A'))),
        edit::Command::Property { name: "PRIORITY".to_string(), value: "A".to_string() },
    ] {
        assert_eq!(run(&command, text, 2, &disabled).unwrap_err(), "Priority commands are disabled");
    }
    // `S-<up>` on a headline is not for priorities then.
    assert!(run(&edit::Command::Shift(edit::Direction::Up), text, 2, &disabled).is_err());
    assert_eq!(
        run(&edit::Command::Shift(edit::Direction::Up), text, 2, &edit::Settings::default()).unwrap(),
        "* TODO [#B] a\n"
    );
}
