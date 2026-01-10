use lsp_types::{Position, Range};
use ltk_ritobin::Span;

#[derive(Debug)]
pub struct LineNumbers {
    line_starts: Vec<u32>,
    length: u32,
}

impl LineNumbers {
    pub fn new(src: &str) -> Self {
        Self {
            length: src.len() as u32,
            line_starts: std::iter::once(0)
                .chain(src.match_indices('\n').map(|(i, _)| i as u32 + 1))
                .collect(),
        }
    }

    /// Get the line number for a byte index
    pub fn line_number(&self, byte_index: u32) -> u32 {
        self.line_starts
            .binary_search(&byte_index)
            .unwrap_or_else(|next_line| next_line - 1) as u32
    }

    // TODO: handle unicode characters that may be more than 1 byte in width
    pub fn position(&self, byte_index: u32) -> Position {
        let line = self.line_number(byte_index);
        let column = byte_index
            - self
                .line_starts
                .get(line as usize)
                .copied()
                .unwrap_or_default();
        Position::new(line, column)
    }

    // TODO: handle unicode characters that may be more than 1 byte in width
    /// 0 indexed line and character to byte index
    pub fn byte_index(&self, line: u32, character: u32) -> u32 {
        match self.line_starts.get((line) as usize) {
            Some(line_index) => *line_index + character,
            None => self.length,
        }
    }

    pub fn from_span(&self, span: Span) -> Range {
        Range::new(self.position(span.start), self.position(span.end))
    }

    pub fn iter_span_lines(
        &self,
        span: Span,
    ) -> impl Iterator<Item = (u32, std::ops::RangeInclusive<u32>)> + '_ {
        let span_start = span.start;
        let span_end = span.end;

        let start_lc = self.position(span_start);
        let end_lc = self.position(span_end);

        let start_line = start_lc.line;
        let end_line = end_lc.line;

        (start_line..=end_line).map(move |line| {
            let line_start = self.line_starts[(line) as usize];
            let line_end = self
                .line_starts
                .get(line as usize + 1)
                .copied()
                .unwrap_or(self.length);

            let line_len = line_end - line_start;
            tracing::debug!(?start_line, ?end_line, ?self.length);
            tracing::debug!(?line, ?line_start, ?line_end, ?line_len);

            let (from, to) = if start_line == end_line {
                (start_lc.character, end_lc.character)
            } else if line == start_line {
                (start_lc.character, line_len)
            } else if line == end_line {
                (0, end_lc.character)
            } else {
                (0, line_len)
            };

            (line, from..=to)
        })
    }
}

#[test]
fn byte_index() {
    let src = &r#"import gleam/io

pub fn main() {
  io.println("Hello, world!")
}
"#;
    let line_numbers = LineNumbers::new(src);

    assert_eq!(line_numbers.byte_index(0, 0), 0);
    assert_eq!(line_numbers.byte_index(0, 4), 4);
    assert_eq!(line_numbers.byte_index(100, 1), src.len() as u32);
    assert_eq!(line_numbers.byte_index(2, 1), 18);
}
