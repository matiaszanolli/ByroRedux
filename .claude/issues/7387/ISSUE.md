# #7387 — REN-D12-2026-11-23-02: `--cornell-reference-view` is validated globally (0..=10), so a view outside a rung's own range silently aliases another view

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D12-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Cornell Harness
- **Location**: `cornell_reference_view` and the `ReferenceRung::SkyEnvironment` arm of `setup_reference_scene` (`byroredux/src/cornell/reference.rs`)
- **Status**: NEW
- **Description**: Since d079fd404 the parser accepts `0..=MAX_REFERENCE_VIEW` (10) for every rung. Per-rung ranges are: `sky-env` 0..=5, `sky-probe` 0..=10, `materials` / `materials-spec` / `far-sun` / box rungs 0..=2, the rest ignore it. Out-of-range values no longer error: `sky-env` clamps with `(view as usize).min(SKY_ENV_FACES.len() - 1)` (views 6..=10 re-capture face 5, the -Z face), and the 0..=2 rungs fall into their `_ =>` / `_` arm (views 3..=10 become view 2). Before d079fd404 a bad view was a hard error for all rungs.
- **Evidence**: the test `view_selector_defaults_to_zero_and_rejects_out_of_range` only pins 11 as the global bound; nothing pins a per-rung bound.
- **Impact**: A mistyped view in a gate invocation produces a plausible but duplicated capture (for `sky-env`, a face that then overwrites the lat-long seam source) instead of a refusal. `scripts/m-light/gates.py` iterates the right ranges today, so no wrong number ships; this is harness robustness.
- **Related**: RLT-REF-012 / RLT-REF-021; #7186 touched the same CLI surface.
- **Suggested Fix**: make `ReferenceRung` own its view count and reject `view >= count` in `setup_reference_scene` (or the parser once the rung is known); drop the `.min()` clamp.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
