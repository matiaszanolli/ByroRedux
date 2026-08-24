//! Morph-target vertex-delta extraction (#3231).
//!
//! Walks a shape's `NiObjectNET.controller_ref` chain looking for a
//! `NiGeomMorpherController`, resolves its `NiMorphData`, and converts
//! each target's per-vertex deltas to the renderer's Y-up convention.
//! `NiMorphData` is parsed (`crates/nif/src/blocks/controller/morph.rs`)
//! but was, pre-#3231, never read past `.morphs[i].name` (only used to
//! resolve a KF channel's target index) — `.vectors` reached this point
//! and was then discarded on every import.

use super::super::ImportedMorphTarget;
use crate::anim::walk_controller_chain;
use crate::blocks::controller::{NiGeomMorpherController, NiMorphData};
use crate::scene::NifScene;
use crate::types::BlockRef;

use super::super::coord::zup_point_to_yup;

/// Hard cap on morph targets forwarded per mesh. Vanilla FaceGen content
/// authors a few dozen sliders at most; this is generous headroom, not a
/// tight fit — same "generous bound + graceful truncation + one warn"
/// shape as `SKIN_MAX_SLOTS` uses for the unrelated bone-slot pool.
/// Exists so a pathological/hand-authored NIF with an implausible morph
/// count can't blow up the per-mesh GPU delta buffer the renderer
/// allocates from this list's length.
pub(crate) const MAX_MORPH_TARGETS_PER_MESH: usize = 64;

