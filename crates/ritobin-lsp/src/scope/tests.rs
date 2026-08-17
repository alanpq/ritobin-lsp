use ltk_ritobin::parse::Span;

use super::*;

/// `|` marks the cursor. Returns the class the cursor is *directly* inside, and the full chain
/// of enclosing class hashes, outermost first.
fn at(src: &str) -> (Option<BinHash>, Vec<BinHash>) {
    let offset = src.find('|').expect("fixture needs a | cursor marker") as u32;
    let text = src.replacen('|', "", 1);
    let cst = Cst::parse(&text);
    let scopes = cst.class_context_at(offset, &text);
    (
        scopes.current().and_then(|s| s.hash),
        scopes.classes().map(|s| s.hash.unwrap()).collect(),
    )
}

/// The class each `EntryKey` resolves to, in walk order - what the linter checks against.
fn tracked_keys(text: &str) -> Vec<(&str, Option<&str>)> {
    struct Probe<'a> {
        tracker: ClassTracker<'a>,
        seen: Vec<(&'a str, Option<&'a str>)>,
    }

    impl<'a> Visitor for Probe<'a> {
        fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
            let visit = self.tracker.enter_tree(ctx, tree);

            let text = self.tracker.text;
            if let Some(node) = ctx.node(tree).filter(|n| n.kind == TreeKind::EntryKey) {
                let class = self.tracker.current().map(|s| &text[s.token.span]);
                self.seen.push((text[node.span].trim(), class));
            }

            visit
        }

        fn exit_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
            self.tracker.exit_tree(ctx, tree)
        }
    }

    Probe {
        tracker: ClassTracker::new(text),
        seen: Vec::new(),
    }
    .walk(&Cst::parse(text))
    .seen
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
    assert_eq!(at(&src), (Some(h(SKIN)), vec![h(SKIN)]));
}

#[test]
fn top_level_has_no_scope() {
    assert_eq!(at("type: string = \"PROP\"\n|\n"), (None, vec![]));
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
            Some(h("SkinMeshDataProperties")),
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
        (Some(BinHash(0x9b67e9f6)), vec![BinHash(0x9b67e9f6)])
    );
}

#[test]
fn a_cursor_on_a_class_name_is_at_that_class() {
    // Hovering a class name renders the class it names, so the name has to resolve to the class
    // itself and not to the one whose body it sits in.
    let src = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinMeshProperties: embed = SkinMeshData|Properties {{
        }}
    }}
}}
"
    );
    assert_eq!(
        at(&src),
        (
            Some(h("SkinMeshDataProperties")),
            vec![h(SKIN), h("SkinMeshDataProperties")]
        )
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
    assert_eq!(at(&src), (None, vec![h(SKIN)]));
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
            Some(h("SkinMeshDataProperties")),
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
    assert_eq!(at(&src), (Some(h(SKIN)), vec![h(SKIN)]));
}

#[test]
fn a_class_in_a_map_entry_does_not_outlive_its_body() {
    // leaving the nested class used to leave "we are in a class body" set, so every
    // map key after the first resolved to the enclosing class and was linted as its property.
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: map[hash,embed] = {{
            0xdeadbeef = SkinMeshDataProperties {{ boneName: string = \"root\" }}
            0xfeedface = SkinMeshDataProperties {{ boneName: string = \"neck\" }}
        }}
        afterTheMap: u32 = 1
    }}
}}
"
    );

    assert_eq!(
        tracked_keys(&text),
        vec![
            ("entries", None),
            ("\"0x1\"", None),
            ("childList", Some(SKIN)),
            ("0xdeadbeef", None),
            ("boneName", Some("SkinMeshDataProperties")),
            ("0xfeedface", None),
            ("boneName", Some("SkinMeshDataProperties")),
            ("afterTheMap", Some(SKIN)),
        ]
    );
}

#[test]
fn a_container_block_does_not_outlive_its_body() {
    // the container's block used to leave "we are not in a class body" set, so the
    // first property after any container-valued property was never checked at all.
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: list[u32] = {{
            1, 2
        }}
        afterTheList: u32 = 1
    }}
}}
"
    );

    assert_eq!(
        tracked_keys(&text),
        vec![
            ("entries", None),
            ("\"0x1\"", None),
            ("childList", Some(SKIN)),
            ("afterTheList", Some(SKIN)),
        ]
    );
}

#[test]
fn a_missing_brace_does_not_desync_the_scopes() {
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinMeshProperties: embed = SkinMeshDataProperties {{
            boneName: string = \"root\"
        afterTheHole: u32 = 1
    }}
}}
"
    );

    assert_eq!(
        tracked_keys(&text),
        vec![
            ("entries", None),
            ("\"0x1\"", None),
            ("skinMeshProperties", Some(SKIN)),
            ("boneName", Some("SkinMeshDataProperties")),
            ("afterTheHole", Some("SkinMeshDataProperties")),
        ]
    );
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
