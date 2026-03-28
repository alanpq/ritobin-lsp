use lsp_types::{Position, Range};
use ltk_ritobin::{
    cst::{Cst, TreeKind, Visitor, visitor::Visit},
    parse::{Span, Token, TokenKind},
};
use ritobin_lsp::line_ends::LineNumbers;

use crate::lsp::semantic_tokens::{
    self,
    builder::{SemanticTokensBuilder, type_index},
};

pub struct SemanticVisitor<'a> {
    pub text: &'a str,
    pub line_nums: &'a LineNumbers,
    pub builder: SemanticTokensBuilder,
    pub stack: Vec<TreeKind>,
    pub range: Option<Span>,
}

impl Visitor for SemanticVisitor<'_> {
    fn enter_tree(&mut self, tree: &Cst) -> Visit {
        if matches!(tree.kind, TreeKind::ErrorTree) {
            return Visit::Continue;
        }
        self.stack.push(tree.kind);
        Visit::Continue
    }

    fn exit_tree(&mut self, tree: &Cst) -> Visit {
        if matches!(tree.kind, TreeKind::ErrorTree) {
            return Visit::Continue;
        }
        self.stack.pop();
        Visit::Continue
    }
    fn visit_token(&mut self, token: &Token, _context: &Cst) -> Visit {
        if let Some(range) = self.range
            && !token.span.intersects(&range)
        {
            return Visit::Continue;
        }
        let last_tree = self.stack.last().unwrap();
        // tracing::debug!(
        //     "{:?} ({:?}) | last tree: {last_tree:?}",
        //     token.kind,
        //     &self.text[token.span.start as usize..token.span.end as usize],
        // );

        use TokenKind::*;
        let token_kind = match (last_tree, token.kind) {
            (_, Comment) => semantic_tokens::types::COMMENT,
            (_, Colon | Comma | Eq) => semantic_tokens::types::PUNCTUATION,
            (_, RCurly | LCurly | RBrack | LBrack) => semantic_tokens::types::BRACKET,

            (TreeKind::TypeExpr, _) => semantic_tokens::types::TYPE,
            (TreeKind::TypeArg, _) | (TreeKind::TypeArgList, _) => {
                semantic_tokens::types::TYPE_PARAMETER
            }
            (TreeKind::Class, _) => semantic_tokens::types::CLASS,
            (_, Name) => semantic_tokens::types::KEYWORD,
            (_, Quote) | (_, String) | (_, UnterminatedString) => semantic_tokens::types::STRING,
            (_, Number) | (_, HexLit) => semantic_tokens::types::NUMBER,
            _ => {
                return Visit::Continue;
            }
        };
        for (line, range) in self.line_nums.iter_span_lines(token.span) {
            // tracing::debug!(?line, ?range);
            self.builder.push(
                Range::new(
                    Position::new((line) as _, *range.start()),
                    Position::new((line) as _, *range.end()),
                ),
                type_index(&token_kind),
                semantic_tokens::modifier_set::ModifierSet::default().0,
            );
        }
        Visit::Continue
    }
}
