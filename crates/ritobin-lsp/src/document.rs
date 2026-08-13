use lsp_types::TextDocumentContentChangeEvent;
use lsp_types::Url;
use ritobin_lsp::line_ends::LineNumbers;

pub struct Document {
    pub uri: Url,
    pub version: i32,
    pub text: String,
    pub line_numbers: LineNumbers,
}

macro_rules! match_token {
    ($expr:expr, $kind:path) => {{
        match $expr {
            Child::Token(token @ Token { kind: $kind, .. }) => Some(token),
            _ => None,
        }
    }};
}
macro_rules! match_tree {
    ($expr:expr, $kind:path) => {{
        match $expr {
            Child::Tree(tree @ Cst { kind: $kind, .. }) => Some(tree),
            _ => None,
        }
    }};
}
impl Document {
    pub fn new(uri: Url, version: i32, text: String) -> Self {
        // let cst = Cst::parse(&text);
        // let parse_errors = FlatErrors::walk(&cst);
        Self {
            uri,
            version,
            line_numbers: LineNumbers::new(&text),
            text,
        }
    }

    pub fn update(&mut self, version: i32, changes: Vec<TextDocumentContentChangeEvent>) {
        self.version = version;
        for change in changes {
            match change.range {
                None => {
                    self.text = change.text;
                }
                Some(range) => {
                    let span = self.line_numbers.from_range(&range);
                    self.text
                        .replace_range(span.start as usize..span.end as usize, &change.text);

                    if self.line_numbers.splice(span, &change.text) {
                        continue;
                    }
                }
            }

            self.line_numbers = LineNumbers::new(&self.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

    fn document(text: &str) -> Document {
        Document::new(Url::parse("file:///test.rito").unwrap(), 0, text.to_owned())
    }

    fn edit(start: (u32, u32), end: (u32, u32), text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range::new(
                Position::new(start.0, start.1),
                Position::new(end.0, end.1),
            )),
            range_length: None,
            text: text.to_owned(),
        }
    }

    fn replace_whole_document(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_single_change_replaces_its_range() {
        let mut doc = document("aa\nbb\n");

        doc.update(3, vec![edit((1, 0), (1, 2), "zz")]);

        assert_eq!(doc.text, "aa\nzz\n");
        assert_eq!(doc.version, 3);
    }

    /// One notification carries one change per cursor.
    #[test]
    fn later_changes_in_a_batch_see_the_earlier_ones() {
        let mut doc = document("aa\nbb\n");

        doc.update(
            1,
            vec![edit((0, 0), (0, 0), "X"), edit((1, 0), (1, 0), "Y")],
        );

        assert_eq!(doc.text, "Xaa\nYbb\n");
    }

    #[test]
    fn a_change_that_adds_a_line_shifts_the_lines_after_it() {
        let mut doc = document("aa\nbb\n");

        doc.update(
            1,
            vec![edit((0, 2), (0, 2), "\nmid"), edit((2, 0), (2, 2), "cc")],
        );

        assert_eq!(doc.text, "aa\nmid\ncc\n");
    }

    /// Columns are utf-16 code units, so `b` is column 3 of `aé b`, not byte 4.
    #[test]
    fn a_change_after_a_wide_character_lands_in_the_right_place() {
        let mut doc = document("aé b\n");

        doc.update(1, vec![edit((0, 3), (0, 4), "B")]);

        assert_eq!(doc.text, "aé B\n");
    }

    /// A byte index landing inside the `é` panics `replace_range`.
    #[test]
    fn an_edit_after_a_wide_character_does_not_split_it() {
        let mut doc = document("é\n");

        doc.update(1, vec![edit((0, 1), (0, 1), "x")]);

        assert_eq!(doc.text, "éx\n");
    }

    /// `u32::MAX` is how a client says "to the end of the line".
    #[test]
    fn a_change_running_past_the_end_of_a_line_is_clamped() {
        let mut doc = document("aa\nbb\n");

        doc.update(1, vec![edit((0, 0), (0, u32::MAX), "zzz")]);

        assert_eq!(doc.text, "zzz\nbb\n");
    }

    #[test]
    fn a_full_replace_rebases_the_changes_after_it() {
        let mut doc = document("x");

        doc.update(
            1,
            vec![
                replace_whole_document("hello\nworld\n"),
                edit((1, 5), (1, 5), "!"),
            ],
        );

        assert_eq!(doc.text, "hello\nworld!\n");
    }
}
