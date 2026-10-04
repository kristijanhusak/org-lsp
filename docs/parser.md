# The parser

`server/org-syntax` is a hand-written parser. It has no dependency on the
language server and can be used on its own.

## Shape of the tree

The parser produces a lossless [rowan](https://docs.rs/rowan) tree. Every
byte of the input is in exactly one token, so the tree always prints back to
the input, and every node knows its source range. `parse` never fails.

Parsing follows `org-element.el`:

- Headlines are found first, by line prefix alone. That fixes the outline.
- Each section is parsed into elements in the order
  `org-element--current-element` tries them. Blocks, drawers and LaTeX
  environments are only elements when their closing line exists.
- What ends a paragraph is decided by a separate, looser rule than what
  starts an element, as in Org.
- Affiliated keywords (`#+NAME:`, `#+CAPTION:`, `#+ATTR_*:` …) are the
  leading children of the element they belong to.
- Inline objects follow `org-element-object-restrictions`: for example a
  link description cannot contain another link.

The document is reparsed in full on every change. In a release build, a
2 KB file parses in about 0.1 ms and a 4 MB file with 177,000 lines in about
130 ms.

## Conformance

`tests/fixtures/*.org` are parsed by both this parser and Emacs
(`scripts/org-element-dump.el`, Org 9.6.15). The test
`server/org-syntax/tests/org_element.rs` requires the same set of elements
and objects, with the same types, at the same positions.

`scripts/fuzz-against-emacs.py` does the same for randomly generated
documents. Several thousand have been compared.

Known differences from `org-element`:

- `* TODO` with nothing after the keyword. Org 9.6's parser wants a space
  after the keyword and reads `TODO` as the title; the rest of Org treats it
  as a keyword, and so does this parser.
- Radio links (text that matches a `<<<radio target>>>` elsewhere) are not
  recognised.
- `table.el` tables and inline tasks are not recognised; they parse as
  paragraphs and headlines.
- Line breaks (`\\` at the end of a line) are not a separate node.
- Citations are a single node; their references are not split out.
- A `#+TBLFM:` line that follows a table inside a list item, but is indented
  less than the item, ends the item here. In Org the table reaches past the
  end of the item that contains it.
- Timestamps that make Org itself signal an error, such as
  `<2026-1-1 Thu +1w>`, are plain text.

## Limits

Nesting is capped at 64 levels for elements (blocks, drawers, list items) and
for objects (emphasis, footnotes). Anything deeper is kept as plain text.
Searches for closing delimiters are memoised, so documents with thousands of
unclosed openers still parse in linear time. `tests/robustness.rs` covers
both.

## Tools

```sh
cargo run -p org-syntax --example dump -- file.org           # print the tree
cargo run --release -p org-lsp --example timing -- file.org   # time it
scripts/update-fixtures.sh                                    # needs Emacs
scripts/update-entities.sh                                    # needs Emacs
scripts/fuzz-against-emacs.py --count 500 --seed 1            # needs Emacs
```
