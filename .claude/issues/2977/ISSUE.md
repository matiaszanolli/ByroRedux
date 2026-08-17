# TD1-2026-08-16-01: the >2000-LOC set grew 6 → 11; seven newly-crossed files are unfiled and two are pure test files

**Issue**: #2977
**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 1 — File / Function / Module Complexity). Effort: medium (per file).

**Location**: see the table in #2974
**Status note**: NEW for the seven newly-crossed files. `volumetrics.rs` / `material.rs` are #2256 / #2257, `context/mod.rs` is #1749 — not re-filed here.

## Description

Seven files crossed 2000 LOC since the last recorded set and none has an issue. Read against #2974, **only two of them are production complexity worth acting on**; the rest is test growth.

Recording the membership change so the next audit can diff, and proposing split axes only where the axis is real:

### Worth splitting

**`crates/renderer/src/vulkan/acceleration/tests.rs` (2327, pure test)** — created by the Session-35 split of `acceleration.rs` into `{constants, types, predicates, blas_static, blas_skinned, tlas, memory}`, but the tests were *not* split with it.

Split axis: mirror the production module names it already exercises — `blas_static_tests.rs`, `blas_skinned_tests.rs`, `tlas_tests.rs`, `memory_tests.rs`, `predicates_tests.rs`. This is the pattern every other refactored module in the tree already follows (`crates/nif/src/import/tests/`, `crates/plugin/src/esm/cell/tests/`, `byroredux/src/render/*_tests.rs`).

**`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (2125, pure test)** — same treatment. It now holds shader-source contract tests (`correctness_debug_views_bypass_non_transport_frame_graph_terms`, `gi_zero_budget_is_a_true_no_ray_floor`, `normal_alpha_masks_specular_intensity_not_roughness`) that are **not layout tests at all** and belong beside the shader-constants suite.

### Recorded for the diff only — no production split warranted

| File | Production LOC |
|---|---|
| `crates/renderer/src/vulkan/svgf.rs` | 1700 |
| `crates/plugin/src/esm/records/misc/world.rs` | 1202 |
| `crates/physics/src/world.rs` | 1017 |
| `byroredux/src/env_translate.rs` | 869 |
| `crates/renderer/src/texture_registry.rs` | 838 |

Two files present in the SKILL's orientation set have fallen back under threshold: *`byroredux/src/save_io.rs`* and *`byroredux/src/asset_provider/tests/`*.

## Evidence

```bash
find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```

Production halves measured at the first `#[cfg(test)]`. Re-verified 2026-08-16.

## Impact

Low on its own. The signal is that the **production** side of the set has been stable at two files for several sweeps while the test side grew by five — worth knowing before anyone budgets a "split the big files" pass.

## Suggested Fix

File and land the two test-file splits. Leave the five majority-test production files alone.

## Related

- #2974 (the recipe defect that makes this set read as worse than it is)
- #2256, #2257, #1749 (the three already-filed members)

## Completeness Checks
- [ ] **SIBLING**: The split mirrors the production module names, matching the house pattern
- [ ] **NO-BEHAVIOUR-CHANGE**: Test splits move code without altering assertions or coverage
- [ ] **SCOPE**: The five majority-test files deliberately left alone, per the finding
- [ ] **TESTS**: Full crate suite green after each split (`cargo test -p byroredux-renderer`)

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2977 --json state` when live state is needed.*
