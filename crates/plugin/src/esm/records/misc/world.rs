//! World-definition records — navigation, regions, encounter zones,
//! lighting templates, image-space adapters, activators, terminals.

use super::super::common::{read_zstring, CommonNamedFields};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// Navigation mesh master record (`NAVI`). Skyrim+ splits navigation
/// metadata into a top-level master + per-cell `NAVM` children; for
/// pre-Skyrim games this is rare but still present on wilderness
/// worldspaces. Post-render, not a draw path.
#[derive(Debug, Clone, Default)]
pub struct NaviRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `NVER` version tag — format revision the mesh data was exported at.
    pub version: u32,
}

pub fn parse_navi(form_id: u32, subs: &[SubRecord]) -> NaviRecord {
    let mut out = NaviRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"NVER" if sub.data.len() >= 4 => {
                out.version = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
}

/// Per-cell navigation mesh (`NAVM`). Geometry is not extracted — the
/// AI / pathfinding system lands separately and will need to re-parse
/// the full vertex + triangle + edge table.
#[derive(Debug, Clone, Default)]
pub struct NavmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub version: u32,
}

pub fn parse_navm(form_id: u32, subs: &[SubRecord]) -> NavmRecord {
    let mut out = NavmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"NVER" if sub.data.len() >= 4 => {
                out.version = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
}

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
    /// `RDMP` map name as inline text (Oblivion / FO3 / FNV).
    Map(String),
    /// `RDMP` map name as a localised string ID (Skyrim+), resolved against
    /// the plugin's `.STRINGS` tables rather than stored inline.
    MapStringId(u32),
    /// `ICON` landscape-texture path.
    Landscape(String),
    /// `RDGS` raw payload — a single 32-byte sample exists (one Oblivion
    /// region), too little to establish a row layout.
    Grass(Vec<u8>),
    Sound {
        /// `RDMD` music type (Oblivion) / `RDMO` music form (Skyrim) /
        /// `RDSB` blanket sound (FNV).
        music: Option<u32>,
        /// `RDSI` incidental sound form (FNV).
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

/// Region record (`REGN`). Tags a world-space area with a weather type, a
/// colour tint, one or more bounding polygons, and a priority-ordered chain of
/// `RDAT` data entries driving ambient sound, weather, objects and map name.
#[derive(Debug, Clone, Default)]
pub struct RegnRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `WNAM` — weather form that this region enforces. `None` when the
    /// region inherits from its worldspace.
    pub weather_form: Option<u32>,
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
    /// Entries of one kind, highest priority first.
    ///
    /// Ties keep authored order (the sort is stable), which is the only
    /// defensible tie-break: nothing in the record distinguishes two entries
    /// at equal priority.
    pub fn entries_by_priority(&self, kind: RegionDataKind) -> Vec<&RegionDataEntry> {
        let mut matching: Vec<&RegionDataEntry> =
            self.entries.iter().filter(|e| e.kind == kind).collect();
        matching.sort_by(|a, b| b.priority.cmp(&a.priority));
        matching
    }
}

/// Decode a little-endian `u32` at `offset`, or `None` past the end.
fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = data.get(offset..end)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Split `data` into fixed-size rows, ignoring a short trailing remainder.
///
/// Every `RDAT` payload observed in the corpus is an exact multiple of its row
/// size; truncating rather than erroring keeps one malformed plugin from
/// dropping an entire region.
fn rows(data: &[u8], stride: usize) -> impl Iterator<Item = &[u8]> {
    data.chunks_exact(stride.max(1))
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
pub fn parse_regn(form_id: u32, subs: &[SubRecord]) -> RegnRecord {
    let mut out = RegnRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;

    // Section state: the RDAT currently open, and the area currently being
    // accumulated. Both flush when their next opener arrives or at the end.
    let mut open: Option<RegionDataEntry> = None;
    let mut area: Option<RegionArea> = None;

    for sub in subs {
        match &sub.sub_type {
            b"WNAM" if sub.data.len() >= 4 => {
                out.weather_form = SubReader::new(&sub.data).u32().ok();
            }
            b"RCLR" if sub.data.len() >= 3 => {
                out.color = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            b"RPLI" => {
                if let Some(previous) = area.take() {
                    out.areas.push(previous);
                }
                area = Some(RegionArea {
                    edge_fall_off: u32_at(&sub.data, 0).unwrap_or(0),
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
                    kind: RegionDataKind::from_word(u32_at(&sub.data, 0).unwrap_or(0)),
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
        if exact && data.len() % stride != 0 {
            return None;
        }
        let out: Vec<RegionWeather> = rows(data, stride)
            .filter_map(|r| {
                Some(RegionWeather {
                    weather_form: u32_at(r, 0)?,
                    chance: u32_at(r, 4)?,
                    global_form: if stride >= 12 { u32_at(r, 8) } else { None },
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
            let forms: Vec<u32> = rows(data, 52).filter_map(|r| u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Objects(forms);
        }
        b"RDWT" => {
            entry.payload = RegionDataPayload::Weather(decode_weather_rows(data));
        }
        b"RDMP" => {
            // Skyrim localises RDMP: the payload is a 4-byte string ID into
            // the .STRINGS tables rather than inline text, while Oblivion /
            // FO3 / FNV author a zero-terminated string. Reading the localised
            // form as text yields control characters, which is exactly what a
            // corpus sweep showed before this branch existed.
            if data.len() == 4 {
                entry.payload =
                    RegionDataPayload::MapStringId(u32_at(data, 0).expect("length checked"));
            } else {
                entry.payload = RegionDataPayload::Map(read_zstring(data));
            }
        }
        b"ICON" => entry.payload = RegionDataPayload::Landscape(read_zstring(data)),
        b"RDGS" => entry.payload = RegionDataPayload::Grass(data.to_vec()),
        b"RDID" => {
            let forms: Vec<u32> = rows(data, 4).filter_map(|r| u32_at(r, 0)).collect();
            entry.payload = RegionDataPayload::Imposters(forms);
        }
        // Sound sections mix a single music/blanket form with a variable
        // sound list, and the signatures differ per game, so the payload is
        // built up across several sub-records rather than replaced.
        b"RDSD" | b"RDSA" => {
            let list: Vec<RegionSound> = rows(data, 12)
                .filter_map(|r| {
                    Some(RegionSound {
                        sound_form: u32_at(r, 0)?,
                        flags: u32_at(r, 4)?,
                        chance_raw: u32_at(r, 8)?,
                    })
                })
                .collect();
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { sounds, .. } = p {
                    *sounds = list;
                }
            });
        }
        b"RDMD" | b"RDMO" | b"RDSB" => {
            let value = u32_at(data, 0);
            merge_sound(entry, |p| {
                if let RegionDataPayload::Sound { music, .. } = p {
                    *music = value;
                }
            });
        }
        b"RDSI" => {
            let value = u32_at(data, 0);
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
            incidental: None,
            sounds: Vec::new(),
        };
    }
    f(&mut entry.payload);
}

/// Encounter zone (`ECZN`). Governs spawn scaling / faction ownership
/// on the cells that reference it via `XEZN`. The `DATA` layout is:
/// `owner (u32 FormID) + rank (u8) + min-level (u8) + flags (u8) + unused (u8)`.
#[derive(Debug, Clone, Default)]
pub struct EcznRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Form ID of the faction or actor that owns this zone. `0` when
    /// the field is unset (wilderness zones sometimes leave it blank).
    pub owner_form: u32,
    /// Faction rank required; 0 = no rank gate.
    pub rank: u8,
    /// Minimum player level for zone to unlock spawn overrides.
    pub min_level: u8,
    pub flags: u8,
}

pub fn parse_eczn(form_id: u32, subs: &[SubRecord]) -> EcznRecord {
    let mut out = EcznRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 7 => {
                let mut r = SubReader::new(&sub.data);
                out.owner_form = r.u32_or_default();
                out.rank = r.u8_or_default();
                out.min_level = r.u8_or_default();
                out.flags = r.u8_or_default();
            }
            _ => {}
        }
    }
    out
}

/// Lighting template (`LGTM`). Provides a named bundle of XCLL-shaped
/// lighting values that cells can reference via `LTMP` and selectively
/// override per-field. Full per-field inheritance fallback lands
/// alongside #379. Skyrim templates also carry the extended fog/fade
/// values in `DATA` and the directional ambient/specular bundle in
/// `DALC`; both are needed when CELL.XCLL selects template fields via
/// its inheritance mask.
///
/// The `DATA` sub-record mirrors XCLL byte-for-byte (bytes 0-39):
///   0-3:   ambient  (RGBA, byte order per cell.rs XCLL parser)
///   4-7:   directional (RGBA)
///   8-11:  fog color (RGBA)
///   12-15: fog near (f32)
///   16-19: fog far (f32)
///   20-23: rotation X (i32 degrees)
///   24-27: rotation Y (i32 degrees)
///   28-31: directional fade (f32)
///   32-35: fog clip (f32)
///   36-39: fog power (f32)
#[derive(Debug, Clone, Default)]
pub struct LgtmRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ambient color normalised to [0, 1] RGB. XCLL uses RGB byte order
    /// (see `cell.rs` comment; corrected post-#389 revert).
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub directional_rotation: [f32; 2],
    pub directional_fade: Option<f32>,
    pub fog_clip: Option<f32>,
    pub fog_power: Option<f32>,
    pub fog_far_color: Option<[f32; 3]>,
    pub fog_max: Option<f32>,
    pub light_fade_begin: Option<f32>,
    pub light_fade_end: Option<f32>,
    pub directional_ambient: Option<[[f32; 3]; 6]>,
    pub specular_color: Option<[f32; 3]>,
    pub specular_alpha: Option<f32>,
    pub fresnel_power: Option<f32>,
}

pub fn parse_lgtm(form_id: u32, subs: &[SubRecord]) -> LgtmRecord {
    let mut out = LgtmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 20 => {
                let mut r = SubReader::new(&sub.data);
                out.ambient = r.rgb_color().unwrap_or([0.0; 3]);
                out.directional = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_color = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_near = r.f32_or_default();
                out.fog_far = r.f32_or_default();
                let rot_x = (r.i32_or_default() as f32).to_radians();
                let rot_y = (r.i32_or_default() as f32).to_radians();
                out.directional_rotation = [rot_x, rot_y];
                out.directional_fade = r.f32().ok();
                out.fog_clip = r.f32().ok();
                out.fog_power = r.f32().ok();

                // Skyrim v34+ DATA mirrors XCLL except that its ambient
                // bundle at 40-71 is explicitly unused; the live ambient
                // cube/specular/fresnel values are in DALC below. The final
                // u32 (88-91) is likewise unused on LGTM.
                if sub.data.len() >= 92 {
                    r.skip_or_eof(32);
                    out.fog_far_color = r.rgb_color().ok();
                    out.fog_max = r.f32().ok();
                    out.light_fade_begin = r.f32().ok();
                    out.light_fade_end = r.f32().ok();
                }
            }
            b"DALC" if sub.data.len() >= 24 => {
                let mut r = SubReader::new(&sub.data);
                let mut ambient_cube = [[0.0f32; 3]; 6];
                for face in &mut ambient_cube {
                    *face = r.rgb_color().unwrap_or([0.0; 3]);
                }
                out.directional_ambient = Some(ambient_cube);
                if let Ok(spec) = r.rgba_color() {
                    out.specular_color = Some([spec[0], spec[1], spec[2]]);
                    out.specular_alpha = Some(spec[3]);
                }
                out.fresnel_power = r.f32().ok();
            }
            _ => {}
        }
    }
    out
}

/// Image-space record (`IMGS`). Drives per-cell HDR / colour-grading
/// settings — cells reference an IMGS via `XCIM` to override the
/// worldspace-default tone-map / cinematic / tint LUT.
///
/// Skyrim ships ~1k IMGS entries; almost every Solitude / Whiterun
/// interior overrides the worldspace default. Vanilla Skyrim's
/// `DNAM` is 152 bytes (HDR eye-adapt + cinematic
/// saturation/brightness/contrast + tint RGBA + bloom params);
/// FO3/FNV's is the 56-byte subset. Pre-#624 the entire top-level
/// `IMGS` group fell through to the catch-all skip in `parse_esm`,
/// so XCIM cross-references couldn't resolve to anything in the
/// index.
///
/// This stub captures `EDID` + the raw `DNAM` payload so a future
/// per-cell HDR-LUT consumer can decode the tone-map fields lazily
/// without re-walking the ESM. The full DNAM struct decode + IMAD
/// modifier-graph parser are deferred to M48.
#[derive(Debug, Clone, Default)]
pub struct ImgsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Raw `DNAM` payload — Skyrim 152 B (HDR + cinematic + tint),
    /// FO3/FNV 56 B (subset). `None` when the record has no DNAM
    /// (rare; a few legacy entries on FO3/FNV).
    pub dnam_raw: Option<Vec<u8>>,
}

/// Parse an `IMGS` record into an [`ImgsRecord`]. Mirrors the
/// stub-shape of [`parse_lgtm`] — captures EDID + the data payload
/// and defers field-by-field decoding to the consumer. See #624 /
/// SK-D6-NEW-03.
pub fn parse_imgs(form_id: u32, subs: &[SubRecord]) -> ImgsRecord {
    let mut out = ImgsRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DNAM" => out.dnam_raw = Some(sub.data.clone()),
            _ => {}
        }
    }
    out
}

