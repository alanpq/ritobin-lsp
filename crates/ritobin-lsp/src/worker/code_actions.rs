use std::collections::HashMap;

use lsp_types::{
    CodeAction, CodeActionOrCommand, CodeActionResponse, Diagnostic, Range, TextEdit, WorkspaceEdit,
};
use ritobin_lsp::cst_ext::CstExt;

use crate::worker::Worker;

pub enum CodeActionData {}

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
        &self,
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

    fn code_action(&self, diagnostic: Diagnostic) -> Option<CodeActionOrCommand> {
        let idx = diagnostic.data.and_then(|d| d.as_u64())?;
        let data = self.code_action_data.get(idx as usize)?;
        Some(match data {
            _ => return None,
        })
    }
}
