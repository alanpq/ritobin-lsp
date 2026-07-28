use ltk_hash::BinHash;
use ltk_meta::PropertyKind;
use ltk_ritobin::RitoType;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

pub type Map<K, V> = HashMap<K, V>;
use std::str::FromStr;

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U32Hash(pub u32);

impl From<u32> for U32Hash {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<BinHash> for U32Hash {
    fn from(value: BinHash) -> Self {
        Self(*value)
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

/// Latest version we know about
pub const FORMAT_VERSION: u32 = 2;
/// Version of dumps written before the field existed.
pub const FORMAT_VERSION_UNVERSIONED: u32 = 0;

#[derive(Debug, Serialize, Deserialize)]
pub struct DumpFile {
    /// Schema version of this file. Absent in every dump written before the
    /// field was introduced, which reads back as [`FORMAT_VERSION_UNVERSIONED`].
    #[serde(rename = "formatVersion", default)]
    pub format_version: u32,
    /// Game version string (e.g., "14.24.6442327").
    pub version: String,
    /// Map of class hash (hex string) to class definition.
    pub classes: Map<U32Hash, Class>,
}

#[skip_serializing_none]
#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyContainer {
    /// Vtable offset (TODO: don't use U32Hash type here)
    pub vtable: U32Hash,
    /// Type of values in the container.
    pub value_type: BinType,
    /// Size of each value in bytes.
    pub value_size: usize,
    /// Fixed size for fixed-length arrays, if applicable.
    pub fixed_size: Option<usize>,
    /// Storage type, if known.
    pub storage: Option<ContainerStorage>,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct PropertyMap {
    /// Vtable offset (TODO: don't use U32Hash type here)
    pub vtable: U32Hash,
    /// Type of keys in the map.
    pub key_type: BinType,
    /// Type of values in the map.
    pub value_type: BinType,
    /// Storage type.
    pub storage: MapStorage,
}

#[skip_serializing_none]
#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Property {
    /// Other class hash for Link/Pointer/Embed types.
    pub other_class: Option<U32Hash>,
    /// Byte offset within the class.
    pub offset: u32,
    /// Bitmask for Flag types.
    pub bitmask: u8,
    /// The property's value type.
    pub value_type: BinType,
    /// Container info for List/Option types.
    pub container: Option<PropertyContainer>,
    /// Map info for Map types.
    pub map: Option<PropertyMap>,
    /// Unknown pointer (always "0x0").
    pub unkptr: U32Hash,
}
impl Property {
    pub fn rito_type(&self) -> RitoType {
        let base = self.value_type.into();
        match (&self.container, &self.map) {
            (Some(container), None) => RitoType {
                base,
                subtypes: [Some(container.value_type.into()), None],
            },
            (None, Some(map)) => RitoType {
                base,
                subtypes: [Some(map.key_type.into()), Some(map.value_type.into())],
            },

            (Some(_container), Some(_map)) => unreachable!("property is both container and map?"),
            (None, None) => RitoType::simple(base),
        }
    }
}

#[skip_serializing_none]
#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFunctions {
    /// Upcast to secondary base function.
    pub upcast_secondary: Option<U32Hash>,
    /// Constructor function.
    pub constructor: Option<U32Hash>,
    /// Destructor function.
    pub destructor: Option<U32Hash>,
    /// In-place constructor function.
    pub inplace_constructor: Option<U32Hash>,
    /// In-place destructor function.
    pub inplace_destructor: Option<U32Hash>,
    /// Register function.
    pub register: Option<U32Hash>,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassFlags {
    /// True if this is an interface (no constructor).
    pub interface: bool,
    /// True if this is a value type.
    pub value: bool,
    /// True if this is a secondary base class.
    pub secondary_base: bool,
    /// An "already processed" marker - the metaclass registry sets it when it finalises an entry.
    ///
    /// Written as `unk5` before format version 2, which is what every dump up to
    /// 16.14 carries
    #[serde(alias = "unk5")]
    pub finalized: bool,
}

#[skip_serializing_none]
#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize)]
pub struct Class {
    /// Base class hash, if any.
    pub base: Option<U32Hash>,
    /// Secondary base classes with their offsets.
    pub secondary_bases: Map<U32Hash, u32>,
    /// Secondary child classes with their offsets.
    pub secondary_children: Map<U32Hash, u32>,
    /// Size of the class in bytes.
    pub size: u32,
    /// Alignment requirement.
    pub alignment: u32,

    /// Class flags.
    #[serde(rename = "is")]
    pub flags: ClassFlags,

    /// Class function pointers.
    #[serde(rename = "fn")]
    pub functions: ClassFunctions,

    /// Map of property hash to property definition.
    pub properties: Map<U32Hash, Property>,
    /// Default values for properties (None for interfaces).
    pub defaults: Option<Map<U32Hash, serde_json::Value>>,
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BinType {
    #[cfg_attr(test, default)]
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

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(C)]
pub enum MapStorage {
    #[cfg_attr(test, default)]
    UnknownMap,
    StdMap,
    StdUnorderedMap,
    RitoVectorMap,
}
