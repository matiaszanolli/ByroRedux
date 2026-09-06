//! `GRAS` — ground-cover (grass) base record.
//!
//! EXAL ground cover Phase 5 (#3807, design
//! [`docs/engine/exal-groundcover.md`] §7). Until now `GRAS` dispatched
//! through the long-tail [`parse_minimal_esm_record`] stub — `EDID` and
//! `FULL` only — so every field describing the plant was discarded and the
//! record had no consumer at all.
//!
//! # What this record is *not* used for
//!
//! Read design §1 before reaching for [`GrasRecord::density`],
//! [`GrasRecord::min_slope`]/[`GrasRecord::max_slope`] or
//! [`GrasRecord::distance_from_water`] to "finish the port". ByroRedux
//! deliberately does **not** reproduce the Creation Engine's placement
//! model, in which a `GRAS` is keyed to an `LTEX` landscape texture and
//! scattered on a fixed per-cell grid wherever that texture is painted.
//! That scheme is *why* vanilla grass reads as patches — density steps hard
//! exactly where one splat layer stops — and the counter is a continuous
//! GPU density field with per-game data demoted to a palette hint.
//!
//! So the placement fields are decoded here (they are real authored data,
//! and a future consumer — a compatibility/diagnostic mode, an importer —
//! may want them) but the ground-cover translate boundary ignores them by
//! construction. Only the *dimension* fields feed a canonical species.
//!
//! # Sub-record layout
//!
//! Cross-referenced against OpenMW's `components/esm4/loadgras.cpp`
//! (itself derived from UESP's `Tes4Mod:Mod_File_Format`) and verified
//! against the four installed corpora — **168 records: Oblivion 108, FO3 9,
//! FNV 24, Skyrim SE 27** (census 2026-09-06).
//!
//! - `EDID` — editor ID (z-string). Present on all 168.
//! - `MODL` — model path (z-string). Present on all 168.
//! - `MODB` — bound radius (f32). **Oblivion only** (108/108; absent from
//!   every FO3/FNV/Skyrim record).
//! - `OBND` — object bounds, six i16. **FO3+ only** (60/60; absent from
//!   every Oblivion record). See [`ObjectBounds`].
//! - `MODT` — model texture hashes (skipped; Oblivion 99, Skyrim 27).
//! - `DATA` — the record proper, **exactly 32 bytes on all 168 records in
//!   all four games**. No version drift, so no era branch below.
//!
//! # `DATA` field order
//!
//! ```text
//! +0  u8   density                     +12 f32  position_range
//! +1  u8   min_slope                   +16 f32  height_range
//! +2  u8   max_slope                   +20 f32  colour_range
//! +3  u8   (padding)                   +24 f32  wave_period
//! +4  u16  distance_from_water         +28 u8   flags
//! +6  u16  (padding)                   +29 u8   (padding)
//! +8  u32  water_distance_application  +30 u16  (padding)
//! ```
//!
//! The four padding fields hold **uninitialised memory**, not zeroes —
//! OpenMW's "unused fields are probably packing" comment, confirmed by the
//! census: `+3` takes the values {0, 1, 2, 3, 61, 62} and `+30` takes
//! {0, 4, 128, 156, 401, 426} across the corpus. They are skipped, never
//! read, and must stay that way: a future field promoted out of one of
//! these offsets needs its own corpus check first.
//!
//! # Measured field ranges (all 168 records)
//!
//! | field | range | note |
//! |---|---|---|
//! | `density` | 1–100 | ignored by design §1 |
//! | `min_slope` | 0 | **always** zero, all 168 |
//! | `max_slope` | 28–90 | degrees; ignored, §3 has its own slope gate |
//! | `distance_from_water` | 0–390 | ignored |
//! | `water_distance_application` | 0–3 | of the 1–8 enum OpenMW documents |
//! | `position_range` | 7–90 | placement jitter, ignored |
//! | `height_range` | 0.0–0.85 | **fraction**, not units — the size signal |
//! | `colour_range` | 0.0–0.5 | fraction |
//! | `wave_period` | 0.0001–600 | scale differs per game — see below |
//! | `flags` | 0x00–0x06 | 147 of 168 are `0x06` |
//!
//! `wave_period`'s scale is **not** comparable across games (Oblivion
//! 0.0001–30 median 15; FO3/FNV 10–100 median 27; Skyrim 50–600 median
//! 240), and it does not correlate with plant height in any corpus
//! (Spearman +0.11 on Oblivion's n=99, −0.05 on Skyrim's n=21). It is
//! decoded and exposed, but nothing derives a canonical value from it —
//! doing so would be inventing a heuristic the data does not support.

