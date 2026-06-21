//! `SCPT` (Script) record parser — FO3 / FNV pre-Papyrus bytecode.
//!
//! Oblivion, FO3, and FNV predate Papyrus and ship scripts as a binary
//! layout:
//!   - `SCHR` — 20-byte header: `numRefs u32`, `compiled_size u32`,
//!     `var_count u32`, `script_type u16`, `flags u16`.
//!   - `SCDA` — compiled bytecode blob (opaque here).
//!   - `SCTX` — original source text (zstring, optional).
//!   - `SLSD` / `SCVR` — local variable metadata (one pair per local var).
//!   - `SCRV` / `SCRO` — resolved cross-record references for the runtime
//!     stack (u32 FormID per entry).
//!
//! Extraction only. Bytecode runtime is out of scope — the ECS-native
//! scripting model is tracked separately (M30). What we need here is
//! structured storage so every NPC / item `SCRI` cross-reference
//! actually resolves to a record instead of dangling. See #443.
//!
//! nif.xml equivalent: the Oblivion / FO3 `Script` record schema under
//! UESP. Skyrim+ uses `VMAD` instead (Papyrus attached data) — different
//! layout, tracked via `CommonItemFields.has_script`.

use super::common::read_zstring;
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// Script type byte (from `SCHR.script_type`). Values come from the
/// Oblivion / FO3 script compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptType {
    /// Object script — runs attached to an item / REFR (most common).
    #[default]
    Object,
    /// Quest script — runs as part of a quest's state machine.
    Quest,
    /// Magic effect script — FO3/FNV spell / perk effect callback.
    MagicEffect,
    /// Unknown / future variant. Preserved so a later audit can still
    /// dispatch without the parser silently erasing the value.
    Unknown(u16),
}

impl ScriptType {
    fn from_u16(v: u16) -> Self {
        match v {
            0x0000 => Self::Object,
            0x0001 => Self::Quest,
            0x0100 => Self::MagicEffect,
            other => Self::Unknown(other),
        }
    }
}

/// Per-script local variable metadata — one per `SCVR` sub-record.
#[derive(Debug, Clone)]
pub struct ScriptLocalVar {
    /// Index within the script's local table (from `SLSD.index`).
    pub index: u32,
    /// Type — 0 = f32/ref, 1 = short, 2 = long. Raw passthrough.
    pub var_type: u8,
    /// Name as authored in the script source (`SCVR` zstring).
    pub name: String,
}

/// Parsed SCPT record.
///
/// Raw bytecode is retained verbatim because the ECS-native runtime
/// lands later; the parse just needs structural integrity so
/// `SCRI` / quest / dialogue / terminal references stop dangling.
#[derive(Debug, Clone, Default)]
pub struct ScriptRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Declared number of `SCRV`/`SCRO` cross-record references the
    /// bytecode expects on its runtime stack. Cross-reference with
    /// `ref_form_ids.len()` to detect a truncated `SCRV`/`SCRO` tail.
    pub num_refs: u32,
    /// Size of the compiled bytecode blob (`SCDA`) in bytes.
    pub compiled_size: u32,
    /// Declared number of local variables. `locals.len()` mirrors
    /// this when the parse sees every `SLSD`+`SCVR` pair.
    pub var_count: u32,
    pub script_type: ScriptType,
    /// `SCHR` trailing flags — a `u16` on every game. Oblivion through
    /// FNV ship a 20-byte SCHR whose final 2 bytes are this field
    /// (verified via openmw `esm4/script.hpp` + a raw byte-scan: every
    /// vanilla SCHR is exactly 20 bytes). Only the `is_hidden` (bit 1)
    /// bit is documented; the rest are reserved. Parsed-but-unused
    /// today — no downstream consumer (#1654).
    pub flags: u16,
    /// `SCDA` compiled bytecode — opaque.
    pub compiled: Vec<u8>,
    /// `SCTX` original source text (may be absent on compile-only
    /// packaged scripts).
    pub source: Option<String>,
    /// Local-var metadata from `SLSD` + `SCVR` pairs, in source order.
    pub locals: Vec<ScriptLocalVar>,
    /// Cross-record FormIDs the script references (`SCRV` numeric vars
    /// + `SCRO` object refs). Each entry is one u32 FormID.
    pub ref_form_ids: Vec<u32>,
}

