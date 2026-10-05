# #5263: CONC-D3-2026-10-05-01: The `vulkan-validation` lane is red on WARN-level `OutputNotConsumed` performance warnings since #4987's fix, with zero validation errors

**Labels**: medium,concurrency,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5263

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-05.md` — `CONC-D3-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM. This is the same grading as #4987 and #4603: a permanently red gate masks the next real `VUID-*` /
  `SYNC-HAZARD-*`.
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `.github/workflows/ci.yml:398` (`export RUST_LOG=error,byroredux_renderer=info`) and `:413` (the `[Vulkan]` grep).
  The warnings come from main-pipeline creation, logged right after "Graphics pipelines created (opaque early tests=true …)".
- **Status**: NEW. Searched "OutputNotConsumed" and "vulkan-validation lane"; the only hits are the closed #4596, #4987, #5062 and #5064.
  The skill's own Dim 3 text warns that WARN-level lines can redden the lane, but no issue tracks it.
- **Verification Path**: CI log (captured).
- **Description**: To let the positive `Selected GPU:` gate see `device.rs`'s info line, `6d5d8fa5f` raised the renderer crate from
  `error` to `info`. The debug messenger logs performance warnings at WARN with the `[Vulkan]` prefix. The gate's
  `grep -qF '[Vulkan]'` does not distinguish severities, so the 9–10 `WARNING-Shader-OutputNotConsumed` lines on every run fail the
  job. They report vertex outputs at locations 6, 7, 10–13, 15, 16, 20 and 21 that the fragment stage of the early-test variant does
  not consume.
- **Evidence**: The `vulkan-validation` job across main runs:

  | Runs | Commits | Result |
  |---|---|---|
  | 36712814522 … 36789466327 | `c9254beb8` … `01a85cfc4` (09-30) | **success**, zero messages |
  | 36908954333 | `4ad847a81` (first run containing `6d5d8fa5f`) | failure: 9 × OutputNotConsumed, 0 VUID, 0 SYNC |
  | every later run through 37341506292 | through `23524b446` | failure: 9–10 × OutputNotConsumed, 0 VUID, 0 SYNC, no `panicked at` |

  `6d5d8fa5f` is not an ancestor of `01a85cfc4`, and it is an ancestor of `4ad847a81`.
- **Trigger Conditions**: Every CI run since 10-01.
- **Impact**: For five days the lane's red status has carried no information. A reviewer must open each log and read which severity
  fired. Combined with CONC-D2-2026-10-05-01, the lane is now both red on a non-error and blind to RT.
- **Related**: #4987, #4603, #4596, CONC-D2-2026-10-05-01.
- **Suggested Fix**:
  - Scope the info lift to the device module only: `RUST_LOG=error,byroredux_renderer::vulkan::device=info`.
  - Or make the gate match error-severity messenger lines only. Pin whichever you choose next to
    `vulkan_validation_job_requires_a_selected_device`.
  - Separately, and optionally, trim the unconsumed varyings or accept them explicitly.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
