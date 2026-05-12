//! Effects / FX / VATS / impact records.

use super::super::common::{read_f32_at, read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// Actor Value Information record (`AVIF`). Defines the ~30 actor
/// values FO3/FNV expose to the perk / VATS / SPECIAL pipelines —
/// Strength, Endurance, CombatSkill, every governed skill, plus
/// resistances + resources. Skyrim+ adds a per-skill perk-tree
/// graph (PNAM/INAM/CNAM section list); only the FO3/FNV-shape
/// fields are captured here. The Skyrim perk-tree decoder lands
/// alongside the perk-graph consumer.
///
/// Pre-fix the whole top-level group fell through the catch-all
/// skip in `parse_esm`, so every NPC `skill_bonuses` cross-ref,
/// every BOOK skill-book ref, and every AVIF-keyed condition
/// predicate (~300 condition functions) dangled. See #519.
#[derive(Debug, Clone, Default)]
pub struct AvifRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `DESC` — long description shown in the Pip-Boy / skills UI.
    pub description: String,
    /// `ANAM` — short-form abbreviation. Only present on a handful
    /// of values (1Hand, 2Hand, etc.); empty otherwise.
    pub abbreviation: String,
    /// `CNAM` — skill category for skill-typed AVIFs:
    /// 0 = None, 1 = Combat, 2 = Magic, 3 = Stealth.
    /// Non-skill AVIFs reuse the four bytes for opaque flag data
    /// (kept verbatim — semantics differ per game, decoded by the
    /// consuming subsystem).
    pub category: u32,
    /// `AVSK` — skill-scaling tuple (only present for skill AVIFs):
    /// `[skill_use_mult, skill_use_offset, skill_improve_mult, skill_improve_offset]`.
    /// `None` for non-skill records (resistances, resources, attributes).
    pub skill_scaling: Option<[f32; 4]>,
}

pub fn parse_avif(form_id: u32, subs: &[SubRecord]) -> AvifRecord {
    let mut out = AvifRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"ANAM" => out.abbreviation = read_zstring(&sub.data),
            b"CNAM" => out.category = read_u32_at(&sub.data, 0).unwrap_or(0),
            b"AVSK" if sub.data.len() >= 16 => {
                out.skill_scaling = Some([
                    read_f32_at(&sub.data, 0).unwrap_or(0.0),
                    read_f32_at(&sub.data, 4).unwrap_or(0.0),
                    read_f32_at(&sub.data, 8).unwrap_or(0.0),
                    read_f32_at(&sub.data, 12).unwrap_or(0.0),
                ]);
            }
            _ => {}
        }
    }
    out
}

// ── #808 / FNV-D2-NEW-01 — gameplay-critical record stubs ──────────
//
// Five record types that gate FNV gameplay subsystems:
//   PROJ — projectile data; every WEAP references one for muzzle
//          velocity, gravity, AoE, lifetime, impact behavior.
//   EFSH — effect shader; visual effects for spells, grenades,
//          muzzle flashes, blood splatter.
//   IMOD — item mod (FNV-CORE); weapon attachments — sights,
//          suppressors, extended mags, scopes.
//   ARMA — armor addon; race-specific biped slot variants. ARMO
//          → ARMA → race-specific MODL chain.
//   BPTD — body part data; per-NPC dismemberment routing + biped
//          slot count.
//
// All five are stub-form: EDID + a handful of key scalar / form-ref
// fields. Full sub-record decoding lands when the consuming subsystem
// arrives. Pattern matches the #458 / #519 / #520 / #521 / #629 /
// #630 / #631 closeouts.

/// PROJ — projectile record. Every WEAP references a PROJ for
/// muzzle velocity, gravity, AoE radius, lifetime, impact behavior.
/// The full FNV `DATA` payload is 92 bytes; the stub captures only
/// the flag bitfield (offset 0) and the muzzle speed (offset 8) so
/// downstream firing-simulator code has a starting point. See
/// audit `FNV-D2-NEW-01` / #808.
#[derive(Debug, Clone, Default)]
pub struct ProjRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `DATA` offset 0..4 — projectile type bitfield (Missile, Lobber,
    /// Beam, Flame, Cone, Barrier, Arrow). Decoded lazily per-game.
    pub flags: u32,
    /// `DATA` offset 8..12 — muzzle speed in game units / second.
    pub muzzle_speed: f32,
}

pub fn parse_proj(form_id: u32, subs: &[SubRecord]) -> ProjRecord {
    let mut out = ProjRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 12 => {
                out.flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                out.muzzle_speed = read_f32_at(&sub.data, 8).unwrap_or(0.0);
            }
            _ => {}
        }
    }
    out
}

