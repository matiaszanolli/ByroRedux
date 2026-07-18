//! Character appearance records — head parts, eyes, hair.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

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
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
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
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
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
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
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

// ── AI / dialogue / effect stubs (#446, #447) ────────────────────────
//
// Extended set of record types that pre-#446/#447 fell through to the
// catch-all skip. Same philosophy as the WATR/NAVI/... stubs above:
// capture EDID + FULL + a handful of flags / form refs so that dangling
// cross-references resolve; full per-record decoding lands with the
// consuming subsystem (AI runtime, dialogue system, perk pipeline).

/// CSTY — combat style record. NPC combat AI behavior profile
/// (aggression, stealth preference, ranged vs melee). Per-NPC
/// reference via NPC.SPCT. `CSTD` carries the FO3/FNV 124-byte
/// payload; the stub captures only the first 4 bytes of CSTD as a
/// flags scalar so the dispatch is verifiable. Full CSTD decode
/// lands with the AI consumer. See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct CstyRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `CSTD` offset 0..4 — combat-style flag bitfield (u32). Decoded
    /// lazily per-game; vanilla FNV uses ~12 bits.
    pub csty_flags: u32,
}

pub fn parse_csty(form_id: u32, subs: &[SubRecord]) -> CstyRecord {
    let mut out = CstyRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"CSTD" if sub.data.len() >= 4 => {
                out.csty_flags = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
}

/// IDLE — idle animation record. NPC behavior tree references —
/// "lean against wall", "smoke", "drink", etc. Each NPC's PACK
/// references IDLEs by form ID. Stub captures EDID + animation file
/// path (MODL). See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct IdleRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — animation file path (typically `.kf`).
    pub animation_path: String,
}

pub fn parse_idle(form_id: u32, subs: &[SubRecord]) -> IdleRecord {
    let mut out = IdleRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"MODL" => out.animation_path = read_zstring(&sub.data),
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

    #[test]
    fn parse_csty_picks_edid_csty_flags() {
        // `csyAggressive` shape: CSTD with a flag byte at offset 0.
        let mut cstd = [0u8; 124];
        cstd[0..4].copy_from_slice(&0x0000_0042_u32.to_le_bytes());
        let subs = vec![sub(b"EDID", b"csyAggressive\0"), sub(b"CSTD", &cstd)];
        let c = parse_csty(0x0008_3122, &subs);
        assert_eq!(c.editor_id, "csyAggressive");
        assert_eq!(c.csty_flags, 0x42);
    }

    #[test]
    fn parse_idle_picks_edid_modl() {
        // `IdleStandSmokingCigarette` shape: EDID + MODL pointing at
        // a `.kf` animation file in `meshes\\actors\\character\\` etc.
        let subs = vec![
            sub(b"EDID", b"IdleStandSmokingCigarette\0"),
            sub(b"MODL", b"actors\\character\\idleanims\\smoke.kf\0"),
        ];
        let i = parse_idle(0x000A_FB31, &subs);
        assert_eq!(i.editor_id, "IdleStandSmokingCigarette");
        assert!(i.animation_path.contains("smoke.kf"));
    }

    // ── AI / dialogue / effect stubs (#446, #447) ──────────────────
}
