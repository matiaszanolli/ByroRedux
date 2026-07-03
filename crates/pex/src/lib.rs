//! `byroredux-pex` — a reader for compiled Papyrus (`.pex`) files, the
//! vanilla-runtime script format shipped in Skyrim / SSE / FO4 / FO76 /
//! Starfield BSAs and BA2s.
//!
//! This is **Phase 1 of M47.2**: decode a `.pex` into a structured
//! [`Pex`] model — header, string table, debug info, and objects (script
//! classes) with their properties, states, and functions down to the
//! per-function bytecode [`Instruction`] stream. It is a structural port
//! of the `Pex/` half of [Champollion](https://github.com/Orvid/Champollion),
//! the reference `.pex` → `.psc` decompiler.
//!
//! What it does **not** do: reconstruct control flow / expressions from the
//! bytecode (that is the Phase 2 decompiler, which consumes
//! [`Function::instructions`]), nor emit `.psc` text (ByroRedux lowers the
//! decompiled tree to `byroredux_papyrus::ast::Script` instead).
//!
//! ```no_run
//! let bytes = std::fs::read("DefaultRumbleOnActivate.pex").unwrap();
//! let pex = byroredux_pex::parse(&bytes).unwrap();
//! let object = pex.main_object().unwrap();
//! println!("script {} extends {}", object.name, object.parent_class_name);
//! ```
//!
//! See `docs/engine/m47-2-design.md` for where this sits in the pipeline.

pub mod decompile;
mod model;
mod opcode;
mod reader;

pub use model::*;
pub use opcode::{OpCode, MAX_OPCODE};

use thiserror::Error;

/// Why a `.pex` failed to decode. Every variant is a structural defect
/// caught before any partial model escapes — the reader never returns a
/// half-built [`Pex`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PexError {
    /// Leading magic was neither the LE (`0xFA57C0DE`) nor BE
    /// (`0xDEC057FA`) marker — not a `.pex`, or byte-swapped junk.
    #[error("invalid .pex magic 0x{magic:08X}")]
    BadMagic { magic: u32 },

    /// Ran off the end of the buffer mid-field.
    #[error("unexpected end of .pex at byte {offset}")]
    UnexpectedEof { offset: usize },

    /// A `u16` string-table index pointed past the table.
    #[error("string index {index} out of range (table has {table_len} entries)")]
    BadStringIndex { index: usize, table_len: usize },

    /// A value's type tag wasn't one of the six known `ValueType`s.
    #[error("invalid value type tag {ty}")]
    BadValueType { ty: u8 },

    /// An instruction's leading byte was `>= MAX_OPCODE`.
    #[error("invalid opcode byte {byte}")]
    BadOpcode { byte: u8 },

    /// A var-arg opcode's count operand wasn't a non-negative integer.
    #[error("var-arg count operand was not a non-negative integer")]
    BadVarArgCount,
}

