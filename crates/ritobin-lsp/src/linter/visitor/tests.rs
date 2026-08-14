use std::collections::HashMap;

use lsp_types::Url;
use ltk_hash::{BinHash, Hash as _};
use ltk_ritobin::{Cst, cst::visitor::VisitorExt as _};

use super::*;
use meta_wiki::schema::{Class, ClassFlags, Property, U32Hash};

const SKIN: &str = "SkinCharacterDataProperties";
const CHILD: &str = "SkinMeshDataProperties";

/// `SKIN` owns `skinClassification` and a `childList`; `CHILD` owns `boneName`.
fn classes() -> Classes {
    let prop = || Property::default();
    let class = |properties: Vec<&str>| Class {
        properties: properties
            .into_iter()
            .map(|name| (U32Hash::from(BinHash::hash_str(name)), prop()))
            .collect(),
        flags: ClassFlags::default(),
        ..Default::default()
    };

    Classes::new(HashMap::from([
        (
            U32Hash::from(BinHash::hash_str(SKIN)),
            class(vec![
                "loadscreen",
                "skinClassification",
                "childList",
                "skinMeshProperties",
            ]),
        ),
        (
            U32Hash::from(BinHash::hash_str("CensoredImage")),
            class(vec!["image"]),
        ),
        (
            U32Hash::from(BinHash::hash_str(CHILD)),
            class(vec!["boneName"]),
        ),
        (U32Hash(0x1234abcd), class(vec!["hexOnlyField"])),
    ]))
}

/// The text each reported `UnknownField` points at.
fn unknown_fields(text: &str) -> Vec<String> {
    let document = Document::new(Url::parse("file:///t.rito").unwrap(), 0, text.to_owned());
    let classes = classes();
    let cst = Cst::parse(&document.text);

    Linter::new(&document, &classes)
        .walk(&cst)
        .lints
        .into_iter()
        .map(|lint| match lint {
            Lint::UnknownField { span, .. } => document.text[span].to_owned(),
        })
        .collect()
}

#[test]
fn a_declared_property_is_not_flagged() {
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinClassification: u32 = 1
    }}
}}
"
    );
    assert_eq!(unknown_fields(&text), Vec::<String>::new());
}

#[test]
fn an_undeclared_property_is_flagged() {
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        skinClassification: u32 = 1
        notARealField: u32 = 2
    }}
}}
"
    );
    assert_eq!(unknown_fields(&text), vec!["notARealField".to_owned()]);
}

#[test]
fn properties_are_checked_against_the_innermost_class() {
    // `boneName` belongs to CHILD, not SKIN - and vice versa.
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{

        loadscreen: embed = CensoredImage {{
            image: string = \"hello\"
        }}

        skinMeshProperties = {CHILD} {{
            boneName: string = \"root\"
            skinClassification: u32 = 1
        }}
    }}
}}
"
    );
    assert_eq!(unknown_fields(&text), vec!["skinClassification".to_owned()]);
}

#[test]
fn keys_inside_a_container_block_are_not_properties() {
    // Map keys live in a container block, so no class declares them.
    let text = format!(
        "entries: map[hash,embed] = {{
    \"0x1\" = {SKIN} {{
        childList: map[hash,embed] = {{
            0xdeadbeef = {CHILD} {{
                boneName: string = \"root\"
            }}
        }}
    }}
}}
"
    );
    assert_eq!(unknown_fields(&text), Vec::<String>::new());
}

#[test]
fn an_unknown_class_is_not_checked() {
    let text = "entries: map[hash,embed] = {
    \"0x1\" = SomeClassNotInTheDump {
        whateverField: u32 = 1
    }
}
";
    assert_eq!(unknown_fields(text), Vec::<String>::new());
}

#[test]
fn a_hash_too_wide_for_a_binhash_is_skipped() {
    let text = format!(
        "entries: map[hash,embed] = {{
    0xdeadbeefdeadbeef = 0xcafebabecafebabe {{
        0x1234567890: u32 = 1
    }}
    \"0x2\" = {SKIN} {{
        0xfeedfacefeedface: u32 = 2
        notARealField: u32 = 3
    }}
}}
"
    );
    assert_eq!(unknown_fields(&text), vec!["notARealField".to_owned()]);
}

#[test]
fn a_hex_class_name_resolves_to_its_hash() {
    let text = "entries: map[hash,embed] = {
    \"0x1\" = 0x1234abcd {
        hexOnlyField: u32 = 1
        notARealField: u32 = 2
    }
}
";
    assert_eq!(unknown_fields(text), vec!["notARealField".to_owned()]);
}
