//! PKIN (Pack-In) — FO4+ reusable content bundle.
//!
//! A PKIN names one **template CELL** (`CNAM`) whose references are the
//! pack-in's contents: a level designer drops the pack-in in the CK and
//! the CK *bakes* that CELL's placements into ordinary REFRs at placement
//! time. Vanilla data carries **no PKIN-based REFR** (census, #5231:
//! 0 across Fallout4.esm and every DLC), so the pack-in never round-trips
//! through a shipped plugin — the baked REFRs do. The expander's
//! content-base fan-out below therefore only ever fires on mod-authored
//! PKIN REFRs; a CELL-typed CNAM (every vanilla one) is logged as an
//! explicit miss instead, because instancing a template CELL's references
//! under an outer transform is unimplemented (#5231).
//!
//! **Sub-record layout** (per FO4 xEdit v4.2 / UESP `Fallout4Mod:PKIN`):
//!
//! - `EDID` — editor ID (z-string; required on vanilla records)
//! - `FULL` — optional display name (z-string); present on a minority
//!   of records, usually mod-authored.
//! - `CNAM` — u32 form ID of the pack-in's **template CELL** (#5231
//!   corrected the model: community docs once read this as a content
//!   base record — LVLI/CONT/STAT — but every vanilla CNAM resolves to a
//!   CELL: 872/872 Fallout4.esm, 20/21 DLCRobot, 64/64 DLCCoast, 6/6
//!   DLCworkshop03, 46/46 DLCNukaWorld). Vanilla authors typically ship
//!   a single CNAM per PKIN; we collect every CNAM sub-record so
//!   authored-multi-child bundles round-trip.
//! - `VNAM` — optional u32 integer *Version* (xEdit `FO4.pas` PKIN:
//!   `wbInteger(VNAM, 'Version', itU32)`). NOT a form ID — #5422: the
//!   pre-fix decode routed it through `remap_fid` and documented it as
//!   an unknown "workshop / preview marker", the inverse-remap shape
//!   (a plain integer fed to the FormID remap). Vanilla census: 741/741
//!   Fallout4.esm carry `0`; DLCRobot ships one `1`. No consumer yet.
//! - `FNAM` — optional u32 flag bits (bit 1 = "Location Reference
//!   Type", bit 2 = "Perk" per xEdit comments). Captured verbatim.
//! - `FLTR` — optional zstring, the CK object-window filter path the
//!   pack-in is filed under (#5493: xEdit `wbFLTR := wbString(FLTR,
//!   'Filter')`, `FO4.pas:5274` / `FO76.pas:7040` / `SF1.pas:5697`).
//!   230 of 872 vanilla Fallout4.esm PKINs ship `FLTR`, every one
//!   NUL-terminated text (`SetDressing\IndustrialMachines\`,
//!   `DummyObjects\`, `\lights\`). Pre-#5493 the sub was decoded as a
//!   flat `u32` FormID array with `remap_fid` per 4-byte slice — ASCII
//!   path bytes always tripped the remap's "genuinely suspicious"
//!   warn arm (2,843 false warnings across the FO4 DLCs, 3,857 on
//!   Shattered Space), and the stored "IDs" were garbage. The quest
//!   parser's own FLTR (`misc/quest.rs`) was already a zstring.
//!
//! Vanilla Fallout4.esm ships 872 PKIN records. Pre-#589 the cell
//! parser routed PKIN through the MODL-only catch-all at `cell.rs:521`
//! which silently produced a `StaticObject { model_path: "" }` — the
//! CNAM list was discarded on every record. REFR spawn sites would then
//! see an empty model path, drop through to the light-only branch (no
//! LIGH data either), and contribute zero world content.
//!
//! See audit FO4-DIM4-03 / #589; the CNAM semantic correction is #5231
//! (FO4-D4-02).

use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::records::common::{remap_fid, CommonNamedFields};

