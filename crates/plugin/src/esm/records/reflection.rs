//! The BGS **reflection** container decoder (#5364, #5627) — the schema + value
//! grammar behind Starfield's `WTHS` weather settings (`REFL` / `RDIF`) and the
//! `LMSW` / `MTPT` material records, which share it.
//!
//! The blob's grammar is the BGS **reflection** container, recovered
//! from xEdit's `wbDefinitionsSF1.pas` (`wbReflectionChunk`, the
//! reflection-block draft — still commented out upstream at
//! dev-4.1.5) and validated byte-for-byte against `Starfield.esm` with
//! the `dump_wths_refl` example:
//!
//! ```text
//! container  := chunk*
//! chunk      := sig: char[4], size: u32, payload: byte[size]
//! BETH       := version: u32, chunk_count: u32          // first chunk
//! STRT       := (nul-terminated string)*                 // string table;
//!                                                        // name refs are
//!                                                        // i32 byte offsets
//! TYPE       := class_count: u32
//! CLAS       := name: i32, type: i32, flags: u16, field_count: u16,
//!               field_count × { name: i32, type: i32, offset: u16, size: u16 }
//! DIFF       := type: u32, (field_index: u16, value)*, 0xFFFF   // RDIF only
//! OBJT/LIST/USER/USRD/MAPC                           // the instance values
//! ```
//!
//! A `DIFF` is an object diff, not a flat index list (#5514): the type
//! ref names the class (through its string-table name offset), then
//! each overridden field follows as `(u16 index, inline value)` until
//! the `0xFFFF` terminator. Inline value sizes follow the CDB reader's
//! skip rules (`crates/sfmaterial/src/reader.rs`): built-ins by size,
//! `String` by its `u16` length prefix, `Ref` by recursing its target,
//! and a non-user class (struct) as a nested diff-pair stream — diff
//! semantics propagate into overridden structs. Chunk-typed fields
//! (List/Map built-ins, user-flagged classes) carry no inline bytes:
//! their values ride the side chunks after the terminator, which the
//! container census keeps counting as instance chunks.
//!
//! `name`/`type` refs are i32 string-table offsets, or the *negative*
//! built-in type ids (`0xFFFFFF02` String, `0xFFFFFF0D` UInt32,
//! `0xFFFFFF11` Float, … — the full table in
//! [`builtin_type_label`]). CLAS flags: bit 2 `User`, bit 3 `Struct`.
//!
//! **Scope of this decode.** The container walk, the schema half —
//! string table, class list, every field's name/type/offset/size — and,
//! since #5627, the instance half: the root `OBJT` decodes into a value
//! tree ([`BgsReflection::root_instance`]), the `DIFF` chunk decodes to
//! named+valued overrides ([`BgsReflection::diff_values`]), and
//! `LIST`/`MAPC`/`USER`/`USRD` side chunks resolve through the same
//! walk. The instance grammar lives in
//! [`super::weather_settings_instance`] (grounded from the vanilla
//! corpus; xEdit has no shipped reader for these chunks either). A
//! record whose instance walk disagrees with its schema keeps the chunk
//! census as evidence and leaves the value fields `None` — the
//! pre-#5627 state — rather than guessing.

use super::common::read_zstring;
use crate::esm::reader::FormIdRemap;
use crate::esm::sub_reader::SubReader;

/// Built-in reflection type ids as the raw i32 stored on disk
/// (`0xFFFFFF02` = -254 = String, `0xFFFFFF0D` = -243 = UInt32,
/// `0xFFFFFF11` = -239 = Float, …). The same values
/// `byroredux-sfmaterial`'s `BuiltinType` names as `u32`; this crate does
/// not depend on that one, so the ids live here once and
/// [`BUILTINS`] is the only table that maps them. The instance-value
/// reader (#5627) shares the table.
pub(super) mod builtin {
    pub const NULL: i32 = -255;
    pub const STRING: i32 = -254;
    pub const LIST: i32 = -253;
    pub const MAP: i32 = -252;
    pub const REF: i32 = -251;
    pub const INT8: i32 = -248;
    pub const UINT8: i32 = -247;
    pub const INT16: i32 = -246;
    pub const UINT16: i32 = -245;
    pub const INT32: i32 = -244;
    pub const UINT32: i32 = -243;
    pub const INT64: i32 = -242;
    pub const UINT64: i32 = -241;
    pub const BOOL: i32 = -240;
    pub const FLOAT: i32 = -239;
    pub const DOUBLE: i32 = -238;
}

