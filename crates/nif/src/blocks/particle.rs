//! Particle system block parsers — NiPSys* types for Oblivion through Skyrim.
//!
//! These blocks define particle effects (fire, magic, weather, etc.).
//! Parse-only — no rendering. The goal is byte-correct consumption so
//! subsequent blocks parse correctly, especially on Oblivion NIFs which
//! have no block_size fallback.

use super::controller::NiTimeControllerBase;
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
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

/// Base fields for NiPSysEmitter: speed, variation, declination, color, etc.
fn skip_emitter_base(stream: &mut NifStream) -> io::Result<()> {
    // NiPSysEmitter (nif.xml line 3579) — base 12 floats are universal:
    //   speed + speed_variation + declination + declination_variation +
    //   planar_angle + planar_angle_variation +
    //   initial_color (Color4 = 4 floats) +
    //   initial_radius + life_span = 12 floats = 48 bytes.
    stream.skip(4 * 12)?;
    // Bethesda BS_GTE_FO3 (BSVER >= 34) adds two trailing floats:
    // `radius_variation` (since 10.4.0.1) and `life_span_variation`,
    // bumping the base to 14 floats = 56 bytes. Empirically:
    //   FNV (BSVER 34, version 20.2.0.7): 14 floats (audit D5-F2 — 8-byte
    //   under-read on every Box/Cylinder/Sphere/Mesh emitter pre-#383).
    //   Oblivion (BSVER 11, version 20.0.0.5): 12 floats (the BS_GTE_FO3
    //   gate keeps these 2 floats out, so we don't over-read and shift
    //   downstream blocks into garbage).
    if stream.bsver() >= 34 {
        stream.skip(4 * 2)?;
    }
    Ok(())
}

/// NiPSysVolumeEmitter adds: emitter_object_ref (ptr/i32).
fn skip_volume_emitter_base(stream: &mut NifStream) -> io::Result<()> {
    skip_emitter_base(stream)?;
    let _emitter_object_ref = stream.read_block_ref()?;
    Ok(())
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
    // world_aligned: since v20.0.0.4
    if stream.version() >= crate::version::NifVersion(0x14000004) {
        let _world_aligned = stream.read_byte_bool()?;
    }
    Ok(NiPSysBlock {
        original_type: "NiPSysGravityModifier".to_string(),
    })
}

/// NiPSysGrowFadeModifier: base + grow_time(f32) + grow_generation(u16) +
/// fade_time(f32) + fade_generation(u16) + base_scale(f32) [BS_GTE_FO3 +
/// version 20.2.0.7]
pub fn parse_grow_fade_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _grow_time = stream.read_f32_le()?;
    let _grow_generation = stream.read_u16_le()?;
    let _fade_time = stream.read_f32_le()?;
    let _fade_generation = stream.read_u16_le()?;
    // Bethesda 20.2.0.7 + BS_GTE_FO3 (BSVER >= 34): adds Base Scale.
    // Per nif.xml line 4803. FNV/Skyrim/FO4 all match this gate. Pre-#383
    // these 4 bytes were dropped on every grow-fade modifier (890
    // occurrences in vanilla `Fallout - Meshes.bsa`).
    if stream.version() == crate::version::NifVersion::V20_2_0_7 && stream.bsver() >= 34 {
        let _base_scale = stream.read_f32_le()?;
    }
    Ok(NiPSysBlock {
        original_type: "NiPSysGrowFadeModifier".to_string(),
    })
}

