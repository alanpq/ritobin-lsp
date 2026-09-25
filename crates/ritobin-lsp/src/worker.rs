use std::{
    fmt::Display,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Duration,
};

use lsp_server::{Notification, RequestId};
use lsp_types::{
    Color, CompletionContext, CompletionResponse, Diagnostic, DocumentSymbolResponse,
    FormattingOptions, Hover, MarkedString, MessageType, PartialResultParams, Position, Range,
    ShowMessageParams, TextDocumentContentChangeEvent, Url, WorkDoneProgressParams,
    notification::ShowMessage, request::DocumentSymbolRequest,
};
use ltk_ritobin::{Cst, ast::Ast};
use serde::Serialize;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, sleep_until},
};

use crate::{
    document::Document,
    lsp::{ext::PositionOrRange, semantic_tokens::TokenCache},
    server::Server,
    worker::{code_actions::CodeActionData, symbols::Symbols},
};

pub mod code_actions;
pub mod color;
pub mod completion;
pub mod diagnostics;
pub mod semantic_tokens;
pub mod symbols;
pub mod unhash;

mod format;
mod hover;

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
    SymbolsRequest {
        id: RequestId,
        work_done_progress_params: WorkDoneProgressParams,
        partial_result_params: PartialResultParams,
    },
    CodeActionRequest {
        id: RequestId,
        range: Range,
        diagnostics: Vec<Diagnostic>,
        work_done_progress_params: WorkDoneProgressParams,
        partial_result_params: PartialResultParams,
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
        /// The `result_id` of the last response the client holds, if it is asking for a delta.
        previous_result_id: Option<String>,
    },

    DocumentColorRequest {
        id: RequestId,
        work_done_progress_params: WorkDoneProgressParams,
        partial_result_params: PartialResultParams,
    },
    ColorPresentationRequest {
        id: RequestId,
        color: Color,
        range: Range,
        work_done_progress_params: WorkDoneProgressParams,
        partial_result_params: PartialResultParams,
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

impl WorkerHandle {
    /// Ends the worker.
    pub fn shutdown(self) -> JoinHandle<()> {
        self.handle
    }
}

// TODO: Make this configurable ?
/// How long the document has to go quiet before we typecheck and lint it.
const DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_millis(250);

pub struct Parse {
    pub cst: Cst,
    pub ast: Ast,
}

pub enum ParsePanic {
    Cst(Box<dyn std::any::Any + Send>),
    Ast(Box<dyn std::any::Any + Send>),
}

impl Parse {
    pub fn new(text: &str) -> Result<Self, ParsePanic> {
        let cst = catch_unwind(AssertUnwindSafe(|| Cst::parse(text))).map_err(ParsePanic::Cst)?;
        let ast =
            catch_unwind(AssertUnwindSafe(|| cst.build_ast(text))).map_err(ParsePanic::Ast)?;
        Ok(Self { cst, ast })
    }
}

pub struct Worker {
    rx: mpsc::Receiver<Message>,
    server: Arc<Server>,

    document: Document,
    parse: Option<Parse>,
    tokens: TokenCache,
    code_action_data: Vec<CodeActionData>,

    /// Deadline for the next diagnostics pass. `None` when diagnostics are up to date.
    diagnostics_due: Option<Instant>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Unparsed(#[from] Unparsed),
}

#[derive(Debug, thiserror::Error)]
#[error("Document has not been parsed.")]
pub struct Unparsed;

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
                    parse: None,
                    server,
                    tokens: TokenCache::default(),
                    code_action_data: Vec::new(),
                    diagnostics_due: None,
                };
                if worker.reparse().is_some()
                    && let Err(e) = worker.service().await
                {
                    tracing::error!("document worker error: {e:?}");
                };
            }),
        }
    }

    pub fn parsed(&self) -> Result<&Parse, Unparsed> {
        self.parse.as_ref().ok_or(Unparsed)
    }
    pub fn parsed_mut(&mut self) -> Result<&mut Parse, Unparsed> {
        self.parse.as_mut().ok_or(Unparsed)
    }

    pub fn cst(&self) -> Result<&Cst, Unparsed> {
        self.parse.as_ref().map(|p| &p.cst).ok_or(Unparsed)
    }
    pub fn ast(&self) -> Result<&Ast, Unparsed> {
        self.parse.as_ref().map(|p| &p.ast).ok_or(Unparsed)
    }

    fn reparse(&mut self) -> Option<&Parse> {
        tracing::debug!("[worker] '{}' reparse", self.document.uri);
        // self.diagnostics_due = Some(Instant::now() + DIAGNOSTICS_DEBOUNCE);
        self.diagnostics_due.take();
        self.parse = Parse::new(&self.document.text)
            .inspect_err(|e| match e {
                ParsePanic::Cst(any) => {
                    tracing::error!(
                        "[worker] '{}' cst parse panicked - {any:?}",
                        self.document.uri
                    );
                }

                ParsePanic::Ast(any) => {
                    tracing::error!(
                        "[worker] '{}' ast build panicked - {any:?}",
                        self.document.uri
                    );
                }
            })
            .ok();

        let _ = self.lint_and_publish_parse_errors();

        self.parse.as_ref()
    }

    pub async fn service(mut self) -> anyhow::Result<()> {
        tracing::debug!("[worker] '{}' started", self.document.uri);

        // Continually drain any queued changes
        let mut pending: Option<Message> = None;
        loop {
            let req = match pending.take() {
                Some(req) => req,
                None => match wake(&mut self.rx, self.diagnostics_due).await {
                    Wake::Message(req) => req,
                    Wake::Quiet => {
                        self.reparse();
                        continue;
                    }
                    Wake::Closed => break,
                },
            };

            match self.respond(req).await {
                Ok(next) => pending = next,
                Err(e) => {
                    tracing::error!("[worker] '{}' request failed: {e:?}", self.document.uri)
                }
            }
        }
        Ok(())
    }

    fn send_result<T, E>(
        &mut self,
        id: RequestId,
        f: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<(), anyhow::Error>
    where
        T: Serialize,
        E: Display,
    {
        match f(&mut *self) {
            Ok(v) => self.server.send_ok(id, &v),
            Err(e) => {
                self.server
                    .send_err(id, lsp_server::ErrorCode::RequestFailed, &e.to_string())
            }
        }
    }
    async fn send_fut_result<T, E, Fut>(&self, id: RequestId, f: Fut) -> Result<(), anyhow::Error>
    where
        Fut: Future<Output = Result<T, E>>,
        T: Serialize,
        E: Display,
    {
        match f.await {
            Ok(v) => self.server.send_ok(id, &v),
            Err(e) => {
                self.server
                    .send_err(id, lsp_server::ErrorCode::RequestFailed, &e.to_string())
            }
        }
    }
    /// Handles one message, returning the request that interrupted a run of document changes.
    async fn respond(&mut self, req: Message) -> anyhow::Result<Option<Message>> {
        // TODO: propagate err to lsp client instead of swallowing it
        match req {
            Message::UnhashRequest { id, range } => {
                let _ = self.send_result(id, |w| {
                    Ok::<_, anyhow::Error>(w.unhash(range)?.unwrap_or_default())
                });
            }
            Message::HoverRequest {
                id,
                position,
                work_done_progress_params,
            } => {
                let _ = self
                    .send_fut_result(id, async {
                        Ok::<_, anyhow::Error>(
                            self.hover(position, work_done_progress_params)
                                .await?
                                .unwrap_or_else(|| Hover {
                                    contents: lsp_types::HoverContents::Scalar(
                                        MarkedString::String(String::new()),
                                    ),
                                    range: None,
                                }),
                        )
                    })
                    .await;
            }
            Message::SymbolsRequest {
                id,
                work_done_progress_params,
                partial_result_params,
            } => {
                let _ = self.send_result(id, |w| {
                    let Symbols {
                        symbols,
                        total_limit_reached,
                        depth_limit_reached,
                    } = w.symbols(work_done_progress_params, partial_result_params)?;
                    if let Some((limit, true)) = total_limit_reached {
                        let _ = w
                            .server
                            .send_notification::<ShowMessage>(ShowMessageParams {
                                typ: MessageType::WARNING,
                                message: format!(
                                    "Document symbols have been truncated to {limit} total symbols."
                                ),
                            });
                    }
                    if let Some((limit, true)) = depth_limit_reached {
                        let _ = w
                            .server
                            .send_notification::<ShowMessage>(ShowMessageParams {
                                typ: MessageType::WARNING,
                                message: format!(
                                    "Document symbol tree has been limited to {limit} symbols deep."
                                ),
                            });
                    }
                    Ok::<_, Unparsed>(DocumentSymbolResponse::Nested(symbols))
                });
            }
            Message::CompletionRequest(req) => {
                let _ = self.send_result(req.id.clone(), |w| {
                    Ok::<_, anyhow::Error>(
                        w.complete(req)?
                            .unwrap_or_else(|| CompletionResponse::Array(vec![])),
                    )
                });
            }
            Message::CodeActionRequest {
                id,
                range,
                diagnostics,
                ..
            } => {
                let _ = self.send_result(id, |w| {
                    Ok::<_, anyhow::Error>(w.code_actions(range, diagnostics)?.unwrap_or_default())
                });
            }
            Message::FormatRequest {
                id,
                options,
                work_done_progress_params,
            } => {
                let _ = self.send_result(id, |w| w.format(options, work_done_progress_params));
            }
            Message::SemanticTokens {
                id,
                range,
                previous_result_id,
                ..
            } => {
                let _ = self.send_result(id, |w| w.semantic_tokens(range, previous_result_id));
            }
            Message::DocumentColorRequest {
                id,
                work_done_progress_params,
                partial_result_params,
            } => {
                let _ = self.send_result(id, |w| {
                    w.document_colors(work_done_progress_params, partial_result_params)
                });
            }
            Message::ColorPresentationRequest {
                id, color, range, ..
            } => {
                let _ = self.send_result(id, |w| {
                    Ok::<_, Unparsed>(w.color_presentations(color, range))
                });
            }
            Message::DocumentChange { version, changes } => {
                self.document.update(version, changes);
                let pending = drain_changes(&mut self.rx, &mut self.document);
                self.diagnostics_due = Some(Instant::now() + DIAGNOSTICS_DEBOUNCE);
                return Ok(pending);
            }
        }
        Ok(None)
    }
}

