# OBL-D1-01: NiKeyframeController.Data (until=10.1.0.103) is never read -- the sole remaining Oblivion truncation

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2562
**Finding ID**: OBL-D1-01

**Severity**: MEDIUM
**Dimension**: NIF Version Handling (v20.0.0.4/.5 retail + v10.x NetImmerse Tail)
**Location**: `crates/nif/src/blocks/mod.rs:748-758`, `crates/nif/src/blocks/controller/mod.rs:253-267`
**Status**: NEW

## Description
nif.xml defines a complementary pair (`NiSingleInterpController.Interpolator since="10.1.0.104"` / `NiKeyframeController.Data until="10.1.0.103"`) so the block always carries exactly one 4-byte ref; the dispatcher routes `NiKeyframeController` straight to `NiSingleInterpController::parse`, which only reads the `>= 10.1.0.104` field. Below that version nothing is read and the block ends 4 bytes early. The helper this needs already exists (`NifVersion::has_keyframe_controller_data()`) but its only caller is `BsKeyframeController`.

## Evidence
Byte-traced on `meshes\marker_map.nif` (v4.2.1.0): the parser drops 8 of 13 blocks, including both `NiTriShape` subtrees — the Oblivion world-map marker imports with no geometry. This is the only file in `Oblivion - Meshes.bsa` (1/8032) that still truncates; fixing it takes Oblivion sizeless parity to 8032/8032. Confirmed directly: `NiSingleInterpController::parse` (`controller/mod.rs:253-267`) reads `interpolator_ref` only when `stream.version() >= NifVersion::V10_1_0_104`, else `BlockRef::NULL` with no read at all.

## Impact
The one remaining Oblivion NIF truncation — `marker_map.nif` imports with no geometry (drops 8/13 blocks).

## Related
#2345 (sibling `ControlledBlock` mis-gate, still open).

## Suggested Fix
Give `NiKeyframeController` its own parser calling the existing gate helper (`NifVersion::has_keyframe_controller_data()`), split `NiVisController`/`NiAlphaController`/`NiTransformController` out of the shared arm with the same gate.

## Completeness Checks
- [ ] **TESTS**: A regression test decodes `marker_map.nif` and confirms full 13-block parity
- [ ] **SIBLING**: OBL-D1-02 (this session) covers the same root cause in five other controller parsers — fix together
