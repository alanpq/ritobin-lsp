use std::{
    fmt::Write as _,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Duration,
};

use imara_diff::{Algorithm, Diff, InternedInput};
use lsp_server::RequestId;
use lsp_types::{
    CompletionContext, CompletionResponse, FormattingOptions, Hover, MarkedString, MarkupContent,
    MarkupKind, PartialResultParams, Position, Range, SemanticToken, SemanticTokensFullDeltaResult,
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
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, sleep_until},
};

use crate::{
    document::Document,
    lol_meta::schema::U32Hash,
    lsp::{
        ext::PositionOrRange,
        semantic_tokens::{TokenCache, TokenRequest, builder::SemanticTokensBuilder},
    },
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
        /// The `result_id` of the last response the client holds, if it is asking for a delta.
        previous_result_id: Option<String>,
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

// TODO: Make this configurable ?
/// How long the document has to go quiet before we typecheck and lint it.
const DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_millis(250);

/// `None` when the parser itself panicked.
fn parse_cst(text: &str) -> Option<Cst> {
    catch_unwind(AssertUnwindSafe(|| Cst::parse(text))).ok()
}

/// `None` when the typechecker panicked on this revision.
fn typecheck(cst: &Cst, text: &str) -> Option<Vec<DiagnosticWithSpan>> {
    catch_unwind(AssertUnwindSafe(|| cst.build_bin(text)))
        .ok()
        .map(|(_bin, errors)| errors)
}

pub struct Worker {
    rx: mpsc::Receiver<Message>,
    document: Document,
    cst: Option<Cst>,
    server: Arc<Server>,
    tokens: TokenCache,
    /// Deadline for the next diagnostics pass. `None` when diagnostics are up to date.
    diagnostics_due: Option<Instant>,
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
                    cst: None,
                    server,
                    tokens: TokenCache::default(),
                    diagnostics_due: None,
                };
                worker.refresh_cst();

                if let Err(e) = worker.service().await {
                    tracing::error!("document worker error: {e:?}");
                }
            }),
        }
    }

    /// Reparses the tree.
    fn refresh_cst(&mut self) {
        tracing::debug!("[worker] '{}' reparse", self.document.uri);

        self.diagnostics_due = Some(Instant::now() + DIAGNOSTICS_DEBOUNCE);

        self.cst = parse_cst(&self.document.text);
        if self.cst.is_none() {
            tracing::error!("[worker] '{}' parser panicked", self.document.uri);
        }
    }

    /// Typechecks and lints the current tree
    fn refresh_diagnostics(&mut self) {
        self.diagnostics_due = None;

        let Some(cst) = self.cst.as_ref() else {
            return;
        };

        tracing::debug!("[worker] '{}' diagnostics", self.document.uri);
        let errors = typecheck(cst, &self.document.text);
        if let Err(e) = self.publish_parse_errors(cst, errors) {
            tracing::error!("[worker] '{}' publish failed: {e:?}", self.document.uri);
        }
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
                        self.refresh_diagnostics();
                        continue;
                    }
                    Wake::Closed => break,
                },
            };

            match self.respond(req) {
                Ok(next) => pending = next,
                Err(e) => {
                    tracing::error!("[worker] '{}' request failed: {e:?}", self.document.uri)
                }
            }
        }
        Ok(())
    }

    /// Handles one message, returning the request that interrupted a run of document changes.
    fn respond(&mut self, req: Message) -> anyhow::Result<Option<Message>> {
        // TODO: propagate err to lsp client instead of swallowing it
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
                range,
                previous_result_id,
                ..
            } => {
                let res = self.semantic_tokens(range, previous_result_id);
                let _ = self.server.send_ok(id, &res);
            }
            Message::DocumentChange { version, changes } => {
                self.document.update(version, changes);
                let pending = drain_changes(&mut self.rx, &mut self.document);
                self.refresh_cst();
                return Ok(pending);
            }
        }
        Ok(None)
    }

    fn semantic_tokens(
        &mut self,
        range: Option<Range>,
        previous_result_id: Option<String>,
    ) -> SemanticTokensFullDeltaResult {
        let tokens = self.collect_tokens(range.as_ref());

        let request = match (&range, &previous_result_id) {
            (Some(_), _) => TokenRequest::Range,
            (None, Some(cited)) => TokenRequest::Delta(cited),
            (None, None) => TokenRequest::Full,
        };

        self.tokens.respond(request, tokens)
    }

    fn collect_tokens(&self, range: Option<&Range>) -> Vec<SemanticToken> {
        let doc = &self.document;
        let Some(cst) = self.cst.as_ref() else {
            return Vec::new();
        };

        let visitor = SemanticVisitor {
            text: &doc.text,
            line_nums: &doc.line_numbers,
            stack: Vec::new(),
            entry_typed: Vec::new(),
            range: range.map(|range| doc.line_numbers.from_range(range)),
            builder: SemanticTokensBuilder::default(),
        }
        .walk(cst);

        visitor.builder.build()
    }

    fn hover(
        &self,
        position: PositionOrRange,
        _work_done_progress_params: WorkDoneProgressParams,
    ) -> anyhow::Result<Option<Hover>> {
        let pos = position.start();
        let doc = &self.document;
        let Some(cst) = self.cst.as_ref() else {
            return Ok(None);
        };

        let offset = doc.line_numbers.from_position(pos);
        let scope = cst.class_context_at(offset, &doc.text).current().copied();
        let found_token = cst
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
                        _ => match cst.find_node(offset) {
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
                value: match cst.find_node(offset) {
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
        let Some(cst) = self.cst.as_ref() else {
            return Ok(None);
        };
        let mut formatted = String::new();
        ltk_ritobin::print::CstPrinter::new(&doc.text, &mut formatted, PrintConfig::default())
            .print(cst)
            .unwrap();

        Ok(Some(diff_to_textedits(&doc.text, &formatted)))
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

    /// The typechecker unwraps a `Literal`'s first child as a token, so recovery trees that put a
    /// subtree there panic it. Half-typed and half-deleted strings reach that state constantly, and
    /// a panic here used to kill the worker - and with it the document's highlighting.
    #[test]
    fn a_typechecker_panic_still_yields_a_tree() {
        const BROKEN: &[&str] = &[
            // a string left unterminated mid-entry
            r#"#PROP_text
entries: map[hash,embed] = {
    "0x1" = Def {
        name: string = "TRA
        Group: string = "Transition Effect"
    }
}
"#,
            // ... and one that swallows the brace closing its entry
            r#"#PROP_text
entries: map[hash,embed] = {
    "0x1" = Def {
        name: string = "TRA
    }
    "0x2" = Def {
        on: bool = false
    }
}
"#,
            // the quote on its own, as it exists for one keystroke
            r#"#PROP_text
entries: map[hash,embed] = {
    "0x1" = Def {
        name: string = "
    }
}
"#,
        ];

        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let parsed: Vec<_> = BROKEN
            .iter()
            .map(|text| {
                let cst = parse_cst(text).expect("the parser itself must survive");
                let errors = typecheck(&cst, text);
                (cst, errors)
            })
            .collect();
        std::panic::set_hook(hook);

        for (text, (cst, errors)) in BROKEN.iter().zip(parsed) {
            // no diagnostics at all means the typechecker went down - that is the case under
            // test, and the tree has to come back anyway
            assert!(errors.is_none(), "no longer panics on:\n{text}");
            assert!(!cst.errors.is_empty(), "should report a syntax error");
        }
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
