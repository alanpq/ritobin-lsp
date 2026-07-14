use std::{fmt::Write as _, sync::Arc};

use imara_diff::{Algorithm, Diff, InternedInput};
use lsp_server::RequestId;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, FormattingOptions,
    Hover, MarkedString, MarkupContent, MarkupKind, PartialResultParams, Position, Range,
    SemanticTokens, TextDocumentContentChangeEvent, TextEdit, Url, WorkDoneProgressParams,
};
use ltk_hash::{BinHash, Hash};
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    Cst,
    cst::{
        Kind as TreeKind, NodeId, TokenId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt as _},
    },
    parse::{Span, Token},
    print::PrintConfig,
    typecheck::diagnostics::DiagnosticWithSpan,
};
use ritobin_lsp::cst_ext::CstExt as _;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    document::Document,
    lol_meta::schema::U32Hash,
    lsp::{ext::PositionOrRange, semantic_tokens::builder::SemanticTokensBuilder},
    server::Server,
    worker::semantic_tokens::SemanticVisitor,
};

pub mod diagnostics;
pub mod semantic_tokens;
pub mod unhash;

#[derive(Debug)]
pub struct CompletionRequest {
    pub id: RequestId,
    pub position: Position,
    pub work_done_progress_params: WorkDoneProgressParams,
    pub partial_result_params: PartialResultParams,
    pub context: Option<CompletionContext>,
}

#[derive(Debug)]
pub enum Message {
    UnhashRequest {
        id: RequestId,
        range: Option<Range>,
    },
    HoverRequest {
        id: RequestId,
        position: PositionOrRange,
        work_done_progress_params: WorkDoneProgressParams,
    },
    CompletionRequest(CompletionRequest),
    FormatRequest {
        id: RequestId,
        options: FormattingOptions,
        work_done_progress_params: WorkDoneProgressParams,
    },

    SemanticTokens {
        id: RequestId,
        work_done_progress_params: WorkDoneProgressParams,
        partial_result_params: PartialResultParams,
        range: Option<Range>,
    },

    DocumentChange {
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    },
}

pub struct WorkerHandle {
    pub tx: mpsc::Sender<Message>,
    handle: JoinHandle<()>,
}

struct ParseData {
    cst: Cst,
    bin: ltk_meta::Bin,
    errors: Vec<DiagnosticWithSpan>,
}

impl ParseData {
    pub fn parse(text: &str) -> Self {
        let cst = Cst::parse(text);
        let (bin, errors) = cst.build_bin(text);
        Self { cst, bin, errors }
    }
}

pub struct Worker {
    rx: mpsc::Receiver<Message>,
    document: Document,
    data: Option<ParseData>,
    server: Arc<Server>,
}

impl Worker {
    pub fn spawn(server: Arc<Server>, uri: Url, version: i32, text: String) -> WorkerHandle {
        let (tx, rx) = mpsc::channel(1024);
        WorkerHandle {
            tx,
            handle: tokio::spawn(async move {
                // tracing::debug!("[worker] '{uri}' spawning...");
                let mut worker = Self {
                    rx,
                    document: Document::new(uri, version, text),
                    data: None,
                    server,
                };
                worker.update();

                if let Err(e) = worker.service().await {
                    tracing::error!("document worker error: {e:?}");
                }
            }),
        }
    }

    fn update(&mut self) {
        tracing::debug!("[worker] '{}' update", self.document.uri);
        let mut data = ParseData::parse(&self.document.text);
        let _ = self.publish_parse_errors(&data.cst, data.errors.drain(..));
        self.data.replace(data);
    }

    pub async fn service(mut self) -> anyhow::Result<()> {
        tracing::debug!("[worker] '{}' started", self.document.uri);
        while let Some(req) = self.rx.recv().await {
            // TODO: propagate err to lsp client instead of killing worker
            match req {
                Message::UnhashRequest { id, range } => {
                    let _ = self
                        .server
                        .send_ok(id, &self.unhash(range)?.unwrap_or_default());
                }
                Message::HoverRequest {
                    id,
                    position,
                    work_done_progress_params,
                } => {
                    let res = self
                        .hover(position, work_done_progress_params)?
                        .unwrap_or_else(|| Hover {
                            contents: lsp_types::HoverContents::Scalar(MarkedString::String(
                                String::new(),
                            )),
                            range: None,
                        });
                    let _ = self.server.send_ok(id, &res);
                }
                Message::CompletionRequest(req) => {
                    let _ = self.server.send_ok(
                        req.id.clone(),
                        &self
                            .complete(req)?
                            .unwrap_or_else(|| CompletionResponse::Array(vec![])),
                    );
                }
                Message::FormatRequest {
                    id,
                    options,
                    work_done_progress_params,
                } => {
                    if let Some(res) = self.format(options, work_done_progress_params)? {
                        let _ = self.server.send_ok(id, &res);
                    }
                }
                Message::SemanticTokens {
                    id,
                    work_done_progress_params,
                    partial_result_params,
                    range,
                } => {
                    if let Some(res) = self.semantic_tokens(
                        work_done_progress_params,
                        partial_result_params,
                        range,
                    )? {
                        let _ = self.server.send_ok(id, &res);
                    }
                }
                Message::DocumentChange { version, changes } => {
                    self.document.update(version, changes);
                    self.update();
                }
            }
        }
        Ok(())
    }

