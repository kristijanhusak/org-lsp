//! Snapshots of the token-level tree shape. Node ranges are checked against
//! Emacs in `org_element.rs`; these pin down how lines split into tokens,
//! which the server relies on for highlighting and edits.
//!
//! Update with `UPDATE_EXPECT=1 cargo test -p org-syntax --test tree`.

use expect_test::{expect, Expect};
use org_syntax::{parse, TodoKeywords};

fn check(text: &str, expected: Expect) {
    let root = parse(text, &TodoKeywords::default()).syntax();
    assert_eq!(root.text().to_string(), text);
    expected.assert_eq(&format!("{root:#?}"));
}

#[test]
fn headline_line() {
    check(
        "** TODO [#A] Write *the* parser  :work:rust:\n",
        expect![[r#"
            DOCUMENT@0..45
              HEADLINE@0..45
                STARS@0..2 "**"
                WHITESPACE@2..3 " "
                TODO_KEYWORD@3..7 "TODO"
                WHITESPACE@7..8 " "
                PRIORITY@8..12 "[#A]"
                WHITESPACE@12..13 " "
                TITLE@13..31
                  TEXT@13..19 "Write "
                  BOLD@19..24
                    MARKER@19..20 "*"
                    TEXT@20..23 "the"
                    MARKER@23..24 "*"
                  TEXT@24..31 " parser"
                WHITESPACE@31..33 "  "
                TAGS@33..44
                  COLON@33..34 ":"
                  TAG@34..38 "work"
                  COLON@38..39 ":"
                  TAG@39..43 "rust"
                  COLON@43..44 ":"
                NEWLINE@44..45 "\n"
        "#]],
    );
}

#[test]
fn outline_nesting() {
    check(
        "intro\n* a\n** b\n* c\n",
        expect![[r#"
            DOCUMENT@0..19
              SECTION@0..6
                PARAGRAPH@0..6
                  TEXT@0..5 "intro"
                  NEWLINE@5..6 "\n"
              HEADLINE@6..15
                STARS@6..7 "*"
                WHITESPACE@7..8 " "
                TITLE@8..9
                  TEXT@8..9 "a"
                NEWLINE@9..10 "\n"
                HEADLINE@10..15
                  STARS@10..12 "**"
                  WHITESPACE@12..13 " "
                  TITLE@13..14
                    TEXT@13..14 "b"
                  NEWLINE@14..15 "\n"
              HEADLINE@15..19
                STARS@15..16 "*"
                WHITESPACE@16..17 " "
                TITLE@17..18
                  TEXT@17..18 "c"
                NEWLINE@18..19 "\n"
        "#]],
    );
}

#[test]
fn planning_and_property_drawer() {
    check(
        "* h\nSCHEDULED: <2026-01-01 Thu> CLOSED: [2026-01-02 Fri]\n:PROPERTIES:\n:ID: abc\n:END:\n",
        expect![[r#"
            DOCUMENT@0..85
              HEADLINE@0..85
                STARS@0..1 "*"
                WHITESPACE@1..2 " "
                TITLE@2..3
                  TEXT@2..3 "h"
                NEWLINE@3..4 "\n"
                SECTION@4..85
                  PLANNING@4..57
                    PLANNING_KEYWORD@4..14 "SCHEDULED:"
                    WHITESPACE@14..15 " "
                    TIMESTAMP@15..31
                      TIMESTAMP_TEXT@15..31 "<2026-01-01 Thu>"
                    TEXT@31..32 " "
                    PLANNING_KEYWORD@32..39 "CLOSED:"
                    WHITESPACE@39..40 " "
                    TIMESTAMP@40..56
                      TIMESTAMP_TEXT@40..56 "[2026-01-02 Fri]"
                    NEWLINE@56..57 "\n"
                  PROPERTY_DRAWER@57..85
                    DRAWER_BEGIN@57..69 ":PROPERTIES:"
                    NEWLINE@69..70 "\n"
                    NODE_PROPERTY@70..79
                      PROPERTY_KEY@70..74 ":ID:"
                      WHITESPACE@74..75 " "
                      PROPERTY_VALUE@75..78 "abc"
                      NEWLINE@78..79 "\n"
                    DRAWER_END@79..84 ":END:"
                    NEWLINE@84..85 "\n"
        "#]],
    );
}

#[test]
fn list_items() {
    check(
        "- [X] done\n  1. [@3] nested\n- term :: text\n",
        expect![[r#"
            DOCUMENT@0..43
              SECTION@0..43
                LIST@0..43
                  LIST_ITEM@0..28
                    BULLET@0..1 "-"
                    WHITESPACE@1..2 " "
                    CHECKBOX@2..5 "[X]"
                    WHITESPACE@5..6 " "
                    PARAGRAPH@6..11
                      TEXT@6..10 "done"
                      NEWLINE@10..11 "\n"
                    LIST@11..28
                      LIST_ITEM@11..28
                        WHITESPACE@11..13 "  "
                        BULLET@13..15 "1."
                        WHITESPACE@15..16 " "
                        COUNTER_SET@16..20 "[@3]"
                        WHITESPACE@20..21 " "
                        PARAGRAPH@21..28
                          TEXT@21..27 "nested"
                          NEWLINE@27..28 "\n"
                  LIST_ITEM@28..43
                    BULLET@28..29 "-"
                    WHITESPACE@29..30 " "
                    ITEM_TAG@30..34
                      TEXT@30..34 "term"
                    WHITESPACE@34..35 " "
                    TAG_SEPARATOR@35..37 "::"
                    WHITESPACE@37..38 " "
                    PARAGRAPH@38..43
                      TEXT@38..42 "text"
                      NEWLINE@42..43 "\n"
        "#]],
    );
}

#[test]
fn block_and_keyword() {
    check(
        "#+NAME: x\n#+BEGIN_SRC rust :tangle yes\nfn main() {}\n#+END_SRC\n#+TITLE: t\n",
        expect![[r##"
            DOCUMENT@0..73
              SECTION@0..73
                BLOCK@0..62
                  KEYWORD@0..10
                    KEYWORD_KEY@0..7 "#+NAME:"
                    WHITESPACE@7..8 " "
                    KEYWORD_VALUE@8..9 "x"
                    NEWLINE@9..10 "\n"
                  BLOCK_BEGIN@10..21 "#+BEGIN_SRC"
                  WHITESPACE@21..22 " "
                  BLOCK_PARAMETERS@22..38 "rust :tangle yes"
                  NEWLINE@38..39 "\n"
                  TEXT@39..51 "fn main() {}"
                  NEWLINE@51..52 "\n"
                  BLOCK_END@52..61 "#+END_SRC"
                  NEWLINE@61..62 "\n"
                KEYWORD@62..73
                  KEYWORD_KEY@62..70 "#+TITLE:"
                  WHITESPACE@70..71 " "
                  KEYWORD_VALUE@71..72 "t"
                  NEWLINE@72..73 "\n"
        "##]],
    );
}

#[test]
fn table() {
    check(
        "| a | [[l][d]] |\n|---+---|\n#+TBLFM: $1=1\n",
        expect![[r##"
            DOCUMENT@0..41
              SECTION@0..41
                TABLE@0..41
                  TABLE_ROW@0..17
                    PIPE@0..1 "|"
                    TABLE_CELL@1..4
                      WHITESPACE@1..2 " "
                      TEXT@2..3 "a"
                      WHITESPACE@3..4 " "
                    PIPE@4..5 "|"
                    TABLE_CELL@5..15
                      WHITESPACE@5..6 " "
                      LINK@6..14
                        LINK_OPEN@6..8 "[["
                        LINK_PATH@8..9 "l"
                        LINK_SEPARATOR@9..11 "]["
                        TEXT@11..12 "d"
                        LINK_CLOSE@12..14 "]]"
                      WHITESPACE@14..15 " "
                    PIPE@15..16 "|"
                    NEWLINE@16..17 "\n"
                  TABLE_ROW@17..27
                    TABLE_RULE@17..26 "|---+---|"
                    NEWLINE@26..27 "\n"
                  KEYWORD@27..41
                    KEYWORD_KEY@27..35 "#+TBLFM:"
                    WHITESPACE@35..36 " "
                    KEYWORD_VALUE@36..40 "$1=1"
                    NEWLINE@40..41 "\n"
        "##]],
    );
}

#[test]
fn links_and_friends() {
    check(
        "[[a][b]] <https://x.y> https://x.y [fn:1] [fn:n:def] [1/2] <<t>> x_1 \\alpha $m$\n",
        expect![[r#"
            DOCUMENT@0..80
              SECTION@0..80
                PARAGRAPH@0..80
                  LINK@0..8
                    LINK_OPEN@0..2 "[["
                    LINK_PATH@2..3 "a"
                    LINK_SEPARATOR@3..5 "]["
                    TEXT@5..6 "b"
                    LINK_CLOSE@6..8 "]]"
                  TEXT@8..9 " "
                  ANGLE_LINK@9..22
                    ANGLE_OPEN@9..10 "<"
                    LINK_PATH@10..21 "https://x.y"
                    ANGLE_CLOSE@21..22 ">"
                  TEXT@22..23 " "
                  PLAIN_LINK@23..34
                    LINK_PATH@23..34 "https://x.y"
                  TEXT@34..35 " "
                  FOOTNOTE_REFERENCE@35..41
                    FOOTNOTE_LABEL@35..41 "[fn:1]"
                  TEXT@41..42 " "
                  FOOTNOTE_REFERENCE@42..52
                    FOOTNOTE_OPEN@42..48 "[fn:n:"
                    TEXT@48..51 "def"
                    FOOTNOTE_CLOSE@51..52 "]"
                  TEXT@52..53 " "
                  STATISTICS_COOKIE@53..58
                    COOKIE_TEXT@53..58 "[1/2]"
                  TEXT@58..59 " "
                  TARGET@59..64
                    TARGET_TEXT@59..64 "<<t>>"
                  TEXT@64..66 " x"
                  SUBSCRIPT@66..68
                    MARKER@66..67 "_"
                    TEXT@67..68 "1"
                  TEXT@68..69 " "
                  ENTITY@69..75
                    TEXT@69..75 "\\alpha"
                  TEXT@75..76 " "
                  LATEX_FRAGMENT@76..79
                    TEXT@76..79 "$m$"
                  NEWLINE@79..80 "\n"
        "#]],
    );
}

#[test]
fn todo_keywords_from_document() {
    check(
        "#+TODO: WAIT | GONE\n* WAIT a\n* TODO b\n",
        expect![[r##"
            DOCUMENT@0..38
              SECTION@0..20
                KEYWORD@0..20
                  KEYWORD_KEY@0..7 "#+TODO:"
                  WHITESPACE@7..8 " "
                  KEYWORD_VALUE@8..19 "WAIT | GONE"
                  NEWLINE@19..20 "\n"
              HEADLINE@20..29
                STARS@20..21 "*"
                WHITESPACE@21..22 " "
                TODO_KEYWORD@22..26 "WAIT"
                WHITESPACE@26..27 " "
                TITLE@27..28
                  TEXT@27..28 "a"
                NEWLINE@28..29 "\n"
              HEADLINE@29..38
                STARS@29..30 "*"
                WHITESPACE@30..31 " "
                TITLE@31..37
                  TEXT@31..37 "TODO b"
                NEWLINE@37..38 "\n"
        "##]],
    );
}