/// CLAS flag bit 2 `User`: the class's values ride side chunks, nothing
/// inline in a diff stream (the CDB reader's `ClassFlags::IS_USER`).
pub(super) const CLASS_FLAG_USER: u16 = 1 << 2;

/// #5687 — ceiling on struct / `Ref` nesting while skipping a diff
/// value. `skip_diff_value` recurses on file-controlled data (a struct
/// field may name its own class; a `Ref` target may be `Ref` again), each
/// level costing only 2–4 payload bytes, so a hostile `RDIF` blob recursed
/// ~payload/2 deep and aborted the process with a stack overflow. Mirrors
/// `crates/sfmaterial/src/reader.rs`'s `MAX_NESTING_DEPTH` (duplicated
/// because this crate does not depend on that one, #5615). Vanilla
/// `RDIF`s are shallow — 163/163 decode — so 64 is ample. The
/// instance-value reader (#5627) shares the cap.
pub(super) const MAX_NESTING_DEPTH: usize = 64;

/// How a built-in's value is laid out inline in a diff stream.
#[derive(Clone, Copy)]
pub(super) enum InlineValue {
    /// Nothing inline: `Null`, and `List` / `Map` (chunk-typed side chunks).
    Nothing,
    /// A fixed number of bytes.
    Fixed(usize),
    /// `u16` length prefix, then that many bytes.
    LengthPrefixedString,
    /// `i32` target type, then the target's own value (a List/Map target is
    /// the null sentinel — nothing inline).
    Ref,
}

/// Every built-in: raw id, label, inline layout. One table so the label
/// and the value-skip walk cannot disagree about which ids exist.
pub(super) const BUILTINS: [(i32, &str, InlineValue); 16] = [
    (builtin::NULL, "Null", InlineValue::Nothing),
    (builtin::STRING, "String", InlineValue::LengthPrefixedString),
    (builtin::LIST, "List", InlineValue::Nothing),
    (builtin::MAP, "Map", InlineValue::Nothing),
    (builtin::REF, "Ref", InlineValue::Ref),
    (builtin::INT8, "Int8", InlineValue::Fixed(1)),
    (builtin::UINT8, "UInt8", InlineValue::Fixed(1)),
    (builtin::INT16, "Int16", InlineValue::Fixed(2)),
    (builtin::UINT16, "UInt16", InlineValue::Fixed(2)),
    (builtin::INT32, "Int32", InlineValue::Fixed(4)),
    (builtin::UINT32, "UInt32", InlineValue::Fixed(4)),
    (builtin::INT64, "Int64", InlineValue::Fixed(8)),
    (builtin::UINT64, "UInt64", InlineValue::Fixed(8)),
    (builtin::BOOL, "Bool", InlineValue::Fixed(1)),
    (builtin::FLOAT, "Float", InlineValue::Fixed(4)),
    (builtin::DOUBLE, "Double", InlineValue::Fixed(8)),
];

pub(super) fn builtin_entry(raw: i32) -> Option<&'static (i32, &'static str, InlineValue)> {
    BUILTINS.iter().find(|(id, _, _)| *id == raw)
}

pub fn builtin_type_label(raw: i32) -> Option<&'static str> {
    builtin_entry(raw).map(|(_, label, _)| *label)
}

/// One field of a reflection class: name and type resolved through the
/// string table (or the built-in label), plus the authored offset/size
/// into the class's instance layout. `type_ref` keeps the raw i32 —
/// negative = built-in id, non-negative = the referenced class's
/// string-table name offset — because inline value sizes (a DIFF walk,
/// the future values reader) derive from the raw ref, not the label.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReflectionField {
    pub name: String,
    pub type_name: String,
    pub type_ref: i32,
    pub offset: u16,
    pub size: u16,
}

/// One CLAS chunk: a named class with its field list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReflectionClass {
    /// The class's name string-table offset — the key a `DIFF` type ref
    /// resolves through (Gibbed's `typeMap` is indexed by `NameOffset`).
    pub name_offset: i32,
    pub name: String,
    /// Bit 2 `User`, bit 3 `Struct` (xEdit's flag table; vanilla roots
    /// mark `XMFLOAT4`-style value types as Struct).
    pub flags: u16,
    pub fields: Vec<ReflectionField>,
}

