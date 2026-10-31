# ESM-2026-10-31-05: The FURN `KWDA` arm re-spells `kwda_keywords`, which is declared after the test module

Labels: low,esm-plugin,bug,tech-debt

Source: `docs/audits/AUDIT_ESM_2026-10-31.md` (ESM-2026-10-31-05). Severity LOW (duplicate logic / lint). Record: `FURN` / `KWDA`.

**Location**: `crates/plugin/src/esm/records/dispatch_world_placement.rs:42-59`; `crates/plugin/src/esm/records/common.rs:772-780`

## Description
- `kwda_keywords` was added in `ccccbaf03` and is used by FLOR, ACTI and CONT.
- The FURN arm (from earlier `851636a13`) still inlines the same chunk-and-remap loop, using `remap.map_or(id, |m| m.remap(id))` instead of `remap_fid`. It could call the helper and clone the result for the second map.
- `kwda_keywords` itself is declared after `mod tests { ... }` in `common.rs` (line 774 vs the module at 503), tripping clippy `items_after_test_module`. The tree-wide clippy step is already red (#6492).

## Impact
No behaviour difference today (`FormIdRemap::remap` short-circuits 0). Risk is drift: a future fix to the helper (sentinel, game gate) would miss FURN, whose keywords decide what counts as a crafting bench.

## Suggested Fix
Replace the FURN closure body with `let keywords = kwda_keywords(subs, &remap);` and move the helper above `mod tests`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
