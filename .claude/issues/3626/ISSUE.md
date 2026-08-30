# #3626 — REN-2026-08-30-D20-01: `depth.stats` runs a full-resolution depth decode inside the frame-blocking exclusive debug system

**Labels**: `low,renderer,performance,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3626 --json state`.

---

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`), `crates/core/src/ecs/components/camera.rs` (`Camera::analyze_depth_field`), `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_finish_readback`)
- **Status**: Open
- **Description**: Both halves of the depth-capture round trip do O(width × height) work on threads that own the frame. `depth_capture_finish_readback` builds `samples: Vec<f32>` by `chunks_exact(4).map(f32::from_le_bytes).collect()` at the top of `draw_frame`, before the swapchain acquire. `DepthStatsCommand::execute` then hands that whole slice to `analyze_depth_field`, which walks every sample and does `codes[band].insert(z.to_bits())` into a per-band `HashSet<u32>` — and it runs inside `DebugDrainSystem`, documented at `crates/debug-server/src/system.rs:1` as "Late-stage exclusive system … Runs after all other systems, with exclusive access to the World", i.e. inside the frame.
- **Evidence**:
  - `depth_capture.rs` readback: `let samples: Vec<f32> = slice[..expected].chunks_exact(4).map(...).collect();` — `expected = width * height * 4`, and `extent = self.frame_extents.render`, so 1920×1080 is 2 073 600 samples / 8.3 MB, 3840×2160 is 8 294 400 / 33.2 MB.
  - `camera.rs` `analyze_depth_field`: `let mut codes: Vec<HashSet<u32>> = vec![HashSet::new(); edges.len() - 1];` then one `insert` per non-background sample.
  - Dispatch route: `crates/debug-server/src/evaluator.rs:430` (`reg.execute(world, expr)`) is called from `eval_request`, driven by `DebugDrainSystem`.
  - The result also *stays* resident: `depth_capture_result` holds the `Vec<f32>` until the next `depth.stats` `take_result()`, and `depth_capture_staging` is never shrunk or freed except at teardown (`ensure_depth_capture_staging` only grows) — so one `depth.stats` pins roughly `2 × w × h × 4` bytes for the process lifetime.
- **Impact**: A single `depth.stats` costs one full-frame hash-set build on the render thread plus a multi-megabyte allocation inside `draw_frame`. The resulting hitch lands in the very `CpuFrameTimings` / metrics surfaces the operator is reading next to it, so the diagnostic perturbs the numbers it sits beside. Diagnostic-only and one-shot per invocation, hence Low.
- **Suggested Fix**: Replace the per-band `HashSet<u32>` with a per-band `Vec<u32>` plus `sort_unstable()` + `dedup()` at the end — identical `distinct_codes`, no hashing, and 4 bytes/sample peak instead of a hash table's load-factor overhead. Optionally give `analyze_depth_field` an explicit sample stride (reported in the output) so a 4K capture can be analysed at a fixed budget.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D20-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