/// `ACTI` activator record. FO3/FNV/Oblivion wall switches, buttons,
/// vending machines, lever-activated doors — anything that the player
/// "use"s but isn't a container, door, or NPC. SCRI on these records
/// runs the trigger script; DEST controls destruction-state meshes.
/// Full destruction-stage decoding is deferred — the stub captures
/// identity + model + SCRI cross-ref so dangling references resolve
/// at lookup time. See #521.
///
/// **Runtime consumer gap (M47.0):** the captured `script_form_id` /
/// `sound_form_id` / `radio_form_id` cross-refs ride through unused
/// today; the trigger / event-hook runtime planned for M47.0 will
/// dispatch ActivateEvent to the SCRI-linked script and play the
/// SNAM/RNAM sound on `OnActivate`. Until then the stub closes the
/// parser-side silent drop so the M47.0 work has one grep target.
#[derive(Debug, Clone, Default)]
pub struct ActiRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// NIF path from MODL — already populated in `cells.statics` via
    /// the MODL catch-all, but duplicated here so a structured record
    /// map is internally consistent.
    pub model_path: String,
    /// SCRI — script form ID attached to this activator. `0` = no
    /// script. Referenced by trigger-system dispatch once it lands.
    pub script_form_id: u32,
    /// SNAM — sound form ID played on activation (optional).
    pub sound_form_id: u32,
    /// RADR / RNAM — radio station form ID, applicable to FNV radio
    /// transmitters (activator variant). `0` when absent.
    pub radio_form_id: u32,
    /// Decoded `VMAD` script attachments (Skyrim+ inline Papyrus). `None`
    /// on FO3 / FNV / Oblivion activators (those use the `SCRI` →  SCPT /
    /// Obscript path). Consumed by the M47.2 scripting-translation layer
    /// to fetch + decompile the attached `.pex`. Activators are the most
    /// common scripted base record in Skyrim cells (levers, doors, traps).
    pub script_instance: Option<crate::esm::records::script_instance::ScriptInstanceData>,
}

