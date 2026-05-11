//! NifScene — resolved scene graph after linking.

use crate::blocks::{node::NiNode, NiObject};
use crate::types::BlockRef;
use std::collections::BTreeMap;

/// A fully parsed and linked NIF file.
#[derive(Debug)]
pub struct NifScene {
    /// All parsed blocks in file order.
    pub blocks: Vec<Box<dyn NiObject>>,
    /// Index of the root block (typically first NiNode).
    pub root_index: Option<usize>,
    /// `true` when the parse loop aborted early because a block
    /// parser returned `Err` on a NIF file without per-block sizes
    /// (Oblivion era) — any blocks after the failure point are
    /// missing from `blocks` and the scene graph may reference
    /// unreachable indices. The first NiNode heuristic for
    /// `root_index` may also pick a subtree rather than the real
    /// root when truncation hits before the scene root. Consumers
    /// that need complete scenes should treat this as a hard error;
    /// consumers that can tolerate partial geometry (e.g. cell
    /// loaders doing best-effort import) can ignore it. See #175.
    pub truncated: bool,
    /// Number of blocks that were dropped from the scene because
    /// the parse loop bailed out before reaching them. Non-zero
    /// implies `truncated == true`. Lets observability layers
    /// (telemetry, cell_loader diagnostics) quantify how much of
    /// the file was lost without re-reading the raw header. See #224.
    pub dropped_block_count: usize,
    /// Number of blocks that fell into the `NiUnknown` recovery path
    /// because their parser returned `Err` and the parse loop was
    /// able to seek past them (block-size header, runtime size cache,
    /// or `oblivion_skip_sizes` hint) OR because the block type wasn't
    /// in the dispatch table. Distinct from `dropped_block_count` —
    /// the scene still contains a placeholder block at the original
    /// index, block refs downstream still resolve, but the block's
    /// data is gone. See #568 (SK-D5-06).
    ///
    /// The parse-rate gate treats any NIF with `recovered_blocks > 0`
    /// as non-clean so under-consuming parser bugs (e.g. #546) turn
    /// the integration metric red rather than hiding behind a "100 %
    /// clean" rate. Pre-#568 the recovery path warned but didn't
    /// surface anywhere observable.
    pub recovered_blocks: usize,
    /// Number of dangling `BlockRef`s found by [`Self::validate_refs`]
    /// when [`crate::ParseOptions::validate_links`] was set on the
    /// parse call. Zero for parses that didn't request validation
    /// (default) or that produced a link-clean scene. A regression
    /// that produces technically-Ok scenes with semantically-broken
    /// links would surface here without any caller doing the walk
    /// itself; before this field landed only debug-build / nif_stats
    /// callers ran the walk explicitly. See #892.
    pub link_errors: usize,
    /// Per-block-type stream-drift histogram. `drift_histogram[type][n]`
    /// is the number of times a block parser of that type returned `Ok`
    /// but consumed a number of bytes that disagreed with the header's
    /// `block_size` field, where `n = declared - consumed`. Positive
    /// drift = the parser under-read (stream realigned forward),
    /// negative drift = the parser over-read (stream realigned backward).
    ///
    /// Only populated for NIFs that have a `block_sizes` table (Skyrim
    /// SE / FO4 / FO76 / Starfield era, v20.2.0.7+). Oblivion-era files
    /// with no per-block sizes have nothing to reconcile against and
    /// produce an empty histogram — the runtime-size-cache drift
    /// detector at `parse_nif` handles them separately (#395).
    ///
    /// Havok constraint stub block types (`bhkHingeConstraint` & al, see
    /// `is_havok_constraint_stub` in `lib.rs`) are intentionally
    /// excluded — they under-consume by design (#117) and would
    /// otherwise drown the histogram on every actor spawn.
    ///
    /// Drives the `nif_stats --drift-histogram` aggregation across full
    /// archive walks: a clean parse rate can paper over byte-level
    /// parser drift, so the histogram surfaces "this block type's
    /// parser is consistently 1 byte short" patterns that would
    /// otherwise only show up as a much-later downstream block reading
    /// garbage. See #939.
    pub drift_histogram: BTreeMap<String, BTreeMap<i64, u32>>,
}

impl Default for NifScene {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            root_index: None,
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        }
    }
}

impl NifScene {
    /// Get a block by index.
    pub fn get(&self, index: usize) -> Option<&dyn NiObject> {
        self.blocks.get(index).map(|b| b.as_ref())
    }

