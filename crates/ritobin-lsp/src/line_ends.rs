use std::collections::HashMap;

use lsp_types::{Position, Range};
use ltk_ritobin::parse::Span;

/// A non-ASCII character, as byte offsets from the start of its line.
#[derive(Debug, PartialEq, Eq)]
struct NonAsciiChar {
    start: u32,
    end: u32,
}

impl NonAsciiChar {
    fn len(&self) -> u32 {
        self.end - self.start
    }

    fn len_utf16(&self) -> u32 {
        if self.len() == 4 { 2 } else { 1 }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LineNumbers {
    line_starts: Vec<u32>,
    length: u32,
    /// The byte offsets of non-ASCII characters in each line
    non_ascii_chars: HashMap<u32, Vec<NonAsciiChar>>,
}

impl LineNumbers {
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0];
        let mut non_ascii_chars: HashMap<u32, Vec<NonAsciiChar>> = HashMap::new();

        if src.is_ascii() {
            line_starts.extend(src.match_indices('\n').map(|(i, _)| i as u32 + 1));
        } else {
            let mut line_start = 0;
            for (i, c) in src.char_indices() {
                let i = i as u32;
                if c == '\n' {
                    line_starts.push(i + 1);
                    line_start = i + 1;
                } else if !c.is_ascii() {
                    non_ascii_chars
                        .entry(line_starts.len() as u32 - 1)
                        .or_default()
                        .push(NonAsciiChar {
                            start: i - line_start,
                            end: i - line_start + c.len_utf8() as u32,
                        });
                }
            }
        }

        Self {
            length: src.len() as u32,
            line_starts,
            non_ascii_chars,
        }
    }

    /// Re-indexes after `span` was replaced with `text`.
    ///
    /// `span` is in pre-edit offsets, so capture it before replacing the text.
    ///
    /// Returns:
    /// - `true` if the splice was successful and the index is now valid for the new text.
    /// - `false` if the splice bailed and a re-scan is needed to rebuild the index.
    #[must_use]
    pub fn splice(&mut self, span: Span, text: &str) -> bool {
        // splicing `non_ascii_chars` would rekey every line after the edit
        if !self.non_ascii_chars.is_empty() || !text.is_ascii() {
            return false;
        }

        let first = self.line_number(span.start) as usize;
        let last = self.line_number(span.end) as usize;
        let delta = text.len() as i64 - (span.end - span.start) as i64;

        let starts = text
            .match_indices('\n')
            .map(|(i, _)| span.start + i as u32 + 1)
            .collect::<Vec<_>>();

        // exclusive: an edit inside one line has `first == last` and replaces no line at all
        let tail = first + 1 + starts.len();
        self.line_starts.splice(first + 1..last + 1, starts);

        for start in &mut self.line_starts[tail..] {
            *start = (*start as i64 + delta) as u32;
        }
        self.length = (self.length as i64 + delta) as u32;

        true
    }

    /// Get the line number for a byte index
    pub fn line_number(&self, byte_index: u32) -> u32 {
        self.line_starts
            .binary_search(&byte_index)
            .unwrap_or_else(|next_line| next_line - 1) as u32
    }

    pub fn position(&self, byte_index: u32) -> Position {
        let line = self.line_number(byte_index);
        let column = byte_index
            - self
                .line_starts
                .get(line as usize)
                .copied()
                .unwrap_or_default();
        Position::new(line, self.utf8_to_utf16_col(line, column))
    }

    pub fn from_position(&self, position: &Position) -> u32 {
        self.byte_index(position.line, position.character)
    }

    pub fn from_range(&self, range: &Range) -> Span {
        let start = self.from_position(&range.start);
        let end = self.from_position(&range.end);

        Span::new(start.min(end), start.max(end))
    }

    /// Maps a `line` and `character` to a byte index.
    /// Remarks:
    /// - The index is clamped to the end of the line, and to the end of the document.
    pub fn byte_index(&self, line: u32, character: u32) -> u32 {
        let Some(line_start) = self.line_starts.get(line as usize) else {
            return self.length;
        };

        line_start
            .saturating_add(self.utf16_to_utf8_col(line, character))
            .min(self.line_content_end(line))
    }

