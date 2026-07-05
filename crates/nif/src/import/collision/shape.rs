//! Collision shape-tree resolution — the `resolve_shape` recursion that
//! walks a bhk shape block into an engine [`CollisionShape`], plus the
//! NiTriStrips / packed / compressed mesh decoders it dispatches to.
//! Split out of the original `import/collision.rs` (#1876).

use std::collections::HashSet;

use crate::blocks::collision::*;
use crate::blocks::tri_shape::NiTriStripsData;
use crate::scene::NifScene;
use crate::types::BlockRef;

use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::math::{Quat, Vec3};

use super::{decompose_havok_matrix, finite, finite_vec, havok_to_engine};

/// Recursively resolve a bhk shape block into a CollisionShape enum.
///
/// Maximum collision-shape resolution depth. Real Havok shape trees are
/// shallow (MoppBvTree → List → primitives is ~3 levels; the deepest
/// vanilla nesting observed is well under 10). 64 is a generous ceiling
/// that no legitimate content reaches, while bounding the recursion so a
/// corrupt or adversarial NIF declaring a long acyclic chain of
/// `BhkListShape`/transform shapes cannot overflow the stack
/// (MEM-06 / #1385). Cycle detection (#1269) already handles *repeated*
/// blocks; this additionally caps *distinct* deep chains.
const MAX_COLLISION_SHAPE_DEPTH: usize = 64;

/// `visited` records `BlockRef` indices currently on the resolution
/// stack so a `BhkListShape` whose `sub_shape_refs` cycle (directly or
/// transitively) returns `None` instead of overflowing the stack
/// (#1269 / SAFE-DIM3-NEW-01). Entries are removed on return so a
/// legitimate DAG (the same shape referenced from two sibling subtrees)
/// still resolves on both arms. Because each on-stack block is held in
/// `visited` for exactly the span of its recursive call, `visited.len()`
/// is the current resolution depth — capped at
/// [`MAX_COLLISION_SHAPE_DEPTH`] so a deep *acyclic* chain (which cycle
/// detection alone does not catch) cannot crash the parser (MEM-06 /
/// #1385). Vanilla content has no such cycles or deep chains, but a
/// corrupt or adversarial NIF could otherwise crash the parser.
pub(super) fn resolve_shape(
    scene: &NifScene,
    shape_ref: BlockRef,
    visited: &mut HashSet<usize>,
) -> Option<CollisionShape> {
    let idx = shape_ref.index()?;
    if !visited.insert(idx) {
        log::warn!(
            "resolve_shape: cycle detected at block {} — breaking recursion (#1269)",
            idx,
        );
        return None;
    }
    // `visited` now holds the full chain from the root to `idx`, so its
    // length is this node's depth. Bail on pathologically deep acyclic
    // chains before they overflow the native stack. Remove the entry we
    // just inserted so the ancestor's bookkeeping stays balanced.
    if visited.len() > MAX_COLLISION_SHAPE_DEPTH {
        log::warn!(
            "resolve_shape: depth {} exceeds limit {} at block {} — \
             breaking recursion (MEM-06 / #1385)",
            visited.len(),
            MAX_COLLISION_SHAPE_DEPTH,
            idx,
        );
        visited.remove(&idx);
        return None;
    }
    let result = resolve_shape_inner(scene, idx, visited);
    visited.remove(&idx);
    result
}

