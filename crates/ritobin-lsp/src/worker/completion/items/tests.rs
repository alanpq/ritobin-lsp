use super::*;
use meta_wiki::schema::{BinType, Class, ClassFlags, Property, PropertyContainer, PropertyMap};
use std::collections::HashMap;

fn class(base: Option<u32>, interface: bool, properties: Vec<(u32, Property)>) -> Class {
    Class {
        base: base.map(U32Hash),
        flags: ClassFlags {
            interface,
            ..Default::default()
        },
        properties: properties
            .into_iter()
            .map(|(hash, prop)| (U32Hash(hash), prop))
            .collect(),
        ..Default::default()
    }
}

fn classes(entries: impl IntoIterator<Item = (u32, Class)>) -> Classes {
    Classes::new(
        entries
            .into_iter()
            .map(|(hash, class)| (U32Hash(hash), class))
            .collect::<HashMap<_, _>>(),
    )
}

fn pointer_to(other: u32) -> Property {
    Property {
        value_type: BinType::Pointer,
        other_class: Some(U32Hash(other)),
        ..Default::default()
    }
}

fn list_of(value_type: BinType, other: u32) -> Property {
    Property {
        value_type: BinType::List,
        other_class: Some(U32Hash(other)),
        container: Some(PropertyContainer {
            value_type,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn map_of(value_type: BinType, other: u32) -> Property {
    Property {
        value_type: BinType::Map,
        other_class: Some(U32Hash(other)),
        map: Some(PropertyMap {
            key_type: BinType::Hash,
            value_type,
            vtable: 0.into(),
            storage: meta_wiki::schema::MapStorage::UnknownMap,
        }),
        ..Default::default()
    }
}

fn named(names: &[(u32, &'static str)]) -> impl Fn(U32Hash) -> Option<Name<'static>> {
    let names: HashMap<u32, &'static str> = names.iter().copied().collect();
    move |hash: U32Hash| names.get(&hash.0).map(|n| Cow::Borrowed(*n))
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn value_slot_reads_the_properties_own_kind() {
    let classes = classes([(1, class(None, false, vec![(10, pointer_to(2))]))]);
    assert_eq!(
        value_slot(&classes, BinHash(1), BinHash(10), false),
        Some(ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        })
    );
}

#[test]
fn value_slot_reads_the_container_element_kind() {
    let classes = classes([(
        1,
        class(None, false, vec![(10, list_of(BinType::Embed, 2))]),
    )]);
    assert_eq!(
        value_slot(&classes, BinHash(1), BinHash(10), true),
        Some(ValueSlot {
            kind: PropertyKind::Embedded,
            other_class: Some(U32Hash(2)),
        })
    );
}

#[test]
fn value_slot_reads_the_map_value_kind() {
    let classes = classes([(
        1,
        class(None, false, vec![(10, map_of(BinType::Pointer, 2))]),
    )]);
    assert_eq!(
        value_slot(&classes, BinHash(1), BinHash(10), true),
        Some(ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        })
    );
}

#[test]
fn value_slot_finds_properties_inherited_from_a_base() {
    let classes = classes([
        (1, class(None, false, vec![(10, pointer_to(2))])),
        (5, class(Some(1), false, vec![])),
    ]);
    assert_eq!(
        value_slot(&classes, BinHash(5), BinHash(10), false),
        Some(ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        })
    );
}

#[test]
fn value_slot_is_none_for_an_unknown_property() {
    let classes = classes([(1, class(None, false, vec![]))]);
    assert_eq!(value_slot(&classes, BinHash(1), BinHash(10), false), None);
}

#[test]
fn a_bool_slot_offers_true_and_false() {
    let classes = classes([]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Bool,
            other_class: None,
        },
        named(&[]),
        true,
    );
    assert_eq!(labels(&items), vec!["true", "false"]);
}

#[test]
fn a_bitbool_slot_offers_true_and_false() {
    let classes = classes([]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::BitBool,
            other_class: None,
        },
        named(&[]),
        true,
    );
    assert_eq!(labels(&items), vec!["true", "false"]);
}

#[test]
fn a_pointer_slot_offers_concrete_subclasses_nearest_first() {
    let classes = classes([
        (2, class(None, true, vec![])),
        (3, class(Some(2), false, vec![])),
        (4, class(Some(2), false, vec![])),
        (5, class(Some(3), false, vec![])),
    ]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        },
        named(&[(2, "Base"), (3, "Beta"), (4, "Alpha"), (5, "Deep")]),
        true,
    );
    assert_eq!(labels(&items), vec!["Alpha", "Beta", "Deep"]);
}

#[test]
fn unnamed_classes_fall_back_to_hex_and_sort_after_named_peers() {
    let classes = classes([
        (2, class(None, false, vec![])),
        (3, class(Some(2), false, vec![])),
    ]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        },
        named(&[(2, "Root")]),
        true,
    );
    assert_eq!(labels(&items), vec!["Root", "0x00000003"]);
}

#[test]
fn a_class_item_inserts_a_snippet_body_when_snippets_are_supported() {
    let classes = classes([(2, class(None, false, vec![]))]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        },
        named(&[(2, "Root")]),
        true,
    );
    assert_eq!(items[0].insert_text.as_deref(), Some("Root {\n\t$0\n}"));
    assert_eq!(items[0].insert_text_format, Some(InsertTextFormat::SNIPPET));
}