    /// Byte index of the end of `line`'s content.
    fn line_content_end(&self, line: u32) -> u32 {
        match self.line_starts.get(line as usize + 1) {
            Some(next_start) => next_start - 1,
            None => self.length,
        }
    }

    /// Maps an LSP UTF-16 `character` to a byte offset into the line
    fn utf16_to_utf8_col(&self, line: u32, col: u32) -> u32 {
        let Some(non_ascii) = self.non_ascii_chars.get(&line) else {
            return col;
        };

        let (mut utf16_col, mut byte_col) = (0, 0);
        for c in non_ascii {
            let ascii_run = c.start - byte_col;
            if col <= utf16_col + ascii_run {
                return byte_col.saturating_add(col - utf16_col);
            }
            utf16_col += ascii_run + c.len_utf16();
            byte_col = c.end;

            // snap to the end of the wide char
            if col < utf16_col {
                return byte_col;
            }
        }

        byte_col.saturating_add(col - utf16_col)
    }

    /// Maps a byte offset into the line to an LSP UTF-16 `character`
    fn utf8_to_utf16_col(&self, line: u32, col: u32) -> u32 {
        let Some(non_ascii) = self.non_ascii_chars.get(&line) else {
            return col;
        };

        let mut utf16_col = col;
        for c in non_ascii {
            if c.end > col {
                break;
            }
            utf16_col -= c.len() - c.len_utf16();
        }
        utf16_col
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

            let line_len = self.utf8_to_utf16_col(line, line_end - line_start);
            // tracing::debug!(?start_line, ?end_line, ?self.length);
            // tracing::debug!(?line, ?line_start, ?line_end, ?line_len);

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

#[test]
fn positions_past_the_end_clamp() {
    let src = "ab\ncdef\n";
    let line_numbers = LineNumbers::new(src);

    // past the end of a line
    assert_eq!(line_numbers.byte_index(0, 99), 2);
    assert_eq!(line_numbers.byte_index(1, u32::MAX), 7);
    // past the end of the document
    assert_eq!(line_numbers.byte_index(2, 99), src.len() as u32);
    assert_eq!(line_numbers.byte_index(99, 0), src.len() as u32);
}

/// An LSP `character` counts utf-16 code units, not bytes: `é` is 2 bytes but 1 unit, `😀` is
/// 4 bytes but 2.
#[test]
fn wide_characters_shift_the_columns_after_them() {
    // bytes:  a 0 | é 1..3 | ' ' 3 | b 4
    // utf-16: a 0 | é 1    | ' ' 2 | b 3
    let line_numbers = LineNumbers::new("aé b\n");

    assert_eq!(line_numbers.byte_index(0, 0), 0);
    assert_eq!(line_numbers.byte_index(0, 1), 1);
    assert_eq!(line_numbers.byte_index(0, 2), 3);
    assert_eq!(line_numbers.byte_index(0, 3), 4);
    assert_eq!(line_numbers.byte_index(0, 4), 5);

    assert_eq!(line_numbers.position(3), Position::new(0, 2));
    assert_eq!(line_numbers.position(4), Position::new(0, 3));
}

#[test]
fn an_astral_character_is_two_utf16_units() {
    // bytes:  😀 0..4 | x 4
    // utf-16: 😀 0..2 | x 2
    let line_numbers = LineNumbers::new("😀x\n");

    assert_eq!(line_numbers.byte_index(0, 0), 0);
    assert_eq!(line_numbers.byte_index(0, 2), 4);
    assert_eq!(line_numbers.position(4), Position::new(0, 2));
    // a position splitting the surrogate pair snaps to a char boundary rather than into it
    assert_eq!(line_numbers.byte_index(0, 1), 4);
}

#[test]
fn positions_round_trip_to_byte_offsets() {
    for src in ["ab\ncd\n", "aé b\nx\n", "😀x\né😀é\n", "é", "\n\n", ""] {
        let line_numbers = LineNumbers::new(src);

        for (i, _) in src.char_indices() {
            let i = i as u32;
            let pos = line_numbers.position(i);

            assert_eq!(
                line_numbers.byte_index(pos.line, pos.character),
                i,
                "{src:?} at byte {i} ({pos:?})"
            );
        }
    }
}

/// Applies the edit to `src` and asserts the spliced index is the one a rescan would build.
#[cfg(test)]
#[track_caller]
fn splice_matches_rescan(src: &str, span: (u32, u32), text: &str) -> String {
    let span = Span::new(span.0, span.1);
    let mut spliced = LineNumbers::new(src);

    let mut edited = src.to_owned();
    edited.replace_range(span.start as usize..span.end as usize, text);

    assert!(
        spliced.splice(span, text),
        "{src:?} {span:?} {text:?} should have taken the fast path"
    );
    assert_eq!(
        spliced,
        LineNumbers::new(&edited),
        "{src:?} {span:?} += {text:?} -> {edited:?}"
    );

    edited
}

#[test]
fn an_edit_inside_one_line_keeps_the_lines_around_it() {
    // the case that gets the splice range wrong: nothing is replaced, so an inclusive range
    // would swallow the line below
    splice_matches_rescan("ab\ncd\nef\n", (3, 3), "X");
    splice_matches_rescan("ab\ncd\nef\n", (3, 5), "");
    splice_matches_rescan("ab\ncd\nef\n", (3, 5), "XYZ");
}

#[test]
fn an_edit_can_add_and_remove_lines() {
    splice_matches_rescan("ab\ncd\nef\n", (3, 3), "1\n2\n3");
    // deleting across a line break joins the two lines
    splice_matches_rescan("ab\ncd\nef\n", (1, 4), "");
    // and replacing a whole run of them
    splice_matches_rescan("ab\ncd\nef\n", (0, 9), "one line");
}

#[test]
fn an_edit_at_either_end_of_the_document() {
    splice_matches_rescan("ab\ncd\n", (0, 0), "X\n");
    splice_matches_rescan("ab\ncd\n", (6, 6), "X");
    splice_matches_rescan("ab\ncd\n", (6, 6), "\n");
    splice_matches_rescan("", (0, 0), "x\ny");
    splice_matches_rescan("ab\ncd\n", (0, 6), "");
}

/// Every edit in a long sequence has to leave the index exactly where a rescan would, because
/// the next edit resolves its positions against it - one bad splice corrupts everything after.
#[test]
fn a_sequence_of_edits_never_drifts_from_a_rescan() {
    const INSERTS: [&str; 6] = ["x", "", "\n", "a\nb", "hello", "\n\n\n"];

    let mut text = "alpha\nbeta\ngamma\ndelta\n".to_owned();
    // a deterministic walk over the edit space - a seeded LCG, so a failure is reproducible
    let mut seed = 0x2545_F491u32;
    let mut rand = move |n: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        if n == 0 { 0 } else { (seed >> 16) % n }
    };

    for _ in 0..500 {
        let len = text.len() as u32;
        let a = rand(len + 1);
        let b = rand(len + 1);
        let insert = INSERTS[rand(INSERTS.len() as u32) as usize];

        text = splice_matches_rescan(&text, (a.min(b), a.max(b)), insert);
        // keep it from collapsing to nothing or running away
        if text.len() > 4096 {
            text.truncate(2048);
        }
    }
}

#[test]
fn non_ascii_falls_back_to_a_rescan() {
    assert!(!LineNumbers::new("aé b\n").splice(Span::new(0, 0), "x"));
    // including when the edit is what introduces it
    assert!(!LineNumbers::new("ab\n").splice(Span::new(0, 0), "é"));
}

#[test]
fn a_reversed_range_yields_an_ordered_span() {
    let line_numbers = LineNumbers::new("ab\ncdef\n");

    let span = line_numbers.from_range(&Range::new(Position::new(1, 2), Position::new(0, 1)));

    assert_eq!((span.start, span.end), (1, 5));
}