/// A decoded BGS reflection container (the `REFL`/`RDIF` payload).
#[derive(Debug, Clone, Default)]
pub struct BgsReflection {
    /// From the leading `BETH` chunk.
    pub version: u32,
    /// Chunk count declared by `BETH` (sanity: the walk's chunk tally
    /// must reach it — a mismatch means an unknown chunk kind).
    pub chunk_count: u32,
    /// The `STRT` string table, in authored order.
    pub strings: Vec<String>,
    /// `TYPE`'s class count (must equal `classes.len()`).
    pub class_count: u32,
    /// Every `CLAS` chunk, in authored order.
    pub classes: Vec<ReflectionClass>,
    /// `DIFF` field overrides (RDIF blobs only), decoded with VALUES
    /// (#5627): each pair carries the overridden field's index, its
    /// resolved name, and its value — inline values read by field type,
    /// chunk-typed fields (List/Map/user classes) read from the side
    /// chunks. `None` when the blob carries no DIFF chunk, or the diff
    /// walk could not resolve the class / ran past the chunk (vanilla
    /// never trips either).
    pub diff_values: Option<Vec<super::weather_settings_instance::DiffOverride>>,
    /// The root instance (the first `OBJT` chunk) decoded into a value
    /// tree (#5627): every field of the root class bound to its
    /// authored value, chunk-typed fields resolved from the side chunks.
    /// `None` when the blob carries no `OBJT` (RDIF children), or the
    /// instance walk disagreed with the schema — a truncation, a
    /// trailing byte, a prefix that isn't the field's type — in which
    /// case the census below is the record's evidence. Vanilla decodes
    /// clean on all 18 templates.
    pub root_instance: Option<super::weather_settings_instance::ReflectionObject>,
    /// Side chunks the instance walk did not consume (0 on a clean
    /// decode; any other count means the grammar drifted from the data).
    pub unconsumed_instance_chunks: usize,
    /// The instance chunks (`OBJT`/`LIST`/`USER`/`USRD`/`MAPC`),
    /// censused as `(signature, payload size)`.
    pub instance_chunks: Vec<([u8; 4], usize)>,
    /// Signatures of chunks the walker did not recognize (skipped by
    /// size). Empty on vanilla data.
    pub unknown_chunks: Vec<[u8; 4]>,
}

impl BgsReflection {
    /// The class named exactly `BGSWeatherSettingsForm` — the root of
    /// every weather-settings blob (its field list is the weather's
    /// parameter schema: fog, colors, wind, precipitation, …).
    pub fn weather_root_class(&self) -> Option<&ReflectionClass> {
        self.classes.iter().find(|c| c.name == "BGSWeatherSettingsForm")
    }
}

/// Resolve a name/type reference: negative = built-in type id,
/// non-negative = byte offset into the decoded string table.
pub(super) fn resolve_ref(raw: i32, strings: &[(usize, String)]) -> String {
    if raw < 0 {
        return builtin_type_label(raw)
            .map(str::to_string)
            .unwrap_or_else(|| format!("<builtin {raw:#010x}>"));
    }
    let off = raw as usize;
    // The table is sorted by construction (offsets ascend with the
    // string list); binary search the last entry whose offset <= off.
    match strings.binary_search_by(|(o, _)| o.cmp(&off)) {
        Ok(i) => strings[i].1.clone(),
        Err(0) => format!("<offset {off}>"),
        Err(i) => strings[i - 1].1.clone(),
    }
}

/// Walk the reflection container in `blob` (the REFL/RDIF payload).
/// Tolerant: a chunk whose declared size overflows the blob, or a CLAS
/// whose field walk runs past its chunk, ends the walk with everything
/// decoded so far (vanilla data never trips either).
pub fn parse_bgs_reflection(blob: &[u8]) -> BgsReflection {
    parse_bgs_reflection_with_remap(blob, &None)
}

