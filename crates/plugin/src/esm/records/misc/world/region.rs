//! `REGN` regions and `ECZN` encounter zones.


use byroredux_core::bytes::le_u32_at;
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use super::super::super::common::{read_lstring_or_zstring, read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;
use std::collections::HashMap;
use super::rows;

/// What a [`RegionDataEntry`] configures. The discriminants are the raw
/// `RDAT` type word.
///
/// Derived by correlating every `RDAT` type against the sub-records that
/// follow it across `Oblivion.esm` (211 regions), `FalloutNV.esm` (276) and
/// `Skyrim.esm` (317) — see [`parse_regn`] for the full table. Not every game
/// authors every type: `Grass` appears once in Oblivion and nowhere else,
/// `Imposter` is FNV-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionDataKind {
    /// `RDOT` — scattered objects (rocks, flora) placed procedurally.
    Objects,
    /// `RDWT` — the region's weather table.
    Weather,
    /// `RDMP` — map name shown for this region.
    Map,
    /// `ICON` — landscape texture association.
    Landscape,
    /// `RDGS` — grass placement.
    Grass,
    /// Ambient audio: `RDSD`/`RDSA` sound list plus `RDMD`/`RDMO`/`RDSB`
    /// music or blanket-sound form.
    Sound,
    /// `RDID` — imposters (FNV).
    Imposter,
    /// A type this parser does not model. Carries the raw word so an
    /// unexpected value is reported rather than silently dropped.
    Unknown(u32),
}

impl RegionDataKind {
    fn from_word(word: u32) -> Self {
        match word {
            2 => Self::Objects,
            3 => Self::Weather,
            4 => Self::Map,
            5 => Self::Landscape,
            6 => Self::Grass,
            7 => Self::Sound,
            8 => Self::Imposter,
            other => Self::Unknown(other),
        }
    }
}

/// One weather-table row: a `WTHR` form and its selection chance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionWeather {
    pub weather_form: u32,
    /// Percent chance, 0–100. Observed values are 40 / 50 / 85 / 100.
    pub chance: u32,
    /// `GLOB` form gating this row. `None` on Oblivion, whose 8-byte row has
    /// no such field.
    pub global_form: Option<u32>,
}

/// One ambient-sound row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionSound {
    pub sound_form: u32,
    /// Playback flags (pleasant / cloudy / rainy / snowy bits).
    pub flags: u32,
    /// Raw chance word. Deliberately **not** rescaled: every observed value is
    /// a multiple of 10,000 (200000, 400000, 700000, 2000000, 2500000), which
    /// is consistent with a fixed-point percentage but the divisor is not
    /// established by the data. Left raw rather than guessed at; a consumer
    /// that needs a probability should pin the scale against the game first.
    pub chance_raw: u32,
}

/// One `RDAT` entry: what it configures, how it ranks, and its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionDataEntry {
    pub kind: RegionDataKind,
    /// `RDAT` flags byte. Only bit 0 is ever set in shipped data.
    pub flags: u8,
    /// `RDAT` priority byte, 0–100 (50 is overwhelmingly the default).
    ///
    /// This is the ordering EX-16 requires to be deterministic: it is
    /// *authored*, not invented by the engine, so a consumer must sort by it
    /// rather than by record order.
    pub priority: u8,
    pub payload: RegionDataPayload,
}

/// Type-specific payload of a [`RegionDataEntry`].
#[derive(Debug, Clone, PartialEq)]
pub enum RegionDataPayload {
    /// `RDOT` object FormIDs. The 52-byte row carries density/clustering
    /// fields beyond the form; only the form is decoded, since the remaining
    /// layout is not established by the corpus.
    Objects(Vec<u32>),
    Weather(Vec<RegionWeather>),
    /// `RDMP` map name (`wbLString`): inline text on Oblivion / FO3 / FNV, a `.STRINGS` id on a localized
    /// plugin. Either way it is resolved at parse time through `read_lstring_or_zstring`, because the
    /// per-plugin string table is gone once the walk ends and the merged index could never resolve a stored id
    /// (#6799). An id the table lacks surfaces as the `<lstring 0x…>` placeholder.
    Map(String),
    /// `ICON` landscape-texture path.
    Landscape(String),
    /// `RDGS` raw payload — a single 32-byte sample exists (one Oblivion
    /// region), too little to establish a row layout.
    Grass(Vec<u8>),
    Sound {
        /// `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB` (FNV) — the raw
        /// 4-byte value, decoded generically as a `u32` since its meaning
        /// is per-era and none of the three targets `SOUN`.
        ///
        /// **None of these three eras store a `SOUN` FormID here** — the
        /// consumer-side assumption that this field always names a `SOUN`
        /// was never true for any of them:
        ///
        /// - **Oblivion `RDMD`**: NOT a FormID at all — confirmed against
        ///   the xEdit `wbDefinitionsTES4.pas` record definition as a
        ///   `uint32` **enum** (`0 = Default`, `1 = Public`, `2 = Dungeon`),
        ///   a coarse music-category selector. Oblivion's `REGN` has no
        ///   `MUSC` (or equivalent) record at all to point at. FNV's record
        ///   *definition* inherits this field (`fopdoc`'s `FalloutNV/Records/
        ///   REGN.md`), but its shipped data never uses it: an ESM census
        ///   of all 276 `FalloutNV.esm` regions (2026-09-06, #3915) finds
        ///   `RDMD` ×0 — FNV authors `RDSB` ×44 / `RDSI` ×11 exclusively
        ///   (`Fallout3.esm` ships two `RDMD`). The near-universal `0`
        ///   values are an Oblivion measurement (#3811).
        /// - **Skyrim `RDMO`**: a genuine FormID — confirmed against
        ///   `wbDefinitionsTES5.pas`'s `wbFormIDCk(RDMO, 'Music', [MUSC], …)`
        ///   as pointing at a `MUSC` ("Music Type") record, not `SOUN`.
        ///   `MUSC` itself carries no direct audio path — it's a container
        ///   (playback flags + priority/ducking + fade duration + an array
        ///   of `MUST` "Music Track" FormIDs), and the actual file path +
        ///   loop-point data live one level further down, in `MUST`. Both
        ///   are decoded (`misc/music.rs`, `03f9d2158`, #3816 piece 1) and
        ///   `first_playable_track` resolves this field through
        ///   `MUSC` → first playable `MUST`; the "Plays One
        ///   Selection"/"Cycle Tracks" selection policy is not applied.
        /// - **FNV `RDSB`**: `MSET` (Media Set) — census-confirmed 44/44
        ///   (#3787), not `SOUN`. `MSET` is decoded into
        ///   `EsmIndex::media_sets` (`media_set.rs`).
        ///
        /// The music controller (`byroredux/src/music/translate.rs`,
        /// `docs/engine/music.md`) reads Oblivion's `RDMD` as an exterior's
        /// music category and FNV's `RDSB` as the region's Battle set;
        /// Fallout 3 / New Vegas location music reads no region (#6900).
        /// Skyrim's `RDMO` music waits on xWMA playback (#3816). It replaced
        /// the 2026-08-23 *dispatch_region_ambient_music* path, which looked
        /// every value up as a `SounRecord` (#6872). Census: #3787 (FNV) /
        /// #3811 (Oblivion + Skyrim, this doc), both closed doc-only.
        music: Option<u32>,
        /// Oblivion / FO3 `RDMD`, kept apart from [`Self::Sound::music`] because it is NOT a FormID: a raw
        /// `uint32` music-category enum (`0` Default, `1` Public, `2` Dungeon; xEdit
        /// `wbDefinitionsTES4.pas:4668`, `wbDefinitionsFNV.pas:7861`). Never load-order remapped (#6787); no
        /// consumer reads it yet (#3816).
        music_type: Option<u32>,
        /// `RDSI` incidental sound form (FNV). Same caveat as `music`
        /// above: census-confirmed 10 of 11 `RDSI` targets are `MSET`, not
        /// `SOUN` (#3787) — all ten an `Incidental Set`
        /// ([`MediaSetRecord::incidental`](super::MediaSetRecord::incidental));
        /// the eleventh dangles.
        incidental: Option<u32>,
        sounds: Vec<RegionSound>,
    },
    /// `RDID` imposter FormIDs (FNV).
    Imposters(Vec<u32>),
    /// The entry declared a type but authored no following payload.
    Empty,
}

