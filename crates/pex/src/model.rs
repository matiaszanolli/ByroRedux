//! The decoded `.pex` data model — a structural port of Champollion's
//! `Pex/` headers (`Binary`, `Header`, `Object`, `Property`, `State`,
//! `Function`, `Instruction`, …).
//!
//! Two deliberate departures from the C++ for Rust ergonomics:
//!
//! 1. **Strings are resolved eagerly.** Champollion threads a
//!    `StringTable::Index` (table pointer + `u16`) through every field and
//!    resolves lazily. Every index actually read from a `.pex` is in-range
//!    (the reader rejects out-of-range indices, matching the C++), so we
//!    resolve to owned `String`s at read time. The raw table is still kept
//!    on [`Pex::string_table`] for diagnostics / round-trip.
//! 2. **`Value` is a plain enum**, not a tagged union — same six variants.

/// Which game's `.pex` dialect this file is. Determined by magic
/// endianness + `game_id` (Champollion `Binary::ScriptType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptType {
    /// Big-endian magic — the original Skyrim layout (no const flags,
    /// struct info, or extended debug info).
    Skyrim,
    /// Little-endian, `game_id` != 3/4.
    Fallout4,
    /// Little-endian, `game_id` == 3.
    Fallout76,
    /// Little-endian, `game_id` == 4.
    Starfield,
}

impl ScriptType {
    /// Skyrim is the only big-endian / "old format" dialect; the rest add
    /// the FO4-era fields (const flags, struct infos, property groups).
    pub fn is_skyrim(self) -> bool {
        matches!(self, ScriptType::Skyrim)
    }
}

/// `.pex` file header (Champollion `Pex::Header`).
#[derive(Debug, Clone, Default)]
pub struct Header {
    pub major_version: u8,
    pub minor_version: u8,
    /// `1` Skyrim … `3` FO76, `4` Starfield (FO4 reports `0`/varies).
    pub game_id: u16,
    /// Compilation `time_t` (seconds since epoch), verbatim.
    pub compilation_time: i64,
    pub source_file_name: String,
    pub user_name: String,
    pub computer_name: String,
}

/// A Papyrus operand / literal (Champollion `Pex::Value`).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    /// `ValueType::None` (0) — void / absent operand.
    #[default]
    None,
    /// `ValueType::Identifier` (1) — a name reference (variable, temp,
    /// type, label). Resolved from the string table.
    Identifier(String),
    /// `ValueType::String` (2) — a string literal.
    Str(String),
    /// `ValueType::Integer` (3).
    Integer(i32),
    /// `ValueType::Float` (4).
    Float(f32),
    /// `ValueType::Bool` (5).
    Bool(bool),
}

