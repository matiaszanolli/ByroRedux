//! Item record parsers — WEAP, ARMO, AMMO, ALCH, MISC, INGR, BOOK, NOTE, KEYM.
//!
//! Every item record shares the same set of "this is a thing in the world"
//! fields (`editor_id`, `full_name`, `model_path`, `value`, `weight`) plus a
//! type-specific data block. We collapse them all into a single `ItemRecord`
//! whose `kind: ItemKind` enum carries the type-specific stats.
//!
//! Field selection is intentionally minimal — just what gameplay systems need.
//! Adding more fields later is straightforward; the parsers walk sub-records
//! by 4-char code and ignore anything they don't recognize.

use super::common::{read_f32_at, read_u16_at, read_u32_at, CommonItemFields};
use crate::esm::reader::{GameKind, SubRecord};

/// What kind of item this is, with kind-specific stats.
#[derive(Debug, Clone)]
pub enum ItemKind {
    /// MISC: weight + value already on the parent record. No extra fields.
    Misc,
    /// BOOK: notes, skill book teach data.
    Book {
        /// Skill bonus form ID (AVIF) when this is a skill book; 0 for plain books.
        teaches_skill: u32,
        /// Bonus value (typically +1 per skill book in FNV).
        skill_bonus: u8,
        /// Flags (0x01 = scroll, 0x02 = can't be taken).
        flags: u8,
    },
    /// NOTE: holotape / written note. Type tells you sound vs text vs voice.
    Note {
        /// 0 = sound, 1 = text, 2 = image, 3 = voice.
        note_type: u8,
        /// Form ID of attached SOUN/TXT.
        topic_form: u32,
    },
    /// INGR: edible ingredient (rare in FNV; common in older TES games).
    Ingredient { magic_effects: Vec<u32> },
    /// ALCH: consumable / aid (Stimpaks, food, drugs). FNV uses this for nearly
    /// all consumables.
    Aid {
        magic_effects: Vec<u32>,
        addiction_chance: f32,
    },
    /// KEYM: key — same as MISC but the engine treats it specially.
    Key,
    /// AMMO: ammunition (rounds, cells, etc.).
    Ammo {
        damage: f32,
        /// Damage modifier vs DT.
        dt_mult: f32,
        /// Spread modifier (rad).
        spread: f32,
        /// Form ID of casing item left after firing.
        casing_form: u32,
        /// Clip rounds (FNV: usually 0 — meaningful per WEAP).
        clip_rounds: u8,
    },
    /// ARMO: armor or clothing.
    Armor {
        /// Body parts covered (bitfield). Source varies by game: FO3/FNV
        /// use BMDT low-u32 as biped slots; Skyrim uses BOD2 u32 biped
        /// slots directly.
        biped_flags: u32,
        /// Damage Threshold (FO3/FNV) — flat damage absorbed. 0 on Skyrim
        /// and later (game uses `armor_rating` instead).
        dt: f32,
        /// Damage Resistance (FO3/FNV) — percentage absorbed. 0 on Skyrim+.
        dr: u32,
        /// Health / condition durability (FO3/FNV only; Skyrim dropped it).
        health: u32,
        /// Equip slot mask (FO3/FNV-only biped-flag low 16 bits).
        slot_mask: u16,
        /// Skyrim+: armor rating × 100 from DNAM. 0 on FO3/FNV (use dt/dr).
        armor_rating_x100: u32,
        /// Skyrim+: armor type from BOD2 second u32 (0=light, 1=clothing,
        /// 2=heavy). `None` on pre-Skyrim games.
        armor_type: Option<u32>,
    },
    /// WEAP: weapon.
    Weapon {
        /// Form ID of ammo this weapon uses.
        ammo_form: u32,
        /// Base damage per shot.
        damage: u32,
        /// Magazine capacity.
        clip_size: u8,
        /// Animation type (0 = handgun, 1 = rifle, 2 = launcher, ...).
        anim_type: u8,
        /// Action point cost (VATS).
        ap_cost: u32,
        /// Skill required form (AVIF).
        skill_form: u32,
        /// Min spread (radians).
        min_spread: f32,
        /// Spread (radians).
        spread: f32,
        /// Critical chance multiplier.
        crit_mult: f32,
        /// Reload animation (0..n).
        reload_anim: u8,
    },
}

