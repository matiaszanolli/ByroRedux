//! Spatial-unit translation at the NIF import boundary.
//!
//! Starfield authors NiAVObject, skeleton/bind transforms and bounds in
//! meters. The .mesh decoder retains nifly's 69.969 tooling scale, which is
//! NOT a world transform: applying it to vertices alone makes geometry huge
//! relative to bones, placements and lights. Public imports use the engine's
//! common Bethesda units (70/m); rotations, weights and authored scales stay
//! dimensionless. Havok shapes already have their own unit translation.

use super::*;
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

pub(crate) fn length_scale(scene: &NifScene) -> f32 {
    if scene.bsver >= crate::version::bsver::FO76_STARFIELD_BOUNDARY {
        BETHESDA_UNITS_PER_METER
    } else {
        1.0
    }
}

pub(super) fn scale_vec<const N: usize>(v: &mut [f32; N], scale: f32) {
    for x in v {
        *x *= scale;
    }
}

fn scale_matrix_translation(m: &mut [[f32; 4]; 4], scale: f32) {
    // S M S^-1: change the coordinate units, not the authored linear map.
    for x in &mut m[3][..3] {
        *x *= scale;
    }
}

pub(super) fn meshes(scene: &NifScene, meshes: &mut [ImportedMesh]) {
    let scale = length_scale(scene);
    if scale == 1.0 {
        return;
    }
    for mesh in meshes {
        let vertex_scale = if mesh.bs_geometry_lod_slot.is_some() {
            scale / crate::blocks::bs_geometry::BSGeometryMeshData::HAVOK_SCALE
        } else {
            scale
        };
        for p in &mut mesh.positions {
            scale_vec(p, vertex_scale);
        }
        scale_vec(&mut mesh.local_bound_center, vertex_scale);
        mesh.local_bound_radius *= vertex_scale;
        scale_vec(&mut mesh.translation, scale);
        if let Some(skin) = &mut mesh.skin {
            scale_matrix_translation(&mut skin.global_skin_transform, scale);
            for bone in &mut skin.bones {
                scale_matrix_translation(&mut bone.bind_inverse, scale);
                scale_vec(&mut bone.bounding_sphere, scale);
            }
        }
        if let Some(targets) = &mut mesh.morph_targets {
            for target in targets {
                for delta in &mut target.deltas {
                    scale_vec(delta, scale);
                }
            }
        }
    }
}

pub(super) fn hierarchy(scene: &NifScene, imported: &mut ImportedScene) {
    let scale = length_scale(scene);
    if scale == 1.0 {
        return;
    }
    meshes(scene, &mut imported.meshes);
    for node in &mut imported.nodes {
        scale_vec(&mut node.translation, scale);
        if let Some(bound) = &mut node.bs_ordered_node {
            scale_vec(&mut bound.alpha_sort_bound, scale);
        }
        if let Some(lod) = &mut node.lod_group {
            scale_vec(&mut lod.center, scale);
            for (near, far) in &mut lod.levels {
                *near *= scale;
                *far *= scale;
            }
        }
    }
    if let Some((center, extents)) = &mut imported.bs_bound {
        scale_vec(center, scale);
        scale_vec(extents, scale);
    }
    // phantom_bounds/ragdoll/collision are already in engine units via PHYSAL.
    // Attach points, furniture and animation use their standalone import
    // boundaries below, shared with the flat cell-loader path.
    for emitter in &mut imported.particle_emitters {
        scale_vec(&mut emitter.local_translation, scale);
        emitter_params(&mut emitter.emitter_params, scale);
        force_fields(&mut emitter.force_fields, scale);
    }
}

pub(super) fn emitter_params(params: &mut Option<ImportedEmitterParams>, scale: f32) {
    if let Some(p) = params {
        p.speed *= scale;
        p.speed_variation *= scale;
        p.initial_radius *= scale;
        p.radius_variation *= scale;
    }
}

pub(super) fn force_fields(fields: &mut [ImportedParticleForceField], scale: f32) {
    use ImportedParticleForceField::*;
    // Preserve the dimensions consumed by integrate_force_fields: accelerations
    // grow with distance units, attenuation coefficients shrink with them.
    for field in fields {
        match field {
            Gravity {
                strength, decay, ..
            } => {
                *strength *= scale;
                *decay /= scale;
            }
            Vortex { decay, .. } => {
                *decay /= scale;
            }
            Air {
                strength, falloff, ..
            }
            | Radial { strength, falloff } => {
                *strength *= scale;
                *falloff /= scale;
            }
            Turbulence {
                scale: amplitude, ..
            } => {
                *amplitude *= scale;
            }
            Drag { .. } => {}
        }
    }
}

pub(crate) fn animation(scene: &NifScene, clip: &mut crate::anim::AnimationClip) {
    let scale = length_scale(scene);
    if scale == 1.0 {
        return;
    }
    for channel in clip.channels.values_mut() {
        for key in &mut channel.translation_keys {
            scale_vec(&mut key.value, scale);
            scale_vec(&mut key.forward, scale);
            scale_vec(&mut key.backward, scale);
        }
    }
    for (_, channel) in &mut clip.float_channels {
        if channel.target == crate::anim::FloatTarget::LightRadius {
            for key in &mut channel.keys {
                key.value *= scale;
            }
        }
    }
}
