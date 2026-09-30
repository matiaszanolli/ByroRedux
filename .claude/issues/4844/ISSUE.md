# #4844: REN-D1-2026-09-24-05: post-#4576 hygiene — a doc comment attached to the wrong item, an unused import, a partition test that only feeds `dst_blend = 7`

**Labels**: bug,renderer,low,tech-debt

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D1-2026-09-24-05**._

- **Severity**: LOW (dead code / doc / test coverage; works correctly).
- **Dimension**: AS Correctness
- **Location**: `acceleration/predicates.rs` — `GAMEBRYO_DST_BLEND_ONE` / `mask_divert_cause`; `acceleration/tests/tlas_tests.rs` (imports, `every_actor_instance_is_either_bucketed_or_diverted_exactly_once`); `telemetry.rs` #4581 comment.
- **Status**: NEW
- **Description / Evidence**: #4576 inserted `/// Gamebryo DstBlendMode::ONE …` + `const GAMEBRYO_DST_BLEND_ONE: u8 = 0;` directly under the `mask_divert_cause` doc block, so that block now attaches to the const and the function has no doc. `cargo test` warns `unused import: MATERIAL_KIND_NO_LIGHTING` (`tlas_tests.rs`); the workspace already has a red `clippy -D warnings` (#4765). The exactly-once actor test feeds `dst_blend = 7` only; the additive×Actor partition is covered only by `divert_cause_matches_the_mask_it_explains`. Cosmetic: `//Glass` (missing space) in the `telemetry.rs` #4581 comment.
- **Suggested Fix**: Move the const above the doc block, drop the import, loop `dst_blend` in the exactly-once test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