/// Inner body of `resolve_shape`. Extracted so the outer function can
/// own the `visited` insert/remove bookkeeping at a single entry/exit
/// point regardless of which match arm returns.
fn resolve_shape_inner(
    scene: &NifScene,
    idx: usize,
    visited: &mut HashSet<usize>,
) -> Option<CollisionShape> {
    let scale = scene.havok_scale;
    let block = scene.get(idx)?;

    // Sphere
    if let Some(s) = block.as_any().downcast_ref::<BhkSphereShape>() {
        return Some(CollisionShape::Ball {
            radius: finite(s.radius * scale)?,
        });
    }

    // bhkPlaneShape — an AABB-bounded infinite plane (#1334). Parsed (no
    // NiUnknown) but deliberately not mapped to a Rapier collider: there is
    // no half-space / `Plane` `CollisionShape` variant, and approximating
    // the bounded plane as a solid `Cuboid` from its AABB would fill the
    // volume instead of presenting a surface. Returning `None` drops the one
    // vanilla SSE instance (the slaughterfish egg-cluster ground plane) to
    // the synthesized-trimesh fallback (`spawn.rs`) — its render-mesh
    // surface, which is the correct ground anyway. A true half-space mapping
    // is a follow-up.
    if block.as_any().downcast_ref::<BhkPlaneShape>().is_some() {
        return None;
    }

    // Multi-sphere — up to 8 offset spheres approximating a volume.
    // Each becomes a `Ball` child of a `Compound`, positioned at its
    // (havok→engine, scaled) center. Pre-fix this fell through to the
    // "unsupported" log and the authored collision was dropped entirely.
    if let Some(s) = block.as_any().downcast_ref::<BhkMultiSphereShape>() {
        let mut children = Vec::with_capacity(s.spheres.len());
        for sph in &s.spheres {
            let center = havok_to_engine(sph[0], sph[1], sph[2]) * scale;
            let radius = sph[3] * scale;
            // Drop only the corrupt sphere (NIFAL-S4 / #1409) — the rest
            // of the multi-sphere stays a valid approximation; an empty
            // residue falls through to `None` below → trimesh fallback.
            if !center.is_finite() || !radius.is_finite() {
                continue;
            }
            children.push((
                center,
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius }),
            ));
        }
        return match children.len() {
            0 => None,
            // A single centred sphere is just a Ball — no Compound needed.
            1 if children[0].0 == Vec3::ZERO => {
                let (_, _, shape) = children.into_iter().next().unwrap();
                Some(*shape)
            }
            _ => Some(CollisionShape::Compound { children }),
        };
    }

    // Box
    if let Some(s) = block.as_any().downcast_ref::<BhkBoxShape>() {
        let [hx, hy, hz] = s.dimensions;
        return Some(CollisionShape::Cuboid {
            half_extents: finite_vec(havok_to_engine(hx, hy, hz) * scale)?,
        });
    }

    // Capsule
    if let Some(s) = block.as_any().downcast_ref::<BhkCapsuleShape>() {
        let p1 = finite_vec(havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * scale)?;
        let p2 = finite_vec(havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * scale)?;
        // p1/p2 are finite, so the derived half_height is finite too.
        let half_height = (p2 - p1).length() * 0.5;
        let radius = finite(s.radius1.max(s.radius2) * scale)?;
        return Some(CollisionShape::Capsule {
            half_height,
            radius,
        });
    }

    // Cylinder
    if let Some(s) = block.as_any().downcast_ref::<BhkCylinderShape>() {
        let p1 = finite_vec(havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * scale)?;
        let p2 = finite_vec(havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * scale)?;
        let half_height = (p2 - p1).length() * 0.5;
        let radius = finite(s.cylinder_radius * scale)?;
        return Some(CollisionShape::Cylinder {
            half_height,
            radius,
        });
    }

    // Convex hull
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexVerticesShape>() {
        let verts: Vec<Vec3> = s
            .vertices
            .iter()
            .map(|v| havok_to_engine(v[0], v[1], v[2]) * scale)
            .collect();
        // A single non-finite vertex makes the whole hull unbuildable in
        // parry3d — drop to trimesh fallback rather than feed it a NaN
        // point (NIFAL-S4 / #1409).
        if verts.iter().any(|v| !v.is_finite()) {
            return None;
        }
        return Some(CollisionShape::ConvexHull { vertices: verts });
    }

    // MOPP BV tree — skip the MOPP data, recurse into the wrapped shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkMoppBvTreeShape>() {
        return resolve_shape(scene, s.shape_ref, visited);
    }

    // Convex sweep — wraps a child shape swept along a direction. For static
    // rest-pose collision the sweep direction is a runtime motion hint, not
    // part of the collider, so we recurse into the wrapped shape (same as the
    // MOPP wrapper above). Dispatched at blocks/mod.rs but had no resolve arm
    // pre-#1360 → the authored collision silently dropped.
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexSweepShape>() {
        return resolve_shape(scene, s.shape_ref, visited);
    }

    // List shape — compound of sub-shapes.
    if let Some(s) = block.as_any().downcast_ref::<BhkListShape>() {
        let mut children = Vec::with_capacity(s.sub_shape_refs.len());
        for sub_ref in &s.sub_shape_refs {
            if let Some(child) = resolve_shape(scene, *sub_ref, visited) {
                children.push((Vec3::ZERO, Quat::IDENTITY, Box::new(child)));
            }
        }
        return if children.is_empty() {
            // All sub-shapes failed to resolve (cycle elimination or
            // unsupported types). Surface as None rather than an empty
            // Compound (#1269 — pre-fix a cycled BhkListShape would
            // return Compound { children: [] } after cycle detection
            // dropped the only sub-shape).
            None
        } else if children.len() == 1 {
            // Unwrap single-child compound.
            let (_, _, shape) = children.into_iter().next().unwrap();
            Some(*shape)
        } else {
            Some(CollisionShape::Compound { children })
        };
    }

    // Convex list — like BhkListShape, a compound of convex sub-shapes
    // (FO3/FNV/Skyrim destructibles, debris). Pre-fix this fell through
    // to the "unsupported" log and the authored collision was dropped.
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexListShape>() {
        let mut children = Vec::with_capacity(s.sub_shapes.len());
        for sub_ref in &s.sub_shapes {
            if let Some(child) = resolve_shape(scene, *sub_ref, visited) {
                children.push((Vec3::ZERO, Quat::IDENTITY, Box::new(child)));
            }
        }
        return if children.is_empty() {
            None
        } else if children.len() == 1 {
            let (_, _, shape) = children.into_iter().next().unwrap();
            Some(*shape)
        } else {
            Some(CollisionShape::Compound { children })
        };
    }

    // Transform shape — apply 4x4 transform to child shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkTransformShape>() {
        let child = resolve_shape(scene, s.shape_ref, visited)?;
        let (translation, rotation) = decompose_havok_matrix(&s.transform, scale);
        return Some(CollisionShape::Compound {
            children: vec![(translation, rotation, Box::new(child))],
        });
    }

    // NiTriStrips mesh collision — resolve referenced NiTriStripsData.
    if let Some(s) = block.as_any().downcast_ref::<BhkNiTriStripsShape>() {
        return resolve_tri_strips_collision(scene, s);
    }

    // Mesh shape (Oblivion 10.0.1.0 only) — references NiTriStripsData just
    // like BhkNiTriStripsShape, plus a per-axis Scale vector. Dispatched at
    // blocks/mod.rs but had no resolve arm pre-#1361 → the authored collision
    // silently dropped. The Scale folds in (NiTriStripsData geometry is not
    // havok-scaled, #1744); a degenerate/unset scale vector falls back to
    // identity rather than collapsing the mesh to a point (which would render
    // as empty → None).
    if let Some(s) = block.as_any().downcast_ref::<BhkMeshShape>() {
        return resolve_tri_strips_data_refs(scene, &s.data_refs, per_axis_scale(&s.scale));
    }

    // Packed tri strips mesh collision. The packed data is authored in Havok
    // units (so `havok_scale` applies), and the shape carries its own per-axis
    // Scale on top — folded here (#1777); pre-fix only `havok_scale` was passed
    // and the authored per-axis scale was silently dropped.
    if let Some(s) = block.as_any().downcast_ref::<BhkPackedNiTriStripsShape>() {
        let data_idx = s.data_ref.index()?;
        let data = scene.get_as::<HkPackedNiTriStripsData>(data_idx)?;
        return resolve_packed_mesh(data, scale, per_axis_scale(&s.scale));
    }

    // Compressed mesh (Skyrim+) — resolve via data ref.
    if let Some(s) = block.as_any().downcast_ref::<BhkCompressedMeshShape>() {
        let data_idx = s.data_ref.index()?;
        let data = scene.get_as::<BhkCompressedMeshShapeData>(data_idx)?;
        return resolve_compressed_mesh(data, scale);
    }

    // Phantom shapes (bhkSimpleShapePhantom / bhkAabbPhantom) are trigger
    // volumes, not solid colliders. Drop BOTH to None — consistent with the
    // collision-object path (`extract_from_phantom`), which returns None so a
    // trigger isn't mis-promoted into a solid collider that would block the
    // player from walking through a quest-trigger region. Pre-#1363 only the
    // simple-shape variant had an arm, and it *resolved* its inner shape
    // (promoting the trigger) while the AABB variant silently dropped — an
    // inconsistency between the two phantom paths. When a `TriggerVolume` ECS
    // path lands, both should translate to that, not to a CollisionShape.
    if block.as_any().is::<BhkSimpleShapePhantom>() || block.as_any().is::<BhkAabbPhantom>() {
        log::debug!(
            "resolve_shape: phantom shape '{}' at block {} is a trigger volume, \
             not a solid collider — dropping (no TriggerVolume ECS path yet, #1363)",
            block.block_type_name(),
            idx,
        );
        return None;
    }

    log::debug!(
        "Unsupported collision shape type at block {}: {}",
        idx,
        block.block_type_name()
    );
    None
}

