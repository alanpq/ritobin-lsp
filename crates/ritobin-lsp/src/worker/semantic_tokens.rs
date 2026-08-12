use lsp_types::{Position, Range};
use ltk_ritobin::{
    cst::{
        Node, NodeId, TokenId, TreeKind, Visitor,
        visitor::{Visit, VisitCtx},
    },
    parse::{Span, TokenKind},
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
    /// For each [`TreeKind::Entry`] we are currently inside of, whether it declares a type
    /// (`name: type = value`).
    pub entry_typed: Vec<bool>,
    pub range: Option<Span>,
}

impl SemanticVisitor<'_> {
    /// Whether the entry we are currently inside of declares a type - i.e whether its key is a
    /// field name, rather than a map key (`key = value`).
    fn in_typed_entry(&self) -> bool {
        self.entry_typed.last().copied().unwrap_or(false)
    }
}

/// Whether an [`TreeKind::Entry`] node has an explicit type expression (`name: type = value`).
fn has_type_expr(ctx: &VisitCtx, entry: &Node) -> bool {
    entry
        .children
        .get(ctx.cst)
        .iter()
        .filter_map(|child| child.tree(ctx.cst))
        .any(|child| child.kind == TreeKind::TypeExpr)
}

impl Visitor for SemanticVisitor<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let tree = ctx.cst.node(node).unwrap();

        if matches!(tree.kind, TreeKind::ErrorTree) {
            return Visit::Continue;
        }
        if matches!(tree.kind, TreeKind::Entry) {
            self.entry_typed.push(has_type_expr(ctx, tree));
        }
        self.stack.push(tree.kind);
        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let tree = ctx.cst.node(node).unwrap();

        if matches!(tree.kind, TreeKind::ErrorTree) {
            return Visit::Continue;
        }
        if matches!(tree.kind, TreeKind::Entry) {
            self.entry_typed.pop();
        }
        self.stack.pop();
        Visit::Continue
    }
    fn visit_token(&mut self, ctx: &VisitCtx, token: TokenId, _parent: NodeId) -> Visit {
        let token = ctx.cst.token(token).unwrap();

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

            // built-in bin types - `string`, `f32`, `hash`, and the args of `list[..]`/`map[..]`
            (TreeKind::TypeExpr | TreeKind::TypeArg | TreeKind::TypeArgList, _) => {
                semantic_tokens::types::BUILTIN_TYPE
            }
            // meta class names - `VfxSystemDefinitionData { .. }`
            (TreeKind::Class, _) => semantic_tokens::types::CLASS,
            // field names - `particleName: string = ".."`. Keys of untyped entries are map keys
            // rather than fields, so those keep their literal highlighting.
            (TreeKind::EntryKey, Name) => semantic_tokens::types::PROPERTY,
            (TreeKind::EntryKey, HexLit) if self.in_typed_entry() => {
                semantic_tokens::types::PROPERTY
            }

            (_, True) | (_, False) => semantic_tokens::types::BOOLEAN,
            (_, Null) => semantic_tokens::types::KEYWORD,
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

#[cfg(test)]
mod tests {
    use ltk_ritobin::cst::{Cst, visitor::VisitorExt};

    use super::*;
    use crate::lsp::semantic_tokens::types::SUPPORTED_TYPES;

    /// Highlight `src`, returning a `"<lexeme> -> <token type>"` line per emitted token.
    fn highlight(src: &str) -> Vec<String> {
        let line_nums = LineNumbers::new(src);
        let cst = Cst::parse(src);
        let visitor = SemanticVisitor {
            text: src,
            line_nums: &line_nums,
            builder: SemanticTokensBuilder::new("test".to_owned()),
            stack: Vec::new(),
            entry_typed: Vec::new(),
            range: None,
        }
        .walk(&cst);

        let (mut line, mut start) = (0, 0);
        visitor
            .builder
            .build()
            .data
            .into_iter()
            .map(|token| {
                line += token.delta_line;
                start = if token.delta_line > 0 {
                    token.delta_start
                } else {
                    start + token.delta_start
                };

                let from = line_nums.byte_index(line, start) as usize;
                let ty = SUPPORTED_TYPES[token.token_type as usize].as_str();
                format!("{} -> {ty}", &src[from..from + token.length as usize])
            })
            .collect()
    }

    #[test]
    fn highlights_fields_types_and_classes() {
        let tokens = highlight(
            "entries: map[hash, embed] = {\n    \
                 0x18563f21 = VfxSystemDefinitionData {\n        \
                     particleName: string = \"sparks\"\n        \
                     isSingleParticle: flag = true\n        \
                     objectPath: hash = 0x18563f21\n        \
                     0x6d6b7c10: f32 = 0.5\n    \
                 }\n\
             }\n",
        );

        assert_eq!(
            tokens,
            [
                // top level entry - a field name, a builtin type & its args
                "entries -> property",
                ": -> punctuation",
                "map -> builtinType",
                "[ -> bracket",
                "hash -> builtinType",
                ", -> punctuation",
                "embed -> builtinType",
                "] -> bracket",
                "= -> punctuation",
                "{ -> bracket",
                // an untyped entry - the key is a map key, not a field name
                "0x18563f21 -> number",
                "= -> punctuation",
                "VfxSystemDefinitionData -> class",
                "{ -> bracket",
                // fields of the meta class
                "particleName -> property",
                ": -> punctuation",
                "string -> builtinType",
                "= -> punctuation",
                "\"sparks\" -> string",
                "isSingleParticle -> property",
                ": -> punctuation",
                "flag -> builtinType",
                "= -> punctuation",
                "true -> boolean",
                "objectPath -> property",
                ": -> punctuation",
                "hash -> builtinType",
                "= -> punctuation",
                "0x18563f21 -> number",
                // an unresolved (hashed) field name is still a field name
                "0x6d6b7c10 -> property",
                ": -> punctuation",
                "f32 -> builtinType",
                "= -> punctuation",
                "0.5 -> number",
                "} -> bracket",
                "} -> bracket",
            ]
        );
    }
}
