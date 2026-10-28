# NIF-D4-2026-10-27-01: `SkeletonBones::from_scene` is a third hand-rolled NIF world-transform composition that #6303 left behind (no scale, no #4549 sanitiser, bare `NiNode` downcast, unreported parse)

Labels: low, nif-parser, nif, game:starfield, bug
Source: docs/audits/AUDIT_NIF_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_NIF_2026-10-27.md` -- finding `NIF-D4-2026-10-27-01` (LOW)
**Location (re-mapped to current code)**: `crates/nif/src/import/mesh/skeleton.rs` (`SkeletonBones::from_scene` ~:76-131; `resolve_external_bone_names` parse+cache ~:290-295). Premise re-read at HEAD: still builds the parent map with `scene.get_as::<NiNode>` only, folds rotation x translation by hand with no scale term and no sanitiser.
**Status at filing**: NEW (follow-through on #6303, closed, which moved `extract_np_ragdoll`'s `node_world` onto `compose_transforms` + `any_ni_node`)

## Description
The Starfield external-skeleton `BSSkin` bone-name resolver builds each joint's bind world translation by hand:
- the parent map and the chain use only `scene.get_as::<NiNode>` (no `any_ni_node`);
- rotation x translation is folded manually with **no scale term** (the child translation is never multiplied by the parent's scale, which `compose_transforms` does);
- there is no `sanitize_transform_translation_and_scale` (#4549).

The skeleton is parsed with `crate::parse_nif(&bytes).ok()?` and cached for the process; a truncated or recovered skeleton is never reported because the `note_scene_health` path lives in the binary crate.

## Evidence
A probe measured the three Starfield human skeletons: `meshes\actors\human\characterassets\skeleton.nif` (116 nodes), `_1stperson` (112), `human_crowd` (90). All are plain `NiNode` and every node has scale 1.0, so vanilla output is correct today. The other remaining hand composition (`ragdoll.rs::node_world`) already folds `compose_transforms`; a grep over `crates/nif/src` finds no further copies.

## Impact
Latent. Wrong joint positions need a mod skeleton with a non-unit bone scale or a `BSMultiBoundNode` / `NiSwitchNode` parent. Even then the anchor solver declines rather than mis-assigns, so the worst case is the existing `Bone{i}` + bind-pose fallback.

## Related
#6303, #3549, #4549

## Suggested Fix
Build the parent map through `any_ni_node` and fold `compose_transforms`, or share one `node_world` helper with `import/collision/ragdoll.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

