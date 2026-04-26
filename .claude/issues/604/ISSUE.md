# D5-NEW-01: NiLookAtInterpolator reaches extract_transform_channel but is never decoded

## Finding: D5-NEW-01

- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: Oblivion (embedded look-at chains), FNV (~18 occurrences in R3 histogram), Skyrim SE (~5)
- **Location**: [crates/nif/src/anim.rs:874-915](crates/nif/src/anim.rs#L874-L915), [crates/nif/src/blocks/interpolator.rs:407-450](crates/nif/src/blocks/interpolator.rs#L407-L450)

## Description

Commit 7548e64 (Session 18) added the `NiLookAtInterpolator` block parser as the modern replacement for the deprecated `NiLookAtController`. The block parses correctly and `parse_block` dispatches it (mod.rs:597). However, when a `ControlledBlock.interpolator_ref` resolves to `NiLookAtInterpolator`, `extract_transform_channel` checks only `NiTransformInterpolator` (line 888) and `NiBSplineCompTransformInterpolator` (line 910), then returns `None`. The blend-target shim from #334 does not change this — it resolves a `NiBlendTransformInterpolator` to a sub-interpolator that may itself be a `NiLookAtInterpolator`.

Continuation of closed #228 (NiLookAtController + NiPathController). #228 was closed when the parsers landed; the import-side dispatch did not.

## Evidence

```rust
// crates/nif/src/anim.rs:888-915
if let Some(interp) = scene.get_as::<NiTransformInterpolator>(interp_idx) { ... }
// Fall back to the Skyrim / FO4 NiBSplineCompTransformInterpolator path.
if let Some(interp) = scene.get_as::<NiBSplineCompTransformInterpolator>(interp_idx) {
    return extract_transform_channel_bspline(scene, interp);
}
None  // <-- NiLookAtInterpolator + NiPathInterpolator fall through here
```

## Impact

Embedded look-at sequences silently degrade to static transforms. Single-clip KF playback works; NIFs with controller-manager chains containing a look-at sub-interpolator lose head/eye tracking on creatures and dragons.

## Suggested Fix

Add a third branch in `extract_transform_channel` that downcasts to `NiLookAtInterpolator`. Either sample the look-at vector against the static transform from the NiAVObject and emit a constant TransformChannel (when target is the world origin), or surface a `LookAt` ECS component carrying the target ref so the runtime can compute the rotation each frame.

## Related

- #228 (closed): parser added; import dispatch missing.
- #334 (closed): NiBlendInterpolator shim — leaves the resolved sub-interpolator unhandled if it is NiLookAtInterpolator.
- D5-NEW-02 (companion finding for NiPathInterpolator).

## Completeness Checks

- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: NiPathInterpolator (D5-NEW-02) requires the same dispatch; bundle.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Construct synthetic NIF with NiLookAtInterpolator → verify TransformChannel emitted with non-identity rotation.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
