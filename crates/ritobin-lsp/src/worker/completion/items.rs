use std::borrow::Cow;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Range, TextEdit,
};
use ltk_hash::BinHash;
use ltk_meta::PropertyKind;
use ltk_ritobin::ast::node::root::RootKind;
use rustc_hash::FxHashSet;

use super::context::RootKindSet;

use meta_wiki::{
    schema::{Property, U32Hash},
    service::Classes,
};

pub type Name<'a> = Cow<'a, str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueSlot {
    pub kind: PropertyKind,
    pub other_class: Option<U32Hash>,
}

/// The type a value must have at this position. `element` selects a container's item type or a
/// map's value type over the property's own type.
pub fn value_slot(
    classes: &Classes,
    class: BinHash,
    property: BinHash,
    element: bool,
) -> Option<ValueSlot> {
    let property = classes.find_property(class, property)?;

    let kind = match (element, &property.container, &property.map) {
        (false, ..) => property.value_type,
        (true, Some(container), _) => container.value_type,
        (true, _, Some(map)) => map.value_type,
        (true, None, None) => return None,
    };

    Some(ValueSlot {
        kind: kind.into(),
        other_class: property.other_class,
    })
}

pub fn value_items<'a>(
    classes: &Classes,
    slot: ValueSlot,
    name_of: impl Fn(U32Hash) -> Option<Name<'a>>,
    snippets: bool,
    replace: Range,
) -> Vec<CompletionItem> {
    use PropertyKind as K;

    match slot.kind {
        K::Bool | K::BitBool => ["true", "false"]
            .into_iter()
            .map(|text| literal_item(text, replace))
            .collect(),
        K::Struct | K::Embedded => slot
            .other_class
            .map(|root| class_items(classes, root, name_of, snippets, replace))
            .unwrap_or_default(),
        K::Container | K::UnorderedContainer | K::Optional | K::Map => {
            vec![block_item(snippets, replace)]
        }
        _ => Vec::new(),
    }
}

pub fn property_items<'a>(
    classes: &Classes,
    class: BinHash,
    name_of: impl Fn(U32Hash) -> Option<Name<'a>>,
    replace: Range,
) -> Vec<CompletionItem> {
    inherited_properties(classes, class)
        .into_iter()
        .map(
            |FatProperty {
                 hash,
                 property,
                 class,
             }| {
                let label = name_of(hash).unwrap_or_else(|| hash_label(hash).into());
                let ty = property.rito_type().to_string();
                let new_text = format!("{label}: {ty} = ");

                CompletionItem {
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace,
                        new_text,
                    })),
                    sort_text: Some(sort_key(0, &label, name_of(hash).is_some(), hash)),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some(format!(": {ty}")),
                        description: None,
                    }),
                    kind: Some(CompletionItemKind::PROPERTY),
                    label: label.into_owned(),
                    data: Some(class.to_string().into()),
                    ..Default::default()
                }
            },
        )
        .collect()
}

pub fn type_item(
    classes: &Classes,
    class: BinHash,
    property: BinHash,
    replace: Range,
) -> Option<CompletionItem> {
    let ty = classes
        .find_property(class, property)?
        .rito_type()
        .to_string();

    Some(CompletionItem {
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text: ty.clone(),
        })),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        preselect: Some(true),
        label: ty,
        ..Default::default()
    })
}

pub fn root_property_items(missing: RootKindSet, replace: Range) -> Vec<CompletionItem> {
    missing
        .iter()
        .enumerate()
        .filter_map(|(order, kind)| {
            let ty = kind.expected_type()?.to_string();
            let label = kind.as_str();

            Some(CompletionItem {
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace,
                    new_text: format!("{label}: {ty} = "),
                })),
                sort_text: Some(format!("{order:02}")),
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!(": {ty}")),
                    description: None,
                }),
                kind: Some(CompletionItemKind::PROPERTY),
                label: label.to_owned(),
                ..Default::default()
            })
        })
        .collect()
}

/// The type a root kind expects, e.g. `u32` for `version`
pub fn root_type_item(kind: RootKind, replace: Range) -> Option<CompletionItem> {
    let ty = kind.expected_type()?.to_string();

    Some(CompletionItem {
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text: ty.clone(),
        })),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        preselect: Some(true),
        label: ty,
        ..Default::default()
    })
}