    fn semantic_tokens(
        &self,
        _work_done_progress_params: WorkDoneProgressParams,
        _partial_result_params: PartialResultParams,
        range: Option<Range>,
    ) -> anyhow::Result<Option<SemanticTokens>> {
        let doc = &self.document;
        let Some(data) = self.data.as_ref() else {
            return Ok(None);
        };

        let builder = SemanticTokensBuilder::new(doc.uri.to_string());
        let visitor = SemanticVisitor {
            text: &doc.text,
            line_nums: &doc.line_numbers,
            stack: Vec::new(),
            range: range
                .as_ref()
                .map(|range| doc.line_numbers.from_range(range)),
            builder,
        }
        .walk(&data.cst);

        Ok(Some(visitor.builder.build()))
    }

    fn complete(&self, req: CompletionRequest) -> anyhow::Result<Option<CompletionResponse>> {
        let doc = &self.document;
        let Some(data) = self.data.as_ref() else {
            return Ok(None);
        };

        let class = ClassFinder::new(
            doc.line_numbers.from_position(&Position::new(
                req.position.line,
                req.position.character + 1,
            )),
            doc.text.clone(),
        )
        .walk(&data.cst);

        let classes = self.server.meta.classes.read();
        let Some((name, class)) = class
            .class_stack
            .last()
            .map(|(_, class)| {
                (
                    &doc.text.as_str()[class],
                    BinHash::hash_str(&doc.text.as_str()[class]),
                )
            })
            .and_then(|(name, hash)| Some((name, classes.get(hash)?)))
        else {
            return Ok(None);
        };

        tracing::debug!("-> {name}");

        let bin_fields = self
            .server
            .hashes
            .as_ref()
            .and_then(|h| h.table(Table::BinFields));
        if bin_fields.is_none() {
            tracing::error!("NO HASHES");
        }

        let properties = class.properties.iter().map(|(k, prop)| {
            let label = bin_fields.as_ref().and_then(|h| h.get((**k).into()));
            let type_part = format!(": {}", prop.rito_type());
            CompletionItem {
                insert_text: Some(format!(
                    "{}{type_part} = ",
                    label.clone().unwrap_or_else(|| k.to_string().into())
                )),
                sort_text: Some(
                    label
                        .clone()
                        .unwrap_or_else(|| format!("XXX{:x}", **k).into())
                        .into(),
                ),
                label: label.unwrap_or_else(|| k.to_string().into()).into(),
                label_details: Some(lsp_types::CompletionItemLabelDetails {
                    detail: Some(type_part),
                    description: None,
                }),
                kind: Some(CompletionItemKind::PROPERTY),
                ..Default::default()
            }
        });
        Ok(Some(CompletionResponse::Array(properties.collect())))
    }

