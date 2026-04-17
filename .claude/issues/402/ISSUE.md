# OBL-D6-4: KF animation importer does not handle Oblivion NiSequenceStreamHelper + NiKeyframeController chain

**Issue**: #402 — https://github.com/matiaszanolli/ByroRedux/issues/402
**Labels**: bug, animation, nif-parser, high, legacy-compat

---

## Finding

`crates/nif/src/blocks/controller.rs:922-924` has the explicit comment: *"We don't currently consume this from the animation importer — that work remains as a follow-up"*.

`crates/nif/src/anim.rs::import_kf` (line 227) walks only two paths:
1. `NiControllerManager → sequence_refs` (Skyrim+ path)
2. Top-level `NiControllerSequence` (Skyrim-era root)

Oblivion/FO3/FNV KF files use an **older root**: `NiSequenceStreamHelper` containing per-bone `NiKeyframeController` chains (`NiSingleInterpController` + `NiTransformData` pair). Both blocks **parse** (`controller.rs:928`, `blocks/mod.rs:350, 390`), but `import_kf` never inspects them → clip list returns empty on every Oblivion `.kf` file.

## Impact

- Every Oblivion door idle animation, torch flicker via NiKeyframeController, creature/character animation, and in-NIF embedded animation controller is dead-on-arrival.
- Static meshes render fine (they don't depend on `import_kf`), but as soon as the cell loader or animation system asks for a clip, nothing's there.

The name-resolution infrastructure is already in place: `byroredux/src/anim_convert.rs::build_subtree_name_map` walks the scene graph and builds the node-name → bone-index map. Only the KF reader path is missing.

## Fix

Add **Path 3** in `import_kf`:
1. Scan top-level blocks for `NiSequenceStreamHelper`.
2. Walk its extra-data chain to find `NiTextKeyExtraData` (event markers) and the name string.
3. For each `NiNode` in the target scene, walk its `controller` chain collecting `NiKeyframeController` + `NiTransformController`.
4. Each controller references `NiTransformData` (keyframe data) directly in the Oblivion era (rather than via `NiTransformInterpolator → NiTransformData` as in Skyrim+).
5. Build `AnimationClip` with channels keyed by target node name, convert to ECS animation player input via existing `anim_convert`.

Approximate size: 1-2 days.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Same NiKeyframeController+NiTransformData pattern is used for FO3/FNV KF files — landing this unblocks animation on all three pre-Skyrim titles.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Import `meshes\creatures\imp\idle.kf` (or an equivalent Oblivion KF with `NiSequenceStreamHelper` root), assert clip with N keyframe channels present, play via AnimationPlayer and verify bone transforms at t=0 and t=clip_duration/2 match expected values.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 6 #4.
