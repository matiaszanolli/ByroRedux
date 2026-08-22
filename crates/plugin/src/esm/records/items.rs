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

use super::common::CommonItemFields;
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// What kind of item this is, with kind-specific stats.
#[derive(Debug, Clone)]
pub enum ItemKind {
    /// MISC: weight + value already on the parent record. No extra fields.
    Misc,
    /// Fallout 4 / 76 MISC carrying component rows (`CVPA`). These are shown
    /// under Junk rather than the generic Misc inventory category.
    Junk,
    /// Fallout 4 / 76 loose object-mod item. The inventory object is a MISC
    /// record referenced by an OMOD's `LNAM`; the OMOD definition itself is
    /// not the carried stack.
    Mod {
        /// Whether the underlying MISC record's own `CVPA` classified it
        /// `Junk` before `EsmIndex::classify_fallout_inventory_kinds`
        /// promoted it to `Mod`. Preserved so a later plugin's override
        /// clearing the `OMOD.LNAM` reference (demoting it back out of
        /// `Mod`) restores the record's authored classification instead of
        /// collapsing it to plain `Misc` (#2991).
        was_junk: bool,
    },
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
        /// Form ID of the projectile emitted by this ammunition.
        projectile_form: u32,
        /// Form ID of casing item left after firing.
        casing_form: u32,
        /// Charge / condition carried by the ammunition record (FO4 fusion
        /// cores); zero when the source format has no such field.
        health: u32,
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
        /// Health / condition durability (FO3/FNV/FO4; Skyrim dropped it).
        health: u32,
        /// Equip slot mask (FO3/FNV-only biped-flag low 16 bits).
        slot_mask: u16,
        /// Armor rating × 100. Sourced from DATA on Oblivion, FNAM on FO4,
        /// and DNAM on Skyrim+. 0 on FO3/FNV, which use `dt`/`dr` instead.
        armor_rating_x100: u32,
        /// Skyrim+: armor type from BOD2 second u32 (0=light, 1=clothing,
        /// 2=heavy). `None` on pre-Skyrim games.
        armor_type: Option<u32>,
        /// Skyrim+ Armature RArray — FormID references to ARMA records
        /// that supply the actual worn mesh paths (per-race, per-gender).
        /// Empty on Oblivion / FO3 / FNV (those use `common.model_path`
        /// directly as the worn mesh). Populated from successive `MODL`
        /// sub-records on Skyrim+ ARMO records, where MODL is overloaded
        /// to carry 4-byte FormID payloads instead of path strings.
        armatures: Vec<u32>,
    },
    /// WEAP: weapon.
    Weapon {
        /// Form ID of ammo this weapon uses.
        ammo_form: u32,
        /// Base damage per shot.
        damage: u32,
        /// Magazine capacity.
        clip_size: u16,
        /// Animation type (0 = handgun, 1 = rifle, 2 = launcher, ...).
        anim_type: u8,
        /// Action point cost (VATS).
        ap_cost: f32,
        /// Skill required form (AVIF).
        skill_form: u32,
        /// Min spread (radians).
        min_spread: f32,
        /// Spread (radians).
        spread: f32,
        /// Critical chance multiplier.
        crit_mult: f32,
        /// Melee/attack reach in Bethesda units. `0.0` means "not decoded
        /// for this game" (Skyrim/FO3/FNV/76/Starfield DNAM layouts aren't
        /// mapped yet) — combat.rs treats that as "use the unarmed
        /// fallback constant." Populated for Oblivion (DATA) and FO4
        /// (DNAM offset 12, #2992).
        reach: f32,
        /// Attack speed multiplier (1.0 = authored baseline cadence).
        /// Same `0.0`-means-unset convention as `reach`.
        speed: f32,
        /// Reload animation (0..n).
        reload_anim: u8,
    },
}

