# bug, renderer, low

## REN-D14-NEW-01: Combined caustic accumulator sum can wrap u32 before the float divide

**Severity**: LOW
**Dimension**: Caustics
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW (theoretical)

## Description
`composite.frag` does `float(causticRaw + waterCausticRaw) / CAUSTIC_FIXED_SCALE`. Each per-pixel accumulator is clamped *per deposit* to `0xFFFFFFFFu / scale`, but the atomic *sum* across many deposits can climb toward the u32 ceiling on either accumulator; adding two large values wraps modulo 2^32 to a small number → near-black pixel where a bright caustic cusp should be. The `min(…, 16.0)` firefly cap runs AFTER the divide and cannot recover a wrapped value.

## Evidence
- `crates/renderer/shaders/caustic_splat.comp` bounds each `imageAtomicAdd` argument, not the running per-pixel total.
- `crates/renderer/shaders/composite.frag:376` — `float causticLum = float(causticRaw + waterCausticRaw) / CAUSTIC_FIXED_SCALE;` (sums two accumulators before the divide); firefly cap `const float CAUSTIC_FIREFLY_MAX = 16.0;` at line ~385 applies after.

## Impact
Cosmetic flicker (occasional dark pixel) only at extreme caustic concentration with overlapping glass + water caustics. Not observed in shipping content (physical attenuation keeps values well below the ceiling).

## Suggested Fix
Promote to 64-bit before the add, or clamp each raw to a shared `CAP <= 0x7FFFFFFF` before summing.

## Completeness Checks
- [ ] **SIBLING**: both accumulator writers (`caustic_splat.comp` glass + the water-side splat) share the same per-deposit / pre-sum cap convention
- [ ] **TESTS**: not unit-testable in a shader; note the chosen cap value inline so the invariant is documented
