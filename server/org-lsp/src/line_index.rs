//! Conversion between byte offsets and LSP positions.

use lsp_types::{Position, PositionEncodingKind, Range};
use org_syntax::{TextRange, TextSize};

/// The unit LSP `character` offsets are counted in, as negotiated with the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

impl Encoding {
    /// Pick the cheapest encoding the client offers. UTF-16 is the protocol
    /// default and the only one every client supports.
    pub fn negotiate(offered: Option<&[PositionEncodingKind]>) -> Encoding {
        let offered = offered.unwrap_or(&[]);
        if offered.contains(&PositionEncodingKind::UTF8) {
            Encoding::Utf8
        } else if offered.contains(&PositionEncodingKind::UTF32) {
            Encoding::Utf32
        } else {
            Encoding::Utf16
        }
    }

    pub fn kind(self) -> PositionEncodingKind {
        match self {
            Encoding::Utf8 => PositionEncodingKind::UTF8,
            Encoding::Utf16 => PositionEncodingKind::UTF16,
            Encoding::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    /// Length of `text` in this encoding's code units.
    pub fn len(self, text: &str) -> u32 {
        let len = match self {
            Encoding::Utf8 => text.len(),
            Encoding::Utf16 => text.chars().map(char::len_utf16).sum(),
            Encoding::Utf32 => text.chars().count(),
        };
        len as u32
    }

    fn char_len(self, c: char) -> u32 {
        match self {
            Encoding::Utf8 => c.len_utf8() as u32,
            Encoding::Utf16 => c.len_utf16() as u32,
            Encoding::Utf32 => 1,
        }
    }
}

/// How far apart, in bytes, the marks of a text that is not ASCII are.
const MARK_EVERY: usize = 256;

/// Byte offsets of every line start. LSP treats `\n`, `\r\n` and a lone
/// `\r` as line terminators.
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
    /// For a text that is not ASCII, at the first character at or after
    /// every [`MARK_EVERY`] bytes: its byte offset, and the UTF-16 and
    /// UTF-32 units before it. A position is counted from the mark before
    /// it, not from the start of its line, which can be long. None for
    /// ASCII, where a byte is a unit in every encoding.
    marks: Option<Vec<(usize, u32, u32)>>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let bytes = text.as_bytes();
        let mut line_starts = vec![0];
        for (i, b) in bytes.iter().enumerate() {
            match b {
                b'\n' => line_starts.push(i + 1),
                b'\r' if bytes.get(i + 1) != Some(&b'\n') => line_starts.push(i + 1),
                _ => {}
            }
        }
        let marks = (!text.is_ascii()).then(|| {
            let mut marks = Vec::with_capacity(text.len() / MARK_EVERY + 1);
            let (mut utf16, mut utf32) = (0, 0);
            let mut next = 0;
            for (at, c) in text.char_indices() {
                if at >= next {
                    marks.push((at, utf16, utf32));
                    next = at + MARK_EVERY;
                }
                utf16 += c.len_utf16() as u32;
                utf32 += 1;
            }
            marks
        });
        LineIndex { line_starts, len: text.len(), marks }
    }

