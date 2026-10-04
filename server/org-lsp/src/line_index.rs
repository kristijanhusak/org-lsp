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

/// Byte offsets of every line start. LSP treats `\n`, `\r\n` and a lone
/// `\r` as line terminators.
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
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
        LineIndex { line_starts, len: text.len() }
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
        Position { line: line as u32, character: encoding.len(&text[start..offset]) }
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
        let mut units = 0;
        for (index, c) in text[start..end].char_indices() {
            if units >= position.character {
                return start + index;
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
}