/// EFSH — effect shader record. Visual-effect surface (fill texture,
/// particle texture, addon model). Referenced from MGEF / SPEL / EXPL.
/// Full DATA struct decode (render flags, fill colors, blend modes,
/// addon model) deferred to the VFX consumer. See audit
/// `FNV-D2-NEW-01` / #808.
#[derive(Debug, Clone, Default)]
pub struct EfshRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `ICON` — fill texture path. The EFSH surface's primary look.
    pub fill_texture: String,
    /// `ICO2` — particle / addon texture path. Optional.
    pub particle_texture: String,
}

pub fn parse_efsh(form_id: u32, subs: &[SubRecord]) -> EfshRecord {
    let mut out = EfshRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"ICON" => out.fill_texture = read_zstring(&sub.data),
            b"ICO2" => out.particle_texture = read_zstring(&sub.data),
            _ => {}
        }
    }
    out
}

/// IMOD — item mod record (FNV-CORE). Weapon attachments — sights,
/// suppressors, extended mags, scopes. Each WEAP has up to 3 mod
/// slots referencing IMODs. Stub captures EDID + display name +
/// description + value/weight scalars. See audit `FNV-D2-NEW-01`
/// / #808.
#[derive(Debug, Clone, Default)]
pub struct ImodRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// `DATA` offset 0..4 — caps value (i32).
    pub value: i32,
    /// `DATA` offset 4..8 — weight in pounds (f32).
    pub weight: f32,
}

pub fn parse_imod(form_id: u32, subs: &[SubRecord]) -> ImodRecord {
    let mut out = ImodRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 8 => {
                out.value =
                    i32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                out.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
            }
            _ => {}
        }
    }
    out
}

/// REPU — reputation record (FNV-CORE). NCR / Legion / Powder
/// Gangers / Boomers / Brotherhood / Followers — drives FNV's
/// faction-reputation system and quest gating. ~12 vanilla records.
/// See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct RepuRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `DATA` — base reputation modifier (f32). Most records ship
    /// `0.0` or `1.0`; edge cases shift towards a faction.
    pub base_value: f32,
}

pub fn parse_repu(form_id: u32, subs: &[SubRecord]) -> RepuRecord {
    let mut out = RepuRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 4 => {
                out.base_value = read_f32_at(&sub.data, 0).unwrap_or(0.0);
            }
            _ => {}
        }
    }
    out
}

/// EXPL — explosion record. Frag grenades, mines, explosive ammo
/// blast effects. Linked to PROJ via PROJ→EXPL→EFSH chain, plus
/// damage / radius / sound / impact-data refs. Stub captures the
/// damage and force/radius scalars from `DATA`. See audit
/// `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct ExplRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// `DATA` offset 8..12 — damage in HP per direct hit (f32).
    pub damage: f32,
    /// `DATA` offset 12..16 — explosion blast radius in game units (f32).
    pub radius: f32,
}

pub fn parse_expl(form_id: u32, subs: &[SubRecord]) -> ExplRecord {
    let mut out = ExplRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 16 => {
                out.damage = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                out.radius = read_f32_at(&sub.data, 12).unwrap_or(0.0);
            }
            _ => {}
        }
    }
    out
}

/// IPCT — impact record. Bullet-impact visual effect (puff of dust on
/// stone, splinters on wood, water splash, blood spray on flesh).
/// Each material-class IPDS routes to a per-material IPCT. Stub
/// captures EDID + impact model path. See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct IpctRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — impact-effect model path (particle / decal mesh).
    pub model_path: String,
}

pub fn parse_ipct(form_id: u32, subs: &[SubRecord]) -> IpctRecord {
    let mut out = IpctRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"MODL" => out.model_path = read_zstring(&sub.data),
            _ => {}
        }
    }
    out
}

/// IPDS — impact data set. 12-entry table mapping per-material
/// surface kinds (stone, dirt, grass, glass, metal, wood, organic,
/// cloth, water, hollow metal, organic bug, organic glow) to their
/// respective IPCT records. Referenced by WEAP / PROJ for per-shot
/// material-aware impact effects. Stub captures EDID + the count of
/// material-IPCT pairs from `DATA`. See audit `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct IpdsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Number of material-IPCT pairs (12 on FO3/FNV per UESP).
    /// `DATA` is a fixed 12-entry array of (material_kind: u8, ipct: u32)
    /// or similar; stub captures the count for sanity-check.
    pub material_pair_count: u32,
}

