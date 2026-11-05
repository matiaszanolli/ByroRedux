//! Effects / FX / VATS / impact records.

use super::super::common::{read_lstring_or_zstring, read_zstring, remap_fid, CommonNamedFields};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

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
    /// `PNAM` — FormIDs of PERK records that gate on this actor value.
    /// Skyrim+ perk-tree wiring. Empty on FO3/FNV.
    pub perks: Vec<u32>,
}

pub fn parse_avif(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> AvifRecord {
    let mut out = AvifRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"ANAM" => out.abbreviation = read_zstring(&sub.data),
            b"CNAM" => out.category = SubReader::new(&sub.data).u32_or_default(),
            b"AVSK" if sub.data.len() >= 16 => {
                out.skill_scaling = Some(
                    SubReader::new(&sub.data)
                        .f32_array::<4>()
                        .unwrap_or([0.0; 4]),
                );
            }
            b"PNAM" => {
                if let Ok(raw) = SubReader::new(&sub.data).u32() {
                    let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                    out.perks.push(remapped);
                }
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
//          Parse-but-don't-consume gate (TD5-016): gated on M41
//          Phase 4 (physics/dismemberment system). Parser captures
//          EDID + key scalars; full sub-record decode lands when the
//          consuming subsystem arrives.
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
    /// `MODL` — the projectile's world mesh. #3753: `CommonNamedFields`
    /// has always read this and `parse_proj` discarded it, so a consumer
    /// holding a `ProjRecord` had no way to draw one. The cell-side sink
    /// is `cells.statics` (the `PROJ` dispatch runs through
    /// `extract_records_with_modl` since #3542); this is the same path on
    /// the typed record, for consumers that resolve through
    /// `index.projectiles` instead. All 52 `Fallout3.esm` PROJ records
    /// author one.
    pub model_path: String,
    /// `DATA` offset 4 — gravity (xEdit `wbPROJ`), in the engine's own unit.
    pub gravity: f32,
    /// `DATA` offset 12 — range in game units.
    pub range: f32,
    /// `DATA` offset 36 — the `EXPL` the projectile detonates into, or 0.
    pub explosion: u32,
    /// `DATA` offset 52 — impact force.
    pub impact_force: f32,
    /// `DATA` offset 72 (Skyrim+ only; FO3/FNV author rotation there) —
    /// collision radius in game units.
    pub collision_radius: f32,
    /// `DATA` offset 76 (Skyrim+ only) — lifetime in seconds.
    pub lifetime: f32,
    /// "Explosion - Alt. Trigger - Proximity" (xEdit `wbPROJ`): how close a target must come for an
    /// alternate-trigger projectile — a mine — to go off. `DATA` offset 28 on FO3/FNV/Skyrim, `DNAM`
    /// offset 24 on Fallout 4 and 76. `0.0` when not authored.
    pub alt_trigger_proximity: f32,
    /// "Explosion - Alt. Trigger - Timer": seconds before an alternate-trigger projectile goes off by itself
    /// (`DATA` 32 / Fallout 4 and 76 `DNAM` 28).
    pub alt_trigger_timer: f32,
}

impl ProjRecord {
    /// The `DATA` type word (xEdit: 1 Missile, 2 Lobber, 4 Beam, 8 Flame,
    /// 0x10 Cone, 0x20 Barrier, 0x40 Arrow).
    pub fn projectile_type(&self) -> u16 {
        (self.flags >> 16) as u16
    }

    /// The `DATA` flag word (bit 0 Hitscan, 1 Explosion, 2 Alt. Trigger, 3
    /// Muzzle Flash, 5 Can Be Disabled, 6 Can Be Picked Up, 7 Supersonic, 8 Pins
    /// Limbs, 9 Pass Through Small Transparent, 10 Disable Combat Aim
    /// Correction, 11 Rotation).
    pub fn projectile_flags(&self) -> u16 {
        (self.flags & 0xFFFF) as u16
    }

    /// `Alt. Trigger` (flag bit 2) with a proximity distance and an explosion to detonate: a mine. A bullet
    /// or grenade (no alt trigger) and a timed charge (no proximity) are not.
    pub fn is_proximity_mine(&self) -> bool {
        self.projectile_flags() & 0x4 != 0 && self.alt_trigger_proximity > 0.0 && self.explosion != 0
    }
}

/// `PROJ` `DATA` (FO3 / FNV / Skyrim; xEdit `wbPROJ`): the decode reads flags, gravity and speed before
/// anything else, so a shorter body is not decoded.
const PROJ_DATA_MIN_LEN: usize = 12;
/// `DATA` field offsets the decode seeks to (xEdit `wbDefinitions{FNV,TES5}.pas`): the alt-trigger proximity
/// (then the timer at 32 and the explosion at 36), the impact force, and the Skyrim+ collision radius (then the
/// lifetime at 76). 16..28 are the two lights and the tracer chance, 40..52 the sound and the muzzle-flash and
/// fade durations, 56..68 the countdown and disable sounds and the default weapon source; at 68 Skyrim+
/// authors the cone spread and FO3 / FNV a rotation vector.
const PROJ_DATA_ALT_TRIGGER_PROXIMITY: usize = 28;
const PROJ_DATA_IMPACT_FORCE: usize = 52;
const PROJ_DATA_COLLISION_RADIUS: usize = 72;
/// Fallout 4 and Fallout 76's `PROJ` `DNAM` (xEdit `wbDefinitions{FO4,FO76}.pas` `wbPROJ`, the same 93 bytes on both
/// with an empty `DATA`), through the lifetime at 72..76, the last field decoded.
const PROJ_DNAM_LEN: usize = 76;
/// `DNAM` field offsets: flags u16 and type u16, gravity, speed, range and the two lights come first (`DATA`'s
/// tracer chance is gone); then the alt-trigger proximity (timer 28, explosion 32), the impact force, and the
/// collision radius (lifetime 72).
const PROJ_DNAM_ALT_TRIGGER_PROXIMITY: usize = 24;
const PROJ_DNAM_IMPACT_FORCE: usize = 48;
const PROJ_DNAM_COLLISION_RADIUS: usize = 68;

pub fn parse_proj(
    form_id: u32,
    subs: &[SubRecord],
    game: crate::esm::reader::GameKind,
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> ProjRecord {
    let mut out = ProjRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    out.model_path = common.model_path;
    for sub in subs {
        match &sub.sub_type {
            // #5846 — one sequential SubReader pass: offsets and their
            // length gates can no longer drift apart (this used to re-slice
            // `&sub.data[N..]` behind a hand-typed `len() >= N+4` per field).
            b"DATA" if sub.data.len() >= PROJ_DATA_MIN_LEN => {
                let mut r = SubReader::new(&sub.data);
                out.flags = r.u32_or_default();
                out.gravity = r.f32_or_default();
                out.muzzle_speed = r.f32_or_default();
                out.range = r.f32_or_default();
                r.skip_to_or_eof(PROJ_DATA_ALT_TRIGGER_PROXIMITY);
                out.alt_trigger_proximity = r.f32_or_default();
                out.alt_trigger_timer = r.f32_or_default();
                let explosion = r.u32_or_default();
                out.explosion = if explosion == 0 { 0 } else { remap_fid(explosion, remap) };
                r.skip_to_or_eof(PROJ_DATA_IMPACT_FORCE);
                out.impact_force = r.f32_or_default();
                // The tail from 68 differs by era: Skyrim+ authors cone spread, collision radius and lifetime
                // there, FO3/FNV a rotation vector.
                let skyrim_plus = matches!(
                    game,
                    crate::esm::reader::GameKind::Skyrim
                        | crate::esm::reader::GameKind::Fallout4
                        | crate::esm::reader::GameKind::Fallout76
                        | crate::esm::reader::GameKind::Starfield
                );
                if skyrim_plus {
                    r.skip_to_or_eof(PROJ_DATA_COLLISION_RADIUS);
                    out.collision_radius = r.f32_or_default();
                    out.lifetime = r.f32_or_default();
                }
            }
            // Fallout 4 moved the struct to `DNAM` and Fallout 76 kept it there: the `PROJ_DNAM_*` offsets above.
            b"DNAM"
                if matches!(game, crate::esm::reader::GameKind::Fallout4 | crate::esm::reader::GameKind::Fallout76)
                    && sub.data.len() >= PROJ_DNAM_LEN =>
            {
                let mut r = SubReader::new(&sub.data);
                let flags = r.u16_or_default();
                let kind = r.u16_or_default();
                out.flags = (u32::from(kind) << 16) | u32::from(flags);
                out.gravity = r.f32_or_default();
                out.muzzle_speed = r.f32_or_default();
                out.range = r.f32_or_default();
                r.skip_to_or_eof(PROJ_DNAM_ALT_TRIGGER_PROXIMITY);
                out.alt_trigger_proximity = r.f32_or_default();
                out.alt_trigger_timer = r.f32_or_default();
                let explosion = r.u32_or_default();
                out.explosion = if explosion == 0 { 0 } else { remap_fid(explosion, remap) };
                r.skip_to_or_eof(PROJ_DNAM_IMPACT_FORCE);
                out.impact_force = r.f32_or_default();
                r.skip_to_or_eof(PROJ_DNAM_COLLISION_RADIUS);
                out.collision_radius = r.f32_or_default();
                out.lifetime = r.f32_or_default();
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
    /// `DATA` @16 — the Fill/Texture effect's colour (Oblivion, Fallout 3, New Vegas share this
    /// leading layout, xEdit `wbDefinitions{TES4,FNV}.pas`). `None` when `DATA` is too short.
    pub fill: Option<EfshFill>,
}

/// The Fill/Texture ("membrane") effect of an effect shader: its colour and the alpha timeline it
/// plays over (`DATA` @16..48). Times are seconds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EfshFill {
    pub color: [u8; 3],
    pub fade_in: f32,
    pub full_alpha_time: f32,
    pub fade_out: f32,
    /// The alpha the shader settles at once its timeline has run, while it keeps playing.
    pub persistent_alpha_ratio: f32,
    pub pulse_amplitude: f32,
    pub pulse_frequency: f32,
    /// `DATA` flag bit 0, "No Membrane Shader": there is no fill at all.
    pub no_membrane: bool,
}

pub fn parse_efsh(form_id: u32, subs: &[SubRecord]) -> EfshRecord {
    let mut out = EfshRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"ICON" => out.fill_texture = read_zstring(&sub.data),
            b"ICO2" => out.particle_texture = read_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 44 => {
                let f = |at: usize| f32::from_le_bytes(sub.data[at..at + 4].try_into().unwrap());
                out.fill = Some(EfshFill {
                    color: [sub.data[16], sub.data[17], sub.data[18]],
                    fade_in: f(20),
                    full_alpha_time: f(24),
                    fade_out: f(28),
                    persistent_alpha_ratio: f(32),
                    pulse_amplitude: f(36),
                    pulse_frequency: f(40),
                    no_membrane: sub.data[0] & 1 != 0,
                });
            }
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                out.value = r.i32_or_default();
                out.weight = r.f32_or_default();
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 4 => {
                out.base_value = SubReader::new(&sub.data).f32_or_default();
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
    /// Damage in HP per direct hit (f32), where [`expl_data_layout`] finds it in `DATA`.
    pub damage: f32,
    /// Explosion blast radius in game units (f32): the one radius a record authors, or its `Outer Radius`
    /// when it authors two.
    pub radius: f32,
    /// `Inner Radius` where a record authors one (Fallout 4 and 76 from form version 97); otherwise equal to
    /// [`Self::radius`].
    pub inner_radius: f32,
}

/// Force-first `EXPL` `DATA` (FO3 / FNV `wbEXPL`, and Skyrim's oldest forms): Force at 0, then the damage here, the
/// radius after it, and the Light form id at 12.
const EXPL_FORCE_FIRST_DAMAGE: usize = 4;
/// Forms-first `EXPL` `DATA` (Skyrim, Fallout 4 and Fallout 76 `wbEXPL`): six form ids (light, two sounds, impact
/// data set, placed object, spawn projectile) fill 0..24, the force sits at 24, then the damage here and the radii
/// after it.
const EXPL_FORMS_FIRST_DAMAGE: usize = 28;
/// Fallout 76's `Damage Curve Table` form id takes 24 and pushes the force and everything after it along one word.
const EXPL_FO76_CURVE_TABLE_DAMAGE: usize = 32;
/// These layouts are gated on the form version in the record header, which the record walker does not keep.
/// `DATA` only grows with form version, so its length gives the same answer on every shipped master (raw census,
/// #6562). Skyrim below form version 30 is still force-first (40-44 B; xEdit's layout carries a note that
/// `FireStormExplosion02` contradicts it) and forms-first from here up.
const EXPL_SKYRIM_FORMS_FIRST_MIN_LEN: usize = 48;
/// Fallout 4 and 76 author `Inner Radius` from form version 97 (xEdit `wbFromVersion(97, ...)`): 52-60 B without it,
/// and from here up with it, where it takes the word the one radius sat in and pushes `Outer Radius` along.
const EXPL_INNER_RADIUS_MIN_LEN: usize = 64;
/// Fallout 76 authors the `Damage Curve Table` from form version 150 (xEdit `wbFromVersion(150, ...)`): the 84-byte
/// full form below that (`ExplosionRailwayRifleStaggerSmall`, version 128) has none, every longer form has it.
const EXPL_FO76_CURVE_TABLE_MIN_LEN: usize = 88;

/// Where an `EXPL` `DATA` of `len` bytes keeps its damage, and whether the words after it are `Inner Radius` and
/// `Outer Radius` rather than the one radius. `None` for a game with no `EXPL` `DATA` layout: Oblivion has no `EXPL`,
/// and Starfield authors `ENAM` instead.
fn expl_data_layout(game: crate::esm::reader::GameKind, len: usize) -> Option<(usize, bool)> {
    use crate::esm::reader::GameKind;
    match game {
        GameKind::Fallout3NV => Some((EXPL_FORCE_FIRST_DAMAGE, false)),
        GameKind::Skyrim if len < EXPL_SKYRIM_FORMS_FIRST_MIN_LEN => Some((EXPL_FORCE_FIRST_DAMAGE, false)),
        GameKind::Skyrim => Some((EXPL_FORMS_FIRST_DAMAGE, false)),
        GameKind::Fallout76 if len >= EXPL_FO76_CURVE_TABLE_MIN_LEN => Some((EXPL_FO76_CURVE_TABLE_DAMAGE, true)),
        GameKind::Fallout4 | GameKind::Fallout76 => Some((EXPL_FORMS_FIRST_DAMAGE, len >= EXPL_INNER_RADIUS_MIN_LEN)),
        GameKind::Oblivion | GameKind::Starfield => None,
    }
}

pub fn parse_expl(form_id: u32, subs: &[SubRecord], game: crate::esm::reader::GameKind) -> ExplRecord {
    let mut out = ExplRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" => {
                let Some((damage_at, two_radii)) = expl_data_layout(game, sub.data.len()) else {
                    continue;
                };
                // A body too short to reach its last radius word is not decoded.
                let radius_words = if two_radii { 2 } else { 1 };
                if sub.data.len() < damage_at + 4 + radius_words * 4 {
                    continue;
                }
                let mut r = SubReader::new(&sub.data);
                r.skip_to_or_eof(damage_at);
                out.damage = r.f32_or_default();
                out.inner_radius = r.f32_or_default();
                out.radius = if two_radii { r.f32_or_default() } else { out.inner_radius };
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.model_path = common.model_path;
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    for sub in subs {
        if &sub.sub_type == b"DATA" {
            // FO3/FNV IPDS DATA is a fixed-size 96-byte array
            // (12 × 8 bytes = (material_kind: u32, ipct: u32) per
            // entry). Skyrim uses 4-byte entries. Counting only:
            out.material_pair_count = (sub.data.len() / 8) as u32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

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
            sub(b"CNAM", 1u32.to_le_bytes()), // Combat
            sub(b"AVSK", &avsk),
        ];
        let a = parse_avif(0x0000_002B, &subs, &None);
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
        let a = parse_avif(0x0000_0000, &subs, &None);
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
            sub(b"DATA", data),
        ];
        let p = parse_proj(0x0007_4824, &subs, crate::esm::reader::GameKind::Fallout3NV, &None);
        assert_eq!(p.editor_id, "5mmRoundProjectile");
        assert_eq!(p.full_name, "5mm Round");
        assert_eq!(p.flags, 0x0000_0001);
        assert!((p.muzzle_speed - 3000.0).abs() < 1e-3);
    }

    /// Skyrim's `TrapDart` projectile (`TrapDartWooden01`, Skyrim.esm `000141EF`):
    /// flags word 0x0208 (Muzzle Flash, Pins Limbs), type 1 (Missile), speed
    /// 1300. The xEdit TES5 layout carries gravity at 4, range at 12, the
    /// explosion at 36, impact force at 52, collision radius at 72, lifetime at 76.
    #[test]
    fn parse_proj_reads_the_skyrim_layout() {
        let mut data = [0u8; 84];
        data[0..2].copy_from_slice(&0x0208u16.to_le_bytes());
        data[2..4].copy_from_slice(&1u16.to_le_bytes());
        data[4..8].copy_from_slice(&0.25_f32.to_le_bytes());
        data[8..12].copy_from_slice(&1300.0_f32.to_le_bytes());
        data[12..16].copy_from_slice(&5000.0_f32.to_le_bytes());
        data[36..40].copy_from_slice(&0x0001_0203u32.to_le_bytes());
        data[52..56].copy_from_slice(&9.0_f32.to_le_bytes());
        data[72..76].copy_from_slice(&6.0_f32.to_le_bytes());
        data[76..80].copy_from_slice(&30.0_f32.to_le_bytes());
        let subs = vec![sub(b"EDID", b"TrapDartWooden01\0"), sub(b"DATA", data)];
        let p = parse_proj(0x0001_41EF, &subs, crate::esm::reader::GameKind::Skyrim, &None);
        assert_eq!((p.projectile_type(), p.projectile_flags()), (1, 0x0208));
        assert_eq!((p.gravity, p.muzzle_speed, p.range), (0.25, 1300.0, 5000.0));
        assert_eq!((p.explosion, p.impact_force), (0x0001_0203, 9.0));
        assert_eq!((p.collision_radius, p.lifetime), (6.0, 30.0));
        // FO3/FNV author a rotation where Skyrim has radius and lifetime.
        let subs = vec![sub(b"DATA", data)];
        let fnv = parse_proj(1, &subs, crate::esm::reader::GameKind::Fallout3NV, &None);
        assert_eq!((fnv.collision_radius, fnv.lifetime), (0.0, 0.0));
    }

    #[test]
    fn parse_proj_short_data_is_tolerated() {
        // Truncated DATA (or absent DATA on placeholder PROJs) must not
        // panic. Stub returns Default::default() field values.
        let subs = vec![sub(b"EDID", b"TestProjectile\0")];
        let p = parse_proj(0xDEADBEEF, &subs, crate::esm::reader::GameKind::Skyrim, &None);
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
    fn parse_efsh_reads_the_fill_colour_and_its_alpha_timeline() {
        // DATA: flags u8 + 3 unused, three u32 blend/test words, then the Fill colour (RGB + unused)
        // at 16 and the timings (fade in, full, fade out, persistent ratio, pulse amp, pulse freq).
        let mut data = vec![0u8; 48];
        data[16..19].copy_from_slice(&[255, 128, 0]);
        for (i, value) in [0.5f32, 1.0, 2.0, 0.25, 0.1, 3.0].iter().enumerate() {
            data[20 + 4 * i..24 + 4 * i].copy_from_slice(&value.to_le_bytes());
        }
        let e = parse_efsh(0x77, &[sub(b"EDID", b"EFFTest\0"), sub(b"DATA", &data)]);
        assert_eq!(
            e.fill,
            Some(EfshFill { color: [255, 128, 0], fade_in: 0.5, full_alpha_time: 1.0, fade_out: 2.0, persistent_alpha_ratio: 0.25, pulse_amplitude: 0.1, pulse_frequency: 3.0, no_membrane: false })
        );
        // A DATA too short for the layout is no fill, not a made-up one.
        assert_eq!(parse_efsh(0x78, &[sub(b"DATA", [0u8; 20])]).fill, None);
        // Flag bit 0 is "No Membrane Shader".
        data[0] = 1;
        assert!(parse_efsh(0x79, &[sub(b"DATA", &data)]).fill.unwrap().no_membrane);
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
            sub(b"DATA", data),
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
            sub(b"DATA", data),
        ];
        let r = parse_repu(0x0011_E662, &subs);
        assert_eq!(r.editor_id, "NCR");
        assert_eq!(r.full_name, "New California Republic");
        assert!((r.base_value - 1.0).abs() < 1e-6);
    }

    /// A real 52-byte FNV `EXPL` `DATA` image — `MineFragExplosion` (0x0007078B) from `FalloutNV.esm`: force 250,
    /// damage 100, radius 192, then the light (a LIGH form id, `0x00014BFF`, which the old decode read as the
    /// radius: a denormal) and the rest. (#6541)
    #[test]
    fn parse_expl_reads_fo3_fnv_damage_and_radius_at_xedits_offsets() {
        let mut data = [0u8; 52];
        data[0..4].copy_from_slice(&250.0_f32.to_le_bytes()); // force
        data[4..8].copy_from_slice(&100.0_f32.to_le_bytes()); // damage
        data[8..12].copy_from_slice(&192.0_f32.to_le_bytes()); // radius
        data[12..16].copy_from_slice(&0x0001_4BFFu32.to_le_bytes()); // light
        let subs = vec![
            sub(b"EDID", b"MineFragExplosion\0"),
            sub(b"FULL", b"Frag Mine\0"),
            sub(b"DATA", data),
        ];
        let e = parse_expl(0x0007_078B, &subs, crate::esm::reader::GameKind::Fallout3NV);
        assert_eq!(e.editor_id, "MineFragExplosion");
        assert_eq!((e.damage, e.radius, e.inner_radius), (100.0, 192.0, 192.0));
    }
    /// A Fallout 4 mine (`FragMineProjectile` from `Fallout4.esm`): flags 0x66 (Explosion | Alt. Trigger | Can
    /// Be Disabled | Can Be Picked Up), type Lobber, proximity 100, timer 1.5, explosion 0xEECE9 — and its
    /// `fragMineExplosion` (damage 100, inner radius 100, outer 500).
    #[test]
    fn a_fallout4_mine_projectile_and_explosion_decode_from_their_dnam_and_data() {
        let mut dnam = vec![0u8; 93];
        dnam[0..2].copy_from_slice(&0x66u16.to_le_bytes());
        dnam[2..4].copy_from_slice(&2u16.to_le_bytes());
        dnam[12..16].copy_from_slice(&15000.0f32.to_le_bytes());
        dnam[24..28].copy_from_slice(&100.0f32.to_le_bytes());
        dnam[28..32].copy_from_slice(&1.5f32.to_le_bytes());
        dnam[32..36].copy_from_slice(&0x000E_ECE9u32.to_le_bytes());
        let p = parse_proj(0xE56C3, &[sub(b"EDID", b"FragMineProjectile\0"), sub(b"DNAM", &dnam)], crate::esm::reader::GameKind::Fallout4, &None);
        assert_eq!((p.alt_trigger_proximity, p.alt_trigger_timer, p.explosion), (100.0, 1.5, 0x000E_ECE9));
        assert_eq!(p.projectile_type(), 2);
        assert!(p.is_proximity_mine());
        // The same projectile without the alt-trigger flag (a thrown grenade) is not a mine.
        let mut grenade = dnam.clone();
        grenade[0..2].copy_from_slice(&0x2u16.to_le_bytes());
        let g = parse_proj(1, &[sub(b"DNAM", &grenade)], crate::esm::reader::GameKind::Fallout4, &None);
        assert!(!g.is_proximity_mine());

        let mut data = vec![0u8; 84];
        data[24..28].copy_from_slice(&2000.0f32.to_le_bytes());
        data[28..32].copy_from_slice(&100.0f32.to_le_bytes());
        data[32..36].copy_from_slice(&100.0f32.to_le_bytes());
        data[36..40].copy_from_slice(&500.0f32.to_le_bytes());
        let e = parse_expl(0xEECE9, &[sub(b"EDID", b"fragMineExplosion\0"), sub(b"DATA", &data)], crate::esm::reader::GameKind::Fallout4);
        assert_eq!((e.damage, e.inner_radius, e.radius), (100.0, 100.0, 500.0));
    }

    /// An `EXPL` `DATA` image of `len` bytes holding `floats` as consecutive words from offset `at`.
    fn expl_data(len: usize, at: usize, floats: &[f32]) -> Vec<u8> {
        let mut data = vec![0u8; len];
        for (i, value) in floats.iter().enumerate() {
            data[at + i * 4..at + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        data
    }

    /// #6562 — Fallout 4 authors `Inner Radius` from form version 97, so the shorter, older `DATA` has its one
    /// radius in the word the newer form gives the inner one. Force onward, read from `Fallout4.esm`.
    #[test]
    fn fallout4_expl_reads_the_radius_its_form_version_authors() {
        for (name, len, floats, damage, inner, outer) in [
            // Form version 63 / 88 / 95, no `Inner Radius`.
            ("FakeForce300Explosion", 52, [300.0, 0.0, 512.0, 0.0, 0.1], 0.0, 512.0, 512.0),
            ("FakeForce200Explosion", 56, [200.0, 0.0, 128.0, 0.0, 0.1], 0.0, 128.0, 128.0),
            ("CarNukeTEST", 60, [20000.0, 200.0, 550.0, 10000.0, 0.3], 200.0, 550.0, 550.0),
            // Form version 99: the first length with `Inner Radius`.
            ("MirelurkHunterSpitExplosion", 64, [0.0, 0.0, 0.0, 200.0, 0.0], 0.0, 0.0, 200.0),
        ] {
            let subs = [sub(b"DATA", expl_data(len, 24, &floats))];
            let e = parse_expl(1, &subs, crate::esm::reader::GameKind::Fallout4);
            assert_eq!((e.damage, e.inner_radius, e.radius), (damage, inner, outer), "{name}");
        }
    }

    /// #6562's sibling — Skyrim below form version 30 keeps the FO3 / FNV force-first layout, with `Inner Radius`
    /// never authored. `FireStormExplosion02` (form version 28) has its impact data set `0x00026113` at 28, where
    /// the forms-first decode would read it as the damage. Values read from `Skyrim.esm`.
    #[test]
    fn skyrims_oldest_expl_forms_keep_the_force_first_layout() {
        let mut fire_storm = expl_data(44, 0, &[100.0, 25.0, 360.0]);
        fire_storm[24..28].copy_from_slice(&1024.0f32.to_le_bytes());
        fire_storm[28..32].copy_from_slice(&0x0002_6113u32.to_le_bytes());
        let e = parse_expl(0x0008_77F9, &[sub(b"DATA", fire_storm)], crate::esm::reader::GameKind::Skyrim);
        assert_eq!((e.damage, e.inner_radius, e.radius), (25.0, 360.0, 360.0));

        // Form version 30, the first forms-first length, and the 52-byte full form.
        let trap = expl_data(48, 24, &[0.0, 50.0, 60.0, 25.0]);
        let e = parse_expl(0x000A_036F, &[sub(b"DATA", trap)], crate::esm::reader::GameKind::Skyrim);
        assert_eq!((e.damage, e.inner_radius, e.radius), (50.0, 60.0, 60.0));
        let nudge = expl_data(52, 24, &[1.0, 0.0, 96.0]);
        let e = parse_expl(0x000C_800D, &[sub(b"DATA", nudge)], crate::esm::reader::GameKind::Skyrim);
        assert_eq!((e.damage, e.inner_radius, e.radius), (0.0, 96.0, 96.0));
    }

    /// #6563 — Fallout 76's `EXPL` has the Fallout 4 layout until the `Damage Curve Table` (form version 150, 88 B+)
    /// pushes it along a word. `crExplosionGrenadePumpkin` used to decode at the FO3 offsets, taking its first sound's
    /// form id for a denormal damage and its empty second sound for a radius of 0. Values read from `SeventySix.esm`.
    #[test]
    fn fallout76_expl_skips_the_damage_curve_table_from_its_form_version() {
        // Form version 210: light, first sound, impact data set and curve table are all set.
        let mut pumpkin = expl_data(96, 28, &[550.0, 1.0, 150.0, 500.0, 1000.0]);
        pumpkin[0..4].copy_from_slice(&0x000D_8F99u32.to_le_bytes());
        pumpkin[4..8].copy_from_slice(&0x0053_03F7u32.to_le_bytes());
        pumpkin[12..16].copy_from_slice(&0x0001_F5B0u32.to_le_bytes());
        pumpkin[24..28].copy_from_slice(&0x0076_E9C4u32.to_le_bytes());
        let e = parse_expl(0x0086_321C, &[sub(b"DATA", pumpkin)], crate::esm::reader::GameKind::Fallout76);
        assert_eq!((e.damage, e.inner_radius, e.radius), (1.0, 150.0, 500.0));

        // Form version 197 (92 B) and 164 (88 B): the first lengths with the curve table, here unset.
        for len in [88, 92] {
            let acid = expl_data(len, 28, &[100.0, 0.0, 150.0, 250.0, 800.0]);
            let e = parse_expl(1, &[sub(b"DATA", acid)], crate::esm::reader::GameKind::Fallout76);
            assert_eq!((e.damage, e.inner_radius, e.radius), (0.0, 150.0, 250.0), "{len} B");
        }

        // Form version 128: the 84-byte full form has no curve table, so the force sits at 24.
        let stagger = expl_data(84, 24, &[1.0, 1.0, 100.0, 100.0]);
        let e = parse_expl(0x0021_5E08, &[sub(b"DATA", stagger)], crate::esm::reader::GameKind::Fallout76);
        assert_eq!((e.damage, e.inner_radius, e.radius), (1.0, 100.0, 100.0));
    }

    /// #6563 — Fallout 76's `PROJ` keeps the struct in the same 93-byte `DNAM` as Fallout 4, with an empty `DATA`, so
    /// the flags, range, alt trigger and explosion all read from there. `ProjectileStormbossFragMine` from
    /// `SeventySix.esm`.
    #[test]
    fn a_fallout76_mine_projectile_decodes_from_its_dnam() {
        let mut dnam = vec![0u8; 93];
        dnam[0..2].copy_from_slice(&0x600Eu16.to_le_bytes());
        dnam[2..4].copy_from_slice(&2u16.to_le_bytes());
        dnam[4..8].copy_from_slice(&3.5f32.to_le_bytes());
        dnam[8..12].copy_from_slice(&1600.0f32.to_le_bytes());
        dnam[12..16].copy_from_slice(&15000.0f32.to_le_bytes());
        dnam[24..28].copy_from_slice(&235.0f32.to_le_bytes());
        dnam[28..32].copy_from_slice(&1.6f32.to_le_bytes());
        dnam[32..36].copy_from_slice(&0x0077_B4BEu32.to_le_bytes());
        dnam[68..72].copy_from_slice(&5.0f32.to_le_bytes());
        dnam[72..76].copy_from_slice(&100.0f32.to_le_bytes());
        let subs = [sub(b"EDID", b"ProjectileStormbossFragMine\0"), sub(b"DATA", b""), sub(b"DNAM", &dnam)];
        let p = parse_proj(0x0075_D8D5, &subs, crate::esm::reader::GameKind::Fallout76, &None);
        assert_eq!((p.projectile_flags(), p.projectile_type()), (0x600E, 2));
        assert_eq!((p.gravity, p.muzzle_speed, p.range), (3.5, 1600.0, 15000.0));
        assert_eq!((p.alt_trigger_proximity, p.alt_trigger_timer, p.explosion), (235.0, 1.6, 0x0077_B4BE));
        assert_eq!((p.collision_radius, p.lifetime), (5.0, 100.0));
        assert!(p.is_proximity_mine());
    }

    /// A body that stops before its last radius word, and a game with no `EXPL` `DATA` layout, decode nothing
    /// rather than reading the words at some other game's offsets.
    #[test]
    fn expl_data_without_a_layout_or_its_radius_stays_zero() {
        use crate::esm::reader::GameKind;
        let floats = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        for (game, len) in [
            (GameKind::Fallout4, 32),
            (GameKind::Fallout3NV, 8),
            (GameKind::Oblivion, 36),
            (GameKind::Starfield, 36),
        ] {
            let e = parse_expl(1, &[sub(b"DATA", expl_data(36, 0, &floats)[..len].to_vec())], game);
            assert_eq!((e.damage, e.inner_radius, e.radius), (0.0, 0.0, 0.0), "{game:?} {len} B");
        }
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
        let subs = vec![sub(b"EDID", b"GenericImpactDataSet\0"), sub(b"DATA", data)];
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

    #[test]
    fn parse_avif_perk_list_two_entries() {
        let subs = vec![
            sub(b"EDID", b"SmallGuns\0"),
            sub(b"FULL", b"Small Guns\0"),
            sub(b"ANAM", b"SG\0"),
            sub(b"PNAM", 0x0101u32.to_le_bytes()),
            sub(b"PNAM", 0x0202u32.to_le_bytes()),
        ];
        let a = parse_avif(0xDEADBEEF, &subs, &None);
        assert_eq!(a.perks, vec![0x0101, 0x0202]);
    }

    #[test]
    fn parse_avif_no_pnam_keeps_perks_empty() {
        let subs = vec![sub(b"EDID", b"Strength\0"), sub(b"FULL", b"Strength\0")];
        let a = parse_avif(0xDEAD0001, &subs, &None);
        assert!(a.perks.is_empty());
    }

    // ── #810 / FNV-D2-NEW-03 — minimal-stub regression guards ─────
}
