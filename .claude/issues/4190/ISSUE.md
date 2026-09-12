# PERF-D1-2026-09-11-02: `scene_trigger_actor_approach_system` still deep-clones every `ScenePlayer`'s unused heap fields every frame

Labels: low,performance,scripting,bug

**Description**: Residual gap in a system already partially fixed by closed issue #3838 ("deep-clones every ScenePlayer into a fresh Vec each frame"). The outer-container reallocation that #3838 targeted is gone (closure-captured scratch now in place), but the per-entity deep clone of `active_actions: Vec<...>` / `completed_actions: HashSet<u32>` remains, and neither is read downstream — only three `Copy` scalar fields are.

**Evidence**:
`byroredux/src/systems/cinematic.rs:452-458` clones the full `ScenePlayer` including its two heap collections despite only three scalar fields being consumed by the three downstream consumer passes.

**Impact**: Per-entity heap allocation (Vec + HashSet clone) every frame for every actor with an active `ScenePlayer`, for data never read. Bounded by concurrently-active scene players, not by scene size.

**Related**: Closed #3838 (fixed the outer-container half of this same system).

**Suggested Fix**: Clone only the three scalar fields actually needed, or iterate the query directly for the three consumer passes instead of building an intermediate cloned collection.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
