//! MOVS (Movable Static) — single-mesh record with optional sound
//! attachments and destruction data. Visually identical to STAT (one
//! `MODL` pointer); MOVS distinguishes itself by being driven by Havok
//! at runtime — the mesh's `bhk` collision chain produces dynamic
//! rigid-body motion when the cell is alive.
//!
//! Pre-#588 every MOVS record routed through the MODL-only catch-all
//! at [`crate::esm::cell::parse_modl_group`] alongside STAT / FURN /
//! ACTI / etc. That preserved visual placement (REFRs targeting a
//! MOVS form ID still resolved a `StaticObject` with the right model
//! path) but silently dropped every sub-record that distinguishes a
//! MOVS from a STAT — the loop / activate sound IDs, the destruction
//! marker, and the script attachment flag. Audit `FO4-DIM4-02` framed
//! the gap as a "no movable-static physics data captured" symptom.
//!
//! **Sub-record layout** (per FO4 xEdit `wbDefinitionsFO4` and UESP):
//!
//! - `EDID` — editor ID (z-string)
//! - `VMAD` — virtual machine adapter (script attachments) [optional]
//! - `OBND` — object bounds (12 B, 6 × i16) [unused by the parser]
//! - `MODL` — model file path (z-string)
//! - `MODT` / `MODC` / `MODS` / `MODF` — texture hashes / colors /
//!   alternate-texture refs / model flags [unused by the parser]
//! - `DEST` — destruction-data preamble (8 B). When present, the
//!   record carries one or more `DSTD/DSTF/DMDL/DMDT/DMDS` chunks
//!   describing destruction stages. The full destruction tree isn't
//!   captured today — the renderer has no destruction subsystem to
//!   consume it — but `has_destruction` surfaces the bit so the cell
//!   loader can log REFRs that would lose breakable behaviour. See
//!   FO4-DIM4-02 / #588.
//! - `LNAM` — looping sound FormID (4 B; engine drone, generator hum)
//! - `ZNAM` — activate sound FormID (4 B; impact / interact thud)
//! - `KSIZ` / `KWDA` — keyword count + keyword form IDs [optional;
//!   unused by the parser]
//!
//! **Physics data does not live on the record.** MOVS's "movable"
//! semantic is encoded in the *referenced* NIF mesh's
//! `bhkRigidBody` / `bhkConvexShape` chain. Surfacing the mesh path
//! via the existing `StaticObject` registration is enough for the
//! Havok import path to find it once a physics ECS exists — no MOVS
//! record fields are required for that integration. The audit's
//! "physics kind / collision overrides / motion properties" framing
//! conflates MOVS with the (separate) per-REFR `XHLP` / `XAPD`
//! placement overrides which DO live on the placement, not the base.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::read_string_sub;

/// Parsed MOVS record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovableStaticRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — visual mesh path. Identical role to `STAT.MODL`.
    /// Empty when the record omits the sub (rare; mod-authored
    /// header-only stubs).
    pub model_path: String,
    /// `LNAM` — looping sound FormID. `None` when absent.
    pub loop_sound_form_id: Option<u32>,
    /// `ZNAM` — activate sound FormID. `None` when absent.
    pub activate_sound_form_id: Option<u32>,
    /// `DEST` chunk presence. `true` when the record carries a
    /// destruction preamble (and presumably a `DSTD`/`DSTF`/`DMDL`
    /// stage chain). Captured as a flag pending a destruction
    /// subsystem; the per-stage data is not retained today.
    pub has_destruction: bool,
    /// `VMAD` script-attachment presence. Mirrors `NpcRecord::has_script`
    /// and `StaticObject::has_script` — used by downstream tooling to
    /// flag entities that ship Papyrus event handlers we don't yet
    /// dispatch. See [`crate::esm::records::actor::NpcRecord`].
    pub has_script: bool,
}

