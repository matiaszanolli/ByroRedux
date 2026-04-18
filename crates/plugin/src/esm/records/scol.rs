//! SCOL (Static Collection) — FO4+ composite static record.
//!
//! An SCOL packages a set of child base forms (STATs, usually) with
//! per-child placement arrays into a single collection. Fallout 4 uses
//! these to ship prebuilt building interiors, clutter groupings, and
//! furniture arrangements that are placed in cells by a single REFR.
//!
//! **Sub-record layout:**
//!
//! - `EDID` — editor ID (z-string)
//! - `OBND` — object bounds (6 × i16, unused by the parser)
//! - `MODL` — cached combined mesh path (z-string, often empty; see
//!   `CM*.NIF` paths like `SCOL\Fallout4.esm\CM00249DF2.NIF`)
//! - `MODT` — mesh texture hashes (unused by the parser)
//! - `ONAM` + `DATA` pairs (repeated):
//!     - `ONAM` = 4 B base form ID of the child (STAT, MSTT, …)
//!     - `DATA` = repeated 28 B placement records:
//!         `pos[3 × f32] + rot[3 × f32] + scale[f32]`
//! - `FLTR` — filter form IDs (unused by the parser, kept optional)
//!
//! Every `DATA` block always follows an `ONAM`; a vanilla Fallout4.esm
//! scan counted 2617 SCOL records × ~6 ONAM/DATA pairs = 15,878 per-
//! child placements, every one of which the pre-#405 `parse_modl_group`
//! arm discarded silently.
//!
//! **Downstream use:** the cell loader expands an SCOL REFR into N
//! synthetic placed refs when the cached `CM*.NIF` isn't present
//! (common for mod-added SCOLs whose previsibine step was skipped).
//! See the `scol_parts` field on [`crate::esm::cell::StaticObject`].

use crate::esm::reader::SubRecord;
use crate::esm::records::common::read_string_sub;

/// One per-child placement inside an SCOL — position / rotation
/// (Euler XYZ, radians) / uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScolPlacement {
    /// World-space offset from the SCOL REFR origin, in game units.
    pub pos: [f32; 3],
    /// Euler angles in radians (Bethesda Z-up); the cell loader
    /// converts to the engine's Y-up quaternion at spawn time.
    pub rot: [f32; 3],
    /// Uniform scale multiplier.
    pub scale: f32,
}

impl ScolPlacement {
    /// Raw SCOL DATA entry size. Size is fixed — any DATA length that
    /// isn't a multiple of 28 indicates a parser mismatch (the 7-entry
    /// mod-made variant from Fallout 3's CK is an outlier we don't
    /// target; FO4 / FO76 / Starfield all use this 28-byte layout).
    pub const WIRE_SIZE: usize = 28;

    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::WIRE_SIZE {
            return None;
        }
        let read_f32 = |off: usize| {
            f32::from_le_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
            ])
        };
        Some(Self {
            pos: [read_f32(0), read_f32(4), read_f32(8)],
            rot: [read_f32(12), read_f32(16), read_f32(20)],
            scale: read_f32(24),
        })
    }
}

/// One `(ONAM, DATA)` pair from an SCOL body — the child base form ID
/// plus every placement authored against it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScolPart {
    /// Form ID of the child base record (STAT / MSTT / …).
    pub base_form_id: u32,
    /// One entry per placement slot. Pre-#405 the parser dropped
    /// the entire list; on vanilla FO4 a CambridgeDecoInt01 SCOL
    /// carries 10 placements (280 B of DATA).
    pub placements: Vec<ScolPlacement>,
}

/// A parsed SCOL record.
#[derive(Debug, Clone, PartialEq)]
pub struct ScolRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Cached combined mesh path (`SCOL\Fallout4.esm\CM*.NIF`). Empty
    /// string when the author didn't ship a CM-file — typical for
    /// mod-added SCOLs. When empty the cell loader falls back to
    /// expanding `parts` into synthetic refs.
    pub model_path: String,
    /// One entry per `(ONAM, DATA)` pair in the record, in authoring
    /// order. Empty when the record carries no children (rare — a
    /// vanilla FO4 scan found zero such SCOLs).
    pub parts: Vec<ScolPart>,
    /// FLTR filter form IDs when present. FO4 ships 2244 SCOLs with
    /// FLTR entries out of 2617 — content the previs system uses to
    /// exclude the SCOL from certain lighting / shadow passes. We
    /// only retain the IDs; actual filtering is downstream work.
    pub filter: Vec<u32>,
}