/// A closed polygon bounding part of a region, with its edge falloff.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionArea {
    /// `RPLI` — edge fall-off distance in world units.
    pub edge_fall_off: u32,
    /// `RPLD` — polygon vertices in world XY. Stride is 8 bytes
    /// (two `f32`), confirmed across all three corpora.
    pub points: Vec<(f32, f32)>,
}

/// Overlap smaller than this (world units) does not make a region cover a cell.
const REGION_EDGE_EPSILON: f32 = 0.5;

impl RegionArea {
    /// Even-odd point-in-polygon test against the area's `RPLD` vertices (world XY).
    pub fn contains_point(&self, point: (f32, f32)) -> bool {
        let poly = &self.points;
        let mut inside = false;
        let mut j = poly.len().wrapping_sub(1);
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > point.1) != (yj > point.1) && point.0 < (xj - xi) * (point.1 - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Whether the polygon overlaps the axis-aligned rectangle `[x0, x1] x [y0, y1]`: a vertex
    /// inside it, a rectangle corner inside the polygon, or an edge crossing a rectangle edge.
    pub fn overlaps_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
        let poly = &self.points;
        if poly.len() < 3 {
            return false;
        }
        if poly.iter().any(|&(x, y)| x >= x0 && x <= x1 && y >= y0 && y <= y1) {
            return true;
        }
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        if corners.iter().any(|&c| self.contains_point(c)) {
            return true;
        }
        fn orient(p: (f32, f32), q: (f32, f32), r: (f32, f32)) -> f32 {
            (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
        }
        (0..poly.len()).any(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            (0..4).any(|k| {
                let (c, d) = (corners[k], corners[(k + 1) % 4]);
                (orient(a, b, c) > 0.0) != (orient(a, b, d) > 0.0)
                    && (orient(c, d, a) > 0.0) != (orient(c, d, b) > 0.0)
            })
        })
    }
}

/// Region record (`REGN`). Tags an area of a worldspace (`WNAM`) with a
/// colour tint, one or more bounding polygons, and a priority-ordered chain
/// of `RDAT` data entries driving ambient sound, weather, objects and map
/// name. The region's weather itself is a `RDAT` Weather entry, not WNAM.
#[derive(Debug, Clone, Default)]
pub struct RegnRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `WNAM` — the WRLD this region belongs to (xEdit TES4/FO3/FNV/TES5:
    /// `wbFormIDCkNoReach(WNAM, 'Worldspace', [WRLD])`; 210/210 vanilla
    /// Oblivion REGNs point at a WRLD). #5422/#5421 renamed this from
    /// `weather_form` — the region's weather is a `RDAT` Weather entry,
    /// never WNAM. Routed through `remap_fid` by the #3401 post-pass.
    pub worldspace_form: Option<u32>,
    /// `CNAM` — climate form. **xEdit defines no REGN `CNAM`** (TES4/FO3/
    /// FNV/TES5), and the vanilla census (#5421) found **zero** authored
    /// occurrences: 0 on Oblivion's 211 REGNs (SI merged), 0 on FNV.
    /// The pre-#5421 doc claimed Cyrodiil's climate reached cells through
    /// CELL `XCLR` → REGN `CNAM` → CLMT — refuted by a whole-file scan (a
    /// CLMT FormID appears only in CELL `XCCM`, WRLD `CNAM`, SCPT, PGRD
    /// and LAND; no REGN references a climate). Kept only for
    /// mod-authored data, decoded through `remap_fid` like every other
    /// cross-record form here.
    pub climate_form: Option<u32>,
    /// `RCLR` — RGB region tint for map shading. Stored as raw u8[3];
    /// alpha byte (if any) is ignored.
    pub color: Option<[u8; 3]>,
    /// `RPLI`/`RPLD` bounding polygons. A region may declare several.
    pub areas: Vec<RegionArea>,
    /// `RDAT` chain in authored order. Sort by
    /// [`priority`](RegionDataEntry::priority) for evaluation.
    pub entries: Vec<RegionDataEntry>,
}

