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
                }
            }
        }

        self.line_numbers = LineNumbers::new(&self.text);
    }
}
