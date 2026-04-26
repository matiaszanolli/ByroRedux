//! MSWP (Material Swap) — FO4+ per-REFR material substitution table.
//!
//! An MSWP authors a list of `(source.bgsm/.bgem → target.bgsm/.bgem,
//! optional intensity)` substitutions plus an optional path-prefix
//! filter. Cell REFRs reference an MSWP via `XMSP` to apply the
//! substitution table to a given placement (Raider armour colour
//! variants, station-wagon rust variants, vault-decay overlays), or
//! the table is sourced through other workflows (Material Swap form
//! lists, weapon mod outputs).
//!
//! **Sub-record layout** (every record):
//!
//! - `EDID` — editor ID (z-string), exactly once
//! - `FNAM` — optional path-prefix filter (z-string). Limits the swap
//!   to materials whose source path starts with this string.
//! - Repeated `(BNAM, SNAM, optional CNAM)` triples — one per swap
//!   entry. Order matters when the same source path appears more
//!   than once (later entry wins).
//!     - `BNAM` = source material path (z-string)
//!     - `SNAM` = target material path (z-string)
//!     - `CNAM` = optional 4-byte f32 intensity / colour multiplier
//!       attached to the most-recently-emitted (BNAM, SNAM) pair.
//!
//! **Vanilla counts** (Fallout4.esm at audit time):
//!   - 2,537 records
//!   - 5,542 (BNAM, SNAM) pairs (~2.18 per record)
//!   - 156 records carry CNAM (always 4 bytes)
//!   - 1,338 records carry FNAM
//!
//! **Downstream use:** `EsmCellIndex.material_swaps` is the lookup
//! the cell loader will consult once `XMSP` REFR sub-records are
//! parsed (FO4-DIM6-02 stage 2). The XMSP value is a FormID pointing
//! at an MSWP record; the cell loader resolves it here, walks the
//! `swaps` list, and produces per-REFR `TextureSlotSwap` overrides
//! by routing source paths through the existing `TextureSet`
//! infrastructure.
//!
//! See audit FO4-DIM6-05 / #590.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::{find_sub, read_string_sub, read_zstring};

/// One source → target swap entry inside an MSWP record.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialSwapEntry {
    /// Source material path (BGSM / BGEM). Matched verbatim against
    /// the host mesh's authored material slots when applying the swap.
    pub source: String,
    /// Replacement material path. Empty means "fall back to the
    /// authored material" — vanilla MSWPs occasionally ship that
    /// shape with a CNAM-only intensity override.
    pub target: String,
    /// Optional intensity / colour multiplier (f32 in `[0.0, 1.0]`
    /// for vanilla content, but unbounded by the file format). When
    /// `None`, the swap is a straight target-replaces-source. When
    /// `Some(v)`, the renderer-side consumer treats `v` as a
    /// brightness or tint factor on top of the swap.
    pub color_intensity: Option<f32>,
}

/// A parsed MSWP record. Stored on
/// [`crate::esm::cell::EsmCellIndex::material_swaps`] keyed by form
/// ID so cell-load REFR resolution can apply the table.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialSwapRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Optional path-prefix filter limiting which host meshes the
    /// swap applies to. Set on ~53 % of vanilla records (1338/2537).
    /// `None` means "apply to any host mesh".
    pub path_filter: Option<String>,
    /// Swap entries in authoring order. Vanilla records average
    /// ~2.18 entries; the largest ships ~30 entries (Vault damage
    /// theme overlays).
    pub swaps: Vec<MaterialSwapEntry>,
}

