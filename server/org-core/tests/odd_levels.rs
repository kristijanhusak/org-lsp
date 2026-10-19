//! `org-odd-levels-only` against what Org 9.8.10 says: the valid levels
//! (`org-get-valid-level`), and `LEVEL` in a match, the reduced level.

use org_core::agenda::matcher::Clock;
use org_core::agenda::Matcher;
use org_core::date::Date;
use org_core::outline::Outline;
use org_core::structure::{odd_levels, valid_level};
use org_syntax::TodoKeywords;

#[test]
fn valid_levels_are_odd() {
    // Level, change, and `org-get-valid-level` in Emacs.
    let emacs = "1 -2 1;1 -1 1;1 0 1;1 1 3;1 2 5;2 -2 1;2 -1 1;2 0 3;2 1 3;2 2 5;3 -2 1;3 -1 1;3 0 3;3 1 5;3 2 7;\
                 4 -2 1;4 -1 3;4 0 5;4 1 5;4 2 7;5 -2 1;5 -1 3;5 0 5;5 1 7;5 2 9;6 -2 3;6 -1 5;6 0 7;6 1 7;6 2 9;\
                 7 -2 3;7 -1 5;7 0 7;7 1 9;7 2 11";
    for case in emacs.split(';') {
        let numbers: Vec<isize> = case.split(' ').map(|number| number.parse().unwrap()).collect();
        let (level, change, valid) = (numbers[0] as usize, numbers[1], numbers[2] as usize);
        assert_eq!(valid_level(level, change, true), valid, "{case}");
        assert_eq!(valid_level(level, change, false), (level as isize + change).max(1) as usize, "{case}");
    }
}

#[test]
fn startup_words_turn_it_on_and_off() {
    assert!(odd_levels("#+STARTUP: odd\n", false));
    assert!(!odd_levels("#+STARTUP: odd oddeven\n", true));
    assert!(odd_levels("* A\n", true));
}

#[test]
fn level_matches_the_reduced_level() {
    let text = "#+STARTUP: odd\n* A\n*** B\n***** C\n** even\n";
    let parse = org_syntax::parse(text, &TodoKeywords::default());
    let outline = Outline::new(&parse, text, "odd");
    let clock = Clock { today: Date::new(2026, 5, 18).to_day(), minute: 630 };
    let titles = |matching: &str| -> Vec<String> {
        let matcher = Matcher::new(matching);
        (0..outline.entries.len())
            .filter(|index| matcher.matches(&outline, *index, false, clock))
            .map(|index| outline.entries[index].head.clone())
            .collect()
    };
    // As `org-map-entries` finds them in Emacs.
    assert_eq!(titles("LEVEL=2"), ["B", "even"]);
    assert_eq!(titles("LEVEL=3"), ["C"]);
}
