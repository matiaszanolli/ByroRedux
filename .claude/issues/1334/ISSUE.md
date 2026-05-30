**Severity:** LOW · **Dimension:** Coverage · **Game Affected:** Skyrim SE (has block_sizes)

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-06).

## Description
`bhkPlaneShape` (nif.xml `inherit="bhkHeightFieldShape"`, `#BETHESDA#`) has no dispatch arm. SSE carries a block_sizes table, so the fallback skips the block cleanly and emits one `NiUnknown`; the rest of the file parses (no cascade — contrast the Oblivion HIGH finding).

## Location
`crates/nif/src/blocks/mod.rs:1153` fallback

## Evidence
Probe on `Skyrim - Meshes1.bsa`: `meshes\plants\switchnodechildren\slaughterfisheggcluster01_1.nif` — header=9, parsed=9, niunknown=1 → complete. 1 instance / 1 file in all vanilla SSE meshes. Confirmed: no dispatch arm in `crates/nif/src/blocks/mod.rs`.

## Impact
The egg-cluster loses its planar collision shape (won't collide as a ground plane); the mesh renders fully. Cosmetic/physics-only, single vanilla file.

## Suggested Fix
Add a `bhkPlaneShape` arm parsing the bhkHeightFieldShape layout (material + plane normal + constant), or accept the LOW-severity NiUnknown skip. Confirm #766 scope first.

## Related
#766 (SE Havok long-tail incl. bhkPlaneShape — CLOSED; this instance is the collision-shape variant the close-out may not have wired a parser for — verify scope).

## Completeness Checks
- [ ] **SIBLING**: Re-verify #766's SE Havok long-tail scope — was `bhkPlaneShape` intended to be covered and missed?
- [ ] **TESTS**: If a parser is added, fixture asserts the plane shape resolves to a `CollisionShape` via `extract_collision`
