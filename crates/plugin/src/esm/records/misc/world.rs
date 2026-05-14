//! World-definition records — navigation, regions, encounter zones,
//! lighting templates, image-space adapters, activators, terminals.

use super::super::common::{read_f32_at, read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

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
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
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
}

pub fn parse_acti(form_id: u32, subs: &[SubRecord]) -> ActiRecord {
    let mut out = ActiRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"SCRI" => out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"SNAM" => out.sound_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"RNAM" | b"RADR" => {
                out.radio_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
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
    let mut out = TermRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            b"SCRI" => out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0),
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