/// Parsed PKIN record.
#[derive(Debug, Clone, PartialEq)]
pub struct PkinRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Display name (`FULL`). Empty when the record omits the sub.
    pub full_name: String,
    /// Form IDs resolved from `CNAM` sub-records. **Every vanilla CNAM is
    /// the pack-in's template CELL** (#5231) — the CK bakes that CELL's
    /// placements into ordinary REFRs at drop time, which is why vanilla
    /// carries no PKIN-based REFR. A base-record (LVLI/CONT/STAT/…)
    /// CNAM is a mod-authored shape; the spawn-time expander fans those
    /// out and logs CELL-typed CNAMs as an explicit miss. Vanilla records
    /// typically carry one CNAM; multi-CNAM records are accepted for
    /// safety.
    pub contents: Vec<u32>,
    /// `VNAM` — the record's integer *Version* (xEdit `FO4.pas`:
    /// `wbInteger(VNAM, 'Version', itU32)`), stored raw: it is not a
    /// FormID and must never ride the remap (#5422). `0` when the
    /// record omits the sub.
    pub version: u32,
    /// `FNAM` flag bits (xEdit comments: bit 1 = "Location Reference
    /// Type", bit 2 = "Perk"). `0` when the record omits the sub.
    pub flags: u32,
    /// `FLTR` — the CK object-window filter path (zstring, #5493).
    /// Empty when the record omits the sub. Same decode as
    /// `ScolRecord::filter`; not a FormID, never remapped.
    pub filter: String,
}