/// Decode a `.pex` byte buffer into a [`Pex`] model.
///
/// Endianness (and thus game dialect) is auto-detected from the magic.
pub fn parse(bytes: &[u8]) -> Result<Pex, PexError> {
    reader::Reader::new(bytes).read_binary()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny hand-built `.pex` writer, just enough to exercise the reader
    /// end-to-end. Mirrors the on-disk layout field-for-field so the
    /// round-trip pins the reader's read order. `big_endian` selects the
    /// Skyrim dialect (BE magic + BE multi-byte fields); the magic itself is
    /// always written little-endian regardless (the reader's `u32_opt(true)`
    /// decodes it that way before endianness is known — see `read_header`).
    struct PexWriter {
        buf: Vec<u8>,
        strings: Vec<String>,
        big_endian: bool,
    }

    impl PexWriter {
        fn new() -> Self {
            Self { buf: Vec::new(), strings: Vec::new(), big_endian: false }
        }
        fn new_be() -> Self {
            Self { buf: Vec::new(), strings: Vec::new(), big_endian: true }
        }
        fn u8(&mut self, v: u8) { self.buf.push(v); }
        fn u16(&mut self, v: u16) {
            let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
            self.buf.extend_from_slice(&b);
        }
        fn u32(&mut self, v: u32) {
            let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
            self.buf.extend_from_slice(&b);
        }
        fn i64(&mut self, v: i64) {
            let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
            self.buf.extend_from_slice(&b);
        }
        /// Magic is always little-endian on disk regardless of dialect.
        fn magic(&mut self, v: u32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        fn string(&mut self, s: &str) {
            self.u16(s.len() as u16);
            self.buf.extend_from_slice(s.as_bytes());
        }
        /// Intern a string and return its table index.
        fn intern(&mut self, s: &str) -> u16 {
            if let Some(i) = self.strings.iter().position(|x| x == s) {
                return i as u16;
            }
            self.strings.push(s.to_string());
            (self.strings.len() - 1) as u16
        }
        fn sidx(&mut self, s: &str) {
            let i = self.intern(s);
            self.u16(i);
        }
    }

    /// Build a one-object FO4 `.pex`: a script `Foo extends ObjectReference`
    /// with one auto property and one event holding a couple of
    /// instructions (one with var-args).
    fn build_sample() -> Vec<u8> {
        // Pre-intern every string so the table is written before the body
        // that references it (the on-disk order: table first, then refs).
        let mut w = PexWriter::new();
        for s in [
            "Foo", "ObjectReference", "MyProp", "Int", "OnActivate",
            "akActivator", "None", "::temp0", "Self", "Bar",
        ] {
            w.intern(s);
        }

        // ── header (magic written LE) ──
        w.magic(0xFA57_C0DE);
        w.u8(3); // major
        w.u8(2); // minor
        w.u16(0); // game_id (FO4)
        w.i64(1_700_000_000); // compilation time
        w.string("Foo.psc");
        w.string("user");
        w.string("computer");

        // ── string table ──
        let table = w.strings.clone();
        w.u16(table.len() as u16);
        for s in &table {
            w.string(s);
        }

        // ── debug info: absent ──
        w.u8(0);

        // ── user flags: none ──
        w.u16(0);

        // ── objects: 1 ──
        w.u16(1);
        w.sidx("Foo"); // name
        w.u32(0); // size (ignored)
        w.sidx("ObjectReference"); // parent
        w.sidx("None"); // doc string ("" would need an interned empty; reuse None)
        w.u8(0); // const flag (FO4)
        w.u32(0); // user flags
        w.sidx("None"); // auto-state name
        // struct infos (FO4): 0
        w.u16(0);
        // variables: 0
        w.u16(0);
        // (no guards — not Starfield)
        // properties: 1 auto property
        w.u16(1);
        w.sidx("MyProp"); // name
        w.sidx("Int"); // type
        w.sidx("None"); // doc
        w.u32(0); // user flags
        w.u8(property_flag::READ | property_flag::WRITE | property_flag::AUTOVAR);
        w.sidx("Bar"); // auto var name
        // states: 1 (the empty default state) with one function
        w.u16(1);
        w.sidx("None"); // state name (reuse "None" as a non-empty token)
        w.u16(1); // function count
        w.sidx("OnActivate"); // function name
        // function body:
        w.sidx("None"); // return type
        w.sidx("None"); // doc
        w.u32(0); // user flags
        w.u8(0); // flags
        // params: 1
        w.u16(1);
        w.sidx("akActivator");
        w.sidx("ObjectReference");
        // locals: 1
        w.u16(1);
        w.sidx("::temp0");
        w.sidx("Int");
        // instructions: 2
        w.u16(2);
        // (1) iadd ::temp0, 1, 2   (3 fixed args, no varargs)
        w.u8(OpCode::IAdd as u8);
        w.u8(1);
        w.sidx("::temp0"); // identifier value
        w.u8(3);
        w.u32(1i32 as u32); // integer
        w.u8(3);
        w.u32(2i32 as u32);
        // (2) callmethod Bar, Self, ::temp0  + 1 vararg
        w.u8(OpCode::CallMethod as u8);
        w.u8(1);
        w.sidx("Bar"); // method name (identifier)
        w.u8(1);
        w.sidx("Self"); // object
        w.u8(1);
        w.sidx("::temp0"); // result dest
        // varargs: count=1
        w.u8(3);
        w.u32(1i32 as u32);
        w.u8(5); // bool arg
        w.u8(1);

        w.buf
    }

    #[test]
    fn parses_a_handbuilt_fo4_pex() {
        let bytes = build_sample();
        let pex = parse(&bytes).expect("sample .pex parses");

        assert_eq!(pex.script_type, ScriptType::Fallout4);
        assert_eq!(pex.header.major_version, 3);
        assert_eq!(pex.header.source_file_name, "Foo.psc");
        assert!(!pex.debug_info.present);

        let obj = pex.main_object().expect("one object");
        assert_eq!(obj.name, "Foo");
        assert_eq!(obj.parent_class_name, "ObjectReference");

        assert_eq!(obj.properties.len(), 1);
        let prop = &obj.properties[0];
        assert_eq!(prop.name, "MyProp");
        assert_eq!(prop.type_name, "Int");
        assert!(prop.has_auto_var());
        assert_eq!(prop.auto_var_name.as_deref(), Some("Bar"));

        assert_eq!(obj.states.len(), 1);
        let func = &obj.states[0].functions[0];
        assert_eq!(func.name, "OnActivate");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "akActivator");
        assert_eq!(func.locals[0].name, "::temp0");

        assert_eq!(func.instructions.len(), 2);
        let iadd = &func.instructions[0];
        assert_eq!(iadd.op, OpCode::IAdd);
        assert_eq!(iadd.args.len(), 3);
        assert_eq!(iadd.args[0], Value::Identifier("::temp0".into()));
        assert_eq!(iadd.args[1], Value::Integer(1));
        assert_eq!(iadd.args[2], Value::Integer(2));
        assert!(iadd.var_args.is_empty());

        let call = &func.instructions[1];
        assert_eq!(call.op, OpCode::CallMethod);
        assert_eq!(call.args.len(), 3);
        assert_eq!(call.args[0].as_identifier(), Some("Bar"));
        assert_eq!(call.var_args, vec![Value::Bool(true)]);
    }

    /// #1728 / SCR-D1-02 — build a one-object Skyrim `.pex`: big-endian
    /// magic + all multi-byte fields BE, and the original-Skyrim object
    /// layout (no const-flag byte, no struct infos, no guards — those are
    /// FO4+/Starfield additions gated on `script_type.is_skyrim()` /
    /// `== ScriptType::Starfield` in `reader.rs::read_objects`).
    fn build_sample_skyrim_be() -> Vec<u8> {
        let mut w = PexWriter::new_be();
        for s in ["Foo", "Actor", "MyProp", "Int", "OnActivate", "None"] {
            w.intern(s);
        }

        // ── header (magic written LE even in the BE dialect) ──
        w.magic(0xDEC0_57FA);
        w.u8(3); // major
        w.u8(2); // minor
        w.u16(0); // game_id (irrelevant for Skyrim — endian alone selects it)
        w.i64(1_700_000_000); // compilation time
        w.string("Foo.psc");
        w.string("user");
        w.string("computer");

        // ── string table ──
        let table = w.strings.clone();
        w.u16(table.len() as u16);
        for s in &table {
            w.string(s);
        }

        w.u8(0); // debug info: absent
        w.u16(0); // user flags: none

        // ── objects: 1 ──
        w.u16(1);
        w.sidx("Foo"); // name
        w.u32(0); // size (ignored)
        w.sidx("Actor"); // parent
        w.sidx("None"); // doc string
        // NO const_flag byte on Skyrim.
        w.u32(0); // user flags
        w.sidx("None"); // auto-state name
        // NO struct_infos count on Skyrim.
        // variables: 1, NO const_flag byte per variable on Skyrim.
        w.u16(1);
        w.sidx("MyProp");
        w.sidx("Int");
        w.u32(0); // user flags
        w.u8(3); // Value tag = Integer
        w.u32(7i32 as u32); // default value = 7
        // NO guards on Skyrim.
        // properties: 0
        w.u16(0);
        // states: 1 (default state) with one no-op function
        w.u16(1);
        w.sidx("None"); // state name
        w.u16(1); // function count
        w.sidx("OnActivate");
        w.sidx("None"); // return type
        w.sidx("None"); // doc
        w.u32(0); // user flags
        w.u8(0); // flags
        w.u16(0); // params: 0
        w.u16(0); // locals: 0
        w.u16(0); // instructions: 0

        w.buf
    }

    #[test]
    fn parses_a_handbuilt_skyrim_be_pex() {
        let bytes = build_sample_skyrim_be();
        let pex = parse(&bytes).expect("BE sample .pex parses");

        assert_eq!(pex.script_type, ScriptType::Skyrim);
        assert_eq!(pex.header.major_version, 3);
        assert_eq!(pex.header.source_file_name, "Foo.psc");
        assert!(!pex.debug_info.present);

        let obj = pex.main_object().expect("one object");
        assert_eq!(obj.name, "Foo");
        assert_eq!(obj.parent_class_name, "Actor");
        assert_eq!(obj.const_flag, 0, "Skyrim has no const-flag field, reader defaults to 0");
        assert!(obj.struct_infos.is_empty(), "Skyrim has no struct infos");
        assert!(obj.guards.is_empty(), "Skyrim has no guards");

        assert_eq!(obj.variables.len(), 1);
        let var = &obj.variables[0];
        assert_eq!(var.name, "MyProp");
        assert_eq!(var.default_value, Value::Integer(7));
        assert_eq!(var.const_flag, 0, "Skyrim variables have no const-flag field");

        let func = &obj.states[0].functions[0];
        assert_eq!(func.name, "OnActivate");
        assert!(func.instructions.is_empty());
    }

    /// #1728 / SCR-D1-02 — build a one-object Starfield `.pex`: LE magic +
    /// `game_id == 4`, exercising the fields Starfield adds on top of the
    /// FO4 layout: per-object `const_flag`, `struct_infos`, per-variable
    /// `const_flag`, and the Starfield-only `guards` list.
    fn build_sample_starfield_with_guards() -> Vec<u8> {
        let mut w = PexWriter::new();
        for s in ["Foo", "ScriptObject", "MyProp", "Int", "OnInit", "None", "SomeGuard"] {
            w.intern(s);
        }

        w.magic(0xFA57_C0DE);
        w.u8(3); // major
        w.u8(2); // minor
        w.u16(4); // game_id (Starfield)
        w.i64(1_700_000_000);
        w.string("Foo.psc");
        w.string("user");
        w.string("computer");

        let table = w.strings.clone();
        w.u16(table.len() as u16);
        for s in &table {
            w.string(s);
        }

        w.u8(0); // debug info: absent
        w.u16(0); // user flags: none

        // ── objects: 1 ──
        w.u16(1);
        w.sidx("Foo");
        w.u32(0); // size
        w.sidx("ScriptObject");
        w.sidx("None");
        w.u8(1); // const_flag (FO4+/Starfield field)
        w.u32(0); // user flags
        w.sidx("None"); // auto-state name
        w.u16(0); // struct_infos: 0
        // variables: 1, WITH const_flag byte per variable.
        w.u16(1);
        w.sidx("MyProp");
        w.sidx("Int");
        w.u32(0); // user flags
        w.u8(3); // Value tag = Integer
        w.u32(9i32 as u32); // default value = 9
        w.u8(1); // const_flag
        // guards: 1 (Starfield-only)
        w.u16(1);
        w.sidx("SomeGuard");
        // properties: 0
        w.u16(0);
        // states: 1 with one no-op function
        w.u16(1);
        w.sidx("None");
        w.u16(1);
        w.sidx("OnInit");
        w.sidx("None");
        w.sidx("None");
        w.u32(0);
        w.u8(0);
        w.u16(0); // params: 0
        w.u16(0); // locals: 0
        w.u16(0); // instructions: 0

        w.buf
    }

    #[test]
    fn parses_a_handbuilt_starfield_pex_with_guards() {
        let bytes = build_sample_starfield_with_guards();
        let pex = parse(&bytes).expect("Starfield sample .pex parses");

        assert_eq!(pex.script_type, ScriptType::Starfield);

        let obj = pex.main_object().expect("one object");
        assert_eq!(obj.name, "Foo");
        assert_eq!(obj.const_flag, 1, "Starfield reads the const_flag byte");
        assert!(obj.struct_infos.is_empty(), "0 struct infos, but the count field was read");

        assert_eq!(obj.variables.len(), 1);
        let var = &obj.variables[0];
        assert_eq!(var.default_value, Value::Integer(9));
        assert_eq!(var.const_flag, 1, "Starfield variables carry a const_flag byte");

        assert_eq!(obj.guards.len(), 1, "Starfield-only guards list");
        assert_eq!(obj.guards[0].name, "SomeGuard");

        let func = &obj.states[0].functions[0];
        assert_eq!(func.name, "OnInit");
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = [0u8; 16];
        assert!(matches!(parse(&bytes), Err(PexError::BadMagic { .. })));
    }

    #[test]
    fn rejects_truncation() {
        // Valid magic, then EOF mid-header.
        let mut bytes = 0xFA57_C0DEu32.to_le_bytes().to_vec();
        bytes.push(3); // major only, then nothing
        assert!(matches!(parse(&bytes), Err(PexError::UnexpectedEof { .. })));
    }
}
