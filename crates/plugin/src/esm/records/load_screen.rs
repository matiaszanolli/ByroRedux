//! Game-authored LSCR presentation data. Rendering and contextual selection
//! are separate consumers; decoding a record does not make it eligible.
//!
//! Layout references: TES5Edit/TES5Edit Core/wbDefinitions{TES4,FO3,FNV,TES5,
//! FO4,SF1}.pas, and wbDefinitionsCommon.pas::wbLoadScreenLocations
//! (dev-4.1.6). In particular LNAM stores grid **Y before X**.

use super::common::{read_lstring_or_zstring, read_zstring, remap_fid};
use super::condition::{push_ctda, ConditionList};
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

fn shorts<const N: usize>(d: &[u8]) -> [i16; N] {
    std::array::from_fn(|i| i16::from_le_bytes(d[i * 2..i * 2 + 2].try_into().unwrap()))
}

fn finite_floats<const N: usize>(d: &[u8]) -> Option<[f32; N]> {
    let values =
        std::array::from_fn(|i| f32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap()));
    values.iter().all(|v| v.is_finite()).then_some(values)
}

#[cfg(test)]
mod tests;
