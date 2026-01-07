use std::ops;

use lsp_types::{
    Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensEdit,
};

use crate::lsp::semantic_tokens::types::{LAST_STANDARD_MOD, SUPPORTED_MODIFIERS};

#[derive(Default)]
pub(crate) struct ModifierSet(pub(crate) u32);

impl ModifierSet {
    pub(crate) fn standard_fallback(&mut self) {
        // Remove all non standard modifiers
        self.0 &= !(!0u32 << LAST_STANDARD_MOD)
    }
}

impl ops::BitOrAssign<SemanticTokenModifier> for ModifierSet {
    fn bitor_assign(&mut self, rhs: SemanticTokenModifier) {
        let idx = SUPPORTED_MODIFIERS
            .iter()
            .position(|it| it == &rhs)
            .unwrap();
        self.0 |= 1 << idx;
    }
}