/// Parse a PKIN record from its sub-record list. Unknown sub-records
/// are ignored. Empty input yields a `PkinRecord` with empty fields —
/// the caller keys the map by `form_id` either way.
pub fn parse_pkin(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> PkinRecord {
    // EDID + FULL via shared helper (FULL is lstring on Skyrim-localized
    // plugins per #348; helper handles both forms). TD3-203 / #1113.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut contents: Vec<u32> = Vec::new();
    let mut version = 0u32;
    let mut flags = 0u32;
    let mut filter = String::new();

    let read_u32 = |bytes: &[u8]| -> Option<u32> {
        if bytes.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };

    for sub in subs {
        match sub.sub_type.as_slice() {
            // #3400 — `contents` is looked up in `index.packins` /
            // `index.statics`, both keyed by the REMAPPED id. Left raw,
            // `expand_packin_placements_with_depth` returned `None` for
            // the whole package-in on every plugin past the first.
            b"CNAM" => {
                if let Some(form) = read_u32(&sub.data) {
                    contents.push(remap_fid(form, remap));
                }
            }
            b"VNAM" => {
                // #5422 — xEdit's integer Version, not a form ID: keep it
                // raw, away from remap_fid (the inverse shape #5075/#5076
                // guarded elsewhere).
                if let Some(value) = read_u32(&sub.data) {
                    version = value;
                }
            }
            b"FNAM" => {
                if let Some(bits) = read_u32(&sub.data) {
                    flags = bits;
                }
            }
            b"FLTR" => {
                // #5493 — xEdit's wbFLTR is a zstring (the CK
                // object-window filter path), not a FormID array. See
                // the module doc for the false-remap-warning census.
                filter = crate::esm::records::common::read_zstring(&sub.data);
            }
            _ => {}
        }
    }

    PkinRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        contents,
        version,
        flags,
        filter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::{edid, sub};

    fn cnam(form_id: u32) -> SubRecord {
        sub(b"CNAM", form_id.to_le_bytes())
    }

    /// Baseline: a vanilla-shape PKIN (EDID + single CNAM + VNAM +
    /// FNAM) round-trips with every field populated. FLTR is
    /// absent so `filter` is the empty Vec.
    #[test]
    fn parse_pkin_single_cnam_round_trip() {
        let subs = vec![
            edid("PackIn_WorkbenchLoot"),
            cnam(0x0010_1234), // content form: a CONT or LVLI
            sub(b"VNAM", 0x0002_5678u32.to_le_bytes()),
            sub(b"FNAM", 0x0000_0002u32.to_le_bytes()),
        ];
        let rec = parse_pkin(0x0055_0001, &subs, &None);
        assert_eq!(rec.form_id, 0x0055_0001);
        assert_eq!(rec.editor_id, "PackIn_WorkbenchLoot");
        assert_eq!(rec.contents, vec![0x0010_1234]);
        assert_eq!(rec.version, 0x0002_5678);
        assert_eq!(rec.flags, 0x0000_0002);
        assert!(
            rec.filter.is_empty(),
            "absent FLTR sub-record yields an empty filter Vec",
        );
    }

    /// Multi-CNAM defensive path — a mod-authored PKIN that ships
    /// several content refs. Every CNAM must be captured in authoring
    /// order so downstream consumers iterate in the right sequence.
    #[test]
    fn parse_pkin_multiple_cnam_preserves_authoring_order() {
        let subs = vec![
            edid("PackIn_Multi"),
            cnam(0x0010_0001),
            cnam(0x0010_0002),
            cnam(0x0010_0003),
        ];
        let rec = parse_pkin(0x0055_0002, &subs, &None);
        assert_eq!(rec.contents, vec![0x0010_0001, 0x0010_0002, 0x0010_0003]);
    }

    /// A PKIN that ships only EDID + FULL — no CNAM at all — is a
    /// malformed / author-trimmed record. Parser must not panic; the
    /// resulting `contents` list is empty so the cell loader falls
    /// through to the default single-entry path.
    #[test]
    fn parse_pkin_without_cnam_yields_empty_contents() {
        let subs = vec![edid("PackIn_EmptyDecl"), sub(b"FULL", b"Shell\0")];
        let rec = parse_pkin(0x0055_0003, &subs, &None);
        assert_eq!(rec.editor_id, "PackIn_EmptyDecl");
        assert_eq!(rec.full_name, "Shell");
        assert!(rec.contents.is_empty());
        assert_eq!(rec.version, 0);
        assert_eq!(rec.flags, 0);
    }

    /// Regression for #815 / FO4-D4-NEW-03 / #5493: 230 of 872
    /// vanilla `Fallout4.esm` PKIN records ship `FLTR` — the CK
    /// object-window filter path, a zstring. Pre-#815 the parser
    /// dropped it; pre-#5493 it decoded the text as a FormID array.
    #[test]
    fn parse_pkin_fltr_round_trips_the_filter_path() {
        let subs = vec![
            edid("PackIn_Filtered"),
            cnam(0x0010_1234),
            sub(b"FLTR", b"\\lights\\\0".to_vec()),
        ];
        let rec = parse_pkin(0x0055_0010, &subs, &None);
        assert_eq!(rec.contents, vec![0x0010_1234]);
        assert_eq!(
            rec.filter,
            "\\lights\\",
            "FLTR is the CK filter path, mirroring SCOL (#5493)",
        );
    }

    /// Edge case: a record that ships only a FLTR sub (no CNAM) —
    /// captures the filter list without panicking on the missing
    /// content array. Mirrors the
    /// `parse_pkin_without_cnam_yields_empty_contents` shape on the
    /// CNAM side.
    #[test]
    fn parse_pkin_fltr_only_yields_empty_contents() {
        // #5493 — real-shape payload: a NUL-terminated filter path.
        let subs = vec![
            edid("PackIn_FltrOnly"),
            sub(b"FLTR", b"SetDressing\\IndustrialMachines\\\0".to_vec()),
        ];
        let rec = parse_pkin(0x0055_0011, &subs, &None);
        assert!(rec.contents.is_empty());
        assert_eq!(rec.filter, "SetDressing\\IndustrialMachines\\");
    }

    /// #5493 — an unterminated FLTR still decodes to the whole payload
    /// (read_zstring's tolerant shape); it is editor text, not a fixed-
    /// width array, so there is no "partial id" class to drop.
    #[test]
    fn parse_pkin_fltr_unterminated_decodes_whole_payload() {
        let subs = vec![
            edid("PackIn_TruncFltr"),
            sub(b"FLTR", b"DummyObjects".to_vec()),
        ];
        let rec = parse_pkin(0x0055_0012, &subs, &None);
        assert_eq!(rec.filter, "DummyObjects");
    }

    /// Truncated CNAM (< 4 bytes) is silently dropped rather than
    /// crashing — mirrors every other record parser's "short sub-record
    /// ignored" policy.
    #[test]
    fn parse_pkin_truncated_cnam_silently_dropped() {
        let subs = vec![
            edid("PackIn_Trunc"),
            sub(b"CNAM", vec![0x11, 0x22]), // 2 bytes, too short
            cnam(0x0010_1234),
        ];
        let rec = parse_pkin(0x0055_0004, &subs, &None);
        // Only the well-formed CNAM survives.
        assert_eq!(rec.contents, vec![0x0010_1234]);
    }
}

#[cfg(test)]
mod remap_tests {
    use super::*;
    use crate::esm::reader::GlobalSlot;

    /// #3400 — `PKIN.CNAM` names the package-in's content base record, and
    /// `expand_packin_placements_with_depth` looks it up in
    /// `index.packins` / `index.statics`. Left raw it returned `None` for
    /// the *whole* package-in on the second and later plugin of a load
    /// order. Remap shape: the third plugin of a FO4 DLC order, whose own
    /// forms are authored `0x01…` but keyed `0x02…`.
    #[test]
    fn pkin_content_forms_are_remapped_into_global_space() {
        let remap = Some(FormIdRemap {
            plugin_slot: GlobalSlot::Regular(0x02),
            master_slots: vec![GlobalSlot::Regular(0x00)],
        });
        let mk = |typ: &[u8; 4], v: u32| SubRecord {
            sub_type: *typ,
            data: v.to_le_bytes().to_vec(),
        };
        let subs = vec![
            mk(b"CNAM", 0x0100_1111),
            mk(b"CNAM", 0x0000_2222), // master-owned
            mk(b"VNAM", 0x0100_3333),
        ];

        let pkin = parse_pkin(0x0200_0001, &subs, &remap);

        assert_eq!(pkin.contents, vec![0x0200_1111, 0x0000_2222]);
    }

    /// #5493 — FLTR is editor text (the CK object-window filter path),
    /// so a remap that rewrites every real FormID slot must leave it
    /// untouched. The pre-fix u32 decode fed ASCII slices to
    /// `remap_fid`, logging a false "genuinely suspicious" warning per
    /// slice on every DLC load (2,843 on the FO4 DLCs, 3,857 on
    /// Shattered Space).
    #[test]
    fn pkin_fltr_filter_path_stays_raw_under_remap() {
        let remap = Some(FormIdRemap {
            plugin_slot: GlobalSlot::Regular(0x02),
            master_slots: vec![GlobalSlot::Regular(0x00)],
        });
        let subs = vec![
            SubRecord {
                sub_type: *b"CNAM",
                data: 0x0100_1111u32.to_le_bytes().to_vec(),
            },
            SubRecord {
                sub_type: *b"FLTR",
                data: b"\\lights\\\0".to_vec(),
            },
        ];
        let pkin = parse_pkin(0x0200_0001, &subs, &remap);
        assert_eq!(pkin.filter, "\\lights\\");
    }

    /// #5422 — `VNAM` is xEdit's integer Version, not a form ID: it must
    /// stay RAW under a remap that rewrites every real FormID slot. The
    /// pre-fix decode routed it through `remap_fid`, so the plugin-local
    /// `0x0100_3333` "version" would have been rewritten to `0x0200_3333`
    /// (and a non-zero high byte warned as out-of-range on other shapes).
    #[test]
    fn pkin_vnam_version_stays_raw_under_remap() {
        let remap = Some(FormIdRemap {
            plugin_slot: GlobalSlot::Regular(0x02),
            master_slots: vec![GlobalSlot::Regular(0x00)],
        });
        let mk = |typ: &[u8; 4], v: u32| SubRecord {
            sub_type: *typ,
            data: v.to_le_bytes().to_vec(),
        };
        let pkin = parse_pkin(
            0x0200_0001,
            &[mk(b"CNAM", 0x0100_1111), mk(b"VNAM", 0x0100_3333)],
            &remap,
        );
        assert_eq!(pkin.version, 0x0100_3333, "the Version never rides the remap");
        assert_eq!(pkin.contents, vec![0x0200_1111], "CNAM still remaps");
    }

    /// Null stays null, and no remap leaves the parse unchanged.
    #[test]
    fn pkin_null_and_unremapped_paths_are_unchanged() {
        let mk = |typ: &[u8; 4], v: u32| SubRecord {
            sub_type: *typ,
            data: v.to_le_bytes().to_vec(),
        };
        let pkin = parse_pkin(
            0x0100_0002,
            &[mk(b"CNAM", 0x0100_1111), mk(b"VNAM", 0)],
            &None,
        );
        assert_eq!(pkin.contents, vec![0x0100_1111]);
        assert_eq!(pkin.version, 0);
    }
}
