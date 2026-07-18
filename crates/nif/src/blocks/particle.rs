//! Particle system block parsers — NiPSys* types for Oblivion through Skyrim.
//!
//! These blocks define particle effects (fire, magic, weather, etc.).
//! Parse-only — no rendering. The goal is byte-correct consumption so
//! subsequent blocks parse correctly, especially on Oblivion NIFs which
//! have no block_size fallback.

use super::controller::{parse_interp_controller_base, NiTimeControllerBase};
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::io;
use std::sync::Arc;

// ── NiPSysModifier base ────────────────────────────────────────────

/// Base fields for all NiPSysModifier subclasses.
/// name(string) + order(u32) + target(ptr/i32) + active(bool)
#[derive(Debug)]
pub struct NiPSysModifierBase {
    pub name: Option<Arc<str>>,
    pub order: u32,
    pub target_ref: BlockRef,
    pub active: bool,
}

impl NiPSysModifierBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;
        let order = stream.read_u32_le()?;
        let target_ref = stream.read_block_ref()?;
        let active = stream.read_byte_bool()?;
        Ok(Self {
            name,
            order,
            target_ref,
            active,
        })
    }
}

// ── NiPSysEmitter base (extends NiPSysModifier) ────────────────────

/// Decoded `NiPSysEmitter` base fields (nif.xml `NiPSysEmitter`). These
/// are the per-emitter spawn parameters every `NiPSys*Emitter` subclass
/// shares; pre-NIFAL they were skipped wholesale (byte-correct stream
/// advancement only) so the engine fell back to a name-heuristic preset
/// regardless of what the NIF authored. See
/// `docs/engine/nifal.md` — particles slice.
///
/// Angles are radians; `initial_color` is linear RGBA; `initial_radius`
/// / `life_span` are world units / seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EmitterBaseParams {
    pub speed: f32,
    pub speed_variation: f32,
    pub declination: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub initial_color: [f32; 4],
    pub initial_radius: f32,
    pub radius_variation: f32,
    pub life_span: f32,
    pub life_span_variation: f32,
}

/// Read the `NiPSysEmitter` base. Byte layout (and the version gate) is
/// identical to the pre-NIFAL `skip_emitter_base` — only now the values
/// are captured instead of discarded, in nif.xml field order:
///
///   speed, speed_variation, declination, declination_variation,
///   planar_angle, planar_angle_variation, initial_color (Color4),
///   initial_radius, [radius_variation since 10.4.0.1], life_span,
///   life_span_variation.
///
/// Per nif.xml ONLY `Radius Variation` carries `since="10.4.0.1"`; it is
/// interleaved BEFORE `Life Span`, which is followed by `Life Span
/// Variation` (no version condition — present at every version).
/// Pre-#1239 the gate was `bsver() >= 34` (BS_GTE_FO3), which excluded
/// Oblivion (bsver=11, version 20.0.0.5 — still ≥ 10.4.0.1) and
/// under-read every Oblivion NiPSys*Emitter by 8 bytes, truncating 219
/// NIFs in `Oblivion - Meshes.bsa`; #1239 fixed that with a
/// `>= 10.4.0.1` gate — but bundled `Life Span Variation` into the same
/// gate, which then dropped 4 bytes on the v10.x sub-versions
/// (< 10.4.0.1). #1507 split them: `Life Span Variation` is now always
/// read. All retail Bethesda titles are 20.x (≥ 10.4.0.1), so only the
/// v10.x Oblivion path changes — no #383/#1239 regression.
fn read_emitter_base(stream: &mut NifStream) -> io::Result<EmitterBaseParams> {
    let speed = stream.read_f32_le()?;
    let speed_variation = stream.read_f32_le()?;
    let declination = stream.read_f32_le()?;
    let declination_variation = stream.read_f32_le()?;
    let planar_angle = stream.read_f32_le()?;
    let planar_angle_variation = stream.read_f32_le()?;
    let initial_color = [
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
    ];
    let initial_radius = stream.read_f32_le()?;
    // Per nif.xml only `Radius Variation` carries `since="10.4.0.1"`;
    // `Life Span Variation` has NO `since` and is present at every
    // version. Pre-#1507 both were bundled under the `>= 10.4.0.1` gate,
    // which dropped `Life Span Variation` (4 B) on v10.x Oblivion
    // emitters and under-read every NiPSys*Emitter, cascading truncation
    // through the sizeless format. Verified by byte-tracing
    // `effects/metalsparks.nif` (v10.2.0.0): reading Life Span Variation
    // here lands the next block exactly on its name-length boundary.
    let radius_variation = if stream.version() >= NifVersion::V10_4_0_1 {
        stream.read_f32_le()?
    } else {
        0.0
    };
    let life_span = stream.read_f32_le()?;
    let life_span_variation = stream.read_f32_le()?;
    Ok(EmitterBaseParams {
        speed,
        speed_variation,
        declination,
        declination_variation,
        planar_angle,
        planar_angle_variation,
        initial_color,
        initial_radius,
        radius_variation,
        life_span,
        life_span_variation,
    })
}

/// `NiPSysVolumeEmitter` adds a trailing `emitter_object_ref` (ptr/i32)
/// after the shared emitter base. We don't consume the ref's target;
/// only the base params flow downstream.
fn read_volume_emitter_base(stream: &mut NifStream) -> io::Result<EmitterBaseParams> {
    let params = read_emitter_base(stream)?;
    let _emitter_object_ref = stream.read_block_ref()?;
    Ok(params)
}

// ── NiPSysCollider base ────────────────────────────────────────────

fn skip_collider_base(stream: &mut NifStream) -> io::Result<()> {
    // bounce(f32) + spawn_on_collide(bool) + die_on_collide(bool) +
    // spawn_modifier_ref(ref) + manager_ref(ptr) + next_collider_ref(ref) +
    // collider_object_ref(ptr)
    let _bounce = stream.read_f32_le()?;
    let _spawn_on_collide = stream.read_byte_bool()?;
    let _die_on_collide = stream.read_byte_bool()?;
    let _spawn_modifier_ref = stream.read_block_ref()?;
    let _manager_ref = stream.read_block_ref()?;
    let _next_collider_ref = stream.read_block_ref()?;
    let _collider_object_ref = stream.read_block_ref()?;
    Ok(())
}

// ── Generic particle block (shared struct for all types) ────────────

/// Generic particle system block. All particle types are opaque to the
/// importer — we parse them only for byte-correct stream advancement.
#[derive(Debug)]
pub struct NiPSysBlock {
    /// Original NIF type name (for debug logging).
    pub original_type: String,
}

/// Typed `NiPSys*Emitter` block — carries the decoded
/// [`EmitterBaseParams`] so the importer can downcast it and translate
/// the authored spawn parameters into the canonical `ParticleEmitter`
/// (NIFAL particles slice), instead of falling back to a name-heuristic
/// preset. `original_type` keeps the concrete subclass name (box /
/// sphere / cylinder / array / mesh) for telemetry.
#[derive(Debug)]
pub struct NiPSysEmitter {
    pub params: EmitterBaseParams,
    pub original_type: String,
}

