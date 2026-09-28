# #4980: REN-D12-2026-09-27-04: `GeometryTimerPhase` hard-codes query slot 46 and active bit 23 with no range or uniqueness assertion — the next named bracket's natural slot collides with MainOpaque

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4980
- **Labels**: low,renderer,vulkan,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D12-2026-09-27-04**._

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:421-437` (`GeometryTimerPhase::query_start` = `46 + 2*n`, `active_bit` = `1 << (23 + n)`), next to the named `Q_*` constants (last `Q_VOLUMETRICS_INTEGRATE_END = 45`) and `BIT_*` (last `BIT_VOLUMETRICS_INTEGRATE = 1 << 22`)
- **Status**: NEW (`0925f7926`)
- **Description**:
  - The named bracket constants run contiguously to slot 45 and bit 22. The five geometry phases occupy slots 46-55 and bits 23-27 through two literals inside `impl GeometryTimerPhase`.
  - The next bracket added the way every previous one was added (`Q_X_START = 46`, `BIT_X = 1 << 23`, `QUERIES_PER_FRAME = 58`, plus a table row) would silently alias MainOpaque's slot and bit.
  - Writing a timestamp twice into one unreset query per frame is VUID-vkCmdWriteTimestamp-None-00830.
  - `doc_table_slot_count_matches_queries_per_frame` only counts table rows. Nothing asserts that `Q_*` and phase slots are unique and `< QUERIES_PER_FRAME`, or that the `BIT_*` and phase bits are disjoint and fit a `u32`.
  - 28 of the 32 `active_bits` bits are now used.
- **Evidence**: `fn query_start(self) -> u32 { 46 + 2 * self as u32 }` and `fn active_bit(self) -> u32 { 1 << (23 + self as u32) }`, with no named base constant.
- **Impact**: This is a latent Validation-visible VUID and corrupted timings on the next bracket bump. The bracket set has grown five times in a month (#4210, SKYAL, #4315, #4618, `88c23887b`), so another bump is likely.
- **Related**: #4541 and #4210 (the same count-rot family).
- **Suggested Fix**: Introduce `Q_GEOMETRY_PHASE_BASE` and `BIT_GEOMETRY_PHASE_BASE`, derived from the last named constant. Add a test (or `const` assert) that collects every START slot (named plus phases), checks they are even, distinct and `+1 < QUERIES_PER_FRAME`, and checks every bit is distinct.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