fn class_items<'a>(
    classes: &Classes,
    root: U32Hash,
    name_of: impl Fn(U32Hash) -> Option<Name<'a>>,
    snippets: bool,
    replace: Range,
) -> Vec<CompletionItem> {
    let mut found: Vec<_> = classes
        .concrete_descendants(root)
        .into_iter()
        .map(|(depth, hash)| {
            let name = name_of(hash);
            (
                depth,
                hash,
                name.is_some(),
                name.unwrap_or_else(|| hash_label(hash).into()),
            )
        })
        .collect();

    found.sort_unstable_by(
        |(a_depth, _, a_named, a_label), (b_depth, _, b_named, b_label)| {
            a_depth
                .cmp(b_depth)
                .then_with(|| b_named.cmp(a_named))
                .then_with(|| a_label.cmp(b_label))
        },
    );

    found
        .into_iter()
        .map(|(depth, hash, named, label)| {
            let base = classes
                .get(hash)
                .and_then(|class| class.base)
                .and_then(&name_of);

            let new_text = match snippets {
                true => format!("{label} {{\n\t$0\n}}"),
                false => format!("{label} {{\n}}"),
            };

            CompletionItem {
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace,
                    new_text,
                })),
                insert_text_format: snippets.then_some(InsertTextFormat::SNIPPET),
                sort_text: Some(sort_key(depth, &label, named, hash)),
                label_details: base.map(|base| CompletionItemLabelDetails {
                    detail: Some(format!(": {base}")),
                    description: None,
                }),
                documentation: named.then(|| wiki_link(&label)),
                kind: Some(CompletionItemKind::CLASS),
                label: label.into_owned(),
                ..Default::default()
            }
        })
        .collect()
}

struct FatProperty<'a> {
    hash: U32Hash,
    property: &'a Property,
    class: U32Hash,
}

/// A class's own properties plus everything it inherits, nearest declaration winning. The visited
/// set keeps a malformed dump with a cyclic base chain from looping forever.
fn inherited_properties(classes: &Classes, class: BinHash) -> Vec<FatProperty<'_>> {
    let mut out = Vec::new();
    let mut seen_classes = FxHashSet::default();
    let mut seen_properties = FxHashSet::default();
    let mut search = Some(U32Hash::from(class));

    while let Some(class_hash) = search.filter(|hash| seen_classes.insert(*hash)) {
        let Some(class) = classes.get(class_hash) else {
            break;
        };
        out.extend(
            class
                .properties
                .iter()
                .filter(|(hash, _)| seen_properties.insert(**hash))
                .map(|(hash, property)| FatProperty {
                    hash: *hash,
                    property,
                    class: class_hash,
                }),
        );
        search = class.base;
    }

    out
}

fn literal_item(text: &str, replace: Range) -> CompletionItem {
    CompletionItem {
        label: text.to_owned(),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text: text.to_owned(),
        })),
        kind: Some(CompletionItemKind::VALUE),
        ..Default::default()
    }
}

fn block_item(snippets: bool, replace: Range) -> CompletionItem {
    let new_text = match snippets {
        true => "{\n\t$0\n}".to_owned(),
        false => "{\n}".to_owned(),
    };

    CompletionItem {
        label: "{}".to_owned(),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text,
        })),
        insert_text_format: snippets.then_some(InsertTextFormat::SNIPPET),
        kind: Some(CompletionItemKind::STRUCT),
        ..Default::default()
    }
}

fn wiki_link(name: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "[{name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)",
            name.to_ascii_lowercase()
        ),
    })
}

fn hash_label(hash: U32Hash) -> String {
    format!("0x{:08x}", hash.0)
}

/// Nearest subclasses first, named before unnamed at the same distance, then alphabetical. `~`
/// sorts after every alphanumeric, which is what pushes the hash-only entries to the back.
fn sort_key(depth: u32, label: &str, named: bool, hash: U32Hash) -> String {
    match named {
        true => format!("{depth:02}{label}"),
        false => format!("{depth:02}~{:08x}", hash.0),
    }
}

#[cfg(test)]
mod tests;
