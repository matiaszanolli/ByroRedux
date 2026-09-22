# SAFE-D7-2026-09-21-01: Auto-exposure divides the 64-thread log-luminance sum by one thread's sample count

**Labels**: medium, renderer, shaders, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: MEDIUM (correctness in an opt-in mode; the safety audit routes it to renderer Dim 11) · **Dimension**: 7 — GPU-fed data & shader loop bounds
**Location**: `crates/renderer/shaders/exposure_meter.comp`, the `main()` log-luminance reduction (~:57-94). The committed `exposure_meter.comp.spv` is in source/artifact parity at HEAD.
**Status**: NEW (introduced by `c5663fe39`, 2026-09-21)
**Verified against**: HEAD `f97775ca8`

## Description

- Each of the 64 invocations accumulates its own `log_sum` and its own `int count` over its share of the 64×64 = 4,096-sample grid (every 64th index).
- The shared tree reduction sums `log_sum` across all 64 invocations into `shared_log_sum[0]`. Invocation 0 then divides that total by its *own* `count` (about 64), not by the total sample count (about 4,096).
- The result is `avg_luminance = exp2(64 × mean(log2 L))` instead of `exp2(mean(log2 L))`: the geometric mean raised to the 64th power.

## Evidence

```glsl
float log_sum = 0.0;
int count = 0;                                                     // per invocation
for (int idx = int(gl_LocalInvocationID.x); idx < side * side; idx += 64) { … log_sum += log2(max(l, 1.0e-6)); count += 1; }
shared_log_sum[gl_LocalInvocationID.x] = log_sum;                  // then a tree reduction across 64
…
if (gl_LocalInvocationID.x == 0) {
    float samples = max(float(count), 1.0);                        // invocation 0's count only
    float avg_luminance = exp2(shared_log_sum[0] / samples);
```

- At HEAD, CI's shader-parity job reports drift only for `composite.frag.spv`, so the shipped `exposure_meter.comp.spv` matches this source.

## Impact

- Metering is effectively bang-bang: exposure ends up at one limit or the other.
  - Host limits are `MIN_AUTO_EXPOSURE` (1/256) and `MAX_AUTO_EXPOSURE` (16).
  - With zero compensation, a scene geometric-mean luminance below about 0.93 (essentially any linear-HDR interior) pins exposure at `MAX_AUTO_EXPOSURE`. Above about 1.06, it pins at `MIN_AUTO_EXPOSURE`.
  - Only the narrow band between meters to an intermediate value. The correct target is `0.15 / L`.
  - The output stays finite only because of the final clamp to `params.limits`.
- Tests cannot see the bug. The host `auto_exposure(average_luminance, …)` in `crates/renderer/src/vulkan/exposure.rs`, and its tests, take the average as an input and never model the shader's reduction.
- Scope: this affects only `--auto-exposure` and the console command `exposure auto`. Fixed mode is the default (`byroredux/src/cli_args.rs`).

## Related

- REN-D11-2026-09-21-02 (#4590): the same shader adapts each per-frame-in-flight slot from its own two-frame-old value, so the effective time constant is 2τ. That is a separate defect, and the adaptation-history fix belongs there.
- REN-D11-2026-09-21-01 (#4578): AgX clamps linear input before `log2` in the presentation pass that this meter feeds. A separate defect.
- REN-D11-2026-09-21-03 (#4591): the same pass lacks the raw-debug gate.
- REN-D3-2026-09-21-02 (#4585): `MeterParams` ↔ `Params` is missing from the UBO block-size table.
- `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

- Reduce a `shared_count[64]` alongside `shared_log_sum` and divide by the reduced total. Alternatively, divide by the known in-bounds sample total.
- Add a guard that can see the reduction, because the host model cannot. Either a source pin that forbids a per-invocation divisor after the shared reduction, or a GPU readback test on a constant-luminance image that expects `0.15 / L` (clamped), not a limit.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D7-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the other single-workgroup reductions in `crates/renderer/shaders/` checked for a per-invocation divisor after a shared reduction
- [ ] **SIBLING**: `exposure_meter.comp.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a constant-luminance scene meters to `0.15 / L` (clamped), not to a limit
