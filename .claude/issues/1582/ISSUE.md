**Severity**: MEDIUM · **Dimension**: GPU Pipeline · **Status**: RE-AFFIRMED carry-over (prior F1/PERF1-01, 2026-06-11) — never filed
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F2)

## Description
For every light that produces a screen-projected splat, `crates/renderer/shaders/caustic_splat.comp` recomputes the 5×5 Gaussian normalization `wsum` (25 `exp()`, `:408-411`) and the per-tap weight `exp(...)/wsum` (25 more, `:416`) inside the per-light loop `for li` opened at `:258`. All 50 `exp()` per light depend only on the fixed kernel offsets and constant σ=1 — fully loop-invariant.

## Evidence
Verified live (`caustic_splat.comp:408-417`): `wsum` is summed via `exp(-float(kx*kx+ky*ky)*0.5)` and each tap recomputes the same `exp(...)/wsum`, both nested inside the per-light loop.

## Impact
Up to N_LIGHTS × 50 transcendentals per glass/caustic pixel where 25 computed once suffices. Bounded to glass-source pixels (O(pixels)) but pure waste in a transcendental-heavy compute pass on glass-heavy interiors with multiple caustic lights.

## Suggested Fix
Compute `wsum` once before the `for li` loop (it is a compile-time constant); precompute a `const float kGauss5[25]` of pre-normalized weights indexed `(ky+2)*5+(kx+2)`. Recompile with plain `glslangValidator -V`.

## Completeness Checks
- [ ] **SIBLING**: Check the same loop-invariant kernel recompute in `water_caustic.comp` / any other per-light splat pass
- [ ] **TESTS**: Recompile shader with plain `-V`; confirm caustic output unchanged (Cornell / glass-interior smoke)