    /// Get a block by index, downcasted to a concrete type.
    pub fn get_as<T: 'static>(&self, index: usize) -> Option<&T> {
        self.blocks.get(index)?.as_any().downcast_ref::<T>()
    }

    /// Get the root block (typically NiNode).
    pub fn root(&self) -> Option<&dyn NiObject> {
        self.root_index.and_then(|i| self.get(i))
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Walk every block and check that each non-null `BlockRef` resolves
    /// to an in-range block index. Returns an empty `Vec` when the scene
    /// is link-clean; otherwise returns one [`RefError`] per dangling
    /// reference with enough context (block index, block type, ref kind,
    /// bad index) for diagnostics.
    ///
    /// This is an optional post-parse sanity pass — `parse_nif` does not
    /// run it, so consumers that care (debug builds, `nif_stats`, tests
    /// against real corpora) can opt in via:
    /// ```ignore
    /// let scene = byroredux_nif::parse_nif(&bytes)?;
    /// for err in scene.validate_refs() {
    ///     log::warn!("{err:?}");
    /// }
    /// ```
    ///
    /// Coverage is driven by the existing `HasObjectNET`/`HasAVObject`/
    /// `HasShaderRefs` upcast traits plus an explicit `NiNode` downcast
    /// for `children`/`effects`. The scene `root_index` is also
    /// range-checked. Per-field type checking is intentionally out of
    /// scope — this is a range-validity net, not a full schema
    /// validator. See #226.
    pub fn validate_refs(&self) -> Vec<RefError> {
        let mut errors = Vec::new();
        let len = self.blocks.len();

        let check = |errors: &mut Vec<RefError>,
                     block_index: usize,
                     block_type: &'static str,
                     kind: RefKind,
                     r: BlockRef| {
            if let Some(idx) = r.index() {
                if idx >= len {
                    errors.push(RefError {
                        block_index,
                        block_type,
                        kind,
                        bad_index: idx,
                        blocks_len: len,
                    });
                }
            }
        };

        for (i, block) in self.blocks.iter().enumerate() {
            let type_name = block.block_type_name();

            if let Some(net) = block.as_object_net() {
                check(
                    &mut errors,
                    i,
                    type_name,
                    RefKind::Controller,
                    net.controller_ref(),
                );
                for r in net.extra_data_refs() {
                    check(&mut errors, i, type_name, RefKind::ExtraData, *r);
                }
            }
            if let Some(av) = block.as_av_object() {
                check(
                    &mut errors,
                    i,
                    type_name,
                    RefKind::Collision,
                    av.collision_ref(),
                );
                for r in av.properties() {
                    check(&mut errors, i, type_name, RefKind::Property, *r);
                }
            }
            if let Some(sref) = block.as_shader_refs() {
                check(
                    &mut errors,
                    i,
                    type_name,
                    RefKind::ShaderProperty,
                    sref.shader_property_ref(),
                );
                check(
                    &mut errors,
                    i,
                    type_name,
                    RefKind::AlphaProperty,
                    sref.alpha_property_ref(),
                );
            }

            // NiNode children/effects are not exposed via a trait — they
            // carry the scene graph edges, so we downcast explicitly.
            if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
                for r in &node.children {
                    check(&mut errors, i, type_name, RefKind::Child, *r);
                }
                for r in &node.effects {
                    check(&mut errors, i, type_name, RefKind::Effect, *r);
                }
            }
        }

        if let Some(root) = self.root_index {
            if root >= len {
                errors.push(RefError {
                    block_index: usize::MAX,
                    block_type: "NifScene",
                    kind: RefKind::Root,
                    bad_index: root,
                    blocks_len: len,
                });
            }
        }

        errors
    }
}

/// One dangling-reference finding produced by [`NifScene::validate_refs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefError {
    /// Index of the block that owns the bad reference, or `usize::MAX`
    /// when the error is on the scene itself (root_index out of range).
    pub block_index: usize,
    /// Static type name of the owning block (`"NifScene"` for root errors).
    pub block_type: &'static str,
    /// Which field the reference came from.
    pub kind: RefKind,
    /// The out-of-range index that was read from the NIF.
    pub bad_index: usize,
    /// Number of blocks actually present in the scene (bound that was exceeded).
    pub blocks_len: usize,
}