pub fn parse_acti(form_id: u32, subs: &[SubRecord]) -> ActiRecord {
    // EDID / FULL / MODL / SCRI / VMAD via the shared helper (TD2-109 /
    // #2068). VMAD is the Skyrim+ inline Papyrus attachment — absent on
    // FO3/FNV, which carry SCRI instead — and the helper decodes it so the
    // M47.2 attach path can decompile the named `.pex` and bind its
    // properties. Only the ACTI-specific sound / radio arms remain below.
    let common = CommonNamedFields::from_subs(subs);
    let mut out = ActiRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        script_instance: common.script_instance,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"SNAM" => out.sound_form_id = SubReader::new(&sub.data).u32_or_default(),
            b"RNAM" | b"RADR" => {
                out.radio_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
}

/// `TERM` terminal record — FO3/FNV computer consoles. Carries a
/// menu tree (MNAM entries), password (ANAM), body text (DNAM), and
/// the NIF model of the physical terminal. MNAM text is collected
/// into `menu_items` so a future terminal-interaction system can
/// walk the options without re-parsing. See #521.
///
/// **Runtime consumer gap (M47.0):** the menu tree, password, and
/// SCRI cross-ref ride through unused — terminal interaction needs
/// the event-hook runtime planned for M47.0 (NNAM target dispatch +
/// CTDA option-gate evaluation, plus a UI overlay for the
/// `body_size`-driven screen). The stub captures the surface so
/// the M47.0 work has one grep target and the labels don't have to
/// be re-walked from the ESM.
#[derive(Debug, Clone, Default)]
pub struct TermRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// SCRI — script form ID (some terminals trigger quest advance
    /// scripts on successful hack).
    pub script_form_id: u32,
    /// ANAM — password string (may be empty for unlocked terminals).
    pub password: String,
    /// DNAM — footer / body text displayed on the terminal screen.
    pub footer_text: String,
    /// BSIZ — body-size scalar (u8, 0 = small, 1 = large). Defaults 0.
    pub body_size: u8,
    /// MNAM — menu-item text, one per entry. Order preserved. Each
    /// MNAM is flanked by its own sub-record group (NNAM target,
    /// CTDA conditions) which is deferred; the stub just captures
    /// the labels so the menu tree isn't lost.
    pub menu_items: Vec<String>,
}

