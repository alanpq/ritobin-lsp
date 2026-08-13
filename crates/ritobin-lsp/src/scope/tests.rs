use ltk_ritobin::parse::Span;

use super::*;

/// `|` marks the cursor. Returns the context state and the full chain of enclosing class
/// hashes, outermost first.
fn at(src: &str) -> (ClassContextState, Vec<BinHash>) {
    let offset = src.find('|').expect("fixture needs a | cursor marker") as u32;
    let text = src.replacen('|', "", 1);
    let cst = Cst::parse(&text);
    let scopes = cst.class_context_at(offset, &text);
    (
        scopes.state,
        scopes.scopes.iter().map(|s| s.hash.unwrap()).collect(),
    )
}

fn h(name: &str) -> BinHash {
    BinHash::hash_str(name)
}

const SKIN: &str = "SkinCharacterDataProperties";

#[test]
fn a_class_body_is_the_innermost_scope() {
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        sibling: embed = SiblingClass {{}}
        |
    }}
}}
"
    );
    assert_eq!(at(&src), (ClassContextState::InScope, vec![h(SKIN)]));
}

#[test]
fn top_level_has_no_scope() {
    assert_eq!(
        at("type: string = \"PROP\"\n|\n"),
        (ClassContextState::NotInScope, vec![])
    );
}

#[test]
fn a_nested_class_shadows_its_parent() {
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinMeshProperties: embed = SkinMeshDataProperties {{
            |
        }}
    }}
}}
"
    );
    assert_eq!(
        at(&src),
        (
            ClassContextState::InScope,
            vec![h(SKIN), h("SkinMeshDataProperties")]
        )
    );
}

#[test]
fn a_hex_class_name_is_parsed_not_hashed() {
    // Regression: hover used to `hash_str` the literal text "0x9b67e9f6", so every hex-named
    // class in a directly-opened .bin resolved to garbage.
    let src = "entries: map[hash,embed] = {
    \"0x1\" = 0x9b67e9f6 {
        |
    }
}
";
    assert_eq!(
        at(src),
        (ClassContextState::InScope, vec![BinHash(0x9b67e9f6)])
    );
}

#[test]
fn a_container_block_has_no_innermost_class_but_keeps_the_enclosing_one() {
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: list[embed] = {{
            |
        }}
    }}
}}
"
    );
    assert_eq!(at(&src), (ClassContextState::NotInScope, vec![h(SKIN)]));
}

#[test]
fn a_class_inside_a_container_block_is_the_innermost_scope() {
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: list[embed] = {{
            SkinMeshDataProperties {{
                |
            }}
        }}
    }}
}}
"
    );
    assert_eq!(
        at(&src),
        (
            ClassContextState::InScope,
            vec![h(SKIN), h("SkinMeshDataProperties")]
        )
    );
}

#[test]
fn leaving_a_class_body_restores_the_outer_scope() {
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinMeshProperties: embed = SkinMeshDataProperties {{
        }}
        |
    }}
}}
"
    );
    assert_eq!(at(&src), (ClassContextState::InScope, vec![h(SKIN)]));
}

#[test]
fn hash_token_hashes_names_and_parses_hex() {
    let text = "Foo 0xdeadbeef";
    let name = Token {
        kind: TokenKind::Name,
        span: Span::new(0, 3),
    };
    let hex = Token {
        kind: TokenKind::HexLit,
        span: Span::new(4, 14),
    };

    assert_eq!(name.as_bin_hash(text), Some(h("Foo")));
    assert_eq!(hex.as_bin_hash(text), Some(BinHash(0xdeadbeef)));
}

#[test]
fn a_hex_literal_we_cannot_fit_has_no_hash() {
    let text = "0xdeadbeefdeadbeef 0x";
    let hex = |span| Token {
        kind: TokenKind::HexLit,
        span,
    };

    assert_eq!(hex(Span::new(0, 18)).as_bin_hash(text), None);
    assert_eq!(hex(Span::new(19, 21)).as_bin_hash(text), None);
}
