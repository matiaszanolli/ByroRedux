# SKY-D1-2026-09-11-01: BSTriShape particle-data trailing read gated on bsver < FALLOUT4 where nif.xml gates it on exact BSVER 100 (#BS_SSE#)

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4249

**Severity**: LOW
**Dimension**: 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:659-668`
**Status**: NEW

**Description**: `BSTriShape`'s particle-data trailing read is gated on `bsver < FALLOUT4` (a broad range) where `nif.xml` gates the field on `#BS_SSE#` (BSVER exactly 100). No vanilla content reaches the gap (Skyrim LE ships `NiTriShape`, not `BSTriShape`), so blast radius is limited to modded/backported/synthetic geometry with a mis-detected BSVER header.

**Evidence**: Confirmed in current code — `bs_tri_shape.rs:659`: `if stream.bsver() < crate::version::bsver::FALLOUT4 {` reads the particle-data size/arrays for any BSVER below FO4's 130, not just the exact SSE value of 100 that nif.xml's `#BS_SSE#` vercond specifies.

**Impact**: Latent — no vanilla corpus reaches BSVER values between the SSE value (100) and FO4 (130) on a `BSTriShape` block, so no observed exposure today. A future intermediate BSVER band or a mis-detected header would misalign the read.

**Suggested Fix**: Narrow the gate to the exact `#BS_SSE#` predicate (BSVER == 100) matching nif.xml, rather than the broad `< FALLOUT4` range.

## Completeness Checks
- [ ] **SIBLING**: Verify no other `BSTriShape`-family field uses the same broad `< FALLOUT4` gate where nif.xml specifies an exact BSVER
- [ ] **TESTS**: A fixture at an intermediate BSVER (between 100 and 130) pins the corrected exact-match gate
