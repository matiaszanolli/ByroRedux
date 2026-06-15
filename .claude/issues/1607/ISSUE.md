# #1607 — NIF-D1-01: Residual v10.1.0.106 interpolator stride drift (2 Oblivion files)

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: MEDIUM (sizeless-format truncation the `block_size` reconciliation cannot cover; small blast radius) · **Dimension**: Stream Position (+ Version Gating, coupled to the v10.1.0.x sub-band) · **Status**: NEW — confirms doc finding NIF-NEW-03 (residual tail)
**Source**: AUDIT_NIF_2026-06-14 (NIF-D1-01)
**Game Affected**: Oblivion (the v10.1.0.106 / bsver-5 sub-corpus, sizeless format).

**Location**: the `NiBoolData` / `NiBoolInterpolator` / `NiBlendPoint3Interpolator` chain ([blocks/interpolator.rs](crates/nif/src/blocks/interpolator.rs)); surfaces at the subsequent block in [lib.rs](crates/nif/src/lib.rs) (`parse_nif_with_options`).

## Description
After the v10.1.0.x version-predicate + `Manager Controlled` bool fixes landed, the Oblivion truncation set dropped 56 → 8 files. The prior audit's named root files (`pickold.nif`, `_1stperson\skeleton.nif`) are now FIXED by #1506/#1509. Two of the residual 8 (`scampswitch01.nif`, `arwelkydclusterfx01.nif`, both file-version 10.1.0.106) still drift inside the bool/blend-interpolator chain.

## Evidence
Byte-trace shows a block reading `0x3F800000` (the float `1.0`) as a `u32` array count; downstream error `unknown KeyType: 16744447` confirms a misaligned offset entered upstream in the interpolator chain. Both files are already tracked in `oblivion_truncations.tsv`, so the no-new-truncation gate covers them.

## Impact
2 of the 8 remaining Oblivion truncations — minor scamp-switch + cluster-FX animation rigs lose their tail. No corruption or OOM (guards hold); the head still imports.

## Related
06-13 NIF-NEW-03; #1506; #1509.

## Suggested Fix
Byte-audit the v10.1.0.106 `NiBoolData` / `NiBoolInterpolator` / `NiBlendPoint3Interpolator` stride against nif.xml (the bool-interpolator base and blend-interpolator variants carry version-gated fields that shift between 10.1.x and 20.x). Add a v10.1.0.106 bool-interpolator fixture. Shares no new version-constant prerequisite (those landed in #1506).

## Completeness Checks
- [ ] **SIBLING**: Verify the same v10.1.0.106 stride against the other blend-interpolator variants (`NiBlendFloatInterpolator`, `NiBlendTransformInterpolator`)
- [ ] **TESTS**: Add a v10.1.0.106 bool-interpolator fixture pinning the fix
