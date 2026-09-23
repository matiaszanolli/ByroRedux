# #4789: PERF-D8-2026-09-23-01: `volumetrics_ms` cannot attribute its cost: inject and integrate share one TOP_OF_PIPE-started bracket, and no volumetrics state reaches the logs or bench

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D8-2026-09-23-01)

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**:
  - `crates/renderer/src/vulkan/gpu_timers.rs:1007-1040` (START `TOP_OF_PIPE`, END `COMPUTE_SHADER`), `:158-171` (upper-bound doc);
  - `context/post_passes.rs:790-797` (bracket around `dispatch`);
  - `volumetrics.rs:1279-1287` (the first in-bracket barrier, COMPUTE→COMPUTE, waits on the preceding SVGF à-trous + caustic dispatches);
  - `context/build_and_upload_instances.rs:730-737` (controller input `max(main_render, volumetrics)`);
  - `byroredux/src/systems/debug.rs:67-100` (the `gpu_ms` / SLOW FRAME format).
- **Status**: NEW. #2040 (closed) documented the generic TOP_OF_PIPE upper-bound caveat. This finding is the volumetrics-specific attribution gap plus the missing state.
- **Description**:
  - The START timestamp is written before the pass's first barrier. That barrier serialises against the still-draining SVGF/caustic compute, so their tail lands inside `volumetrics_ms`.
  - Inject (ray-query and transport heavy) and integrate (a cheap column march) share one bracket.
  - Neither the once-a-second `gpu_ms:` line, the SLOW FRAME warning nor the bench TSV carries:
    - the adaptive RT tier, and hence `volumetric_light_cap`;
    - the froxel extent;
    - whether transport was armed (`dt > 0`);
    - the fog-volume count.
  - A number like Markarth's `volumetrics=3.0` therefore cannot be split between sun rays, light rays, the transport stencil and absorbed queue drain.
- **Evidence**:
  - Within-config bimodality in the checked-in TSVs: Whiterun FSR-Q 0.537 / 0.536 / 0.824 ms and balanced 0.485 / 0.742 / 0.485 (record `4c9a5b36`); MedTek FSR-Q 1.198 / 1.270 / 0.750.
  - The steps are about 0.25–0.3 ms, consistent with either a tier flip or tail absorption. The telemetry cannot tell which.
- **Impact**: Every volumetrics perf claim, including this report's, rests on cross-scene inference rather than attribution. The ray-budget controller also consumes the inflated value.
- **Related**: #2040, #4618 (open; exposure meter has no bracket), the skill's bench-discipline rule "report the tier".
- **Suggested Fix**:
  - Split into `volumetrics_inject_ms` and `volumetrics_integrate_ms`.
  - Write the START timestamp at `COMPUTE_SHADER` stage (it waits for prior compute to finish that stage), so the SVGF/caustic tail is excluded.
  - Add the RT tier, `volumetric_light_cap`, the froxel extent, `transport_armed` and the fog-volume count to the `gpu_ms` line and as TSV columns.

**Note**: Sibling of open #4618 (PERF-D8-2026-09-21-01, exposure meter has no timer bracket) — same telemetry-attribution class, different pass.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