pub fn parse_term(form_id: u32, subs: &[SubRecord]) -> TermRecord {
    // EDID / FULL / MODL / SCRI via the shared helper (TD2-109 / #2068).
    // TERM is FO3/FNV-only, so the helper's VMAD arm never fires here and
    // `TermRecord` carries no script-instance field; only the terminal-
    // specific password / footer / menu arms remain below.
    let common = CommonNamedFields::from_subs(subs);
    let mut out = TermRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        script_form_id: common.script_form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"ANAM" => out.password = read_zstring(&sub.data),
            b"DNAM" => out.footer_text = read_zstring(&sub.data),
            b"BSIZ" if !sub.data.is_empty() => {
                out.body_size = sub.data[0];
            }
            b"MNAM" => {
                // FO3/FNV sometimes ships MNAM as the menu-item text
                // directly and sometimes as a 4-byte form ref (when
                // the label lives elsewhere). Treat as text whenever
                // the bytes are printable; otherwise skip. Keeps the
                // stub robust against the mixed wild encoding.
                let text = read_zstring(&sub.data);
                if !text.is_empty() {
                    out.menu_items.push(text);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_navi_extracts_version() {
        let subs = vec![
            sub(b"EDID", b"NavMaster\0"),
            sub(b"NVER", &11u32.to_le_bytes()),
        ];
        let n = parse_navi(0x5678, &subs);
        assert_eq!(n.editor_id, "NavMaster");
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_navm_extracts_version() {
        let subs = vec![sub(b"NVER", &11u32.to_le_bytes())];
        let n = parse_navm(0xAABB, &subs);
        assert_eq!(n.form_id, 0xAABB);
        assert_eq!(n.version, 11);
    }

    #[test]
    fn parse_regn_picks_weather_and_color() {
        let subs = vec![
            sub(b"EDID", b"WastelandRegion\0"),
            sub(b"WNAM", &0x0001_B000u32.to_le_bytes()),
            sub(b"RCLR", &[128, 96, 64, 0]),
        ];
        let r = parse_regn(0xBEEF, &subs);
        assert_eq!(r.editor_id, "WastelandRegion");
        assert_eq!(r.weather_form, Some(0x0001_B000));
        assert_eq!(r.color, Some([128, 96, 64]));
    }

    #[test]
    fn parse_eczn_picks_owner_rank_flags() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_CAFEu32.to_le_bytes()); // owner form
        data.push(3); // rank
        data.push(15); // min level
        data.push(0x02); // flags
        let subs = vec![sub(b"EDID", b"NcrZone\0"), sub(b"DATA", &data)];
        let z = parse_eczn(0x9876, &subs);
        assert_eq!(z.editor_id, "NcrZone");
        assert_eq!(z.owner_form, 0x0001_CAFE);
        assert_eq!(z.rank, 3);
        assert_eq!(z.min_level, 15);
        assert_eq!(z.flags, 0x02);
    }

    #[test]
    fn parse_lgtm_decodes_xcll_prefix() {
        // Use distinct byte patterns so an off-by-one on any field
        // surfaces as a visible assertion failure.
        let mut data = Vec::with_capacity(92);
        data.extend_from_slice(&[80, 82, 85, 0]); // ambient
        data.extend_from_slice(&[200, 195, 180, 0]); // directional
        data.extend_from_slice(&[40, 45, 50, 0]); // fog color
        data.extend_from_slice(&64.0f32.to_le_bytes()); // fog near
        data.extend_from_slice(&4000.0f32.to_le_bytes()); // fog far
        data.extend_from_slice(&15i32.to_le_bytes()); // rot X
        data.extend_from_slice(&(-30i32).to_le_bytes()); // rot Y
        data.extend_from_slice(&0.5f32.to_le_bytes()); // dir fade
        data.extend_from_slice(&6000.0f32.to_le_bytes()); // fog clip
        data.extend_from_slice(&1.25f32.to_le_bytes()); // fog power
        data.extend_from_slice(&[0xAA; 32]); // DATA ambient bundle is unused
        data.extend_from_slice(&[90, 91, 92, 0]); // fog far color
        data.extend_from_slice(&0.75f32.to_le_bytes()); // fog max
        data.extend_from_slice(&512.0f32.to_le_bytes()); // light fade begin
        data.extend_from_slice(&1024.0f32.to_le_bytes()); // light fade end
        data.extend_from_slice(&0u32.to_le_bytes()); // unused

        let mut dalc = Vec::with_capacity(32);
        for face in 0u8..6 {
            let base = 10 + face * 10;
            dalc.extend_from_slice(&[base, base + 1, base + 2, 0]);
        }
        dalc.extend_from_slice(&[210, 211, 212, 128]);
        dalc.extend_from_slice(&3.5f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"LgtmInteriorDim\0"),
            sub(b"DATA", &data),
            sub(b"DALC", &dalc),
        ];
        let l = parse_lgtm(0xDEAD, &subs);
        assert_eq!(l.editor_id, "LgtmInteriorDim");
        assert!((l.ambient[0] - 80.0 / 255.0).abs() < 1e-6);
        assert!((l.directional[1] - 195.0 / 255.0).abs() < 1e-6);
        assert!((l.fog_color[2] - 50.0 / 255.0).abs() < 1e-6);
        assert_eq!(l.fog_near, 64.0);
        assert_eq!(l.fog_far, 4000.0);
        assert!((l.directional_rotation[0] - 15.0f32.to_radians()).abs() < 1e-6);
        assert!((l.directional_rotation[1] - (-30.0f32).to_radians()).abs() < 1e-6);
        assert_eq!(l.directional_fade, Some(0.5));
        assert_eq!(l.fog_clip, Some(6000.0));
        assert_eq!(l.fog_power, Some(1.25));
        assert_eq!(
            l.fog_far_color,
            Some([90.0 / 255.0, 91.0 / 255.0, 92.0 / 255.0])
        );
        assert_eq!(l.fog_max, Some(0.75));
        assert_eq!(l.light_fade_begin, Some(512.0));
        assert_eq!(l.light_fade_end, Some(1024.0));
        let cube = l.directional_ambient.expect("DALC ambient cube");
        assert_eq!(cube[0], [10.0 / 255.0, 11.0 / 255.0, 12.0 / 255.0]);
        assert_eq!(cube[5], [60.0 / 255.0, 61.0 / 255.0, 62.0 / 255.0]);
        assert_eq!(
            l.specular_color,
            Some([210.0 / 255.0, 211.0 / 255.0, 212.0 / 255.0])
        );
        assert_eq!(l.specular_alpha, Some(128.0 / 255.0));
        assert_eq!(l.fresnel_power, Some(3.5));
    }

    /// Regression for #624 / SK-D6-NEW-03. IMGS records were dropped
    /// on the parse_esm catch-all skip pre-fix; this tests the stub
    /// parser captures EDID + raw DNAM so XCIM cross-references can
    /// resolve through `EsmIndex.image_spaces`.
    #[test]
    fn parse_imgs_captures_edid_and_dnam_payload() {
        // 56-byte DNAM patterned with distinct bytes so a future
        // field decoder catches misalignment vs the captured raw
        // payload. Vanilla FO3/FNV ship the 56-byte form; Skyrim
        // extends to 152 — the stub captures whatever DNAM length
        // the file ships with.
        let dnam: Vec<u8> = (0u8..56).collect();
        let subs = vec![sub(b"EDID", b"InteriorWarmDim\0"), sub(b"DNAM", &dnam)];
        let imgs = parse_imgs(0x000A_1234, &subs);
        assert_eq!(imgs.form_id, 0x000A_1234);
        assert_eq!(imgs.editor_id, "InteriorWarmDim");
        assert_eq!(imgs.dnam_raw.as_deref(), Some(dnam.as_slice()));
    }

    /// Companion: an IMGS record with no DNAM (legacy FO3 entries)
    /// captures EDID and leaves `dnam_raw` at None — pinning the
    /// stub's "best-effort capture" semantics so a future consumer
    /// doesn't have to guard against the absent case downstream.
    #[test]
    fn parse_imgs_without_dnam_leaves_payload_none() {
        let subs = vec![sub(b"EDID", b"LegacyImagespace\0")];
        let imgs = parse_imgs(0x000A_5678, &subs);
        assert_eq!(imgs.editor_id, "LegacyImagespace");
        assert!(imgs.dnam_raw.is_none());
    }

    #[test]
    fn parse_lgtm_short_data_returns_defaults() {
        // DATA under 20 bytes → all field captures short-circuit.
        let subs = vec![sub(b"EDID", b"ShortLgtm\0"), sub(b"DATA", &[1, 2, 3, 4])];
        let l = parse_lgtm(0xBEEF, &subs);
        assert_eq!(l.editor_id, "ShortLgtm");
        assert_eq!(l.ambient, [0.0; 3]);
        assert_eq!(l.fog_near, 0.0);
        assert!(l.directional_fade.is_none());
    }
    #[test]
    fn parse_acti_extracts_scri_and_model() {
        let subs = vec![
            sub(b"EDID", b"NukaColaMachine01\0"),
            sub(b"FULL", b"Nuka-Cola Machine\0"),
            sub(b"MODL", b"activators\\nukacolamachine01.nif\0"),
            sub(b"SCRI", &0x0010_ABCDu32.to_le_bytes()),
            sub(b"SNAM", &0x0009_0000u32.to_le_bytes()),
        ];
        let a = parse_acti(0x0002_9E7A, &subs);
        assert_eq!(a.editor_id, "NukaColaMachine01");
        assert_eq!(a.full_name, "Nuka-Cola Machine");
        assert_eq!(a.model_path, "activators\\nukacolamachine01.nif");
        assert_eq!(a.script_form_id, 0x0010_ABCD);
        assert_eq!(a.sound_form_id, 0x0009_0000);
        // Radio form defaults to 0 when RNAM/RADR absent.
        assert_eq!(a.radio_form_id, 0);
    }

    /// TD2-109 / #2068 — `parse_acti` now sources its EDID/FULL/MODL/SCRI/
    /// VMAD bundle from `CommonNamedFields::from_subs`, which means each
    /// field is copied across by hand into `ActiRecord`. Every other field
    /// is asserted by `parse_acti_extracts_scri_and_model` above; the
    /// decoded VMAD attachment was not covered by anything, so a dropped
    /// `script_instance` mapping would have gone unnoticed. Activators are
    /// the most common scripted base record in Skyrim cells, and the M47.2
    /// attach path reads exactly this field.
    #[test]
    fn parse_acti_decodes_vmad_into_script_instance() {
        // version 5, objectFormat 2, 1 script "DoorScript", 0 props —
        // same shape as `common::tests::common_named_fields_decodes_vmad_script_instance`.
        let mut vmad = Vec::new();
        vmad.extend_from_slice(&5i16.to_le_bytes());
        vmad.extend_from_slice(&2i16.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        let name = b"DoorScript";
        vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
        vmad.extend_from_slice(name);
        vmad.push(0); // script status
        vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

        let subs = vec![
            sub(b"EDID", b"SkyrimLever01\0"),
            sub(b"MODL", b"clutter\\lever01.nif\0"),
            sub(b"VMAD", &vmad),
        ];
        let a = parse_acti(0x0001_0000, &subs);
        assert_eq!(a.editor_id, "SkyrimLever01");
        assert_eq!(a.model_path, "clutter\\lever01.nif");
        let inst = a
            .script_instance
            .expect("VMAD must survive the CommonNamedFields hand-off");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "DoorScript");
        // Skyrim activators carry VMAD *instead of* SCRI.
        assert_eq!(a.script_form_id, 0);
    }

    #[test]
    fn parse_term_extracts_password_footer_menu() {
        let subs = vec![
            sub(b"EDID", b"Vault21OverseerTerminal\0"),
            sub(b"FULL", b"Overseer's Terminal\0"),
            sub(b"MODL", b"clutter\\junk\\terminal01.nif\0"),
            sub(b"ANAM", b"tranquility\0"),
            sub(b"DNAM", b"Welcome, Overseer. Vault 21 online.\0"),
            sub(b"BSIZ", &[1u8]),
            sub(b"MNAM", b"Open Vault Door\0"),
            sub(b"MNAM", b"View Resident Log\0"),
            sub(b"MNAM", b"Disable Security\0"),
            sub(b"SCRI", &0x0004_2CD2u32.to_le_bytes()),
        ];
        let t = parse_term(0x0004_2424, &subs);
        assert_eq!(t.editor_id, "Vault21OverseerTerminal");
        assert_eq!(t.password, "tranquility");
        assert!(t.footer_text.starts_with("Welcome, Overseer"));
        assert_eq!(t.body_size, 1);
        assert_eq!(t.menu_items.len(), 3);
        assert_eq!(t.menu_items[0], "Open Vault Door");
        assert_eq!(t.menu_items[2], "Disable Security");
        assert_eq!(t.script_form_id, 0x0004_2CD2);
    }
    #[test]
    fn parse_term_unlocked_terminal_has_empty_password() {
        // Tutorial / ambient terminals often ship without ANAM; stub
        // must tolerate that without panicking.
        let subs = vec![
            sub(b"EDID", b"GoodspringsSchoolTerminal\0"),
            sub(b"FULL", b"School Terminal\0"),
            sub(b"DNAM", b"Primer by Mr. Goodsprings.\0"),
        ];
        let t = parse_term(0x0008_1111, &subs);
        assert!(t.password.is_empty());
        assert_eq!(t.body_size, 0);
        assert!(t.menu_items.is_empty());
    }

    // ── #808 / FNV-D2-NEW-01 stubs ─────────────────────────────────
}