impl ItemKind {
    /// Type-name string for diagnostics / counting.
    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Misc => "MISC",
            ItemKind::Book { .. } => "BOOK",
            ItemKind::Note { .. } => "NOTE",
            ItemKind::Ingredient { .. } => "INGR",
            ItemKind::Aid { .. } => "ALCH",
            ItemKind::Key => "KEYM",
            ItemKind::Ammo { .. } => "AMMO",
            ItemKind::Armor { .. } => "ARMO",
            ItemKind::Weapon { .. } => "WEAP",
        }
    }
}

/// Parsed item record. The `common` block holds fields shared across types;
/// `kind` carries the type-specific stats.
#[derive(Debug, Clone)]
pub struct ItemRecord {
    pub form_id: u32,
    pub common: CommonItemFields,
    pub kind: ItemKind,
}

// ── Per-type parsers ──────────────────────────────────────────────────

pub fn parse_weap(form_id: u32, subs: &[SubRecord], game: GameKind) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut ammo_form = 0u32;
    let mut damage = 0u32;
    let mut clip_size = 0u8;
    let mut anim_type = 0u8;
    let mut ap_cost = 0u32;
    let mut skill_form = 0u32;
    let mut min_spread = 0.0f32;
    let mut spread = 0.0f32;
    let mut crit_mult = 1.0f32;
    let mut reload_anim = 0u8;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => match game {
                // FO3/FNV WEAP DATA (16 bytes): value(i32), health(i32),
                // weight(f32), damage(i16), clip(u8) + pad.
                GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Fallout4 => {
                    if sub.data.len() >= 16 {
                        common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
                        let _health = read_u32_at(&sub.data, 4).unwrap_or(0);
                        common.weight = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                        damage = read_u16_at(&sub.data, 12).unwrap_or(0) as u32;
                        clip_size = sub.data.get(14).copied().unwrap_or(0);
                    }
                }
                // Skyrim WEAP DATA (10 bytes): value(u32), weight(f32),
                // damage(u16). No health, no clip. Skyrim dropped the
                // condition/durability system and clip lives in DNAM.
                GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                    if sub.data.len() >= 10 {
                        common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
                        common.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
                        damage = read_u16_at(&sub.data, 8).unwrap_or(0) as u32;
                    }
                }
            },
            // DNAM is a large, version-dependent stats blob. The FO3/FNV
            // layout starts with anim_type(u8) and places ap_cost/min_spread
            // at fixed offsets. Skyrim rewrote the whole blob (~100 bytes,
            // different field positions); extracting per-field values for
            // Skyrim requires a separate layout walk that isn't wired yet.
            // For Skyrim we leave these fields at their zero defaults.
            b"DNAM" if matches!(game, GameKind::Fallout3NV | GameKind::Oblivion) => {
                if sub.data.len() >= 24 {
                    anim_type = sub.data[0];
                    ap_cost = read_u32_at(&sub.data, 16).unwrap_or(0);
                    min_spread = read_f32_at(&sub.data, 20).unwrap_or(0.0);
                }
            }
            b"ANAM" => reload_anim = sub.data.first().copied().unwrap_or(0),
            // Ammunition reference (FO3/FNV). Skyrim uses ETYP for ammo
            // *type* and per-arrow NAM7; not yet decoded.
            b"AMMO" if sub.data.len() >= 4 => {
                ammo_form = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"DESC" => {} // description string (we don't store it yet)
            b"ETYP" if sub.data.len() >= 4 => {
                skill_form = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            b"CRDT" if sub.data.len() >= 8 => {
                // Critical data: chance(u16), unused(u16), mult(f32). Shared
                // shape across FO3/FNV; Skyrim extends with extra tail
                // fields but the leading 8 bytes still produce a sane mult.
                crit_mult = read_f32_at(&sub.data, 4).unwrap_or(1.0);
            }
            b"NAM6" if sub.data.len() >= 4 => {
                // Spread (FO3/FNV). Skyrim doesn't emit NAM6.
                spread = read_f32_at(&sub.data, 0).unwrap_or(0.0);
            }
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Weapon {
            ammo_form,
            damage,
            clip_size,
            anim_type,
            ap_cost,
            skill_form,
            min_spread,
            spread,
            crit_mult,
            reload_anim,
        },
    }
}

