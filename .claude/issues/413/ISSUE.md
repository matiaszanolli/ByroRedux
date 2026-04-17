# FO4-D5-M1: BSBehaviorGraphExtraData over-reads 3 bytes on every FO4 occurrence (controls_base_skeleton u8 vs u32)

**Issue**: #413 — https://github.com/matiaszanolli/ByroRedux/issues/413
**Labels**: bug, nif-parser, medium, legacy-compat

---

## Finding

`crates/nif/src/blocks/extra_data.rs:272` (`BSBehaviorGraphExtraData::parse`) reads:
- `name: read_string` (4 bytes)
- `behaviour_graph_file: read_string` (4 bytes)
- `controls_base_skeleton: read_u32_le` (4 bytes)
= **12 bytes**

The file reserved **9 bytes** → 3-byte over-read on every FO4 occurrence.

## Evidence

FO4 real-data sweep logged 736 warnings in `Fallout4 - Meshes.ba2` alone of the form:
```
Block N 'BSBehaviorGraphExtraData': expected 9 bytes, consumed 12
```

Strings themselves parse correctly — the Dim 5 probe confirmed `deathclaw.nif` reports `graph="GenericBehaviors\CharFXAttachBurst\CharFXAttachBurst.hkx"` and `workshoplightbox01.nif` reports `"GenericBehaviors\WorkshopColorsNoTransitions\...hkx"`.

The 3-byte delta is a textbook u8-vs-u32 field-width mismatch. Most plausible: **`controls_base_skeleton` is a `u8` bool** on BSVER 130+, not `u32`. Matches the common Bethesda pattern of "bool on the wire" for newer games.

## Impact

- `controls_base_skeleton` reads 4 bytes of unrelated next-block data as the bool value — currently junk on FO4.
- The over-read steals 3 bytes from the subsequent block before `block_size` recovery seeks back. On clean content this is harmless; combined with FO4-D5-H3 (unchecked `Vec::with_capacity`), a malformed NIF can convert this into an OOM.
- Since Havok behavior graphs are parse-only today (verified in Dim 6 §5 — zero downstream consumers), **functional impact is low** — but the data is wrong for when they become used.

## Fix

Gate the field width on BSVER:

```rust
let controls_base_skeleton = if stream.bsver() >= 130 {
    stream.read_u8()? != 0  // FO4+: u8 bool
} else {
    stream.read_u32_le()? != 0  // Skyrim: u32 bool
};
```

Verify against nif.xml's `BSBehaviorGraphExtraData` `Controls Base Skeleton` field width condition before picking the boundary.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Dim 5 M-2 lists ~8 other block types (BSClothExtraData, NiStringsExtraData, BSProceduralLightningController, BSLagBoneController, BSSkyShaderProperty, BSWaterShaderProperty) with per-block size mismatches on FO4. Same field-width-per-BSVER audit shape.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic BSBehaviorGraphExtraData at BSVER=130 with `controls_base_skeleton=true` round-trips with `consumed == 9`; live test assert zero warnings on a full FO4 meshes sweep.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 5 M-1.
