# #5326: PAR-D4-2026-10-05-01: The two real-data gates added since 09-29 can pass vacuously or under-assert

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5326
- **Labels**: low,import-pipeline,bug,test-gap,game:skyrim,game:starfield
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D4-2026-10-05-01)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/hkx/src/animation.rs:1236-1305` (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`); `crates/sfmaterial/tests/real_cdb.rs:161-187` (`material_index_dlc_cdb_resolves_the_base_corpus`)
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness)
- **Description**:
  - **HKX census.**
    - It asserts only `max_mfpb <= 256`. It has no `clips > 0` check and no pinned clip or skip counts (6,126 / 1,573 today).
    - A `Packfile::parse` or extract regression therefore counts every file as "skipped" and passes with `max_mfpb = 0`.
    - It reads raw fields and never runs `decode_spline_animation`, so a vanilla clip rejected by any other gate stays invisible. That includes the `mask_size` tie and the block-chain checks.
    - The name promises "ceilings" (plural), but frames, blocks and samples are not asserted. Asserting samples is also the natural guard for PAR-D1-2026-10-05-02.
  - **SFBGS007 DLC test.**
    - `if !dlc.exists() { eprintln!(SKIP); return; }` passes green under `BYROREDUX_REQUIRE_GAME_DATA`, against the #3850 strict-lane contract.
    - It asserts only `.is_some()`. Every keyed object returns `Some`, even with zero textures, so the DLC CDB's `Components` ↔ stream alignment (PAR-D2-2026-10-05-02) is unverified.
  - **Doc drift.** The base test's comment records "468 MB HWM / 2.2 s"; this run measured 244 MB / 1.6 s on the same machine.
- **Evidence**: the test bodies; strict-lane output `census: 6126 clips, 1573 non-clip/undecodable files; max mfpb=256, max frames=1471, max blocks=15, max samples=124821`.
- **Impact**: a decode regression in HKX, or a misaligned DLC CDB, can pass the nightly `parsers` lane.
- **Related**: #3850, #4660, #5006, #5011, PAR-D2-2026-10-05-02
- **Suggested Fix**:
  - **Census:** pin the clip count, bound the skip count, decode each clip through `decode_spline_animation` and count the rejects (0 expected), and assert the frame, block and sample maximums against the gate constants.
  - **DLC test:** route the DLC skip through `require_game_data`. Assert the five Nightmare slots as the base test does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