/// Parse an SCOL record from its sub-record list. Unknown sub-records
/// (OBND, MODT, FLTR format variants, FULL, PTRN, MODS) are ignored —
/// the renderer / cell loader only needs EDID / MODL / ONAM-DATA.
/// Wire format is FO4-and-later; earlier games don't emit SCOL.
pub fn parse_scol(form_id: u32, subs: &[SubRecord]) -> ScolRecord {
    let editor_id = read_string_sub(subs, b"EDID").unwrap_or_default();
    let model_path = read_string_sub(subs, b"MODL").unwrap_or_default();

    let mut parts: Vec<ScolPart> = Vec::new();
    let mut current_base: Option<u32> = None;
    let mut filter: Vec<u32> = Vec::new();

    for sub in subs {
        match sub.sub_type.as_slice() {
            b"ONAM" => {
                if sub.data.len() >= 4 {
                    current_base = Some(u32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ]));
                    // Each ONAM starts a new ScolPart — push one even
                    // with zero placements so `parts.len() == number
                    // of ONAMs in the record`. The paired DATA fills
                    // the `placements` list below.
                    parts.push(ScolPart {
                        base_form_id: current_base.unwrap(),
                        placements: Vec::new(),
                    });
                }
            }
            b"DATA" => {
                // DATA is only meaningful in the context of a
                // preceding ONAM. A DATA without a leading ONAM is a
                // malformed record; drop silently rather than crashing.
                if current_base.is_none() {
                    continue;
                }
                let Some(part) = parts.last_mut() else {
                    continue;
                };
                let placement_count = sub.data.len() / ScolPlacement::WIRE_SIZE;
                part.placements.reserve(placement_count);
                for i in 0..placement_count {
                    let off = i * ScolPlacement::WIRE_SIZE;
                    if let Some(p) = ScolPlacement::from_bytes(&sub.data[off..]) {
                        part.placements.push(p);
                    }
                }
            }
            b"FLTR" => {
                // FLTR is a flat array of form IDs — length / 4. Some
                // records ship one ID, others a list; we just collect
                // them all.
                let id_count = sub.data.len() / 4;
                filter.reserve(id_count);
                for i in 0..id_count {
                    let off = i * 4;
                    if off + 4 <= sub.data.len() {
                        filter.push(u32::from_le_bytes([
                            sub.data[off],
                            sub.data[off + 1],
                            sub.data[off + 2],
                            sub.data[off + 3],
                        ]));
                    }
                }
            }
            _ => {}
        }
    }

    // Log-once diagnostic for records that parsed cleanly but
    // carry zero children — vanilla FO4 has none, so this catches
    // schema regressions early.
    if parts.is_empty() {
        log::debug!(
            "SCOL {:08X} ('{}') has zero ONAM/DATA pairs — mesh will fall back to MODL '{}'",
            form_id,
            editor_id,
            model_path,
        );
    }

    ScolRecord {
        form_id,
        editor_id,
        model_path,
        parts,
        filter,
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

    fn onam(form_id: u32) -> SubRecord {
        mk_sub(b"ONAM", form_id.to_le_bytes().to_vec())
    }

    fn data(placements: &[ScolPlacement]) -> SubRecord {
        let mut buf = Vec::with_capacity(placements.len() * ScolPlacement::WIRE_SIZE);
        for p in placements {
            for v in &p.pos {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            for v in &p.rot {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            buf.extend_from_slice(&p.scale.to_le_bytes());
        }
        mk_sub(b"DATA", buf)
    }

    /// Regression: #405 — a vanilla-style SCOL with 2 ONAM/DATA pairs
    /// must surface both children with all placements intact. Pre-fix
    /// the whole record was routed through the MODL-only parser and
    /// the placements were discarded.
    #[test]
    fn parse_scol_two_onam_data_pairs_round_trip() {
        let p1a = ScolPlacement {
            pos: [10.0, 20.0, 30.0],
            rot: [0.0, 0.0, 1.57],
            scale: 1.0,
        };
        let p1b = ScolPlacement {
            pos: [40.0, 50.0, 60.0],
            rot: [0.0, 1.57, 0.0],
            scale: 1.5,
        };
        let p2a = ScolPlacement {
            pos: [-1.0, -2.0, -3.0],
            rot: [3.14, 0.0, 0.0],
            scale: 2.0,
        };
        let subs = vec![
            edid("TestScol"),
            modl(r"SCOL\Fallout4.esm\CM00249DF2.NIF"),
            onam(0x0001_A001),
            data(&[p1a, p1b]),
            onam(0x0001_A002),
            data(&[p2a]),
        ];

        let rec = parse_scol(0x0024_9DF2, &subs);
        assert_eq!(rec.editor_id, "TestScol");
        assert_eq!(rec.model_path, r"SCOL\Fallout4.esm\CM00249DF2.NIF");
        assert_eq!(rec.parts.len(), 2);
        assert_eq!(rec.parts[0].base_form_id, 0x0001_A001);
        assert_eq!(rec.parts[0].placements, vec![p1a, p1b]);
        assert_eq!(rec.parts[1].base_form_id, 0x0001_A002);
        assert_eq!(rec.parts[1].placements, vec![p2a]);
    }

    /// Regression: #405 — DATA without a preceding ONAM is malformed;
    /// drop the stray data silently instead of panicking.
    #[test]
    fn parse_scol_rejects_orphan_data_without_onam() {
        let subs = vec![
            edid("MalformedScol"),
            data(&[ScolPlacement {
                pos: [0.0, 0.0, 0.0],
                rot: [0.0, 0.0, 0.0],
                scale: 1.0,
            }]),
        ];
        let rec = parse_scol(0xC0FFEE00, &subs);
        assert!(rec.parts.is_empty());
    }

    /// Regression: #405 — an SCOL can author an ONAM with an empty DATA
    /// (a child base that's declared but has zero placements this
    /// pack). The parser must surface the part with an empty list so
    /// downstream accounting matches the `ONAM` count exactly.
    #[test]
    fn parse_scol_onam_without_following_data_keeps_empty_part() {
        let subs = vec![
            edid("OnamOnly"),
            onam(0x0000_ABCD),
        ];
        let rec = parse_scol(0xDEAD_BEEF, &subs);
        assert_eq!(rec.parts.len(), 1);
        assert_eq!(rec.parts[0].base_form_id, 0x0000_ABCD);
        assert!(rec.parts[0].placements.is_empty());
    }

    /// Regression: #405 — FLTR is a flat array of form IDs; a single
    /// record can carry N of them. Verify a 2-id FLTR round-trips.
    #[test]
    fn parse_scol_fltr_array_collects_every_form_id() {
        let mut fltr_data = Vec::new();
        fltr_data.extend_from_slice(&0x0000_1111u32.to_le_bytes());
        fltr_data.extend_from_slice(&0x0000_2222u32.to_le_bytes());
        let subs = vec![edid("Filtered"), mk_sub(b"FLTR", fltr_data)];
        let rec = parse_scol(0xABCD_0000, &subs);
        assert_eq!(rec.filter, vec![0x0000_1111, 0x0000_2222]);
    }

    /// Regression: #405 — a truncated DATA (length not a multiple of
    /// 28 B) must surface the whole placements that fit and drop
    /// the trailing partial entry rather than crashing.
    #[test]
    fn parse_scol_truncated_data_drops_trailing_partial() {
        // Build a DATA with 1 complete placement (28 B) + 10 B of junk.
        let p = ScolPlacement {
            pos: [1.0, 2.0, 3.0],
            rot: [0.0, 0.0, 0.0],
            scale: 1.0,
        };
        let mut data_bytes = Vec::new();
        for v in &p.pos {
            data_bytes.extend_from_slice(&v.to_le_bytes());
        }
        for v in &p.rot {
            data_bytes.extend_from_slice(&v.to_le_bytes());
        }
        data_bytes.extend_from_slice(&p.scale.to_le_bytes());
        data_bytes.extend_from_slice(&[0u8; 10]); // truncated partial

        let subs = vec![edid("Trunc"), onam(0x0000_0001), mk_sub(b"DATA", data_bytes)];
        let rec = parse_scol(0x1111_2222, &subs);
        assert_eq!(rec.parts.len(), 1);
        assert_eq!(rec.parts[0].placements, vec![p]);
    }
}
