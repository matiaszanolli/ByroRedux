# #2701: FO4-D2-02: Cycle-truncated BGSM template chains are written into the `TemplateCache`, so a material's resolved chain depends on which material the cell loaded first

- **Severity**: MEDIUM
- **Dimension**: 2 — BGSM template resolution
- **Location**: `crates/bgsm/src/template.rs:200-224`; guard test at `crates/bgsm/src/template.rs:574-594`
- **Status**: NEW
- **Description**: When `resolve_depth` detects a cycle it sets `parent = None` on the detecting node and then unconditionally caches that node under its own key. The truncation is valid only for the `visited` prefix that produced it. A later `resolve()` of the same path from a different root gets a cache hit and receives the truncated chain instead of resolving it fresh.
- **Evidence**: the crate's own `resolve_breaks_three_node_a_b_c_b_cycle` (A→B→C→B) leaves `c.bgsm` cached with `parent: None`. A subsequent `cache.resolve(resolver, "c.bgsm")` — a legal standalone resolve whose correct chain is C→B→(break) — returns depth 1. Which chain a given BGSM receives therefore depends on cell-load order. The test *named* for this hazard, `cycle_break_does_not_pollute_cache_with_partial_chains`, does not test it: it asserts only `Arc::ptr_eq` and `cache.len() == 1` for a pure self-reference, and its own comment concedes *"that leaf already has a valid cache entry, but only if it was discovered via the cycle path."*
- **Impact**: Non-deterministic material authoring for cyclic template chains — a mesh can render with or without its template's envmap/texture contributions depending on load order, invisibly to the test suite. Vanilla's known cycle (`defaulttemplate_wet.bgsm`, #1148) is a pure self-reference and yields the same fields either way; longer cycles are a modded-content exposure.
- **Related**: #1148.
- **Suggested Fix**: Do not cache a node whose `parent` was truncated by cycle detection (thread a truncation flag out of the recursion and skip the insert), or key the entry by path + truncation. Then make the guard test actually resolve `c.bgsm` standalone after the A-rooted walk and assert a non-truncated chain.

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D2-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

