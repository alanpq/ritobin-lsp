use ltk_hash::{BinHash, Hash as _};
use ltk_ritobin::{
    Cst,
    cst::{
        Kind as TreeKind, Node, NodeId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt as _},
    },
    parse::{Token, TokenKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorContext {
    PropertyKey {
        class: BinHash,
    },
    PropertyType {
        class: BinHash,
        property: BinHash,
    },
    PropertyValue {
        class: BinHash,
        property: BinHash,
    },
    ContainerItem {
        class: BinHash,
        property: BinHash,
    },
}

pub fn resolve(cst: &Cst, text: &str, offset: u32) -> Option<CursorContext> {
    let path = PathFinder {
        offset,
        stack: Vec::new(),
        path: None,
    }
    .walk(cst)
    .path?;

    classify(cst, text, &path, offset)
}

struct PathFinder {
    offset: u32,
    stack: Vec<NodeId>,
    path: Option<Vec<NodeId>>,
}

impl Visitor for PathFinder {
    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let Some(tree) = ctx.node(node) else {
            return Visit::Skip;
        };
        if !tree.span.contains(self.offset) {
            return Visit::Skip;
        }

        self.stack.push(node);
        if self.path.as_ref().is_none_or(|p| p.len() <= self.stack.len()) {
            self.path = Some(self.stack.clone());
        }
        Visit::Continue
    }

    fn exit_tree(&mut self, _ctx: &VisitCtx, node: NodeId) -> Visit {
        if self.stack.last() == Some(&node) {
            self.stack.pop();
        }
        Visit::Continue
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Class {
        class: BinHash,
    },
    Container {
        class: BinHash,
        property: BinHash,
    },
    Opaque,
}

enum Region {
    Key,
    Type,
    Value,
}

fn classify(cst: &Cst, text: &str, path: &[NodeId], offset: u32) -> Option<CursorContext> {
    let mut scope = None;
    let mut scope_idx = 0;

    for i in 1..path.len() {
        let parent = cst.node(path[i - 1])?;
        let next = match cst.node(path[i])?.kind {
            TreeKind::Block => match parent.kind {
                TreeKind::Class => match class_hash(cst, text, parent) {
                    Some(class) => Scope::Class { class },
                    None => Scope::Opaque,
                },
                TreeKind::EntryValue => owning_entry(cst, text, path, i)
                    .zip(match scope {
                        Some(Scope::Class { class }) => Some(class),
                        _ => None,
                    })
                    .map_or(Scope::Opaque, |(property, class)| Scope::Container {
                        class,
                        property,
                    }),
                _ => Scope::Opaque,
            },
            TreeKind::ListItemBlock => Scope::Opaque,
            _ => continue,
        };

        scope = Some(next);
        scope_idx = i;
    }

    let entry = path[scope_idx..].iter().rev().find_map(|&id| {
        let node = cst.node(id)?;
        (node.kind == TreeKind::Entry).then_some(node)
    });

    match (scope?, entry) {
        (Scope::Class { class }, None) => Some(CursorContext::PropertyKey { class }),
        (Scope::Class { class }, Some(entry)) => match region(cst, entry, offset) {
            Region::Key => Some(CursorContext::PropertyKey { class }),
            Region::Type => Some(CursorContext::PropertyType {
                class,
                property: entry_key(cst, text, entry)?,
            }),
            Region::Value => Some(CursorContext::PropertyValue {
                class,
                property: entry_key(cst, text, entry)?,
            }),
        },
        (Scope::Container { class, property }, None) => {
            Some(CursorContext::ContainerItem { class, property })
        }
        (Scope::Container { class, property }, Some(entry)) => {
            match region(cst, entry, offset) {
                Region::Value => Some(CursorContext::ContainerItem { class, property }),
                _ => None,
            }
        }
        (Scope::Opaque, _) => None,
    }
}

fn owning_entry(cst: &Cst, text: &str, path: &[NodeId], block_idx: usize) -> Option<BinHash> {
    let entry = cst.node(*path.get(block_idx.checked_sub(2)?)?)?;
    (entry.kind == TreeKind::Entry)
        .then(|| entry_key(cst, text, entry))
        .flatten()
}

fn region(cst: &Cst, entry: &Node, offset: u32) -> Region {
    let (mut colon, mut eq) = (None, None);

    for token in entry.children.get(cst).iter().filter_map(|c| c.token(cst)) {
        match token.kind {
            TokenKind::Colon => colon = colon.or(Some(token.span)),
            TokenKind::Eq => eq = eq.or(Some(token.span)),
            _ => {}
        }
    }

    match (colon, eq) {
        (_, Some(eq)) if offset >= eq.end => Region::Value,
        (Some(colon), _) if offset > colon.start => Region::Type,
        _ => Region::Key,
    }
}

fn entry_key(cst: &Cst, text: &str, entry: &Node) -> Option<BinHash> {
    let key = entry.children.get(cst).iter().find_map(|child| {
        let node = child.tree(cst)?;
        (node.kind == TreeKind::EntryKey).then_some(node)
    })?;

    hash_token(text, key.children.get(cst).first()?.token(cst)?)
}

fn class_hash(cst: &Cst, text: &str, class: &Node) -> Option<BinHash> {
    hash_token(text, class.children.get(cst).first()?.token(cst)?)
}

fn hash_token(text: &str, token: &Token) -> Option<BinHash> {
    match token.kind {
        TokenKind::Name => Some(BinHash::hash_str(&text[token.span])),
        TokenKind::HexLit => BinHash::from_str_radix(text[token.span].trim_start_matches("0x"), 16).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
