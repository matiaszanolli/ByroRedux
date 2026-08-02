# TD2-101: sample_scalar / sample_color in cinematic.rs duplicate the same keyed-linear-interpolation control flow for two value types

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2260

**Dimension**: 2 (Logic Duplication)
**Location**: `crates/scripting/src/cinematic.rs:420-464` (`sample_scalar`, `sample_color`)
**Status**: NEW

**Description**: Commit `4598bc74` ("feat: Enhance image space modifiers and presentation effects", 2026-08-01) added the IMAD (Image Space Modifier) sampling path. Two functions, `sample_scalar(keys: &[ImadScalarKey], time, default) -> f32` and `sample_color(keys: &[ImadColorKey], time, default) -> [f32; 4]`, are structurally identical: same early-return-on-empty, same "before-first-key holds first value" clause, same `keys.windows(2)` scan for the bracketing pair, same width/alpha clamp-and-lerp math, same "after-last-key holds last value" fallback — the only difference is which field of the key struct gets interpolated. `grade_factor` and the per-modifier accumulation loop call both, so this is the load-bearing sampler for every IMAD channel (blur, saturation/brightness/contrast grading, tint/fade color).

**Evidence**: Both functions read (abbreviated): early-return on empty keys, `if time <= first.time return first.value`, `for pair in keys.windows(2) { ... width/alpha clamp-lerp ... }`, `keys.last().map_or(default, ...)`. Only `.value` (scalar lerp) vs `.color` (per-channel array lerp via `std::array::from_fn`) differs.

**Impact**: Cosmetic/maintainability only today — both are correct and tested. A future change to the interpolation contract (e.g. adding cubic/hermite segments, or fixing the degenerate-key `width.abs() <= f32::EPSILON` edge case) has to be applied at both sites by hand, with nothing enforcing they stay in lockstep.

**Suggested Fix**: Extract a single generic helper, e.g. `fn sample_keyed<T, V>(keys: &[T], time: f32, default: V, time_of: impl Fn(&T) -> f32, value_of: impl Fn(&T) -> V, lerp: impl Fn(V, V, f32) -> V) -> V`, or give `ImadScalarKey`/`ImadColorKey` a shared `time` accessor and a `Lerp` trait (`fn lerp(self, other: Self, t: f32) -> Self`) implemented for `f32` and `[f32; 4]`, then have one generic function serve both call sites. Collapses ~45 duplicated lines to ~20.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
