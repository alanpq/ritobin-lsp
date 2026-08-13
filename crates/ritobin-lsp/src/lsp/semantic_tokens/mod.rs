//! Semantic Tokens helpers

use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensDelta,
    SemanticTokensEdit, SemanticTokensFullDeltaResult,
};

pub mod builder;
pub mod modifier_set;
pub mod types;

/// Which request the tokens were collected for, as far as result ids are concerned.
pub(crate) enum TokenRequest<'a> {
    Full,
    /// A `full/delta` citing the id of the response the client still holds.
    Delta(&'a str),
    /// A viewport request.
    Range,
}

/// The tokens of the last full response, and the `result_id` naming them.
///
/// A `semanticTokens/full/delta` request cites the id it last received; if it is the one we
/// still hold we answer with a diff instead of the whole array.
#[derive(Default)]
pub(crate) struct TokenCache {
    last_id: u32,
    last: Option<(String, Vec<SemanticToken>)>,
}

impl TokenCache {
    /// Answers with a diff when the cited baseline is the one we cached, and with the full set
    /// otherwise - an unknown or stale id must never get an empty edit list, which the client
    /// would read as "nothing changed".
    pub(crate) fn respond(
        &mut self,
        request: TokenRequest<'_>,
        tokens: Vec<SemanticToken>,
    ) -> SemanticTokensFullDeltaResult {
        // Range tokens are a subset encoded from the first visible token, so they get no id and
        // never become the baseline. Caching one corrupts every delta after it.
        if let TokenRequest::Range = request {
            return SemanticTokensFullDeltaResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            });
        }

        self.last_id += 1;
        let result_id = self.last_id.to_string();

        let edits = match (request, self.last.take()) {
            (TokenRequest::Delta(cited), Some((cached, previous))) if cited == cached => {
                Some(diff_tokens(&previous, &tokens))
            }
            _ => None,
        };

        // Both arms name a fresh id and cache what it stands for, full fallbacks included -
        // otherwise the client keeps citing an id we no longer know and never gets a diff.
        let response = match edits {
            Some(edits) => SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
                result_id: Some(result_id.clone()),
                edits,
            }),
            None => SemanticTokensFullDeltaResult::Tokens(SemanticTokens {
                result_id: Some(result_id.clone()),
                data: tokens.clone(),
            }),
        };

        self.last = Some((result_id, tokens));
        response
    }
}

pub(crate) fn diff_tokens(old: &[SemanticToken], new: &[SemanticToken]) -> Vec<SemanticTokensEdit> {
    let offset = new
        .iter()
        .zip(old.iter())
        .take_while(|&(n, p)| n == p)
        .count();

    let (_, old) = old.split_at(offset);
    let (_, new) = new.split_at(offset);

    let offset_from_end = new
        .iter()
        .rev()
        .zip(old.iter().rev())
        .take_while(|&(n, p)| n == p)
        .count();

    let (old, _) = old.split_at(old.len() - offset_from_end);
    let (new, _) = new.split_at(new.len() - offset_from_end);

    if old.is_empty() && new.is_empty() {
        vec![]
    } else {
        // The lsp data field is actually a byte-diff but we
        // travel in tokens so `start` and `delete_count` are in multiples of the
        // serialized size of `SemanticToken`.
        vec![SemanticTokensEdit {
            start: 5 * offset as u32,
            delete_count: 5 * old.len() as u32,
            data: Some(new.into()),
        }]
    }
}

#[cfg(test)]
mod tests;