#[test]
fn a_class_item_inserts_a_plain_body_without_snippet_support() {
    let classes = classes([(2, class(None, false, vec![]))]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Struct,
            other_class: Some(U32Hash(2)),
        },
        named(&[(2, "Root")]),
        false,
    );
    assert_eq!(items[0].insert_text.as_deref(), Some("Root {\n}"));
    assert_eq!(items[0].insert_text_format, None);
}

#[test]
fn a_container_slot_offers_a_block() {
    let classes = classes([]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Container,
            other_class: None,
        },
        named(&[]),
        true,
    );
    assert_eq!(labels(&items), vec!["{}"]);
    assert_eq!(items[0].insert_text.as_deref(), Some("{\n\t$0\n}"));
}

#[test]
fn a_string_slot_offers_nothing() {
    let classes = classes([]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::String,
            other_class: None,
        },
        named(&[]),
        true,
    );
    assert!(items.is_empty());
}

#[test]
fn a_pointer_slot_without_a_target_class_offers_nothing() {
    let classes = classes([]);
    let items = value_items(
        &classes,
        ValueSlot {
            kind: PropertyKind::Struct,
            other_class: None,
        },
        named(&[]),
        true,
    );
    assert!(items.is_empty());
}

#[test]
fn property_items_list_the_classes_own_and_inherited_properties() {
    let classes = classes([
        (1, class(None, false, vec![(10, pointer_to(2))])),
        (5, class(Some(1), false, vec![(11, pointer_to(2))])),
    ]);
    let found = property_items(
        &classes,
        BinHash(5),
        named(&[(10, "fromBase"), (11, "ownProp")]),
    );
    let mut items = labels(&found);
    items.sort_unstable();
    assert_eq!(items, vec!["fromBase", "ownProp"]);
}

#[test]
fn a_property_item_inserts_the_key_type_and_equals() {
    let classes = classes([(1, class(None, false, vec![(10, pointer_to(2))]))]);
    let items = property_items(&classes, BinHash(1), named(&[(10, "primitive")]));
    assert_eq!(
        items[0].insert_text.as_deref(),
        Some("primitive: pointer = ")
    );
}

#[test]
fn the_type_item_is_the_type_the_meta_declares() {
    let classes = classes([(
        1,
        class(None, false, vec![(10, list_of(BinType::Embed, 2))]),
    )]);
    assert_eq!(
        type_item(&classes, BinHash(1), BinHash(10)).map(|i| i.label),
        Some("list[embed]".to_owned())
    );
}

#[test]
fn the_type_item_is_none_for_an_unknown_property() {
    let classes = classes([(1, class(None, false, vec![]))]);
    assert!(type_item(&classes, BinHash(1), BinHash(10)).is_none());
}