/// `NiPSysEmitterCtlr` — the time controller that drives an emitter's
/// birth rate (particles/sec). We capture `interpolator_ref` so the
/// importer can follow it to the rate source (NiFloatInterpolator's
/// constant value or its NiFloatData keys) for the canonical
/// `ParticleEmitter.rate`. NIFAL particles slice — spawn-rate follow-up.
#[derive(Debug)]
pub struct NiPSysEmitterCtlr {
    pub interpolator_ref: BlockRef,
}

/// `NiPSysEmitterCtlrData` — legacy (pre-interpolator) birth-rate data
/// block. We capture the first birth-rate key value as the canonical
/// rate fallback when the modern NiFloatInterpolator path is absent.
#[derive(Debug)]
pub struct NiPSysEmitterCtlrData {
    /// First birth-rate key value (particles/sec at t=0), `None` when
    /// the block authored no keys.
    pub birth_rate_first: Option<f32>,
}

/// `NiPSysGrowFadeModifier` — modulates particle size over life (grow in
/// at spawn, fade out at death). We capture `base_scale`, the FO3+
/// authored size multiplier on the emitter's `initial_radius`. The
/// grow/fade *shape* (a bell curve) can't map to the canonical
/// `ParticleEmitter`'s linear start→end size, so only the magnitude
/// (`initial_radius × base_scale`) is translated. See
/// `docs/engine/nifal.md` — particles size follow-up.
#[derive(Debug)]
pub struct NiPSysGrowFadeModifier {
    /// FO3+ base size multiplier; `None` on Oblivion (no field) →
    /// treated as `1.0` downstream.
    pub base_scale: Option<f32>,
}

// ── Modifier parsers ────────────────────────────────────────────────

/// Parse a modifier with only the base fields (NiPSysPositionModifier, etc.).
pub fn parse_modifier_only(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysAgeDeathModifier: base + spawn_on_death(bool) + spawn_modifier_ref(ref)
pub fn parse_age_death_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _spawn_on_death = stream.read_byte_bool()?;
    let _spawn_modifier_ref = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysAgeDeathModifier".to_string(),
    })
}

/// `NiPSysPartSpawnModifier` (#1444 / LC-D9-01) — a WorldShift-specific
/// `NiPSysModifier` subclass that only ever appears in v10.2.0.1–10.4.0.1
/// content (per nif.xml `versions="V10_2_0_1 V10_3_0_1 V10_4_0_1"`), i.e.
/// strictly below the Oblivion floor. Pre-fix it fell to the catch-all,
/// which hard-errors on any sizeless (block_sizes-less) header — that's
/// every v10.x NIF — and truncated the rest of the file (the #1332 class).
///
/// It is NOT a base-only modifier: nif.xml adds three trailing fields
/// after the shared base, so a `parse_modifier_only` arm (as the audit
/// suggested) would under-read by 12 bytes and corrupt the stream on
/// exactly the sizeless eras where this block lives. Read them all:
///   base + particles_per_second(f32) + time(f32) + spawner_ref(ref).
pub fn parse_part_spawn_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _particles_per_second = stream.read_f32_le()?;
    let _time = stream.read_f32_le()?;
    let _spawner_ref = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysPartSpawnModifier".to_string(),
    })
}

/// NiPSysBombModifier: base + bomber_ref + axis + decay + delta_v + decay_type + symmetry_type
pub fn parse_bomb_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _bomber = stream.read_block_ref()?;
    stream.skip(12)?; // axis vec3
    let _decay = stream.read_f32_le()?;
    let _delta_v = stream.read_f32_le()?;
    let _decay_type = stream.read_u32_le()?;
    let _symmetry_type = stream.read_u32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysBombModifier".to_string(),
    })
}

/// NiPSysBoundUpdateModifier: base + update_skip(u16)
pub fn parse_bound_update_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _update_skip = stream.read_u16_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysBoundUpdateModifier".to_string(),
    })
}

/// NiPSysColliderManager: base + collider_ref(ref)
pub fn parse_collider_manager(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _collider_ref = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysColliderManager".to_string(),
    })
}

/// `NiPSysColorModifier` — Skyrim-era + earlier modern particle colour
/// modifier. References a [`crate::blocks::interpolator::NiColorData`]
/// keyframe stream that drives RGBA-over-lifetime for every spawned
/// particle.
///
/// Pre-#707 this dispatched to the catch-all `NiPSysBlock` and the
/// `color_data_ref` was discarded — every torch flame, magic effect,
/// hearth ember, and spell cast fell back to one of three name-heuristic
/// presets (`torch_flame()` / `smoke()` / `magic_sparkles()`) regardless
/// of what the NIF authored. Ember columns at the base of Whiterun's
/// Dragonsreach hearth were the surfaced symptom.
///
/// Modeled after [`crate::blocks::legacy_particle::NiParticleColorModifier`]
/// (the pre-Bethesda variant), which has carried `color_data_ref`
/// since #394; the modern variant just re-uses the field with the
/// `NiPSysModifierBase` instead of `NiParticleModifier`.
#[derive(Debug)]
pub struct NiPSysColorModifier {
    pub base: NiPSysModifierBase,
    pub color_data_ref: BlockRef,
}

impl NiPSysColorModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiPSysModifierBase::parse(stream)?;
        let color_data_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            color_data_ref,
        })
    }
}

/// Back-compat shim — earlier dispatch returned a `NiPSysBlock` for
/// every modifier subtype. Kept so the few internal call sites that
/// only need byte-correct stream advancement still compile, but new
/// code should call [`NiPSysColorModifier::parse`] directly.
pub fn parse_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _modifier = NiPSysColorModifier::parse(stream)?;
    Ok(NiPSysBlock {
        original_type: "NiPSysColorModifier".to_string(),
    })
}

/// NiPSysDragModifier: base + parent(ptr) + drag_axis(vec3) + percentage(f32) + range(f32) + range_falloff(f32)
pub fn parse_drag_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _parent = stream.read_block_ref()?;
    stream.skip(12 + 4 + 4 + 4)?; // vec3 + 3 floats
    Ok(NiPSysBlock {
        original_type: "NiPSysDragModifier".to_string(),
    })
}

/// NiPSysGravityModifier: base + gravity_object(ptr) + gravity_axis(vec3) +
/// decay(f32) + strength(f32) + force_type(u32) + turbulence(f32) + turbulence_scale(f32) +
/// world_aligned(bool, since v20.0.0.4)
pub fn parse_gravity_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _gravity_object = stream.read_block_ref()?;
    stream.skip(12)?; // gravity_axis vec3
    let _decay = stream.read_f32_le()?;
    let _strength = stream.read_f32_le()?;
    let _force_type = stream.read_u32_le()?;
    let _turbulence = stream.read_f32_le()?;
    let _turbulence_scale = stream.read_f32_le()?;
    // World Aligned — nif.xml `vercond="!#NI_BS_LTE_16#"`, i.e. `bsver > 16`,
    // NOT a NIF-version gate. The old `version >= V20_0_0_4` gate read this
    // byte on Oblivion (v20.0.0.4, bsver 11) where it is absent — a 1-byte
    // over-read that drifted the stream into the following block and
    // truncated the FX NIF tail (surfaced after #1306 unblocked the parse
    // past NiPSysRotationModifier on `meshes\fire\firetorchsmall.nif`).
    // FO3/FNV (bsver 34) and Skyrim+ are `> 16` and still read it.
    if stream.bsver() > crate::version::bsver::NI_BS_LTE_16 {
        let _world_aligned = stream.read_byte_bool()?;
    }
    Ok(NiPSysBlock {
        original_type: "NiPSysGravityModifier".to_string(),
    })
}