pub fn parse_ipds(form_id: u32, subs: &[SubRecord]) -> IpdsRecord {
    let mut out = IpdsRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            // FO3/FNV IPDS DATA is a fixed-size 96-byte array
            // (12 × 8 bytes = (material_kind: u32, ipct: u32) per
            // entry). Skyrim uses 4-byte entries. Counting only:
            b"DATA" => {
                out.material_pair_count = (sub.data.len() / 8) as u32;
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
    fn parse_avif_skill_record_decodes_avsk_and_category() {
        // Small Guns: skill, Combat category, full AVSK tuple.
        let mut avsk = Vec::new();
        avsk.extend_from_slice(&1.0f32.to_le_bytes()); // skill_use_mult
        avsk.extend_from_slice(&0.0f32.to_le_bytes()); // skill_use_offset
        avsk.extend_from_slice(&1.5f32.to_le_bytes()); // skill_improve_mult
        avsk.extend_from_slice(&2.0f32.to_le_bytes()); // skill_improve_offset
        let subs = vec![
            sub(b"EDID", b"SmallGuns\0"),
            sub(b"FULL", b"Small Guns\0"),
            sub(b"DESC", b"Affects accuracy with pistols and rifles.\0"),
            sub(b"ANAM", b"SG\0"),
            sub(b"CNAM", &1u32.to_le_bytes()), // Combat
            sub(b"AVSK", &avsk),
        ];
        let a = parse_avif(0x0000_002B, &subs);
        assert_eq!(a.editor_id, "SmallGuns");
        assert_eq!(a.full_name, "Small Guns");
        assert_eq!(a.abbreviation, "SG");
        assert_eq!(a.category, 1);
        let scaling = a.skill_scaling.expect("AVSK populated for skill records");
        assert_eq!(scaling, [1.0, 0.0, 1.5, 2.0]);
    }

    #[test]
    fn parse_avif_non_skill_record_has_no_avsk() {
        // Strength: SPECIAL attribute — no AVSK, no category set.
        let subs = vec![
            sub(b"EDID", b"Strength\0"),
            sub(b"FULL", b"Strength\0"),
            sub(b"DESC", b"Raw physical power.\0"),
        ];
        let a = parse_avif(0x0000_0000, &subs);
        assert_eq!(a.editor_id, "Strength");
        assert_eq!(a.category, 0);
        assert!(a.skill_scaling.is_none());
    }
    #[test]
    fn parse_proj_picks_edid_full_speed() {
        // `5mmRoundProjectile` shape: DATA carries the 92-byte payload
        // where the first 4 bytes are the flag/type bitfield and bytes
        // 8..12 are the muzzle speed (f32). Stub captures only the
        // speed for now; the full DATA struct decode lands with the
        // firing simulator.
        let mut data = [0u8; 16];
        data[0..4].copy_from_slice(&0x0000_0001u32.to_le_bytes()); // type bitfield
        data[8..12].copy_from_slice(&3000.0_f32.to_le_bytes()); // muzzle speed
        let subs = vec![
            sub(b"EDID", b"5mmRoundProjectile\0"),
            sub(b"FULL", b"5mm Round\0"),
            sub(b"DATA", &data),
        ];
        let p = parse_proj(0x0007_4824, &subs);
        assert_eq!(p.editor_id, "5mmRoundProjectile");
        assert_eq!(p.full_name, "5mm Round");
        assert_eq!(p.flags, 0x0000_0001);
        assert!((p.muzzle_speed - 3000.0).abs() < 1e-3);
    }

    #[test]
    fn parse_proj_short_data_is_tolerated() {
        // Truncated DATA (or absent DATA on placeholder PROJs) must not
        // panic. Stub returns Default::default() field values.
        let subs = vec![sub(b"EDID", b"TestProjectile\0")];
        let p = parse_proj(0xDEADBEEF, &subs);
        assert_eq!(p.editor_id, "TestProjectile");
        assert_eq!(p.flags, 0);
        assert_eq!(p.muzzle_speed, 0.0);
    }

    #[test]
    fn parse_efsh_picks_edid_fill_texture() {
        // `EFFShockBeamCloud01` shape: ICON is the fill-texture path
        // for the effect surface.
        let subs = vec![
            sub(b"EDID", b"EFFShockBeamCloud01\0"),
            sub(b"ICON", b"effects\\shockbeam_cloud.dds\0"),
            sub(b"ICO2", b"effects\\shockbeam_particles.dds\0"),
        ];
        let e = parse_efsh(0x0010_0BBE, &subs);
        assert_eq!(e.editor_id, "EFFShockBeamCloud01");
        assert_eq!(e.fill_texture, "effects\\shockbeam_cloud.dds");
        assert_eq!(e.particle_texture, "effects\\shockbeam_particles.dds");
    }

    #[test]
    fn parse_imod_picks_edid_full_desc_value_weight() {
        // `Mod.5mmRound.Hollow Point` shape: DATA = i32 value + f32
        // weight. Stub captures both.
        let mut data = [0u8; 8];
        data[0..4].copy_from_slice(&50_i32.to_le_bytes()); // value
        data[4..8].copy_from_slice(&0.05_f32.to_le_bytes()); // weight
        let subs = vec![
            sub(b"EDID", b"Mod5mmRoundHollowPoint\0"),
            sub(b"FULL", b"Hollow Point\0"),
            sub(
                b"DESC",
                b"Increased damage at the cost of reduced DT effectiveness.\0",
            ),
            sub(b"DATA", &data),
        ];
        let m = parse_imod(0x0014_5824, &subs);
        assert_eq!(m.editor_id, "Mod5mmRoundHollowPoint");
        assert_eq!(m.full_name, "Hollow Point");
        assert!(m.description.contains("Increased damage"));
        assert_eq!(m.value, 50);
        assert!((m.weight - 0.05).abs() < 1e-6);
    }
    #[test]
    fn parse_repu_picks_edid_full_base_value() {
        // `NCR` reputation shape: EDID + FULL + DATA(f32).
        let mut data = [0u8; 4];
        data[0..4].copy_from_slice(&1.0_f32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"NCR\0"),
            sub(b"FULL", b"New California Republic\0"),
            sub(b"DATA", &data),
        ];
        let r = parse_repu(0x0011_E662, &subs);
        assert_eq!(r.editor_id, "NCR");
        assert_eq!(r.full_name, "New California Republic");
        assert!((r.base_value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_expl_picks_edid_full_damage_radius() {
        // `FragGrenade` shape: damage at DATA[8..12], radius at [12..16].
        let mut data = [0u8; 16];
        data[8..12].copy_from_slice(&50.0_f32.to_le_bytes()); // damage
        data[12..16].copy_from_slice(&350.0_f32.to_le_bytes()); // radius
        let subs = vec![
            sub(b"EDID", b"FragGrenade\0"),
            sub(b"FULL", b"Frag Grenade\0"),
            sub(b"DATA", &data),
        ];
        let e = parse_expl(0x0006_6EF8, &subs);
        assert_eq!(e.editor_id, "FragGrenade");
        assert!((e.damage - 50.0).abs() < 1e-3);
        assert!((e.radius - 350.0).abs() < 1e-3);
    }
    #[test]
    fn parse_ipct_picks_edid_modl() {
        // `MetalImpactSet` shape: EDID + MODL pointing at the impact
        // particle / decal mesh.
        let subs = vec![
            sub(b"EDID", b"MetalImpactSet\0"),
            sub(b"MODL", b"effects\\impacts\\metal.nif\0"),
        ];
        let i = parse_ipct(0x0007_C0A8, &subs);
        assert_eq!(i.editor_id, "MetalImpactSet");
        assert!(i.model_path.contains("metal.nif"));
    }

    #[test]
    fn parse_ipds_picks_edid_pair_count() {
        // FO3/FNV IPDS DATA is a fixed 96-byte array of 12 × 8-byte
        // (material_kind, ipct_form_id) entries. Stub captures the
        // pair count for sanity-check.
        let data = [0u8; 96];
        let subs = vec![sub(b"EDID", b"GenericImpactDataSet\0"), sub(b"DATA", &data)];
        let i = parse_ipds(0x0006_E1F8, &subs);
        assert_eq!(i.editor_id, "GenericImpactDataSet");
        assert_eq!(i.material_pair_count, 12);
    }
    #[test]
    fn parse_repu_short_data_is_tolerated() {
        // No DATA → base_value defaults to 0.0.
        let subs = vec![
            sub(b"EDID", b"PowderGangers\0"),
            sub(b"FULL", b"Powder Gangers\0"),
        ];
        let r = parse_repu(0x0011_E664, &subs);
        assert_eq!(r.editor_id, "PowderGangers");
        assert_eq!(r.base_value, 0.0);
    }

    // ── #810 / FNV-D2-NEW-03 — minimal-stub regression guards ─────
}
