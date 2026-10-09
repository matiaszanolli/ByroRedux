//! WTHS (Weather Settings) record parser — Starfield's replacement for
//! the WTHR weather type (#5364).
//!
//! A WTHS is an `EDID` + BGS-parameter-blob pair, not the WTHR
//! sub-record schema. The blob rides the `REFL` sub-record on root
//! ("template") records and the `RDIF` (Reflection Diff) sub-record on
//! child records that point up an `RFDP` (Reflection Parent) chain:
//! vanilla `Starfield.esm` ships 181 WTHS — 18 roots with a full REFL
//! (`WeatherClearTemplate`, `DefaultWeatherSettings`, …) and 163
//! children carrying an RFDP + RDIF (e.g. Akila's
//! `WeatherUniqueAkila_Clear_C0_Clear` → `Weather_Clear_C0_Clear` →
//! `WeatherClearTemplate`).
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
//! DIFF       := type: u32, (field_index: u16)*           // RDIF only
//! OBJT/LIST/USER                                    // the instance values
//! ```
//!
//! `name`/`type` refs are i32 string-table offsets, or the *negative*
//! built-in type ids (`0xFFFFFF02` String, `0xFFFFFF0D` UInt32,
//! `0xFFFFFF11` Float, … — the full table in
//! [`builtin_type_label`]). CLAS flags: bit 2 `User`, bit 3 `Struct`.
//!
//! **Scope of this decode.** The container walk plus the schema half —
//! string table, class list, every field's name/type/offset/size — is
//! decoded here. The instance half (`OBJT`/`LIST`/`USER` chunks, the
//! actual parameter values: fog distances, colors, wind) is censused
//! but not interpreted; xEdit has no shipped reader for it either, and
//! grounding those layouts is the tracked follow-up (see the issue).
//! Until a value reader lands, exteriors on WTHS-only climates keep
//! resolving weather through the authored `DefaultWeather` WTHR
//! (#5363's fallback).

use super::common::{read_zstring, remap_fid};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// Built-in reflection type ids (xEdit `wbStringTableLookup`'s negative
/// enum). Values are the raw i32 as stored on disk (`0xFFFFFF02` =
/// -254 = String, `0xFFFFFF0D` = -243 = UInt32, `0xFFFFFF11` = -239 =
/// Float, …).
pub fn builtin_type_label(raw: i32) -> Option<&'static str> {
    Some(match raw {
 -255 => "Null",
 -254 => "String",
 -253 => "List",
 -252 => "Map",
 -251 => "Ref",
 -248 => "Int8",
 -247 => "UInt8",
 -246 => "Int16",
 -245 => "UInt16",
 -244 => "Int32",
 -243 => "UInt32",
 -242 => "Int64",
 -241 => "UInt64",
 -240 => "Bool",
 -239 => "Float",
 -238 => "Double",
        _ => return None,
    })
}

/// One field of a reflection class: name and type resolved through the
/// string table (or the built-in label), plus the authored offset/size
/// into the class's instance layout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReflectionField {
    pub name: String,
    pub type_name: String,
    pub offset: u16,
    pub size: u16,
}

/// One CLAS chunk: a named class with its field list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReflectionClass {
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
    /// `DIFF` field indices (RDIF blobs only): which fields of the
    /// parent's class the child overrides. `None` when no DIFF chunk.
    pub diff_field_indices: Option<Vec<u16>>,
    /// The instance chunks (`OBJT`/`LIST`/`USER`), censused as
    /// `(signature, payload size)` — the value half is the tracked
    /// follow-up, and this census is the evidence base for it.
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

/// Parsed WTHS record.
#[derive(Debug, Clone, Default)]
pub struct WeatherSettingsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// The `REFL` container — full reflection, on root (template)
    /// records. `None` on children, which carry `reflection_diff` +
    /// `reflection_parent` instead.
    pub reflection: Option<BgsReflection>,
    /// The `RDIF` container — same grammar, plus a `DIFF` chunk naming
    /// the overridden fields. `None` on roots.
    pub reflection_diff: Option<BgsReflection>,
    /// `RFDP` — the parent WTHS this record diffs against. Load-order
    /// remapped like every cross-record FormID. `None` on roots.
    pub reflection_parent: Option<u32>,
}