/// NiPSysGrowFadeModifier: base + grow_time(f32) + grow_generation(u16) +
/// fade_time(f32) + fade_generation(u16) + base_scale(f32) [BS_GTE_FO3 +
/// version 20.2.0.7]
pub fn parse_grow_fade_modifier(stream: &mut NifStream) -> io::Result<NiPSysGrowFadeModifier> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _grow_time = stream.read_f32_le()?;
    let _grow_generation = stream.read_u16_le()?;
    let _fade_time = stream.read_f32_le()?;
    let _fade_generation = stream.read_u16_le()?;
    // Bethesda 20.2.0.7 + BS_GTE_FO3 (BSVER >= 34): adds Base Scale.
    // Per nif.xml line 4803. FNV/Skyrim/FO4 all match this gate (Oblivion
    // v20.0.0.4 does NOT → `None`). Pre-#383 these 4 bytes were dropped
    // on every grow-fade modifier (890 occurrences in vanilla
    // `Fallout - Meshes.bsa`).
    let base_scale = if stream.version() == crate::version::NifVersion::V20_2_0_7
        && stream.bsver() >= crate::version::bsver::FO3_FNV
    {
        Some(stream.read_f32_le()?)
    } else {
        None
    };
    Ok(NiPSysGrowFadeModifier { base_scale })
}

/// NiPSysRotationModifier: base + initial_speed(f32) + [since 20.0.0.2]
/// speed_variation(f32) + initial_angle(f32) + angle_variation(f32) +
/// random_rot_speed_sign(bool) + random_axis(bool) + axis(vec3)
pub fn parse_rotation_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _initial_speed = stream.read_f32_le()?;
    // speed_variation + initial_angle + angle_variation: since v20.0.0.2.
    // On FO76 (`#BS_F76#`, bsver == 155) nif.xml interleaves two extra fields
    // *between* Rotation Speed Variation and Rotation Angle — Unknown Vector
    // (Vector4, 16 B) + Unknown Byte (1 B) — so the three floats can't be one
    // lumped skip there. Pre-#1896 the single skip(12) left the stream 17 B
    // short on FO76 (recovered by the dispatcher's block_size seek; inert but
    // misaligning). Split so `consumed == block_size` on every title.
    if stream.version() >= NifVersion::V20_0_0_2 {
        stream.skip(4)?; // Rotation Speed Variation
        if stream.bsver() == crate::version::bsver::FO76 {
            stream.skip(16 + 1)?; // Unknown Vector (Vector4) + Unknown Byte
        }
        stream.skip(4 * 2)?; // Rotation Angle + Rotation Angle Variation
    }
    // Random Rot Speed Sign — nif.xml `since="20.0.0.2"`, no game/bsver
    // condition. The #383 fix narrowed this to `bsver >= 34` on the claim
    // that Oblivion (bsver 11) omits the byte, but that was wrong:
    // block-tracing `meshes\fire\firetorchsmall.nif` (v20.0.0.4, bsver 11)
    // shows NiPSysRotationModifier under-reads by exactly 1 byte without
    // it — the stream drifts onto a stray 0x00, NiPSysGravityModifier then
    // reads a garbage length, and the FX emitter (plus every later block)
    // is discarded, so the torch/fire never renders (#1306 / OBL-D6-NEW-03).
    // With the byte read the layout aligns (random_axis + a clean unit
    // Axis) and the parse completes. FNV (v20.2.0.7) and FO3 already
    // satisfy `>= 20.0.0.2`, so this only *adds* the read for Oblivion /
    // early-FO3-dev content the old gate wrongly skipped — validated by a
    // before/after truncation sweep over the Oblivion FX mesh set.
    if stream.version() >= NifVersion::V20_0_0_2 {
        let _random_rot_speed_sign = stream.read_byte_bool()?;
    }
    let _random_axis = stream.read_byte_bool()?;
    stream.skip(12)?; // axis vec3
    Ok(NiPSysBlock {
        original_type: "NiPSysRotationModifier".to_string(),
    })
}

/// NiPSysSpawnModifier: base + num_spawn_generations(u16) + percentage_spawned(f32) +
/// min_num_to_spawn(u16) + max_num_to_spawn(u16) + spawn_speed_variation(f32) +
/// spawn_dir_variation(f32) + life_span(f32) + life_span_variation(f32)
pub fn parse_spawn_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _num_spawn_generations = stream.read_u16_le()?;
    let _percentage_spawned = stream.read_f32_le()?;
    let _min_num_to_spawn = stream.read_u16_le()?;
    let _max_num_to_spawn = stream.read_u16_le()?;
    stream.skip(4 * 4)?; // 4 floats
    Ok(NiPSysBlock {
        original_type: "NiPSysSpawnModifier".to_string(),
    })
}

/// NiPSysMeshUpdateModifier: base + num_meshes(u32) + mesh_refs[N]
pub fn parse_mesh_update_modifier(
    stream: &mut NifStream,
    type_name: &str,
) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let num_meshes = stream.read_u32_le()? as usize;
    for _ in 0..num_meshes {
        let _mesh_ref = stream.read_block_ref()?;
    }
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// BSPSysHavokUpdateModifier: NiPSysMeshUpdateModifier + modifier_ref(ref)
pub fn parse_havok_update_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let num_meshes = stream.read_u32_le()? as usize;
    for _ in 0..num_meshes {
        let _mesh_ref = stream.read_block_ref()?;
    }
    let _modifier_ref = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "BSPSysHavokUpdateModifier".to_string(),
    })
}

/// BSParentVelocityModifier / BSWindModifier: base + damping(f32)
pub fn parse_float_modifier(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _value = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// BSPSysInheritVelocityModifier: base + inherit_object(ptr) + 3 floats
pub fn parse_inherit_velocity_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _inherit_object = stream.read_block_ref()?;
    stream.skip(4 * 3)?; // 3 floats
    Ok(NiPSysBlock {
        original_type: "BSPSysInheritVelocityModifier".to_string(),
    })
}

/// BSPSysRecycleBoundModifier: base + 2×vec3 + target_ref
pub fn parse_recycle_bound_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    stream.skip(12 + 12)?; // 2 vec3s
    let _target = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "BSPSysRecycleBoundModifier".to_string(),
    })
}

/// BSPSysSubTexModifier: base + 7 floats
pub fn parse_sub_tex_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    stream.skip(4 * 7)?; // 7 floats
    Ok(NiPSysBlock {
        original_type: "BSPSysSubTexModifier".to_string(),
    })
}

/// BSPSysLODModifier: base + 4 floats
pub fn parse_lod_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    stream.skip(4 * 4)?; // 4 floats
    Ok(NiPSysBlock {
        original_type: "BSPSysLODModifier".to_string(),
    })
}

/// BSPSysScaleModifier: base + num_floats(u32) + floats[N]
pub fn parse_scale_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let count = stream.read_u32_le()? as u64;
    stream.skip(count * 4)?;
    Ok(NiPSysBlock {
        original_type: "BSPSysScaleModifier".to_string(),
    })
}

