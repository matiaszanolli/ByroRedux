# #2706: SF-2026-08-12-D3-02 - Three doc comments cite a `MaterialProvider::sf_cdbs` `Arc` cache that does not exist, and the claim actively contradicts the code

- **Severity**: LOW
- **Dimension**: 3 — CDB material database
- **Location**: `byroredux/src/asset_provider/material.rs:280`, `:311`, `byroredux/src/app_step.rs:450`
- **Status**: NEW
- **Description**: `csg_cache`'s field doc says it "mirrors the `sf_cdbs` `Arc` hold";
  `geometry_csg`'s doc repeats "mirrors the `sf_cdbs` `Arc` caching"; `app_step.rs`'s
  caching design note lists "`MaterialProvider::sf_cdbs`" among the caches discarded on
  rebuild. `grep -rn sf_cdbs byroredux/src/` returns only those three doc hits — the
  field was replaced by `sf_cdb_count: usize` and no CDB bytes are cached anywhere.
- **Impact**: Documentation-only, but the false claim is load-bearing in the wrong
  direction: a reader auditing provider-rebuild cost would conclude the CDB is already
  `Arc`-cached and stop, which is exactly how SF-2026-08-12-D3-01 stayed unnoticed.
- **Suggested Fix**: Reword all three to reference the real `csg_cache` precedent, or
  land the cache and make the comments true.

---

---
**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D3-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