/// Resolve a name/type reference: negative = built-in type id,
/// non-negative = byte offset into the decoded string table.
fn resolve_ref(raw: i32, strings: &[(usize, String)]) -> String {
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
    let mut out = BgsReflection::default();
    // (byte offset, string) pairs — the i32 refs index this by offset.
    let mut table: Vec<(usize, String)> = Vec::new();
    let mut off = 0usize;
    while off + 8 <= blob.len() {
        let sig: [u8; 4] = blob[off..off + 4].try_into().unwrap();
        let size = u32::from_le_bytes(blob[off + 4..off + 8].try_into().unwrap()) as usize;
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
                out.class_count = u32::from_le_bytes(payload[0..4].try_into().unwrap());
            }
            b"CLAS" if payload.len() >= 12 => {
                let name = i32::from_le_bytes(payload[0..4].try_into().unwrap());
                let flags = u16::from_le_bytes(payload[8..10].try_into().unwrap());
                let field_count = u16::from_le_bytes(payload[10..12].try_into().unwrap()) as usize;
                let mut class = ReflectionClass {
                    name: resolve_ref(name, &table),
                    flags,
                    ..ReflectionClass::default()
                };
                let mut p = 12usize;
                for _ in 0..field_count {
                    let Some(field) = payload.get(p..p + 12) else { break };
                    class.fields.push(ReflectionField {
                        name: resolve_ref(i32::from_le_bytes(field[0..4].try_into().unwrap()), &table),
                        type_name: resolve_ref(
                            i32::from_le_bytes(field[4..8].try_into().unwrap()),
                            &table,
                        ),
                        offset: u16::from_le_bytes(field[8..10].try_into().unwrap()),
                        size: u16::from_le_bytes(field[10..12].try_into().unwrap()),
                    });
                    p += 12;
                }
                out.classes.push(class);
            }
            b"DIFF" if payload.len() >= 4 => {
                let indices = payload[4..]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                out.diff_field_indices = Some(indices);
            }
            b"OBJT" | b"LIST" | b"USER" => {
                out.instance_chunks.push((sig, size));
            }
            _ => {
                out.unknown_chunks.push(sig);
            }
        }
        off += 8 + size;
    }
    out.strings = table.into_iter().map(|(_, s)| s).collect();
    out
}