/// `BSPSysSimpleColorModifier` (FO3+) — the dominant FO3/FNV-era particle
/// colour modifier. Unlike [`NiPSysColorModifier`] (which references a
/// separate `NiColorData` keyframe stream), it carries its colour ramp
/// INLINE: 6 fade/percent floats + a 3-entry `Color4` array
/// (birth / mid / death) per nif.xml.
///
/// Pre-#1345 this dispatched to the opaque `NiPSysBlock` and the colours
/// were discarded, so FNV/FO3 effects authored this way (geysers, steam,
/// many weapon/spell FX) fell back to the name-heuristic preset colour
/// (`torch_flame()` / `smoke()` / `magic_sparkles()`) instead of the
/// authored ramp — the same class of bug #707 fixed for the legacy
/// `NiPSysColorModifier`, just not extended to the Bethesda variant.
///
/// The trailing FO76-only `Unknown Shorts[26]` (`#BS_F76#` in nif.xml, 52 B)
/// is consumed on FO76 (bsver == 155) so `consumed == block_size` (#1896);
/// its contents are opaque and discarded. The leading 72 bytes (base-relative:
/// 6 floats + 3 Color4) are byte-identical across FO3/FNV/Skyrim/FO4/FO76, so
/// `colors` is captured correctly on every title.
#[derive(Debug)]
pub struct BSPSysSimpleColorModifier {
    pub base: NiPSysModifierBase,
    pub fade_in_percent: f32,
    pub fade_out_percent: f32,
    pub color_1_end_percent: f32,
    pub color_1_start_percent: f32,
    pub color_2_end_percent: f32,
    pub color_2_start_percent: f32,
    /// Lifetime colour ramp keys: `[0]` = birth, `[1]` = mid, `[2]` =
    /// death. RGBA linear floats (nif.xml `Colors type="Color4" length="3"`).
    pub colors: [[f32; 4]; 3],
}

impl BSPSysSimpleColorModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiPSysModifierBase::parse(stream)?;
        let fade_in_percent = stream.read_f32_le()?;
        let fade_out_percent = stream.read_f32_le()?;
        let color_1_end_percent = stream.read_f32_le()?;
        let color_1_start_percent = stream.read_f32_le()?;
        let color_2_end_percent = stream.read_f32_le()?;
        let color_2_start_percent = stream.read_f32_le()?;
        let mut colors = [[0.0f32; 4]; 3];
        for c in colors.iter_mut() {
            *c = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
        }
        // Trailing Unknown Shorts[26] (52 B) — FO76 only (`#BS_F76#`, bsver
        // == 155 per nif.xml). Pre-#1896 the dispatcher's block_size recovery
        // skipped it; consume it here so `consumed == block_size` and the drift
        // histogram is clean. `colors` above is captured on every title.
        if stream.bsver() == crate::version::bsver::FO76 {
            stream.skip(26 * 2)?; // ushort × 26
        }
        Ok(Self {
            base,
            fade_in_percent,
            fade_out_percent,
            color_1_end_percent,
            color_1_start_percent,
            color_2_end_percent,
            color_2_start_percent,
            colors,
        })
    }
}

/// Back-compat shim — older dispatch returned an opaque `NiPSysBlock`.
/// Kept for byte-correct stream advancement at call sites that don't need
/// the colours; new code should call [`BSPSysSimpleColorModifier::parse`].
pub fn parse_simple_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _modifier = BSPSysSimpleColorModifier::parse(stream)?;
    Ok(NiPSysBlock {
        original_type: "BSPSysSimpleColorModifier".to_string(),
    })
}

/// BSPSysStripUpdateModifier (FO3+): base + update_delta_time(f32)
pub fn parse_strip_update_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _update_delta = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: "BSPSysStripUpdateModifier".to_string(),
    })
}

// ── Emitter parsers ─────────────────────────────────────────────────

/// NiPSysBoxEmitter: modifier_base + emitter_base + volume_emitter + 3 floats
pub fn parse_box_emitter(stream: &mut NifStream) -> io::Result<NiPSysEmitter> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let params = read_volume_emitter_base(stream)?;
    stream.skip(4 * 3)?; // width, height, depth
    Ok(NiPSysEmitter {
        params,
        original_type: "NiPSysBoxEmitter".to_string(),
    })
}

/// NiPSysCylinderEmitter: modifier_base + emitter_base + volume_emitter + 2 floats
pub fn parse_cylinder_emitter(stream: &mut NifStream) -> io::Result<NiPSysEmitter> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let params = read_volume_emitter_base(stream)?;
    stream.skip(4 * 2)?; // radius, height
    Ok(NiPSysEmitter {
        params,
        original_type: "NiPSysCylinderEmitter".to_string(),
    })
}

/// NiPSysSphereEmitter: modifier_base + emitter_base + volume_emitter + 1 float
pub fn parse_sphere_emitter(stream: &mut NifStream) -> io::Result<NiPSysEmitter> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let params = read_volume_emitter_base(stream)?;
    let _radius = stream.read_f32_le()?;
    Ok(NiPSysEmitter {
        params,
        original_type: "NiPSysSphereEmitter".to_string(),
    })
}

/// BSPSysArrayEmitter: modifier_base + emitter_base + volume_emitter (no own fields)
pub fn parse_array_emitter(stream: &mut NifStream) -> io::Result<NiPSysEmitter> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let params = read_volume_emitter_base(stream)?;
    Ok(NiPSysEmitter {
        params,
        original_type: "BSPSysArrayEmitter".to_string(),
    })
}

/// NiPSysMeshEmitter: modifier_base + emitter_base + num_meshes(u32) + mesh_ptrs[N] +
/// initial_velocity_type(u32) + emission_type(u32) + emission_axis(vec3)
pub fn parse_mesh_emitter(stream: &mut NifStream) -> io::Result<NiPSysEmitter> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let params = read_emitter_base(stream)?;
    let num_meshes = stream.read_u32_le()?;
    // Bound the mesh-ref loop against the remaining stream — same
    // defense as #388 / #407's NiParticleSystem path. Pre-#383 a junk
    // count from the broken emitter base could send this loop walking
    // 5 KB into the next block (visible in nif_stats as 5058-byte
    // over-reads on 97-byte blocks).
    stream.check_alloc((num_meshes as usize).saturating_mul(4))?;
    for _ in 0..num_meshes {
        let _mesh_ptr = stream.read_block_ref()?;
    }
    let _initial_velocity_type = stream.read_u32_le()?;
    let _emission_type = stream.read_u32_le()?;
    stream.skip(12)?; // emission_axis vec3
    Ok(NiPSysEmitter {
        params,
        original_type: "NiPSysMeshEmitter".to_string(),
    })
}

// ── Collider parsers ────────────────────────────────────────────────

/// NiPSysPlanarCollider: collider_base + 2 floats + 2 vec3s
pub fn parse_planar_collider(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    skip_collider_base(stream)?;
    stream.skip(4 * 2 + 12 * 2)?; // 2 floats + 2 vec3s
    Ok(NiPSysBlock {
        original_type: "NiPSysPlanarCollider".to_string(),
    })
}

