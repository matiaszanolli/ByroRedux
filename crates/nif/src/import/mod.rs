//! NIF-to-ECS import — converts a parsed NifScene into meshes and nodes.
//!
//! Walks the NiNode scene graph tree, preserving hierarchy as `ImportedNode`
//! entries with parent indices. Produces `ImportedMesh` per geometry leaf and
//! `ImportedNode` per NiNode. Transforms are local (relative to parent).
//!
//! The output is GPU-agnostic: `ImportedMesh` contains plain `Vec<Vertex>`
//! and `Vec<u32>` data ready for upload via `MeshRegistry::upload()`.

pub mod collision;
mod coord;
mod material;
mod mesh;
mod transform;
mod walk;

// Re-export the public material capture types so `ImportedMesh`'s
// `effect_shader` field can name `BsEffectShaderData` without leaking
// the internal module path.
pub use material::{BsEffectShaderData, NoLightingFalloff, ShaderTypeFields};

use crate::scene::NifScene;
use crate::types::NiTransform;
use byroredux_core::string::StringPool;

// Cfg-test imports are reachable from the child `test_support` /
// `tests` modules through `use super::*;`.
#[cfg(test)]
use byroredux_core::string::FixedString;
#[cfg(test)]
use std::sync::Arc;

/// Callback for resolving Starfield external `.mesh` companion files (SF-D4-02).
///
/// Implementations look up the raw bytes of an external mesh file by its
/// archive-relative path (e.g. `"geometries/abc/abc.mesh"`). Returns `None`
/// when the file is not available (stripped archive, wrong BA2 chain, etc.).
///
/// The NIF importer calls this for each `BSGeometry` LOD slot whose
/// `mesh_name` reference is external. When `None` is returned the LOD slot
/// is silently skipped — existing callers that don't need Starfield mesh
/// support pass `None` for the resolver and take the same path.
pub trait MeshResolver {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>>;
}


mod types;

// Re-export every public type produced by the import pipeline so
// downstream callers keep importing them through `crate::import::*`.
pub use types::*;

/// Test-only helpers for the FixedString migration (#609 / D6-NEW-01).
/// Sibling test modules across the import tree resolve mesh paths via
/// these so per-test boilerplate stays minimal.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Resolve a `FixedString` path back to a `&str` using the supplied
    /// pool. Returns `None` when the handle is `None` or the lookup
    /// misses (which would indicate the wrong pool was used).
    pub(crate) fn resolve_path<'a>(
        pool: &'a StringPool,
        sym: Option<FixedString>,
    ) -> Option<&'a str> {
        sym.and_then(|s| pool.resolve(s))
    }
}

/// Walk a parsed NIF scene flat and return every renderable particle
/// emitter (`NiParticleSystem` and friends), with NIF-local positions
/// and the nearest named ancestor's name. See #401.
pub fn import_nif_particle_emitters(scene: &NifScene) -> Vec<ImportedParticleEmitterFlat> {
    let mut out = Vec::new();
    let Some(root_idx) = scene.root_index else {
        return out;
    };
    walk::walk_node_particle_emitters_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        None,
        &mut out,
    );
    out
}