    fn hover(
        &self,
        position: PositionOrRange,
        _work_done_progress_params: WorkDoneProgressParams,
    ) -> anyhow::Result<Option<Hover>> {
        let pos = position.start();
        let doc = &self.document;
        let Some(data) = self.data.as_ref() else {
            return Ok(None);
        };

        let finder =
            ClassFinder::new(doc.line_numbers.from_position(pos), doc.text.clone()).walk(&data.cst);
        let classes = self.server.meta.classes.read();
        let class_name = finder
            .class_stack
            .last()
            .map(|(_, class)| (class, BinHash::hash_str(&doc.text.as_str()[class])));

        let markup = match class_name {
            Some((class_name_span, class_hash)) => {
                let class_name = &doc.text.as_str()[*class_name_span];
                let class = classes.get(class_hash);

                MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: match finder.found_token {
                        Some((token, TreeKind::EntryKey)) => {
                            let txt = &doc.text.as_str()[token.span];
                            let hash = BinHash::hash_str(txt);
                            match classes.find_property(class_hash, hash) {
                                Some(prop) => {
                                    format!(
                                        r#"### [{class_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)

`{txt}`: `{}`

`0x{:>08x}`

*No documentation available.*
"#,
                                        class_name.to_ascii_lowercase(),
                                        prop.rito_type(),
                                        hash,
                                    )
                                }
                                None => format!("{txt}: ??"),
                            }
                        }
                        Some((_token, TreeKind::Class)) => match class {
                            Some(class) => {
                                let mut str = format!(
                                    "[{class_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/) (`0x{:>08x}`)\n\n",
                                    class_name.to_ascii_lowercase(),
                                    class_hash,
                                );

                                let mut base = Some((U32Hash::from(class_hash), class));
                                let mut d = 0;
                                let bin_types = self
                                    .server
                                    .hashes
                                    .as_ref()
                                    .and_then(|hashes| hashes.table(Table::BinTypes));
                                while let Some((hash, class)) = base {
                                    if d > 0 {
                                        let base_name = bin_types
                                            .as_ref()
                                            .and_then(|h| h.get((*hash).into()))
                                            .unwrap_or_else(|| hash.to_string().into());
                                        writeln!(
                                            str,
                                            "{}└─ [{base_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)\n",
                                            "\u{00A0}".repeat(d - 1),
                                            base_name.to_ascii_lowercase()
                                        )?;
                                    }
                                    d += 1;
                                    base = class.base.and_then(|b| Some((b, classes.get(b)?)));
                                }

                                str
                            }
                            None => format!("*Unknown class `{class_name}`*"),
                        },
                        _ => {
                            match data
                                .cst
                                .find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1))
                            {
                                Some((node, tok)) => {
                                    let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                                    format!("{txt:?} | {node:?} | {:?}", tok.kind)
                                }
                                None => "".into(),
                            }
                        }
                    },
                }
            }
            None => MarkupContent {
                kind: lsp_types::MarkupKind::PlainText,
                value: match data
                    .cst
                    .find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1))
                {
                    Some((node, tok)) => {
                        let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                        format!("{txt:?} | {node:?} | {:?}", tok.kind)
                    }
                    None => "".into(),
                },
            },
        };

        // let txt = match cst.find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1)) {
        //     Some((node, tok)) => {
        //         let txt = &doc.text[tok.span.start as _..tok.span.end as _];
        //         format!("{txt:?} | {node:?} | {:?}", tok.kind)
        //     }
        //     None => "".into(),
        // };
        Ok(Some(Hover {
            contents: lsp_types::HoverContents::Markup(markup),
            range: None,
        }))
    }

    fn format(
        &mut self,
        _options: FormattingOptions,
        _work_done_progress_params: WorkDoneProgressParams,
    ) -> anyhow::Result<Option<Vec<TextEdit>>> {
        let doc = &self.document;
        if doc.text.len() > (10 * (2 << 20)) {
            // TODO: propagate this
            // server.send_err(
            //     req.id.clone(),
            //     lsp_server::ErrorCode::RequestFailed,
            //     "File too big to format.",
            // )?;
            tracing::error!("file too big to format!");
            return Ok(None);
        }
        let Some(data) = self.data.as_ref() else {
            return Ok(None);
        };
        let mut formatted = String::new();
        ltk_ritobin::print::CstPrinter::new(&doc.text, &mut formatted, PrintConfig::default())
            .print(&data.cst)
            .unwrap();

        Ok(Some(diff_to_textedits(&doc.text, &formatted)))
    }
}

fn diff_to_textedits(original: &str, formatted: &str) -> Vec<TextEdit> {
    if original == formatted {
        return Vec::new();
    }

    let input = InternedInput::new(original, formatted);
    let mut diff = Diff::compute(Algorithm::Myers, &input);
    diff.postprocess_lines(&input);

    diff.hunks()
        .map(|hunk| TextEdit {
            range: Range {
                start: Position::new(hunk.before.start, 0),
                end: Position::new(hunk.before.end, 0),
            },
            new_text: (hunk.after.start..hunk.after.end)
                .map(|idx| input.interner[input.after[idx as usize]])
                .collect(),
        })
        .collect()
}

struct ClassFinder {
    stack: Vec<TreeKind>,
    offset: u32,
    text: String,
    pub found_token: Option<(Token, TreeKind)>,
    pub class_stack: Vec<(usize, Span)>,
}

impl ClassFinder {
    pub fn new(offset: u32, text: String) -> Self {
        Self {
            stack: Vec::new(),
            text,
            offset,
            found_token: None,
            class_stack: vec![],
        }
    }
}

impl Visitor for ClassFinder {
    fn visit_token(&mut self, ctx: &VisitCtx, token: TokenId, parent: NodeId) -> Visit {
        let token = ctx.cst.token(token).unwrap();
        if token.span.contains(self.offset) {
            let parent = ctx.node(parent).unwrap();
            self.found_token.replace((*token, parent.kind));
            return Visit::Stop;
        }

        Visit::Continue
    }

    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let tree = ctx.node(node).unwrap();
        if tree.span.start > self.offset {
            return Visit::Stop;
        }
        if tree.kind == TreeKind::Class
            && let Some(c) = tree.children.get(ctx.cst).first().map(|c| c.span(ctx.cst))
        {
            self.class_stack.push((self.stack.len(), c));
            // eprintln!("-> {}: {:?}", self.stack.len(), &self.text.as_str()[c]);
        }
        self.stack.push(tree.kind);
        Visit::Continue
    }
    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let tree = ctx.node(node).unwrap();
        if tree.span.end > self.offset {
            return Visit::Stop;
        }
        if let Some(_taken) = self
            .class_stack
            .pop_if(|(depth, _)| self.stack.len() == *depth)
        {
            // eprintln!(
            //     "<- {}: {:?} ({})",
            //     self.stack.len(),
            //     &self.text.as_str()[taken.1],
            //     tree.kind
            // );
        }
        self.stack.pop();
        Visit::Continue
    }
}