/// NiPSysSphericalCollider: collider_base + 1 float
pub fn parse_spherical_collider(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    skip_collider_base(stream)?;
    let _radius = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysSphericalCollider".to_string(),
    })
}

// ── Field modifier parsers ──────────────────────────────────────────
//
// Pre-#984 the field-modifier parsers returned an opaque `NiPSysBlock`
// with every interesting scalar dropped on the floor. Per the
// NIF-D5-NEW-01 audit, particle systems carrying authored gravity /
// vortex / drag / turbulence / air / radial fields therefore rendered
// at heuristic preset config regardless of what the NIF wanted —
// dust devils didn't rotate, magic spell trails looked anemic.
//
// Each modifier now retains its full data payload as a dedicated
// downcastable struct. The importer's
// [`crate::import::walk::collect_force_fields`] walks
// `NiParticleSystem.modifier_refs` and extracts these into a
// `Vec<ParticleForceField>` that the ECS simulator integrates per
// frame. See #984 / NIF-D5-ORPHAN-A2.

/// Base fields shared across every `NiPSysFieldModifier` subclass,
/// captured here so the importer can read magnitude / attenuation /
/// max-distance gates without re-parsing the on-disk bytes.
#[derive(Debug, Clone)]
pub struct NiPSysFieldModifierBase {
    pub field_object_ref: BlockRef,
    pub magnitude: f32,
    pub attenuation: f32,
    pub use_max_distance: bool,
    pub max_distance: f32,
}

impl NiPSysFieldModifierBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let field_object_ref = stream.read_block_ref()?;
        let magnitude = stream.read_f32_le()?;
        let attenuation = stream.read_f32_le()?;
        let use_max_distance = stream.read_byte_bool()?;
        let max_distance = stream.read_f32_le()?;
        Ok(Self {
            field_object_ref,
            magnitude,
            attenuation,
            use_max_distance,
            max_distance,
        })
    }
}

/// `NiPSysGravityFieldModifier` — point-source / directional gravity.
/// `direction` is the gravity vector (NIF Z-up local space).
#[derive(Debug)]
pub struct NiPSysGravityFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub direction: [f32; 3],
}

impl NiPSysGravityFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let dx = stream.read_f32_le()?;
        let dy = stream.read_f32_le()?;
        let dz = stream.read_f32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            direction: [dx, dy, dz],
        })
    }
}

/// `NiPSysVortexFieldModifier` — rotational force around an axis.
/// `direction` is the rotation axis (NIF Z-up local space).
#[derive(Debug)]
pub struct NiPSysVortexFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub direction: [f32; 3],
}

impl NiPSysVortexFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let dx = stream.read_f32_le()?;
        let dy = stream.read_f32_le()?;
        let dz = stream.read_f32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            direction: [dx, dy, dz],
        })
    }
}

/// `NiPSysDragFieldModifier` — velocity-proportional damping. When
/// `use_direction` is false the drag is isotropic; when true the drag
/// is applied only along `direction`.
#[derive(Debug)]
pub struct NiPSysDragFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub use_direction: bool,
    pub direction: [f32; 3],
}

impl NiPSysDragFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let use_direction = stream.read_byte_bool()?;
        let dx = stream.read_f32_le()?;
        let dy = stream.read_f32_le()?;
        let dz = stream.read_f32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            use_direction,
            direction: [dx, dy, dz],
        })
    }
}

/// `NiPSysTurbulenceFieldModifier` — pseudo-random per-particle force.
/// `frequency` drives the noise sampling rate.
#[derive(Debug)]
pub struct NiPSysTurbulenceFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub frequency: f32,
}

impl NiPSysTurbulenceFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let frequency = stream.read_f32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            frequency,
        })
    }
}

/// `NiPSysAirFieldModifier` — directional wind with falloff. `direction`
/// is the wind vector, `air_friction` damps cross-wind motion,
/// `inherit_velocity` ties particle velocity to wind, and `spread`
/// authors a cone half-angle when `enable_spread` is set.
#[derive(Debug)]
pub struct NiPSysAirFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub direction: [f32; 3],
    pub air_friction: f32,
    pub inherit_velocity: f32,
    pub inherit_rotation: bool,
    pub component_only: bool,
    pub enable_spread: bool,
    pub spread: f32,
}

impl NiPSysAirFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let dx = stream.read_f32_le()?;
        let dy = stream.read_f32_le()?;
        let dz = stream.read_f32_le()?;
        let air_friction = stream.read_f32_le()?;
        let inherit_velocity = stream.read_f32_le()?;
        let inherit_rotation = stream.read_byte_bool()?;
        let component_only = stream.read_byte_bool()?;
        let enable_spread = stream.read_byte_bool()?;
        let spread = stream.read_f32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            direction: [dx, dy, dz],
            air_friction,
            inherit_velocity,
            inherit_rotation,
            component_only,
            enable_spread,
            spread,
        })
    }
}

/// `NiPSysRadialFieldModifier` — radial push/pull around the field
/// origin. `radial_type` is a Gamebryo enum (linear / quadratic /
/// constant); the simulator collapses it to a falloff exponent.
#[derive(Debug)]
pub struct NiPSysRadialFieldModifier {
    pub modifier_base: NiPSysModifierBase,
    pub field_base: NiPSysFieldModifierBase,
    pub radial_type: u32,
}

impl NiPSysRadialFieldModifier {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let modifier_base = NiPSysModifierBase::parse(stream)?;
        let field_base = NiPSysFieldModifierBase::parse(stream)?;
        let radial_type = stream.read_u32_le()?;
        Ok(Self {
            modifier_base,
            field_base,
            radial_type,
        })
    }
}

// ── Controller parsers ──────────────────────────────────────────────