/// NiPSysRotationModifier: base + initial_speed(f32) + [since 20.0.0.2]
/// speed_variation(f32) + initial_angle(f32) + angle_variation(f32) +
/// random_rot_speed_sign(bool) + random_axis(bool) + axis(vec3)
pub fn parse_rotation_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _initial_speed = stream.read_f32_le()?;
    // speed_variation + initial_angle + angle_variation: since v20.0.0.2
    if stream.version() >= crate::version::NifVersion(0x14000002) {
        stream.skip(4 * 3)?; // 3 floats
    }
    // Random Rot Speed Sign — nif.xml line 4878 says `since 20.0.0.2`,
    // but empirically Oblivion (BSVER 11, version 20.0.0.5) does NOT
    // emit this byte (pre-#383 the 42-byte parser ran clean on every
    // Oblivion mesh). FNV (BSVER 34, version 20.2.0.7) DOES emit it
    // (the audit measured a 1-byte under-read). Gate on BS_GTE_FO3
    // (BSVER >= 34) — empirical match instead of nif.xml's overly-broad
    // version gate. 1,149 occurrences fixed in vanilla
    // `Fallout - Meshes.bsa`.
    if stream.bsver() >= 34 {
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

/// BSPSysSimpleColorModifier (FO3+): base + 6 floats + 3 Color4s
pub fn parse_simple_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    stream.skip(4 * 6 + 4 * 4 * 3)?; // 6 floats + 3 Color4
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
pub fn parse_box_emitter(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_volume_emitter_base(stream)?;
    stream.skip(4 * 3)?; // width, height, depth
    Ok(NiPSysBlock {
        original_type: "NiPSysBoxEmitter".to_string(),
    })
}

/// NiPSysCylinderEmitter: modifier_base + emitter_base + volume_emitter + 2 floats
pub fn parse_cylinder_emitter(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_volume_emitter_base(stream)?;
    stream.skip(4 * 2)?; // radius, height
    Ok(NiPSysBlock {
        original_type: "NiPSysCylinderEmitter".to_string(),
    })
}

/// NiPSysSphereEmitter: modifier_base + emitter_base + volume_emitter + 1 float
pub fn parse_sphere_emitter(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_volume_emitter_base(stream)?;
    let _radius = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysSphereEmitter".to_string(),
    })
}

/// BSPSysArrayEmitter: modifier_base + emitter_base + volume_emitter (no own fields)
pub fn parse_array_emitter(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_volume_emitter_base(stream)?;
    Ok(NiPSysBlock {
        original_type: "BSPSysArrayEmitter".to_string(),
    })
}

