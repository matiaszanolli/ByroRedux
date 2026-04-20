//! Stub parsers for nine record types that were previously falling
//! through the `parse_esm` catch-all and getting skipped wholesale
//! (#458 / audit FO3-3-07). Each parser extracts enough data for
//! *references* into the record to resolve — typically EDID + a
//! handful of form refs + a couple of scalar fields — without doing
//! deep sub-record decoding. Full parsing of each can be tightened
//! up per-type when the consuming system lands.
//!
//! | Record | Used by | Minimal shape captured |
//! |--------|---------|------------------------|
//! | `WATR` | `CELL.XCWT` water type | EDID, FULL, TNAM texture |
//! | `NAVI` | nav mesh master | EDID, NVER version |
//! | `NAVM` | per-cell nav mesh | EDID, NVER version |
//! | `REGN` | region definition | EDID, WNAM weather, RCLR color |
//! | `ECZN` | encounter zone | EDID, owner / rank / flags / min-level |
//! | `LGTM` | lighting template | EDID + XCLL-shaped DATA block |
//! | `HDPT` | head part (FaceGen) | EDID, FULL, MODL |
//! | `EYES` | eye definition | EDID, FULL, ICON, flags |
//! | `HAIR` | hair definition | EDID, FULL, MODL, ICON, flags |
//!
//! Per-game bit layouts vary on the LGTM + DATA / HDPT / EYES / HAIR
//! records past Skyrim; the stubs parse the FO3/FNV byte layout and
//! gracefully return defaults on short buffers — Skyrim+ re-parsing
//! lands alongside the consuming system.

use super::common::{read_f32_at, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// Water record — referenced by `CELL.XCWT` (water type form ID on a
/// cell). Pre-fix every XCWT reference dangled at cell load.
#[derive(Debug, Clone, Default)]
pub struct WatrRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Diffuse / noise texture path (`TNAM`). Most FNV water types ship
    /// a `textures\water\*.dds` here.
    pub texture_path: String,
}

pub fn parse_watr(form_id: u32, subs: &[SubRecord]) -> WatrRecord {
    let mut out = WatrRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_zstring(&sub.data),
            b"TNAM" => out.texture_path = read_zstring(&sub.data),
            _ => {}
        }
    }
    out
}

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
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"NVER" if sub.data.len() >= 4 => {
                out.version = read_u32_at(&sub.data, 0).unwrap_or(0);
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
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"NVER" if sub.data.len() >= 4 => {
                out.version = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// Region record (`REGN`). Tags a world-space area with a weather type,
/// a color tint, and downstream `RDAT` entries that scale spawn density
/// / map-color / ambient SFX. Only EDID + weather + color are captured
/// here; the `RDAT`-driven sub-records are out of scope.
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
}

pub fn parse_regn(form_id: u32, subs: &[SubRecord]) -> RegnRecord {
    let mut out = RegnRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"WNAM" if sub.data.len() >= 4 => {
                out.weather_form = read_u32_at(&sub.data, 0);
            }
            b"RCLR" if sub.data.len() >= 3 => {
                out.color = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            _ => {}
        }
    }
    out
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
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 7 => {
                out.owner_form = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.rank = sub.data[4];
                out.min_level = sub.data[5];
                out.flags = sub.data[6];
            }
            _ => {}
        }
    }
    out
}

/// Lighting template (`LGTM`). Provides a named bundle of XCLL-shaped
/// lighting values that cells can reference via `XLGT` and selectively
/// override per-field. Full per-field inheritance fallback lands
/// alongside #379; this stub captures the XCLL-prefix bytes so the
/// consuming lookup has something to read.
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
    pub directional_fade: Option<f32>,
    pub fog_clip: Option<f32>,
    pub fog_power: Option<f32>,
}

pub fn parse_lgtm(form_id: u32, subs: &[SubRecord]) -> LgtmRecord {
    let mut out = LgtmRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 20 => {
                let d = &sub.data;
                out.ambient = [
                    d[0] as f32 / 255.0,
                    d[1] as f32 / 255.0,
                    d[2] as f32 / 255.0,
                ];
                out.directional = [
                    d[4] as f32 / 255.0,
                    d[5] as f32 / 255.0,
                    d[6] as f32 / 255.0,
                ];
                out.fog_color = [
                    d[8] as f32 / 255.0,
                    d[9] as f32 / 255.0,
                    d[10] as f32 / 255.0,
                ];
                out.fog_near = read_f32_at(d, 12).unwrap_or(0.0);
                out.fog_far = read_f32_at(d, 16).unwrap_or(0.0);
                if d.len() >= 32 {
                    out.directional_fade = read_f32_at(d, 28);
                }
                if d.len() >= 36 {
                    out.fog_clip = read_f32_at(d, 32);
                }
                if d.len() >= 40 {
                    out.fog_power = read_f32_at(d, 36);
                }
            }
            _ => {}
        }
    }
    out
}

/// Head part record (`HDPT`). Used by FaceGen to assemble NPC faces —
/// each part (head, mouth, ears, etc.) references a mesh + texture
/// set and a type flag. Pre-Skyrim head variation is sparse enough
/// that even a stub unblocks NPC_ head-part resolution.
#[derive(Debug, Clone, Default)]
pub struct HdptRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// Flags byte — bit layout differs per-game; stored raw. Skyrim
    /// repurposes this as a u8 with bits 0-2 encoding the head-part
    /// type slot (face/eyes/hair/beard/brow/scar).
    pub flags: u8,
}