/// [`parse_bgs_reflection`], with the record's load-order remap: FormID
/// `Ref` leaves in the decoded instance values are remapped into the
/// record's global space at decode time (`parse_wths` threads it
/// through, like the RFDP parent).
pub fn parse_bgs_reflection_with_remap(
    blob: &[u8],
    remap: &Option<FormIdRemap>,
) -> BgsReflection {
    use super::weather_settings_instance::decode_instances;

    let mut out = BgsReflection::default();
    // (byte offset, string) pairs — the i32 refs index this by offset.
    let mut table: Vec<(usize, String)> = Vec::new();
    // The instance half: side-chunk payloads in authored order, plus
    // the DIFF chunk (payload + its class's name offset, resolved at
    // walk time) — both decoded after the schema walk completes,
    // because the value reader consumes the side chunks a DIFF names
    // that appear AFTER it in the stream.
    let mut instance_payloads: Vec<([u8; 4], &[u8])> = Vec::new();
    let mut diff_chunk: Option<(&[u8], i32)> = None;
    let mut off = 0usize;
    while off + 8 <= blob.len() {
        // The loop guard leaves at least the 8 header bytes.
        let mut header = SubReader::new(&blob[off..off + 8]);
        let sig: [u8; 4] = header.fixed::<4>().unwrap_or_default();
        let size = header.u32_or_default() as usize;
        if off + 8 + size > blob.len() {
            // Overflowing chunk — record nothing further; the tally
            // mismatch against `chunk_count` is the diagnostic.
            break;
        }
        let payload = &blob[off + 8..off + 8 + size];
        match &sig {
            b"BETH" if payload.len() >= 8 => {
                let mut r = SubReader::new(payload);
                out.version = r.u32_or_default();
                out.chunk_count = r.u32_or_default();
            }
            b"STRT" => {
                let mut p = 0usize;
                while p < payload.len() {
                    let end = payload[p..]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|e| p + e)
                        .unwrap_or(payload.len());
                    table.push((p, read_zstring(&payload[p..end])));
                    p = end + 1;
                }
            }
            b"TYPE" if payload.len() >= 4 => {
                out.class_count = SubReader::new(payload).u32_or_default();
            }
            b"CLAS" if payload.len() >= 12 => {
                let mut r = SubReader::new(payload);
                let name = r.i32_or_default();
                r.skip_or_eof(4); // two u16s never decoded (class kind / size)
                let flags = r.u16_or_default();
                let field_count = usize::from(r.u16_or_default());
                let mut class = ReflectionClass {
                    name_offset: name,
                    name: resolve_ref(name, &table),
                    flags,
                    ..ReflectionClass::default()
                };
                for _ in 0..field_count {
                    let Ok(field) = r.fixed::<12>() else { break };
                    let mut f = SubReader::new(&field);
                    let name_ref = f.i32_or_default();
                    let type_ref = f.i32_or_default();
                    class.fields.push(ReflectionField {
                        name: resolve_ref(name_ref, &table),
                        type_ref,
                        type_name: resolve_ref(type_ref, &table),
                        offset: f.u16_or_default(),
                        size: f.u16_or_default(),
                    });
                }
                out.classes.push(class);
            }
            b"DIFF" if payload.len() >= 4 => {
                // #5514 — a DIFF is an object diff: the leading type ref
                // names the class (by string-table name offset), then
                // (u16 field index, value) pairs until 0xFFFF. The
                // class must resolve here or the pair layout is
                // unknowable — the chunk is left undecoded rather than
                // guessed. The VALUE decode (with side chunks) runs
                // after the walk.
                let type_ref = SubReader::new(payload).i32_or_default();
                if out.classes.iter().any(|c| c.name_offset == type_ref) {
                    diff_chunk = Some((payload, type_ref));
                }
            }
            b"OBJT" | b"LIST" | b"USER" | b"USRD" | b"MAPC" => {
                out.instance_chunks.push((sig, size));
                instance_payloads.push((sig, payload));
            }
            _ => {
                out.unknown_chunks.push(sig);
            }
        }
        off += 8 + size;
    }
    out.strings = table.into_iter().map(|(_, s)| s).collect();

    let decoded = decode_instances(&instance_payloads, diff_chunk, &out.classes, remap.as_ref());
    out.root_instance = decoded.root;
    out.diff_values = decoded.diff;
    out.unconsumed_instance_chunks = decoded.unconsumed_chunks;
    out
}