/// Where a dangling `BlockRef` was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// `NiObjectNET.controller` — animation controller chain head.
    Controller,
    /// `NiObjectNET.extra_data` — per-block metadata attachments.
    ExtraData,
    /// `NiAVObject.collision_object` — bhk collision volume.
    Collision,
    /// `NiAVObject.properties` — legacy property list (Oblivion/FNV).
    Property,
    /// `NiTriShape.shader_property` / `BSTriShape.shader_property` (Skyrim+).
    ShaderProperty,
    /// `NiTriShape.alpha_property` / `BSTriShape.alpha_property` (Skyrim+).
    AlphaProperty,
    /// `NiNode.children` — scene graph descendants.
    Child,
    /// `NiNode.effects` — attached `NiDynamicEffect` (pre-FO4).
    Effect,
    /// `NifScene.root_index` — identified root block.
    Root,
}

#[cfg(test)]
mod validate_refs_tests {
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::types::NiTransform;

    fn empty_net() -> NiObjectNETData {
        NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        }
    }

    fn empty_av() -> NiAVObjectData {
        NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        }
    }

    fn node(av: NiAVObjectData, children: Vec<BlockRef>, effects: Vec<BlockRef>) -> Box<NiNode> {
        Box::new(NiNode {
            av,
            children,
            effects,
        })
    }

    #[test]
    fn clean_scene_reports_no_errors() {
        // Root with two in-range children.
        let root = node(empty_av(), vec![BlockRef(1), BlockRef(2)], Vec::new());
        let child0 = node(empty_av(), Vec::new(), Vec::new());
        let child1 = node(empty_av(), Vec::new(), Vec::new());
        let scene = NifScene {
            blocks: vec![root, child0, child1],
            root_index: Some(0),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        assert!(scene.validate_refs().is_empty());
    }

    #[test]
    fn null_refs_are_ignored() {
        let mut av = empty_av();
        av.net.controller_ref = BlockRef::NULL;
        av.collision_ref = BlockRef::NULL;
        av.net.extra_data_refs = vec![BlockRef::NULL];
        let root = node(av, vec![BlockRef::NULL], vec![BlockRef::NULL]);
        let scene = NifScene {
            blocks: vec![root],
            root_index: Some(0),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        assert!(scene.validate_refs().is_empty());
    }

    #[test]
    fn dangling_child_is_reported() {
        // Root points at block 5 — only 1 block in scene.
        let root = node(empty_av(), vec![BlockRef(5)], Vec::new());
        let scene = NifScene {
            blocks: vec![root],
            root_index: Some(0),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        let errs = scene.validate_refs();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].block_index, 0);
        assert_eq!(errs[0].block_type, "NiNode");
        assert_eq!(errs[0].kind, RefKind::Child);
        assert_eq!(errs[0].bad_index, 5);
        assert_eq!(errs[0].blocks_len, 1);
    }

    #[test]
    fn dangling_controller_and_collision_are_reported() {
        let mut av = empty_av();
        av.net.controller_ref = BlockRef(42);
        av.collision_ref = BlockRef(99);
        let root = node(av, Vec::new(), Vec::new());
        let scene = NifScene {
            blocks: vec![root],
            root_index: Some(0),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        let errs = scene.validate_refs();
        assert_eq!(errs.len(), 2);
        assert!(errs
            .iter()
            .any(|e| e.kind == RefKind::Controller && e.bad_index == 42));
        assert!(errs
            .iter()
            .any(|e| e.kind == RefKind::Collision && e.bad_index == 99));
    }

    #[test]
    fn dangling_effect_is_reported() {
        let root = node(empty_av(), Vec::new(), vec![BlockRef(7)]);
        let scene = NifScene {
            blocks: vec![root],
            root_index: Some(0),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        let errs = scene.validate_refs();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, RefKind::Effect);
        assert_eq!(errs[0].bad_index, 7);
    }

    #[test]
    fn out_of_range_root_is_reported() {
        let root = node(empty_av(), Vec::new(), Vec::new());
        let scene = NifScene {
            blocks: vec![root],
            root_index: Some(4),
            truncated: false,
            dropped_block_count: 0,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: BTreeMap::new(),
        };
        let errs = scene.validate_refs();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, RefKind::Root);
        assert_eq!(errs[0].block_index, usize::MAX);
        assert_eq!(errs[0].block_type, "NifScene");
        assert_eq!(errs[0].bad_index, 4);
    }

    #[test]
    fn empty_scene_with_no_root_is_clean() {
        let scene = NifScene::default();
        assert!(scene.validate_refs().is_empty());
    }
}