impl RegnRecord {
    /// Whether one of the region's `RPLD` polygons overlaps exterior cell `(grid_x, grid_y)`.
    ///
    /// This is the region's area of effect: the CK page says "it is the exact shape of the region
    /// as drawn in the Region Preview Window that will decide its actual area of effect", and the
    /// per-cell `XCLR` list is a stale snapshot of it (Volumetric Lighting PRD VLM-REG-008; the
    /// 2026-11-07 census found it wrong in both directions on every title). The caller restricts
    /// regions to the cell's worldspace (`worldspace_form`); coordinates alone cannot.
    pub fn covers_cell(&self, grid_x: i32, grid_y: i32) -> bool {
        // The rectangle is inset by half a unit so a polygon that only shares a cell edge (the
        // usual case: regions are drawn on the cell grid) does not claim the neighbour.
        let (x0, y0) = (
            grid_x as f32 * EXTERIOR_CELL_UNITS + REGION_EDGE_EPSILON,
            grid_y as f32 * EXTERIOR_CELL_UNITS + REGION_EDGE_EPSILON,
        );
        let side = EXTERIOR_CELL_UNITS - 2.0 * REGION_EDGE_EPSILON;
        self.areas
            .iter()
            .any(|area| area.overlaps_rect(x0, y0, x0 + side, y0 + side))
    }

    /// Entries of one kind, highest priority first.
    ///
    /// Ties keep authored order (the sort is stable), which is the only
    /// defensible tie-break: nothing in the record distinguishes two entries
    /// at equal priority.
    pub fn entries_by_priority(&self, kind: RegionDataKind) -> Vec<&RegionDataEntry> {
        let mut matching: Vec<&RegionDataEntry> =
            self.entries.iter().filter(|e| e.kind == kind).collect();
        matching.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
        matching
    }
}

/// Resolve the highest-priority `RDAT` `Sound` entry across every region
/// tagging one resident cell (`CellData::regions`, XCLR) — EX-16 item 1's
/// "REGN runtime consumption" (#2372).
///
/// A cell can be tagged by several overlapping `REGN` polygons at once;
/// `RDAT`'s authored `priority` byte is the only cross-region ordering
/// signal that exists (there is no "region A outranks region B" field).
/// Ties keep authoring order — `region_form_ids` order first, then
/// within-region entry order — the same tie-break
/// [`RegnRecord::entries_by_priority`] uses, generalised across more than
/// one region.
///
/// A FormID absent from `regions` (bad data, or a REGN this parser never
/// saw) is skipped rather than treated as an error — mirrors every other
/// "resolve a FormID, tolerate a miss" site in this crate.
pub fn select_active_region_sound<'a>(
    region_form_ids: &[u32],
    regions: &'a HashMap<u32, RegnRecord>,
) -> Option<&'a RegionDataEntry> {
    let mut candidates: Vec<&RegionDataEntry> = region_form_ids
        .iter()
        .filter_map(|id| regions.get(id))
        .flat_map(|r| r.entries.iter())
        .filter(|e| e.kind == RegionDataKind::Sound)
        .collect();
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
    candidates.into_iter().next()
}