/// Per-axis multiplier from a shape's authored `Scale` Vector4, falling back to
/// identity when any of the first three components is non-finite or zero — a
/// degenerate/unset scale would otherwise collapse the mesh to a plane or point
/// and render as empty geometry. Shared by every NiTriStrips-family shape that
/// folds an authored scale (`bhkMeshShape`, `bhkNiTriStripsShape`,
/// `bhkPackedNiTriStripsShape`); the guard originated with `bhkMeshShape` (#1361)
/// and was generalised when the sibling drops were folded (#1777).
fn per_axis_scale(scale: &[f32; 4]) -> [f32; 3] {
    if scale[..3].iter().all(|c| c.is_finite() && *c != 0.0) {
        [scale[0], scale[1], scale[2]]
    } else {
        [1.0, 1.0, 1.0]
    }
}

/// Convert bhkNiTriStripsShape into a TriMesh by merging all referenced NiTriStripsData.
fn resolve_tri_strips_collision(
    scene: &NifScene,
    shape: &BhkNiTriStripsShape,
) -> Option<CollisionShape> {
    // #1777 — fold the shape's authored per-axis Scale (was dropped pre-fix),
    // exactly like the sibling `bhkMeshShape`. Identity in vanilla content, so
    // no behaviour change there; correct for non-identity authored scales.
    resolve_tri_strips_data_refs(scene, &shape.data_refs, per_axis_scale(&shape.scale))
}