use super::common::{find_sub, read_f32_sub, CommonNamedFields};
use super::tree::ObjectBounds;
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// Wire size of a `GRAS` `DATA` payload. Exactly this on all 168 vanilla
/// records across Oblivion / FO3 / FNV / Skyrim SE — a payload of any
/// other length is a mod-authored or corrupt record and is rejected whole
/// rather than partially decoded from a guessed offset.
pub const GRAS_DATA_LEN: usize = 32;

/// `DATA.flags` bit 0x01 — vertex lighting.
pub const GRAS_FLAG_VERTEX_LIGHTING: u8 = 0x01;
/// `DATA.flags` bit 0x02 — uniform scaling.
pub const GRAS_FLAG_UNIFORM_SCALING: u8 = 0x02;
/// `DATA.flags` bit 0x04 — fit to slope.
pub const GRAS_FLAG_FIT_TO_SLOPE: u8 = 0x04;

/// A parsed `GRAS` base record.
///
/// Every field defaults to its zero value when the corresponding
/// sub-record is absent or malformed; [`Self::has_data`] distinguishes
/// "record carried no usable `DATA`" from "record authored every field as
/// zero", which a consumer needs before trusting a zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GrasRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — path to the plant model, relative to `Data\Meshes\`.
    pub model_path: String,
    /// `OBND` — object bounds (FO3+). `None` on every Oblivion record.
    /// May be present but all-zero; see [`ObjectBounds::is_unset`].
    pub bounds: Option<ObjectBounds>,
    /// `MODB` — bounding-sphere radius (Oblivion). `0.0` on FO3+, and also
    /// on the 9 of Oblivion's 108 records that ship an un-computed `0.0`.
    pub bound_radius: f32,
    /// `DATA` — instances per cell quadrant. **Ignored by the ground-cover
    /// translate boundary** (design §1); see the module doc.
    pub density: u8,
    /// `DATA` — minimum ground slope, degrees. Always `0` in vanilla data.
    pub min_slope: u8,
    /// `DATA` — maximum ground slope, degrees. Ignored: design §3 derives
    /// its own slope gate from the terrain normal.
    pub max_slope: u8,
    /// `DATA` — units from water at which [`Self::water_distance_application`]
    /// applies. Ignored.
    pub distance_from_water: u16,
    /// `DATA` — how [`Self::distance_from_water`] is applied. The authored
    /// enum runs 1–8 (`Above/Below/Either` × `At Least/At Most`); vanilla
    /// data only ever uses 0–3. Ignored.
    pub water_distance_application: u32,
    /// `DATA` — per-instance placement jitter, units. Ignored.
    pub position_range: f32,
    /// `DATA` — per-instance height variation as a **fraction** of the
    /// model's own height, `0.0`–`0.85` in vanilla data. This is the one
    /// `DATA` field the translate boundary reads: with the model's
    /// dimensions it gives the canonical species size range.
    pub height_range: f32,
    /// `DATA` — per-instance colour variation, a fraction. Decoded but not
    /// consumed: the canonical species carries a base→tip gradient, not a
    /// jitter amount.
    pub colour_range: f32,
    /// `DATA` — sway animation period. Units are **not** comparable across
    /// games; see the module doc. Decoded, not consumed.
    pub wave_period: f32,
    /// `DATA` — `GRAS_FLAG_*` bits.
    pub flags: u8,
    /// True when a well-formed 32-byte `DATA` was decoded. False means
    /// every `DATA`-derived field above is a default, not authored data.
    pub has_data: bool,
}

