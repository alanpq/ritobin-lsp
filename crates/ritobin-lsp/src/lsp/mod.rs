use lsp_types::{Position, Range};
use ltk_ritobin::Span;
use ritobin_lsp::line_ends::LineNumbers;

pub mod capabilities;
pub mod ext;
pub mod semantic_tokens;

pub fn src_span_to_lsp_range(location: Span, line_numbers: &LineNumbers) -> Range {
    let start = line_numbers.line_and_column_number(location.start);
    let end = line_numbers.line_and_column_number(location.end);

    Range::new(
        Position::new(start.line - 1, start.column - 1),
        Position::new(end.line - 1, end.column - 1),
    )
}
