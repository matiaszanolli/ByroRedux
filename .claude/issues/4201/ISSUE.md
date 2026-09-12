# PERF-D4-2026-09-11-03: Material interning re-hashes ~110 fields per draw per frame even on frames where every upload gate skips

Labels: low,performance,renderer,bug

**Description**: `intern_by_hash` itself is correctly O(1)/O(unique materials); producing its *key* is not — `material_hash` walks ~110 fields with `FxHasher::write_u32` per `DrawCommand`, every frame, ~800K hash steps (~0.2 ms) at the MedTek workload's 7359 draws. A cache on the `Material` component is not safe as-is: the hash covers animated lanes (shader color/float, alpha, UV offset) that legitimately change per frame with no existing invalidation signal, so a naive cache risks silent mis-dedup (a correctness hazard worse than the cost it would save).

**Evidence**:
`crates/renderer/src/vulkan/context/mod.rs:596-745` (`material_hash`).

**Impact**: ~0.2 ms/frame estimated CPU cost at the MedTek workload; instruction-count estimate, not measured.

**Related**: None named.

**Suggested Fix**: Only if profiling shows this on critical path: split static vs. animated lanes, caching the static half behind the existing `resolve_pbr` import hook.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