impl ItemKind {
    /// Type-name string for diagnostics / counting.
    pub fn label(&self) -> &'static str {
        match self {
            // Junk and Mods are Fallout inventory categories layered over
            // carried MISC records; keep diagnostics truthful about the
            // underlying plugin record type.
            ItemKind::Misc | ItemKind::Junk | ItemKind::Mod { .. } => "MISC",
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

pub fn parse_weap(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut ammo_form = 0u32;
    let mut damage = 0u32;
    let mut clip_size = 0u16;
    let mut anim_type = 0u8;
    let mut ap_cost = 0.0f32;
    let mut skill_form = 0u32;
    let mut min_spread = 0.0f32;
    let mut spread = 0.0f32;
    let mut crit_mult = 1.0f32;
    let mut reload_anim = 0u8;
    let mut reach = 0.0f32;
    let mut speed = 0.0f32;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                match game {
                    // Oblivion WEAP DATA (30 bytes — measured 100% across 1319
                    // records in Oblivion.esm, see #685):
                    //   type(u32) speed(f32) reach(f32) flags(u32)
                    //   value(u32) health(u32) weight(f32) damage(u16)
                    // Oblivion stores all weapon stats in DATA; there is no
                    // DNAM, no AMMO ref (arrows are separate AMMO records),
                    // no clip (no magazine system). `type` (0..5 →
                    // Blade1H/Blade2H/Blunt1H/Blunt2H/Staff/Bow) doesn't
                    // share semantics with FO3/FNV's anim_type — leave
                    // anim_type at its 0 default rather than mix enums.
                    GameKind::Oblivion => {
                        let _type = r.u32_or_default();
                        speed = r.f32_or_default();
                        reach = r.f32_or_default();
                        let _flags = r.u32_or_default();
                        common.value = r.u32_or_default();
                        let _health = r.u32_or_default();
                        common.weight = r.f32_or_default();
                        damage = r.u16_or_default() as u32;
                    }
                    // FO3/FNV WEAP DATA (15 bytes — measured): value(u32),
                    // health(u32), weight(f32), damage(u16), clip(u8).
                    GameKind::Fallout3NV => {
                        common.value = r.u32_or_default();
                        let _health = r.u32_or_default();
                        common.weight = r.f32_or_default();
                        damage = r.u16_or_default() as u32;
                        clip_size = r.u8_or_default() as u16;
                    }
                    // FO4 WEAP has no DATA; all canonical fields are decoded
                    // from its packed 132-byte DNAM below.
                    GameKind::Fallout4 => {}
                    // Skyrim WEAP DATA (10 bytes): value(u32), weight(f32),
                    // damage(u16). No health, no clip. Skyrim dropped the
                    // condition/durability system and clip lives in DNAM.
                    GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                        damage = r.u16_or_default() as u32;
                    }
                }
            }
            // DNAM is a large, version-dependent stats blob present on
            // FO3/FNV/FO4 but absent on Oblivion (which inlines all
            // weapon stats in DATA). The FO3/FNV layout starts with
            // anim_type(u8); Skyrim rewrote the whole blob (~100 bytes,
            // different field positions) and isn't decoded yet.
            b"DNAM" if matches!(game, GameKind::Fallout3NV) => {
                let mut r = SubReader::new(&sub.data);
                anim_type = r.u8_or_default();
                // Bytes 1-15: fields not yet decoded (flags, unknown
                // counters, etc.).
                r.skip_or_eof(15);
                // Offset 16 — Min Spread (f32). Pre-#1342 this was
                // read as `ap_cost: u32`, a type mislabel confirmed by
                // parsing the Varmint Rifle's raw DNAM: the bytes at
                // offset 16 are a float (~0.024), not an integer.
                // `ap_cost`'s true DNAM offset is unconfirmed; it stays
                // at the zero default until the full layout is mapped.
                min_spread = r.f32_or_default(); // offset 16 confirmed
                                                 // Offset 20 — next f32 present in the blob; semantic
                                                 // TBD (may or may not duplicate the NAM6 spread). Not
                                                 // stored; NAM6 remains the authoritative spread source.
                let _offset_20 = r.f32_or_default();
            }
            // FO4 WEAP DNAM is a packed, unaligned 132-byte structure. Keep
            // the game-specific byte layout here at the parser boundary and
            // expose only canonical item fields to gameplay code.
            b"DNAM" if matches!(game, GameKind::Fallout4) => {
                let mut r = SubReader::new(&sub.data);
                ammo_form = r.u32_or_default(); // 0: AMMO FormID
                speed = r.f32_or_default(); // 4
                let _reload_speed = r.f32_or_default(); // 8
                reach = r.f32_or_default(); // 12
                let _min_range = r.f32_or_default(); // 16
                let _max_range = r.f32_or_default(); // 20
                let _attack_delay = r.f32_or_default(); // 24
                let _unused = r.f32_or_default(); // 28
                let _out_of_range_mult = r.f32_or_default(); // 32
                let _on_hit = r.u32_or_default(); // 36

                // 40: Skill and 44: Resist are zero on every vanilla record;
                // consume their published slots without claiming semantics.
                let _skill_form = r.u32_or_default();
                let _resist_form = r.u32_or_default(); // 44
                let _flags = r.u32_or_default(); // 48
                clip_size = r.u16_or_default(); // 52
                anim_type = r.u8_or_default(); // 54
                let _secondary_damage = r.f32_or_default(); // 55
                common.weight = r.f32_or_default(); // 59
                common.value = r.u32_or_default(); // 63
                damage = r.u16_or_default() as u32; // 67

                // Sound level plus eight SNDR FormIDs occupy bytes 69..105.
                r.skip_or_eof(36);
                let _accuracy_bonus = r.u8_or_default(); // 105
                let _attack_seconds = r.f32_or_default(); // 106
                r.skip_or_eof(2); // 110: unknown
                ap_cost = r.f32_or_default(); // 112
            }
            b"ANAM" => {
                reload_anim = SubReader::new(&sub.data).u8_or_default();
            }
            // Ammunition reference (FO3/FNV). Skyrim uses ETYP for ammo
            // *type* and per-arrow NAM7; not yet decoded.
            b"AMMO" => {
                ammo_form = SubReader::new(&sub.data).u32_or_default();
            }
            b"DESC" => {} // description string (we don't store it yet)
            b"ETYP" => {
                skill_form = SubReader::new(&sub.data).u32_or_default();
            }
            b"CRDT" => {
                // Critical data: chance(u16), unused(u16), mult(f32). Shared
                // shape across FO3/FNV; Skyrim extends with extra tail
                // fields but the leading 8 bytes still produce a sane mult.
                let mut r = SubReader::new(&sub.data);
                let _chance = r.u16_or_default();
                let _unused = r.u16_or_default();
                crit_mult = r.f32().unwrap_or(1.0);
            }
            b"NAM6" => {
                // Spread (FO3/FNV). Skyrim doesn't emit NAM6.
                spread = SubReader::new(&sub.data).f32_or_default();
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
            reach,
            speed,
            reload_anim,
        },
    }
}

