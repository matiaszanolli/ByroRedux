# #4981: REN-D12-2026-09-27-05: Timer and telemetry doc rot bundle

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4981
- **Labels**: low,renderer,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D12-2026-09-27-05**._

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:5-9`, `:2051-2060`, `:302-313`, rows 40-55 of the module table; `docs/engine/renderer.md:280`; `GpuTimerSnapshot::composite_ms` doc (`gpu_timers.rs` ~`:240`)
- **Status**: NEW (items a–d); Existing: #4811 (item e, still open, partially unaddressed)
- **Description**:
  - (a) The module header gives the live count (56 / 28), but its bump history stops at "again from 38/19 by the exposure meter (#4618)". It omits 40/20 → 46/23 (`88c23887b`: ground-cover models, volumetrics inject and integrate) and 46/23 → 56/28 (`0925f7926`: five geometry phases).
  - (b) The doc comment of `prose_outside_the_module_header_carries_no_bracket_counts` still says the header states "(40) … 20 start/end brackets". Its `rotted` list stops at 38/19, so it cannot catch 40/20 or 46/23 rot.
  - (c) `groundcover_models_ms`, `volumetrics_inject_ms`, `volumetrics_integrate_ms` and the five `main_*`/`groundcover_*_draw_ms` fields have no doc comments. Nothing on the struct says the phase fields never reach `SkinCoverageStats`, the bench line or the UI (only the `BYRO_PROFILE` log). Table rows 40-55 also drop the table's alignment and prose style.
  - (d) `renderer.md` step 3 says acquire is "Bracketed by a GPU timer so a FIFO-present block is attributable". No acquire bracket exists; `vkAcquireNextImageKHR` is host-side, and the measurement is the CPU `acquire_ms` in `CpuFrameTimings`.
  - (e) #4811: `composite_ms` still lists "+ bloom" as a composite input. Composite declares `bloomTex` unused since #2796, and bloom runs after composite. The sky-aperture mask is still not mentioned.
- **Impact**: This is the recurring count and doc drift the skill warns about. A reader following `renderer.md` looks for a GPU acquire timer that does not exist.
- **Suggested Fix**: Extend the bump history and the `rotted` patterns (40/20, 46/23), document the new fields and the phase fields' profile-only scope, reword `renderer.md` step 3 to "timed on the CPU (`acquire_ms`)", and close #4811 with the composite doc rewrite.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