enum Wake {
    Message(Message),
    /// The deadline passed with no message - signals to run the diagnostics pass.
    Quiet,
    /// The client is gone.
    Closed,
}

async fn recv(rx: &mut mpsc::Receiver<Message>) -> Wake {
    match rx.recv().await {
        Some(msg) => Wake::Message(msg),
        None => Wake::Closed,
    }
}

/// Waits for the next message, or for the debounce deadline if diagnostics are due.
async fn wake(rx: &mut mpsc::Receiver<Message>, due: Option<Instant>) -> Wake {
    let Some(due) = due else {
        return recv(rx).await;
    };

    // Process any queues request before we run the diagnostics pass.
    tokio::select! {
        biased;
        wake = recv(rx) => wake,
        _ = sleep_until(due) => Wake::Quiet,
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

    #[tokio::test]
    async fn a_quiet_document_wakes_for_diagnostics() {
        let (_tx, mut rx) = mpsc::channel(8);
        let due = Instant::now() + Duration::from_millis(10);

        assert!(matches!(wake(&mut rx, Some(due)).await, Wake::Quiet));
    }

    #[tokio::test]
    async fn a_queued_message_beats_a_due_deadline() {
        let (tx, mut rx) = mpsc::channel(8);
        let due = Instant::now();
        // both arms are ready by now, so `biased` is what keeps the request from queueing
        // behind the diagnostics pass
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.try_send(keystroke(1, 0, 0, "a")).unwrap();

        assert!(matches!(wake(&mut rx, Some(due)).await, Wake::Message(_)));
    }

    #[tokio::test]
    async fn an_idle_worker_never_wakes_on_its_own() {
        let (_tx, mut rx) = mpsc::channel::<Message>(8);

        // no deadline armed means diagnostics are already caught up - waking here would
        // retypecheck an unchanged document, forever. Has to outlast the debounce to catch a
        // `None` that quietly defaults to one.
        let idle = tokio::time::timeout(DIAGNOSTICS_DEBOUNCE * 2, wake(&mut rx, None));
        assert!(idle.await.is_err());
    }

    #[tokio::test]
    async fn a_closed_channel_ends_the_loop() {
        let (tx, mut rx) = mpsc::channel::<Message>(8);
        drop(tx);

        assert!(matches!(wake(&mut rx, None).await, Wake::Closed));
        let due = Instant::now() + Duration::from_secs(60);
        assert!(matches!(wake(&mut rx, Some(due)).await, Wake::Closed));
    }
}