/// NiPSysMeshEmitter: modifier_base + emitter_base + num_meshes(u32) + mesh_ptrs[N] +
/// initial_velocity_type(u32) + emission_type(u32) + emission_axis(vec3)
pub fn parse_mesh_emitter(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_emitter_base(stream)?;
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
    Ok(NiPSysBlock {
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
    let _base = NiTimeControllerBase::parse(stream)?;
    let _interpolator_ref = stream.read_block_ref()?; // NiSingleInterpController
    let _modifier_name = stream.read_string()?; // NiPSysModifierCtlr
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysEmitterCtlr: modifier_ctlr + visibility_interpolator_ref(ref)
pub fn parse_emitter_ctlr(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiTimeControllerBase::parse(stream)?;
    let _interpolator_ref = stream.read_block_ref()?;
    let _modifier_name = stream.read_string()?;
    // NiPSysEmitterCtlr adds visibility interpolator ref (since v10.2)
    if stream.version() >= crate::version::NifVersion(0x0A020000) {
        let _vis_interpolator_ref = stream.read_block_ref()?;
    }
    Ok(NiPSysBlock {
        original_type: "NiPSysEmitterCtlr".to_string(),
    })
}

/// BSPSysMultiTargetEmitterCtlr (FO3+): emitter_ctlr + max_emitters(u16) + master_ref(ptr)
pub fn parse_multi_target_emitter_ctlr(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiTimeControllerBase::parse(stream)?;
    let _interpolator_ref = stream.read_block_ref()?;
    let _modifier_name = stream.read_string()?;
    if stream.version() >= crate::version::NifVersion(0x0A020000) {
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
    pub modifier_refs: Vec<BlockRef>,
}

pub fn parse_particle_system(
    stream: &mut NifStream,
    type_name: &str,
) -> io::Result<NiParticleSystem> {
    use super::base::NiAVObjectData;
    use crate::version::NifVersion;

    let _av = NiAVObjectData::parse(stream)?;

    // BS_GTE_SSE = BSVER >= 100 (Skyrim SE / FO4 / FO76 / Starfield).
    // On this path NiGeometry's structure is overridden for
    // NiParticleSystem: bounding_sphere + skin ref replace the usual
    // data_ref + skin_instance_ref + material_data triplet.
    let is_bs_gte_sse = stream.bsver() >= 100;
    // FO76 (BSVER == 155) adds a 6-float `bound_min_max` after
    // `bounding_sphere`. Other SSE+ titles do not.
    let is_bs_f76 = stream.bsver() == 155;

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
        if stream.version() >= NifVersion(0x14020005) {
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
        } else if stream.version() >= NifVersion(0x0A000100)
            && stream.version() <= NifVersion(0x14010003)
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
    if stream.bsver() > 34 {
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
        if stream.bsver() >= 83 {
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
    let is_bs_202 = stream.version() >= NifVersion(0x14020007) && stream.bsver() > 0;

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
    if stream.version() >= NifVersion(0x0A000100) {
        let has_rotations = stream.read_byte_bool()?;
        if has_rotations && !is_bs_202 {
            stream.skip(count * 16)?; // quaternion (4×f32)
        }
    }

    // Has Rotation Angles / Rotation Angles: since 20.0.0.4, array only on !BS202.
    if stream.version() >= NifVersion(0x14000004) {
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
        let num_subtex_offsets: u64 = if stream.bsver() <= 34 {
            stream.read_u8()? as u64
        } else {
            stream.read_u32_le()? as u64
        };
        // Subtexture Offsets: Vector4 × count = 16 bytes each.
        stream.skip(num_subtex_offsets * 16)?;

        // Skyrim+ (BS_GT_FO3): aspect ratio, aspect flags, 3× speed-to-aspect.
        if stream.bsver() > 34 {
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
            let info_size: u64 = if stream.version() <= NifVersion(0x0A040001) {
                40
            } else {
                28
            };
            stream.skip(count * info_size)?;
        }
        // Has Rotation Speeds (since 20.0.0.2) — bool always read; array !BS202.
        if stream.version() >= NifVersion(0x14000002) {
            let has_rotation_speeds = stream.read_byte_bool()?;
            if has_rotation_speeds && !is_bs_202 {
                stream.skip(count * 4)?;
            }
        }
        // Num Added Particles + Added Particles Base: !#BS202# only.
        if !is_bs_202 && stream.version() >= NifVersion(0x14000002) {
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
pub fn parse_emitter_ctlr_data(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    // KeyGroup<float> for float keys
    let num_keys = stream.read_u32_le()?;
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
        stream.skip(key_size * num_keys as u64)?;
    }
    // Visibility keys: num(u32) + Key<byte> array
    let num_vis = stream.read_u32_le()? as u64;
    if num_vis > 0 {
        // Each key<byte>: time(f32) + value(u8) = 5 bytes
        stream.skip(num_vis * 5)?;
    }
    Ok(NiPSysBlock {
        original_type: "NiPSysEmitterCtlrData".to_string(),
    })
}

/// BSMasterParticleSystem: NiNode + max_emitter_count(u16) + num_ptrs(u32) + ptrs[N]
pub fn parse_master_particle_system(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    use super::base::NiAVObjectData;

    let _av = NiAVObjectData::parse(stream)?;
    let _children = stream.read_block_ref_list()?;
    // FO4+ removes the effects list from NiNode (BSVER >= 130). Raw
    // bsver check to keep non-Bethesda Unknown variants correct — see #160.
    if stream.bsver() < 130 {
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
    NiPSysColorModifier,
    NiPSysGravityFieldModifier,
    NiPSysVortexFieldModifier,
    NiPSysDragFieldModifier,
    NiPSysTurbulenceFieldModifier,
    NiPSysAirFieldModifier,
    NiPSysRadialFieldModifier,
    NiParticleSystem,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::version::NifVersion;

    /// FO4-style header (version 20.2.0.7, BSVER 130). The `strings`
    /// table has one entry so a name index of 0 resolves; -1 = None.
    fn make_header_fo4() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 130,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("Sparks")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    /// Hand-build the byte sequence for a minimal FO4 NiParticleSystem
    /// with `num_modifiers` modifier refs. Layout follows nif.xml's
    /// BS_GTE_SSE branch — see [`parse_particle_system`].
    fn build_fo4_particle_system_bytes(num_modifiers: u32) -> Vec<u8> {
        let mut d = Vec::new();

        // ── NiObjectNETData ─────────────────────────────────────────
        d.extend_from_slice(&(-1i32).to_le_bytes()); // name = None (string index -1)
        d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
        d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = NULL

        // ── NiAVObject extension (bsver > 26 ⇒ flags=u32) ──────────
        d.extend_from_slice(&14u32.to_le_bytes()); // flags
        for _ in 0..3 {
            // translation
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = if row == col { 1.0 } else { 0.0 };
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                    // No properties list (bsver > 34).
        d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref

        // ── BS_GTE_SSE NiGeometry override ─────────────────────────
        // Bounding sphere: 4 floats.
        for _ in 0..4 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_ref

        // ── Shader / alpha refs (bsver > 34) ───────────────────────
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());

        // ── NiParticleSystem own (BS_GTE_SSE) ──────────────────────
        d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc
        d.extend_from_slice(&0u16.to_le_bytes()); // far_begin
        d.extend_from_slice(&0u16.to_le_bytes()); // far_end
        d.extend_from_slice(&0u16.to_le_bytes()); // near_begin
        d.extend_from_slice(&0u16.to_le_bytes()); // near_end
        d.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref

        // ── Universal trailer ──────────────────────────────────────
        d.push(1u8); // world_space = true
        d.extend_from_slice(&num_modifiers.to_le_bytes());
        for i in 0..num_modifiers {
            d.extend_from_slice(&(i as i32).to_le_bytes());
        }

        d
    }

    /// Regression: #407 — pre-fix, parse_particle_system on FO4
    /// (BSVER 130) skipped the BS_GTE_SSE bounding-sphere/skin/vertex_desc/
    /// far-near/data prefix and read `world_space` + `num_modifiers` from
    /// inside that missing payload. With even one byte of stream beyond the
    /// real block, `num_modifiers` would soak up an arbitrary u32 and
    /// the loop walked thousands of bytes into the next block (the 75×
    /// over-read). The fix consumes the prefix correctly so the parser
    /// lands exactly on the trailing modifier refs.
    #[test]
    fn parse_particle_system_fo4_consumes_full_block() {
        let header = make_header_fo4();
        let bytes = build_fo4_particle_system_bytes(2);

        // 72 (NiAVObject) + 20 (BS_GTE_SSE NiGeo) + 8 (shader/alpha)
        // + 20 (vertex_desc + far/near + data ref) + 13 (world_space +
        // num_modifiers + 2 refs) = 133.
        assert_eq!(
            bytes.len(),
            133,
            "fixture size drift — recheck the BS_GTE_SSE field list"
        );

        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_particle_system(&mut stream, "NiParticleSystem")
            .expect("FO4 NiParticleSystem should parse cleanly");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "parser must consume the full block — drift here is the #407 over-read"
        );
        assert_eq!(block.original_type, "NiParticleSystem");
    }

    /// Regression: a junk `num_modifiers` (here `u32::MAX`) must be
    /// rejected by the in-stream `check_alloc` gate before the loop
    /// can spin trying to read 16 GB of refs. Pre-#407 this would
    /// have consumed the rest of the stream + EOF'd; now it short-
    /// circuits with `InvalidData`.
    #[test]
    fn parse_particle_system_rejects_junk_num_modifiers() {
        let header = make_header_fo4();
        // Build a fixture with 0 trailing refs but a corrupt count.
        let mut bytes = build_fo4_particle_system_bytes(0);
        // Overwrite the num_modifiers field. It sits 4 bytes before
        // the end (just after world_space).
        let nm_offset = bytes.len() - 4;
        bytes[nm_offset..nm_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        let mut stream = NifStream::new(&bytes, &header);
        let err = parse_particle_system(&mut stream, "NiParticleSystem")
            .expect_err("junk num_modifiers must short-circuit");
        let msg = err.to_string();
        assert!(
            msg.contains("exceeds hard cap")
                || msg.contains("only ") && msg.contains("bytes remaining"),
            "expected check_alloc rejection, got: {msg}"
        );
    }

    /// FNV-style header (version 20.2.0.7, BSVER 34). Used by the #383
    /// regression tests for FNV-era particle modifiers / emitters.
    fn make_header_fnv() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("Mod")],
            max_string_length: 4,
            num_groups: 0,
        }
    }

    /// `NiPSysModifierBase` payload (string index + order + target + active).
    /// 13 bytes on the v20.2.0.7 path (string is a 4-byte index).
    fn modifier_base_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&(-1i32).to_le_bytes()); // name = None
        d.extend_from_slice(&0u32.to_le_bytes()); // order
        d.extend_from_slice(&(-1i32).to_le_bytes()); // target ref
        d.push(1u8); // active
        d
    }

    /// Regression: #383 — `skip_emitter_base` was reading 12 floats
    /// (48 bytes) where nif.xml requires 14 (56 bytes). Every Box /
    /// Cylinder / Sphere / Mesh emitter under-read by 8 bytes; the
    /// downstream consequence on `parse_mesh_emitter` was that
    /// `num_meshes` got a junk u32 from inside the missing fields and
    /// the loop walked thousands of bytes into the next block (5,058-
    /// byte over-reads on 97-byte blocks observed pre-fix).
    ///
    /// Verified directly on `parse_sphere_emitter` since it's the
    /// shortest of the volume emitters and exercises the entire
    /// modifier+emitter+volume+radius chain.
    #[test]
    fn parse_sphere_emitter_consumes_full_block() {
        let header = make_header_fnv();
        let mut d = modifier_base_bytes();
        // 56 bytes of emitter base (14 floats), zeroed.
        d.extend_from_slice(&[0u8; 56]);
        // 4 bytes for the volume emitter object ref.
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // 4 bytes radius.
        d.extend_from_slice(&1.5f32.to_le_bytes());

        // 13 base + 56 emitter + 4 volume + 4 radius = 77 bytes
        // (matches the FNV nif_stats observed `expected 77`).
        assert_eq!(d.len(), 77);

        let mut stream = NifStream::new(&d, &header);
        let block = parse_sphere_emitter(&mut stream)
            .expect("FNV NiPSysSphereEmitter should parse cleanly");
        assert_eq!(stream.position() as usize, d.len());
        assert_eq!(block.original_type, "NiPSysSphereEmitter");
    }

    /// Regression: #383 — `parse_grow_fade_modifier` on FNV
    /// (BS_GTE_FO3 + version 20.2.0.7) was missing the trailing
    /// `Base Scale: f32` per nif.xml line 4803. 890 occurrences in
    /// vanilla `Fallout - Meshes.bsa` under-read by 4 bytes.
    #[test]
    fn parse_grow_fade_modifier_reads_base_scale_on_bs_gte_fo3() {
        let header = make_header_fnv();
        let mut d = modifier_base_bytes();
        d.extend_from_slice(&1.0f32.to_le_bytes()); // grow_time
        d.extend_from_slice(&0u16.to_le_bytes()); // grow_generation
        d.extend_from_slice(&2.0f32.to_le_bytes()); // fade_time
        d.extend_from_slice(&0u16.to_le_bytes()); // fade_generation
        d.extend_from_slice(&3.0f32.to_le_bytes()); // base_scale (BS_GTE_FO3)

        // 13 base + 12 (grow_time + grow_gen + fade_time + fade_gen) +
        // 4 (base_scale) = 29 bytes (matches FNV nif_stats observed
        // `expected 29`).
        assert_eq!(d.len(), 29);

        let mut stream = NifStream::new(&d, &header);
        let block = parse_grow_fade_modifier(&mut stream)
            .expect("FNV NiPSysGrowFadeModifier should parse cleanly");
        assert_eq!(stream.position() as usize, d.len());
        assert_eq!(block.original_type, "NiPSysGrowFadeModifier");
    }

    /// Oblivion-style header (V20_0_0_4, BSVER 11). The `strings` table
    /// is empty — NiPSysData carries no name-indexed fields on this path.
    fn make_header_oblivion() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_4,
            little_endian: true,
            user_version: 0,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Build the NiGeometryData base header bytes that
    /// `parse_geometry_data_base` consumes for the supplied (version,
    /// num_vertices, has_vertices, has_additional_data) shape, with
    /// data_flags/normals/colors/UVs all empty. Mirrors the reader at
    /// `crates/nif/src/blocks/tri_shape.rs:787` exactly so test fixtures
    /// stay in lockstep with the schema gates.
    fn nigeo_base_bytes(num_vertices: u16, version: NifVersion) -> Vec<u8> {
        let mut d = Vec::new();
        // group_id since 10.1.0.114
        if version >= NifVersion(0x0A010072) {
            d.extend_from_slice(&0i32.to_le_bytes());
        }
        d.extend_from_slice(&num_vertices.to_le_bytes());
        // keep/compress flags since 10.1.0.0
        if version >= NifVersion::V10_1_0_0 {
            d.push(0u8);
            d.push(0u8);
        }
        d.push((num_vertices > 0) as u8); // has_vertices
        for _ in 0..num_vertices {
            d.extend_from_slice(&0.0f32.to_le_bytes());
            d.extend_from_slice(&0.0f32.to_le_bytes());
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // data_flags since 10.0.1.0
        if version >= NifVersion(0x0A000100) {
            d.extend_from_slice(&0u16.to_le_bytes());
        }
        d.push(0u8); // has_normals = false
                     // bounding sphere (12 + 4)
        for _ in 0..4 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        d.push(0u8); // has_vertex_colors = false
                     // (no UV sets — data_flags = 0)
                     // consistency_flags since 10.0.1.0
        if version >= NifVersion(0x0A000100) {
            d.extend_from_slice(&0u16.to_le_bytes());
        }
        // additional_data_ref since 20.0.0.4
        if version >= NifVersion(0x14000004) {
            d.extend_from_slice(&(-1i32).to_le_bytes());
        }
        d
    }

    /// Regression: #581 — `parse_particles_data` skipped the `Particle Info`
    /// array entirely on pre-BS202 streams. Oblivion 20.0.0.4 has bsver=11
    /// but version < 20.2.0.7 so `is_bs_202 = false`, meaning every
    /// NiPSysData blob's particle metadata (28 bytes per particle on
    /// post-10.4.0.1 streams) used to vanish from the cursor and cascade
    /// drift into every following block. Test asserts the parser now
    /// consumes exactly the byte range the Particle Info array occupies.
    #[test]
    fn parse_particles_data_skips_particle_info_on_oblivion() {
        let header = make_header_oblivion();
        let mut d = nigeo_base_bytes(2, header.version);
        // NiParticlesData tail (Bethesda-particle subset on !BS202 / Oblivion).
        d.push(0u8); // has_radii (since 10.1.0.0)
        d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
        d.push(0u8); // has_sizes
        d.push(0u8); // has_rotations (since 10.0.1.0)
        d.push(0u8); // has_rotation_angles (since 20.0.0.4)
        d.push(0u8); // has_rotation_axes
                     // BS202 trailers — Oblivion is !is_bs_202 → not present.

        // NiPSysData own: Particle Info × num_vertices. Oblivion is
        // post-10.4.0.1 → 28 B per particle × 2 = 56 B. Pre-#581 these
        // 56 bytes were not consumed and cascaded into block drift.
        d.extend_from_slice(&[0u8; 2 * 28]);

        d.push(0u8); // has_rotation_speeds (since 20.0.0.2)
        d.extend_from_slice(&0u16.to_le_bytes()); // num_added (!BS202)
        d.extend_from_slice(&0u16.to_le_bytes()); // added_particles_base

        let mut stream = NifStream::new(&d, &header);
        let block = parse_particles_data(&mut stream, "NiPSysData")
            .expect("Oblivion NiPSysData with 2 particles should parse cleanly");
        assert_eq!(
            stream.position() as usize,
            d.len(),
            "stream must land exactly at end-of-block — Particle Info skip is what closes the gap"
        );
        assert_eq!(block.original_type, "NiPSysData");
    }

    /// Sibling guard: pre-10.4.0.1 streams (Morrowind 4.x, early Gamebryo)
    /// carry the 40-byte NiParticleInfo layout (Rotation Axis is
    /// `until="10.4.0.1"` per nif.xml line 2267). Test parses NiPSysData
    /// at the boundary version 10.4.0.1 with `num_vertices = 1` to
    /// confirm the version branch picks 40 B.
    ///
    /// NOTE: NiPSysData isn't widespread in real pre-10.4 content; the
    /// test is a pure version-branch guard that the 40-byte path is
    /// reachable and exact.
    #[test]
    fn parse_particles_data_uses_40_byte_particle_info_on_pre_10_4_0_1() {
        // v10.4.0.0 sits inside the v10.4.0.1 `until=` boundary (inclusive
        // per the version.rs doctrine). The Rotation Axis is present at
        // v <= 10.4.0.1; the layout shrinks to 28 B starting at v10.4.0.2.
        // This test exercises the 40-byte legacy layout.
        let version = NifVersion(0x0A040000);
        let header = NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut d = nigeo_base_bytes(1, version);
        // NiParticlesData tail at 10.4.0.1: has_radii since 10.1.0.0 ✓,
        // has_rotations since 10.0.1.0 ✓; has_rotation_angles/axes
        // since 20.0.0.4 → NOT present.
        d.push(0u8); // has_radii
        d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
        d.push(0u8); // has_sizes
        d.push(0u8); // has_rotations

        // Particle Info: 1 × 40 = 40 B (pre-10.4.0.1 layout — Rotation
        // Axis present).
        d.extend_from_slice(&[0u8; 40]);

        // has_rotation_speeds gated on >= 20.0.0.2; 10.4.0.1 < that → not
        // read. Same for num_added / added_particles_base.

        let mut stream = NifStream::new(&d, &header);
        let block = parse_particles_data(&mut stream, "NiPSysData")
            .expect("10.4.0.1 NiPSysData should parse with 40-byte Particle Info");
        assert_eq!(
            stream.position() as usize,
            d.len(),
            "10.4.0.1 layout must consume the full 40 B per particle (Rotation Axis present)"
        );
        assert_eq!(block.original_type, "NiPSysData");
    }

    /// Sibling guard: BS202 streams (FO3+ at 20.2.0.7+) MUST NOT skip
    /// the Particle Info bytes — the array is `vercond="!#BS202#"`, so
    /// the field is absent on Bethesda streams entirely. Pre-fix
    /// behavior on this path was already correct; the test guards
    /// against a future refactor accidentally dropping the `!is_bs_202`
    /// gate.
    #[test]
    fn parse_particles_data_does_not_skip_particle_info_on_bs202() {
        let header = make_header_fnv(); // 20.2.0.7, bsver=34 → is_bs_202 = true
                                        // BS202+non-NiParticlesData uses parse_psys_geometry_data_base
                                        // → array_count = 0 regardless of has_vertices, but the bool
                                        // headers are still serialized.
        let mut d = nigeo_base_bytes(0, header.version);
        d.push(0u8); // has_radii
        d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
        d.push(0u8); // has_sizes
        d.push(0u8); // has_rotations
        d.push(0u8); // has_rotation_angles
        d.push(0u8); // has_rotation_axes
                     // BS202 trailers (FNV bsver=34 → byte-sized num_subtex, no
                     // bsver>34 aspect block).
        d.push(0u8); // has_texture_indices
        d.push(0u8); // num_subtex_offsets (byte)
                     // NO Particle Info — gated on !is_bs_202 (false here).
        d.push(0u8); // has_rotation_speeds (since 20.0.0.2)
                     // NO num_added / added_particles_base — !is_bs_202 (false here).

        let mut stream = NifStream::new(&d, &header);
        let block = parse_particles_data(&mut stream, "NiPSysData")
            .expect("FNV NiPSysData should parse cleanly with no Particle Info skip");
        assert_eq!(
            stream.position() as usize,
            d.len(),
            "BS202 path must NOT skip Particle Info bytes (the field is absent)"
        );
        assert_eq!(block.original_type, "NiPSysData");
    }

    /// Regression: #383 — `parse_rotation_modifier` was missing the
    /// `Random Rot Speed Sign: bool` field (since 20.0.0.2 per nif.xml
    /// line 4878). 1,149 occurrences in vanilla `Fallout - Meshes.bsa`
    /// under-read by 1 byte.
    #[test]
    fn parse_rotation_modifier_reads_random_rot_speed_sign_post_20_0_0_2() {
        let header = make_header_fnv();
        let mut d = modifier_base_bytes();
        d.extend_from_slice(&1.0f32.to_le_bytes()); // initial_speed
        d.extend_from_slice(&0.5f32.to_le_bytes()); // speed_variation
        d.extend_from_slice(&0.0f32.to_le_bytes()); // initial_angle
        d.extend_from_slice(&0.1f32.to_le_bytes()); // angle_variation
        d.push(0u8); // random_rot_speed_sign (since 20.0.0.2)
        d.push(1u8); // random_axis
        d.extend_from_slice(&[0u8; 12]); // axis vec3

        // 13 base + 16 (initial_speed + 3 vars) + 1 (rot_sign) +
        // 1 (random_axis) + 12 (vec3) = 43 bytes (matches FNV
        // nif_stats observed `expected 43`).
        assert_eq!(d.len(), 43);

        let mut stream = NifStream::new(&d, &header);
        let block = parse_rotation_modifier(&mut stream)
            .expect("FNV NiPSysRotationModifier should parse cleanly");
        assert_eq!(stream.position() as usize, d.len());
        assert_eq!(block.original_type, "NiPSysRotationModifier");
    }
}
