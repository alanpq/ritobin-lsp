use super::*;
use ltk_hash::Hash as _;
use ltk_ritobin::Cst;

fn resolve_at(src: &str) -> Option<CursorContext> {
    let offset = src.find('|').expect("fixture needs a | cursor marker") as u32;
    let text = src.replacen('|', "", 1);
    let ast = Cst::parse(&text).build_ast(&text);
    CompletionContext::resolve(&ast, &text, offset).map(|r| r.context)
}

/// Returns the source substring that a completion at the cursor would replace.
fn replace_at(src: &str) -> Option<String> {
    let offset = src.find('|').expect("fixture needs a | cursor marker") as u32;
    let text = src.replacen('|', "", 1);
    let ast = Cst::parse(&text).build_ast(&text);
    let replace = CompletionContext::resolve(&ast, &text, offset)?.replace;
    Some(text[replace.start as usize..replace.end as usize].to_owned())
}

fn h(name: &str) -> BinHash {
    BinHash::hash_str(name)
}

#[test]
fn blank_line_in_class_body_is_a_property_key() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = SkinCharacterDataProperties {
        |
    }
}
"#
        ),
        Some(CursorContext::PropertyKey {
            class: h("SkinCharacterDataProperties"),
        })
    );
}

#[test]
fn partial_key_in_class_body_is_a_property_key() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        prim|
    }
}
"#
        ),
        Some(CursorContext::PropertyKey {
            class: h("VfxEmitterDefinitionData"),
        })
    );
}

#[test]
fn after_colon_is_a_property_type() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: |
    }
}
"#
        ),
        Some(CursorContext::PropertyType {
            class: h("VfxEmitterDefinitionData"),
            property: h("primitive"),
        })
    );
}

#[test]
fn mid_type_expr() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: bo|ol = true
    }
}
"#
        ),
        Some("bool".to_owned())
    );
}

#[test]
fn mid_value_literal() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: bool = t|rue
    }
}
"#
        ),
        Some("true".to_owned())
    );
}

#[test]
fn after_equals_is_a_property_value() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: pointer = |
    }
}
"#
        ),
        Some(CursorContext::PropertyValue {
            class: h("VfxEmitterDefinitionData"),
            property: h("primitive"),
        })
    );
}

#[test]
fn partially_typed_value_is_a_property_value() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: pointer = VfxPrim|
    }
}
"#
        ),
        Some(CursorContext::PropertyValue {
            class: h("VfxEmitterDefinitionData"),
            property: h("primitive"),
        })
    );
}

#[test]
fn untyped_entry_after_equals_is_a_property_value() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive = |
    }
}
"#
        ),
        Some(CursorContext::PropertyValue {
            class: h("VfxEmitterDefinitionData"),
            property: h("primitive"),
        })
    );
}

#[test]
fn blank_line_in_a_list_block_is_a_container_item() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = {
            |
        }
    }
}
"#
        ),
        Some(CursorContext::ContainerItem {
            class: h("VfxEmitterDefinitionData"),
            property: h("probabilityTables"),
        })
    );
}

#[test]
fn almost_at_type_expr() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] |= {
        }
    }
}
"#
        ),
        None
    );
}
#[test]
fn cursor_before_block() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = |{
            
        }
    }
}
"#
        ),
        None
    );
}

#[test]
fn partial_list_item_is_a_container_item() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = {
            VfxProb|
        }
    }
}
"#
        ),
        Some(CursorContext::ContainerItem {
            class: h("VfxEmitterDefinitionData"),
            property: h("probabilityTables"),
        })
    );
}

#[test]
fn class_nested_in_a_list_block_scopes_to_that_class() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = {
            VfxProbabilityTableData {
                |
            }
        }
    }
}
"#
        ),
        Some(CursorContext::PropertyKey {
            class: h("VfxProbabilityTableData"),
        })
    );
}

#[test]
fn map_entry_value_is_a_container_item() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxSystemDefinitionData {
        someMap: map[hash,embed] = {
            0x1234 = |
        }
    }
}
"#
        ),
        Some(CursorContext::ContainerItem {
            class: h("VfxSystemDefinitionData"),
            property: h("someMap"),
        })
    );
}

#[test]
fn map_entry_key_has_no_completion() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxSystemDefinitionData {
        someMap: map[hash,embed] = {
            0x12|34 = 5
        }
    }
}
"#
        ),
        None
    );
}

#[test]
fn top_level_has_no_completion() {
    assert_eq!(resolve_at("type: string = \"PROP\"\n|\n"), None);
}

#[test]
fn hex_class_name_resolves_to_its_hash() {
    assert_eq!(
        resolve_at(
            r#"entries: map[hash,embed] = {
    "0x1" = 0x1234abcd {
        |
    }
}
"#
        ),
        Some(CursorContext::PropertyKey {
            class: BinHash(0x1234abcd),
        })
    );
}

#[test]
fn mid_property_key() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive|: pointer = VfxPrimitiveData {
        }
    }
}
"#
        ),
        Some("primitive: pointer = ".to_owned())
    );
}

#[test]
fn retriggering_a_key_completion_on_a_bare_key_only_replaces_the_key() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        prim|
    }
}
"#
        ),
        Some("prim".to_owned())
    );
}

#[test]
fn retriggering_a_value_completion_replaces_the_whole_existing_class() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: pointer = VfxPrim|itiveData {
        }
    }
}
"#
        ),
        Some("VfxPrimitiveData {\n        }".to_owned())
    );
}

#[test]
fn retriggering_a_list_item_completion_replaces_the_whole_existing_class() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = {
            VfxProb|abilityTableData {
            }
        }
    }
}
"#
        ),
        Some("VfxProbabilityTableData {\n            }".to_owned())
    );
}

#[test]
fn a_blank_value_slot_has_nothing_to_replace() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: pointer = |
    }
}
"#
        ),
        Some(String::new())
    );
}

#[test]
fn a_partial_value_with_the_cursor_right_after_it_replaces_the_whole_partial() {
    // The cursor sits exactly at the end of "VfxPrim" - a half-open span
    // doesn't "contain" that offset, so this must not fall through to an
    // empty (cursor-anchored) replace the way a genuinely blank slot does.
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        primitive: pointer = VfxPrim|
    }
}
"#
        ),
        Some("VfxPrim".to_owned())
    );
}

#[test]
fn a_partial_list_item_with_the_cursor_right_after_it_replaces_the_whole_partial() {
    assert_eq!(
        replace_at(
            r#"entries: map[hash,embed] = {
    "0x1" = VfxEmitterDefinitionData {
        probabilityTables: list[embed] = {
            VfxProb|
        }
    }
}
"#
        ),
        Some("VfxProb".to_owned())
    );
}
