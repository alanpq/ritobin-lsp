use ltk_meta::PropertyKind;
use ltk_ritobin::{RitobinType, typecheck::visitor::RitoType};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

pub type Map<K, V> = HashMap<K, V>;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U32Hash(pub u32);

impl From<u32> for U32Hash {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Display for U32Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

impl Deref for U32Hash {
    type Target = u32;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for U32Hash {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromStr for U32Hash {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        Ok(U32Hash(u32::from_str_radix(s, 16)?))
    }
}

impl<'de> Deserialize<'de> for U32Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for U32Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DumpFile {
    /// League version ("unknown" if not known)
    pub version: String,
    pub classes: Map<U32Hash, Class>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyContainer {
    pub vtable: U32Hash,
    pub value_type: BinType,
    pub value_size: usize,
    pub fixed_size: Option<usize>,
    pub storage: Option<ContainerStorage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyMap {
    pub vtable: U32Hash,
    pub key_type: BinType,
    pub value_type: BinType,
    pub storage: MapStorage,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct Property {
    pub other_class: Option<U32Hash>,
    pub offset: u32,
    pub bitmask: u8,
    pub value_type: BinType,
    pub container: Option<PropertyContainer>,
    pub map: Option<PropertyMap>,
    pub unkptr: U32Hash,
}
impl Property {
    pub fn rito_type(&self) -> RitoType {
        let base = self.value_type.into();
        match (&self.container, &self.map) {
            (Some(container), None) => {
                (RitoType {
                    base,
                    subtypes: [Some(container.value_type.into()), None],
                })
            }
            (None, Some(map)) => {
                (RitoType {
                    base,
                    subtypes: [Some(map.key_type.into()), Some(map.value_type.into())],
                })
            }

            (Some(container), Some(map)) => unreachable!("property is both container and map?"),
            (None, None) => RitoType::simple(base),
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFunctions {
    pub upcast_secondary: Option<U32Hash>,
    pub constructor: Option<U32Hash>,
    pub destructor: Option<U32Hash>,
    pub inplace_constructor: Option<U32Hash>,
    pub inplace_destructor: Option<U32Hash>,
    pub register: Option<U32Hash>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFlags {
    pub interface: bool,
    pub value: bool,
    pub secondary_base: bool,
    pub unk5: bool,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct Class {
    pub base: Option<U32Hash>,
    pub secondary_bases: Map<U32Hash, u32>,
    pub secondary_children: Map<U32Hash, u32>,
    pub size: u32,
    pub alignment: u32,
    pub is: ClassFlags,
    #[serde(rename = "fn")]
    pub functions: ClassFunctions,
    pub properties: Map<U32Hash, Property>,
    pub defaults: Option<Map<U32Hash, serde_json::Value>>,
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

impl From<BinType> for PropertyKind {
    fn from(value: BinType) -> Self {
        match value {
            BinType::None => Self::None,
            BinType::Bool => Self::Bool,
            BinType::I8 => Self::I8,
            BinType::U8 => Self::U8,
            BinType::I16 => Self::I16,
            BinType::U16 => Self::U16,
            BinType::I32 => Self::I32,
            BinType::U32 => Self::U32,
            BinType::I64 => Self::I64,
            BinType::U64 => Self::U64,
            BinType::F32 => Self::F32,
            BinType::Vec2 => Self::Vector2,
            BinType::Vec3 => Self::Vector3,
            BinType::Vec4 => Self::Vector4,
            BinType::Mtx44 => Self::Matrix44,
            BinType::Color => Self::Color,
            BinType::String => Self::String,
            BinType::Hash => Self::Hash,
            BinType::File => Self::WadChunkLink,
            BinType::List => Self::Container,
            BinType::List2 => Self::UnorderedContainer,
            BinType::Pointer => Self::Struct,
            BinType::Embed => Self::Embedded,
            BinType::Link => Self::ObjectLink,
            BinType::Option => Self::Optional,
            BinType::Map => Self::Map,
            BinType::Flag => Self::BitBool,
        }
    }
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