/// Import all renderable meshes from a parsed NIF scene, preserving hierarchy.
///
/// Returns an `ImportedScene` with nodes (NiNode hierarchy) and meshes (geometry leaves).
/// Transforms are local-space (relative to parent). Use the parent indices
/// to rebuild the hierarchy in the ECS.
///
/// `pool` interns every texture-slot path through the engine-wide
/// [`StringPool`] so `MaterialInfo` / `ImportedMesh` can carry
/// [`FixedString`] handles instead of fresh `Option<String>` heap
/// allocations on every cell load. See #609 / D6-NEW-01.
///
/// For Starfield content that uses external `.mesh` companion files,
/// pass a [`MeshResolver`] implementation that can look up the raw bytes
/// by archive-relative path. Pass `None` to skip external meshes.
pub fn import_nif_scene_with_resolver(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> ImportedScene {
    import_nif_scene_impl(scene, pool, resolver)
}

/// Import all renderable meshes from a parsed NIF scene, preserving hierarchy.
///
/// Equivalent to [`import_nif_scene_with_resolver`] with no external mesh resolver.
pub fn import_nif_scene(scene: &NifScene, pool: &mut StringPool) -> ImportedScene {
    import_nif_scene_impl(scene, pool, None)
}

fn import_nif_scene_impl(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> ImportedScene {
    // Pre-size the collection Vecs from the scene block count to
    // avoid the 0→4→8→…→N realloc chain during the walk (#835). Every
    // ImportedNode / ImportedMesh comes from at most one block, so
    // `blocks.len()` is a safe upper bound for `nodes`. Shapes are
    // typically ~¼ of blocks; emitters are rare (most NIFs have 0).
    // The audit accepts a slight VM-commit over-allocation in exchange
    // for zero realloc churn at typical NIF sizes.
    let cap = scene.blocks.len();
    let mut imported = ImportedScene {
        nodes: Vec::with_capacity(cap),
        meshes: Vec::with_capacity(cap / 4),
        particle_emitters: Vec::new(),
        bsx_flags: None,
        bs_bound: None,
        attach_points: None,
        child_attach_connections: None,
        embedded_clip: crate::anim::import_embedded_animations(scene),
    };

    // A truncated scene means at least one block was lost to a mid-parse
    // abort. The root NiNode heuristic may pick a sibling subtree
    // instead of the real root, and block refs inside descendant nodes
    // may dangle. Surface this to any caller (cell_loader, etc.) so the
    // partial import isn't silently accepted as complete. See #393.
    if scene.truncated {
        log::warn!(
            "importing truncated NIF scene — {} blocks dropped; root/refs may be incomplete",
            scene.dropped_block_count,
        );
    }

    let Some(root_idx) = scene.root_index else {
        return imported;
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_hierarchical(
        scene,
        root_idx,
        None,
        &mut props_stack,
        &mut imported,
        pool,
        resolver,
    );

    // Resolve extra data from the root node (BSXFlags, BSBound,
    // BSConnectPoint::Parents / Children).
    if let Some(root_block) = scene.blocks.get(root_idx) {
        if let Some(node) = root_block
            .as_any()
            .downcast_ref::<crate::blocks::node::NiNode>()
        {
            for &ref_idx in &node.av.net.extra_data_refs {
                // BlockRef::NULL (`u32::MAX`) maps to `None`; non-null
                // refs to `Some(usize)`. Pre-cleanup the code did
                // `if idx < 0` on the raw `u32` (always false), tripping
                // an `unused_comparisons` warning.
                let Some(idx) = ref_idx.index() else { continue };
                if let Some(block) = scene.blocks.get(idx) {
                    if let Some(ed) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::NiExtraData>()
                    {
                        if ed.type_name == "BSXFlags" {
                            imported.bsx_flags = ed.integer_value;
                        }
                    }
                    if let Some(bb) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::BsBound>()
                    {
                        // #986 / NIF-D5-ORPHAN-B2 — convert from
                        // Gamebryo Z-up to renderer Y-up so downstream
                        // ECS consumers (frustum culling, spatial
                        // queries) see the same coordinate system as
                        // `Transform` / `GlobalTransform`. `center` is
                        // a point, `dimensions` are unsigned half-
                        // extents along axes — under the Z-up→Y-up
                        // rotation around X, the half-extent along the
                        // new Y is the old Z half-extent and the new Z
                        // half-extent is the absolute value of the old
                        // Y half-extent. Magnitudes don't change sign,
                        // so the dimensions swap is just `[x, z, y]`.
                        let center = [bb.center[0], bb.center[2], -bb.center[1]];
                        let half_extents =
                            [bb.dimensions[0], bb.dimensions[2], bb.dimensions[1]];
                        imported.bs_bound = Some((center, half_extents));
                    }
                    // #985 / NIF-D5-ORPHAN-A3 — FO4+ weapon-mod
                    // attachment graph. Parsers landed pre-#985 but the
                    // payload was dropped on the floor; the OMOD /
                    // material-swap subsystem (#973 downstream) can't
                    // function without these reaching the ECS.
                    if let Some(parents) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::BsConnectPointParents>()
                    {
                        let points = parents
                            .connect_points
                            .iter()
                            .map(|cp| ImportedAttachPoint {
                                parent: cp.parent.clone(),
                                name: cp.name.clone(),
                                rotation: cp.rotation,
                                translation: cp.translation,
                                scale: cp.scale,
                            })
                            .collect();
                        imported.attach_points = Some(points);
                    }
                    if let Some(children) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::BsConnectPointChildren>()
                    {
                        imported.child_attach_connections =
                            Some(ImportedChildAttachConnections {
                                point_names: children.point_names.clone(),
                                skinned: children.skinned,
                            });
                    }
                }
            }
        }
    }

    imported
}

/// Backward-compatible flat import (used by cell loader where hierarchy is unnecessary).
///
/// Returns one `ImportedMesh` per NiTriShape with world-space transforms
/// (parent chain composed). Meshes have `parent_node: None`.
///
/// For Starfield external mesh support pass [`import_nif_with_resolver`].
pub fn import_nif(scene: &NifScene, pool: &mut StringPool) -> Vec<ImportedMesh> {
    import_nif_impl(scene, pool, None)
}

/// Flat import with an optional external mesh resolver (SF-D4-02 Stage B).
pub fn import_nif_with_resolver(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Vec<ImportedMesh> {
    import_nif_impl(scene, pool, resolver)
}

fn import_nif_impl(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Vec<ImportedMesh> {
    // Pre-size from block count; shapes are typically ~¼ of blocks. See #835.
    let mut meshes = Vec::with_capacity(scene.blocks.len() / 4);

    let Some(root_idx) = scene.root_index else {
        return meshes;
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        &mut props_stack,
        &mut meshes,
        None,
        pool,
        resolver,
    );
    meshes
}

/// Walk a parsed NIF scene and extract every NiLight subclass as an
/// `ImportedLight`, positioned in world space (Y-up).
///
/// This is an independent pass from `import_nif` — callers that care
/// about lights (currently: the cell loader) run it alongside the
/// mesh import. See issue #156.
/// Extract BSXFlags from the root node's extra data. Returns 0 if absent.
/// Bit 5 (0x20) = editor marker — the NIF should not be rendered.
pub fn extract_bsx_flags(scene: &NifScene) -> u32 {
    let Some(root_idx) = scene.root_index else {
        return 0;
    };
    let Some(root_block) = scene.blocks.get(root_idx) else {
        return 0;
    };
    let Some(node) = root_block
        .as_any()
        .downcast_ref::<crate::blocks::node::NiNode>()
    else {
        return 0;
    };
    for &ref_idx in &node.av.net.extra_data_refs {
        // BlockRef::NULL (`u32::MAX`) → `None`; non-null → `Some(usize)`.
        let Some(idx) = ref_idx.index() else { continue };
        if let Some(block) = scene.blocks.get(idx) {
            if let Some(ed) = block
                .as_any()
                .downcast_ref::<crate::blocks::extra_data::NiExtraData>()
            {
                if ed.type_name == "BSXFlags" {
                    return ed.integer_value.unwrap_or(0);
                }
            }
        }
    }
    0
}

pub fn import_nif_lights(scene: &NifScene) -> Vec<ImportedLight> {
    let mut lights = Vec::new();
    let Some(root_idx) = scene.root_index else {
        return lights;
    };
    walk::walk_node_lights(scene, root_idx, &NiTransform::default(), &mut lights);
    lights
}

/// Walk a parsed scene graph and surface every `NiTextureEffect`
/// projection (env map / projected light / projected shadow / fog) as
/// an [`ImportedTextureEffect`] with world-space pose + resolved
/// affected-node names + interned texture path. Mirrors
/// [`import_nif_lights`] for the `NiDynamicEffect` sibling that wasn't
/// previously imported. See #891.
///
/// Phase 1 capture only — the renderer-side projector pass is deferred.
/// Vanilla Oblivion / FO3 / FNV ship a small handful of these per
/// cell (sun gobos, light cookies, magic-FX env maps); Skyrim+ /
/// FO4 land most of the same effect surface through dedicated
/// `BSEffectShaderProperty` / `BSLightingShaderProperty` shader
/// variants and rarely use `NiTextureEffect` directly.
pub fn import_nif_texture_effects(
    scene: &NifScene,
    pool: &mut StringPool,
) -> Vec<ImportedTextureEffect> {
    let mut effects = Vec::new();
    let Some(root_idx) = scene.root_index else {
        return effects;
    };
    walk::walk_node_texture_effects(scene, root_idx, &NiTransform::default(), pool, &mut effects);
    effects
}

/// Flat import with collision data.
///
/// Like `import_nif()`, returns world-space meshes (flat, no hierarchy).
/// Additionally extracts collision shapes from NiNodes, returning them
/// in world space alongside the geometry.
pub fn import_nif_with_collision(
    scene: &NifScene,
    pool: &mut StringPool,
) -> (Vec<ImportedMesh>, Vec<ImportedCollision>) {
    import_nif_with_collision_impl(scene, pool, None)
}

/// Flat import with collision data and an optional external mesh resolver (SF-D4-02).
pub fn import_nif_with_collision_and_resolver(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> (Vec<ImportedMesh>, Vec<ImportedCollision>) {
    import_nif_with_collision_impl(scene, pool, resolver)
}

fn import_nif_with_collision_impl(
    scene: &NifScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> (Vec<ImportedMesh>, Vec<ImportedCollision>) {
    // Pre-size from block count: shapes are ~¼ of blocks, collision
    // blocks are rare (most NIFs carry 0-3 bhk* shapes — small floor
    // is enough to avoid the first realloc). See #835.
    let cap = scene.blocks.len();
    let mut meshes = Vec::with_capacity(cap / 4);
    let mut collisions = Vec::with_capacity(cap / 16);

    let Some(root_idx) = scene.root_index else {
        return (meshes, collisions);
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        &mut props_stack,
        &mut meshes,
        Some(&mut collisions),
        pool,
        resolver,
    );
    (meshes, collisions)
}

#[cfg(test)]
mod tests;
