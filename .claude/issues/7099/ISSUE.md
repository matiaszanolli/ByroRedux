# #7099 — ESM-D1-01: `esm_dim8_coverage` still hand-rolls an unbounded recursive GRUP walk (regression of #5819)

Labels: low,esm-plugin,tech-debt,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D1-01)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D1-01**

- **Severity**: LOW (dev-only example, no engine reach)
- **Dimension**: 1, universal GRUP bound
- **Record / Sub-record**: n/a (GRUP)
- **Location**: `crates/plugin/examples/esm_dim8_coverage.rs:128-153` (`fn walk`, raw `group_content_end` at `:139`; the top-level loop at `:77`)
- **Status**: Regression of #5819. The issue named this file among the 22 examples to migrate; 21 were moved to `EsmReader::for_each_record` and this one was left. The 2026-11-08 report found the same defect, but no issue was filed from it, and the file is byte-unchanged.
- **Description**: `walk` recurses as `walk(reader, inner_end, census)` with no depth parameter. Its only guard is `inner_end > end`, a parent clamp reported as an error, so how far is covered but how deep is not. About 100k nested minimal GRUPs overflow the native stack.
- **Evidence**: A grep for `read_group_header` / `is_group()` over all 80-odd examples and `crates/*/tests` that lack `for_each_record` returns exactly two files. `sf_smoke.rs` is clean: it uses an explicit `stack` Vec, so heap-bounded, no recursion. `esm_dim8_coverage.rs` is the other. None of the six examples added since 11-08 (`info_result_census`, `menu_gmst_census`, `music_census`, `perk_body_census`, `spell_cast_census`, `stealth_gmst_census`) or the two new tests (`bow_real_data`, `package_schedule_census`) hand-rolls a walk.
- **Impact**: A census run over a hostile ESM can abort the process. No engine effect.
- **Suggested fix**: Replace `walk` with `reader.for_each_record(end, None, ..)`; the per-record `pairs` / `cur_top` bookkeeping fits the callback. Alternatively thread a `depth` into `walk` and gate it on `MAX_GRUP_NESTING_DEPTH`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