/// Merge the `NiTriStripsData` referenced by `data_refs` into a single TriMesh.
/// `extra_scale` is the shape's authored per-axis Scale (identity when unset);
/// `bhkNiTriStripsShape` and `bhkMeshShape` both pass it via `per_axis_scale`.
///
/// #1744 — these vertices are NOT scaled by `havok_scale`. `bhkNiTriStripsShape`
/// / `bhkMeshShape` "use NiTriStripsData for geometry storage" (nif.xml) — the
/// SAME visual-geometry block the render mesh uses, already in game units.
/// `havok_scale` (×7 for Oblivion) applies to bodies' translations and to
/// primitive/packed shapes authored in Havok units (`resolve_packed_mesh`,
/// `BhkConvexVerticesShape`, …), NOT to the visual mesh shared with rendering.
/// Pre-fix every Oblivion/FO3/FNV `bhkNiTriStripsShape` collider (castle walls,
/// the Anvil cathedral, every large static) came out 7× oversized — a wall read
/// ~123 m tall — which threw exterior spawn ray-casts thousands of units into
/// the sky (a floating collider AABB y[-18185, 20277] in AnvilWorld).
fn resolve_tri_strips_data_refs(
    scene: &NifScene,
    data_refs: &[BlockRef],
    extra_scale: [f32; 3],
) -> Option<CollisionShape> {
    let mut all_verts = Vec::new();
    let mut all_indices = Vec::new();

    for data_ref in data_refs {
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) else {
            continue;
        };

        let base_idx = all_verts.len() as u32;
        for v in &data.vertices {
            all_verts.push(havok_to_engine(
                v.x * extra_scale[0],
                v.y * extra_scale[1],
                v.z * extra_scale[2],
            ));
        }
        // Convert triangle strips to triangles.
        for strip in &data.strips {
            for i in 2..strip.len() {
                let (a, b, c) = if i % 2 == 0 {
                    (strip[i - 2], strip[i - 1], strip[i])
                } else {
                    (strip[i - 1], strip[i - 2], strip[i])
                };
                // Skip degenerate triangles.
                if a != b && b != c && a != c {
                    all_indices.push([
                        a as u32 + base_idx,
                        b as u32 + base_idx,
                        c as u32 + base_idx,
                    ]);
                }
            }
        }
    }

    // #1779 — drop the shape if any vertex is non-finite (corrupt/truncated
    // NIF) so the synthesized-trimesh fallback fires instead of poisoning
    // parry's broadphase with NaN AABB bounds, mirroring the primitive
    // finite guards (#1409 / NIFAL-S4).
    if all_verts.is_empty() || all_verts.iter().any(|v| !v.is_finite()) {
        return None;
    }

    Some(CollisionShape::TriMesh {
        vertices: all_verts,
        indices: all_indices,
    })
}

/// Convert hkPackedNiTriStripsData into a TriMesh. `havok_scale` is the world
/// Havok→engine scale (the packed data is authored in Havok units); `extra_scale`
/// is the shape's own authored per-axis Scale (#1777), applied in the shape's
/// local Havok frame before the Z-up→Y-up swap (a uniform `havok_scale` commutes
/// with the swap, so it is applied after).
fn resolve_packed_mesh(
    data: &HkPackedNiTriStripsData,
    havok_scale: f32,
    extra_scale: [f32; 3],
) -> Option<CollisionShape> {
    if data.vertices.is_empty() {
        return None;
    }

    let vertices: Vec<Vec3> = data
        .vertices
        .iter()
        .map(|v| {
            havok_to_engine(
                v[0] * extra_scale[0],
                v[1] * extra_scale[1],
                v[2] * extra_scale[2],
            ) * havok_scale
        })
        .collect();

    let indices: Vec<[u32; 3]> = data
        .triangles
        .iter()
        .map(|t| [t.v0 as u32, t.v1 as u32, t.v2 as u32])
        .collect();

    // #1779 — non-finite vertices (NaN/±Inf from a corrupt decode) would
    // poison parry's broadphase; drop to the synth-trimesh fallback instead.
    if vertices.iter().any(|v| !v.is_finite()) {
        return None;
    }

    Some(CollisionShape::TriMesh { vertices, indices })
}

/// Convert bhkCompressedMeshShapeData into a TriMesh.
///
/// Merges big tris (full-precision) and chunk tris (quantized, strip-based)
/// into a single vertex/index buffer in engine space.
fn resolve_compressed_mesh(
    data: &BhkCompressedMeshShapeData,
    scale: f32,
) -> Option<CollisionShape> {
    let mut all_verts = Vec::new();
    let mut all_indices = Vec::new();

    // 1. Big tris — full-precision vertices.
    if !data.big_tris.is_empty() {
        let base = all_verts.len() as u32;
        for v in &data.big_verts {
            all_verts.push(havok_to_engine(v[0], v[1], v[2]) * scale);
        }
        for tri in &data.big_tris {
            all_indices.push([
                tri.v1 as u32 + base,
                tri.v2 as u32 + base,
                tri.v3 as u32 + base,
            ]);
        }
    }

    // 2. Chunks — quantized vertices + triangle strips.
    // Dequantization: world_pos = chunk.offset + (u16_vertex * error)
    // Confirmed via Havok source: Chunk::decompressVertex takes m_error parameter.
    // error is typically 0.001 but can vary per mesh.
    let error = data.error;
    for chunk in &data.chunks {
        let base = all_verts.len() as u32;
        let tx = chunk.translation[0];
        let ty = chunk.translation[1];
        let tz = chunk.translation[2];

        for qv in &chunk.vertices {
            let x = tx + qv[0] as f32 * error;
            let y = ty + qv[1] as f32 * error;
            let z = tz + qv[2] as f32 * error;
            all_verts.push(havok_to_engine(x, y, z) * scale);
        }

        // Havok chunk indices reference into the flat u16 vertex component array
        // (pre-multiplied by 3). Since we store vertices as [u16; 3] triples,
        // divide each index by 3 to get the vertex triple index.
        if chunk.strips.is_empty() {
            // Plain triangle list: every 3 indices = 1 triangle.
            let mut i = 0;
            while i + 2 < chunk.indices.len() {
                let a = chunk.indices[i] as u32 / 3 + base;
                let b = chunk.indices[i + 1] as u32 / 3 + base;
                let c = chunk.indices[i + 2] as u32 / 3 + base;
                if a != b && b != c && a != c {
                    all_indices.push([a, b, c]);
                }
                i += 3;
            }
        } else {
            // Triangle strips: convert each strip to triangles.
            let mut idx_offset = 0usize;
            for &strip_len in &chunk.strips {
                let end = idx_offset + strip_len as usize;
                let strip = &chunk.indices[idx_offset..end.min(chunk.indices.len())];
                for j in 2..strip.len() {
                    let (a, b, c) = if j % 2 == 0 {
                        (strip[j - 2], strip[j - 1], strip[j])
                    } else {
                        (strip[j - 1], strip[j - 2], strip[j])
                    };
                    if a != b && b != c && a != c {
                        all_indices.push([
                            a as u32 / 3 + base,
                            b as u32 / 3 + base,
                            c as u32 / 3 + base,
                        ]);
                    }
                }
                idx_offset = end;
            }
        }
    }

    // #1779 — same non-finite guard as the other TriMesh resolvers; a
    // bad dequantized chunk vertex must not reach parry's broadphase.
    if all_verts.is_empty() || all_verts.iter().any(|v| !v.is_finite()) {
        return None;
    }

    Some(CollisionShape::TriMesh {
        vertices: all_verts,
        indices: all_indices,
    })
}

