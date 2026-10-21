//! The parser must accept any input without panicking and must never lose
//! or reorder a byte.

use std::path::Path;

use org_syntax::{parse, TodoKeywords};

fn assert_lossless(text: &str) {
    let root = parse(text, &TodoKeywords::default()).syntax();
    assert_eq!(root.text().to_string(), text);
}

fn fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut texts = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "org") {
            texts.push(std::fs::read_to_string(path).unwrap());
        }
    }
    assert!(!texts.is_empty());
    texts
}

/// A small deterministic generator, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

#[test]
fn empty_and_whitespace_documents() {
    for text in ["", "\n", "\n\n", " ", "\t\n", "\r\n", "\r", "*", "* ", "*\n", "* \n", "**", "\u{a0}"] {
        assert_lossless(text);
    }
}

#[test]
fn every_prefix_of_every_fixture() {
    for text in fixtures() {
        for (index, _) in text.char_indices() {
            assert_lossless(&text[..index]);
        }
    }
}

#[test]
fn every_suffix_of_every_fixture() {
    for text in fixtures() {
        for (index, _) in text.char_indices().step_by(7) {
            assert_lossless(&text[index..]);
        }
    }
}

#[test]
fn crlf_line_endings() {
    for text in fixtures() {
        assert_lossless(&text.replace('\n', "\r\n"));
    }
    // A terminator is never part of a line's content.
    let root = parse("* TODO a\r\nb\r\n", &TodoKeywords::default()).syntax();
    let headline = org_syntax::ast::headlines(&root).next().unwrap();
    assert_eq!(headline.title_text(), "a");
}

#[test]
fn random_edits() {
    const SNIPPETS: &[&str] = &[
        "*",
        "* ",
        "\n",
        "\n\n",
        " ",
        "[",
        "]",
        "[[",
        "]]",
        "<",
        ">",
        ":",
        "::",
        "#+",
        "|",
        "-",
        "\\",
        "$",
        "_",
        "{",
        "}",
        "(",
        ")",
        "@@",
        "~",
        "=",
        "/",
        "+",
        "\t",
        "é",
        "日",
        "\r\n",
        ":END:",
        "#+END_SRC",
        "[fn:",
        "TODO ",
    ];
    let mut rng = Rng(0x5eed);
    for text in fixtures() {
        for _ in 0..300 {
            let mut edited = text.clone();
            for _ in 0..1 + rng.next(4) {
                let boundaries: Vec<usize> =
                    edited.char_indices().map(|(index, _)| index).chain([edited.len()]).collect();
                let at = boundaries[rng.next(boundaries.len())];
                if rng.next(2) == 0 {
                    edited.insert_str(at, SNIPPETS[rng.next(SNIPPETS.len())]);
                } else {
                    let end = boundaries[(rng.next(boundaries.len())).max(1)].max(at);
                    let end = end.min(at + 40);
                    let end = (at..=end).rev().find(|index| edited.is_char_boundary(*index)).unwrap();
                    edited.replace_range(at..end, "");
                }
            }
            assert_lossless(&edited);
        }
    }
}

/// Inputs built to defeat a naive parser: thousands of openers that are
/// never closed, and nesting far deeper than any real document. Each of
/// these took seconds, or overflowed the stack, before the parser memoised
/// its searches and capped nesting.
#[test]
fn hostile_inputs_stay_fast_and_shallow() {
    let repeat = |piece: &str, count: usize| piece.repeat(count);
    let nested_list: String = (0..400).map(|depth| format!("{}- item\n", " ".repeat(depth))).collect();
    let inputs = [
        repeat("[fn::a ", 20_000) + "]\n",
        repeat("\\(a ", 20_000),
        repeat("<http://a\n", 20_000) + "> x\n",
        repeat("a_{b ", 20_000) + "}\n",
        repeat("[cite:@a ", 20_000),
        repeat("[cite:@ ", 20_000) + &repeat("]", 20_000),
        repeat("(src_a", 20_000),
        repeat("{call_a", 20_000),
        repeat("<%%(a ", 20_000) + ">",
        repeat("{{{m(a ", 20_000),
        repeat("@@a:b ", 20_001),
        repeat("[[a][b ", 20_000),
        repeat("*a /b _c +d =e ~f ", 20_000),
        repeat("*a ", 5_000) + "x" + &repeat(" a*", 5_000),
        repeat("[fn::a ", 5_000) + &repeat("]", 5_000),
        repeat("#+begin_src x\n:DRAWER:\n\\begin{a}\n#+BEGIN: d\ntext\n", 5_000),
        repeat("- a\n  #+begin_src x\n", 5_000),
        repeat("#+begin_quote\n#+begin_center\n", 2_000) + &repeat("#+end_center\n#+end_quote\n", 2_000),
        nested_list,
    ];
    let start = std::time::Instant::now();
    for text in &inputs {
        assert_lossless(text);
    }
    // Generous: all of these parse in well under a second in a debug build.
    assert!(start.elapsed() < std::time::Duration::from_secs(20), "took {:?}", start.elapsed());
}

#[test]
fn nesting_is_capped() {
    let depth_of = |text: &str| {
        let root = parse(text, &TodoKeywords::default()).syntax();
        root.descendants().map(|node| node.ancestors().count()).max().unwrap_or(0)
    };
    let bold = "*a ".repeat(500) + "x" + &" a*".repeat(500);
    assert!(depth_of(&bold) < 80, "{}", depth_of(&bold));
    let list: String = (0..500).map(|depth| format!("{}- item\n", " ".repeat(depth))).collect();
    assert!(depth_of(&list) < 200, "{}", depth_of(&list));
}

/// `plain_link_end` asked within a character or past the end finds no
/// link, rather than slicing the text there.
#[test]
fn plain_links_are_not_found_within_a_character() {
    let text = "é https://example.com";
    assert_eq!(org_syntax::plain_link_end(text, 1), None);
    assert_eq!(org_syntax::plain_link_end(text, text.len() + 1), None);
    assert_eq!(org_syntax::plain_link_end(text, 3), Some(text.len()));
}

/// `org_syntax::token_at_offset` finds what rowan's `token_at_offset`
/// does, at every offset of a document with most kinds of nodes.
#[test]
fn token_at_offset_is_rowans() {
    let text = "#+TITLE: x\n\n* TODO [#A] Head *bold* :a:b:\n  SCHEDULED: <2026-10-21 Wed +1d>\n  :PROPERTIES:\n  :ID: 1\n  :END:\n- [ ] item [[https://e.com][link]]\n  1. sub\n\n| a | b |\n|---+---|\n#+begin_src sh\necho é日🎉\n#+end_src\n** Child\nText\\alpha x^2\n*** \n* Last";
    for text in [text, "", "*", "* a\n", "\n\n", "text"] {
        let root = parse(text, &TodoKeywords::default()).syntax();
        for offset in 0..=text.len() as u32 {
            let offset = offset.into();
            let theirs = format!("{:?}", root.token_at_offset(offset).collect::<Vec<_>>());
            let ours = format!("{:?}", org_syntax::token_at_offset(&root, offset).collect::<Vec<_>>());
            assert_eq!(ours, theirs, "at {offset:?} of {text:?}");
        }
    }
}
