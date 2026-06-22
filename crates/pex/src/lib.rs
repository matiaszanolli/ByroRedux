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
    /// end-to-end. Little-endian (FO4 dialect). Mirrors the on-disk layout
    /// field-for-field so the round-trip pins the reader's read order.
    struct PexWriter {
        buf: Vec<u8>,
        strings: Vec<String>,
    }

    impl PexWriter {
        fn new() -> Self {
            Self { buf: Vec::new(), strings: Vec::new() }
        }
        fn u8(&mut self, v: u8) { self.buf.push(v); }
        fn u16(&mut self, v: u16) { self.buf.extend_from_slice(&v.to_le_bytes()); }
        fn u32(&mut self, v: u32) { self.buf.extend_from_slice(&v.to_le_bytes()); }
        fn i64(&mut self, v: i64) { self.buf.extend_from_slice(&v.to_le_bytes()); }
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
        w.u32(0xFA57_C0DE);
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