pub fn parse_armo(form_id: u32, subs: &[SubRecord], game: GameKind) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut biped_flags = 0u32;
    let mut dt = 0.0f32;
    let mut dr = 0u32;
    let mut health = 0u32;
    let mut slot_mask = 0u16;
    let mut armor_rating_x100 = 0u32;
    let mut armor_type: Option<u32> = None;

    for sub in subs {
        match &sub.sub_type {
            // BMDT (FO3/FNV/Oblivion): biped flags (u32) + general flags
            // (u32). Skyrim dropped BMDT in favor of BOD2.
            b"BMDT" if sub.data.len() >= 8 => {
                biped_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                slot_mask = (biped_flags & 0xFFFF) as u16;
            }
            // BOD2 (Skyrim+): biped_slots (u32) + armor_type (u32).
            // armor_type: 0=light, 1=clothing, 2=heavy, 3=none, 4=gauntlets.
            b"BOD2" if sub.data.len() >= 8 => {
                biped_flags = read_u32_at(&sub.data, 0).unwrap_or(0);
                slot_mask = (biped_flags & 0xFFFF) as u16;
                armor_type = Some(read_u32_at(&sub.data, 4).unwrap_or(0));
            }
            b"DATA" => match game {
                // FO3/FNV/Oblivion ARMO DATA (12 bytes):
                //   value(u32), health(u32), weight(f32).
                GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Fallout4 => {
                    if sub.data.len() >= 12 {
                        common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
                        health = read_u32_at(&sub.data, 4).unwrap_or(0);
                        common.weight = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                    }
                }
                // Skyrim+ ARMO DATA (8 bytes):
                //   value(u32), weight(f32). No health — condition/repair
                //   was removed from Skyrim's ARMO data block; equipment
                //   durability lives in the enchantment/tempering system.
                GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                    if sub.data.len() >= 8 {
                        common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
                        common.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
                    }
                }
            },
            b"DNAM" => match game {
                // FO3/FNV/Oblivion ARMO DNAM (8 bytes):
                //   DT (f32), DR (u32).
                GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Fallout4 => {
                    if sub.data.len() >= 8 {
                        dt = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                        dr = read_u32_at(&sub.data, 4).unwrap_or(0);
                    }
                }
                // Skyrim+ ARMO DNAM (4 bytes):
                //   armor_rating × 100 (u32). Skyrim rolled DT/DR into a
                //   single per-armor rating; the × 100 scaling matches the
                //   Creation Kit UI and UESP convention.
                GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                    if sub.data.len() >= 4 {
                        armor_rating_x100 = read_u32_at(&sub.data, 0).unwrap_or(0);
                    }
                }
            },
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Armor {
            biped_flags,
            dt,
            dr,
            health,
            slot_mask,
            armor_rating_x100,
            armor_type,
        },
    }
}

pub fn parse_ammo(form_id: u32, subs: &[SubRecord], game: GameKind) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut damage = 0.0f32;
    let dt_mult = 1.0f32;
    let spread = 0.0f32;
    let mut casing_form = 0u32;
    let mut clip_rounds = 0u8;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => match game {
                // FO3/FNV AMMO DATA (13+ bytes): speed(f32), flags(u8),
                // pad(u8)×3, value(u32), clipRounds(u8).
                GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Fallout4 => {
                    if sub.data.len() >= 13 {
                        let _speed = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                        common.value = read_u32_at(&sub.data, 8).unwrap_or(0);
                        clip_rounds = sub.data.get(12).copied().unwrap_or(0);
                    }
                }
                // Skyrim AMMO DATA (16 bytes): projectile_form(u32),
                // flags(u32), damage(f32), value(u32). "Ignores weapon
                // resistance" etc. live in the flags bitfield.
                GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                    if sub.data.len() >= 16 {
                        casing_form = read_u32_at(&sub.data, 0).unwrap_or(0);
                        let _flags = read_u32_at(&sub.data, 4).unwrap_or(0);
                        damage = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                        common.value = read_u32_at(&sub.data, 12).unwrap_or(0);
                    }
                }
            },
            // DAT2 (FO3/FNV only): projPerShot(u32), proj(formID),
            // weight(f32), consumedAmmo(formID), consumedPercentage(f32).
            // Skyrim doesn't emit DAT2.
            b"DAT2" if matches!(game, GameKind::Fallout3NV | GameKind::Oblivion)
                && sub.data.len() >= 16 =>
            {
                let _proj_count = read_u32_at(&sub.data, 0).unwrap_or(0);
                let _proj = read_u32_at(&sub.data, 4).unwrap_or(0);
                common.weight = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                casing_form = read_u32_at(&sub.data, 12).unwrap_or(0);
            }
            // DAMG (rare/legacy FO3).
            b"DAMG" if sub.data.len() >= 4 => {
                damage = read_f32_at(&sub.data, 0).unwrap_or(0.0);
            }
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Ammo {
            damage,
            dt_mult,
            spread,
            casing_form,
            clip_rounds,
        },
    }
}