    /// The units of `encoding` in `text[..offset]`.
    fn units_before(&self, text: &str, offset: usize, encoding: Encoding) -> u32 {
        let Some(marks) = self.marks.as_ref().filter(|_| encoding != Encoding::Utf8) else { return offset as u32 };
        let (at, utf16, utf32) = marks[marks.partition_point(|(at, _, _)| *at <= offset).max(1) - 1];
        let before = if encoding == Encoding::Utf16 { utf16 } else { utf32 };
        before + encoding.len(&text[at..offset])
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The line containing byte `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|start| *start <= offset) - 1
    }

    /// Byte range of line `line` without its terminator.
    pub fn line_range(&self, text: &str, line: usize) -> (usize, usize) {
        let start = self.line_starts[line];
        let next = self.line_starts.get(line + 1).copied().unwrap_or(self.len);
        let content = text[start..next].trim_end_matches(['\n', '\r']);
        (start, start + content.len())
    }

    pub fn position(&self, text: &str, offset: usize, encoding: Encoding) -> Position {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        let start = self.line_starts[line];
        let character = self.units_before(text, offset, encoding) - self.units_before(text, start, encoding);
        Position { line: line as u32, character }
    }

    pub fn range(&self, text: &str, range: TextRange, encoding: Encoding) -> Range {
        Range {
            start: self.position(text, range.start().into(), encoding),
            end: self.position(text, range.end().into(), encoding),
        }
    }

    /// The byte offset of `position`. Positions past the end of a line or of
    /// the document are clamped, as the protocol asks.
    pub fn offset(&self, text: &str, position: Position, encoding: Encoding) -> usize {
        let line = position.line as usize;
        if line >= self.line_starts.len() {
            return self.len;
        }
        let (start, end) = self.line_range(text, line);
        let marks = match &self.marks {
            Some(marks) if encoding != Encoding::Utf8 => marks,
            // A unit is a byte: the first character at or after it.
            _ => {
                let mut at = (start + position.character as usize).min(end);
                while !text.is_char_boundary(at) {
                    at += 1;
                }
                return at;
            }
        };
        // Counted from the last mark of the line not past the position.
        let base = self.units_before(text, start, encoding);
        let after = |offset: usize| marks.partition_point(|(at, _, _)| *at <= offset);
        let on_line = &marks[after(start)..after(end)];
        let units_at =
            |(_, utf16, utf32): &(usize, u32, u32)| (if encoding == Encoding::Utf16 { *utf16 } else { *utf32 }) - base;
        let (mut from, mut units) = (start, 0);
        let before = on_line.partition_point(|mark| units_at(mark) <= position.character);
        if let Some(mark) = before.checked_sub(1).map(|index| &on_line[index]) {
            (from, units) = (mark.0, units_at(mark));
        }
        for (index, c) in text[from..end].char_indices() {
            if units >= position.character {
                return from + index;
            }
            units += encoding.char_len(c);
        }
        end
    }

    pub fn text_range(&self, text: &str, range: Range, encoding: Encoding) -> TextRange {
        let start = self.offset(text, range.start, encoding);
        let end = self.offset(text, range.end, encoding).max(start);
        TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "a😀b\né\r\n\rlast";

    #[test]
    fn lines() {
        let index = LineIndex::new(TEXT);
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.line_range(TEXT, 0), (0, 6));
        assert_eq!(index.line_range(TEXT, 1), (7, 9));
        assert_eq!(index.line_range(TEXT, 2), (11, 11));
        assert_eq!(&TEXT[12..], "last");
        assert_eq!(index.line_range(TEXT, 3), (12, 16));
    }

    #[test]
    fn positions_per_encoding() {
        let index = LineIndex::new(TEXT);
        // The offset of `b`, after a four-byte character.
        assert_eq!(index.position(TEXT, 5, Encoding::Utf8), Position::new(0, 5));
        assert_eq!(index.position(TEXT, 5, Encoding::Utf16), Position::new(0, 3));
        assert_eq!(index.position(TEXT, 5, Encoding::Utf32), Position::new(0, 2));
        for encoding in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
            for (offset, _) in TEXT.char_indices().filter(|(_, c)| !matches!(c, '\n' | '\r')) {
                let position = index.position(TEXT, offset, encoding);
                assert_eq!(index.offset(TEXT, position, encoding), offset, "{encoding:?} at {offset}");
            }
        }
    }

    #[test]
    fn clamps_out_of_range_positions() {
        let index = LineIndex::new(TEXT);
        assert_eq!(index.offset(TEXT, Position::new(0, 99), Encoding::Utf16), 6);
        assert_eq!(index.offset(TEXT, Position::new(99, 0), Encoding::Utf16), TEXT.len());
        assert_eq!(index.position(TEXT, 999, Encoding::Utf8), Position::new(3, 4));
        // A position inside a surrogate pair rounds up to the next character.
        assert_eq!(index.offset(TEXT, Position::new(0, 2), Encoding::Utf16), 5);
    }

    #[test]
    fn empty_document() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.offset("", Position::new(0, 0), Encoding::Utf16), 0);
        assert_eq!(index.position("", 0, Encoding::Utf16), Position::new(0, 0));
    }

    /// The position of `offset`, measured from the start of its line.
    fn position_from_line_start(index: &LineIndex, text: &str, offset: usize, encoding: Encoding) -> Position {
        let offset = offset.min(text.len());
        let line = index.line_of(offset);
        Position { line: line as u32, character: encoding.len(&text[index.line_starts[line]..offset]) }
    }

    /// The offset of `position`, counted from the start of its line.
    fn offset_from_line_start(index: &LineIndex, text: &str, position: Position, encoding: Encoding) -> usize {
        if position.line as usize >= index.line_count() {
            return text.len();
        }
        let (start, end) = index.line_range(text, position.line as usize);
        let mut units = 0;
        for (at, c) in text[start..end].char_indices() {
            if units >= position.character {
                return start + at;
            }
            units += encoding.char_len(c);
        }
        end
    }

    /// The marks give what counting from the start of the line gives, for
    /// every offset and position of texts with long lines, many-byte
    /// characters and every kind of line end.
    #[test]
    fn marks_count_as_the_line_does() {
        let pieces = ["a", "é", "😀", "\n", "\r\n", "\r", "[[x]] ", "中文", " "];
        let mut seed: u64 = 7;
        for length in [0, 1, 40, 300, 2000, 9000] {
            let text: String = (0..length)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    // Line ends rarely, so that lines are long.
                    let pick = (seed >> 33) as usize % 600;
                    pieces[if pick < pieces.len() { pick } else { pick % 3 }]
                })
                .collect();
            let index = LineIndex::new(&text);
            for encoding in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
                for (offset, _) in text.char_indices().chain([(text.len(), ' ')]) {
                    let expected = position_from_line_start(&index, &text, offset, encoding);
                    assert_eq!(index.position(&text, offset, encoding), expected, "{encoding:?} at {offset}");
                }
                for line in 0..index.line_count() as u32 + 1 {
                    let (start, end) = index.line_range(&text, (line as usize).min(index.line_count() - 1));
                    for character in 0..=encoding.len(&text[start..end]) + 2 {
                        let position = Position::new(line, character);
                        let expected = offset_from_line_start(&index, &text, position, encoding);
                        assert_eq!(index.offset(&text, position, encoding), expected, "{encoding:?} at {position:?}");
                    }
                }
            }
        }
    }

    /// Positions on one long line, which used to be measured from its
    /// start each time: every link of a 240 KB line took a minute.
    #[test]
    fn positions_on_a_long_line_fast() {
        let text = "é [[x]] ".repeat(30_000);
        let index = LineIndex::new(&text);
        let start = std::time::Instant::now();
        for (offset, _) in text.char_indices().step_by(7) {
            for encoding in [Encoding::Utf16, Encoding::Utf32] {
                let position = index.position(&text, offset, encoding);
                assert_eq!(index.offset(&text, position, encoding), offset);
            }
        }
        // Generous: this takes a fraction of a second in a debug build.
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "took {:?}", start.elapsed());
    }
}
