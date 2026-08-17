# TD4-2026-08-16-01: Dimension 1's discovery recipe measures total file LOC — a proxy — and 7 of the 11 files it flags today are majority-test

**Issue**: #2974
**Severity**: MEDIUM
**Dimension**: 4 — Audit-Finding Rot
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 4 — Audit-Finding Rot). Effort: small.

**Location**: `.claude/commands/audit-tech-debt/SKILL.md`:70-72 (Phase-1 snapshot), :99-103 (Dim 1 Discovery)

## Description

Dimension 1 exists to find *production* complexity — "an oversized file taxes every edit, review, and merge", with split axes proposed "by responsibility". Its discovery command is:

```bash
find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```

which measures **total file length, test modules included**.

On the current tree that proxy and the property of interest have decoupled: **7 of the 11 files it reports have production halves under 1300 LOC**, and **2 are pure test files with no production code at all**.

The recipe therefore generates findings that the next auditor must individually disprove — and it has already done so. The title of the issue it produced for `material.rs`, #2257, is literally *"crossed 2000 LOC — mostly inline test growth, **no oversized production function**"*. An issue whose own title says it is not the debt the dimension hunts is the recipe reporting a proxy.

## Evidence

Live set with the production/test split (production LOC measured at the first `#[cfg(test)]`), verified 2026-08-16:

| File | Total | Production | Status |
|---|---|---|---|
| `crates/renderer/src/vulkan/context/draw.rs` | 4594 | 4594 | real |
| `crates/renderer/src/vulkan/context/mod.rs` | 4433 | ~4330 | real (#1749) |
| `crates/renderer/src/vulkan/acceleration/tests.rs` | 2327 | **0** | pure test |
| `crates/renderer/src/vulkan/svgf.rs` | 2238 | 1700 | majority-test |
| `crates/renderer/src/vulkan/volumetrics.rs` | 2212 | — | #2256 OPEN |
| `crates/renderer/src/vulkan/material.rs` | 2173 | — | #2257 OPEN |
| `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` | 2125 | **0** | pure test |
| `crates/plugin/src/esm/records/misc/world.rs` | 2116 | 1202 | majority-test |
| `crates/physics/src/world.rs` | 2089 | 1017 | majority-test |
| `byroredux/src/env_translate.rs` | 2081 | 869 | majority-test |
| `crates/renderer/src/texture_registry.rs` | 2021 | 838 | majority-test |

The SKILL's own orientation paragraph names a 6-file set of which two (*`byroredux/src/save_io.rs`*, *`byroredux/src/asset_provider/tests/`*) have since fallen back under the threshold, and seven others have crossed — so the named composition is wrong in **both directions**, not merely numerically stale.

## Impact

Every tech-debt run pays triage cost on ~7 false-positive candidates, and the two genuinely oversized production files (`context/draw.rs`, `context/mod.rs`) are buried in that noise. A dimension that mostly reports its own measurement artifact stops being read.

## Suggested Fix

Split the recipe in two:

1. **Production LOC** (`awk` truncating at the first `#[cfg(test)]` / `mod tests`) against the 2000 threshold — this is the dimension's actual subject.
2. A separate, lower-priority **"test file >2000 LOC"** bucket.

Update the orientation paragraph to quote the production figure.

## Related

- #2257, #2256 (both OPEN, both produced by this recipe)
- #2262 (the same class of recipe defect in the `#[ignore]` count)
- TD1-2026-08-16-01 (the membership change this recipe reported)

## Completeness Checks
- [ ] **SIBLING**: Other dimensions' discovery recipes checked for the same proxy-vs-property gap (Dim 5's marker count, Dim 8's `allow(dead_code)` count)
- [ ] **ORIENTATION**: The SKILL's named 6-file set refreshed to the production figure, not just the command
- [ ] **NO-SILENT-DROP**: The test-file bucket still reports, so splitting the recipe does not hide the two pure-test files
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes after the SKILL edit

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2974 --json state` when live state is needed.*