/// Parse a WTHS record from its sub-records. Starfield-only; the label
/// is not routed here for other games (their `WTHR` arm stays the
/// weather path).
pub fn parse_wths(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
) -> WeatherSettingsRecord {
    let _ = GameKind::Starfield; // (kept for signature parity with siblings)
    let mut record = WeatherSettingsRecord {
        form_id,
        ..WeatherSettingsRecord::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            // RFDP — the parent WTHS this record diffs against.
            // Cross-record FormID: load-order remapped (#4066 pattern)
            // so `index.weather_settings.get(parent)` resolves.
            b"RFDP" if sub.data.len() >= 4 => {
                record.reflection_parent = SubReader::new(&sub.data)
                    .u32()
                    .ok()
                    .map(|raw| remap_fid(raw, remap));
            }
            b"REFL" => record.reflection = Some(parse_bgs_reflection(&sub.data)),
            b"RDIF" => record.reflection_diff = Some(parse_bgs_reflection(&sub.data)),
            _ => {}
        }
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    /// Hand-built reflection container exercising every decoded chunk
    /// kind, in authored order: BETH → STRT → TYPE → CLAS → OBJT.
    #[test]
    fn reflection_container_decodes_schema_and_census() {
        let mut blob: Vec<u8> = Vec::new();
        // BETH: version 4, 5 chunks.
        blob.extend_from_slice(b"BETH");
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&5u32.to_le_bytes());
        // STRT: "BGSWeatherSettingsForm\0FogNear\0FloatValue\0"
        let strs = b"BGSWeatherSettingsForm\0FogNear\0FloatValue\0";
        let fog_near_off: i32 = 24; // after the root name + nul
        let float_value_off: i32 = fog_near_off + 8;
        blob.extend_from_slice(b"STRT");
        blob.extend_from_slice(&(strs.len() as u32).to_le_bytes());
        blob.extend_from_slice(strs);
        // TYPE: one class.
        blob.extend_from_slice(b"TYPE");
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&1u32.to_le_bytes());
        // CLAS: root class, one FogNear: Float field at offset 0 size 4.
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes()); // name → strtab 0
        clas.extend_from_slice(&0i32.to_le_bytes()); // type
        clas.extend_from_slice(&0u16.to_le_bytes()); // flags
        clas.extend_from_slice(&1u16.to_le_bytes()); // field count
        clas.extend_from_slice(&fog_near_off.to_le_bytes());
        clas.extend_from_slice(&float_value_off.to_le_bytes());
        clas.extend_from_slice(&0u16.to_le_bytes()); // offset
        clas.extend_from_slice(&4u16.to_le_bytes()); // size
        blob.extend_from_slice(b"CLAS");
        blob.extend_from_slice(&(clas.len() as u32).to_le_bytes());
        blob.extend_from_slice(&clas);
        // OBJT: instance chunk, censused not interpreted.
        blob.extend_from_slice(b"OBJT");
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&[0, 0, 0, 0]);

        let refl = parse_bgs_reflection(&blob);
        assert_eq!(refl.version, 4);
        assert_eq!(refl.chunk_count, 5);
        assert_eq!(refl.strings.len(), 3);
        assert_eq!(refl.class_count, 1);
        assert_eq!(refl.classes.len(), 1);
        let root = refl.weather_root_class().expect("root class resolved");
        assert_eq!(root.name, "BGSWeatherSettingsForm");
        assert_eq!(root.fields.len(), 1);
        assert_eq!(root.fields[0].name, "FogNear");
        assert_eq!(root.fields[0].type_name, "FloatValue");
        assert_eq!((root.fields[0].offset, root.fields[0].size), (0, 4));
        assert_eq!(refl.instance_chunks, vec![(*b"OBJT", 4)]);
        assert!(refl.unknown_chunks.is_empty());
        assert!(refl.diff_field_indices.is_none());
    }

    /// Built-in type refs (negative i32) resolve to their labels; the
    /// String id is `0xFFFFFF02` = -254 as authored, and an unlisted
    /// negative keeps its hex for diagnostics.
    #[test]
    fn builtin_refs_label_and_unlisted_negatives_kept_as_hex() {
        let table = vec![(0usize, "Named".to_string())];
        assert_eq!(resolve_ref(-254, &table), "String");
        assert_eq!(resolve_ref(-243, &table), "UInt32");
        assert_eq!(resolve_ref(-239, &table), "Float");
        assert_eq!(resolve_ref(-240, &table), "Bool");
        assert_eq!(resolve_ref(-100, &table), "<builtin 0xffffff9c>");
        assert_eq!(resolve_ref(0, &table), "Named");
        assert_eq!(resolve_ref(3, &table), "Named", "mid-string offsets fall back to the containing entry");
    }

    /// An overflowing chunk size ends the walk with the decoded prefix
    /// intact — no panic, no silent wraparound.
    #[test]
    fn overflowing_chunk_stops_the_walk() {
        let mut blob: Vec<u8> = Vec::new();
        blob.extend_from_slice(b"BETH");
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&2u32.to_le_bytes());
        blob.extend_from_slice(b"OBJT");
        blob.extend_from_slice(&0xFFFF_0000u32.to_le_bytes()); // overflows
        let refl = parse_bgs_reflection(&blob);
        assert_eq!(refl.version, 4);
        assert!(refl.instance_chunks.is_empty());
    }

    /// The WTHS shell: EDID + REFL root shape and the RFDP remap.
    #[test]
    fn wths_shell_parses_refl_and_remaps_rfdp() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        let self_ref = (1u32 << 24) | 0x000E_8397; // parent template, own plugin
        let subs = vec![
            sub(b"EDID", b"WeatherUniqueProbe\0"),
            sub(b"RFDP", self_ref.to_le_bytes()),
            sub(
                b"RDIF",
                // BETH chunk: size 8, then version 4 / chunk count 1.
                b"BETH\x08\x00\x00\x00\x04\x00\x00\x00\x01\x00\x00\x00",
            ),
        ];
        let rec = parse_wths(0x000A_0001, &subs, &Some(remap));
        assert_eq!(rec.editor_id, "WeatherUniqueProbe");
        assert_eq!(rec.reflection_parent, Some((2u32 << 24) | 0x000E_8397));
        let diff = rec.reflection_diff.expect("RDIF decoded");
        assert_eq!(diff.version, 4);
        assert!(rec.reflection.is_none());
    }
}