/// Parse a region (`REGN`), including the `RDAT` data-entry chain (#2737).
///
/// # Format, as established from shipped data
///
/// `RDAT` is always 8 bytes: `type: u32, flags: u8, priority: u8, unused: u16`.
/// The trailing half-word is zero in every one of the 788 entries across
/// `Oblivion.esm`, `FalloutNV.esm` and `Skyrim.esm`; `flags` is only ever 0 or
/// 1; `priority` ranges 0–100 with 50 overwhelmingly dominant.
///
/// Each `RDAT` opens a section, and the sub-records *following* it until the
/// next `RDAT` are its payload. The type → payload mapping was derived by
/// tabulating which signatures follow which type word:
///
/// | type | payload | Oblivion | FNV | Skyrim |
/// |---|---|---|---|---|
/// | 2 | `RDOT` objects | 71 | 19 | 69 (all empty) |
/// | 3 | `RDWT` weather | 57 | 31 | 53 |
/// | 4 | `RDMP` map name | 113 | 55 | 7 |
/// | 5 | `ICON` landscape | 1 | 9 | 1 |
/// | 6 | `RDGS` grass | 1 | — | — |
/// | 7 | sound | `RDMD`+`RDSD` | `RDSB`+`RDSD`+`RDSI` | `RDMO`+`RDSA` |
/// | 8 | `RDID` imposters | — | 18 | — |
///
/// Row strides were established the same way, by taking the GCD of every
/// observed payload length and confirming the smallest observed length equals
/// it. **`RDWT` is the one genuine per-game divergence**: Oblivion's row is 8
/// bytes (`weather`, `chance`) while FO3/FNV/Skyrim's is 12 (`weather`,
/// `chance`, `global`). Both are handled by measuring the payload rather than
/// branching on a game enum — a payload divisible by 12 but not 8 can only be
/// the long form, and the short form is assumed otherwise.
pub fn parse_regn(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> RegnRecord {
    let mut out = RegnRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;

    // Section state: the RDAT currently open, and the area currently being
    // accumulated. Both flush when their next opener arrives or at the end.
    let mut open: Option<RegionDataEntry> = None;
    let mut area: Option<RegionArea> = None;

    for sub in subs {
        match &sub.sub_type {
            b"WNAM" if sub.data.len() >= 4 => {
                // The owning WRLD, not a weather (see the field doc).
                out.worldspace_form = SubReader::new(&sub.data).u32().ok();
            }
            // xEdit-undefined, zero vanilla occurrences (see
            // [`Self::climate_form`]); mod-authored only.
            // `remap_fid` like every other cross-record form on this record.
            b"CNAM" if sub.data.len() >= 4 => {
                out.climate_form = sub
                    .data
                    .get(0..4)
                    .map(|bytes| remap_fid(u32::from_le_bytes(bytes.try_into().unwrap()), remap));
            }
            b"RCLR" if sub.data.len() >= 3 => {
                out.color = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            b"RPLI" => {
                if let Some(previous) = area.take() {
                    out.areas.push(previous);
                }
                area = Some(RegionArea {
                    edge_fall_off: le_u32_at(&sub.data, 0).unwrap_or(0),
                    points: Vec::new(),
                });
            }
            b"RPLD" => {
                // Stride 8 = two f32. An RPLD without a preceding RPLI still
                // describes a real polygon, so synthesise a zero-falloff area
                // rather than dropping the geometry.
                let target = area.get_or_insert_with(RegionArea::default);
                for row in rows(&sub.data, 8) {
                    let x = f32::from_le_bytes([row[0], row[1], row[2], row[3]]);
                    let y = f32::from_le_bytes([row[4], row[5], row[6], row[7]]);
                    target.points.push((x, y));
                }
            }
            b"RDAT" if sub.data.len() >= 8 => {
                if let Some(previous) = open.take() {
                    out.entries.push(previous);
                }
                open = Some(RegionDataEntry {
                    kind: RegionDataKind::from_word(le_u32_at(&sub.data, 0).unwrap_or(0)),
                    flags: sub.data[4],
                    priority: sub.data[5],
                    payload: RegionDataPayload::Empty,
                });
            }
            other => {
                let Some(entry) = open.as_mut() else { continue };
                apply_region_payload(entry, other, &sub.data);
            }
        }
    }
    if let Some(previous) = open.take() {
        out.entries.push(previous);
    }
    if let Some(previous) = area.take() {
        out.areas.push(previous);
    }
    // #3401 — one post-pass over every FormID-bearing field, for the same
    // reason `parse_navm` uses one: the `RDAT` payloads are built across
    // several sub-records by `apply_region_payload`, so remapping at the
    // decode sites would mean touching every arm of a growing match.
    out.worldspace_form = out.worldspace_form.map(|f| remap_fid(f, remap));
    for entry in &mut out.entries {
        match &mut entry.payload {
            RegionDataPayload::Objects(forms) | RegionDataPayload::Imposters(forms) => {
                for f in forms {
                    *f = remap_fid(*f, remap);
                }
            }
            RegionDataPayload::Weather(rows) => {
                for row in rows {
                    row.weather_form = remap_fid(row.weather_form, remap);
                    row.global_form = row.global_form.map(|f| remap_fid(f, remap));
                }
            }
            RegionDataPayload::Sound {
                music,
                incidental,
                sounds,
                // `RDMD` is a raw enum, not a FormID: it stays unremapped (#6787).
                music_type: _,
            } => {
                *music = music.map(|f| remap_fid(f, remap));
                *incidental = incidental.map(|f| remap_fid(f, remap));
                for snd in sounds {
                    snd.sound_form = remap_fid(snd.sound_form, remap);
                }
            }
            RegionDataPayload::Map(_)
            | RegionDataPayload::Landscape(_)
            | RegionDataPayload::Grass(_)
            | RegionDataPayload::Empty => {}
        }
    }
    out
}

/// Decode an `RDWT` weather table, choosing the row stride by validation.
///
/// Oblivion's row is 8 bytes (`weather`, `chance`); FO3/FNV/Skyrim's is 12
/// (`weather`, `chance`, `global`). Lengths divisible by both — 24, 48 — are
/// genuinely ambiguous, and Oblivion authors plenty of them, so a
/// divisibility tie-break silently mis-splits real data. (It did: an early
/// version preferring 12 produced 65 Oblivion rows with `chance > 100`.)
///
/// `chance` is a percentage, so it is its own checksum. Decode with each
/// candidate stride and keep the one whose chances are all in range; prefer
/// the 12-byte form only when it validates and the 8-byte form does not, so
/// the common unambiguous cases stay exact.
fn decode_weather_rows(data: &[u8]) -> Vec<RegionWeather> {
    fn decode(data: &[u8], stride: usize, exact: bool) -> Option<Vec<RegionWeather>> {
        if exact && !data.len().is_multiple_of(stride) {
            return None;
        }
        let out: Vec<RegionWeather> = rows(data, stride)
            .filter_map(|r| {
                Some(RegionWeather {
                    weather_form: le_u32_at(r, 0)?,
                    chance: le_u32_at(r, 4)?,
                    global_form: if stride >= 12 { le_u32_at(r, 8) } else { None },
                })
            })
            .collect();
        // An empty result must not count as "validated" — `all()` is
        // vacuously true on an empty iterator, which would let a stride that
        // yielded nothing win over one that yielded good rows.
        (!out.is_empty() && out.iter().all(|w| w.chance <= 100)).then_some(out)
    }
    // Exact divisibility first, and 8 before 12. Both orderings matter:
    //
    // - Exact-first is what stops a 12-byte long row being read as a single
    //   8-byte row. Its `chance` sits at the same offset in both readings, so
    //   validation alone cannot tell them apart — only the length can.
    // - 8-before-12 resolves the genuinely ambiguous lengths (24, 48) in
    //   Oblivion's favour, and validation catches the case where that is
    //   wrong: reading FNV's 2x12 payload as 3x8 puts a FormID in the second
    //   row's `chance`, which fails the <=100 check and falls through to 12.
    //
    // The lenient pass is a last resort for a ragged payload, so one
    // malformed plugin loses a row rather than the whole table.
    decode(data, 8, true)
        .or_else(|| decode(data, 12, true))
        .or_else(|| decode(data, 8, false))
        .or_else(|| decode(data, 12, false))
        .unwrap_or_default()
}

/// Fold one payload sub-record into the currently-open `RDAT` section.
fn apply_region_payload(entry: &mut RegionDataEntry, sig: &[u8], data: &[u8]) {
    match sig {
        b"RDOT" => {
            let forms: Vec<u32> = rows(data, 52).filter_map(|r| le_u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Objects(forms);
        }
        b"RDWT" => {
            entry.payload = RegionDataPayload::Weather(decode_weather_rows(data));
        }
        b"RDMP" => {
            // A localized plugin authors RDMP as a 4-byte `.STRINGS` id, the earlier games as a
            // zero-terminated string. `read_lstring_or_zstring` keys on the TES4 localized flag (not on
            // "four bytes"), so a 3-character inline name is never mistaken for an id (#6799).
            entry.payload = RegionDataPayload::Map(read_lstring_or_zstring(data));
        }
        b"ICON" => entry.payload = RegionDataPayload::Landscape(read_zstring(data)),
        b"RDGS" => entry.payload = RegionDataPayload::Grass(data.to_vec()),
        b"RDID" => {
            let forms: Vec<u32> = rows(data, 4).filter_map(|r| le_u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Imposters(forms);
        }
        // Sound sections mix a single music/blanket form with a variable
        // sound list, and the signatures differ per game, so the payload is
        // built up across several sub-records rather than replaced.
        b"RDSD" | b"RDSA" => {
            let list: Vec<RegionSound> = rows(data, 12)
                .filter_map(|r| {
                    Some(RegionSound {
                        sound_form: le_u32_at(r, 0)?,
                        flags: le_u32_at(r, 4)?,
                        chance_raw: le_u32_at(r, 8)?,
                    })
                })
                .collect();
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { sounds, .. } = p {
                    *sounds = list;
                }
            });
        }
        b"RDMD" => {
            let value = le_u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { music_type, .. } = p {
                    *music_type = value;
                }
            });
        }
        b"RDMO" | b"RDSB" => {
            let value = le_u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { music, .. } = p {
                    *music = value;
                }
            });
        }
        b"RDSI" => {
            let value = le_u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { incidental, .. } = p {
                    *incidental = value;
                }
            });
        }
        _ => {}
    }
}

