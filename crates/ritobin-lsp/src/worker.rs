use std::{
    sync::{Arc, atomic::AtomicI32},
    time::{Duration, Instant},
};

use anyhow::bail;
use lsp_server::RequestId;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, DocumentChanges,
    DocumentFormattingParams, FormattingOptions, Hover, HoverContents, PartialResultParams,
    Position, Range, SemanticTokens, SemanticTokensParams, SemanticTokensRangeParams,
    TextDocumentContentChangeEvent, TextEdit, Url, WorkDoneProgressParams,
};
use ltk_hash::fnv1a;
use ltk_ritobin::{
    Cst,
    cst::{
        Kind as TreeKind, Visitor,
        visitor::{Visit, VisitorExt as _},
    },
    parse::{Span, Token},
    print::PrintConfig,
};
use ritobin_lsp::{cst_ext::CstExt as _, line_ends::LineNumbers};
use similar::{DiffOp, TextDiff};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    document::Document,
    lsp::{
        ext::{HoverParams, PositionOrRange},
        semantic_tokens::builder::SemanticTokensBuilder,
    },
    server::Server,
    worker::semantic_tokens::SemanticVisitor,
};

pub mod diagnostics;
pub mod semantic_tokens;

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

pub struct Worker {
    rx: mpsc::Receiver<Message>,
    document: Document,
    bin: Option<(Cst, ltk_meta::Bin)>,
    server: Arc<Server>,
}

impl Worker {
    pub fn spawn(server: Arc<Server>, uri: Url, version: i32, text: String) -> WorkerHandle {
        let (tx, rx) = mpsc::channel(1024);
        WorkerHandle {
            tx,
            handle: tokio::spawn(async move {
                let mut worker = Self {
                    rx,
                    bin: None,
                    document: Document::new(uri, version, text),
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
        let cst = Cst::parse(&self.document.text);
        let (bin, errors) = cst.build_bin(&self.document.text);
        let _ = self.publish_parse_errors(&cst, errors);
        self.bin.replace((cst, bin));
    }

    pub async fn service(mut self) -> anyhow::Result<()> {
        while let Some(req) = self.rx.recv().await {
            // TODO: propagate err to lsp client instead of killing worker
            tracing::debug!("[worker] got req: {req:#?}");
            match req {
                Message::HoverRequest {
                    id,
                    position,
                    work_done_progress_params,
                } => {
                    if let Some(res) = self.hover(position, work_done_progress_params)? {
                        let _ = self.server.send_ok(id, &res);
                    }
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
        let Some((cst, _)) = self.bin.as_ref() else {
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
        .walk(cst);

        Ok(Some(visitor.builder.build()))
    }

    fn complete(&self, req: CompletionRequest) -> anyhow::Result<Option<CompletionResponse>> {
        let doc = &self.document;
        let Some((cst, bin)) = self.bin.as_ref() else {
            return Ok(None);
        };

        let class = ClassFinder::new(
            doc.line_numbers.from_position(&req.position),
            doc.text.clone(),
        )
        .walk(cst);

        let classes = self.server.meta.classes.read();
        let Some(class) = class
            .class_stack
            .last()
            .map(|(_, class)| fnv1a::hash_lower(&doc.text.as_str()[class]))
            .and_then(|hash| classes.get(&hash.into()))
        else {
            return Ok(None);
        };

        let properties = class.properties.iter().map(|(k, prop)| CompletionItem {
            label: k.to_string(),
            label_details: Some(lsp_types::CompletionItemLabelDetails {
                detail: Some(format!(": {}", prop.rito_type())),
                description: None,
            }),
            kind: Some(CompletionItemKind::PROPERTY),
            ..Default::default()
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
        let Some((cst, bin)) = self.bin.as_ref() else {
            return Ok(None);
        };

        let class =
            ClassFinder::new(doc.line_numbers.from_position(pos), doc.text.clone()).walk(cst);
        let classes = self.server.meta.classes.read();
        let class_hash = class
            .class_stack
            .last()
            .map(|(_, class)| (class, fnv1a::hash_lower(&doc.text.as_str()[class])));

        let txt = match class_hash {
            Some((name, hash)) => {
                let class = classes.get(&hash.into());
                let properties = class.map(|c| &c.properties);
                format!(
                    "class ({}/{hash}): {properties:#?}",
                    &doc.text.as_str()[*name]
                )
            }
            None => match cst.find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1)) {
                Some((node, tok)) => {
                    let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                    format!("{txt:?} | {node:?} | {:?}", tok.kind)
                }
                None => "".into(),
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
            contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(txt)),
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
        let Some((cst, _)) = self.bin.as_ref() else {
            return Ok(None);
        };
        let mut formatted = String::new();
        ltk_ritobin::print::CstPrinter::new(&doc.text, &mut formatted, PrintConfig::default())
            .print(cst)
            .unwrap();

        Ok(Some(diff_to_textedits(&doc.text, &formatted)))
    }
}

fn diff_to_textedits(original: &str, formatted: &str) -> Vec<TextEdit> {
    if original == formatted {
        return Vec::new();
    }

    let diff = TextDiff::configure()
        .algorithm(similar::Algorithm::Lcs)
        .deadline(Instant::now() + Duration::from_secs(1))
        .diff_lines(original, formatted);
    let mut edits = Vec::new();

    for group in diff.grouped_ops(3) {
        let mut old_start = usize::MAX;
        let mut old_end = 0;
        let mut new_start = usize::MAX;
        let mut new_end = 0;

        for op in group {
            let o = op.old_range();
            let n = op.new_range();

            old_start = old_start.min(o.start);
            old_end = old_end.max(o.end);

            new_start = new_start.min(n.start);
            new_end = new_end.max(n.end);
        }

        let fmt_lines: Vec<&str> = formatted.lines().collect();
        let replacement = fmt_lines[new_start..new_end]
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>();

        edits.push(TextEdit {
            range: Range {
                start: Position::new(old_start as u32, 0),
                end: Position::new(old_end as u32, 0),
            },
            new_text: replacement,
        });
    }

    edits
}

struct ClassFinder {
    stack: Vec<TreeKind>,
    offset: u32,
    class_depth: usize,
    text: String,
    pub found_token: Option<Token>,
    pub class_stack: Vec<(usize, Span)>,
}

impl ClassFinder {
    pub fn new(offset: u32, text: String) -> Self {
        Self {
            stack: Vec::new(),
            text,
            offset,
            class_depth: 0,
            found_token: None,
            class_stack: vec![],
        }
    }
}

impl Visitor for ClassFinder {
    fn visit_token(&mut self, token: &Token, _context: &Cst) -> Visit {
        if token.span.contains(self.offset) {
            self.found_token.replace(*token);
            return Visit::Stop;
        }

        Visit::Continue
    }

    fn enter_tree(&mut self, tree: &Cst) -> Visit {
        if tree.kind == TreeKind::Class {
            if let Some(c) = tree.children.first().map(|c| c.span()) {
                self.class_stack.push((self.stack.len(), c));
                eprintln!("-> {}: {:?}", self.class_depth, &self.text.as_str()[c]);
            }
        }
        self.stack.push(tree.kind);
        Visit::Continue
    }
    fn exit_tree(&mut self, tree: &Cst) -> Visit {
        if let Some(taken) = self
            .class_stack
            .pop_if(|(depth, _)| self.stack.len() == *depth)
        {
            eprintln!(
                "<- {}: {:?} ({})",
                self.stack.len(),
                &self.text.as_str()[taken.1],
                tree.kind
            );
        }
        self.stack.pop();
        Visit::Continue
    }
}