/// Parse a MOVS record from its sub-record list. Unknown sub-records
/// (`OBND`, `MODT/MODC/MODS/MODF`, `KSIZ`/`KWDA`, the `DSTD/DSTF/DMDL`
/// destruction chain) are ignored — the cell loader only needs
/// `EDID + MODL` for visual placement and the optional sound /
/// destruction / script flags as forward-looking metadata.
///
/// Wire format is FO4-and-later. Earlier games emit no MOVS records;
/// vanilla Fallout4.esm itself ships zero MOVS, but DLC / mod content
/// authors MOVS for breakable furniture, deployable workshop objects,
/// and physics-puzzle props. Empty input yields an all-default record;
/// the caller keys the map by `form_id` either way.
pub fn parse_movs(form_id: u32, subs: &[SubRecord]) -> MovableStaticRecord {
    let editor_id = read_string_sub(subs, b"EDID").unwrap_or_default();
    let model_path = read_string_sub(subs, b"MODL").unwrap_or_default();
    let mut loop_sound_form_id: Option<u32> = None;
    let mut activate_sound_form_id: Option<u32> = None;
    let mut has_destruction = false;
    let mut has_script = false;

    let read_u32 = |bytes: &[u8]| -> Option<u32> {
        if bytes.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };

    for sub in subs {
        match sub.sub_type.as_slice() {
            b"LNAM" => {
                if let Some(form) = read_u32(&sub.data) {
                    loop_sound_form_id = Some(form);
                }
            }
            b"ZNAM" => {
                if let Some(form) = read_u32(&sub.data) {
                    activate_sound_form_id = Some(form);
                }
            }
            b"DEST" => {
                has_destruction = true;
            }
            b"VMAD" => {
                has_script = true;
            }
            _ => {}
        }
    }

    MovableStaticRecord {
        form_id,
        editor_id,
        model_path,
        loop_sound_form_id,
        activate_sound_form_id,
        has_destruction,
        has_script,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_sub(code: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *code,
            data,
        }
    }

    fn edid(name: &str) -> SubRecord {
        let mut z = name.as_bytes().to_vec();
        z.push(0);
        mk_sub(b"EDID", z)
    }

    fn modl(path: &str) -> SubRecord {
        let mut z = path.as_bytes().to_vec();
        z.push(0);
        mk_sub(b"MODL", z)
    }

    /// Baseline: a vanilla-shape MOVS (EDID + MODL + LNAM + ZNAM)
    /// round-trips with every field populated. Models the
    /// `MovableGenerator01` archetype: a humming machine that plays
    /// `loop_sound` while alive and `activate_sound` on impact.
    #[test]
    fn parse_movs_with_sounds_round_trips_all_fields() {
        let subs = vec![
            edid("MovableGenerator01"),
            modl(r"Furniture\Generator\GeneratorMov01.nif"),
            mk_sub(b"LNAM", 0x0001_2345u32.to_le_bytes().to_vec()),
            mk_sub(b"ZNAM", 0x0001_2346u32.to_le_bytes().to_vec()),
        ];
        let rec = parse_movs(0x0010_BEEF, &subs);
        assert_eq!(rec.form_id, 0x0010_BEEF);
        assert_eq!(rec.editor_id, "MovableGenerator01");
        assert_eq!(rec.model_path, r"Furniture\Generator\GeneratorMov01.nif");
        assert_eq!(rec.loop_sound_form_id, Some(0x0001_2345));
        assert_eq!(rec.activate_sound_form_id, Some(0x0001_2346));
        assert!(!rec.has_destruction);
        assert!(!rec.has_script);
    }

    /// Minimal record (EDID + MODL only) — visual placement still
    /// works; the optional sound / destruction / script flags stay at
    /// their `None` / `false` defaults. Mirrors a typical mod-added
    /// MOVS that doesn't bother with sound or breakage.
    #[test]
    fn parse_movs_edid_modl_only_keeps_optional_fields_unset() {
        let subs = vec![edid("PlainMovable"), modl(r"Clutter\Plain01.nif")];
        let rec = parse_movs(0xABCD_0000, &subs);
        assert_eq!(rec.editor_id, "PlainMovable");
        assert_eq!(rec.model_path, r"Clutter\Plain01.nif");
        assert!(rec.loop_sound_form_id.is_none());
        assert!(rec.activate_sound_form_id.is_none());
        assert!(!rec.has_destruction);
        assert!(!rec.has_script);
    }

    /// `DEST` presence flips `has_destruction` even when the per-stage
    /// `DSTD/DSTF/DMDL` chunks aren't consumed — the cell loader uses
    /// the bit to log REFRs that would lose breakable behaviour pending
    /// a destruction subsystem.
    #[test]
    fn parse_movs_dest_chunk_sets_has_destruction_flag() {
        let subs = vec![
            edid("BreakableCrate"),
            modl(r"Clutter\BreakCrate01.nif"),
            mk_sub(b"DEST", vec![0u8; 8]), // payload contents irrelevant to flag
        ];
        let rec = parse_movs(0x0044_0001, &subs);
        assert!(rec.has_destruction);
    }

    /// `VMAD` presence flips `has_script` mirroring NpcRecord /
    /// StaticObject conventions — script attachments are not dispatched
    /// today but the flag lets downstream tooling identify
    /// scripted-MOVS REFRs.
    #[test]
    fn parse_movs_vmad_flips_has_script() {
        let subs = vec![
            edid("ScriptedMovable"),
            modl(r"Clutter\ScriptedMov01.nif"),
            mk_sub(b"VMAD", vec![0u8; 16]),
        ];
        let rec = parse_movs(0x0044_0002, &subs);
        assert!(rec.has_script);
    }

    /// Stray / unknown sub-records (`OBND`, `MODT`, `KSIZ`/`KWDA`)
    /// must NOT corrupt parsing of the captured fields. Verify a
    /// realistic record with all the noise sub-records vanilla FO4
    /// emits still parses cleanly.
    #[test]
    fn parse_movs_ignores_unknown_subs_without_disturbing_captured_fields() {
        let subs = vec![
            edid("NoisyMovable"),
            mk_sub(b"OBND", vec![0u8; 12]),
            modl(r"Clutter\Noisy01.nif"),
            mk_sub(b"MODT", vec![0u8; 64]),
            mk_sub(b"MODS", vec![0u8; 8]),
            mk_sub(b"LNAM", 0xDEAD_BEEFu32.to_le_bytes().to_vec()),
            mk_sub(b"KSIZ", 1u32.to_le_bytes().to_vec()),
            mk_sub(b"KWDA", 0xCAFE_BABEu32.to_le_bytes().to_vec()),
        ];
        let rec = parse_movs(0x0044_0003, &subs);
        assert_eq!(rec.editor_id, "NoisyMovable");
        assert_eq!(rec.model_path, r"Clutter\Noisy01.nif");
        assert_eq!(rec.loop_sound_form_id, Some(0xDEAD_BEEF));
        assert!(rec.activate_sound_form_id.is_none());
    }

    /// Truncated `LNAM` (< 4 bytes) must be silently dropped rather
    /// than panicking. Mirrors the SCOL `parse_scol` defensive
    /// posture against malformed mod content.
    #[test]
    fn parse_movs_truncated_lnam_drops_silently() {
        let subs = vec![
            edid("TruncatedLNAM"),
            modl(r"Clutter\Truncated01.nif"),
            mk_sub(b"LNAM", vec![0u8; 2]),
        ];
        let rec = parse_movs(0x0044_0004, &subs);
        assert!(rec.loop_sound_form_id.is_none());
    }
}
