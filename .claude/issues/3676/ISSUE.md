# #3676 — PERF-D9-2026-08-30-03: three per-frame GPU work items sit outside every `gpu_timers` bracket — including `skin_palette.comp`, which a sibling dimension's matrix records as covered

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3676

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2496` (bone_world copy), `:2520` (bind_inverse copies), `:2536-2579` (`skin_palette.comp` dispatch), `:3729-3752` (egui overlay pass). Bracket boundaries: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:379` / `:440`.
- **Status**: NEW
- **Description**: The `skin_dispatch_ms` bracket does **not** cover the bone
  palette pass. `cmd_skin_dispatch_start` / `_end` are written inside
  `record_skinned_blas_refit` (`skinned_blas_refit.rs:379`, `:440`) and wrap only
  the per-entity `skin_vertices.comp` dispatch loop. `record_skinned_blas_refit`
  is called from `draw.rs:2600` — *after* the palette dispatch at `:2536-2579`
  and after the two SSBO transfer copies at `:2496` / `:2520`. All three are
  therefore recorded into the command buffer with no bracket around them. A
  fourth item, the egui overlay render pass at `:3729-3752`, is likewise
  unbracketed and runs on **every** frame (`app_frame.rs:119-126` runs egui
  unconditionally whenever `debug_ui` exists, because it draws the crosshair and
  interaction prompt even with the panel hidden) and includes a queue-locking
  `set_textures` upload. `gpu_timers.rs:9-10` labels slots 0/1 "skin compute
  dispatch loop", which reads as covering the whole skin chain and does not.
- **Evidence**:
  ```rust
  // draw.rs:2536  — palette pass, NO bracket
  if let Some(ref mut skin_palette) = self.skin_palette {
      ...
      skin_palette.dispatch(&self.device, cmd, frame, /* … */);
  }
  // draw.rs:2600  — bracket only starts inside here
  self.record_skinned_blas_refit(cmd, frame, draw_commands, pose_dirty);
  ```
  ```rust
  // skinned_blas_refit.rs:378-380
  if let Some(ref mut timers) = self.gpu_timers {
      timers.cmd_skin_dispatch_start(&self.device, cmd, frame);
  }
  ```
  **Correction to a sibling**: `/tmp/audit/performance/dim_5.md:55` records
  "`skin_palette.comp` + `skin_vertices.comp` → `skin_dispatch_ms` → timer
  present: yes". The second half is right; the first is not.
- **Impact**: The palette pass is the one this matters most for right now:
  sibling Dim 4 (`dim_4.md:230, :275`) reports it dispatches over the **full**
  bone range every frame with only #1811's coarse `skip_skin_gpu_refresh` gate,
  so "the wasted bytes buy wasted GPU threads too". No checked-in instrument can
  size that waste or confirm a fix — the CPU-side `cpu_skin_chain_ms` (#2803)
  measures host work, not the dispatch. The egui overlay is a whole render pass
  on the frame's critical path with no cost visibility at all. The transfer
  copies are the smallest of the four. All four land in `cmd_record` on the CPU
  side, which lumps them with the entire rest of the frame.
- **Related**: Dim 5's `PERF-D5-2026-08-30-02` (`copy_depth_to_history`, the
  fifth unbracketed item — **not re-filed here**, and its claim to be "the only
  per-frame GPU pass with no bracket" is what this finding corrects); Dim 4's
  full-range palette dispatch finding, which this blocks from being measured;
  #1194 (the bracket set's origin).
- **Suggested Fix**: Add one bracket pair (`Q_SKIN_PALETTE_START/END`, raising
  `QUERIES_PER_FRAME` 28 → 30 and adding `BIT_SKIN_PALETTE`) around
  `draw.rs:2536-2579`, and extend it upward to `:2496` if the transfer copies
  should be attributed with it. Bracket egui separately, or accept it as a known
  hole and say so in `gpu_timers.rs`'s module doc. Update the slot table's
  "skin compute dispatch loop" wording either way.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
