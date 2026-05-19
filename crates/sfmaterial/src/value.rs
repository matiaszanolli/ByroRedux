use crate::types::TypeReference;
use std::collections::BTreeMap;

/// Generic dynamic value — the CDB reader emits these as a tree.
/// Consumers (e.g. the material-extraction step in `byroredux/src/
/// asset_provider.rs`) walk by class name + field name without needing
/// a static schema.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Float(f32),
    Double(f64),
    String(String),
    /// `Ref` builtin — a tagged inner value plus a `TypeReference`
    /// that names the referent type (negative for builtin, non-negative
    /// for declared class). The C# reference reads the inner value
    /// inline when the referent is a struct, or resolves to a side-
    /// chunk OBJT when it's a user class.
    Ref(Ref),
    /// Variable-length homogeneous list — element type ID is captured
    /// in the chunk header but not preserved here (consumers branch on
    /// the leaf `Value` variant).
    List(Vec<Value>),
    /// Variable-length map. Keys can be any `Value`; the chunk header
    /// captures key + value types but they aren't preserved here.
    /// `Vec<(K, V)>` rather than `BTreeMap` because `Value` lacks `Ord`
    /// (would require recursive Ord on `f32` / `Ref` / `List` / itself).
    Map(Vec<(Value, Value)>),
    /// User-declared class instance. Field order is preserved by the
    /// insertion order on the inner map — `BTreeMap` because we want
    /// deterministic iteration without paying the IndexMap dep cost,
    /// and field-name lookup dominates the call pattern.
    Object(ObjectInstance),
}

/// A class-instance value — Gibbed's `ObjectInstance.Fields` mirror.
#[derive(Debug, Clone)]
pub struct ObjectInstance {
    /// Class name (resolved from the STRT offset at parse time).
    pub class_name: String,
    /// 32-bit content-addressed type id from the Class declaration.
    pub type_id: u32,
    /// Field-name → field-value, ordered. `BTreeMap` is deterministic
    /// for diffing snapshots; if hot-path callers want insertion-
    /// order they can sort the field list themselves from `class.fields`.
    pub fields: BTreeMap<String, Value>,
}

/// `Ref` builtin — the inner value is stored separately because it can
/// be of any `BuiltinType` (when `type_ref.is_builtin()`) or a user
/// object (carried as `Value::Object`).
#[derive(Debug, Clone)]
pub struct Ref {
    /// Type of the referent (negative = BuiltinType, non-negative = class).
    pub type_ref: TypeReference,
    /// The resolved inner value.
    pub inner: Box<Value>,
}
