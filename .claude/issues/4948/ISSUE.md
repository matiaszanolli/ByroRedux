# #4948: REN-D1-2026-09-27-01: `5eb07a4f3`'s TLAS refit-identity rule (EntityId tie-break + membership-replacement → BUILD) has no test; the one updated test makes the tie-break vacuous

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4948
- **Labels**: low,renderer,vulkan,test-gap,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D1-2026-09-27-01**._

- **Severity**: LOW (test gap; the code traces correct)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas`: the `if use_update && !tlas.last_entity_ids.iter().copied().eq(…)` block and the post-record `last_entity_ids` refresh); `crates/renderer/src/vulkan/acceleration/predicates.rs` (`sort_tlas_instances_by_blas_address`, `.then_with(|| entity_ids_by_ssbo[… low_24() …])`); `crates/renderer/src/vulkan/acceleration/tests/predicates_tests.rs` (`tlas_instance_sort_key_is_independent_of_draw_order`)
- **Status**: NEW
- **Description**: `5eb07a4f3` added two behaviours:
  - The TLAS canonical order is now `(BLAS address, full EntityId)`, with the EntityId found via `tlas_entity_ids_scratch[instance_custom_index]`.
  - A same-address, same-count frame whose entity membership differs is forced from UPDATE to BUILD.

  Neither behaviour is exercised by any test. The only test touched was `tlas_instance_sort_key_is_independent_of_draw_order`, now called as `sort_tlas_instances_by_blas_address(&mut instances, &[0])`. Every instance in it has `instance_custom_index = 0` and a distinct address, so the tie-break is never reached. The membership rule is inline in `build_tlas`, so without a device it cannot be tested at all. `decide_use_update`, the pure decision helper the module doc names as the home of the BUILD-vs-UPDATE decision, still sees only `needs_full_rebuild`, the map generation and the address slices. A future edit could drop the entity check, break the tie-break, or refresh `last_entity_ids` on the UPDATE arm, and every test would stay green. The result would be the progressive refit-quality loss the commit exists to fix: legal, invisible, and a silent performance regression.
- **Evidence**: `grep -rn "last_entity_ids\|tlas_entity_ids_scratch" crates/renderer/src` finds no test reference. `predicates_tests.rs` has `&[0]` as the only entity table.
- **Impact**: Regression exposure only. Correctness today: I traced the tie-break index (always `< draw_commands.len()`, because the map compacts to ≤ i and #4833 caps it), the refresh happening only on BUILD, and the rollback invalidation (`invalidate_tlas_recording`, reached from all three `draw_frame` tail `Err` sites via `rollback_skin_frame_state`). All hold.
- **Related**: #3666 (the canonical sort this extends); #3991 / #2674 (record-time commit discipline).
- **Suggested Fix**: Fold the entity-sequence comparison into `decide_use_update` (pass `last_entity_ids` plus the current canonical ID sequence). Add tests for: equal-address instances ordered by entity ID regardless of input order; the same address multiset with a swapped entity forcing BUILD; an unchanged membership with permuted SSBO indices still selecting UPDATE.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