/// Ensure `entry` holds a `Sound` payload, then apply `f` to it.
fn merge_sound(entry: &mut RegionDataEntry, f: impl FnOnce(&mut RegionDataPayload)) {
    if !matches!(entry.payload, RegionDataPayload::Sound { .. }) {
        entry.payload = RegionDataPayload::Sound {
            music: None,
            music_type: None,
            incidental: None,
            sounds: Vec::new(),
        };
    }
    f(&mut entry.payload);
}

/// Encounter zone (`ECZN`). Governs spawn scaling / faction ownership
/// on the cells that reference it via `XEZN`.
///
/// `DATA` layout by game (xEdit `wbDefinitions*` ECZN):
/// - Fallout 3 / New Vegas: `owner (u32 FormID) + rank (s8) + min level (s8)
///   + flags (u8) + unused (u8)`.
/// - Skyrim / Fallout 4 / Starfield: `owner (u32) + location (u32 LCTN) +
///   rank (s8) + min level (s8) + flags (u8) + max level (s8)`; Skyrim records
///   below form version 34 stop after the location.
#[derive(Debug, Clone, Default)]
pub struct EcznRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Form ID of the faction or actor that owns this zone. `0` when
    /// the field is unset (wilderness zones sometimes leave it blank).
    pub owner_form: u32,
    /// The zone's `LCTN` (Skyrim and later); `0` when unset or absent.
    pub location_form: u32,
    /// Faction rank required; 0 = no rank gate.
    pub rank: u8,
    /// CK *Encounter Zone* "Min Level": the lowest level the zone resolves to
    /// (a player below it is lifted to it, unless
    /// [`match_pc_below_minimum`](Self::match_pc_below_minimum)).
    pub min_level: u8,
    /// CK "Max Level": the highest level the zone resolves to; `0` = no cap.
    /// Absent (always 0) in Fallout 3 / New Vegas.
    pub max_level: u8,
    pub flags: u8,
}

impl EcznRecord {
    /// `Never Resets` (bit 0).
    pub const FLAG_NEVER_RESETS: u8 = 0x01;
    /// `Match PC Below Minimum Level` (bit 1): "the encounter zone will match
    /// the player's level if the player enters the encounter zone below the
    /// minimum level" (CK *Encounter Zone*).
    pub const FLAG_MATCH_PC_BELOW_MINIMUM: u8 = 0x02;

    pub fn match_pc_below_minimum(&self) -> bool {
        self.flags & Self::FLAG_MATCH_PC_BELOW_MINIMUM != 0
    }
}

