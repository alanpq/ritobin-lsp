use lsp_types::{Position, Range};
use ltk_ritobin::{
    cst::{
        Node, NodeId, TokenId, TreeKind, Visitor,
        visitor::{Visit, VisitCtx},
    },
    parse::{Span, TokenKind},
};
use ritobin_lsp::line_ends::LineNumbers;

use crate::lsp::semantic_tokens::{self, builder::SemanticTokensBuilder, types::idx};

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

    /// [`Visit::Skip`] if nothing within `span` can be part of a ranged request, so the whole
    /// subtree is stepped over instead of walked token by token.
    fn prune(&self, span: Span) -> Visit {
        match self.range {
            Some(range) if span.end <= range.start => Visit::Skip,
            _ => Visit::Continue,
        }
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
            return self.prune(tree.span);
        }
        if matches!(tree.kind, TreeKind::Entry) {
            self.entry_typed.push(has_type_expr(ctx, tree));
        }
        self.stack.push(tree.kind);
        self.prune(tree.span)
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

        if let Some(range) = self.range {
            if token.span.start >= range.end {
                return Visit::Stop;
            }
            if !token.span.intersects(&range) {
                return Visit::Continue;
            }
        }
        let last_tree = self.stack.last().unwrap();
        // tracing::debug!(
        //     "{:?} ({:?}) | last tree: {last_tree:?}",
        //     token.kind,
        //     &self.text[token.span.start as usize..token.span.end as usize],
        // );

        use TokenKind::*;
        let token_type = match (last_tree, token.kind) {
            (_, Comment) => idx::COMMENT,
            (_, Colon | Comma | Eq) => idx::PUNCTUATION,
            (_, RCurly | LCurly | RBrack | LBrack) => idx::BRACKET,

            // built-in bin types - `string`, `f32`, `hash`, and the args of `list[..]`/`map[..]`
            (TreeKind::TypeExpr | TreeKind::TypeArg | TreeKind::TypeArgList, _) => {
                idx::BUILTIN_TYPE
            }

            // meta class names - `VfxSystemDefinitionData { .. }`
            (TreeKind::Class, _) => idx::CLASS,

            // field names - `particleName: string = ".."`. Keys of untyped entries are map keys
            // rather than fields, so those keep their literal highlighting.
            (TreeKind::EntryKey, Name) => idx::PROPERTY,
            (TreeKind::EntryKey, HexLit) if self.in_typed_entry() => idx::PROPERTY,

            (_, True) | (_, False) => idx::BOOLEAN,
            (_, Null) => idx::KEYWORD,
            (_, Name) => idx::KEYWORD,
            (_, Quote) | (_, String) | (_, UnterminatedString) => idx::STRING,
            (_, Number) | (_, HexLit) => idx::NUMBER,
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
                token_type,
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
        highlight_range(src, None)
    }

    /// [`highlight`], restricted to `range` as a `textDocument/semanticTokens/range` request would.
    fn highlight_range(src: &str, range: Option<Range>) -> Vec<String> {
        decode(src, range)
            .into_iter()
            .map(|(span, ty)| format!("{} -> {ty}", &src[span]))
            .collect()
    }

    /// Run the visitor and undo the relative encoding, giving `(span, token type)` per token.
    ///
    /// Assumes no token spans a line break (only an unterminated string can), so that one
    /// emitted token is one source token.
    fn decode(src: &str, range: Option<Range>) -> Vec<(Span, &'static str)> {
        let line_nums = LineNumbers::new(src);
        let cst = Cst::parse(src);
        let visitor = SemanticVisitor {
            text: src,
            line_nums: &line_nums,
            builder: SemanticTokensBuilder::new("test".to_owned()),
            stack: Vec::new(),
            entry_typed: Vec::new(),
            range: range.as_ref().map(|range| line_nums.from_range(range)),
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

                let from = line_nums.byte_index(line, start);
                let ty = SUPPORTED_TYPES[token.token_type as usize].as_str();
                (Span::new(from, from + token.length), ty)
            })
            .collect()
    }

    /// 0: entries: map[hash, embed] = {
    /// 1:     0x18563f21 = VfxSystemDefinitionData {
    /// 2:         particleName: string = "sparks"
    /// 3:         isSingleParticle: flag = true
    /// 4:         objectPath: hash = 0x18563f21
    /// 5:         0x6d6b7c10: f32 = 0.5
    /// 6:     }
    /// 7: }
    const SRC: &str = "entries: map[hash, embed] = {\n    \
                           0x18563f21 = VfxSystemDefinitionData {\n        \
                               particleName: string = \"sparks\"\n        \
                               isSingleParticle: flag = true\n        \
                               objectPath: hash = 0x18563f21\n        \
                               0x6d6b7c10: f32 = 0.5\n    \
                           }\n\
                       }\n";

    #[test]
    fn highlights_fields_types_and_classes() {
        let tokens = highlight(SRC);

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

    #[test]
    fn range_request_yields_only_tokens_in_range() {
        let tokens = highlight_range(
            SRC,
            Some(Range::new(Position::new(3, 0), Position::new(5, 0))),
        );

        assert_eq!(
            tokens,
            [
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
            ]
        );

        // the pruned walk must agree with the unpruned one on what it does emit
        let full = highlight(SRC);
        assert!(
            full.windows(tokens.len()).any(|w| w == tokens),
            "ranged tokens are not a contiguous slice of the full tokens"
        );
    }

    /// A ranged request must return exactly the tokens a full request would, filtered to the
    /// range - the subtree pruning in `enter_tree` must never drop one.
    ///
    /// This is load bearing: a tree's `span.start` can sit *after* tokens the tree contains, so
    /// the obvious `!span.intersects(range)` prune silently loses tokens (a `Block` whose start
    /// is anchored at its closing brace drops its opening one). Sources below cover the shapes
    /// that provoke it - lists of classes, comments, and error recovery.
    #[test]
    fn ranged_requests_match_the_full_walk() {
        const SAMPLES: &[&str] = &[
            SRC,
            // nested blocks + a list of classes - the shape with mis-anchored `Block` spans
            "entries: map[hash, embed] = {\n    0x1 = A {\n        l: list[pointer] = {\n            \
                 B { a: f32 = 1 }\n            C { b: f32 = 2 }\n            D { }\n        }\n        \
                 after: u8 = 9\n    }\n    0x2 = E { }\n}\n",
            // comments, including the ones the parser re-opens trees around
            "# leading\ntype: string = \"PROP\" # trailing\n# between\nentries: map[hash, embed] = {\n    \
                 # inside\n    0xaa = A {\n        # deep\n        x: f32 = 1 # after value\n    }\n}\n",
            // error recovery - junk, a bare `0x`, a missing type
            "type: string = \"PROP\"\nentries: map[hash, embed] = {\n    0x = Broken {\n        \
                 oops: = 3\n        name string = \"x\"\n        trailing: f32 = 1 !!! junk\n    }\n}\n\
             dangling\n{ }\n: = ,\n",
            "\n\n\n",
            "",
        ];

        // `Span` isn't `PartialEq`, so compare on the raw offsets
        fn cmp(tokens: Vec<(Span, &'static str)>) -> Vec<(u32, u32, &'static str)> {
            tokens
                .into_iter()
                .map(|(span, ty)| (span.start, span.end, ty))
                .collect()
        }

        for src in SAMPLES {
            let full = decode(src, None);
            let lines = src.lines().count() as u32;

            for start in 0..=lines + 1 {
                for end in start..=lines + 1 {
                    for (sc, ec) in [(0, 0), (3, 7), (0, 999)] {
                        let range = Range::new(Position::new(start, sc), Position::new(end, ec));
                        let span = LineNumbers::new(src).from_range(&range);

                        let expected: Vec<_> = full
                            .iter()
                            .copied()
                            .filter(|(tok, _)| tok.intersects(&span))
                            .collect();

                        assert_eq!(
                            cmp(decode(src, Some(range))),
                            cmp(expected),
                            "range {start}:{sc}..{end}:{ec} of {src:?}"
                        );
                    }
                }
            }
        }
    }
}
