use lsp_types::SymbolKind as LspSymbolKind;
use ltk_ritobin::ast::Value;
use std::fmt::Debug;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Simple(SimpleKind),
    Property(Option<SimpleKind>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleKind {
    RootEntry,
    /// A struct/embedded object
    Object,

    Map,
    MapEntry,

    List,
    /// a `[0]`, `[1]`, etc. entry
    ListItem,

    String,
    Number,
}

impl SymbolKind {
    pub fn as_simple(self) -> Option<SimpleKind> {
        match self {
            SymbolKind::Simple(simple) => Some(simple),
            _ => None,
        }
    }

    pub fn holds(self, other: SimpleKind) -> bool {
        match self {
            SymbolKind::Simple(simple) => simple == other,
            SymbolKind::Property(simple) => simple == Some(other),
        }
    }
}

impl SimpleKind {
    pub fn from_value(v: &Value) -> Option<Self> {
        use ltk_meta::PropertyKind as K;
        Some(match v.kind()? {
            K::F32 | K::U8 | K::U16 | K::U32 | K::U64 | K::I8 | K::I16 | K::I32 | K::I64 => {
                SimpleKind::Number
            }

            K::String => SimpleKind::String,
            K::Container
            | K::UnorderedContainer
            | K::Color
            | K::Vector2
            | K::Vector3
            | K::Vector4 => SimpleKind::List,

            K::Struct | K::Embedded => SimpleKind::Object,
            K::Map => SimpleKind::Map,

            _ => return None,
        })
    }
}

impl Debug for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple(arg0) => arg0.fmt(f),
            Self::Property(arg0) => match arg0 {
                Some(arg0) => f.debug_tuple("Property").field(arg0).finish(),
                None => f.debug_tuple("Property").finish(),
            },
        }
    }
}

impl From<SimpleKind> for SymbolKind {
    fn from(value: SimpleKind) -> Self {
        Self::Simple(value)
    }
}
impl From<SimpleKind> for LspSymbolKind {
    fn from(value: SimpleKind) -> Self {
        match value {
            SimpleKind::RootEntry => Self::PACKAGE,
            SimpleKind::Object => Self::STRUCT,
            //
            SimpleKind::Map => Self::OBJECT,
            SimpleKind::MapEntry => Self::KEY,
            //
            SimpleKind::List => Self::ARRAY,
            SimpleKind::ListItem => Self::NUMBER,
            //
            SimpleKind::String => Self::STRING,
            SimpleKind::Number => Self::NUMBER,
        }
    }
}

impl From<SymbolKind> for LspSymbolKind {
    fn from(value: SymbolKind) -> Self {
        match value {
            SymbolKind::Simple(simple) => simple.into(),
            SymbolKind::Property(Some(simple)) => simple.into(),
            SymbolKind::Property(_) => Self::FIELD,
        }
    }
}
