# #2115: D9-01: CPU/GPU per-phase breakdown strings are format!-built every frame, not gated

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2115
**Labels**: bug, low, performance

---

**Severity**: low
**Dimension**: Telemetry & Origin Cost
**Location**: `byroredux/src/systems/debug.rs:101-109` (`log_stats_system`), helpers `gpu_breakdown` (:55-64) / `cpu_breakdown` (:74-83)
**Status**: NEW

## Description
`log_stats_system` builds both the `gpu` and `cpu` breakdown `String`s unconditionally every frame via `.map(|cov| gpu_breakdown(&cov))` / `.map(|t| cpu_breakdown(&t))`, but the results are only read in two places further down: the slow-frame `log::warn!` branch (gated on `stats.frame_time_ms > SLOW_FRAME_WARN_MS`) and the once-per-second `crosses_one_second_boundary` block. On the overwhelming majority of frames neither condition is true, so both `format!`-built Strings (each formatting 10-12 fields) are allocated and immediately dropped — ~2 discarded allocations/frame, ~120/s at 60 fps. This is erosion of the intended once-a-second gating the surrounding code comments describe.

## Evidence
```rust
// debug.rs — built unconditionally at the top of the system
let gpu = world.try_resource::<SkinCoverageStats>().map(|cov| gpu_breakdown(&cov));
let cpu = world.try_resource::<CpuFrameTimings>().map(|t| cpu_breakdown(&t));
...
// only consumed here, further down:
if stats.frame_time_ms > SLOW_FRAME_WARN_MS && stats.frame_index() > 3 { /* uses gpu, cpu */ }
if crosses_one_second_boundary(total, dt) { /* uses gpu, cpu */ }
```

## Impact
Minor — ~2 discarded `String` allocations/frame. Not hot-path-critical, but is a real deviation from the "once a second" telemetry-gating intent stated in the surrounding comments.

## Suggested Fix
Compute `let want = stats.frame_time_ms > SLOW_FRAME_WARN_MS && stats.frame_index() > 3 || crosses_one_second_boundary(total, dt);` first, then build `gpu`/`cpu` only `if want`.

## Completeness Checks
- [ ] **TESTS**: A regression test or dhat bound pins the gated-allocation behavior

