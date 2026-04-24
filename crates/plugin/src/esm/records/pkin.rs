//! PKIN (Pack-In) — FO4+ reusable content bundle.
//!
//! A PKIN groups one or more base records (typically LVLI / CONT /
//! STAT / MSTT / FURN) behind a single form ID so a level designer
//! can drop "a generic workbench with loot" as one REFR instead of
//! authoring every child placement individually. The CELL parser
//! surfaces the REFR normally; at spawn time the cell loader resolves
//! the base FormID, sees it's a PKIN, and enumerates the PKIN's
//! [`PkinRecord::contents`] list — emitting one synthetic placement
//! per content ref at the outer REFR's transform.
//!
//! **Sub-record layout** (per FO4 xEdit v4.2 / UESP `Fallout4Mod:PKIN`):
//!
//! - `EDID` — editor ID (z-string; required on vanilla records)
//! - `FULL` — optional display name (z-string); present on a minority
//!   of records, usually mod-authored.
//! - `CNAM` — u32 form ID of the content base record. Vanilla authors
//!   typically ship a single CNAM per PKIN; we collect every CNAM
//!   sub-record so authored-multi-child bundles round-trip.
//! - `VNAM` — optional u32 form ID (workshop / preview marker).
//!   Semantics not documented by community tools; captured for future
//!   consumer wiring.
//! - `FNAM` — optional u32 flag bits (bit 1 = "Location Reference
//!   Type", bit 2 = "Perk" per xEdit comments). Captured verbatim.
//!
//! Vanilla Fallout4.esm ships 872 PKIN records. Pre-#589 the cell
//! parser routed PKIN through the MODL-only catch-all at `cell.rs:521`
//! which silently produced a `StaticObject { model_path: "" }` — the
//! CNAM-driven content list was discarded on every record. REFR spawn
//! sites would then see an empty model path, drop through to the
//! light-only branch (no LIGH data either), and contribute zero world
//! content.
//!
//! See audit FO4-DIM4-03 / #589.

use crate::esm::reader::SubRecord;
use crate::esm::records::common::read_string_sub;

/// Parsed PKIN record.
#[derive(Debug, Clone, PartialEq)]
pub struct PkinRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Display name (`FULL`). Empty when the record omits the sub.
    pub full_name: String,
    /// Content form IDs resolved from `CNAM` sub-records. Each entry
    /// points at a LVLI / CONT / STAT / MSTT / FURN (or similar) that
    /// defines the actual packed content. Vanilla records typically
    /// carry one CNAM; multi-CNAM records are accepted for safety.
    pub contents: Vec<u32>,
    /// Optional `VNAM` form ID — community tools describe this as a
    /// workshop / preview reference. Kept verbatim for future consumer
    /// wiring. `0` when the record omits the sub.
    pub vnam_form_id: u32,
    /// `FNAM` flag bits (xEdit comments: bit 1 = "Location Reference
    /// Type", bit 2 = "Perk"). `0` when the record omits the sub.
    pub flags: u32,
}

/// Parse a PKIN record from its sub-record list. Unknown sub-records
/// are ignored. Empty input yields a `PkinRecord` with empty fields —
/// the caller keys the map by `form_id` either way.
pub fn parse_pkin(form_id: u32, subs: &[SubRecord]) -> PkinRecord {
    let editor_id = read_string_sub(subs, b"EDID").unwrap_or_default();
    let full_name = read_string_sub(subs, b"FULL").unwrap_or_default();
    let mut contents: Vec<u32> = Vec::new();
    let mut vnam_form_id = 0u32;
    let mut flags = 0u32;

    let read_u32 = |bytes: &[u8]| -> Option<u32> {
        if bytes.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };

    for sub in subs {
        match sub.sub_type.as_slice() {
            b"CNAM" => {
                if let Some(form) = read_u32(&sub.data) {
                    contents.push(form);
                }
            }
            b"VNAM" => {
                if let Some(form) = read_u32(&sub.data) {
                    vnam_form_id = form;
                }
            }
            b"FNAM" => {
                if let Some(bits) = read_u32(&sub.data) {
                    flags = bits;
                }
            }
            _ => {}
        }
    }

    PkinRecord {
        form_id,
        editor_id,
        full_name,
        contents,
        vnam_form_id,
        flags,
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

    fn cnam(form_id: u32) -> SubRecord {
        mk_sub(b"CNAM", form_id.to_le_bytes().to_vec())
    }

    /// Baseline: a vanilla-shape PKIN (EDID + single CNAM + VNAM +
    /// FNAM) round-trips with every field populated.
    #[test]
    fn parse_pkin_single_cnam_round_trip() {
        let subs = vec![
            edid("PackIn_WorkbenchLoot"),
            cnam(0x0010_1234), // content form: a CONT or LVLI
            mk_sub(b"VNAM", 0x0002_5678u32.to_le_bytes().to_vec()),
            mk_sub(b"FNAM", 0x0000_0002u32.to_le_bytes().to_vec()),
        ];
        let rec = parse_pkin(0x0055_0001, &subs);
        assert_eq!(rec.form_id, 0x0055_0001);
        assert_eq!(rec.editor_id, "PackIn_WorkbenchLoot");
        assert_eq!(rec.contents, vec![0x0010_1234]);
        assert_eq!(rec.vnam_form_id, 0x0002_5678);
        assert_eq!(rec.flags, 0x0000_0002);
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
        let rec = parse_pkin(0x0055_0002, &subs);
        assert_eq!(
            rec.contents,
            vec![0x0010_0001, 0x0010_0002, 0x0010_0003]
        );
    }

    /// A PKIN that ships only EDID + FULL — no CNAM at all — is a
    /// malformed / author-trimmed record. Parser must not panic; the
    /// resulting `contents` list is empty so the cell loader falls
    /// through to the default single-entry path.
    #[test]
    fn parse_pkin_without_cnam_yields_empty_contents() {
        let subs = vec![edid("PackIn_EmptyDecl"), mk_sub(b"FULL", b"Shell\0".to_vec())];
        let rec = parse_pkin(0x0055_0003, &subs);
        assert_eq!(rec.editor_id, "PackIn_EmptyDecl");
        assert_eq!(rec.full_name, "Shell");
        assert!(rec.contents.is_empty());
        assert_eq!(rec.vnam_form_id, 0);
        assert_eq!(rec.flags, 0);
    }

    /// Truncated CNAM (< 4 bytes) is silently dropped rather than
    /// crashing — mirrors every other record parser's "short sub-record
    /// ignored" policy.
    #[test]
    fn parse_pkin_truncated_cnam_silently_dropped() {
        let subs = vec![
            edid("PackIn_Trunc"),
            mk_sub(b"CNAM", vec![0x11, 0x22]), // 2 bytes, too short
            cnam(0x0010_1234),
        ];
        let rec = parse_pkin(0x0055_0004, &subs);
        // Only the well-formed CNAM survives.
        assert_eq!(rec.contents, vec![0x0010_1234]);
    }
}
