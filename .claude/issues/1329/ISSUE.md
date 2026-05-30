**Severity:** HIGH · **Dimension:** Coverage · **Game Affected:** Oblivion (TES4, v20.0.0.5 — sizeless, no block_sizes table)

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-01).

## Description
`bhkConvexSweepShape` (nif.xml `inherit="bhkConvexShapeBase"`, `V10_0_1_0`) and `bhkMeshShape` (`inherit="bhkShape"`) have **no dispatch arm** in `parse_block`. On Oblivion there is no per-block size to realign, so the failed parse cannot be skipped — the loader **truncates the remainder of the file**.

## Location
- `crates/nif/src/blocks/mod.rs:1153` (`_ =>` fallback returns `Err` when `block_size == None`)
- recovery: `crates/nif/src/lib.rs:557-620`

## Evidence
Probe (header `num_blocks` vs parsed count) on `Oblivion - Meshes.bsa`:
- `meshes\clutter\farm\handscythe01.nif` — header=47, parsed=3 → **lost 44**
- `meshes\clutter\farm\oar01.nif` — header=17, parsed=3 → **lost 14**
- `meshes\architecture\basementsections\ungrdltraphingedoor.nif` — header=31, parsed=3 → **lost 28**

The bhk shape sits early (block ~3, under the collision object), so the *render geometry* that follows is discarded. `parse_nif` returns `Ok` (truncation = "successful recovery"), masking the loss from the clean-rate gate. Confirmed: no dispatch arm for either type in `crates/nif/src/blocks/mod.rs`.

## Impact
A scythe, an oar, and a hinged trapdoor render as empty/stub geometry in Oblivion. Low file count but total data loss for those files; residual tail of the Oblivion sizeless-cascade class (#474, #979, #980 — all CLOSED).

## Suggested Fix
Add dispatch arms for both shapes. Even a structured skip-to-known-size stub registered in `oblivion_skip_sizes` (#224 mechanism) stops the truncation and lets render geometry load. Layouts: `bhkConvexSweepShape` = bhkShape ref + material + radius + unknown vec; `bhkMeshShape` ≈ legacy bhkNiTriStripsShape. Verify against nifly `BSHavok` decoders / nif.xml.

## Related
#474, #979, #980 (same mechanism, CLOSED), #224 (`oblivion_skip_sizes`), and the missing parse-block regression pin (filed separately as NIF-2026-05-29-04).

## Completeness Checks
- [ ] **SIBLING**: Audit the full Oblivion Havok shape family (other `bhk*Shape` types) for the same sizeless-cascade gap, not just these two
- [ ] **COVERAGE-PIN**: Once fixed, add the per-game block-count parity test (see companion finding NIF-2026-05-29-04) so this class can't regress silently
- [ ] **TESTS**: Regression test parsing the 3 named vanilla NIFs asserts `parsed_blocks == header.num_blocks`