#[cfg(test)]
mod regn_tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *sig,
            data: data.to_vec(),
        }
    }

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
        let r = parse_regn(1, &[rdat(3, 1, 75)]);
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
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &short)]);
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
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)]);
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
                sub(b"RDSB", &0x99u32.to_le_bytes()),
                sub(b"RDSD", &sounds),
                sub(b"RDSI", &0x77u32.to_le_bytes()),
            ],
        );
        match &r.entries[0].payload {
            RegionDataPayload::Sound {
                music,
                incidental,
                sounds,
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

    #[test]
    fn skyrim_music_signature_merges_the_same_way() {
        let r = parse_regn(1, &[rdat(7, 0, 50), sub(b"RDMO", &0x4242u32.to_le_bytes())]);
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Sound {
                music: Some(0x4242),
                ..
            }
        ));
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
                sub(b"RPLI", &128u32.to_le_bytes()),
                sub(b"RPLD", &points),
                sub(b"RPLI", &64u32.to_le_bytes()),
                sub(b"RPLD", &points[..16]),
            ],
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
        let r = parse_regn(1, &[sub(b"RPLD", &points)]);
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
        let r = parse_regn(1, &[rdat(2, 0, 50), rdat(2, 1, 50), rdat(2, 0, 50)]);
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
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &ragged)]);
        match &r.entries[0].payload {
            RegionDataPayload::Weather(w) => assert_eq!(w.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_rdat_with_no_payload_stays_empty_rather_than_absent() {
        // Skyrim authors 69 empty RDOT sections; they must still appear so a
        // consumer sees the declared-but-empty distinction.
        let r = parse_regn(1, &[rdat(2, 0, 50), sub(b"RDOT", &[])]);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, RegionDataKind::Objects);
        assert!(matches!(
            r.entries[0].payload,
            RegionDataPayload::Objects(ref v) if v.is_empty()
        ));
    }

    #[test]
    fn payloads_before_any_rdat_are_ignored_not_misattributed() {
        let r = parse_regn(1, &[sub(b"RDMP", b"orphan\0"), rdat(4, 0, 50)]);
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
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &oblivion)]);
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
        let r = parse_regn(1, &[rdat(3, 0, 50), sub(b"RDWT", &long)]);
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
    fn skyrim_localised_map_names_are_string_ids_not_text() {
        // Skyrim's RDMP is a 4-byte .STRINGS id; reading it as inline text
        // yields control characters, which a corpus sweep surfaced as garbage
        // map names before this branch existed.
        let r = parse_regn(
            1,
            &[rdat(4, 0, 50), sub(b"RDMP", &0x0001_1086u32.to_le_bytes())],
        );
        assert_eq!(
            r.entries[0].payload,
            RegionDataPayload::MapStringId(0x0001_1086)
        );

        // …while the inline form still parses as text.
        let r = parse_regn(1, &[rdat(4, 0, 50), sub(b"RDMP", b"Arefu\0")]);
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
                sub(b"WNAM", &0x1234u32.to_le_bytes()),
                sub(b"RCLR", &[10, 20, 30, 255]),
            ],
        );
        assert_eq!(r.form_id, 7);
        assert_eq!(r.editor_id, "MojaveRegion");
        assert_eq!(r.weather_form, Some(0x1234));
        assert_eq!(r.color, Some([10, 20, 30]));
    }
}
