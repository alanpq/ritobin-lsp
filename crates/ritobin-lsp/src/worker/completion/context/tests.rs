use super::*;
use ltk_hash::Hash as _;

fn resolve_at(src: &str) -> Option<CursorContext> {
    let offset = src.find('|').expect("fixture needs a | cursor marker") as u32;
    let text = src.replacen('|', "", 1);
    let cst = Cst::parse(&text);
    resolve(&cst, &text, offset)
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
            class: h("SkinCharacterDataProperties")
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
            class: h("VfxEmitterDefinitionData")
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
            class: h("VfxProbabilityTableData")
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
            class: BinHash(0x1234abcd)
        })
    );
}