/// NiPSysUpdateCtlr / NiPSysResetOnLoopCtlr: just NiTimeController base.
pub fn parse_time_controller(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    let _base = NiTimeControllerBase::parse(stream)?;
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysModifierCtlr chain: NiSingleInterpController + modifier_name(string).
/// Used by NiPSysEmitterCtlr (+ visibility_interpolator_ref) and all
/// NiPSysModifier*Ctlr aliases (no additional fields).
pub fn parse_modifier_ctlr(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    // NiPSysModifierCtlr inherits NiSingleInterpController → NiInterpController,
    // so it carries the Manager Controlled bool on the 10.1.0.104–108 band. (#1543)
    let _base = parse_interp_controller_base(stream)?;
    let _interpolator_ref = stream.read_block_ref()?; // NiSingleInterpController
    let _modifier_name = stream.read_string()?; // NiPSysModifierCtlr
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysEmitterCtlr: modifier_ctlr + visibility_interpolator_ref(ref)
pub fn parse_emitter_ctlr(stream: &mut NifStream) -> io::Result<NiPSysEmitterCtlr> {
    let _base = parse_interp_controller_base(stream)?;
    let interpolator_ref = stream.read_block_ref()?;
    let _modifier_name = stream.read_string()?;
    // NiPSysEmitterCtlr.Visibility Interpolator (Ref) — nif.xml since=10.1.0.104
    // (the pre-10.1.0.104 `Data` ref is the mutually-exclusive legacy slot).
    // Was wrongly gated >= V10_2_0_0, skipping the 4-byte ref on old-Gamebryo
    // FX in the 10.1.0.104–10.2 band. (#1544)
    if stream.version() >= NifVersion::V10_1_0_104 {
        let _vis_interpolator_ref = stream.read_block_ref()?;
    }
    Ok(NiPSysEmitterCtlr { interpolator_ref })
}

/// BSPSysMultiTargetEmitterCtlr (FO3+): emitter_ctlr + max_emitters(u16) + master_ref(ptr)
pub fn parse_multi_target_emitter_ctlr(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = parse_interp_controller_base(stream)?;
    let _interpolator_ref = stream.read_block_ref()?;
    let _modifier_name = stream.read_string()?;
    // Visibility Interpolator (Ref) — nif.xml since=10.1.0.104, not v10.2. (#1544)
    if stream.version() >= NifVersion::V10_1_0_104 {
        let _vis_interpolator_ref = stream.read_block_ref()?;
    }
    let _max_emitters = stream.read_u16_le()?;
    let _master_ref = stream.read_block_ref()?;
    Ok(NiPSysBlock {
        original_type: "BSPSysMultiTargetEmitterCtlr".to_string(),
    })
}

// ── Geometry node parsers (NiParticleSystem etc.) ────────────────────

/// NiParticles / NiParticleSystem / NiMeshParticleSystem / BSStripParticleSystem.
///
/// Inherits NiGeometry. The NiGeometry layout shifts at BSVER >= 100
/// (Skyrim SE, FO4, FO76, Starfield) — `onlyT="NiParticleSystem"` in
/// nif.xml swaps `data_ref` + `skin_instance_ref` + `material_data`
/// for an inline `bounding_sphere` (4 floats) + a single `skin` ref.
/// Shader / alpha property refs land at the end on every BSVER > 34
/// path. NiParticleSystem then adds (BSVER >= 100): `vertex_desc`
/// (u64), four `Far/Near Begin/End` (u16s), and a separate `data` ref
/// before the universal `world_space` (bool) + `num_modifiers` (u32) +
/// `modifier_refs[N]`.
///
/// Before #407 the parser missed the BS_GTE_SSE prefix on FO4 content
/// (~32 bytes per block), so `num_modifiers` read a junk u32 from
/// inside the missing payload. With a junk count of e.g. 3000, the
/// modifier-ref loop walked ~12 KB into the next block — the 75×
/// over-read the audit flagged.
/// Top-level particle-system block — capturing the `modifier_refs`
/// list so the importer can walk the chain into the
/// `NiPSysGravityFieldModifier` / `NiPSysVortexFieldModifier` /
/// `NiPSysDragFieldModifier` / `NiPSysTurbulenceFieldModifier` /
/// `NiPSysAirFieldModifier` / `NiPSysRadialFieldModifier` blocks and
/// surface authored force fields to the ECS simulator. See #984.
///
/// `original_type` discriminates between the five top-level types
/// (`NiParticleSystem` / `NiParticles` / `NiMeshParticleSystem` /
/// `BSStripParticleSystem` / `BSMasterParticleSystem`) so the walker
/// can preserve the pre-#984 telemetry. `modifier_refs` is empty for
/// `NiParticles` (the wire format has no modifier list on that path).
#[derive(Debug)]
pub struct NiParticleSystem {
    pub original_type: String,
    /// Local TRS (relative to the host `NiNode`) from the block's
    /// `NiAVObjectData` base. Pre-#1333 this was parsed into `_av` and
    /// dropped, so a particle block authored with a non-zero offset
    /// (campfire smoke above the fire, FO4 steam stacks) spawned at the
    /// host node origin. Both walkers now compose host-world × this to
    /// anchor the emitter. (`NiParticles` carries the same base; the
    /// field is harmless there.)
    pub transform: crate::types::NiTransform,
    pub modifier_refs: Vec<BlockRef>,
}

pub fn parse_particle_system(
    stream: &mut NifStream,
    type_name: &str,
) -> io::Result<NiParticleSystem> {
    use super::base::NiAVObjectData;
    use crate::version::NifVersion;

    // Retain the NiAVObjectData base (#1333): its `transform` is the
    // block's local TRS relative to the host node. Previously dropped
    // into `_av`, which silently zeroed any authored emitter offset.
    let av = NiAVObjectData::parse(stream)?;

    // BS_GTE_SSE = BSVER >= 100 (Skyrim SE / FO4 / FO76 / Starfield).
    // On this path NiGeometry's structure is overridden for
    // NiParticleSystem: bounding_sphere + skin ref replace the usual
    // data_ref + skin_instance_ref + material_data triplet.
    let is_bs_gte_sse = stream.bsver() >= crate::version::bsver::SKYRIM_SE;
    // FO76 (BSVER == 155) adds a 6-float `bound_min_max` after
    // `bounding_sphere`. Other SSE+ titles do not.
    let is_bs_f76 = stream.bsver() == crate::version::bsver::FO76;

    if is_bs_gte_sse {
        // Bounding sphere: 3 floats center + 1 float radius = 16 bytes.
        stream.skip(16)?;
        if is_bs_f76 {
            // Bound Min Max: 6 floats = 24 bytes.
            stream.skip(24)?;
        }
        let _skin_ref = stream.read_block_ref()?;
    } else {
        // Pre-SSE NiGeometry: data ref + skin instance ref + material data.
        let _data_ref = stream.read_block_ref()?;
        let _skin_ref = stream.read_block_ref()?;

        // Material data: num_materials(u32) + (name_idx, extra_data)[N] +
        // active_material_index(u32) + dirty_flag(u8 since v20.2.0.7).
        // Present since v20.2.0.5, only on the pre-SSE branch — same
        // gate `NiTriShape::parse` uses (see tri_shape.rs:108).
        if stream.version() >= NifVersion::V20_2_0_5 {
            let num_materials = stream.read_u32_le()?;
            // Each entry is 8 on-disk bytes (name_idx + extra_data); bound
            // the loop so a junk count can't OOM. See #388 / #407.
            stream.check_alloc((num_materials as usize).saturating_mul(8))?;
            for _ in 0..num_materials {
                let _mat_name_idx = stream.read_u32_le()?;
                let _mat_extra_data = stream.read_u32_le()?;
            }
            let _active_material_index = stream.read_u32_le()?;
            if stream.version() >= NifVersion::V20_2_0_7 {
                let _dirty_flag = stream.read_u8()?;
            }
        } else if stream.version() >= NifVersion::V10_0_1_0
            && stream.version() <= NifVersion::V20_1_0_3
        {
            // MaterialData "Has Shader" + name + impl. nif.xml gates
            // `since="10.0.1.0" until="20.1.0.3"`; both boundaries are
            // inclusive per the version.rs doctrine — present at v in
            // [10.0.1.0, 20.1.0.3].
            let has_shader = stream.read_bool()?;
            if has_shader {
                let _shader_name = stream.read_sized_string()?;
                let _implementation = stream.read_i32_le()?;
            }
        }
    }

    // Shader / alpha refs land on every BSVER > 34 path. Use raw bsver()
    // rather than a variant predicate so non-Bethesda Gamebryo content at
    // BSVER > 34 stays aligned (matches base.rs:73 and node.rs:107).
    if stream.bsver() > crate::version::bsver::FO3_FNV {
        let _shader_ref = stream.read_block_ref()?;
        let _alpha_ref = stream.read_block_ref()?;
    }

    // NiParticleSystem-specific fields (NiParticles has none).
    let mut modifier_refs: Vec<BlockRef> = Vec::new();
    if type_name != "NiParticles" {
        // SSE+ (BSVER >= 100): vertex_desc (u64) + Far/Near Begin/End
        // (4 × u16) + data ref. Skyrim LE (BSVER >= 83) has just the
        // Far/Near pairs without vertex_desc + data. Pre-Skyrim has
        // none of these.
        if is_bs_gte_sse {
            let _vertex_desc = stream.read_u64_le()?;
        }
        if stream.bsver() >= crate::version::bsver::SKYRIM_LE {
            let _far_begin = stream.read_u16_le()?;
            let _far_end = stream.read_u16_le()?;
            let _near_begin = stream.read_u16_le()?;
            let _near_end = stream.read_u16_le()?;
        }
        if is_bs_gte_sse {
            let _data_ref = stream.read_block_ref()?;
        }

        let _world_space = stream.read_byte_bool()?;
        let num_modifiers = stream.read_u32_le()?;
        // Bound the modifier-ref loop against the remaining stream so a
        // junk count from drifted bytes can't OOM the process or walk
        // 12 KB into the next block. Each ref is 4 bytes on disk.
        // See #388 / #407.
        stream.check_alloc((num_modifiers as usize).saturating_mul(4))?;
        modifier_refs.reserve_exact(num_modifiers as usize);
        for _ in 0..num_modifiers {
            modifier_refs.push(stream.read_block_ref()?);
        }
    }

    Ok(NiParticleSystem {
        original_type: type_name.to_string(),
        transform: av.transform,
        modifier_refs,
    })
}

// ── BSStripParticleSystem (FO3+): same as NiParticleSystem ──────────

/// BSStripParticleSystem: inherits NiParticleSystem, no own fields.
pub fn parse_strip_particle_system(stream: &mut NifStream) -> io::Result<NiParticleSystem> {
    parse_particle_system(stream, "BSStripParticleSystem")
}

// ── BSMasterParticleSystem: NiNode subclass ─────────────────────────

// ── Data block parsers ──────────────────────────────────────────────

/// NiParticlesData / NiPSysData / NiMeshPSysData / BSStripPSysData:
/// These inherit NiGeometryData (complex, variable-length).
/// Reuses the existing geometry data base parser then reads particle-specific fields.
///
/// Schema source: `docs/legacy/nif.xml` NiParticlesData (lines 3990–4016)
/// and NiPSysData (lines 4028–4038). Critical insight: Bethesda 20.2.0.7+
/// streams (`#BS202#` = FO3, FNV, Skyrim LE/SE, FO4, FO76, Starfield)
/// DROP all the per-particle arrays — only the bool headers are serialized,
/// and the data is stored elsewhere. Before #322 the parser read those
/// arrays unconditionally, walking thousands of bytes past the block end.
pub fn parse_particles_data(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    use crate::version::NifVersion;

    // BS202 stream = Bethesda file at version 20.2.0.7+ (bsver != 0). All
    // per-particle data arrays are *absent* on this path; only the bool
    // headers are written. See nif.xml `vercond="!#BS202#"` on every array.
    let is_bs_202 = stream.version() >= NifVersion::V20_2_0_7 && stream.bsver() > crate::version::bsver::PRE_BETHESDA;

    // NiGeometryData base (shared with NiTriShapeData). For NiPSysData on
    // BS_GTE_FO3, the Vertices/Normals/Tangents/Colors/UV arrays have length
    // 0 regardless of the bools — use the psys variant to suppress them.
    // See nif.xml line 3880 and #322.
    let use_psys_base = is_bs_202 && type_name != "NiParticlesData";
    let (_verts, _flags, _normals, _center, _radius, _colors, _uvs) = if use_psys_base {
        super::tri_shape::parse_psys_geometry_data_base(stream)?
    } else {
        super::tri_shape::parse_geometry_data_base(stream)?
    };
    // For the particle-specific arrays after the base, `count` is the raw
    // vertex count (non-zero on non-BS202, zero on BS202 via the psys base).
    let count = _verts.len() as u64;

    // Has Radii / Radii: since 10.1.0.0, array only on !BS202.
    if stream.version() >= NifVersion::V10_1_0_0 {
        let has_radii = stream.read_byte_bool()?;
        if has_radii && !is_bs_202 {
            stream.skip(count * 4)?;
        }
    }

    // Num Active Particles (u16) — always present.
    let _active_particles = stream.read_u16_le()?;

    // Has Sizes / Sizes: array only on !BS202.
    let has_sizes = stream.read_byte_bool()?;
    if has_sizes && !is_bs_202 {
        stream.skip(count * 4)?;
    }

    // Has Rotations / Rotations: since 10.0.1.0, array only on !BS202.
    if stream.version() >= NifVersion::V10_0_1_0 {
        let has_rotations = stream.read_byte_bool()?;
        if has_rotations && !is_bs_202 {
            stream.skip(count * 16)?; // quaternion (4×f32)
        }
    }

    // Has Rotation Angles / Rotation Angles: since 20.0.0.4, array only on !BS202.
    if stream.version() >= NifVersion::V20_0_0_4 {
        let has_rotation_angles = stream.read_byte_bool()?;
        if has_rotation_angles && !is_bs_202 {
            stream.skip(count * 4)?;
        }
        // Has Rotation Axes / Rotation Axes — same gating.
        let has_rotation_axes = stream.read_byte_bool()?;
        if has_rotation_axes && !is_bs_202 {
            stream.skip(count * 12)?; // Vector3
        }
    }

    // BS202-only trailing fields (texture atlas / subtexture offsets). These
    // were completely missing before #322 — they are the primary cause of
    // the massive over-reads in the FNV corpus.
    if is_bs_202 {
        let _has_texture_indices = stream.read_byte_bool()?;
        // Num Subtexture Offsets: byte for BSVER ≤ 34 (FO3/FNV), uint for
        // BSVER > 34 (Skyrim LE+). nif.xml lines 4008–4009.
        let num_subtex_offsets: u64 = if stream.bsver() <= crate::version::bsver::FO3_FNV {
            stream.read_u8()? as u64
        } else {
            stream.read_u32_le()? as u64
        };
        // Subtexture Offsets: Vector4 × count = 16 bytes each.
        stream.skip(num_subtex_offsets * 16)?;

        // Skyrim+ (BS_GT_FO3): aspect ratio, aspect flags, 3× speed-to-aspect.
        if stream.bsver() > crate::version::bsver::FO3_FNV {
            let _aspect_ratio = stream.read_f32_le()?;
            let _aspect_flags = stream.read_u16_le()?;
            let _speed_to_aspect_aspect_2 = stream.read_f32_le()?;
            let _speed_to_aspect_speed_1 = stream.read_f32_le()?;
            let _speed_to_aspect_speed_2 = stream.read_f32_le()?;
        }
    }

    // NiPSysData-specific fields. The Particle Info array (`!#BS202#`) and
    // the Num Added / Added Particles Base fields (also `!#BS202#`) are all
    // absent on Bethesda streams.
    if type_name != "NiParticlesData" {
        // Particle Info: NiParticleInfo × num_vertices. Per nif.xml line 4030
        // (`vercond="!#BS202#"`) the array is present on every non-BS202
        // stream — Oblivion (20.0.0.4) is the primary consumer in our corpus.
        // Pre-#581 the bytes were skipped entirely, so a 482-byte gap
        // (15 particles × 28 + 4-byte spawn-trailer) cascaded ~80% of the
        // Oblivion `NiUnknown` pool via runtime-size-cache recovery.
        //
        // Per-entry size depends on the stream version (NiParticleInfo
        // struct in nif.xml line 2263, `Rotation Axis until="10.4.0.1"`
        // inclusive per the version.rs doctrine):
        //   <= 10.4.0.1: Velocity(12) + Rotation Axis(12) + Age/Life/Update(12)
        //                + Spawn Generation(2) + Code(2)                  = 40 B
        //   >  10.4.0.1: Velocity(12) + Age/Life/Update(12)
        //                + Spawn Generation(2) + Code(2)                  = 28 B
        // Oblivion is 20.0.0.4 → 28-byte path.
        if !is_bs_202 {
            let info_size: u64 = if stream.version() <= NifVersion::V10_4_0_1 {
                40
            } else {
                28
            };
            stream.skip(count * info_size)?;
        }
        // Unknown Vector (Vector3, 12 B) — FO76 only (`#BS_F76#`, i.e. bsver
        // == 155 exactly per nif.xml). Sits between the (BS202-absent)
        // Particle Info array and Has Rotation Speeds; nif.xml NiPSysData.
        // Pre-#1896 these 12 bytes fell through to the dispatcher's block_size
        // seek — inert (nothing downstream reads them) but the stream sat 12 B
        // short here, so any future structural read of the later fields would
        // start misaligned on FO76. Consuming it makes `consumed == block_size`.
        if stream.bsver() == crate::version::bsver::FO76 {
            stream.skip(12)?; // Vector3
        }
        // Has Rotation Speeds (since 20.0.0.2) — bool always read; array !BS202.
        if stream.version() >= NifVersion::V20_0_0_2 {
            let has_rotation_speeds = stream.read_byte_bool()?;
            if has_rotation_speeds && !is_bs_202 {
                stream.skip(count * 4)?;
            }
        }
        // Num Added Particles + Added Particles Base: nif.xml gates these
        // on `vercond="!#BS202#"` with NO `since`, so they are present on
        // every non-Bethesda (Oblivion-era) NiPSysData regardless of
        // version — including the v10.x sub-versions. The old
        // `>= V20_0_0_2` gate dropped these 4 bytes on v10.1/v10.2
        // Oblivion particle NIFs, under-reading NiPSysData by 4 and
        // cascading truncation through the sizeless format (#1507).
        if !is_bs_202 {
            let _num_added = stream.read_u16_le()?;
            let _added_particles_base = stream.read_u16_le()?;
        }
    }

    // NiMeshPSysData-specific fields:
    if type_name == "NiMeshPSysData" {
        let _default_pool_size = stream.read_u32_le()?;
        let _fill_pools = stream.read_byte_bool()?;
        let num_generations = stream.read_u32_le()? as usize;
        for _ in 0..num_generations {
            let _pool_size = stream.read_u32_le()?;
        }
        let _mesh_ref = stream.read_block_ref()?;
    }

    // BSStripPSysData-specific fields (FO3+):
    if type_name == "BSStripPSysData" {
        let _max_point_count = stream.read_u16_le()?;
        let _start_cap_size = stream.read_f32_le()?;
        let _end_cap_size = stream.read_f32_le()?;
        let _do_z_prepass = stream.read_byte_bool()?;
    }

    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysEmitterCtlrData: KeyGroup<float> (visibility keys) + num(u32) + byte_key array.
/// Rare/deprecated but needs to parse correctly.
pub fn parse_emitter_ctlr_data(stream: &mut NifStream) -> io::Result<NiPSysEmitterCtlrData> {
    // KeyGroup<float> of birth-rate keys (particles/sec over time).
    let num_keys = stream.read_u32_le()?;
    let mut birth_rate_first = None;
    if num_keys > 0 {
        let interpolation = stream.read_u32_le()?;
        let key_size: u64 = match interpolation {
            1 | 5 => 8, // LINEAR / CONSTANT: time(f32) + value(f32)
            2 => 16,    // QUADRATIC: time + value + fwd + bwd
            3 => 20,    // TBC: time + value + tension + bias + continuity
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "NiPSysEmitterCtlrData: unknown float key interpolation {other} \
                         with {num_keys} keys — stream position unreliable"
                    ),
                ));
            }
        };
        // Capture the first key's birth-rate value (t=0); advance past
        // the rest of that key + every remaining key. `key_size - 8`
        // skips the tangent/TBC trailer (0 for LINEAR/CONSTANT).
        let _time = stream.read_f32_le()?;
        birth_rate_first = Some(stream.read_f32_le()?);
        stream.skip((key_size - 8) + key_size * (num_keys as u64 - 1))?;
    }
    // Visibility keys: num(u32) + Key<byte> array
    let num_vis = stream.read_u32_le()? as u64;
    if num_vis > 0 {
        // Each key<byte>: time(f32) + value(u8) = 5 bytes
        stream.skip(num_vis * 5)?;
    }
    Ok(NiPSysEmitterCtlrData { birth_rate_first })
}