/// Parse an MSWP record from its sub-record list. Robust to:
///   - Records missing EDID (one such record exists in vanilla
///     Fallout4.esm — defaults to empty string).
///   - Stray CNAM before any BNAM/SNAM pair (dropped).
///   - SNAM without preceding BNAM (dropped — malformed).
///   - Multiple consecutive BNAMs without SNAMs (each starts a new
///     entry whose target stays empty until the next SNAM lands).
///
/// Wire format is FO4-and-later; pre-FO4 games don't emit MSWP.
pub fn parse_mswp(form_id: u32, subs: &[SubRecord]) -> MaterialSwapRecord {
    let editor_id = read_string_sub(subs, b"EDID").unwrap_or_default();
    let path_filter = read_string_sub(subs, b"FNAM").filter(|s| !s.is_empty());

    let mut swaps: Vec<MaterialSwapEntry> = Vec::new();

    for sub in subs {
        match sub.sub_type.as_slice() {
            b"BNAM" => {
                // Each BNAM starts a new swap entry. Target / colour
                // fields are populated by the matching SNAM / CNAM
                // that follow before the next BNAM.
                swaps.push(MaterialSwapEntry {
                    source: read_zstring(&sub.data),
                    target: String::new(),
                    color_intensity: None,
                });
            }
            b"SNAM" => {
                // SNAM only makes sense in the context of a preceding
                // BNAM. A vanilla scan finds zero stray SNAMs; drop
                // defensively rather than panic on malformed mods.
                if let Some(entry) = swaps.last_mut() {
                    entry.target = read_zstring(&sub.data);
                }
            }
            b"CNAM" => {
                // 4-byte f32 attached to the most-recent (BNAM, SNAM)
                // pair — confirmed via vanilla corpus survey (every
                // CNAM in 156 records is exactly 4 bytes, sitting
                // after its paired SNAM).
                if let Some(entry) = swaps.last_mut() {
                    if sub.data.len() >= 4 {
                        let bytes = [sub.data[0], sub.data[1], sub.data[2], sub.data[3]];
                        entry.color_intensity = Some(f32::from_le_bytes(bytes));
                    }
                }
            }
            _ => {
                // EDID / FNAM are read above via `find_sub`; everything
                // else (FULL, OBND, etc. — none observed in the vanilla
                // corpus but mods can add them) is silently ignored.
            }
        }
    }

    // Drop entries whose source is empty AND target is empty AND
    // colour is None — those are placeholders the parser produced
    // when defensively handling a stray CNAM/SNAM with no preceding
    // BNAM. Keeps the swap list clean for the cell-loader consumer.
    swaps.retain(|e| !(e.source.is_empty() && e.target.is_empty() && e.color_intensity.is_none()));

    MaterialSwapRecord {
        form_id,
        editor_id,
        path_filter,
        swaps,
    }
}

