//! CDB type system: builtin primitive tags + user-declared `Class` /
//! `Field` definitions.
//!
//! `TypeReference.id` is signed: negative values map to [`BuiltinType`]
//! (the bottom byte names the primitive); non-negative values index the
//! `TYPE` chunk's declared classes.

/// Built-in primitive kinds. The on-disk u32 reads as `0xFFFFFF##`
/// with the low byte selecting the kind. Cast `TypeReference.id`
/// to `u32`, then transmute into this enum via `BuiltinType::from_u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BuiltinType {
    Null = 0xFFFFFF01,
    String = 0xFFFFFF02,
    List = 0xFFFFFF03,
    Map = 0xFFFFFF04,
    Ref = 0xFFFFFF05,
    Int8 = 0xFFFFFF08,
    UInt8 = 0xFFFFFF09,
    Int16 = 0xFFFFFF0A,
    UInt16 = 0xFFFFFF0B,
    Int32 = 0xFFFFFF0C,
    UInt32 = 0xFFFFFF0D,
    Int64 = 0xFFFFFF0E,
    UInt64 = 0xFFFFFF0F,
    Bool = 0xFFFFFF10,
    Float = 0xFFFFFF11,
    Double = 0xFFFFFF12,
}

impl BuiltinType {
    /// Decode a raw u32 (= `TypeReference.id as u32` when the id is
    /// negative). Returns `Err` if the byte pattern doesn't match any
    /// of the documented primitive tags.
    pub fn from_u32(raw: u32) -> crate::Result<Self> {
        Ok(match raw {
            0xFFFFFF01 => BuiltinType::Null,
            0xFFFFFF02 => BuiltinType::String,
            0xFFFFFF03 => BuiltinType::List,
            0xFFFFFF04 => BuiltinType::Map,
            0xFFFFFF05 => BuiltinType::Ref,
            0xFFFFFF08 => BuiltinType::Int8,
            0xFFFFFF09 => BuiltinType::UInt8,
            0xFFFFFF0A => BuiltinType::Int16,
            0xFFFFFF0B => BuiltinType::UInt16,
            0xFFFFFF0C => BuiltinType::Int32,
            0xFFFFFF0D => BuiltinType::UInt32,
            0xFFFFFF0E => BuiltinType::Int64,
            0xFFFFFF0F => BuiltinType::UInt64,
            0xFFFFFF10 => BuiltinType::Bool,
            0xFFFFFF11 => BuiltinType::Float,
            0xFFFFFF12 => BuiltinType::Double,
            _ => return Err(crate::Error::UnsupportedBuiltin { raw }),
        })
    }

    /// `List` / `Map` are written as separate chunks rather than inline
    /// — the value-reader must queue them up as side-chunk consumers
    /// instead of reading bytes from the current cursor.
    pub fn is_chunk(self) -> bool {
        matches!(self, BuiltinType::List | BuiltinType::Map)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassFlags(pub u16);

impl ClassFlags {
    pub const IS_USER: u16 = 1 << 2;
    pub const IS_STRUCT: u16 = 1 << 3;
    pub const KNOWN: u16 = Self::IS_USER | Self::IS_STRUCT;

    pub fn is_user(self) -> bool {
        self.0 & Self::IS_USER != 0
    }

    pub fn is_struct(self) -> bool {
        self.0 & Self::IS_STRUCT != 0
    }
}

/// Reference to a type: negative `id` is a [`BuiltinType`] tag, non-
/// negative `id` indexes the `TYPE` chunk's declared classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeReference {
    pub id: i32,
}

impl TypeReference {
    pub fn new(id: i32) -> Self {
        Self { id }
    }

    pub fn is_builtin(self) -> bool {
        self.id < 0
    }

    pub fn as_builtin(self) -> crate::Result<BuiltinType> {
        debug_assert!(self.is_builtin());
        BuiltinType::from_u32(self.id as u32)
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeReference,
    pub offset: u16,
    pub size: u16,
}

#[derive(Debug, Clone)]
pub struct Class {
    /// Offset into the STRT string table — preserved as the canonical
    /// type-map key (per Gibbed `typeMap.Add(type.NameOffset, type)`).
    pub name_offset: i32,
    /// Resolved class name (ASCII, NUL-terminated in STRT).
    pub name: String,
    /// 32-bit type id (NOT the same as the position-indexed `TYPE` slot;
    /// this is a content-addressed hash assigned by Bethesda's
    /// reflection at build time).
    pub type_id: u32,
    pub flags: ClassFlags,
    pub fields: Vec<Field>,
}
