//! Game-authored LSCR presentation data. Rendering and contextual selection
//! are separate consumers; decoding a record does not make it eligible.
//!
//! Layout references: TES5Edit/TES5Edit Core/wbDefinitions{TES4,FO3,FNV,TES5,
//! FO4,SF1}.pas, and wbDefinitionsCommon.pas::wbLoadScreenLocations
//! (dev-4.1.6). In particular LNAM stores grid **Y before X**.

use super::common::{read_lstring_or_zstring, read_zstring, remap_fid};
use super::condition::{push_ctda, ConditionList};
use super::index::EsmIndex;
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadScreenLocation {
    /// Direct CELL or WRLD reference; zero means the indirect entry is used.
    pub direct: u32,
    pub world: u32,
    pub grid_x: i16,
    pub grid_y: i16,
}

#[derive(Debug, Clone, Default)]
pub struct LoadScreenRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Preserve all header flags, including main-menu display and no-rotation.
    pub flags: u32,
    /// Legacy image path, or Starfield's authored loadscreen path.
    pub icon: String,
    pub locations: Vec<LoadScreenLocation>,
    /// FNV WMI1 -> LSCT, not a texture or a model.
    pub screen_type: u32,
    pub conditions: ConditionList,
    /// Skyrim+ NNAM -> STAT (or SCOL on later games), not a NIF filename.
    pub model: u32,
    /// FO4+ TNAM -> TRNS.
    pub transform: u32,
    pub initial_scale: Option<f32>,
    pub initial_rotation: Option<[i16; 3]>,
    pub rotation_bounds: Option<[i16; 2]>,
    pub initial_translation: Option<[f32; 3]>,
    pub zoom_bounds: Option<[f32; 2]>,
    pub camera_path: String,
    /// Do not select records with malformed known fields: dropping a broken
    /// restriction would turn a contextual screen into a global one.
    pub malformed_fields: Vec<[u8; 4]>,
}

pub fn parse_lscr(
    form_id: u32,
    flags: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> LoadScreenRecord {
    let legacy = matches!(game, GameKind::Oblivion | GameKind::Fallout3NV);
    let modern = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Starfield
    );
    let mut out = LoadScreenRecord {
        form_id,
        flags,
        ..Default::default()
    };
    for sub in subs {
        let d = sub.data.as_slice();
        // Fixed-width decoding is exact, never zero-filling a truncated
        // restriction or allowing a non-finite transform into the renderer.
        let expected = match &sub.sub_type {
            b"LNAM" if legacy => Some(12),
            b"WMI1" if game == GameKind::Fallout3NV => Some(4),
            b"NNAM" if modern => Some(4),
            b"TNAM" | b"ZNAM" if matches!(game, GameKind::Fallout4 | GameKind::Starfield) => {
                Some(if sub.sub_type == *b"TNAM" { 4 } else { 8 })
            }
            b"SNAM" if game == GameKind::Skyrim => Some(4),
            b"RNAM" if game == GameKind::Skyrim => Some(6),
            b"XNAM" if game == GameKind::Skyrim => Some(12),
            b"ONAM" if modern => Some(4),
            _ => None,
        };
        if expected.is_some_and(|n| d.len() != n) {
            out.malformed_fields.push(sub.sub_type);
            continue;
        }
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(d),
            b"FULL" => out.full_name = read_lstring_or_zstring(d),
            b"DESC" => out.description = read_lstring_or_zstring(d),
            b"ICON" if legacy || game == GameKind::Starfield => out.icon = read_zstring(d),
            b"LNAM" if legacy => out.locations.push(LoadScreenLocation {
                direct: remap_fid(u32::from_le_bytes(d[0..4].try_into().unwrap()), remap),
                world: remap_fid(u32::from_le_bytes(d[4..8].try_into().unwrap()), remap),
                grid_y: i16::from_le_bytes(d[8..10].try_into().unwrap()),
                grid_x: i16::from_le_bytes(d[10..12].try_into().unwrap()),
            }),
            b"WMI1" if game == GameKind::Fallout3NV => {
                out.screen_type = remap_fid(u32::from_le_bytes(d.try_into().unwrap()), remap)
            }
            b"NNAM" if modern => {
                out.model = remap_fid(u32::from_le_bytes(d.try_into().unwrap()), remap)
            }
            b"TNAM" if matches!(game, GameKind::Fallout4 | GameKind::Starfield) => {
                out.transform = remap_fid(u32::from_le_bytes(d.try_into().unwrap()), remap)
            }
            b"SNAM" if game == GameKind::Skyrim => {
                out.initial_scale = finite_floats::<1>(d).map(|v| v[0]);
                if out.initial_scale.is_none() {
                    out.malformed_fields.push(sub.sub_type);
                }
            }
            b"RNAM" if game == GameKind::Skyrim => out.initial_rotation = Some(shorts(d)),
            b"ONAM" if modern => out.rotation_bounds = Some(shorts(d)),
            b"XNAM" if game == GameKind::Skyrim => {
                out.initial_translation = finite_floats(d);
                if out.initial_translation.is_none() {
                    out.malformed_fields.push(sub.sub_type);
                }
            }
            b"ZNAM" if matches!(game, GameKind::Fallout4 | GameKind::Starfield) => {
                out.zoom_bounds = finite_floats(d);
                if out.zoom_bounds.is_none() {
                    out.malformed_fields.push(sub.sub_type);
                }
            }
            b"MOD2" if modern => out.camera_path = read_zstring(d),
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => {
                let before = out.conditions.len();
                push_ctda(sub, remap, &mut out.conditions);
                if matches!(&sub.sub_type, b"CTDA" | b"CTDT") && before == out.conditions.len() {
                    out.malformed_fields.push(sub.sub_type);
                }
            }
            _ => {}
        }
    }
    out
}

