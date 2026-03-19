use anyhow::{Result, anyhow};
use itertools::Itertools;
use lsp_server::Request as ServerRequest;
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, DocumentFormattingParams, Hover,
    HoverContents, MarkedString, Position, Range, SemanticTokensParams, SemanticTokensRangeParams,
    TextEdit,
    request::{
        Completion, Formatting, GotoDefinition, HoverRequest, SemanticTokensFullRequest,
        SemanticTokensRangeRequest,
    },
};
use ltk_ritobin::{
    cst::{
        Cst, TreeKind, Visitor,
        visitor::{Visit, VisitorExt},
    },
    parse::{Span, Token, TokenKind},
    print::PrintConfig,
};
use ritobin_lsp::{cst_ext::CstExt, line_ends::LineNumbers};

use crate::{
    lsp::{
        ext::HoverParams,
        semantic_tokens::{
            self,
            builder::{SemanticTokensBuilder, type_index},
        },
    },
    server::Server,
};

pub fn request(server: &Server, req: &ServerRequest) -> Result<()> {
    tracing::debug!(?req, "handle_request");
    match req.method.as_str() {
        GotoDefinition::METHOD => {
            server.send_ok(
                req.id.clone(),
                &lsp_types::GotoDefinitionResponse::Array(Vec::new()),
            )?;
        }
        Completion::METHOD => {
            let item = CompletionItem {
                label: "HelloFromLSP".into(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("dummy completion".into()),
                ..Default::default()
            };
            server.send_ok(req.id.clone(), &CompletionResponse::Array(vec![item]))?;
        }
        HoverRequest::METHOD => {
            let p: HoverParams = serde_json::from_value(req.params.clone())?;
            let pos = p.position.start();

            let docs = server.docs.read().unwrap();
            let doc = docs
                .get(&p.text_document.uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;

            let txt = match doc
                .cst
                .find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1))
            {
                Some((node, tok)) => {
                    let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                    format!("{txt:?} | {node:?} | {:?}", tok.kind)
                }
                None => "".into(),
            };

            let hover = Hover {
                contents: HoverContents::Scalar(MarkedString::String(txt)),
                range: None,
            };
            server.send_ok(req.id.clone(), &hover)?;
        }
        Formatting::METHOD => {
            let p: DocumentFormattingParams = serde_json::from_value(req.params.clone())?;
            let uri = p.text_document.uri;
            let docs = server.docs.read().unwrap();
            let doc = docs
                .get(&uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;
            // let formatted = run_rustfmt(text)?;

            // 10 MiB limit
            if doc.text.len() > (10 * (2 << 20)) {
                server.send_err(
                    req.id.clone(),
                    lsp_server::ErrorCode::RequestFailed,
                    "File too big to format.",
                )?;
                return Ok(());
            }

            let mut formatted = String::new();
            ltk_ritobin::print::CstPrinter::new(&doc.text, &mut formatted, PrintConfig::default())
                .print(&doc.cst)
                .unwrap();

            let edit = TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: doc
                        .line_numbers
                        .position(doc.text.len().try_into().unwrap()),
                },
                new_text: formatted,
            };
            server.send_ok(req.id.clone(), &vec![edit])?;
        }
        SemanticTokensRangeRequest::METHOD => {
            let p: SemanticTokensRangeParams = serde_json::from_value(req.params.clone())?;
            let builder = SemanticTokensBuilder::new(p.text_document.uri.to_string());
            let docs = server.docs.read().unwrap();
            let doc = docs
                .get(&p.text_document.uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;

            let range = doc.line_numbers.from_range(&p.range);

            let visitor = SemanticVisitor {
                text: &doc.text,
                line_nums: &doc.line_numbers,
                stack: Vec::new(),
                range: Some(range),
                builder,
            }
            .walk(&doc.cst);

            let tokens = visitor.builder.build();
            server.send_ok(req.id.clone(), &tokens)?;
        }
        SemanticTokensFullRequest::METHOD => {
            let p: SemanticTokensParams = serde_json::from_value(req.params.clone())?;
            let builder = SemanticTokensBuilder::new(p.text_document.uri.to_string());
            let docs = server.docs.read().unwrap();
            let doc = docs
                .get(&p.text_document.uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;

            let visitor = SemanticVisitor {
                text: &doc.text,
                line_nums: &doc.line_numbers,
                stack: Vec::new(),
                range: None,
                builder,
            }
            .walk(&doc.cst);

            let tokens = visitor.builder.build();
            server.send_ok(req.id.clone(), &tokens)?;
        }
        _ => server.send_err(
            req.id.clone(),
            lsp_server::ErrorCode::MethodNotFound,
            "unhandled method",
        )?,
    }
    Ok(())
}

struct SemanticVisitor<'a> {
    text: &'a str,
    line_nums: &'a LineNumbers,
    builder: SemanticTokensBuilder,
    stack: Vec<TreeKind>,
    range: Option<Span>,
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
