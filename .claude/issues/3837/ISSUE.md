# #3837: PERF-D1-2026-09-05-04: the `frame_lights_scratch` `mem::take` round-trip has four error-path exits that leave the scratch at zero capacity

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-04) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3837 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-04), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs:86` (take) → `crates/renderer/src/vulkan/context/draw.rs:2154` (restore)
- **Status**: NEW
- **Description**: `assemble_camera_and_lights` does
  `let mut frame_lights = std::mem::take(&mut self.frame_lights_scratch);`,
  leaving the field a zero-capacity `Vec` until it's handed back ~2,000 lines
  later. Four `return Err(...)` sites sit between the two:
  `assemble_camera_and_lights.rs:235` (FSR frame-parameter failure) and
  `draw.rs:1945`/`:1997`/`:2036` (`end_command_buffer` / `reset_fences` /
  submit). On any of those the taken `Vec` drops and amortised capacity is
  lost, forcing a 0→`MAX_LIGHTS` regrow next frame — the exact `mem::take`
  capacity-churn pattern this dimension's checklist names, on error paths
  rather than the steady state.
- **Evidence**: `grep -n "return Err" crates/renderer/src/vulkan/context/draw.rs`
  → 1945, 1997, 2036, all between the take (`draw.rs:1724`) and the restore
  (`draw.rs:2154`); plus the `return Err(e)` at
  `assemble_camera_and_lights.rs:235`.
- **Impact**: Bounded and rare — one regrow on the frame after a
  submit/fence/FSR error. The three `draw.rs` sites are exactly the paths
  #910 already hardened for a semaphore leak, so they're known-reachable in
  practice (swapchain churn), not theoretical.
- **Related**: #910 (same three recovery sites), #3694 (`ScratchTelemetry`,
  which reports `frame_lights_scratch` len/capacity and would make this
  observable).
- **Suggested Fix**: Replace the `mem::take` with a split-borrow
  (`let Self { frame_lights_scratch, volumetrics, .. } = self;`) so the field
  is never vacated, removing the invariant instead of documenting it.
- **Confidence**: High on the code shape; impact is intentionally scoped as
  minor since it only fires on already-rare error paths.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
