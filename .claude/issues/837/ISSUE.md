# SK-D5-NEW-03: BSLagBoneController + BSProceduralLightningController emit by-design block_size WARN noise per block

## Description

`BSLagBoneController` and `BSProceduralLightningController` are dispatched to `NiTimeController::parse` only (the base class) per the explicit "we don't model yet" comment at `blocks/mod.rs:624-636`. The trailing 3 floats (BSLagBoneController: shake amplitude / damping / shake speed) and 3 interp refs + strip data (BSProceduralLightningController) are consumed by `block_size` recovery and discarded.

The under-consumption fires the WARN-level `block_size` realignment notice on every block, even though the parser is doing exactly what its dispatch comment says. Net effect: 42 WARN events / 11 WARN events for these two types per full Meshes0 sweep, all benign by design but indistinguishable in the log from real per-block drift bugs (the BSLODTriShape ones in the same run, where the parser DOES claim to model the trailer, are actual data-loss).

## Location

`crates/nif/src/blocks/mod.rs:624-636`

## Evidence

Single Meshes0 parse run: `BSLagBoneController` warnings on 42 NIFs (78 blocks aggregated), `BSProceduralLightningController` on 3 blocks.

```rust
// crates/nif/src/blocks/mod.rs:630-636
"BSLagBoneController"  // base + 3 floats
| "BSKeyframeController"
| "BSProceduralLightningController"  // base + 3 interp refs + strip data
| "NiMorpherController"
| "NiMorphController"
| "NiMorphWeightsController" => {
    Ok(Box::new(NiTimeController::parse(stream)?))
}
```

## Impact

For BSLagBoneController (used on cape, cloak, hair, dragon-wing physics) the bone-lag amplitude / damping / shake speed fields are unread. Animations driven by these controllers fall back to engine defaults — visual difference is small but content authors set non-default values intentionally. BSProceduralLightningController is rarer (storms, magic).

The warnings train the eye to ignore ALL `consumed != block_size` warnings, which is bad because real drift bugs (e.g. SK-D5-NEW-07 on BSLODTriShape) sit in the same channel.

## Suggested Fix

Two-part:

1. **Implement the trailing fields** for BSLagBoneController (12 bytes: 3 × f32 — shake amplitude / damping / shake speed) and BSProceduralLightningController. Both are well-documented in nif.xml and straightforward 10-line parsers.

2. **Until then, downgrade the by-design under-consumption to `log::debug!`** for the explicit "we don't model yet" set at `mod.rs:630-636` by emitting them through a typed-stub path that bypasses the realignment WARN — or annotate via a per-type `expected_under_consume: bool` flag so the per-block consumption check in `lib.rs` can downgrade these specific types.

## Related

- SK-D5-NEW-04 (the warning text references closed #615; same channel)
- SK-D5-NEW-07 (BSLODTriShape — the real-drift case that gets buried in this noise)
- Issues #234, #235 (referenced in the code comment)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: All 6 types in the L630-636 group share the same dispatch — pick a consistent treatment (implement-or-suppress) across the group
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic round-trip on a BSLagBoneController block — assert the 3 trailing floats land in the parsed struct

## Source Audit

`docs/audits/AUDIT_SKYRIM_2026-05-05_DIM5.md` — SK-D5-NEW-03