impl GrasRecord {
    /// The plant's nominal height in record units, or `None` when the
    /// record carries no usable dimensions.
    ///
    /// This is the per-game seam, and it is deliberately *here* rather than
    /// at the translate boundary: Oblivion sizes a `GRAS` with a `MODB`
    /// bounding-sphere radius and FO3+ with an `OBND` box, so a consumer
    /// that read the two fields itself would need a game branch — exactly
    /// what the format-translation doctrine keeps out of everything
    /// downstream of the parser.
    ///
    /// - `OBND` present and not [`ObjectBounds::is_unset`] → the z extent,
    ///   which is the model's true height.
    /// - else `MODB` non-zero → the bounding-sphere radius. A sphere
    ///   enclosing the whole clump has `radius >= height/2`, so this
    ///   over-estimates a wide low clump; it is the only size signal
    ///   Oblivion's records carry.
    /// - else `None` — 9 of Oblivion's 108 records (`MODB` 0.0), 15 of
    ///   FNV's 24 and 6 of Skyrim's 27 (all-zero `OBND`).
    ///
    /// Never returns a non-positive or non-finite value.
    pub fn nominal_height(&self) -> Option<f32> {
        [
            self.bounds
                .filter(|b| !b.is_unset())
                .map(|b| b.extent(2) as f32),
            Some(self.bound_radius),
        ]
        .into_iter()
        .flatten()
        .find(|h| h.is_finite() && *h > 0.0)
    }

    /// True when the record sets the "fit to slope" flag.
    pub fn fits_to_slope(&self) -> bool {
        self.flags & GRAS_FLAG_FIT_TO_SLOPE != 0
    }

    /// True when the record sets the "uniform scaling" flag.
    pub fn scales_uniformly(&self) -> bool {
        self.flags & GRAS_FLAG_UNIFORM_SCALING != 0
    }
}

/// Parse a `GRAS` record from its sub-record list.
///
/// Unknown sub-records (`MODT` model hashes) are ignored, matching every
/// other parser in this module. A `DATA` of any length other than
/// [`GRAS_DATA_LEN`] leaves [`GrasRecord::has_data`] false and every
/// `DATA`-derived field at its default — a short payload cannot be
/// partially decoded without guessing which fields survived.
pub fn parse_gras(form_id: u32, subs: &[SubRecord]) -> GrasRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut out = GrasRecord {
        form_id,
        editor_id: common.editor_id,
        model_path: common.model_path,
        bounds: ObjectBounds::from_subs(subs),
        bound_radius: read_f32_sub(subs, b"MODB").unwrap_or(0.0),
        ..Default::default()
    };

    let Some(data) = find_sub(subs, b"DATA").filter(|d| d.len() == GRAS_DATA_LEN) else {
        return out;
    };
    let mut r = SubReader::new(data);
    out.density = r.u8_or_default();
    out.min_slope = r.u8_or_default();
    out.max_slope = r.u8_or_default();
    r.skip_or_eof(1); // +3 padding — uninitialised, never read
    out.distance_from_water = r.u16_or_default();
    r.skip_or_eof(2); // +6 padding
    out.water_distance_application = r.u32_or_default();
    out.position_range = r.f32_or_default();
    out.height_range = r.f32_or_default();
    out.colour_range = r.f32_or_default();
    out.wave_period = r.f32_or_default();
    out.flags = r.u8_or_default();
    // +29 / +30 padding intentionally not read.
    out.has_data = true;
    out
}

#[cfg(test)]
#[path = "gras_tests.rs"]
mod tests;
