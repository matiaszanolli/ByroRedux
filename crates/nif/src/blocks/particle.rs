//! Particle system block parsers — NiPSys* types for Oblivion through Skyrim.
//!
//! These blocks define particle effects (fire, magic, weather, etc.).
//! Parse-only — no rendering. The goal is byte-correct consumption so
//! subsequent blocks parse correctly, especially on Oblivion NIFs which
//! have no block_size fallback.

use super::controller::NiTimeControllerBase;
use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
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
    // NiPSysEmitter adds: speed(f32) + speed_variation(f32) + declination(f32) +
    // declination_variation(f32) + planar_angle(f32) + planar_angle_variation(f32) +
    // initial_color(Color4=4×f32) + initial_radius(f32) + radius_variation(f32) +
    // life_span(f32) + life_span_variation(f32)
    stream.skip(4 * 12)?; // 12 floats = 48 bytes
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

// ── NiPSysFieldModifier base (extends NiPSysModifier) ──────────────

fn skip_field_modifier_base(stream: &mut NifStream) -> io::Result<()> {
    // field_object_ref(ptr) + magnitude(f32) + attenuation(f32) +
    // use_max_distance(bool) + max_distance(f32)
    let _field_object = stream.read_block_ref()?;
    let _magnitude = stream.read_f32_le()?;
    let _attenuation = stream.read_f32_le()?;
    let _use_max_distance = stream.read_byte_bool()?;
    let _max_distance = stream.read_f32_le()?;
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

impl NiObject for NiPSysBlock {
    fn block_type_name(&self) -> &'static str {
        "NiPSysBlock"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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

/// NiPSysColorModifier: base + color_data_ref(ref)
pub fn parse_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _color_data_ref = stream.read_block_ref()?;
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
/// fade_time(f32) + fade_generation(u16)
pub fn parse_grow_fade_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _grow_time = stream.read_f32_le()?;
    let _grow_generation = stream.read_u16_le()?;
    let _fade_time = stream.read_f32_le()?;
    let _fade_generation = stream.read_u16_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysGrowFadeModifier".to_string(),
    })
}

/// NiPSysRotationModifier: base + initial_speed(f32) + speed_variation(f32) +
/// initial_angle(f32) + angle_variation(f32) + random_axis(bool) + axis(vec3)
pub fn parse_rotation_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    let _initial_speed = stream.read_f32_le()?;
    // speed_variation + initial_angle + angle_variation: since v20.0.0.2
    if stream.version() >= crate::version::NifVersion(0x14000002) {
        stream.skip(4 * 3)?; // 3 floats
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
    let num_meshes = stream.read_u32_le()? as usize;
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

/// Parse field modifier with vec3 trailing data (vortex, gravity, radial directions).
pub fn parse_field_modifier_vec3(
    stream: &mut NifStream,
    type_name: &str,
) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_field_modifier_base(stream)?;
    stream.skip(12)?; // direction vec3
    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

/// NiPSysDragFieldModifier: field_base + use_direction(bool) + direction(vec3)
pub fn parse_drag_field_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_field_modifier_base(stream)?;
    let _use_direction = stream.read_byte_bool()?;
    stream.skip(12)?; // direction vec3
    Ok(NiPSysBlock {
        original_type: "NiPSysDragFieldModifier".to_string(),
    })
}

/// NiPSysTurbulenceFieldModifier: field_base + frequency(f32)
pub fn parse_turbulence_field_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_field_modifier_base(stream)?;
    let _frequency = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysTurbulenceFieldModifier".to_string(),
    })
}

/// NiPSysAirFieldModifier: field_base + direction(vec3) + 2 floats + 3 bools + 1 float
pub fn parse_air_field_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_field_modifier_base(stream)?;
    stream.skip(12)?; // direction vec3
    stream.skip(4 * 2)?; // air friction + inherit velocity
    let _inherit_rotation = stream.read_byte_bool()?;
    let _component_only = stream.read_byte_bool()?;
    let _enable_spread = stream.read_byte_bool()?;
    let _spread = stream.read_f32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysAirFieldModifier".to_string(),
    })
}

/// NiPSysRadialFieldModifier: field_base + radial_type(u32)
pub fn parse_radial_field_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _base = NiPSysModifierBase::parse(stream)?;
    skip_field_modifier_base(stream)?;
    let _radial_type = stream.read_u32_le()?;
    Ok(NiPSysBlock {
        original_type: "NiPSysRadialFieldModifier".to_string(),
    })
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

/// NiParticles / NiParticleSystem / NiMeshParticleSystem:
/// These inherit NiGeometry (same as NiTriShape). Parse NiAVObject + data/skin refs.
/// NiParticleSystem adds: world_space(bool) + num_modifiers(u32) + modifier_refs[N].
pub fn parse_particle_system(stream: &mut NifStream, type_name: &str) -> io::Result<NiPSysBlock> {
    use super::base::NiAVObjectData;

    let _av = NiAVObjectData::parse(stream)?;
    let _data_ref = stream.read_block_ref()?;

    // NiGeometry: skin_ref (since v3.3.0.13)
    let _skin_ref = stream.read_block_ref()?;

    // Shader/alpha refs for Skyrim+ (BSVER > 34). Use raw bsver() rather than
    // a variant predicate so non-Bethesda Gamebryo content at BSVER > 34 stays
    // aligned (matches base.rs:73 and node.rs:107).
    if stream.bsver() > 34 {
        let _shader_ref = stream.read_block_ref()?;
        let _alpha_ref = stream.read_block_ref()?;
    }

    // NiParticleSystem-specific fields (NiParticles has none).
    if type_name != "NiParticles" {
        let _world_space = stream.read_byte_bool()?;
        let num_modifiers = stream.read_u32_le()? as usize;
        for _ in 0..num_modifiers {
            let _modifier_ref = stream.read_block_ref()?;
        }
    }

    Ok(NiPSysBlock {
        original_type: type_name.to_string(),
    })
}

// ── BSStripParticleSystem (FO3+): same as NiParticleSystem ──────────

/// BSStripParticleSystem: inherits NiParticleSystem, no own fields.
pub fn parse_strip_particle_system(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
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
    if stream.version() >= NifVersion(0x0A010000) {
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
        // Particle Info: NiParticleInfo × num_vertices. Non-Bethesda only.
        // Size per entry: Vector3 velocity + Vector3 rotation_axis + f32 age +
        // f32 life_span + f32 last_update + i16 generation + u16 code = 40 bytes.
        // (Derived from NiParticleInfo struct; skip if we ever hit one.)
        //
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
    let num_ptrs = stream.read_u32_le()? as usize;
    for _ in 0..num_ptrs {
        let _ptr = stream.read_block_ref()?;
    }
    Ok(NiPSysBlock {
        original_type: "BSMasterParticleSystem".to_string(),
    })
}