pub fn parse_eczn(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> EcznRecord {
    let mut out = EcznRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    let has_location = !matches!(game, GameKind::Oblivion | GameKind::Fallout3NV);
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 7 => {
                let mut r = SubReader::new(&sub.data);
                // #3715 — embedded owner (faction/actor) FormID.
                out.owner_form = remap_fid(r.u32_or_default(), remap);
                if has_location {
                    out.location_form = remap_fid(r.u32_or_default(), remap);
                }
                out.rank = r.u8_or_default();
                out.min_level = r.u8_or_default();
                out.flags = r.u8_or_default();
                if has_location {
                    out.max_level = r.u8_or_default();
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod regn_tests {
    use super::*;
    
    use crate::esm::records::test_support::sub;

    /// `RDAT` header as established from shipped data: type u32, flags u8,
    /// priority u8, then a half-word that is zero in all 788 corpus entries.
    fn rdat(kind: u32, flags: u8, priority: u8) -> SubRecord {
        let mut d = kind.to_le_bytes().to_vec();
        d.push(flags);
        d.push(priority);
        d.extend_from_slice(&[0, 0]);
        sub(b"RDAT", &d)
    }

    #[test]
    fn rdat_header_fields_are_decoded() {
        let r = parse_regn(1, &[rdat(3, 1, 75)], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, RegionDataKind::Weather);
        assert_eq!(r.entries[0].flags, 1);
        assert_eq!(r.entries[0].priority, 75);
    }

    #[test]
    fn every_observed_type_word_maps_to_a_kind() {
        // The seven type words the corpus actually authors.
        let expected = [
            (2, RegionDataKind::Objects),
            (3, RegionDataKind::Weather),
            (4, RegionDataKind::Map),
            (5, RegionDataKind::Landscape),
            (6, RegionDataKind::Grass),
            (7, RegionDataKind::Sound),
            (8, RegionDataKind::Imposter),
        ];
        for (word, kind) in expected {
            assert_eq!(RegionDataKind::from_word(word), kind, "type {word}");
        }
        // An unmodelled type must surface its word rather than vanish.
        assert_eq!(RegionDataKind::from_word(99), RegionDataKind::Unknown(99));
    }

    #[test]
    fn payload_sub_records_attach_to_the_open_section() {
        // The chain is positional: sub-records belong to the most recent
        // RDAT. Getting this wrong silently reassigns every payload.
        let mut weather = 0x0017_3564u32.to_le_bytes().to_vec();
        weather.extend_from_slice(&85u32.to_le_bytes());
        weather.extend_from_slice(&0u32.to_le_bytes());
        let record = parse_regn(
            1,
            &[
                rdat(3, 0, 50),
                sub(b"RDWT", &weather),
                rdat(4, 0, 50),
                sub(b"RDMP", b"Mojave Wasteland\0"),
            ],
            &None,
        );
        assert_eq!(record.entries.len(), 2);
        assert!(matches!(
            record.entries[0].payload,
            RegionDataPayload::Weather(ref w) if w.len() == 1 && w[0].chance == 85
        ));
        assert!(matches!(
            record.entries[1].payload,
            RegionDataPayload::Map(ref m) if m == "Mojave Wasteland"
        ));
    }

    #[test]
    fn weather_row_stride_follows_the_payload_not_a_game_flag() {
        // Oblivion authors 8-byte rows, FO3/FNV/Skyrim 12-byte. Deciding from
        // the payload length keeps the parser free of a game branch, per the
        // format-abstraction doctrine.
        let mut short = 0xAAu32.to_le_bytes().to_vec();
        short.extend_from_slice(&40u32.to_le_bytes());
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &short)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].chance, 40);
                assert_eq!(w[0].global_form, None, "Oblivion rows carry no GLOB");
            }
            other => panic!("{other:?}"),
        }

        let mut long = 0xBBu32.to_le_bytes().to_vec();
        long.extend_from_slice(&100u32.to_le_bytes());
        long.extend_from_slice(&0x1234u32.to_le_bytes());
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].global_form, Some(0x1234));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sound_sections_accumulate_across_their_sub_records() {
        // A sound section is several sub-records that must merge, not
        // overwrite: FNV authors RDSB + RDSD + RDSI in one section.
        let mut sounds = 0x0014_1866u32.to_le_bytes().to_vec();
        sounds.extend_from_slice(&15u32.to_le_bytes());
        sounds.extend_from_slice(&200_000u32.to_le_bytes());
        let r = parse_regn(
            1,
            &[
                rdat(7, 0, 50),
                sub(b"RDSB", 0x99u32.to_le_bytes()),
                sub(b"RDSD", &sounds),
                sub(b"RDSI", 0x77u32.to_le_bytes()),
            ],
            &None,
        );
        match &r.entries[0].payload {
            RegionDataPayload::Sound {
                music,
                incidental,
                sounds,
                ..
            } => {
                assert_eq!(*music, Some(0x99));
                assert_eq!(*incidental, Some(0x77));
                assert_eq!(sounds.len(), 1);
                assert_eq!(sounds[0].sound_form, 0x0014_1866);
                assert_eq!(sounds[0].flags, 15);
                assert_eq!(sounds[0].chance_raw, 200_000);
            }
            other => panic!("{other:?}"),
        }
    }

    /// #6787 — Oblivion / FO3 `RDMD` is a raw music-category enum: it lands in `music_type` and a
    /// non-identity load-order remap leaves it alone, while `RDMO` / `RDSB` FormIDs are remapped.
    #[test]
    fn rdmd_enum_is_never_remapped_but_rdmo_is() {
        let remap = Some(crate::esm::reader::FormIdRemap::regular(2, vec![0]));
        let r = parse_regn(1, &[rdat(7, 0, 50), sub(b"RDMD", 1u32.to_le_bytes())], &remap);
        match &r.entries[0].payload {
            RegionDataPayload::Sound { music, music_type, .. } => {
                assert_eq!(*music_type, Some(1), "the enum stays raw under a slot remap");
                assert_eq!(*music, None, "an enum is not a music FormID");
            }
            other => panic!("{other:?}"),
        }
        let r = parse_regn(1, &[rdat(7, 0, 50), sub(b"RDMO", 0x0100_4242u32.to_le_bytes())], &remap);
        match &r.entries[0].payload {
            RegionDataPayload::Sound { music, music_type, .. } => {
                assert_eq!(*music, Some(0x0200_4242));
                assert_eq!(*music_type, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn skyrim_music_signature_merges_the_same_way() {
        let r = parse_regn(
            1,
            &[rdat(7, 0, 50), sub(b"RDMO", 0x4242u32.to_le_bytes())],
            &None,
        );
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Sound {
                music: Some(0x4242),
                ..
            }
        ));
    }

    fn square_region(x0: f32, y0: f32, x1: f32, y1: f32) -> RegnRecord {
        RegnRecord {
            areas: vec![RegionArea {
                edge_fall_off: 0,
                points: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            }],
            ..Default::default()
        }
    }

    /// VLM-REG-008: a region covers the cells its polygon overlaps, not only the ones whose
    /// centre it contains, and not the cells beside it.
    #[test]
    fn a_region_covers_the_cells_its_polygon_overlaps() {
        // Cells (1,1) and (2,1) fully, (3,1) by a 1-unit sliver, nothing of (0,1) or (1,3).
        let r = square_region(4096.0, 4096.0, 12289.0, 8192.0);
        assert!(r.covers_cell(1, 1));
        assert!(r.covers_cell(2, 1));
        assert!(r.covers_cell(3, 1), "a polygon edge inside the cell is an overlap");
        assert!(!r.covers_cell(0, 1));
        assert!(!r.covers_cell(1, 3));
        assert!(!r.covers_cell(1, 0), "sharing only the edge y = 4096 is not coverage");
    }

    /// A polygon much larger than the cell has no vertex inside it and no edge crossing it; only
    /// the cell-corner-in-polygon rule finds it.
    #[test]
    fn a_region_polygon_larger_than_the_cell_still_covers_it() {
        let r = square_region(-100_000.0, -100_000.0, 100_000.0, 100_000.0);
        assert!(r.covers_cell(7, -9));
    }

    #[test]
    fn a_concave_region_leaves_its_notch_uncovered() {
        // An L: the unit square (0..3, 0..3) minus the (1..3, 1..3) cells' block.
        let u = 4096.0;
        let r = RegnRecord {
            areas: vec![RegionArea {
                edge_fall_off: 0,
                points: vec![(0.0, 0.0), (3.0 * u, 0.0), (3.0 * u, u), (u, u), (u, 3.0 * u), (0.0, 3.0 * u)],
            }],
            ..Default::default()
        };
        assert!(r.covers_cell(0, 0));
        assert!(r.covers_cell(2, 0));
        assert!(r.covers_cell(0, 2));
        assert!(!r.covers_cell(2, 2), "the notch");
    }

    #[test]
    fn a_region_without_polygons_covers_nothing() {
        assert!(!RegnRecord::default().covers_cell(0, 0));
        let degenerate = RegnRecord {
            areas: vec![RegionArea { edge_fall_off: 0, points: vec![(0.0, 0.0), (9999.0, 9999.0)] }],
            ..Default::default()
        };
        assert!(!degenerate.covers_cell(0, 0), "two points enclose nothing");
    }

    #[test]
    fn areas_pair_edge_falloff_with_their_point_list() {
        let mut points = Vec::new();
        for (x, y) in [(0.0f32, 0.0f32), (4096.0, 0.0), (4096.0, 4096.0)] {
            points.extend_from_slice(&x.to_le_bytes());
            points.extend_from_slice(&y.to_le_bytes());
        }
        let r = parse_regn(
            1,
            &[
                sub(b"RPLI", 128u32.to_le_bytes()),
                sub(b"RPLD", &points),
                sub(b"RPLI", 64u32.to_le_bytes()),
                sub(b"RPLD", &points[..16]),
            ],
            &None,
        );
        assert_eq!(r.areas.len(), 2);
        assert_eq!(r.areas[0].edge_fall_off, 128);
        assert_eq!(r.areas[0].points.len(), 3);
        assert_eq!(r.areas[0].points[1], (4096.0, 0.0));
        assert_eq!(r.areas[1].edge_fall_off, 64);
        assert_eq!(r.areas[1].points.len(), 2);
    }

    #[test]
    fn a_point_list_without_edge_falloff_still_keeps_its_geometry() {
        // Dropping the polygon because RPLI was absent would silently shrink
        // the region, which is worse than a zero falloff.
        let mut points = 1.0f32.to_le_bytes().to_vec();
        points.extend_from_slice(&2.0f32.to_le_bytes());
        let r = parse_regn(1, &[sub(b"RPLD", &points)], &None);
        assert_eq!(r.areas.len(), 1);
        assert_eq!(r.areas[0].points, vec![(1.0, 2.0)]);
        assert_eq!(r.areas[0].edge_fall_off, 0);
    }

    #[test]
    fn entries_sort_by_authored_priority() {
        // EX-16 requires deterministic priority, and it is authored in RDAT
        // rather than derived from record order.
        let r = parse_regn(
            1,
            &[
                rdat(7, 0, 50),
                rdat(7, 0, 100),
                rdat(7, 0, 75),
                rdat(3, 0, 99),
            ],
            &None,
        );
        let sounds = r.entries_by_priority(RegionDataKind::Sound);
        assert_eq!(
            sounds.iter().map(|e| e.priority).collect::<Vec<_>>(),
            vec![100, 75, 50]
        );
        // Filtering is by kind — the higher-priority weather entry must not
        // leak into the sound query.
        assert_eq!(sounds.len(), 3);
    }

    #[test]
    fn equal_priorities_keep_authored_order() {
        // Nothing in the record distinguishes two entries at equal priority,
        // so a stable sort is the only defensible tie-break.
        let r = parse_regn(1, &[rdat(2, 0, 50), rdat(2, 1, 50), rdat(2, 0, 50)], &None);
        let objects = r.entries_by_priority(RegionDataKind::Objects);
        assert_eq!(
            objects.iter().map(|e| e.flags).collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
    }

    #[test]
    fn truncated_payloads_drop_the_short_row_not_the_region() {
        // One malformed plugin must not take the whole region with it.
        let mut ragged = 0xAAu32.to_le_bytes().to_vec();
        ragged.extend_from_slice(&40u32.to_le_bytes());
        ragged.extend_from_slice(&[0xFF]); // 9 bytes: one 8-byte row + junk
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &ragged)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => assert_eq!(w.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_rdat_with_no_payload_stays_empty_rather_than_absent() {
        // Skyrim authors 69 empty RDOT sections; they must still appear so a
        // consumer sees the declared-but-empty distinction.
        let r = parse_regn(1, &[rdat(2, 0, 50), sub(b"RDOT", [])], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, RegionDataKind::Objects);
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Objects(ref v) if v.is_empty()
        ));
    }

    #[test]
    fn payloads_before_any_rdat_are_ignored_not_misattributed() {
        let r = parse_regn(1, &[sub(b"RDMP", b"orphan\0"), rdat(4, 0, 50)], &None);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].payload, RegionDataPayload::Empty);
    }

    #[test]
    fn ambiguous_weather_lengths_resolve_by_validating_the_chance() {
        // 24 bytes is divisible by both strides. A divisibility tie-break got
        // this wrong and produced 65 Oblivion rows with chance > 100 in a
        // corpus sweep; `chance` is a percentage, so it is its own checksum.
        let mut oblivion = Vec::new();
        for (form, chance) in [(0xAAu32, 40u32), (0xBB, 50), (0xCC, 10)] {
            oblivion.extend_from_slice(&form.to_le_bytes());
            oblivion.extend_from_slice(&chance.to_le_bytes());
        }
        assert_eq!(oblivion.len(), 24);
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &oblivion)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 3, "24B of 8-byte rows must split into 3");
                assert!(w.iter().all(|x| x.chance <= 100));
                assert!(w.iter().all(|x| x.global_form.is_none()));
            }
            other => panic!("{other:?}"),
        }

        // The same length as two 12-byte rows: reading it as three 8-byte rows
        // puts a FormID in the second row's chance, which must fail validation
        // and fall through to the long form.
        let mut long = Vec::new();
        for (form, chance, global) in [(0xAAu32, 40u32, 0x9999u32), (0xBB, 50, 0x8888)] {
            long.extend_from_slice(&form.to_le_bytes());
            long.extend_from_slice(&chance.to_le_bytes());
            long.extend_from_slice(&global.to_le_bytes());
        }
        assert_eq!(long.len(), 24);
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)], &None);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => {
                assert_eq!(w.len(), 2);
                assert_eq!(w[0].global_form, Some(0x9999));
                assert_eq!(w[1].chance, 50);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn skyrim_localised_map_names_resolve_at_parse_time_not_text() {
        use crate::esm::records::common::{LocalizedPluginGuard, StringsTableGuard};
        use crate::esm::strings_table::{StringTableSet, StringsTable};
        // Skyrim's RDMP is a 4-byte .STRINGS id; reading it as inline text
        // yields control characters. It resolves through the plugin's table
        // while that table is installed (#6799).
        let text = b"Whiterun Hold\0";
        let mut table = Vec::new();
        for word in [1u32, text.len() as u32, 0x0001_1086, 0] {
            table.extend_from_slice(&word.to_le_bytes());
        }
        table.extend_from_slice(text);
        let _localized = LocalizedPluginGuard::new(true);
        let _strings = StringsTableGuard::new(StringTableSet {
            strings: Some(StringsTable::parse(&table, false).unwrap()),
            ..Default::default()
        });
        let r = parse_regn(
            1,
            &[rdat(4, 0, 50), sub(b"RDMP", 0x0001_1086u32.to_le_bytes())],
            &None,
        );
        assert_eq!(r.entries[0].payload, RegionDataPayload::Map("Whiterun Hold".to_string()));
        drop(_strings);
        drop(_localized);

        // A non-localized plugin's three-character name is text, never an id.
        let r = parse_regn(1, &[rdat(4, 0, 50), sub(b"RDMP", b"Zoo\0")], &None);
        assert_eq!(r.entries[0].payload, RegionDataPayload::Map("Zoo".to_string()));

        // …while the inline form still parses as text.
        let r = parse_regn(1, &[rdat(4, 0, 50), sub(b"RDMP", b"Arefu\0")], &None);
        assert_eq!(
            r.entries[0].payload,
            RegionDataPayload::Map("Arefu".to_string())
        );
    }

    #[test]
    fn existing_fields_still_parse() {
        let r = parse_regn(
            7,
            &[
                sub(b"EDID", b"MojaveRegion\0"),
                sub(b"WNAM", 0x1234u32.to_le_bytes()),
                sub(b"RCLR", [10, 20, 30, 255]),
            ],
            &None,
        );
        assert_eq!(r.form_id, 7);
        assert_eq!(r.editor_id, "MojaveRegion");
        assert_eq!(r.worldspace_form, Some(0x1234));
        assert_eq!(r.color, Some([10, 20, 30]));
    }

    fn sound_entry(priority: u8, music: Option<u32>) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Sound,
            flags: 0,
            priority,
            payload: RegionDataPayload::Sound {
                music,
                music_type: None,
                incidental: None,
                sounds: Vec::new(),
            },
        }
    }

    fn weather_entry(priority: u8) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Weather,
            flags: 0,
            priority,
            payload: RegionDataPayload::Weather(Vec::new()),
        }
    }

    fn regn(form_id: u32, entries: Vec<RegionDataEntry>) -> RegnRecord {
        RegnRecord {
            form_id,
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn picks_the_only_sound_entry_in_a_single_tagging_region() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0xAAAA))]));
        let winner = select_active_region_sound(&[0x10], &regions).expect("one Sound entry");
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0xAAAA),
                music_type: None,
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn higher_priority_wins_across_two_tagging_regions() {
        // A cell tagged by two overlapping REGN polygons — the RDAT
        // priority byte, not region list order, decides the winner.
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0xAAAA))]));
        regions.insert(0x20, regn(0x20, vec![sound_entry(90, Some(0xBBBB))]));
        // List the lower-priority region FIRST to prove priority, not
        // list order, is what wins.
        let winner =
            select_active_region_sound(&[0x10, 0x20], &regions).expect("higher-priority region");
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0xBBBB),
                music_type: None,
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn ties_keep_region_list_order_then_within_region_order() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![sound_entry(50, Some(0x1111))]));
        regions.insert(0x20, regn(0x20, vec![sound_entry(50, Some(0x2222))]));
        let winner = select_active_region_sound(&[0x10, 0x20], &regions).unwrap();
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0x1111),
                music_type: None,
                incidental: None,
                sounds: Vec::new(),
            },
            "equal priority must keep the first-listed region's entry, mirroring \
             entries_by_priority's stable-sort tie-break"
        );
    }

    #[test]
    fn non_sound_entries_never_win_even_at_higher_priority() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(0x10, vec![weather_entry(100), sound_entry(10, Some(0x1))]),
        );
        let winner = select_active_region_sound(&[0x10], &regions).unwrap();
        assert_eq!(winner.kind, RegionDataKind::Sound);
    }

    #[test]
    fn no_tagging_regions_yields_none() {
        let regions = HashMap::new();
        assert!(select_active_region_sound(&[], &regions).is_none());
    }

    #[test]
    fn a_tagging_region_missing_from_the_map_is_skipped_not_a_panic() {
        // The cell's XCLR points at a REGN this parser never saw (bad
        // load order, or a REGN type this parser doesn't model) — must
        // not panic, and other tagging regions still resolve.
        let mut regions = HashMap::new();
        regions.insert(0x20, regn(0x20, vec![sound_entry(50, Some(0x2222))]));
        let winner = select_active_region_sound(&[0x10_u32, 0x20], &regions).unwrap();
        assert_eq!(
            winner.payload,
            RegionDataPayload::Sound {
                music: Some(0x2222),
                music_type: None,
                incidental: None,
                sounds: Vec::new(),
            }
        );
    }

    #[test]
    fn a_tagging_region_with_no_sound_entries_yields_none() {
        let mut regions = HashMap::new();
        regions.insert(0x10, regn(0x10, vec![weather_entry(100)]));
        assert!(select_active_region_sound(&[0x10], &regions).is_none());
    }
}