/// FO4+ `TRNS` — the authored stage transform an LSCR's TNAM points at.
///
/// Layout: `wbDefinitionsFO4.pas` `wbRecord(TRNS)`: EDID, header flag
/// 0x8000 "Around Origin", and a required `DATA` struct of position (3×f32),
/// rotation (3×f32, **radians** — `wbPosRot`, the same convention as FO4
/// REFR `DATA` rotation; every component of the 1,259 base+DLC TRNS values
/// sits in [0, 2π], on radian landmarks like 3.142/1.571/0.524), scale,
/// then optional zoom min/max (`SetOptionalFrom(2)`: a 28-byte DATA with
/// the zoom tail dropped is legal; 36 bytes with it).
/// Verified against installed `Fallout4.esm` (LoadingBarberTransform:
/// 36-byte DATA, e.g. pos [12, 350, -31.9] rot [6.1, 0, 6.1] — a
/// −10.5°-per-axis pose — scale 1.0 zoom [-0.5, 1.0]).
#[derive(Debug, Clone)]
pub struct LoadScreenTransform {
    pub form_id: u32,
    pub editor_id: String,
    /// Header flag 0x8000 — rotate the model around its origin rather
    /// than its bounds centre. Retained faithfully; the first model
    /// backend does not consume it yet.
    pub around_origin: bool,
    pub translation: [f32; 3],
    pub rotation_rad: [f32; 3],
    pub scale: f32,
    pub zoom_bounds: Option<[f32; 2]>,
    /// Same conservatism contract as [`LoadScreenRecord::malformed_fields`]:
    /// a transform with a truncated or non-finite DATA must never reach the
    /// renderer, and an LSCR pointing at one is ineligible rather than
    /// silently posed at a guess.
    pub malformed_fields: Vec<[u8; 4]>,
}

