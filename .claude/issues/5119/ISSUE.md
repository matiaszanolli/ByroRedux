# REG-2026-09-29-03: Two verified fixes have no regression guard (#4607 hot-path hashing, #1042 bare version literals)

**Labels**: low,performance,nif-parser,nif,test-gap,bug

**Source report**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Guard coverage (hardening gap)
- **Location**: `byroredux/src/render/groundcover.rs` (#4607); `crates/nif/src/blocks/` and `crates/nif/src/version.rs` `bsver` (#1042)
- **Status**: NEW (PARTIAL results for #4607 and #1042)
- **Description**:
  - **#4607**: `render/groundcover.rs` is FxHash end to end. The only std `HashSet` is at `:1013`, inside `#[cfg(test)]`. But unlike `skin_offsets`, `light_history`, `SkinSlotPool` and `rigid_motion_history`, no `*_not_siphash` / `does_not_use_siphash` source-scan test covers the file. The fix commit's only CI edit (`c010c9fb9`) was the unrelated #4603 lock-order lane.
  - **#1042**: the sweep holds. There are 0 bare `NifVersion(0x…)` in non-test `blocks/`, and all 37 bare `bsver <op> N` hits in `crates/nif/src` are comments or strings. No test enforces this.
- **Evidence**: `git grep -n "HashMap\|HashSet" byroredux/src/render/groundcover.rs` finds `:18/89/90/261` (Fx) and `:1013` (test). `git grep -nE "bsver(\(\))?\s*(>=|<=|>|<|==|!=)\s*[0-9]+" crates/nif/src ':!*test*'` returns 37 lines, all in comments or strings.
- **Impact**: A reintroduced std map in the per-frame ground-cover path would break the #2923 hot-path rule, or a new bare BSVER literal would appear, and nothing would fail.
- **Related**: #4607, #2923, #1042, #1336
- **Suggested Fix**: Add a `source_scan::production_text`-based pin for `render/groundcover.rs` next to `skin_offsets_hasher_tests.rs`. Add a NIF-crate source scan that rejects `NifVersion(0x` and bare `bsver` comparisons outside `version.rs`.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/render/groundcover.rs` uses `FxHashMap`/`FxHashSet` (:18/89/90/261) and std `HashSet` only in test code (:1013), but no `*_not_siphash` / `does_not_use_siphash` scan covers it (`skin_offsets_hasher_tests.rs` does not reference it; the one `include_str!("render/groundcover.rs")` in `app_events.rs:1657` is a scratch-shrink scan); no NIF-crate source scan rejects `NifVersion(0x` or bare `bsver` comparisons.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