pub fn parse_armo(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut biped_flags = 0u32;
    let mut dt = 0.0f32;
    let mut dr = 0u32;
    let mut health = 0u32;
    let mut slot_mask = 0u16;
    let mut armor_rating_x100 = 0u32;
    let mut armor_type: Option<u32> = None;
    let mut armatures: Vec<u32> = Vec::new();
    let is_skyrim_or_later = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
    );

    for sub in subs {
        match &sub.sub_type {
            // BMDT shape varies: Oblivion (4 bytes — just biped_flags u32)
            // vs FO3/FNV/FO4 (8 bytes — biped_flags u32 + general_flags
            // u32). Skyrim+ dropped BMDT entirely in favor of BOD2.
            // Measured: Oblivion.esm 996/996 ARMO records use 4-byte BMDT.
            b"BMDT" => {
                biped_flags = SubReader::new(&sub.data).u32_or_default();
                slot_mask = (biped_flags & 0xFFFF) as u16;
            }
            // BOD2 (Skyrim+): biped_slots (u32) + armor_type (u32).
            // armor_type: 0=light, 1=clothing, 2=heavy, 3=none, 4=gauntlets.
            b"BOD2" => {
                let mut r = SubReader::new(&sub.data);
                biped_flags = r.u32_or_default();
                slot_mask = (biped_flags & 0xFFFF) as u16;
                if let Ok(t) = r.u32() {
                    armor_type = Some(t);
                }
            }
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                match game {
                    // Oblivion ARMO DATA (14 bytes — measured 100% across
                    // 996 records, see #686):
                    //   armor(u16) value(u32) health(u32) weight(f32)
                    // armor is rating × 100, same convention as Skyrim's
                    // DNAM (so we route it through `armor_rating_x100` for
                    // a uniform consumer surface). DT/DR didn't exist
                    // pre-Fallout 3.
                    GameKind::Oblivion => {
                        armor_rating_x100 = r.u16_or_default() as u32;
                        common.value = r.u32_or_default();
                        health = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                    // FO3/FNV ARMO DATA (12 bytes):
                    //   value(u32), health(u32), weight(f32).
                    GameKind::Fallout3NV => {
                        common.value = r.u32_or_default();
                        health = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                    // FO4 keeps the same size but swaps the last two fields:
                    // value(u32), weight(f32), health(u32).
                    GameKind::Fallout4 => {
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                        health = r.u32_or_default();
                    }
                    // Skyrim+ ARMO DATA (8 bytes):
                    //   value(u32), weight(f32). No health — condition/repair
                    //   was removed from Skyrim's ARMO data block; equipment
                    //   durability lives in the enchantment/tempering system.
                    GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                }
            }
            // Skyrim+ overloads MODL to carry the Armature RArray —
            // each MODL is a 4-byte FormID pointing at an ARMA record
            // (the actual worn mesh + race-specific bindings live
            // there). On pre-Skyrim games MODL is the worn mesh path
            // string; `CommonItemFields::from_subs` already captured
            // it into `common.model_path`, so we leave it alone there.
            b"MODL" if is_skyrim_or_later => {
                if let Ok(id) = SubReader::new(&sub.data).u32() {
                    armatures.push(id);
                }
            }
            // DNAM exists on FO3/FNV (DT/DR, 8 bytes) and Skyrim+
            // (armor_rating × 100, 4 bytes). FO4 uses FNAM instead;
            // Oblivion has no DNAM —
            // armor rating lives in DATA (handled above).
            b"DNAM" => {
                let mut r = SubReader::new(&sub.data);
                match game {
                    GameKind::Fallout3NV => {
                        dt = r.f32_or_default();
                        dr = r.u32_or_default();
                    }
                    GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                        armor_rating_x100 = r.u32_or_default();
                    }
                    GameKind::Oblivion | GameKind::Fallout4 => {}
                }
            }
            // FO4 stores a raw u16 armor rating in FNAM. Canonicalize it to
            // the ×100 unit already used by Oblivion and Skyrim consumers.
            b"FNAM" if matches!(game, GameKind::Fallout4) => {
                armor_rating_x100 = SubReader::new(&sub.data).u16_or_default() as u32 * 100;
            }
            _ => {}
        }
    }

    // On Skyrim+, the path-string read of MODL by `from_subs` produced
    // garbage (MODL is a 4-byte FormID payload there, not a zstring).
    // Clear it so consumers don't try to load a non-path as a mesh.
    if is_skyrim_or_later {
        common.model_path.clear();
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
            armatures,
        },
    }
}

