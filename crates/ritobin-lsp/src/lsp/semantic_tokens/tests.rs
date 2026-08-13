use super::*;

fn tokens(deltas: &[u32]) -> Vec<SemanticToken> {
    deltas
        .iter()
        .map(|&n| SemanticToken {
            delta_line: n,
            delta_start: n,
            length: n,
            token_type: n,
            token_modifiers_bitset: n,
        })
        .collect()
}

/// Applies edits the way a client does - to the flat `u32` array, not to the token vec.
fn apply(before: &[SemanticToken], edits: &[SemanticTokensEdit]) -> Vec<SemanticToken> {
    fn flatten(tokens: &[SemanticToken]) -> Vec<u32> {
        tokens
            .iter()
            .flat_map(|t| {
                [
                    t.delta_line,
                    t.delta_start,
                    t.length,
                    t.token_type,
                    t.token_modifiers_bitset,
                ]
            })
            .collect()
    }

    let mut flat = flatten(before);
    for edit in edits.iter().rev() {
        let start = edit.start as usize;
        let end = start + edit.delete_count as usize;
        flat.splice(start..end, flatten(edit.data.as_deref().unwrap_or(&[])));
    }

    flat.chunks_exact(5)
        .map(|c| SemanticToken {
            delta_line: c[0],
            delta_start: c[1],
            length: c[2],
            token_type: c[3],
            token_modifiers_bitset: c[4],
        })
        .collect()
}

/// `start` and `delete_count` index the flat array, so both are token counts times five.
/// Round tripping is what catches a missing (or doubled) `5 *`.
#[test]
fn applying_a_diff_reproduces_the_new_tokens() {
    const CASES: &[(&[u32], &[u32])] = &[
        (&[], &[]),
        (&[], &[1, 2, 3]),
        (&[1, 2, 3], &[]),
        (&[1, 2, 3], &[1, 2, 3]),
        // insert at start / middle / end
        (&[1, 2], &[9, 1, 2]),
        (&[1, 2], &[1, 9, 8, 2]),
        (&[1, 2], &[1, 2, 9]),
        // remove at start / middle / end
        (&[9, 1, 2], &[1, 2]),
        (&[1, 9, 8, 2], &[1, 2]),
        (&[1, 2, 9], &[1, 2]),
        // replace, and a repeated run where prefix and suffix scans could overlap
        (&[1, 2, 3], &[1, 7, 3]),
        (&[5, 5, 5, 5], &[5, 5]),
        (&[5, 5], &[5, 5, 5, 5]),
    ];

    for (before, after) in CASES {
        let (before, after) = (tokens(before), tokens(after));
        let edits = diff_tokens(&before, &after);

        assert_eq!(apply(&before, &edits), after, "{before:?} -> {after:?}");
    }
}

#[test]
fn an_unchanged_document_diffs_to_nothing() {
    assert!(diff_tokens(&tokens(&[1, 2, 3]), &tokens(&[1, 2, 3])).is_empty());
}

fn full(cache: &mut TokenCache, t: &[u32]) -> SemanticTokensFullDeltaResult {
    cache.respond(TokenRequest::Full, tokens(t))
}

fn delta(cache: &mut TokenCache, cited: &str, t: &[u32]) -> SemanticTokensFullDeltaResult {
    cache.respond(TokenRequest::Delta(cited), tokens(t))
}

fn result_id(res: &SemanticTokensFullDeltaResult) -> Option<&str> {
    match res {
        SemanticTokensFullDeltaResult::Tokens(t) => t.result_id.as_deref(),
        SemanticTokensFullDeltaResult::TokensDelta(d) => d.result_id.as_deref(),
        SemanticTokensFullDeltaResult::PartialTokensDelta { .. } => None,
    }
}

/// Every response has to name an id the *next* delta can cite, full fallbacks included.
#[test]
fn every_response_mints_a_fresh_id() {
    let mut cache = TokenCache::default();

    let first = result_id(&full(&mut cache, &[1])).unwrap().to_owned();
    let second = result_id(&delta(&mut cache, &first, &[1, 2]))
        .unwrap()
        .to_owned();
    let third = result_id(&delta(&mut cache, "stale", &[1, 2, 3]))
        .unwrap()
        .to_owned();

    assert_ne!(first, second);
    assert_ne!(second, third);
    // and the id a full fallback minted is itself citable
    assert!(matches!(
        delta(&mut cache, &third, &[1, 2, 3, 4]),
        SemanticTokensFullDeltaResult::TokensDelta(_)
    ));
}

#[test]
fn citing_the_cached_id_gets_a_diff() {
    let mut cache = TokenCache::default();
    let id = result_id(&full(&mut cache, &[1, 2])).unwrap().to_owned();

    let SemanticTokensFullDeltaResult::TokensDelta(delta) = delta(&mut cache, &id, &[1, 2, 3])
    else {
        panic!("expected a delta");
    };
    assert_eq!(apply(&tokens(&[1, 2]), &delta.edits), tokens(&[1, 2, 3]));
}

/// The client is a version behind, or reconnected, or we restarted. Answering with an empty
/// edit list would leave it showing tokens for text it no longer has.
#[test]
fn citing_an_unknown_id_gets_the_full_set() {
    let mut cache = TokenCache::default();
    full(&mut cache, &[1, 2]);

    let res = delta(&mut cache, "not-an-id", &[1, 2, 3]);

    let SemanticTokensFullDeltaResult::Tokens(t) = res else {
        panic!("expected full tokens");
    };
    assert_eq!(t.data, tokens(&[1, 2, 3]));
}

/// A range response covers only the viewport. If it became the baseline, the next delta
/// would be computed against a fragment and scramble the whole document's highlighting.
#[test]
fn a_range_response_is_not_a_baseline() {
    let mut cache = TokenCache::default();
    let id = result_id(&full(&mut cache, &[1, 2, 3])).unwrap().to_owned();

    let ranged = cache.respond(TokenRequest::Range, tokens(&[2]));

    // it names no id, so the client cannot cite it in the first place
    assert_eq!(result_id(&ranged), None);
    // and the baseline is still the full response that preceded it
    let SemanticTokensFullDeltaResult::TokensDelta(delta) = delta(&mut cache, &id, &[1, 2, 3, 4])
    else {
        panic!("expected a delta");
    };
    assert_eq!(
        apply(&tokens(&[1, 2, 3]), &delta.edits),
        tokens(&[1, 2, 3, 4])
    );
}
