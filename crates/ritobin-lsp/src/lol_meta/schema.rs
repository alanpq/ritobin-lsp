use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type Map<K, V> = HashMap<K, V>;

#[derive(Debug, Serialize, Deserialize)]
pub struct DumpFile {
    /// League version ("unknown" if not known)
    pub version: String,
    pub classes: Map<String, Class>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyContainer {
    pub vtable: String,
    pub value_type: BinType,
    pub value_size: usize,
    pub fixed_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<ContainerStorage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyMap {
    pub vtable: String,
    pub key_type: BinType,
    pub value_type: BinType,
    pub storage: MapStorage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Property {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_class: Option<String>,
    pub offset: u32,
    pub bitmask: u8,
    pub value_type: BinType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<PropertyContainer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<PropertyMap>,
    pub unkptr: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFunctions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upcast_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constructor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inplace_constructor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inplace_destructor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFlags {
    pub interface: bool,
    pub value: bool,
    pub secondary_base: bool,
    pub unk5: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Class {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub secondary_bases: Map<String, u32>,
    pub secondary_children: Map<String, u32>,
    pub size: u32,
    pub alignment: u32,
    pub is: ClassFlags,
    #[serde(rename = "fn")]
    pub functions: ClassFunctions,
    pub properties: Map<String, Property>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BinType {
    None = 0,
    Bool = 1,
    I8 = 2,
    U8 = 3,
    I16 = 4,
    U16 = 5,
    I32 = 6,
    U32 = 7,
    I64 = 8,
    U64 = 9,
    F32 = 10,
    Vec2 = 11,
    Vec3 = 12,
    Vec4 = 13,
    Mtx44 = 14,
    Color = 15,
    String = 16,
    Hash = 17,
    File = 18,
    List = 0x80,
    List2 = 0x80 | 1,
    Pointer = 0x80 | 2,
    Embed = 0x80 | 3,
    Link = 0x80 | 4,
    Option = 0x80 | 5,
    Map = 0x80 | 6,
    Flag = 0x80 | 7,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(C)]
pub enum ContainerStorage {
    UnknownVector,
    Option,
    Fixed,
    StdVector,
    RitoVector,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(C)]
pub enum MapStorage {
    UnknownMap,
    StdMap,
    StdUnorderedMap,
    RitoVectorMap,
}
