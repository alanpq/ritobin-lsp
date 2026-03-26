use std::{
    sync::{Arc, atomic::AtomicI32},
    time::{Duration, Instant},
};

use anyhow::bail;
use lsp_server::RequestId;
use lsp_types::{
    DocumentChanges, DocumentFormattingParams, FormattingOptions, Hover, HoverContents,
    PartialResultParams, Position, Range, SemanticTokens, SemanticTokensParams,
    SemanticTokensRangeParams, TextDocumentContentChangeEvent, TextEdit, Url,
    WorkDoneProgressParams,
};
use ltk_ritobin::{Cst, cst::visitor::VisitorExt as _, print::PrintConfig};
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
pub enum Message {
    HoverRequest {
        id: RequestId,
        position: PositionOrRange,
        work_done_progress_params: WorkDoneProgressParams,
    },
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

        let txt = match cst.find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1)) {
            Some((node, tok)) => {
                let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                format!("{txt:?} | {node:?} | {:?}", tok.kind)
            }
            None => "".into(),
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
