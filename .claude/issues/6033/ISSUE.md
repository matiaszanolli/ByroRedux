# REN-EXT-L-04: ground-cover push-constant blocks have no field-for-field lockstep guard; two use hand-rolled byte casts

Labels: low,renderer,shaders,test-gap,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-18.md

- **Severity**: LOW (test gap)
- **Dimension**: GPU-Struct
- **Location**: `ScatterPush` / `BladePush` in `groundcover.rs` with `scatter_push_bytes` / `blade_push_bytes` in `groundcover/frame.rs`; `ModelPush` in `groundcover_models.rs`
- **Status**: NEW
- **Description**: The descriptor-set side of these pipelines is reflection-validated and the storage records run through `assert_glsl_matches_rust`. The push blocks are not. `push_block_fits_the_guaranteed_minimum` pins only `BladePush` = 64 B, `ScatterPush` ≤ 128 B and the byte-52 offset of the previous-clock lane. `ModelPush` sizes through `NoUninit`/`byte_view` (#5122); the other two push structs use `slice::from_raw_parts`. I diffed all three by hand against their GLSL blocks and they agree today, so this is drift risk only.
- **Suggested Fix**: add the three blocks to the mirror table, or reflect push-constant member offsets in `reflect.rs`; route the two byte views through `byte_view` with a `NoUninit` impl.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
