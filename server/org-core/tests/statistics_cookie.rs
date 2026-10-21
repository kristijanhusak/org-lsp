//! `C-c C-c` (`org-ctrl-c-ctrl-c`) on the statistics cookie of a headline:
//! `org-update-statistics-cookies`, the cookies counted again from the
//! entry's checkboxes or its children's TODO keywords, the boxes left as
//! they are. The expected texts are Emacs's (Org 9.8.10, batch, the
//! cursor on the cookie's last character).

use org_core::date::{Date, DateTime};
use org_core::edit;
use org_syntax::TodoKeywords;

const CASES: &[(&str, &str, &str)] = &[
    (
        "* Top\n** TODO Groceries [10%]\n- [X] milk\n- [ ] bread\n- [ ] 卵\n** Next\n",
        "[10%",
        "* Top\n** TODO Groceries [33%]\n- [X] milk\n- [ ] bread\n- [ ] 卵\n** Next\n",
    ),
    (
        "* Top\n** TODO Project Alpha [0/3] :work:\n*** DONE One\n*** TODO Two\n*** TODO Three\n* Other\n",
        "[0/3",
        "* Top\n** TODO Project Alpha [1/3]                                            :work:\n*** DONE One\n*** TODO Two\n*** TODO Three\n* Other\n",
    ),
    ("* Leaf [3/5] and [50%]\nText\n* After [/]\n", "[3/5", "* Leaf [0/0] and [100%]\nText\n* After [/]\n"),
    (
        "* Mixed [1/1]\n- [ ] a\n:PROPERTIES:\n:COOKIE_DATA: todo\n:END:\n** DONE x\n",
        "[1/1",
        "* Mixed [1/1]\n- [ ] a\n:PROPERTIES:\n:COOKIE_DATA: todo\n:END:\n** DONE x\n",
    ),
    ("* Box [/]\n- [X] a\n** TODO child\n", "[/", "* Box [1/1]\n- [X] a\n** TODO child\n"),
];

#[test]
fn counts_the_cookies_of_a_headline_again() {
    let now = DateTime::new(Date::new(2026, 10, 21), 10, 30);
    let call = edit::Call { count: None, repeated: false, now, note: None, skip_note: true, time_only: false };
    let settings = edit::Settings::default();
    for (text, search, expected) in CASES {
        let offset = text.find(search).unwrap() + search.len() - 1;
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let done = edit::run(&edit::Command::Checkbox, text, &parse, offset, &call, &settings).unwrap();
        assert_eq!(done.text, *expected, "in {text:?}");
    }
}