/// Parse a SCPT record from its sub-records.
///
/// Tolerant of missing / out-of-order sub-records: any subset parses.
/// Unknown sub-types are ignored, matching the rest of this module's
/// policy. The declared counts on `SCHR` (`num_refs`, `var_count`) are
/// preserved as-is so downstream consumers can diff against
/// `ref_form_ids.len()` / `locals.len()` to detect a truncated file.
pub fn parse_scpt(form_id: u32, subs: &[SubRecord]) -> ScriptRecord {
    let mut record = ScriptRecord {
        form_id,
        ..ScriptRecord::default()
    };
    // Buffer the pending SLSD info — SCVR always follows SLSD on disk
    // (per the Oblivion / FO3 layout) and carries the name string
    // without re-encoding the index, so we carry the index forward.
    let mut pending_local: Option<(u32, u8)> = None;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            // SCHR: pad u32 + numRefs u32 + compiled_size u32 +
            //       var_count u32 + script_type u16 + flags u16
            //       → 20 bytes total.
            // Minimum 16 bytes for the leading pad + three u32s; the
            // script_type / flags u16 pair is read only when present.
            b"SCHR" if sub.data.len() >= 16 => {
                let mut r = SubReader::new(&sub.data);
                r.skip_or_eof(4); // Leading u32 is a legacy padding slot.
                record.num_refs = r.u32_or_default();
                record.compiled_size = r.u32_or_default();
                record.var_count = r.u32_or_default();
                if sub.data.len() >= 18 {
                    let ty = r.u16().unwrap_or(0);
                    record.script_type = ScriptType::from_u16(ty);
                }
                // flags is a `u16` on every game (Oblivion through FNV):
                // the vanilla SCHR is exactly 20 bytes, leaving 2 bytes
                // after the script_type u16. Reading it as `u32` here
                // fails-silently on real data and pinned flags to 0 for
                // every script — the #1654 bug. We don't decode specific
                // bits yet, just preserve the value.
                if sub.data.len() >= 20 {
                    record.flags = r.u16_or_default();
                }
            }
            b"SCDA" => {
                record.compiled = sub.data.clone();
            }
            b"SCTX" => {
                record.source = Some(read_zstring(&sub.data));
            }
            // SLSD: local var metadata — index u32 + unknown u32 +
            //       var_type u8 + 3 bytes padding + 4 bytes padding.
            // The name arrives in the paired SCVR that immediately
            // follows.
            b"SLSD" if sub.data.len() >= 9 => {
                let mut r = SubReader::new(&sub.data);
                let index = r.u32_or_default();
                r.skip_or_eof(4); // unknown u32 at offset 4..8
                let var_type = r.u8_or_default();
                pending_local = Some((index, var_type));
            }
            b"SCVR" => {
                let name = read_zstring(&sub.data);
                if let Some((index, var_type)) = pending_local.take() {
                    record.locals.push(ScriptLocalVar {
                        index,
                        var_type,
                        name,
                    });
                } else {
                    // SCVR without a preceding SLSD — log once per
                    // record so a malformed file surfaces without
                    // breaking the parse.
                    log::debug!(
                        "SCPT {form_id:08X}: orphan SCVR '{name}' without \
                         preceding SLSD — skipping var"
                    );
                }
            }
            // SCRV: numeric cross-record refs (local var referencing a
            // script-owned variable). u32 FormID per entry.
            b"SCRV" if sub.data.len() >= 4 => {
                if let Ok(fid) = SubReader::new(&sub.data).u32() {
                    record.ref_form_ids.push(fid);
                }
            }
            // SCRO: object cross-record refs (bytecode literal). u32
            // FormID per entry.
            b"SCRO" if sub.data.len() >= 4 => {
                if let Ok(fid) = SubReader::new(&sub.data).u32() {
                    record.ref_form_ids.push(fid);
                }
            }
            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(st: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *st,
            data,
        }
    }

    #[test]
    fn parse_scpt_extracts_schr_scda_sctx_and_vars() {
        // Vanilla 20-byte SCHR: 4-byte pad + 3×u32 + script_type u16 +
        // flags u16. This is the exact on-disk layout every FO3/FNV
        // script ships (#1654); a wider fixture would let the flags read
        // succeed as a u32 and mask the u16/u32 regression.
        let mut schr = Vec::new();
        schr.extend_from_slice(&0u32.to_le_bytes()); // unused pad
        schr.extend_from_slice(&3u32.to_le_bytes()); // num_refs
        schr.extend_from_slice(&128u32.to_le_bytes()); // compiled_size
        schr.extend_from_slice(&2u32.to_le_bytes()); // var_count
        schr.extend_from_slice(&1u16.to_le_bytes()); // script_type = Quest
        schr.extend_from_slice(&0x0002u16.to_le_bytes()); // flags (u16)
        let subs = vec![
            sub(b"EDID", b"MegatonDoorScript\0".to_vec()),
            sub(b"SCHR", schr),
            sub(b"SCDA", vec![0xDEu8, 0xAD, 0xBE, 0xEF, 0x12, 0x34]),
            sub(
                b"SCTX",
                b"scn MegatonDoorScript\n\nBegin OnActivate\nEnd\0".to_vec(),
            ),
            // Local var 0 (type = 2, long).
            {
                let mut d = Vec::new();
                d.extend_from_slice(&0u32.to_le_bytes()); // index
                d.extend_from_slice(&0u32.to_le_bytes()); // unknown pad
                d.push(2u8); // var_type = long
                             // Trailing 4 bytes of SLSD padding tolerated — some files
                             // write 13 bytes total, some 9. Our parser reads only the
                             // 9-byte prefix, so leave the tail short.
                sub(b"SLSD", d)
            },
            sub(b"SCVR", b"iDoorOpen\0".to_vec()),
            // Local var 1 (type = 1, short).
            {
                let mut d = Vec::new();
                d.extend_from_slice(&1u32.to_le_bytes());
                d.extend_from_slice(&0u32.to_le_bytes());
                d.push(1u8);
                sub(b"SLSD", d)
            },
            sub(b"SCVR", b"sDoorState\0".to_vec()),
            // Cross-record refs.
            sub(b"SCRO", 0xCAFEBABEu32.to_le_bytes().to_vec()),
            sub(b"SCRV", 0x1000_0001u32.to_le_bytes().to_vec()),
            sub(b"SCRO", 0x1000_0002u32.to_le_bytes().to_vec()),
        ];

        let rec = parse_scpt(0xBEEF_1234, &subs);
        assert_eq!(rec.form_id, 0xBEEF_1234);
        assert_eq!(rec.editor_id, "MegatonDoorScript");
        assert_eq!(rec.num_refs, 3);
        assert_eq!(rec.compiled_size, 128);
        assert_eq!(rec.var_count, 2);
        assert_eq!(rec.script_type, ScriptType::Quest);
        // Regression guard for #1654: a vanilla 20-byte SCHR has only a
        // u16 flags tail, so the value must survive the parse intact
        // (the old strict-u32 read pinned it to 0 on every real script).
        assert_eq!(rec.flags, 0x0002);
        assert_eq!(rec.compiled, vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34]);
        assert!(rec.source.as_deref().unwrap().starts_with("scn "));
        assert_eq!(rec.locals.len(), 2);
        assert_eq!(rec.locals[0].name, "iDoorOpen");
        assert_eq!(rec.locals[0].var_type, 2);
        assert_eq!(rec.locals[1].name, "sDoorState");
        assert_eq!(rec.ref_form_ids.len(), 3);
        assert!(rec.ref_form_ids.contains(&0xCAFEBABE));
        assert!(rec.ref_form_ids.contains(&0x1000_0001));
        assert!(rec.ref_form_ids.contains(&0x1000_0002));
    }

    /// Missing `SCDA` / `SCTX` / `SCVR` sub-records must not crash the
    /// parse — some vanilla FO3 scripts ship bytecode-only.
    #[test]
    fn parse_scpt_tolerates_missing_subrecords() {
        let mut schr = Vec::new();
        schr.extend_from_slice(&0u32.to_le_bytes()); // pad
        schr.extend_from_slice(&0u32.to_le_bytes()); // num_refs = 0
        schr.extend_from_slice(&0u32.to_le_bytes()); // compiled_size = 0
        schr.extend_from_slice(&0u32.to_le_bytes()); // var_count = 0
        schr.extend_from_slice(&0u16.to_le_bytes()); // script_type = Object
        schr.extend_from_slice(&0u16.to_le_bytes()); // flags (u16) = 0
        let subs = vec![sub(b"EDID", b"TinyScript\0".to_vec()), sub(b"SCHR", schr)];
        let rec = parse_scpt(0x0CAFEu32, &subs);
        assert_eq!(rec.editor_id, "TinyScript");
        assert_eq!(rec.script_type, ScriptType::Object);
        assert!(rec.compiled.is_empty());
        assert!(rec.source.is_none());
        assert!(rec.locals.is_empty());
        assert!(rec.ref_form_ids.is_empty());
    }

    /// Unknown script_type must land in `ScriptType::Unknown(raw)` so
    /// a future record class isn't silently erased.
    #[test]
    fn script_type_unknown_preserves_raw() {
        assert_eq!(ScriptType::from_u16(0x0042), ScriptType::Unknown(0x0042));
        assert_eq!(ScriptType::from_u16(0x0000), ScriptType::Object);
        assert_eq!(ScriptType::from_u16(0x0001), ScriptType::Quest);
        assert_eq!(ScriptType::from_u16(0x0100), ScriptType::MagicEffect);
    }
}