impl Value {
    /// The identifier text, if this is an `Identifier`. The decompiler
    /// reaches for this constantly (operands are mostly identifiers).
    pub fn as_identifier(&self) -> Option<&str> {
        match self {
            Value::Identifier(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }
}

/// A `(name, type)` pair — function parameters and locals
/// (Champollion `Pex::TypedName`).
#[derive(Debug, Clone)]
pub struct TypedName {
    pub name: String,
    pub type_name: String,
}

/// One bytecode instruction (Champollion `Pex::Instruction`): an opcode,
/// its fixed operands, and (for the var-arg opcodes) a trailing operand
/// list.
#[derive(Debug, Clone)]
pub struct Instruction {
    pub op: super::opcode::OpCode,
    /// Fixed operands — exactly `op.arg_count()` of them.
    pub args: Vec<Value>,
    /// Trailing variable operands (call args, guard lists). Empty unless
    /// `op.has_varargs()`.
    pub var_args: Vec<Value>,
}

/// A function body (Champollion `Pex::Function`) — signature flags plus
/// the parameter/local tables and the instruction stream the decompiler
/// consumes.
#[derive(Debug, Clone, Default)]
pub struct Function {
    /// Empty for property getter/setter bodies, which carry no name of
    /// their own (the property owns it). Set for named functions/events.
    pub name: String,
    pub return_type_name: String,
    pub doc_string: String,
    pub user_flags: u32,
    /// Raw flag byte: bit 0 = Global, bit 1 = Native.
    pub flags: u8,
    pub params: Vec<TypedName>,
    pub locals: Vec<TypedName>,
    pub instructions: Vec<Instruction>,
}

impl Function {
    pub fn is_global(&self) -> bool {
        self.flags & 0x01 != 0
    }
    pub fn is_native(&self) -> bool {
        self.flags & 0x02 != 0
    }
}

/// `PropertyFlag` bits (Champollion `Pex::PropertyFlag`).
pub mod property_flag {
    pub const READ: u8 = 1 << 0;
    pub const WRITE: u8 = 1 << 1;
    pub const AUTOVAR: u8 = 1 << 2;
}

/// A script property (Champollion `Pex::Property`). Either auto (backed by
/// a variable named in `auto_var_name`) or full (with read/write function
/// bodies).
#[derive(Debug, Clone, Default)]
pub struct Property {
    pub name: String,
    pub type_name: String,
    pub doc_string: String,
    pub user_flags: u32,
    /// `property_flag::{READ,WRITE,AUTOVAR}`.
    pub flags: u8,
    /// Set iff `AUTOVAR` — the backing variable's name.
    pub auto_var_name: Option<String>,
    /// Full-property getter body (present iff `READ` and not `AUTOVAR`).
    pub read_function: Option<Function>,
    /// Full-property setter body (present iff `WRITE` and not `AUTOVAR`).
    pub write_function: Option<Function>,
}

impl Property {
    pub fn is_readable(&self) -> bool {
        self.flags & property_flag::READ != 0
    }
    pub fn is_writable(&self) -> bool {
        self.flags & property_flag::WRITE != 0
    }
    pub fn has_auto_var(&self) -> bool {
        self.flags & property_flag::AUTOVAR != 0
    }
}

/// A named state and the functions/events it overrides
/// (Champollion `Pex::State`). The empty-named state is the script's
/// default (auto) state.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub name: String,
    pub functions: Vec<Function>,
}

/// A script variable (Champollion `Pex::Variable`) — including the
/// synthetic `::temp`/`::mangled` ones the compiler emits.
#[derive(Debug, Clone, Default)]
pub struct Variable {
    pub name: String,
    pub type_name: String,
    pub user_flags: u32,
    pub default_value: Value,
    /// FO4+ const flag byte (`0` on Skyrim, which lacks the field).
    pub const_flag: u8,
}

/// One member of a struct definition (Champollion `StructInfo::Member`).
#[derive(Debug, Clone, Default)]
pub struct StructMember {
    pub name: String,
    pub type_name: String,
    pub user_flags: u32,
    pub value: Value,
    pub const_flag: u8,
    pub doc_string: String,
}

/// An FO4+ struct definition (Champollion `Pex::StructInfo`).
#[derive(Debug, Clone, Default)]
pub struct StructInfo {
    pub name: String,
    pub members: Vec<StructMember>,
}

/// A Starfield guard declaration (Champollion `Pex::Guard`). Name only.
#[derive(Debug, Clone, Default)]
pub struct Guard {
    pub name: String,
}

/// One object (script class) in the file (Champollion `Pex::Object`).
/// A `.pex` usually holds exactly one.
#[derive(Debug, Clone, Default)]
pub struct Object {
    pub name: String,
    pub parent_class_name: String,
    pub doc_string: String,
    /// FO4+ const flag byte (`0` on Skyrim).
    pub const_flag: u8,
    pub user_flags: u32,
    pub auto_state_name: String,
    /// FO4+ struct definitions (empty on Skyrim).
    pub struct_infos: Vec<StructInfo>,
    pub variables: Vec<Variable>,
    /// Starfield guards (empty otherwise).
    pub guards: Vec<Guard>,
    pub properties: Vec<Property>,
    pub states: Vec<State>,
}

/// `DebugInfo::FunctionType` (Champollion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    Method,
    Getter,
    Setter,
}

/// Per-function source-line mapping inside the debug-info block.
#[derive(Debug, Clone, Default)]
pub struct FunctionInfo {
    pub object_name: String,
    pub state_name: String,
    pub function_name: String,
    /// `Method` / `Getter` / `Setter`; `Method` when the byte is unknown.
    pub function_type: Option<FunctionType>,
    /// One source line per instruction — the decompiler's boolean-operator
    /// reconstruction uses these to avoid merging across source lines.
    pub line_numbers: Vec<u16>,
}

/// Optional debug-info block (Champollion `Pex::DebugInfo`). Absent when
/// the script was compiled without debug info (the leading flag byte is
/// `0`); the property-group / struct-order tables are FO4+-only.
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    pub present: bool,
    pub modification_time: i64,
    pub function_infos: Vec<FunctionInfo>,
    // Property groups + struct orders (FO4+) are parsed-and-skipped for
    // now — Phase 1 doesn't consume them. Re-add typed fields if a
    // recognizer ever needs editor grouping.
}

/// A user-flag definition (`name → bit index`), Champollion `Pex::UserFlag`.
#[derive(Debug, Clone, Default)]
pub struct UserFlag {
    pub name: String,
    pub flag_index: u8,
}

/// A fully decoded `.pex` file (Champollion `Pex::Binary`).
#[derive(Debug, Clone)]
pub struct Pex {
    pub script_type: ScriptType,
    pub header: Header,
    /// The raw string table, kept for diagnostics; field values elsewhere
    /// are already resolved against it.
    pub string_table: Vec<String>,
    pub debug_info: DebugInfo,
    pub user_flags: Vec<UserFlag>,
    pub objects: Vec<Object>,
}

impl Pex {
    /// The single object a `.pex` almost always carries (the script class).
    pub fn main_object(&self) -> Option<&Object> {
        self.objects.first()
    }
}