/// Tiny convenience: read the FNAM filter from the sub-record list
/// without parsing the rest of the record. Used by callers that only
/// want to know if the swap applies to a given path (filter peek).
#[allow(dead_code)] // Reserved for the FO4-DIM6-02 stage-2 cell-loader integration.
pub(crate) fn peek_path_filter(subs: &[SubRecord]) -> Option<String> {
    find_sub(subs, b"FNAM")
        .map(read_zstring)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    //! Wire-format tests against synthetic MSWP byte streams that
    //! mirror the vanilla Fallout4.esm shapes uncovered by the
    //! corpus survey for #590.
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(code: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *code,
            data: data.to_vec(),
        }
    }

    fn z(s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        b
    }

    /// Vanilla shape: EDID + (BNAM, SNAM) pair without optional
    /// FNAM/CNAM. Mirrors `StationWagon_Postwar_Cheap04_Swap`.
    #[test]
    fn parse_minimal_record_with_single_pair() {
        let subs = vec![
            sub(b"EDID", &z("StationWagon_Postwar_Cheap04_Swap")),
            sub(
                b"BNAM",
                &z("Vehicles\\Automotive\\StationWagon01a_Rust.BGSM"),
            ),
            sub(
                b"SNAM",
                &z("Vehicles\\Automotive\\StationWagon_Postwar_Cheap04.bgsm"),
            ),
        ];
        let rec = parse_mswp(0x0024_9A4E, &subs);
        assert_eq!(rec.form_id, 0x0024_9A4E);
        assert_eq!(rec.editor_id, "StationWagon_Postwar_Cheap04_Swap");
        assert!(rec.path_filter.is_none());
        assert_eq!(rec.swaps.len(), 1);
        assert_eq!(
            rec.swaps[0].source,
            "Vehicles\\Automotive\\StationWagon01a_Rust.BGSM"
        );
        assert_eq!(
            rec.swaps[0].target,
            "Vehicles\\Automotive\\StationWagon_Postwar_Cheap04.bgsm"
        );
        assert!(rec.swaps[0].color_intensity.is_none());
    }

    /// FNAM filter + multiple swap pairs — Vault damage theme shape.
    #[test]
    fn parse_record_with_fnam_filter_and_multiple_pairs() {
        let subs = vec![
            sub(b"EDID", &z("VaultDamageTheme18")),
            sub(b"FNAM", &z("Interiors\\Vault")),
            sub(b"BNAM", &z("interiors\\Vault\\VltUtilColumns01.BGSM")),
            sub(
                b"SNAM",
                &z("interiors\\Vault\\VltUtilColumns01_Damage.BGSM"),
            ),
            sub(b"BNAM", &z("interiors\\Vault\\VltAtrium01.BGSM")),
            sub(b"SNAM", &z("interiors\\Vault\\VltAtrium01_Damage.BGSM")),
        ];
        let rec = parse_mswp(0x0024_70A8, &subs);
        assert_eq!(rec.path_filter.as_deref(), Some("Interiors\\Vault"));
        assert_eq!(rec.swaps.len(), 2);
        assert_eq!(
            rec.swaps[0].source,
            "interiors\\Vault\\VltUtilColumns01.BGSM"
        );
        assert_eq!(
            rec.swaps[0].target,
            "interiors\\Vault\\VltUtilColumns01_Damage.BGSM"
        );
        assert_eq!(rec.swaps[1].source, "interiors\\Vault\\VltAtrium01.BGSM");
        assert_eq!(
            rec.swaps[1].target,
            "interiors\\Vault\\VltAtrium01_Damage.BGSM"
        );
    }

    /// CNAM attaches to the most-recent (BNAM, SNAM) pair. Mirrors
    /// MachineKitBlueLight02 — every pair carries the same intensity.
    #[test]
    fn parse_cnam_attaches_to_preceding_pair() {
        let intensity_bytes = 0.617_f32.to_le_bytes();
        let subs = vec![
            sub(b"EDID", &z("MachineKitBlueLight02")),
            sub(b"FNAM", &z("SetDressing")),
            sub(b"BNAM", &z("setdressing\\machinekit\\machinekit01.bgsm")),
            sub(b"SNAM", &z("setdressing\\machinekit\\machinekit01.bgsm")),
            sub(b"CNAM", &intensity_bytes),
            sub(
                b"BNAM",
                &z("setdressing\\machinekit\\machinekitquad02.bgsm"),
            ),
            sub(
                b"SNAM",
                &z("setdressing\\machinekit\\machinekitquad02.bgsm"),
            ),
            sub(b"CNAM", &intensity_bytes),
        ];
        let rec = parse_mswp(0x0023_CD5F, &subs);
        assert_eq!(rec.swaps.len(), 2);
        assert!(rec.swaps[0].color_intensity.is_some());
        let c0 = rec.swaps[0].color_intensity.unwrap();
        assert!((c0 - 0.617).abs() < 1e-5, "CNAM round-trip: got {c0}");
        let c1 = rec.swaps[1].color_intensity.unwrap();
        assert!((c1 - 0.617).abs() < 1e-5);
    }

    /// Defensive: a stray CNAM before any BNAM is ignored. Mods
    /// (or future format extensions) might emit one; we drop it
    /// rather than crashing or attaching to a non-existent entry.
    #[test]
    fn stray_cnam_before_bnam_is_dropped() {
        let intensity_bytes = 1.0_f32.to_le_bytes();
        let subs = vec![
            sub(b"EDID", &z("Stray")),
            sub(b"CNAM", &intensity_bytes),
            sub(b"BNAM", &z("a.bgsm")),
            sub(b"SNAM", &z("b.bgsm")),
        ];
        let rec = parse_mswp(0xDEADBEEF, &subs);
        assert_eq!(rec.swaps.len(), 1);
        assert!(rec.swaps[0].color_intensity.is_none());
    }

    /// Defensive: a stray SNAM before any BNAM is ignored — keeps
    /// the swap list well-formed.
    #[test]
    fn stray_snam_before_bnam_is_dropped() {
        let subs = vec![
            sub(b"EDID", &z("Stray")),
            sub(b"SNAM", &z("orphan.bgsm")),
            sub(b"BNAM", &z("a.bgsm")),
            sub(b"SNAM", &z("b.bgsm")),
        ];
        let rec = parse_mswp(0xC0FFEE00, &subs);
        assert_eq!(rec.swaps.len(), 1);
        assert_eq!(rec.swaps[0].source, "a.bgsm");
        assert_eq!(rec.swaps[0].target, "b.bgsm");
    }

    /// FNAM equal to empty string is treated as absent — keeps the
    /// `path_filter` field semantically honest (`None` = "apply to
    /// any path", `Some("")` would be ambiguous).
    #[test]
    fn empty_fnam_is_treated_as_no_filter() {
        let subs = vec![
            sub(b"EDID", &z("NoFilter")),
            sub(b"FNAM", &z("")),
            sub(b"BNAM", &z("a.bgsm")),
            sub(b"SNAM", &z("b.bgsm")),
        ];
        let rec = parse_mswp(0xAB12, &subs);
        assert!(rec.path_filter.is_none());
    }

    /// Records with no BNAM / SNAM at all (rare — vanilla has none
    /// but mods could) round-trip cleanly with an empty `swaps` list.
    #[test]
    fn record_with_no_swaps_is_well_formed() {
        let subs = vec![sub(b"EDID", &z("Empty"))];
        let rec = parse_mswp(0xCAFE_F00D, &subs);
        assert!(rec.swaps.is_empty());
        assert_eq!(rec.editor_id, "Empty");
    }
}
