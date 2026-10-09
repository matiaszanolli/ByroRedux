# #5446: PAR-D4-2026-10-08-01: three of the four HKX dimension-gate tests cannot fail if their own clause is removed

**Labels**: low,import-pipeline,animation,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5446

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-08.md` — `PAR-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Corpus Gates (test strength)
- **Location**: `crates/hkx/src/animation.rs:1538-1604` and `:1655-1711`, the fixtures of:
  - `decode_spline_animation_rejects_a_sample_count_bomb`
  - `decode_spline_animation_rejects_frames_beyond_the_declared_blocks`
  - `decode_spline_animation_rejects_a_block_claiming_its_whole_clip`

  The gate they target is `animation.rs:368-395`.
- **Status**: NEW
- **Trigger Input**: n/a (gate weakness).
- **Description**:
  - Every clause of the dimension gate returns the same `InvalidData("unsupported spline clip dimensions")`.
  - Three fixtures leave `mask_size` (offset `0x44`) at 0, so `mask_size != transform_count * 4 + float_count` rejects them whatever clause each test claims to pin.
  - The #5006 fixture also declares 15,998,976 samples, which the #5316 1 M cap rejects on its own.
  - The test comments claim "ONLY the relative check can reject it" (#4655) and "only the MAX_FRAMES_PER_BLOCK ceiling can reject it" (#5006). Both claims are false.
  - Only the new #5316 test, `rejects_the_279kb_spline_probe`, sets `mask_size` correctly and isolates its clause.
- **Evidence**: a mutation probe. In a scratch copy of `crates/hkx`, both the #4655 clause (`num_frames > num_blocks * (mfpb - 1) + 1`) and the #5006 clause (`mfpb <= MAX_FRAMES_PER_BLOCK`) were deleted. `cargo test --lib decode_spline_animation_rejects` then reports **4 passed, 0 failed**.
- **Impact**: the #4655 and #5006 hardening (one-block bypass, frames-vs-blocks tie) can regress with a green unit lane. The real-data census asserts only vanilla maximums, not rejection of hostile shapes.
- **Related**: #4655, #5006, #5316, #5326 (census weakness, separate).
- **Suggested Fix**:
  - Set `mask_size = transform_count * 4` in each fixture.
  - Size the #5006 fixture under 1 M samples, for example 4 tracks × 250,000 frames in 1 block with mfpb 250,001.
  - Ideally return a distinct error string per clause, so each test asserts the clause it pins.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other HKX gate test sharing the single `unsupported spline clip dimensions` error)
- [ ] **TESTS**: A regression test pins this specific fix