#[cfg(test)]
mod cycle_tests {
    //! Regression for #1269 / SAFE-DIM3-NEW-01: `resolve_shape` must
    //! detect a `BhkListShape` whose `sub_shape_refs` cycle and return
    //! `None` rather than overflow the stack. Visited bookkeeping uses
    //! insert-on-entry / remove-on-exit so legitimate DAG sharing (the
    //! same leaf shape referenced from two sibling subtrees) still
    //! resolves on both arms.
    use super::*;
    use crate::blocks::collision::{
        BhkAabbPhantom, BhkConvexListShape, BhkConvexSweepShape, BhkListShape, BhkMeshShape,
        BhkMultiSphereShape, BhkNiTriStripsShape, BhkPackedNiTriStripsShape, BhkSimpleShapePhantom,
        BhkSphereShape, HkPackedNiTriStripsData, PackedTriangle,
    };
    use crate::blocks::tri_shape::NiTriStripsData;
    use crate::blocks::NiObject;
    use crate::types::{BlockRef, NiPoint3};

    fn list_shape(refs: Vec<BlockRef>) -> Box<dyn NiObject> {
        Box::new(BhkListShape {
            sub_shape_refs: refs,
            material: 0,
            filters: Vec::new(),
        })
    }

    fn sphere_shape(radius: f32) -> Box<dyn NiObject> {
        Box::new(BhkSphereShape {
            material: 0,
            radius,
        })
    }

    #[test]
    fn list_shape_self_cycle_returns_none() {
        // Scene:
        //   [0] BhkListShape { sub_shape_refs = [0] }   // self-reference
        // Pre-#1269 this would unbounded-recurse and stack-overflow.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(list_shape(vec![BlockRef(0u32)]));
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        assert!(
            result.is_none(),
            "self-cycle must produce None, not a populated shape"
        );
    }

    #[test]
    fn list_shape_mutual_cycle_does_not_overflow() {
        // Scene:
        //   [0] BhkListShape { sub_shape_refs = [1] }
        //   [1] BhkListShape { sub_shape_refs = [0] }
        // Mutual cycle through two BhkListShapes. The cycle blocks the
        // inner recursion; the outer list ends up with no resolvable
        // children. Success here is "returned without overflowing the
        // stack" — the shape returned may be None or an empty
        // Compound, both are acceptable cycle-broken outcomes.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(list_shape(vec![BlockRef(1u32)]));
        scene.blocks.push(list_shape(vec![BlockRef(0u32)]));
        let mut visited = HashSet::new();
        let _ = resolve_shape(&scene, BlockRef(0u32), &mut visited);
    }

    #[test]
    fn deep_acyclic_list_chain_bails_without_overflow() {
        // MEM-06 / #1385: a long *acyclic* chain of single-child
        // BhkListShapes is not caught by cycle detection (every block is
        // distinct), but recursing it would overflow the native stack.
        // Scene: [0]→[1]→…→[N-1]→[sphere]. With N well past
        // MAX_COLLISION_SHAPE_DEPTH, resolution must bail (None) and,
        // critically, return without overflowing — reaching this assert
        // at all proves no overflow.
        let n = MAX_COLLISION_SHAPE_DEPTH + 200;
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        for i in 0..n {
            scene
                .blocks
                .push(list_shape(vec![BlockRef((i + 1) as u32)]));
        }
        scene.blocks.push(sphere_shape(0.5)); // terminal, never reached
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        assert!(
            result.is_none(),
            "over-deep chain must bail to None, not a populated shape"
        );
        // Bookkeeping must be balanced after a depth bail (no leaked
        // on-stack entries), or a sibling subtree would falsely see a
        // cycle.
        assert!(
            visited.is_empty(),
            "visited must be empty after resolution returns"
        );
    }

