# PERF-D1-NEW-02: Per-frame process-environment lookups in two hot-path sites (PARTIAL)

**Issue**: #1802
**Labels**: low,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-02)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D1-NEW-02)

## Location
`byroredux/src/render/mod.rs:333` (`BYRO_PROFILE`), `byroredux/src/render/static_meshes.rs:138` (`BYRO_NO_CULL`)

## Description
Two live per-frame sites call `std::env::var_os(...)` instead of caching once, unlike the sibling `apply_fog_overrides` (`render/mod.rs:52-71`) which caches via `OnceLock` "so the hot path doesn't `getenv` per frame." Both are `var_os(...).is_some()` — no heap allocation. Note: the third originally-cited site (`render/mod.rs:57`) is inside `apply_fog_overrides` and already `OnceLock`-cached — not a violation; narrower than an earlier framing of this finding. Env vars cannot change mid-process, so caching both is semantics-preserving.

## Evidence
Direct read of all three cited sites; only the two listed above are live per-frame `env::var_os` calls outside a cache.

## Impact
Sub-µs each; consistency/hardening more than a measurable bottleneck. No quantitative guard exists for these sites.

## Related
`apply_fog_overrides`'s `OnceLock` pattern.

## Suggested Fix
Hoist both into a `OnceLock`, mirroring `apply_fog_overrides`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