pub fn parse_ammo(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut damage = 0.0f32;
    let dt_mult = 1.0f32;
    let spread = 0.0f32;
    let mut projectile_form = 0u32;
    let mut casing_form = 0u32;
    let mut health = 0u32;
    let mut clip_rounds = 0u8;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                match game {
                    // Oblivion AMMO DATA (18 bytes — measured 100% across
                    // 128 records, see #691):
                    //   speed(f32) flags(u32) value(u32) weight(f32) damage(u16)
                    // Oblivion arrows carry inline damage (the WEAP "bow"
                    // record's damage is the bow's, not the arrow's). No
                    // clipRounds — magazines arrived with FO3.
                    GameKind::Oblivion => {
                        let _speed = r.f32_or_default();
                        let _flags = r.u32_or_default();
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                        damage = r.u16_or_default() as f32;
                    }
                    // FO3/FNV AMMO DATA (13 bytes): speed(f32), flags(u8),
                    // pad(u8)×3, value(u32), clipRounds(u8).
                    GameKind::Fallout3NV => {
                        let _speed = r.f32_or_default();
                        let _flags_pad = r.u32_or_default();
                        common.value = r.u32_or_default();
                        clip_rounds = r.u8_or_default();
                    }
                    // FO4 AMMO DATA (8 bytes): value(u32), weight(f32).
                    GameKind::Fallout4 => {
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                    // Skyrim AMMO DATA (16 bytes): projectile_form(u32),
                    // flags(u32), damage(f32), value(u32). "Ignores weapon
                    // resistance" etc. live in the flags bitfield.
                    GameKind::Skyrim | GameKind::Fallout76 | GameKind::Starfield => {
                        projectile_form = r.u32_or_default();
                        let _flags = r.u32_or_default();
                        damage = r.f32_or_default();
                        common.value = r.u32_or_default();
                    }
                }
            }
            // DAT2 (FO3/FNV only): projPerShot(u32), proj(formID),
            // weight(f32), consumedAmmo(formID), consumedPercentage(f32).
            // Oblivion stores weight inline in DATA (handled above);
            // Skyrim doesn't emit DAT2.
            b"DAT2" if matches!(game, GameKind::Fallout3NV) => {
                let mut r = SubReader::new(&sub.data);
                let _proj_count = r.u32_or_default();
                projectile_form = r.u32_or_default();
                common.weight = r.f32_or_default();
                casing_form = r.u32_or_default();
            }
            // FO4 AMMO DNAM: projectile(FormID), flags(u8), pad[3],
            // damage(f32), health(u32). Damage intentionally remains zero:
            // FO4 authors per-shot damage on WEAP rather than AMMO.
            b"DNAM" if matches!(game, GameKind::Fallout4) => {
                let mut r = SubReader::new(&sub.data);
                projectile_form = r.u32_or_default();
                let _flags = r.u8_or_default();
                r.skip_or_eof(3);
                let _unused_damage = r.f32_or_default();
                health = r.u32_or_default();
            }
            // DAMG (rare/legacy FO3).
            b"DAMG" => {
                damage = SubReader::new(&sub.data).f32_or_default();
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
            projectile_form,
            casing_form,
            health,
            clip_rounds,
        },
    }
}

pub fn parse_misc(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut has_components = false;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                common.value = r.u32_or_default();
                common.weight = r.f32_or_default();
            }
            // FO4 / FO76 MISC CVPA is an array of
            // `(component FormID: u32, count: u32)` rows. Its presence is
            // the authored distinction between junk that can be broken down
            // at a workbench and an ordinary miscellaneous item.
            b"CVPA" if sub.data.len() >= 8 => has_components = true,
            _ => {}
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: if has_components && matches!(game, GameKind::Fallout4 | GameKind::Fallout76) {
            ItemKind::Junk
        } else {
            ItemKind::Misc
        },
    }
}

/// Extract the carried MISC record referenced by a Fallout 4 / 76 OMOD.
///
/// OMOD is the modification definition; `LNAM` is its optional "Loose Mod"
/// base object. Inventory stacks contain that loose object, so callers keep a
/// reverse classification map and promote the corresponding MISC item to
/// [`ItemKind::Mod`] after the complete plugin has been walked.
pub fn parse_omod_loose_item(subs: &[SubRecord], remap: &Option<FormIdRemap>) -> u32 {
    subs.iter()
        .find(|sub| &sub.sub_type == b"LNAM" && sub.data.len() >= 4)
        .map(|sub| SubReader::new(&sub.data).u32_or_default())
        .map(|raw| remap.as_ref().map_or(raw, |mapping| mapping.remap(raw)))
        .unwrap_or(0)
}