    #[test]
    fn list_chain_within_depth_limit_resolves() {
        // Guard the other side: a chain shallower than the cap must STILL
        // resolve, so the depth bound doesn't regress legitimate nesting.
        // 10 single-child lists → terminal sphere; single-child lists
        // unwrap, so the whole chain collapses to the Ball.
        let depth = 10usize;
        assert!(depth < MAX_COLLISION_SHAPE_DEPTH);
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        for i in 0..depth {
            scene
                .blocks
                .push(list_shape(vec![BlockRef((i + 1) as u32)]));
        }
        scene.blocks.push(sphere_shape(0.5));
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        assert!(
            matches!(result, Some(CollisionShape::Ball { .. })),
            "a within-limit chain must resolve to the terminal sphere, got {result:?}"
        );
    }

    #[test]
    fn non_finite_sphere_radius_drops_to_none() {
        // NIFAL-S4 / #1409: a NaN / ±Inf radius from a corrupt NIF must
        // not reach the parry3d collider builder — resolve_shape returns
        // None so the synthesized-trimesh fallback fires instead.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut scene = NifScene::default();
            scene.havok_scale = 1.0;
            scene.blocks.push(sphere_shape(bad));
            let mut visited = HashSet::new();
            assert!(
                resolve_shape(&scene, BlockRef(0u32), &mut visited).is_none(),
                "radius {bad} must produce None"
            );
        }
    }

    #[test]
    fn finite_sphere_radius_resolves_to_scaled_ball() {
        // Control: the guard must not reject legitimate finite radii.
        let mut scene = NifScene::default();
        scene.havok_scale = 2.0;
        scene.blocks.push(sphere_shape(1.5));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::Ball { radius }) => assert!((radius - 3.0).abs() < 1e-6),
            other => panic!("expected scaled Ball, got {other:?}"),
        }
    }

    #[test]
    fn non_finite_box_dimension_drops_to_none() {
        // SIBLING of the sphere guard: a single non-finite half-extent
        // poisons the whole cuboid → None → trimesh fallback (#1409).
        use crate::blocks::collision::BhkBoxShape;
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkBoxShape {
            material: 0,
            radius: 0.0,
            dimensions: [1.0, f32::INFINITY, 2.0],
        }));
        let mut visited = HashSet::new();
        assert!(resolve_shape(&scene, BlockRef(0u32), &mut visited).is_none());
    }

    #[test]
    fn multi_sphere_shape_resolves_to_compound_of_balls() {
        // Two offset spheres → Compound with two Ball children at their
        // (havok→engine, scaled) centers. Pre-fix this dropped entirely.
        let mut scene = NifScene::default();
        scene.havok_scale = 2.0;
        scene.blocks.push(Box::new(BhkMultiSphereShape {
            material: 0,
            shape_property: [0; 3],
            // havok (x,y,z,r); havok_to_engine maps to engine axes.
            spheres: vec![[1.0, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.25]],
        }));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::Compound { children }) => {
                assert_eq!(children.len(), 2);
                // radii scaled by havok_scale.
                for (_, _, shape) in &children {
                    match **shape {
                        CollisionShape::Ball { radius } => {
                            assert!(radius == 1.0 || radius == 0.5, "got {radius}");
                        }
                        ref other => panic!("expected Ball child, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Compound of Balls, got {other:?}"),
        }
    }

    #[test]
    fn single_centred_multi_sphere_unwraps_to_ball() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMultiSphereShape {
            material: 0,
            shape_property: [0; 3],
            spheres: vec![[0.0, 0.0, 0.0, 3.0]], // centred → plain Ball
        }));
        let mut visited = HashSet::new();
        assert!(matches!(
            resolve_shape(&scene, BlockRef(0u32), &mut visited),
            Some(CollisionShape::Ball { radius }) if radius == 3.0
        ));
    }

    #[test]
    fn convex_list_shape_resolves_to_compound() {
        // ConvexList of two spheres → Compound, like BhkListShape.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkConvexListShape {
            sub_shapes: vec![BlockRef(1u32), BlockRef(2u32)],
            material: 0,
            radius: 0.0,
            use_cached_aabb: false,
            closest_point_min_distance: 0.0,
        }));
        scene.blocks.push(sphere_shape(1.0));
        scene.blocks.push(sphere_shape(2.0));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::Compound { children }) => assert_eq!(children.len(), 2),
            other => panic!("expected Compound, got {other:?}"),
        }
    }

    #[test]
    fn visited_resets_between_sibling_subtrees() {
        // Scene (DAG, not a cycle):
        //   [0] BhkListShape { sub_shape_refs = [1, 1] }   // shared leaf
        //   [1] BhkSphereShape { radius = 2.0 }
        // The same sphere is referenced twice as a child of the outer
        // list. Visited must remove on exit, so the second occurrence
        // still resolves rather than being mis-flagged as a cycle.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene
            .blocks
            .push(list_shape(vec![BlockRef(1u32), BlockRef(1u32)]));
        scene.blocks.push(sphere_shape(2.0));
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        match result {
            Some(CollisionShape::Compound { children }) => {
                assert_eq!(
                    children.len(),
                    2,
                    "DAG sharing must produce two child entries, not one"
                );
            }
            other => panic!("expected Compound with two children, got {other:?}"),
        }
    }

    fn tri_strips_data(verts: Vec<NiPoint3>, strip: Vec<u16>) -> Box<dyn NiObject> {
        Box::new(NiTriStripsData {
            vertices: verts,
            normals: Vec::new(),
            center: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            radius: 0.0,
            vertex_colors: Vec::new(),
            uv_sets: Vec::new(),
            num_triangles: 0,
            strips: vec![strip],
        })
    }

    #[test]
    fn convex_sweep_shape_resolves_to_inner_shape() {
        // #1360: BhkConvexSweepShape was dispatched at parse but had no
        // resolve arm — its wrapped shape silently dropped. It must now
        // recurse into the wrapped shape (like the MOPP wrapper).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkConvexSweepShape {
            shape_ref: BlockRef(1u32),
            material: 0,
            radius: 0.0,
        }));
        scene.blocks.push(sphere_shape(4.0));
        let mut visited = HashSet::new();
        assert!(
            matches!(
                resolve_shape(&scene, BlockRef(0u32), &mut visited),
                Some(CollisionShape::Ball { radius }) if radius == 4.0
            ),
            "convex-sweep must resolve to its wrapped Ball, not drop"
        );
    }

    #[test]
    fn mesh_shape_resolves_to_trimesh() {
        // #1361: BhkMeshShape was dispatched at parse but had no resolve
        // arm — its referenced NiTriStripsData silently dropped. It must
        // now build a TriMesh, like BhkNiTriStripsShape.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMeshShape {
            radius: 0.0,
            scale: [1.0, 1.0, 1.0, 0.0],
            data_refs: vec![BlockRef(1u32)],
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, indices }) => {
                assert_eq!(vertices.len(), 3, "all three verts converted");
                assert_eq!(indices.len(), 1, "one non-degenerate triangle");
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn mesh_shape_folds_per_axis_scale() {
        // BhkMeshShape's authored per-axis Scale vector must fold in (#1744:
        // NiTriStripsData collision is NOT scaled by havok_scale — only the
        // per-axis Scale applies; here havok_scale is 1.0 regardless).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMeshShape {
            radius: 0.0,
            scale: [2.0, 3.0, 5.0, 0.0],
            data_refs: vec![BlockRef(1u32)],
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, .. }) => {
                // Vertex (1,1,1) in havok space → scaled to (2,3,5), then
                // havok_to_engine maps (x,y,z) → (x, z, -y) = (2, 5, -3).
                let v = vertices[1];
                assert!(
                    (v.x - 2.0).abs() < 1e-5
                        && (v.y - 5.0).abs() < 1e-5
                        && (v.z + 3.0).abs() < 1e-5,
                    "per-axis scale not folded; got {v:?}"
                );
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn ni_tri_strips_shape_ignores_havok_scale() {
        // #1744 — bhkNiTriStripsShape stores geometry in NiTriStripsData, the
        // SAME visual-mesh block the renderer uses (game units). havok_scale
        // (×7 for Oblivion) must NOT inflate it; pre-fix every large static
        // collider came out 7× oversized (castle walls reading ~123 m tall),
        // throwing exterior spawn ray-casts into the sky.
        let mut scene = NifScene::default();
        scene.havok_scale = 7.0;
        scene.blocks.push(Box::new(BhkNiTriStripsShape {
            material: 0,
            radius: 0.0,
            scale: [1.0, 1.0, 1.0, 1.0],
            data_refs: vec![BlockRef(1u32)],
            filters: Vec::new(),
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 10.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, .. }) => {
                // Vertex (10,0,0) → havok_to_engine (x,z,-y) = (10,0,0); the
                // magnitude is the game-unit 10, NOT 70 (10 × havok_scale 7).
                let v = vertices[1];
                assert!(
                    (v.x - 10.0).abs() < 1e-5,
                    "havok_scale must not inflate NiTriStripsData collision \
                     (expected x=10, got {})",
                    v.x
                );
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn ni_tri_strips_shape_with_nonfinite_vertex_drops_to_fallback() {
        // #1779 — a NaN/±Inf vertex from a corrupt or truncated NIF must NOT
        // build a TriMesh (it would poison parry's broadphase with NaN AABB
        // bounds); resolve_shape returns None so the synth-trimesh fallback
        // fires, matching the primitive finite guards (#1409).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkNiTriStripsShape {
            material: 0,
            radius: 0.0,
            scale: [1.0, 1.0, 1.0, 1.0],
            data_refs: vec![BlockRef(1u32)],
            filters: Vec::new(),
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: f32::NAN,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: f32::INFINITY,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        assert!(
            resolve_shape(&scene, BlockRef(0u32), &mut visited).is_none(),
            "non-finite TriMesh vertices must drop to the synth fallback, not build a TriMesh"
        );
    }

    #[test]
    fn ni_tri_strips_shape_folds_per_axis_scale() {
        // #1777 — bhkNiTriStripsShape's authored per-axis Scale must fold in,
        // exactly like the sibling bhkMeshShape, and was dropped pre-fix. Like
        // bhkMeshShape the NiTriStripsData geometry is NOT havok-scaled (#1744),
        // so havok_scale=7 here must leave the per-axis-scaled result untouched.
        let mut scene = NifScene::default();
        scene.havok_scale = 7.0;
        scene.blocks.push(Box::new(BhkNiTriStripsShape {
            material: 0,
            radius: 0.0,
            scale: [2.0, 3.0, 5.0, 0.0],
            data_refs: vec![BlockRef(1u32)],
            filters: Vec::new(),
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, .. }) => {
                // (1,1,1) × scale(2,3,5) = (2,3,5) in havok space; havok_to_engine
                // (x,z,-y) = (2,5,-3). havok_scale (7) must NOT inflate it.
                let v = vertices[1];
                assert!(
                    (v.x - 2.0).abs() < 1e-5
                        && (v.y - 5.0).abs() < 1e-5
                        && (v.z + 3.0).abs() < 1e-5,
                    "per-axis scale not folded (or havok_scale leaked); got {v:?}"
                );
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn packed_tristrips_shape_folds_per_axis_scale_and_havok_scale() {
        // #1777 — bhkPackedNiTriStripsShape's authored per-axis Scale was parsed
        // and stored but never read at resolve; only havok_scale was applied.
        // The packed data IS in Havok units, so BOTH the per-axis Scale and the
        // uniform havok_scale must apply (unlike the NiTriStripsData family).
        let mut scene = NifScene::default();
        scene.havok_scale = 10.0;
        scene.blocks.push(Box::new(BhkPackedNiTriStripsShape {
            sub_shapes: Vec::new(),
            data_ref: BlockRef(1u32),
            scale: [2.0, 3.0, 5.0, 0.0],
        }));
        scene.blocks.push(Box::new(HkPackedNiTriStripsData {
            triangles: vec![PackedTriangle {
                v0: 0,
                v1: 1,
                v2: 2,
                welding_info: 0,
                normal: None,
            }],
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 0.0]],
        }));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, indices }) => {
                assert_eq!(indices.len(), 1, "one triangle");
                // (1,1,1) × scale(2,3,5) = (2,3,5); havok_to_engine (x,z,-y) =
                // (2,5,-3); × havok_scale(10) = (20,50,-30).
                let v = vertices[1];
                assert!(
                    (v.x - 20.0).abs() < 1e-4
                        && (v.y - 50.0).abs() < 1e-4
                        && (v.z + 30.0).abs() < 1e-4,
                    "packed per-axis scale × havok_scale not applied; got {v:?}"
                );
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn packed_tristrips_degenerate_scale_falls_back_to_identity() {
        // A zero/non-finite authored Scale must NOT collapse the mesh — the
        // per_axis_scale guard falls back to identity (only havok_scale applies).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkPackedNiTriStripsShape {
            sub_shapes: Vec::new(),
            data_ref: BlockRef(1u32),
            scale: [0.0, 0.0, 0.0, 0.0], // degenerate → identity
        }));
        scene.blocks.push(Box::new(HkPackedNiTriStripsData {
            triangles: vec![PackedTriangle {
                v0: 0,
                v1: 1,
                v2: 2,
                welding_info: 0,
                normal: None,
            }],
            vertices: vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 0.0, 4.0]],
        }));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, .. }) => {
                // identity fallback: (4,0,0) → havok_to_engine (4,0,0), not (0,0,0).
                let v = vertices[1];
                assert!(
                    (v.x - 4.0).abs() < 1e-5,
                    "degenerate scale must fall back to identity, not collapse; got {v:?}"
                );
            }
            other => panic!("expected TriMesh (identity fallback), got {other:?}"),
        }
    }

    #[test]
    fn phantom_shapes_drop_to_none() {
        // #1363: both phantom subclasses are trigger volumes, not solid
        // colliders. Neither may resolve to a CollisionShape even with a
        // non-null inner shape_ref — consistent with extract_from_phantom on
        // the collision-object path. Pre-#1363 bhkSimpleShapePhantom resolved
        // its inner shape (promoting the trigger) while bhkAabbPhantom dropped.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkSimpleShapePhantom {
            shape_ref: BlockRef(2u32),
            havok_filter: 0,
            transform: [[0.0; 4]; 4],
        })); // [0]
        scene.blocks.push(Box::new(BhkAabbPhantom {
            shape_ref: BlockRef(2u32),
            havok_filter: 0,
            aabb_min: [0.0; 4],
            aabb_max: [0.0; 4],
        })); // [1]
        scene.blocks.push(sphere_shape(1.0)); // [2] inner shape — must NOT be promoted

        let mut visited = HashSet::new();
        assert!(
            resolve_shape(&scene, BlockRef(0u32), &mut visited).is_none(),
            "bhkSimpleShapePhantom must drop (trigger volume, not a solid collider)"
        );
        let mut visited = HashSet::new();
        assert!(
            resolve_shape(&scene, BlockRef(1u32), &mut visited).is_none(),
            "bhkAabbPhantom must drop (trigger volume, not a solid collider)"
        );
    }
}