pub fn parse_trns(form_id: u32, flags: u32, subs: &[SubRecord]) -> LoadScreenTransform {
    let mut out = LoadScreenTransform {
        form_id,
        editor_id: String::new(),
        around_origin: flags & 0x8000 != 0,
        translation: [0.0; 3],
        rotation_rad: [0.0; 3],
        scale: 1.0,
        zoom_bounds: None,
        malformed_fields: Vec::new(),
    };
    let mut saw_data = false;
    for sub in subs {
        let d = sub.data.as_slice();
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(d),
            b"DATA" => {
                saw_data = true;
                // 28 = pos+rot+scale with the optional zoom tail dropped;
                // 36 = the full struct. Anything else is a truncation.
                if !matches!(d.len(), 28 | 36) {
                    out.malformed_fields.push(sub.sub_type);
                    continue;
                }
                let mut floats = [0.0f32; 9];
                for (slot, chunk) in d.chunks_exact(4).enumerate() {
                    floats[slot] = f32::from_le_bytes(chunk.try_into().unwrap());
                }
                if !floats.iter().all(|v| v.is_finite()) {
                    out.malformed_fields.push(sub.sub_type);
                    continue;
                }
                out.translation = [floats[0], floats[1], floats[2]];
                out.rotation_rad = [floats[3], floats[4], floats[5]];
                out.scale = floats[6];
                if d.len() == 36 {
                    out.zoom_bounds = Some([floats[7], floats[8]]);
                }
            }
            _ => {}
        }
    }
    if !saw_data {
        // DATA is `.SetRequired` in the reference layout — a TRNS without
        // one is a broken record, not an identity transform.
        out.malformed_fields.push(*b"DATA");
    }
    out
}

fn shorts<const N: usize>(d: &[u8]) -> [i16; N] {
    std::array::from_fn(|i| i16::from_le_bytes(d[i * 2..i * 2 + 2].try_into().unwrap()))
}

fn finite_floats<const N: usize>(d: &[u8]) -> Option<[f32; N]> {
    let values =
        std::array::from_fn(|i| f32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap()));
    values.iter().all(|v| v.is_finite()).then_some(values)
}

/// A Creation-era LSCR's presentation source, resolved against the same
/// index the record came from: the NNAM target's cached NIF path plus the
/// TNAM transform when the game authors one.
#[derive(Debug, Clone)]
pub struct ResolvedLoadScreenModel {
    /// NNAM → STAT (or SCOL with a cached `CM*.NIF`) model path, exactly
    /// as the cell loader would place it.
    pub model_path: String,
    /// TNAM → TRNS. `None` on Skyrim (no TNAM) and on FO4 records that
    /// author no transform. A non-zero TNAM whose TRNS is missing or
    /// malformed resolves to *no model at all* — see
    /// [`EsmIndex::resolve_load_screen_model`].
    pub transform: Option<LoadScreenTransform>,
}

/// Why an LSCR is not eligible for the transition cover. Mirrored into the
/// `loadscreen.census` console output so a game with zero eligible screens
/// reports the dominant reason instead of a bare count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadScreenRejection {
    /// CTDA present — the screen is contextual and the contextual selector
    /// is not connected yet.
    Conditions,
    /// Legacy location restriction (LNAM) present.
    Locations,
    /// Truncated/non-finite known fields — treating a broken restriction as
    /// absent would widen a contextual screen into a global one.
    Malformed,
    /// DESC still a `<lstring …>` placeholder — the companion strings
    /// tables did not resolve, so the tip cannot be shown faithfully.
    TipUnresolved,
    /// The game's presentation data is absent or unresolvable: no ICON on a
    /// legacy game, no NNAM → model path on a Creation-era game, or a TNAM
    /// whose TRNS failed to decode.
    MissingArtwork,
}

/// One LSCR's eligibility verdict for the transition cover.
#[derive(Debug, Clone)]
pub enum LoadScreenVerdict {
    /// The record can be presented. Carries the resolved Creation-era model
    /// source for Skyrim/FO4 (legacy games present their ICON path from the
    /// record directly).
    Model(Option<ResolvedLoadScreenModel>),
    Rejected(LoadScreenRejection),
}

