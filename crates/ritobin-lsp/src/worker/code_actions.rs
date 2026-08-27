use std::collections::HashMap;

use lsp_types::{
    CodeAction, CodeActionOrCommand, CodeActionResponse, Diagnostic, Range, TextEdit, WorkspaceEdit,
};
use ltk_ritobin::{RitoType, parse::Span};

use crate::worker::Worker;

pub enum CodeActionData {
    TypeMismatch { type_expr: Span, expected: RitoType },
    RemoveEntry(Span),
    Replace { span: Span, with: String },
}

impl Worker {
    pub(crate) fn register_code_action(&mut self, data: CodeActionData) -> u32 {
        let idx: u32 = self
            .code_action_data
            .len()
            .try_into()
            .expect("< u32::MAX code actions in one document");
        self.code_action_data.push(data);
        idx
    }

    pub fn code_actions(
        &mut self,
        _range: Range,
        diagnostics: Vec<Diagnostic>,
    ) -> anyhow::Result<Option<CodeActionResponse>> {
        Ok(Some(
            diagnostics
                .into_iter()
                .filter_map(|d| self.code_action(d))
                .collect(),
        ))
    }

    fn code_action(&mut self, diagnostic: Diagnostic) -> Option<CodeActionOrCommand> {
        let idx = diagnostic.data.and_then(|d| d.as_u64())?;
        let data = self.code_action_data.get_mut(idx as usize)?;
        Some(match data {
            CodeActionData::Replace { span, with } => CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Replace with '{with}'",),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from_iter([(
                        self.document.uri.clone(),
                        [TextEdit {
                            range: self.document.line_numbers.from_span(*span),
                            new_text: with.clone(),
                        }]
                        .into(),
                    )])),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            CodeActionData::TypeMismatch {
                type_expr,
                expected,
            } => {
                let new_text = expected.to_string();
                CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Change this entry's type to {new_text}",),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from_iter([(
                            self.document.uri.clone(),
                            [TextEdit {
                                range: self.document.line_numbers.from_span(*type_expr),
                                new_text,
                            }]
                            .into(),
                        )])),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
            }
            CodeActionData::RemoveEntry(entry) => CodeActionOrCommand::CodeAction(CodeAction {
                title: "Remove this entry".into(),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from_iter([(
                        self.document.uri.clone(),
                        [TextEdit {
                            range: self.document.line_numbers.from_span(*entry),
                            new_text: String::new(),
                        }]
                        .into(),
                    )])),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        })
    }
}
