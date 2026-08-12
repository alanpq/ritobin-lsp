use std::{fmt::Write as _, sync::Arc};

use imara_diff::{Algorithm, Diff, InternedInput};
use lsp_server::RequestId;
use lsp_types::{
    CompletionContext, CompletionResponse, FormattingOptions, Hover, MarkedString, MarkupContent,
    MarkupKind, PartialResultParams, Position, Range, SemanticTokens,
    TextDocumentContentChangeEvent, TextEdit, Url, WorkDoneProgressParams,
};
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    Cst,
    cst::{Kind as TreeKind, visitor::VisitorExt as _},
    print::PrintConfig,
    typecheck::diagnostics::DiagnosticWithSpan,
};
use ritobin_lsp::{
    cst_ext::CstExt as _,
    scope::{self, ClassContextExt as _, CstExt, TokenExt},
};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    document::Document,
    lol_meta::schema::U32Hash,
    lsp::{ext::PositionOrRange, semantic_tokens::builder::SemanticTokensBuilder},
    server::Server,
    worker::semantic_tokens::SemanticVisitor,
};

pub mod completion;
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

        // Continually drain any queued changes
        let mut pending: Option<Message> = None;
        loop {
            let req = match pending.take() {
                Some(req) => req,
                None => match self.rx.recv().await {
                    Some(req) => req,
                    None => break,
                },
            };

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
                    pending = drain_changes(&mut self.rx, &mut self.document);
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
            entry_typed: Vec::new(),
            range: range
                .as_ref()
                .map(|range| doc.line_numbers.from_range(range)),
            builder,
        }
        .walk(&data.cst);

        Ok(Some(visitor.builder.build()))
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

        let offset = doc.line_numbers.from_position(pos);
        let scope = data
            .cst
            .class_context_at(offset, &doc.text)
            .current()
            .copied();
        let found_token = data
            .cst
            .find_node(offset)
            .and_then(|(kinds, token)| Some((token, *kinds.last()?)));

        let classes = self.server.meta.classes.read();

        let markup = match scope.zip(scope.and_then(|s| s.hash)) {
            Some((scope, class_hash)) => {
                let class_name_span = scope.token.span;
                let class_name = &doc.text[class_name_span];
                let class = classes.get(class_hash);

                MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: match found_token {
                        Some((token, TreeKind::EntryKey)) => {
                            let txt = &doc.text.as_str()[token.span];
                            match token.as_bin_hash(&doc.text).and_then(|hash| {
                                Some((hash, classes.find_property(class_hash, hash)?))
                            }) {
                                Some((hash, prop)) => {
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
                        _ => match data.cst.find_node(offset) {
                            Some((node, tok)) => {
                                let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                                format!("{txt:?} | {node:?} | {:?}", tok.kind)
                            }
                            None => "".into(),
                        },
                    },
                }
            }
            None => MarkupContent {
                kind: lsp_types::MarkupKind::PlainText,
                value: match data.cst.find_node(offset) {
                    Some((node, tok)) => {
                        let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                        format!("{txt:?} | {node:?} | {:?}", tok.kind)
                    }
                    None => "".into(),
                },
            },
        };

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

/// Drains the channel of all [`Message::DocumentChange`] messages, applying them to the document in a batch
/// Returns the first non-change message
fn drain_changes(rx: &mut mpsc::Receiver<Message>, document: &mut Document) -> Option<Message> {
    loop {
        match rx.try_recv() {
            Ok(Message::DocumentChange { version, changes }) => document.update(version, changes),
            Ok(msg) => return Some(msg),
            Err(_) => return None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        Document::new(Url::parse("file:///test.rito").unwrap(), 0, text.to_owned())
    }

    fn keystroke(version: i32, line: u32, character: u32, text: &str) -> Message {
        Message::DocumentChange {
            version,
            changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(
                    Position::new(line, character),
                    Position::new(line, character),
                )),
                range_length: None,
                text: text.to_owned(),
            }],
        }
    }

    #[test]
    fn a_run_of_changes_drains_in_order() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut doc = document("");
        for (i, text) in ["a", "b", "c"].into_iter().enumerate() {
            tx.try_send(keystroke(i as i32 + 1, 0, i as u32, text))
                .unwrap();
        }

        assert!(drain_changes(&mut rx, &mut doc).is_none());
        assert_eq!(doc.text, "abc");
        assert_eq!(doc.version, 3);
    }

    #[test]
    fn draining_stops_at_a_request_and_hands_it_back() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut doc = document("");
        tx.try_send(keystroke(1, 0, 0, "a")).unwrap();
        tx.try_send(Message::UnhashRequest {
            id: 1.into(),
            range: None,
        })
        .unwrap();
        tx.try_send(keystroke(2, 0, 1, "b")).unwrap();

        let pending = drain_changes(&mut rx, &mut doc);

        assert!(matches!(pending, Some(Message::UnhashRequest { .. })));
        // the text that preceded it
        assert_eq!(doc.text, "a");
        // the change behind it is still queued
        assert!(matches!(
            rx.try_recv(),
            Ok(Message::DocumentChange { version: 2, .. })
        ));
    }

    #[test]
    fn draining_an_empty_channel_is_a_no_op() {
        let (_tx, mut rx) = mpsc::channel(8);
        let mut doc = document("x");

        assert!(drain_changes(&mut rx, &mut doc).is_none());
        assert_eq!(doc.text, "x");
    }
}