pub fn parse_misc(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    for sub in subs {
        if &sub.sub_type == b"DATA" && sub.data.len() >= 8 {
            common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
            common.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Misc,
    }
}

pub fn parse_keym(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    for sub in subs {
        if &sub.sub_type == b"DATA" && sub.data.len() >= 8 {
            common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
            common.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Key,
    }
}

pub fn parse_alch(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut magic_effects = Vec::new();
    let mut addiction_chance = 0.0f32;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 4 => {
                common.weight = read_f32_at(&sub.data, 0).unwrap_or(0.0);
            }
            b"ENIT" if sub.data.len() >= 8 => {
                // ENIT (FNV ALCH): value(i32), flags(u8), pad(u8)x3, withdrawal(formID),
                // addictChance(f32), consumedSound(formID)
                common.value = read_u32_at(&sub.data, 0).unwrap_or(0);
                if sub.data.len() >= 16 {
                    addiction_chance = read_f32_at(&sub.data, 12).unwrap_or(0.0);
                }
            }
            b"EFID" if sub.data.len() >= 4 => {
                magic_effects.push(read_u32_at(&sub.data, 0).unwrap_or(0));
            }
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Aid {
            magic_effects,
            addiction_chance,
        },
    }
}

pub fn parse_ingr(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let common = CommonItemFields::from_subs(subs);
    let mut magic_effects = Vec::new();
    for sub in subs {
        if &sub.sub_type == b"EFID" && sub.data.len() >= 4 {
            magic_effects.push(read_u32_at(&sub.data, 0).unwrap_or(0));
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Ingredient { magic_effects },
    }
}

pub fn parse_book(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut teaches_skill = 0u32;
    let mut skill_bonus = 0u8;
    let mut flags = 0u8;

    for sub in subs {
        match &sub.sub_type {
            // DATA (FNV BOOK): flags(u8), skill(byte=AVIF index), value(i32), weight(f32)
            b"DATA" if sub.data.len() >= 10 => {
                flags = sub.data[0];
                skill_bonus = sub.data[1];
                common.value = read_u32_at(&sub.data, 2).unwrap_or(0);
                common.weight = read_f32_at(&sub.data, 6).unwrap_or(0.0);
            }
            b"SKIL" if sub.data.len() >= 4 => {
                teaches_skill = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Book {
            teaches_skill,
            skill_bonus,
            flags,
        },
    }
}

pub fn parse_note(form_id: u32, subs: &[SubRecord]) -> ItemRecord {
    let mut common = CommonItemFields::from_subs(subs);
    let mut note_type = 0u8;
    let mut topic_form = 0u32;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" if !sub.data.is_empty() => {
                note_type = sub.data[0];
                if sub.data.len() >= 8 {
                    common.weight = read_f32_at(&sub.data, 4).unwrap_or(0.0);
                }
            }
            b"SNAM" if sub.data.len() >= 4 => {
                topic_form = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }

    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Note {
            note_type,
            topic_form,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    fn build_data_weap(value: u32, weight: f32, damage: u16, clip: u8) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&value.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // health
        d.extend_from_slice(&weight.to_le_bytes());
        d.extend_from_slice(&damage.to_le_bytes());
        d.push(clip);
        d.push(0); // pad
        d
    }

    #[test]
    fn weap_extracts_common_and_kind_fields() {
        let subs = vec![
            sub(b"EDID", b"WeapTest\0"),
            sub(b"FULL", b"Test Pistol\0"),
            sub(b"MODL", b"meshes\\weapons\\pistol.nif\0"),
            sub(b"DATA", &build_data_weap(250, 1.5, 12, 8)),
            sub(b"AMMO", &0xDEADBEEFu32.to_le_bytes()),
        ];
        let item = parse_weap(0x100, &subs, GameKind::Fallout3NV);
        assert_eq!(item.form_id, 0x100);
        assert_eq!(item.common.editor_id, "WeapTest");
        assert_eq!(item.common.full_name, "Test Pistol");
        assert_eq!(item.common.model_path, "meshes\\weapons\\pistol.nif");
        assert_eq!(item.common.value, 250);
        assert!((item.common.weight - 1.5).abs() < 1e-6);
        match item.kind {
            ItemKind::Weapon {
                ammo_form,
                damage,
                clip_size,
                ..
            } => {
                assert_eq!(ammo_form, 0xDEADBEEF);
                assert_eq!(damage, 12);
                assert_eq!(clip_size, 8);
            }
            _ => panic!("expected Weapon kind"),
        }
    }

    #[test]
    fn armo_extracts_dt_and_biped_flags() {
        let mut bmdt = Vec::new();
        bmdt.extend_from_slice(&0x00000004u32.to_le_bytes()); // biped (Hair flag for example)
        bmdt.extend_from_slice(&0u32.to_le_bytes()); // general flags
        let mut data = Vec::new();
        data.extend_from_slice(&500u32.to_le_bytes()); // value
        data.extend_from_slice(&100u32.to_le_bytes()); // health
        data.extend_from_slice(&5.0f32.to_le_bytes()); // weight
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&15.5f32.to_le_bytes()); // DT
        dnam.extend_from_slice(&30u32.to_le_bytes()); // DR

        let subs = vec![
            sub(b"EDID", b"ArmorTest\0"),
            sub(b"BMDT", &bmdt),
            sub(b"DATA", &data),
            sub(b"DNAM", &dnam),
        ];
        let item = parse_armo(0x200, &subs, GameKind::Fallout3NV);
        match item.kind {
            ItemKind::Armor {
                biped_flags,
                dt,
                dr,
                health,
                armor_rating_x100,
                armor_type,
                ..
            } => {
                assert_eq!(biped_flags, 0x00000004);
                assert!((dt - 15.5).abs() < 1e-6);
                assert_eq!(dr, 30);
                assert_eq!(health, 100);
                assert_eq!(armor_rating_x100, 0);
                assert_eq!(armor_type, None);
            }
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn misc_extracts_value_and_weight() {
        let mut data = Vec::new();
        data.extend_from_slice(&15u32.to_le_bytes());
        data.extend_from_slice(&0.25f32.to_le_bytes());
        let subs = vec![sub(b"EDID", b"M\0"), sub(b"DATA", &data)];
        let item = parse_misc(0x300, &subs);
        assert_eq!(item.common.value, 15);
        assert!((item.common.weight - 0.25).abs() < 1e-6);
        assert!(matches!(item.kind, ItemKind::Misc));
    }

    // ── Skyrim regression guards (issue #347 / S6-02) ──────────────────

    #[test]
    fn skyrim_weap_data_is_10_bytes_no_health_no_clip() {
        // Skyrim WEAP DATA: value(u32) + weight(f32) + damage(u16).
        // Same 10 bytes that would be the *first 10* of an FO3/FNV DATA,
        // but with different field meanings — FNV's byte 12 is the u16
        // damage; Skyrim's byte 8 is. Confirms the dispatch picks the
        // right layout.
        let mut data = Vec::new();
        data.extend_from_slice(&1000u32.to_le_bytes()); // value
        data.extend_from_slice(&12.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&17u16.to_le_bytes()); // damage
        let subs = vec![sub(b"EDID", b"IronSword\0"), sub(b"DATA", &data)];
        let item = parse_weap(0x12EB7, &subs, GameKind::Skyrim);
        assert_eq!(item.common.value, 1000);
        assert!((item.common.weight - 12.0).abs() < 1e-6);
        match item.kind {
            ItemKind::Weapon {
                damage, clip_size, ..
            } => {
                assert_eq!(damage, 17, "Skyrim damage at byte 8, not 12");
                assert_eq!(clip_size, 0, "Skyrim has no clip field");
            }
            _ => panic!("expected Weapon kind"),
        }
    }

    #[test]
    fn skyrim_armo_uses_bod2_and_4byte_dnam() {
        // BOD2: biped slots u32 + armor type u32.
        let mut bod2 = Vec::new();
        bod2.extend_from_slice(&0x00004000u32.to_le_bytes()); // slot 30 / body
        bod2.extend_from_slice(&2u32.to_le_bytes()); // armor_type = heavy
                                                     // DATA (Skyrim): value + weight only.
        let mut data = Vec::new();
        data.extend_from_slice(&1250u32.to_le_bytes()); // value
        data.extend_from_slice(&35.0f32.to_le_bytes()); // weight
                                                        // DNAM: armor rating × 100 as u32.
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&4000u32.to_le_bytes()); // armor_rating = 40.00

        let subs = vec![
            sub(b"EDID", b"ArmorSteelCuirass\0"),
            sub(b"BOD2", &bod2),
            sub(b"DATA", &data),
            sub(b"DNAM", &dnam),
        ];
        let item = parse_armo(0x13938, &subs, GameKind::Skyrim);
        assert_eq!(item.common.value, 1250);
        assert!((item.common.weight - 35.0).abs() < 1e-6);
        match item.kind {
            ItemKind::Armor {
                biped_flags,
                dt,
                dr,
                health,
                armor_rating_x100,
                armor_type,
                ..
            } => {
                assert_eq!(biped_flags, 0x00004000);
                assert_eq!(armor_type, Some(2));
                assert_eq!(armor_rating_x100, 4000);
                // Pre-Skyrim fields must remain zero on Skyrim records.
                assert_eq!(dt, 0.0, "Skyrim ARMO has no DT");
                assert_eq!(dr, 0, "Skyrim ARMO has no DR");
                assert_eq!(health, 0, "Skyrim ARMO has no health");
            }
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn skyrim_armo_with_fnv_layout_produces_zeros_not_garbage() {
        // Guard against the pre-fix behavior: feeding a Skyrim-shaped 8-byte
        // DATA to the FO3/FNV parser produced zeroed `weight` and
        // garbage-read DNAM fields. With the GameKind dispatch, the Skyrim
        // path reads the 8-byte DATA cleanly, and pre-Skyrim fields stay 0.
        let mut data = Vec::new();
        data.extend_from_slice(&500u32.to_le_bytes());
        data.extend_from_slice(&10.0f32.to_le_bytes());
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&2500u32.to_le_bytes()); // armor_rating_x100
        let subs = vec![sub(b"DATA", &data), sub(b"DNAM", &dnam)];
        let item = parse_armo(0x1, &subs, GameKind::Skyrim);
        match item.kind {
            ItemKind::Armor {
                dt,
                dr,
                armor_rating_x100,
                ..
            } => {
                assert_eq!(dt, 0.0);
                assert_eq!(dr, 0);
                assert_eq!(armor_rating_x100, 2500);
            }
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn skyrim_ammo_data_is_projectile_form_flags_damage_value() {
        // Skyrim AMMO DATA: projectile_form(u32) + flags(u32) + damage(f32)
        //                 + value(u32). No clipRounds, no speed.
        let mut data = Vec::new();
        data.extend_from_slice(&0xC0DEu32.to_le_bytes()); // projectile form
        data.extend_from_slice(&0x1u32.to_le_bytes()); // flags
        data.extend_from_slice(&8.0f32.to_le_bytes()); // damage
        data.extend_from_slice(&1u32.to_le_bytes()); // value
        let subs = vec![sub(b"EDID", b"ArrowIron\0"), sub(b"DATA", &data)];
        let item = parse_ammo(0x139BE, &subs, GameKind::Skyrim);
        assert_eq!(item.common.value, 1);
        match item.kind {
            ItemKind::Ammo {
                damage,
                casing_form,
                clip_rounds,
                ..
            } => {
                assert!((damage - 8.0).abs() < 1e-6);
                assert_eq!(casing_form, 0xC0DE, "projectile_form lands in casing_form slot");
                assert_eq!(clip_rounds, 0);
            }
            _ => panic!("expected Ammo kind"),
        }
    }
}