pub fn parse_keym(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    for sub in subs {
        if &sub.sub_type == b"DATA" {
            let mut r = SubReader::new(&sub.data);
            common.value = r.u32_or_default();
            common.weight = r.f32_or_default();
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Key,
    }
}

pub fn parse_alch(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut magic_effects = Vec::new();
    let mut addiction_chance = 0.0f32;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                common.weight = SubReader::new(&sub.data).f32_or_default();
            }
            b"ENIT" => {
                // ENIT (FNV ALCH): value(i32), flags(u8), pad(u8)x3, withdrawal(formID),
                // addictChance(f32), consumedSound(formID)
                let mut r = SubReader::new(&sub.data);
                common.value = r.u32_or_default();
                let _flags_pad = r.u32_or_default();
                let _withdrawal = r.u32_or_default();
                addiction_chance = r.f32_or_default();
            }
            b"EFID" => {
                magic_effects.push(SubReader::new(&sub.data).u32_or_default());
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

pub fn parse_ingr(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ItemRecord {
    let common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut magic_effects = Vec::new();
    for sub in subs {
        if &sub.sub_type == b"EFID" {
            magic_effects.push(SubReader::new(&sub.data).u32_or_default());
        }
    }
    ItemRecord {
        form_id,
        common,
        kind: ItemKind::Ingredient { magic_effects },
    }
}

pub fn parse_book(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut teaches_skill = 0u32;
    let mut skill_bonus = 0u8;
    let mut flags = 0u8;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                match game {
                    // FO4 BOOK DATA (8 bytes): value(u32), weight(f32).
                    // Skill/perk teaching moved to its separate DNAM schema.
                    GameKind::Fallout4 => {
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                    // FNV BOOK DATA (10 bytes): flags(u8), skill(byte=AVIF
                    // index), value(i32), weight(f32). Preserve the existing
                    // decode for the other families until their BOOK schemas
                    // receive dedicated coverage.
                    GameKind::Oblivion
                    | GameKind::Fallout3NV
                    | GameKind::Skyrim
                    | GameKind::Fallout76
                    | GameKind::Starfield => {
                        flags = r.u8_or_default();
                        skill_bonus = r.u8_or_default();
                        common.value = r.u32_or_default();
                        common.weight = r.f32_or_default();
                    }
                }
            }
            b"SKIL" => {
                teaches_skill = SubReader::new(&sub.data).u32_or_default();
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

pub fn parse_note(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ItemRecord {
    let mut common = CommonItemFields::from_subs_with_remap(subs, remap);
    let mut note_type = 0u8;
    let mut topic_form = 0u32;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let mut r = SubReader::new(&sub.data);
                note_type = r.u8_or_default();
                // Pre-cursor: byte 0 = note_type, weight at offset 4.
                // Cursor jumps from byte 1 to byte 4 — 3 padding bytes.
                r.skip_or_eof(3);
                common.weight = r.f32_or_default();
            }
            b"SNAM" => {
                topic_form = SubReader::new(&sub.data).u32_or_default();
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
        let item = parse_weap(0x100, &subs, GameKind::Fallout3NV, &None);
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

    /// Regression for #1342 / D2-01 — DNAM offset 16 must be read as f32
    /// (Min Spread), not u32 (pre-fix `ap_cost` mislabel). The mislabel was
    /// confirmed by inspecting the Varmint Rifle's raw DNAM: the 4 bytes at
    /// offset 16 are a float (~0.024), not an integer. A u32 read would
    /// produce ~3.16 billion, which the struct's `ap_cost` would silently
    /// accept. Verified by seeding a known float and asserting round-trip.
    #[test]
    fn fnv_weap_dnam_reads_min_spread_as_f32_not_u32() {
        let known_min_spread: f32 = 0.024; // typical Varmint Rifle value
        let mut dnam = vec![0u8; 36]; // anim_type + 15 pad + f32 + f32 + tail
        dnam[0] = 2u8; // anim_type = rifle
        dnam[16..20].copy_from_slice(&known_min_spread.to_le_bytes());
        // offset 20: put a sentinel so we'd notice if it were consumed as min_spread
        dnam[20..24].copy_from_slice(&1234.5f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"VarminRifle\0"),
            sub(b"DATA", &build_data_weap(100, 5.5, 18, 5)),
            sub(b"DNAM", &dnam),
        ];
        let item = parse_weap(0x14DCE, &subs, GameKind::Fallout3NV, &None);
        match item.kind {
            ItemKind::Weapon {
                anim_type,
                min_spread,
                ..
            } => {
                assert_eq!(anim_type, 2, "anim_type should round-trip from byte 0");
                assert!(
                    (min_spread - known_min_spread).abs() < 1e-6,
                    "offset 16 must be read as f32 Min Spread; pre-fix u32 read \
                     would give ~3.16 billion, got {min_spread}"
                );
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
        let item = parse_armo(0x200, &subs, GameKind::Fallout3NV, &None);
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
        let item = parse_misc(0x300, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(item.common.value, 15);
        assert!((item.common.weight - 0.25).abs() < 1e-6);
        assert!(matches!(item.kind, ItemKind::Misc));
    }

    #[test]
    fn fallout_component_misc_is_junk_but_legacy_misc_stays_misc() {
        let mut component = Vec::new();
        component.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        component.extend_from_slice(&2u32.to_le_bytes());
        let subs = vec![sub(b"CVPA", &component)];

        let fallout4 = parse_misc(0x301, &subs, GameKind::Fallout4, &None);
        assert!(matches!(&fallout4.kind, ItemKind::Junk));
        assert_eq!(fallout4.kind.label(), "MISC");
        assert!(matches!(
            parse_misc(0x301, &subs, GameKind::Fallout76, &None).kind,
            ItemKind::Junk
        ));
        assert!(matches!(
            parse_misc(0x301, &subs, GameKind::Fallout3NV, &None).kind,
            ItemKind::Misc
        ));
    }

    #[test]
    fn omod_lnam_resolves_the_loose_mod_inventory_item() {
        let subs = vec![sub(b"LNAM", &0x0100_1234u32.to_le_bytes())];
        let remap = Some(FormIdRemap::regular(2, vec![0]));

        assert_eq!(parse_omod_loose_item(&subs, &remap), 0x0200_1234);
        assert_eq!(parse_omod_loose_item(&[], &remap), 0);
    }

    // ── Fallout 4 item-schema regression guards (#2992–#2996) ──────────

    #[test]
    fn fo4_weap_dnam_decodes_packed_canonical_stats() {
        // Ammo10mm / 10mm Pistol anchors from Fallout4.esm. The packed
        // structure deliberately places floats and integers unaligned.
        let mut dnam = vec![0u8; 132];
        dnam[0..4].copy_from_slice(&0x0001_F276u32.to_le_bytes()); // ammo
        dnam[4..8].copy_from_slice(&1.25f32.to_le_bytes()); // speed
        dnam[12..16].copy_from_slice(&64.0f32.to_le_bytes()); // reach
        dnam[40..44].copy_from_slice(&0x0000_1234u32.to_le_bytes()); // unverified slot
        dnam[52..54].copy_from_slice(&12u16.to_le_bytes()); // capacity
        dnam[54] = 9; // Gun
        dnam[59..63].copy_from_slice(&3.0f32.to_le_bytes()); // weight
        dnam[63..67].copy_from_slice(&45u32.to_le_bytes()); // value
        dnam[67..69].copy_from_slice(&18u16.to_le_bytes()); // base damage
        dnam[112..116].copy_from_slice(&28.0f32.to_le_bytes()); // AP cost

        let item = parse_weap(
            0x0000_1234,
            &[sub(b"DNAM", &dnam)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(item.common.value, 45);
        assert!((item.common.weight - 3.0).abs() < 1e-6);
        match item.kind {
            ItemKind::Weapon {
                ammo_form,
                damage,
                clip_size,
                anim_type,
                ap_cost,
                skill_form,
                reach,
                speed,
                ..
            } => {
                assert_eq!(ammo_form, 0x0001_F276);
                assert_eq!(damage, 18);
                assert_eq!(clip_size, 12);
                assert_eq!(anim_type, 9);
                assert!((ap_cost - 28.0).abs() < 1e-6);
                assert_eq!(skill_form, 0, "unverified FO4 Skill slot stays unencoded");
                assert!(
                    (speed - 1.25).abs() < 1e-6,
                    "speed at DNAM offset 4 (#3096)"
                );
                assert!(
                    (reach - 64.0).abs() < 1e-6,
                    "reach at DNAM offset 12 (#3096)"
                );
            }
            _ => panic!("expected Weapon kind"),
        }
    }

    #[test]
    fn fo4_armo_data_and_fnam_keep_field_order_and_units() {
        let mut data = Vec::new();
        data.extend_from_slice(&25u32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        data.extend_from_slice(&750u32.to_le_bytes());
        let fnam = 3u16.to_le_bytes();
        let item = parse_armo(
            0x0000_1234,
            &[sub(b"DATA", &data), sub(b"FNAM", &fnam)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(item.common.value, 25);
        assert!((item.common.weight - 5.0).abs() < 1e-6);
        match item.kind {
            ItemKind::Armor {
                health,
                armor_rating_x100,
                dt,
                dr,
                ..
            } => {
                assert_eq!(health, 750);
                assert_eq!(armor_rating_x100, 300);
                assert_eq!(dt, 0.0);
                assert_eq!(dr, 0);
            }
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn fo4_ammo_uses_data_and_dnam_layouts() {
        let mut data = Vec::new();
        data.extend_from_slice(&200u32.to_le_bytes());
        data.extend_from_slice(&4.0f32.to_le_bytes());
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&0x0000_C0DEu32.to_le_bytes());
        dnam.extend_from_slice(&[1, 0, 0, 0]);
        dnam.extend_from_slice(&99.0f32.to_le_bytes());
        dnam.extend_from_slice(&500u32.to_le_bytes());
        let item = parse_ammo(
            0x0000_1234,
            &[sub(b"DATA", &data), sub(b"DNAM", &dnam)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(item.common.value, 200);
        assert!((item.common.weight - 4.0).abs() < 1e-6);
        match item.kind {
            ItemKind::Ammo {
                damage,
                projectile_form,
                health,
                ..
            } => {
                assert_eq!(damage, 0.0, "FO4 damage belongs to WEAP");
                assert_eq!(projectile_form, 0x0000_C0DE);
                assert_eq!(health, 500);
            }
            _ => panic!("expected Ammo kind"),
        }
    }

    #[test]
    fn fo4_book_data_is_value_then_weight() {
        let mut data = Vec::new();
        data.extend_from_slice(&125u32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        let item = parse_book(
            0x0000_1234,
            &[sub(b"DATA", &data)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(item.common.value, 125);
        assert!((item.common.weight - 0.5).abs() < 1e-6);
        assert!(matches!(
            item.kind,
            ItemKind::Book {
                teaches_skill: 0,
                skill_bonus: 0,
                flags: 0
            }
        ));
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
        let item = parse_weap(0x12EB7, &subs, GameKind::Skyrim, &None);
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
        let item = parse_armo(0x13938, &subs, GameKind::Skyrim, &None);
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
        let item = parse_armo(0x1, &subs, GameKind::Skyrim, &None);
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

    // ── Oblivion regression guards (issues #685 / #686 / #691) ─────────
    //
    // Sample byte sequences captured directly from Oblivion.esm via
    // crates/plugin/examples/dump_item_data_sizes.rs. Anchoring tests in
    // real on-disk bytes prevents the audit-cycle that produced the
    // original audit's incorrect "15-byte WEAP / 16-byte ARMO" claims:
    // the actual Oblivion shapes are 30 / 14 / 18.

    #[test]
    fn oblivion_weap_data_is_30_bytes_with_value_at_16() {
        // SE13TrophySword1 (form 0x000966A9): a Blade2H trophy sword.
        // Layout: type(u32) speed(f32) reach(f32) flags(u32)
        //         value(u32) health(u32) weight(f32) damage(u16)
        // Decoded from disk: type=1, speed=0.8, reach=1.3, flags=1,
        //                    value=500, health=300, weight=35.0, damage=14.
        let data = [
            0x01, 0x00, 0x00, 0x00, // type = Blade2H
            0xcd, 0xcc, 0x4c, 0x3f, // speed = 0.8
            0x66, 0x66, 0xa6, 0x3f, // reach = 1.3
            0x01, 0x00, 0x00, 0x00, // flags
            0xf4, 0x01, 0x00, 0x00, // value = 500
            0x2c, 0x01, 0x00, 0x00, // health = 300
            0x00, 0x00, 0x0c, 0x42, // weight = 35.0
            0x0e, 0x00, // damage = 14
        ];
        assert_eq!(data.len(), 30);
        let subs = vec![sub(b"EDID", b"SE13TrophySword1\0"), sub(b"DATA", &data)];
        let item = parse_weap(0x000966A9, &subs, GameKind::Oblivion, &None);
        assert_eq!(item.common.value, 500, "value at offset 16, not 0");
        assert!(
            (item.common.weight - 35.0).abs() < 1e-6,
            "weight at offset 24, not 8"
        );
        match item.kind {
            ItemKind::Weapon {
                damage,
                clip_size,
                reach,
                speed,
                ..
            } => {
                assert_eq!(damage, 14, "damage u16 at offset 28");
                assert_eq!(clip_size, 0, "Oblivion has no magazine system");
                assert!(
                    (speed - 0.8).abs() < 1e-6,
                    "speed at offset 4 (#3096 — previously discarded)"
                );
                assert!(
                    (reach - 1.3).abs() < 1e-6,
                    "reach at offset 8 (#3096 — previously discarded)"
                );
            }
            _ => panic!("expected Weapon kind"),
        }
    }

    #[test]
    fn oblivion_weap_with_fnv_layout_would_corrupt_every_field() {
        // Guard against the pre-fix behavior: an Oblivion DATA fed to
        // the FO3/FNV arm would read `value` from offset 0 (which is
        // the WEAP `type` u32), `weight` from offset 8 (which is the
        // upper bytes of `reach`), etc. Demonstrate that the per-game
        // dispatch now keeps these distinct.
        let oblivion_data = [
            0x01, 0x00, 0x00, 0x00, // type
            0xcd, 0xcc, 0x4c, 0x3f, // speed
            0x66, 0x66, 0xa6, 0x3f, // reach
            0x01, 0x00, 0x00, 0x00, // flags
            0xf4, 0x01, 0x00, 0x00, // value = 500
            0x2c, 0x01, 0x00, 0x00, // health
            0x00, 0x00, 0x0c, 0x42, // weight = 35.0
            0x0e, 0x00, // damage
        ];
        let subs = vec![sub(b"DATA", &oblivion_data)];
        let oblivion = parse_weap(0x1, &subs, GameKind::Oblivion, &None);
        let fnv = parse_weap(0x1, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(oblivion.common.value, 500);
        assert_ne!(
            fnv.common.value, 500,
            "FO3/FNV arm reads `type` as `value` — confirms separation matters"
        );
    }

    #[test]
    fn oblivion_armo_data_is_14_bytes_armor_u16_then_value_health_weight() {
        // SE32CirionsHelmet4 (form 0x000972BB): a Shivering Isles helmet.
        // BMDT is 4 bytes (Oblivion drops the second flags word).
        // DATA is 14 bytes: armor(u16) value(u32) health(u32) weight(f32).
        // Decoded from disk: armor_x100=575 (5.75), value=400, health=775,
        //                    weight=9.8.
        let bmdt = [0x00, 0x00, 0x00, 0x00]; // 4-byte BMDT, biped flags 0 for sample
        let data = [
            0x3f, 0x02, // armor = 575 (= 5.75 × 100)
            0x90, 0x01, 0x00, 0x00, // value = 400
            0x07, 0x03, 0x00, 0x00, // health = 775
            0xcd, 0xcc, 0x1c, 0x41, // weight = 9.8
        ];
        assert_eq!(data.len(), 14);
        assert_eq!(bmdt.len(), 4);
        let subs = vec![
            sub(b"EDID", b"SE32CirionsHelmet4\0"),
            sub(b"BMDT", &bmdt),
            sub(b"DATA", &data),
        ];
        let item = parse_armo(0x000972BB, &subs, GameKind::Oblivion, &None);
        assert_eq!(
            item.common.value, 400,
            "value at offset 2 (after armor u16)"
        );
        assert!(
            (item.common.weight - 9.8).abs() < 1e-4,
            "weight at offset 10, not 8"
        );
        match item.kind {
            ItemKind::Armor {
                health,
                armor_rating_x100,
                dt,
                dr,
                ..
            } => {
                assert_eq!(armor_rating_x100, 575, "armor u16 at offset 0");
                assert_eq!(health, 775);
                assert_eq!(dt, 0.0, "Oblivion has no DT/DR");
                assert_eq!(dr, 0);
            }
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn oblivion_armo_4byte_bmdt_no_longer_drops_biped_flags() {
        // Pre-fix the parser required `len >= 8` on BMDT, so Oblivion's
        // 4-byte BMDT silently dropped biped_flags entirely. Guard that
        // a 4-byte BMDT is now accepted on Oblivion records.
        let bmdt = [0x04, 0x00, 0x00, 0x00]; // biped_flags = Hair (bit 2)
        let data = [
            0x00, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let subs = vec![sub(b"BMDT", &bmdt), sub(b"DATA", &data)];
        let item = parse_armo(0x1, &subs, GameKind::Oblivion, &None);
        match item.kind {
            ItemKind::Armor { biped_flags, .. } => assert_eq!(biped_flags, 0x4),
            _ => panic!("expected Armor kind"),
        }
    }

    #[test]
    fn starfield_armo_modl_is_resolved_as_arma_form_id() {
        let arma_form_id = 0x0012_3456u32;
        let subs = vec![
            sub(b"EDID", b"StarfieldArmor\0"),
            sub(b"MODL", &arma_form_id.to_le_bytes()),
        ];
        let item = parse_armo(0x0000_0042, &subs, GameKind::Starfield, &None);
        match item.kind {
            ItemKind::Armor { armatures, .. } => {
                assert_eq!(armatures, vec![arma_form_id]);
            }
            _ => panic!("expected Armor kind"),
        }
        assert!(
            item.common.model_path.is_empty(),
            "the fixed-width reference must not become a string path"
        );
    }

    #[test]
    fn oblivion_ammo_data_is_18_bytes_with_damage_not_clip_rounds() {
        // SE30MadnessMagicArrowA (form 0x0009277E): an Oblivion arrow.
        // Layout: speed(f32) flags(u32) value(u32) weight(f32) damage(u16)
        // Decoded from disk: speed=1.0, flags=2, value=2, weight=0.1, damage=9.
        let data = [
            0x00, 0x00, 0x80, 0x3f, // speed = 1.0
            0x00, 0x00, 0x00, 0x00, // flags = 0
            0x02, 0x00, 0x00, 0x00, // value = 2
            0xcd, 0xcc, 0xcc, 0x3d, // weight = 0.1
            0x09, 0x00, // damage = 9
        ];
        assert_eq!(data.len(), 18);
        let subs = vec![
            sub(b"EDID", b"SE30MadnessMagicArrowA\0"),
            sub(b"DATA", &data),
        ];
        let item = parse_ammo(0x0009277E, &subs, GameKind::Oblivion, &None);
        assert_eq!(item.common.value, 2);
        assert!((item.common.weight - 0.1).abs() < 1e-4);
        match item.kind {
            ItemKind::Ammo {
                damage,
                clip_rounds,
                casing_form,
                ..
            } => {
                assert!(
                    (damage - 9.0).abs() < 1e-4,
                    "Oblivion damage at offset 16, u16 cast to f32"
                );
                assert_eq!(clip_rounds, 0, "no magazine system on Oblivion");
                assert_eq!(casing_form, 0, "no projectile/casing on Oblivion AMMO");
            }
            _ => panic!("expected Ammo kind"),
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
        let item = parse_ammo(0x139BE, &subs, GameKind::Skyrim, &None);
        assert_eq!(item.common.value, 1);
        match item.kind {
            ItemKind::Ammo {
                damage,
                projectile_form,
                clip_rounds,
                ..
            } => {
                assert!((damage - 8.0).abs() < 1e-6);
                assert_eq!(
                    projectile_form, 0xC0DE,
                    "Skyrim projectile FormID reaches the canonical projectile field"
                );
                assert_eq!(clip_rounds, 0);
            }
            _ => panic!("expected Ammo kind"),
        }
    }
}
