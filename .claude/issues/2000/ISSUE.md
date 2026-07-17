# 2000: NIF-D1-03: NiGeomMorpherController reads Morpher Flags / Num Interpolators without their nif.xml version gates

https://github.com/matiaszanolli/ByroRedux/issues/2000

Labels: high, nif-parser, nif, bug

**Severity**: HIGH · **Dimension**: Stream Position Integrity
**Location**: `crates/nif/src/blocks/controller/morph.rs:30-36` (`NiGeomMorpherController::parse`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D1-03)

## Description
nif.xml gates `Morpher Flags` `since="10.0.1.2"` and `Num Interpolators`/`Interpolators` `since="10.1.0.106"`. Both are read unconditionally right after the `NiInterpController` base — mirroring the bug class #1329/#1337 fixed for other blocks in the identical version window, but that work never touched `NiGeomMorpherController`.

## Evidence
```rust
let base = parse_interp_controller_base(stream)?;
let morpher_flags = stream.read_u16_le()?;      // no since=10.0.1.2 gate
let data_ref = stream.read_block_ref()?;
let always_update = stream.read_u8()?;
let num_interpolators = stream.read_u32_le()?;  // no since=10.1.0.106 gate
```

## Impact
On a file version below either threshold (a real, if rare, "old Oblivion" pre-release band), the phantom read misaligns the stream in a band with no `block_sizes` table, cascading unrecoverably.

## Related
#1329, #1337, #1509 (same controller, different gate already fixed)

## Suggested Fix
Gate `morpher_flags` behind `version() >= V10_0_1_2` (default 0 below) and `num_interpolators`/the ref loop behind `version() >= V10_1_0_106` (default empty), matching the pattern already used a few lines below for the trailing `Num Unknown Ints` gate.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other controllers sharing the `NiInterpController` base for the same version band)
- [ ] TESTS: A regression test pins this specific fix (sub-10.0.1.2 and sub-10.1.0.106 fixtures)
