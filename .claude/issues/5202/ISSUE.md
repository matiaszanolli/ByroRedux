# #5202 — REN-D1-2026-10-03-04: `docs/engine/renderer.md` describes the BLAS budget with the pre-#3839 formula

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW (doc-rot)
- **Dimension**: AS Correctness (documentation)
- **Location**: `docs/engine/renderer.md`:
  - the feature bullet "LRU eviction (budget = `device_local / 3`, floored at 256 MB)";
  - the Acceleration Structures section's "**BLAS LRU eviction**: budget is `device_local / 3`, floored at `MIN_BLAS_BUDGET_BYTES = 256 MB`".
- **Status**: NEW. #3866 fixed four other sites of this exact staleness (`mod.rs`, `constants.rs`, `predicates_tests.rs`, `memory-budget.md`). `renderer.md` was not in its scope.
- **Description**: The real rule is `blas_budget_for_heap(heap, reserved) = ((heap − reserved) / 3).clamp(MIN_BLAS_BUDGET_BYTES, MAX_BLAS_BUDGET_BYTES)`.
  - `reserved = screen_scaled_reservation_bytes(extents, volumetrics, upscaler_sdk_bytes)`, which is re-derived on resize by `recompute_blas_budget_for_current_state`.
  - `MAX_BLAS_BUDGET_BYTES` is 1 GiB.
  - The doc omits the reservation, the 1 GiB ceiling and the runtime re-derivation. `memory-budget.md` already states the correct formula.
  - The same section also says static BLAS are "built once when the mesh is uploaded". In fact they are built by `build_blas_batched` at cell or NIF load and restored after eviction by `restore_missing_static_blas_for_draws`.
- **Impact**: An operator sizing VRAM from `renderer.md` overestimates the BLAS budget, by a factor of 4 on a 12 GB card at the ceiling.
- **Related**: #3866, #3839, #3988.
- **Suggested Fix**: Restate both sites against `blas_budget_for_heap` and link `memory-budget.md` §Acceleration Structures as the ledger.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