pub fn parse_hdpt(form_id: u32, subs: &[SubRecord]) -> HdptRecord {
    let mut out = HdptRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// Eye definition (`EYES`). Points NPC_ records at the eye texture +
/// displayable name. Post-Skyrim these are picked via `ALIA`/`HCLF`;
/// FO3 and FNV use this record directly.
#[derive(Debug, Clone, Default)]
pub struct EyesRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub icon_path: String,
    pub flags: u8,
}

pub fn parse_eyes(form_id: u32, subs: &[SubRecord]) -> EyesRecord {
    let mut out = EyesRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_zstring(&sub.data),
            b"ICON" => out.icon_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
            }
            _ => {}
        }
    }
    out
}

/// Hair definition (`HAIR`). Each NPC_ references a hair form that
/// supplies the mesh + texture for the head's hair part.
#[derive(Debug, Clone, Default)]
pub struct HairRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub icon_path: String,
    pub flags: u8,
}

pub fn parse_hair(form_id: u32, subs: &[SubRecord]) -> HairRecord {
    let mut out = HairRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"ICON" => out.icon_path = read_zstring(&sub.data),
            b"DATA" if !sub.data.is_empty() => {
                out.flags = sub.data[0];
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
    fn parse_watr_picks_edid_full_tnam() {
        let subs = vec![
            sub(b"EDID", b"WaterFreshDefault\0"),
            sub(b"FULL", b"Fresh Water\0"),
            sub(b"TNAM", b"textures\\water\\fresh.dds\0"),
        ];
        let w = parse_watr(0x1234, &subs);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "WaterFreshDefault");
        assert_eq!(w.full_name, "Fresh Water");
        assert_eq!(w.texture_path, "textures\\water\\fresh.dds");
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
        let mut data = Vec::with_capacity(40);
        data.extend_from_slice(&[80, 82, 85, 0]); // ambient
        data.extend_from_slice(&[200, 195, 180, 0]); // directional
        data.extend_from_slice(&[40, 45, 50, 0]); // fog color
        data.extend_from_slice(&64.0f32.to_le_bytes()); // fog near
        data.extend_from_slice(&4000.0f32.to_le_bytes()); // fog far
        data.extend_from_slice(&0i32.to_le_bytes()); // rot X
        data.extend_from_slice(&0i32.to_le_bytes()); // rot Y
        data.extend_from_slice(&0.5f32.to_le_bytes()); // dir fade
        data.extend_from_slice(&6000.0f32.to_le_bytes()); // fog clip
        data.extend_from_slice(&1.25f32.to_le_bytes()); // fog power
        let subs = vec![sub(b"EDID", b"LgtmInteriorDim\0"), sub(b"DATA", &data)];
        let l = parse_lgtm(0xDEAD, &subs);
        assert_eq!(l.editor_id, "LgtmInteriorDim");
        assert!((l.ambient[0] - 80.0 / 255.0).abs() < 1e-6);
        assert!((l.directional[1] - 195.0 / 255.0).abs() < 1e-6);
        assert!((l.fog_color[2] - 50.0 / 255.0).abs() < 1e-6);
        assert_eq!(l.fog_near, 64.0);
        assert_eq!(l.fog_far, 4000.0);
        assert_eq!(l.directional_fade, Some(0.5));
        assert_eq!(l.fog_clip, Some(6000.0));
        assert_eq!(l.fog_power, Some(1.25));
    }

    #[test]
    fn parse_lgtm_short_data_returns_defaults() {
        // DATA under 20 bytes → all field captures short-circuit.
        let subs = vec![
            sub(b"EDID", b"ShortLgtm\0"),
            sub(b"DATA", &[1, 2, 3, 4]),
        ];
        let l = parse_lgtm(0xBEEF, &subs);
        assert_eq!(l.editor_id, "ShortLgtm");
        assert_eq!(l.ambient, [0.0; 3]);
        assert_eq!(l.fog_near, 0.0);
        assert!(l.directional_fade.is_none());
    }

    #[test]
    fn parse_hdpt_picks_edid_model_flags() {
        let subs = vec![
            sub(b"EDID", b"HumanHead01\0"),
            sub(b"FULL", b"Human Head\0"),
            sub(b"MODL", b"meshes\\characters\\head.nif\0"),
            sub(b"DATA", &[0x01]),
        ];
        let h = parse_hdpt(0x1111, &subs);
        assert_eq!(h.editor_id, "HumanHead01");
        assert_eq!(h.model_path, "meshes\\characters\\head.nif");
        assert_eq!(h.flags, 0x01);
    }

    #[test]
    fn parse_eyes_picks_icon_flags() {
        let subs = vec![
            sub(b"EDID", b"EyeBlue\0"),
            sub(b"FULL", b"Blue Eyes\0"),
            sub(b"ICON", b"textures\\characters\\eyes\\blue.dds\0"),
            sub(b"DATA", &[0x02]),
        ];
        let e = parse_eyes(0x2222, &subs);
        assert_eq!(e.icon_path, "textures\\characters\\eyes\\blue.dds");
        assert_eq!(e.flags, 0x02);
    }

    #[test]
    fn parse_hair_picks_model_icon_flags() {
        let subs = vec![
            sub(b"EDID", b"HairBrown01\0"),
            sub(b"FULL", b"Brown Hair\0"),
            sub(b"MODL", b"meshes\\characters\\hair\\brown.nif\0"),
            sub(b"ICON", b"textures\\characters\\hair\\brown.dds\0"),
            sub(b"DATA", &[0x00]),
        ];
        let h = parse_hair(0x3333, &subs);
        assert_eq!(h.model_path, "meshes\\characters\\hair\\brown.nif");
        assert_eq!(h.icon_path, "textures\\characters\\hair\\brown.dds");
        assert_eq!(h.flags, 0x00);
    }
}
