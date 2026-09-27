# EXT-D3-2026-09-27-07: Guard and contract gaps around the model tier and #4729 (bundle)

**Issue**: #4923
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,test-gap

**Severity**: LOW (test gap / doc rot)
**Dimension**: Ground-cover pipeline
**Tier Violated**: n/a
**Game Affected**: n/a
**Status**: NEW
**Location**:
see each item
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
1. `groundcover_seed_hashes_are_integer_and_frame_invariant` (`groundcover.rs:2299-2346`) still pins three seed paths. `gcModelHash` and atomic-free placement are unpinned.
2. `gcWaterAdmits` switches on bare `0u..5u` (`comp:215-221`) with no generated constant matching `CoverWaterRule::gpu_code`.
3. The packing `record | (candidate << 8)` (`comp:311,404`) requires `MAX_RECORDS ≤ 256` with no assert.
4. The third assert of `model_emission_preserves_flat_shading_and_render_layer_flags` (`groundcover_models.rs:948-952`) is tautological.
5. The #4729 grass pins are source-text only, and the SpeedTree gust-travel term (`billboard.rs:233-234`) is unpinned.
6. The #4338 doc comment sits on the wrong test (`groundcover.rs:2107-2110`).

## Suggested Fix
Per item:
- extend the seed guard;
- generate `GROUNDCOVER_WATER_RULE_*`;
- add a const assert on the packing bounds;
- drop the tautology;
- pin the crest travel behaviourally;
- move the doc comment back to its test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