fn load_screen_verdict(index: &EsmIndex, screen: &LoadScreenRecord) -> LoadScreenVerdict {
    if !screen.malformed_fields.is_empty() {
        return LoadScreenVerdict::Rejected(LoadScreenRejection::Malformed);
    }
    if !screen.conditions.is_empty() {
        return LoadScreenVerdict::Rejected(LoadScreenRejection::Conditions);
    }
    if !screen.locations.is_empty() {
        return LoadScreenVerdict::Rejected(LoadScreenRejection::Locations);
    }
    if screen.description.starts_with("<lstring ") {
        return LoadScreenVerdict::Rejected(LoadScreenRejection::TipUnresolved);
    }
    match index.game {
        GameKind::Oblivion | GameKind::Fallout3NV => {
            if screen.icon.is_empty() {
                LoadScreenVerdict::Rejected(LoadScreenRejection::MissingArtwork)
            } else {
                LoadScreenVerdict::Model(None)
            }
        }
        GameKind::Skyrim | GameKind::Fallout4 => match index.resolve_load_screen_model(screen) {
            Some(model) => LoadScreenVerdict::Model(Some(model)),
            None => LoadScreenVerdict::Rejected(LoadScreenRejection::MissingArtwork),
        },
        // FO76 shares FO4's LSCR shape but is outside the engine's covered
        // fixture set; Starfield authors ICON loadscreen paths the legacy
        // image backend could adopt, but that arrival is tracked separately
        // — neither presents today.
        GameKind::Fallout76 | GameKind::Starfield => {
            LoadScreenVerdict::Rejected(LoadScreenRejection::MissingArtwork)
        }
    }
}

/// LSCR cover-eligibility census over a whole index. `loadscreen.census`
/// and `probe_load_screens --census` both report this, so a regression in
/// record decode or strings resolution shows up as a count shift, not a
/// silent empty cover.
#[derive(Debug, Default, Clone)]
pub struct LoadScreenCensus {
    pub total: usize,
    pub eligible: usize,
    pub rejected_conditions: usize,
    pub rejected_locations: usize,
    pub rejected_malformed: usize,
    pub rejected_tip_unresolved: usize,
    pub rejected_missing_artwork: usize,
}

/// The lowest-FormID eligible LSCR — the deterministic pick the transition
/// cover uses on every game. `None` when no screen is eligible (the door
/// then proceeds uncovered, as it always has).
pub fn first_backend_load_screen(
    index: &EsmIndex,
) -> Option<(&LoadScreenRecord, LoadScreenVerdict)> {
    index
        .load_screens
        .values()
        .filter_map(|screen| {
            match load_screen_verdict(index, screen) {
                LoadScreenVerdict::Model(model) => Some((screen, LoadScreenVerdict::Model(model))),
                LoadScreenVerdict::Rejected(_) => None,
            }
        })
        .min_by_key(|(screen, _)| screen.form_id)
}

/// Census every LSCR in the index by cover eligibility.
pub fn load_screen_census(index: &EsmIndex) -> LoadScreenCensus {
    let mut census = LoadScreenCensus::default();
    for screen in index.load_screens.values() {
        census.total += 1;
        match load_screen_verdict(index, screen) {
            LoadScreenVerdict::Model(_) => census.eligible += 1,
            LoadScreenVerdict::Rejected(reason) => match reason {
                LoadScreenRejection::Conditions => census.rejected_conditions += 1,
                LoadScreenRejection::Locations => census.rejected_locations += 1,
                LoadScreenRejection::Malformed => census.rejected_malformed += 1,
                LoadScreenRejection::TipUnresolved => census.rejected_tip_unresolved += 1,
                LoadScreenRejection::MissingArtwork => census.rejected_missing_artwork += 1,
            },
        }
    }
    census
}

#[cfg(test)]
mod tests;
