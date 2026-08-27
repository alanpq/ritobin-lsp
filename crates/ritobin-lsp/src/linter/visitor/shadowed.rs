use std::collections::HashMap;

use ltk_hash::{BinHash, WadHash};
use ltk_ritobin::ast::{Ast, Object, Value};

use crate::linter::Lint;

#[derive(PartialEq, Eq, Hash)]
enum MapKey {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    String(String),
    Hash(BinHash),
    WadChunkLink(WadHash),
    ObjectLink(BinHash),
}

impl MapKey {
    fn from_value(value: &Value) -> Option<MapKey> {
        Some(match value {
            Value::Bool(v) => MapKey::Bool(v.value),
            Value::I8(v) => MapKey::I8(v.value),
            Value::U8(v) => MapKey::U8(v.value),
            Value::I16(v) => MapKey::I16(v.value),
            Value::U16(v) => MapKey::U16(v.value),
            Value::I32(v) => MapKey::I32(v.value),
            Value::U32(v) => MapKey::U32(v.value),
            Value::I64(v) => MapKey::I64(v.value),
            Value::U64(v) => MapKey::U64(v.value),
            Value::String(v) => MapKey::String(v.value.clone()),
            Value::Hash(v) => MapKey::Hash(v.value),
            Value::WadChunkLink(v) => MapKey::WadChunkLink(v.value),
            Value::ObjectLink(v) => MapKey::ObjectLink(v.value),
            _ => return None,
        })
    }
}

pub fn check_root_objects(lints: &mut Vec<Lint>, ast: &Ast) {
    let mut seen = HashMap::new();

    for entry in ast.root_entries() {
        if let Some(shadowed_by) = seen.insert(entry.path_hash.value, entry.path_hash.span()) {
            lints.push(Lint::ShadowedEntry {
                entry: entry.path_hash.span(),
                shadowed_by,
            });
        }
    }
}

pub fn check_object(lints: &mut Vec<Lint>, object: &Object) {
    let mut seen = HashMap::new();
    for property in &object.properties {
        if let Some(shadowed_by) = seen.insert(property.name.value, property.name.span()) {
            lints.push(Lint::ShadowedEntry {
                entry: property.name.span(),
                shadowed_by,
            });
        }
    }
}

pub fn check_map_entries<'a>(lints: &mut Vec<Lint>, entry_keys: impl Iterator<Item = &'a Value>) {
    let mut seen = HashMap::new();
    for key in entry_keys {
        let Some(key_value) = MapKey::from_value(key) else {
            continue;
        };
        if let Some(shadowed_by) = seen.insert(key_value, key.span()) {
            lints.push(Lint::ShadowedEntry {
                entry: key.span(),
                shadowed_by,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Url;
    use ltk_ritobin::Cst;
    use meta_wiki::service::Classes;

    use super::*;
    use crate::{document::Document, linter::Linter};

    const SKIN: &str = "SkinCharacterDataProperties";
    const CHILD: &str = "SkinMeshDataProperties";

    /// The text each reported `ShadowedEntry`'s `entry` span points at.
    fn shadowed_entries(text: &str) -> Vec<String> {
        let document = Document::new(Url::parse("file:///t.rito").unwrap(), 0, text.to_owned());
        let classes = Classes::default();
        let ast = Cst::parse(&document.text).build_ast(&document.text);

        Linter::new(&classes, None)
            .run(&ast)
            .into_iter()
            .filter_map(|lint| match lint {
                Lint::ShadowedEntry { entry, .. } => Some(document.text[entry].to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shadowed_object_key() {
        let text = format!(
            "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinClassification: u32 = 1
    }}
    \"0x1\" = {SKIN} {{
        skinClassification: u32 = 2
    }}
}}
"
        );
        assert_eq!(shadowed_entries(&text), vec!["\"0x1\"".to_owned()]);
    }

    #[test]
    fn shadowed_property() {
        let text = format!(
            "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinClassification: u32 = 1
        skinClassification: u32 = 2
    }}
}}
"
        );
        assert_eq!(
            shadowed_entries(&text),
            vec!["skinClassification".to_owned()]
        );
    }

    #[test]
    fn shadowed_map_entry() {
        let text = format!(
            "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: map[hash,embed] = {{
            0xdeadbeef = {CHILD} {{
                boneName: string = \"root\"
            }}
            0xdeadbeef = {CHILD} {{
                boneName: string = \"neck\"
            }}
        }}
    }}
}}
"
        );
        assert_eq!(shadowed_entries(&text), vec!["0xdeadbeef".to_owned()]);
    }
}