/// Find the first `NiGeomMorpherController` on `controller_ref`'s chain,
/// resolve its `NiMorphData`, and convert each usable target to
/// [`ImportedMorphTarget`] (Y-up deltas). Returns `None` when the shape
/// has no morph controller, its data block is unresolved, or every
/// target was dropped (see below) — callers treat `None` and `Some(vec)`
/// identically to "no morph data" only in the former case; an empty
/// `Some(vec)` cannot occur (empty results collapse to `None` below).
///
/// # Per-target vertex-count guard
///
/// `NiMorphData.num_vertices` is read straight off disk by the parser
/// with no cross-check against the owning shape's actual vertex count
/// (confirmed at #3231 investigation time — the two blocks are linked
/// only indirectly, via the controller chain, and nothing enforces count
/// parity at parse or link time). A mismatched target here would corrupt
/// the GPU delta buffer's implicit `[target][vertex]` indexing for every
/// OTHER target sharing that buffer, not just itself — so a mismatch
/// drops the single offending target (logged) rather than the whole
/// mesh's morph data, matching this codebase's general per-item
/// fail-soft convention (e.g. malformed texture slots, missing bones).
pub(crate) fn extract_morph_targets(
    scene: &NifScene,
    controller_ref: BlockRef,
    vertex_count: usize,
    mesh_name: Option<&str>,
) -> Option<Vec<ImportedMorphTarget>> {
    if controller_ref.is_null() || vertex_count == 0 {
        return None;
    }

    let mut data: Option<&NiMorphData> = None;
    walk_controller_chain(scene, controller_ref, |_idx, block, _base| {
        if data.is_some() {
            return;
        }
        let Some(ctrl) = block.as_any().downcast_ref::<NiGeomMorpherController>() else {
            return;
        };
        data = ctrl
            .data_ref
            .index()
            .and_then(|i| scene.get_as::<NiMorphData>(i));
    });
    let data = data?;

    let mut targets: Vec<ImportedMorphTarget> = Vec::with_capacity(data.morphs.len());
    for morph in &data.morphs {
        if morph.vectors.len() != vertex_count {
            log::warn!(
                "morph target {:?} on mesh {:?}: {} deltas vs {} mesh vertices — \
                 NiMorphData.num_vertices doesn't match the owning shape, dropping \
                 this target",
                morph.name.as_deref().unwrap_or("<unnamed>"),
                mesh_name.unwrap_or("<unnamed>"),
                morph.vectors.len(),
                vertex_count,
            );
            continue;
        }
        if targets.len() >= MAX_MORPH_TARGETS_PER_MESH {
            log::warn!(
                "mesh {:?} carries more than {MAX_MORPH_TARGETS_PER_MESH} morph targets \
                 (NiMorphData.morphs.len() = {}) — truncating; remaining targets are \
                 silently inert",
                mesh_name.unwrap_or("<unnamed>"),
                data.morphs.len(),
            );
            break;
        }
        targets.push(ImportedMorphTarget {
            name: morph.name.clone(),
            deltas: morph.vectors.iter().map(zup_point_to_yup).collect(),
        });
    }

    if targets.is_empty() {
        None
    } else {
        Some(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::controller::{MorphTarget, NiTimeControllerBase};
    use crate::types::NiPoint3;
    use std::sync::Arc;

    fn controller_base() -> NiTimeControllerBase {
        NiTimeControllerBase {
            next_controller_ref: BlockRef::NULL,
            flags: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target_ref: BlockRef::NULL,
        }
    }

    fn morpher(data_ref: BlockRef) -> NiGeomMorpherController {
        NiGeomMorpherController {
            base: controller_base(),
            morpher_flags: 0,
            data_ref,
            always_update: 0,
            interpolator_weights: vec![],
        }
    }

    fn point(x: f32, y: f32, z: f32) -> NiPoint3 {
        NiPoint3 { x, y, z }
    }

    /// Scene: [0] NiMorphData, [1] NiGeomMorpherController pointing at it.
    /// `controller_ref` (passed to `extract_morph_targets`) is `BlockRef(1)`.
    fn scene_with(morphs: Vec<MorphTarget>) -> NifScene {
        let data = NiMorphData {
            num_vertices: 0, // unused by extract_morph_targets — per-target .vectors.len() is what's checked
            relative_targets: 0,
            morphs,
        };
        NifScene {
            blocks: vec![Box::new(data), Box::new(morpher(BlockRef(0)))],
            ..NifScene::default()
        }
    }

    #[test]
    fn extracts_matching_target_with_yup_converted_deltas() {
        let scene = scene_with(vec![MorphTarget {
            name: Some(Arc::from("Blink")),
            vectors: vec![point(1.0, 2.0, 3.0), point(0.0, 0.0, 0.0)],
        }]);
        let targets = extract_morph_targets(&scene, BlockRef(1), 2, Some("head")).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_deref(), Some("Blink"));
        // zup_point_to_yup: (x, y, z) -> (x, z, -y)
        assert_eq!(targets[0].deltas, vec![[1.0, 3.0, -2.0], [0.0, 0.0, 0.0]]);
    }

    /// The vertex-count guard (#3231) must drop a mismatched target, not
    /// the whole mesh's morph data or panic on an out-of-range index
    /// downstream.
    #[test]
    fn drops_target_with_mismatched_vertex_count() {
        let scene = scene_with(vec![
            MorphTarget {
                name: Some(Arc::from("Good")),
                vectors: vec![point(1.0, 0.0, 0.0)],
            },
            MorphTarget {
                name: Some(Arc::from("Bad")),
                vectors: vec![point(1.0, 0.0, 0.0), point(2.0, 0.0, 0.0)], // 2 deltas, mesh has 1 vertex
            },
        ]);
        let targets = extract_morph_targets(&scene, BlockRef(1), 1, None).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_deref(), Some("Good"));
    }

    /// Every target mismatched -> the whole result collapses to `None`,
    /// matching "no usable morph data" rather than `Some(vec![])`.
    #[test]
    fn all_targets_mismatched_returns_none() {
        let scene = scene_with(vec![MorphTarget {
            name: Some(Arc::from("Bad")),
            vectors: vec![point(1.0, 0.0, 0.0), point(2.0, 0.0, 0.0)],
        }]);
        assert!(extract_morph_targets(&scene, BlockRef(1), 1, None).is_none());
    }

    #[test]
    fn null_controller_ref_returns_none() {
        let scene = scene_with(vec![MorphTarget {
            name: Some(Arc::from("Blink")),
            vectors: vec![point(1.0, 0.0, 0.0)],
        }]);
        assert!(extract_morph_targets(&scene, BlockRef::NULL, 1, None).is_none());
    }

    #[test]
    fn zero_vertex_count_returns_none() {
        let scene = scene_with(vec![MorphTarget {
            name: Some(Arc::from("Blink")),
            vectors: vec![],
        }]);
        assert!(extract_morph_targets(&scene, BlockRef(1), 0, None).is_none());
    }

    /// A morph controller present in the chain but whose `data_ref` is
    /// unresolved (null, or points at a missing/wrong-type block) must
    /// not fabricate morph data.
    #[test]
    fn morpher_with_unresolved_data_ref_returns_none() {
        let scene = NifScene {
            blocks: vec![Box::new(morpher(BlockRef::NULL))],
            ..NifScene::default()
        };
        assert!(extract_morph_targets(&scene, BlockRef(0), 3, None).is_none());
    }

    /// #3231 — more targets than `MAX_MORPH_TARGETS_PER_MESH` must
    /// truncate, not allocate an unbounded GPU delta buffer.
    #[test]
    fn truncates_at_the_per_mesh_cap() {
        let morphs = (0..MAX_MORPH_TARGETS_PER_MESH + 5)
            .map(|i| MorphTarget {
                name: Some(Arc::from(format!("target{i}"))),
                vectors: vec![point(0.0, 0.0, 0.0)],
            })
            .collect();
        let scene = scene_with(morphs);
        let targets = extract_morph_targets(&scene, BlockRef(1), 1, None).unwrap();
        assert_eq!(targets.len(), MAX_MORPH_TARGETS_PER_MESH);
    }
}
