# #3674 — PERF-D9-2026-08-30-01: `between_frames_ms` is sampled after `draw_frame` returns, so it silently absorbs the entire in-engine render path it exists to exclude

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3674

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/app_frame.rs:593-596` (sample point), `:643` (stamp), `:55` (the correct anchor); doc at `crates/core/src/ecs/resources/mod.rs:731-739`
- **Status**: NEW
- **Description**: `CpuFrameTimings::between_frames_ms` is documented as *"Wall
  time between the END of one frame and the START of the next … If `acquire_ms`
  is small but this is large, the bottleneck is **outside** the engine's render
  path (compositor, OS, ECS systems running between frames)."* The code does not
  measure that. `last_redraw_end` is stamped at the end of `render_one_frame`
  (`:643`), but `elapsed()` is read at `:593` — inside the `Ok(needs_recreate)`
  arm, i.e. **after** `build_render_data` and after the whole `draw_frame` call
  have already run. The reading is therefore
  `true_gap + atw_pre + atw_scheduler + (part of atw_post) + rof_pre_draw + rof_draw_call`,
  not the gap. `render_one_frame`'s own start anchor, `rof_pre_t0 =
  Instant::now()` (`:55`), is the value the metric wants and is already in scope.
- **Evidence**:
  ```rust
  // app_frame.rs:55  — true start of the frame
  let rof_pre_t0 = Instant::now();
  ...
  // app_frame.rs:593 — sampled here, AFTER draw_frame returned
  cpu_t.between_frames_ms = self
      .last_redraw_end
      .map(|t| t.elapsed().as_nanos() as f32 * NS_TO_MS)
      .unwrap_or(0.0);
  ...
  // app_frame.rs:634 — rof_draw_call bracket closes only afterwards
  rof_draw_call_ns = rof_draw_call_t0.elapsed().as_nanos() as u64;
  // app_frame.rs:643 — stamp for the NEXT frame
  self.last_redraw_end = Some(Instant::now());
  ```
  `crates/core/src/ecs/resources/mod.rs:759-771` independently confirms the
  overlap: `rof_pre_draw_ms` and `rof_draw_call_ms` cover exactly the span the
  sample point sits after.
- **Impact**: The metric systematically over-attributes to "outside the engine"
  by the full magnitude of `rof_pre_draw + rof_draw_call` — i.e. by the two
  buckets that hold `build_render_data`, the SSBO build, command recording and
  present. It is the field an operator consults to decide *"is this a compositor
  problem or my problem?"*, and it answers "compositor" for engine-side cost.
  Blast radius is the egui Metrics panel (`metrics.rs:213`), which is the only
  surface that prints it, plus the Phase-9 "501 ms `between_frames` gap"
  conclusion the code comment at `app_events.rs:1106` still cites — that gap was
  measured with this same skew. This is diagnostic-only (no runtime behaviour
  changes), which is why it is MEDIUM rather than HIGH, but it is the same class
  of defect as #2171 (a trace that argued the opposite of the truth).
- **Related**: `PERF-D9-2026-08-30-04` (the same field is also unprinted on the
  console line); #2171 (origin-delta printed after the overwrite); the Phase-9
  / Phase-10 / Phase-15 bracket lineage.
- **Suggested Fix**: Capture the gap next to `rof_pre_t0` at `app_frame.rs:55`
  (`let between_frames_ns = self.last_redraw_end.map(|t| t.elapsed().as_nanos() as u64).unwrap_or(0);`)
  and assign that at `:593` instead of re-reading `elapsed()`. One line moved;
  the `last_redraw_end` stamp at `:643` is already correct.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