/// BSMasterParticleSystem: NiNode + max_emitter_count(u16) + num_ptrs(u32) + ptrs[N]
pub fn parse_master_particle_system(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    use super::base::NiAVObjectData;

    let _av = NiAVObjectData::parse(stream)?;
    let _children = stream.read_block_ref_list()?;
    // FO4+ removes the effects list from NiNode (BSVER >= 130). Raw
    // bsver check to keep non-Bethesda Unknown variants correct — see #160.
    if stream.bsver() < crate::version::bsver::FALLOUT4 {
        let _effects = stream.read_block_ref_list()?;
    }
    let _max_emitter_count = stream.read_u16_le()?;
    let num_ptrs = stream.read_u32_le()?;
    // Bound the ptr loop against remaining stream — same defense as
    // NiParticleSystem's modifier_refs (#388 / #407).
    stream.check_alloc((num_ptrs as usize).saturating_mul(4))?;
    for _ in 0..num_ptrs {
        let _ptr = stream.read_block_ref()?;
    }
    Ok(NiPSysBlock {
        original_type: "BSMasterParticleSystem".to_string(),
    })
}

impl_ni_object!(
    NiPSysBlock,
    NiPSysEmitter,
    NiPSysEmitterCtlr,
    NiPSysEmitterCtlrData,
    NiPSysGrowFadeModifier,
    NiPSysColorModifier,
    BSPSysSimpleColorModifier,
    NiPSysGravityFieldModifier,
    NiPSysVortexFieldModifier,
    NiPSysDragFieldModifier,
    NiPSysTurbulenceFieldModifier,
    NiPSysAirFieldModifier,
    NiPSysRadialFieldModifier,
    NiParticleSystem,
);

#[cfg(test)]
#[path = "particle_tests.rs"]
mod tests;
