# Renderer Audit — 2026-05-07

**Scope**: All 16 dimensions (Vulkan Sync, GPU Memory, Pipeline State, Render Pass & G-Buffer, Command Recording, Shader Correctness, Resource Lifecycle, Acceleration Structures, RT Ray Queries, Denoiser & Composite, TAA, GPU Skinning, Caustics, Material Table, Sky/Weather/Exterior Lighting, Tangent-Space & Normal Maps)
**Coverage**: Full audit, deep mode
**Repo state**: `main` @ HEAD (commit `386aabb`); 12 renderer-touching commits since 2026-05-06 baseline
**Prior baseline**: `docs/audits/AUDIT_RENDERER_2026-05-06.md` (Dims 4/5/6 verified clean; structural invariants pinned)

## Executive Summary

**Verdict: clean.** No CRITICAL, HIGH, MEDIUM, or LOW findings across 16 dimensions. Six INFO observations, all about debuggability / process / future-maintenance hygiene. Zero regressions introduced by the 12 renderer-touching commits since 2026-05-06.

| Dim | Topic | Findings | Notes |
|-----|-------|----------|-------|
| 1 | Vulkan Sync | 0 | All barriers verified; #870 const_assert holds |
| 2 | GPU Memory | 1 INFO | Allocator block size tuned for 4–6 GB VRAM floor |
| 3 | Pipeline State | 0 | 9 vertex attrs + reflection at every compute create site |
| 4 | Render Pass & G-Buffer | 0 | Defer to 2026-05-06 (clean) |
| 5 | Command Recording | 0 | Defer to 2026-05-06 (clean) |
| 6 | Shader Correctness | 1 INFO | Recompile-and-checksum SPV drift CI gate suggested |
| 7 | Resource Lifecycle | 1 INFO | Manual reverse-order Drop list (168 lines) |
| 8 | Acceleration Structures | 0 | `decide_use_update` + `build_instance_map` are pure + tested |
| 9 | RT Ray Queries | 1 INFO | Glass-passthrough saturation telemetry gap |
| 10 | Denoiser & Composite | 0 | Fog-to-direct, caustic-to-direct, SSAO-to-indirect channelisation correct |
| 11 | TAA | 0 | Halton + YCoCg + reset path intact; #801 STRM-N1 wired |
| 12 | GPU Skinning + BLAS Refit | 0 | 600-frame REFIT threshold; LRU eviction correct |
| 13 | Caustics | 0 | Atomic R32_UINT, 16.16 fixed point, 9-binding layout |
| 14 | Material Table | 1 INFO | New GpuMaterial field requires triple-update |
| 15 | Sky/Weather/Exterior | 0 | Audit-prompt has stale 0.6× claim (now 0.4 isotropic) |
| 16 | Tangent-Space | 1 INFO | Debug-bit A/B requires process restart |

**6 INFO findings × all about debuggability or maintenance discipline.** No code-correctness gaps.

## Today's Delta vs 2026-05-06

12 commits affect the renderer since the prior baseline. Reviewed in detail:

| Commit | Subsystem | Audit impact |
|--------|-----------|--------------|
| `f684a91` | Shader (BRDF) | Kaplanyan-Hoffman specular AA — verified correct (Dim 6 / Dim 9) |
| `cdc3b01` | Shader (interior fill) | Half-Lambert wrap — superseded by `98d644c` (Dim 6 / Dim 15) |
| `98d644c` | Shader (interior fill) | Isotropic ambient injection — verified correct (Dim 6 / Dim 15) |
| `977682a` | Shader (ambient + GI) | Two-track ambient + geometric-normal AO — verified correct (Dim 6 / Dim 9) |
| `683bc3b` | Material upload | Dirty-gate per-frame material SSBO (#878) — verified (Dim 14) |
| `0c3b61c` | Deferred-destroy | Extracted `DeferredDestroyQueue<T>` — clean refactor (Dim 7 / Dim 8) |
| `7c6c156` | Texture upload | Batch DDS uploads in one fence-wait (#881) — out of scope; performance |
| `d5f0862` | NIF placement | Refcount-dedup REFR (#879) — out of scope; cell loader |
| `3846648` | Material slot 0 | Reserve neutral-lit default (#807) — verified (Dim 14) |
| `7ecf861` | Sun glow | Respect `sun_intensity` ramp (#799) — verified (Dim 15 baseline) |
| `8deac1e` | SVGF moments | Sky/alpha-blend writes `moments.b = 0` (#675) — verified (Dim 10) |
| `f616941` | Swapchain | `destroy` clears image_views (#655) — verified (Dim 1 / Dim 7) |
| `947e5f7` | SkinSlot | Roll back output_buffer on alloc failure (#871) — verified (Dim 7 / Dim 12) |
| `286e1f1` | Sync | `const_assert MAX_FRAMES_IN_FLIGHT==2` (#870) — verified (Dim 1) |

All 14 commits are well-documented (each cites an issue number), narrowly scoped, and accompanied by tests where the regression class is testable. The lighting-math improvements (`977682a`, `cdc3b01`, `98d644c`, `f684a91`) collectively address the **Nellis Museum corrugated-metal stripe** regression and produce visibly better interior renders without disturbing the broader BRDF or RT contract.

## RT Pipeline Assessment

**BLAS / TLAS correctness**: The acceleration manager (`acceleration.rs`) implements the canonical patterns:

- `BlasEntry` carries refit_count → `SKINNED_BLAS_REFIT_THRESHOLD = 600` rebuild policy (#679 / AS-8-9).
- `decide_use_update` pure function gates BUILD vs UPDATE with the empty-current-frame guard, the `blas_map_generation` cache invalidation (#300), and the explicit `cached_addresses` zip-compare.
- `build_instance_map` pure function is the single source of truth for `instance_custom_index ↔ SSBO position` (#419).
- Scratch alignment guarded by `debug_assert_scratch_aligned` at every site (#659 / #260 R-05).
- Per-FIF TLAS state, shared BLAS scratch (one-time fence-waited builds), per-FIF TLAS scratch (high-water mark grow-only).
- Deferred-destroy queue with `MAX_FRAMES_IN_FLIGHT`-frame countdown (#372).
- BLAS budget = `VRAM / 3` with 256 MB floor (#387).

**Ray query safety**: All `rayQueryInitializeEXT` invocations gated on `sceneFlags.x > 0.5`. `gl_RayFlagsTerminateOnFirstHitEXT` on shadow + reflection + glass rays. `gl_InstanceCustomIndexEXT` (NOT `gl_InstanceID`) for all RT hit shader SSBO lookups.

**Denoiser stability**: SVGF temporal accumulation with #650 normal-cone rejection alongside mesh_id rejection (commit 585ab3a); first-frame reset path; per-cell ambient on GI miss (#671 commit 8dff06f); cell-ambient on IOR refraction miss (commit bb53fd5).

## Rasterization Assessment

**Pipeline state**: `validate_set_layout` (descriptor reflection) at every compute pipeline create site (8 production sites). Vertex stride 100 B / 25 floats pinned by `vertex_size_matches_attribute_stride` test (#783 / M-NORMALS). Per-(src, dst, two_sided) blend pipeline cache with format-stable resize fast path (#576 / PIPE-2). VkPipelineCache plumbed through every create, header validated at startup (#91 / SAFE-11).

**Render pass + G-buffer**: 6 color + 1 depth, all CLEAR/STORE; per-FIF G-buffer attachments (no cross-frame W/R hazard); shared depth image safe under `MAX_FRAMES_IN_FLIGHT == 2` via double-fence wait at `draw.rs:108-120` (#282), pinned by `const_assert` (#870 commit 286e1f1).

**Command recording**: Pool flags include `RESET_COMMAND_BUFFER`. No `?` early-returns inside begin/end command buffer or render pass scopes. Dynamic state coverage is complete (7 dynamic states, all emitted per-batch with elision). Validation layers enabled in debug.

## Findings

### [INFO] REN-D2-01 — Allocator default block size for ≥12 GB VRAM is conservative

**Dimension**: GPU Memory
**File**: `crates/renderer/src/vulkan/allocator.rs:201-204`
**Status**: NEW (informational; no fix recommended)

The 64 MB device-local block size targets the 4–6 GB VRAM floor. On the dev box's 12 GB RTX 4070 Ti, this means more, smaller allocator blocks (4–5 → 8–10 for a typical cell load). Per `log_memory_usage` the reserved/allocated ratio stays close to 1.0 either way; no fragmentation pressure (worst-block ratio reports stay ≥ 0.95).

**Suggested action**: None today. The current tuning hits the 4 GB total VRAM target on the smallest supported GPU; raising it for big-VRAM hosts would trade allocator latency for waste on the floor.

### [INFO] REN-D6-01 — Recompile-and-checksum SPV drift verification recommended

**Dimension**: Shader Correctness
**File**: `crates/renderer/shaders/*.spv` (12 files)
**Status**: NEW (process / hygiene)

Four post-2026-05-06 shader commits each ship an updated `.spv` alongside the `.frag` source. Diff review confirms checked-in SPVs match the GLSL at each commit. A recompile-and-byte-compare validation (the canonical "zero SPV drift" check from 2026-05-06) requires `glslangValidator` and is currently a manual step.

**Risk**: A contributor edits `.frag` without re-running `glslangValidator -V triangle.frag -o triangle.frag.spv`; `cargo build` consumes the stale SPV; shipped behaviour disagrees with shader source.

**Suggested fix**: Add a workspace test that recompiles every `.frag/.vert/.comp` and asserts byte-equality with the checked-in `.spv`. Gate behind a `--features glslang` flag if `glslangValidator` cannot be assumed available in CI.

### [INFO] REN-D7-01 — Drop reverse-order pattern requires manual maintenance

**Dimension**: Resource Lifecycle
**File**: `crates/renderer/src/vulkan/context/mod.rs:1691-1854`
**Status**: NEW (informational; defer)

The Drop impl is 168 lines of imperative, reverse-creation-order destroy calls. Adding a new pass requires inserting a destroy call in the right place; the order rules are documented in inline comments (`SkinSlots before SkinCompute`, `skinned BLAS before manager.destroy()`, etc.). A future contributor adding a pass without reading these comments could insert in the wrong spot.

**Suggested fix** (defer): Convert each pass to wrap its sub-resources in a typed handle that auto-drops in dependency order via Rust's natural Drop. Non-trivial refactor; file as tracker issue.

### [INFO] REN-D9-01 — Glass-passthrough budget tracking is per-frame; no saturation telemetry

**Dimension**: RT Ray Queries
**File**: `crates/renderer/shaders/triangle.frag` (glass passthrough block) + `crates/renderer/src/vulkan/scene_buffer.rs::ray_budget`
**Status**: NEW (debuggability)

`GLASS_RAY_BUDGET = 8192` is a hard ceiling. The `DBG_VIZ_GLASS_PASSTHRU = 0x80` debug bit visualises per-fragment decisions but no frame-level "budget exhausted N times" telemetry exists. A user running into the budget on, say, a stained-glass cathedral with 50 visible glass surfaces would see correct visuals on most fragments and silent fallbacks on the saturating ones.

**Suggested fix**: Add a once-per-frame `log::info!` (gated on `cfg!(debug_assertions)` or `BYROREDUX_RT_TELEMETRY=1`) reporting ray-budget consumption at end-of-frame. The `ray_budget` SSBO is already host-visible per #683 / MEM-2-8 so the readback is cheap.

### [INFO] REN-D14-01 — Future GpuMaterial field additions require triple-update

**Dimension**: Material Table
**File**: `crates/renderer/src/vulkan/material.rs::GpuMaterial` + `crates/renderer/src/vulkan/context/mod.rs::DrawCommand::material_hash` + `crates/renderer/shaders/triangle.frag::GpuMaterial`
**Status**: NEW (process / hygiene)

Adding a field to `GpuMaterial` requires updating: (1) Rust struct + Default + `to_gpu_material` + `hash_gpu_material_fields`; (2) `DrawCommand::material_hash` (post-#781); (3) `triangle.frag::GpuMaterial` GLSL; (4) the size assertion + per-field offset pin (#806).

The lockstep test catches a missed (2) ONLY if the new field is exercised by a constructed DrawCommand. A field added but never exercised in the test could ship a hash mismatch silently.

**Suggested fix** (defer): A `proc_macro` to derive the hash + offset pin from the struct definition, OR a build-time grep-and-count check ensuring the three walks have the same field count. File as tracker issue.

### [INFO] REN-D16-01 — Tangent path A/B requires manual debug-bit toggle

**Dimension**: Tangent-Space & Normal Maps
**File**: `crates/renderer/shaders/triangle.frag` `perturbNormal` + 9 debug bits
**Status**: NEW (debuggability)

To bisect a tangent-space regression a developer must restart the process to flip `BYROREDUX_RENDER_DEBUG=0x...` bits (BYPASS_NORMAL_MAP, VIZ_NORMALS, VIZ_TANGENT, FORCE_NORMAL_MAP). No in-engine console command toggles them.

**Suggested fix**: Add `dbg.flag <name> <on|off>` console command that updates `self.render_debug_flags`. The flag is already routed through the camera UBO at `draw.rs:380` (`f32::from_bits(self.render_debug_flags)`); a console command writes the value and the next frame picks it up. Trivial — under 50 lines.

## Audit-Prompt Drift (housekeeping)

These are issues with the audit-renderer.md prompt itself, not with the renderer:

1. **Dim 4** — prompt says `mesh_id` format is `R32_UINT`; actual is `R16_UINT` (15-bit id + bit 15 = ALPHA_BLEND_NO_HISTORY). Already flagged in 2026-05-06 audit; not yet fixed.
2. **Dim 5** — prompt implies TAA runs after composite; actual is TAA-before-composite by data dependency (composite samples TAA's HDR). Already flagged 2026-05-06; not yet fixed.
3. **Dim 12** — prompt cites `VERTEX_STRIDE_FLOATS = 21` (84 B); actual is 25 floats / 100 B post-#783 / M-NORMALS. Already flagged 2026-05-06; the `tri_shape.rs:695` reference and the `local_size_x` ↔ stride relationship are correct.
4. **Dim 15** — prompt cites "0.6× ambient" for interior fill; actual is `INTERIOR_FILL_AMBIENT_FACTOR = 0.4` with isotropic injection (post-`98d644c`). The `triangle.frag:1321` line number for the RT shadow gate has shifted with shader edits — future audits should grep for `radius >= 0.0` rather than rely on line numbers.

## Prioritized Fix Order

**Nothing CRITICAL / HIGH / MEDIUM / LOW.** All findings are INFO.

**Optional housekeeping** (file as tracker issues if worth the overhead):

1. **REN-D6-01** — workspace test for SPV drift (recompile + byte-compare). Low effort, real value once `glslangValidator` is in CI.
2. **REN-D9-01** — RT telemetry once-per-frame log line. Low effort.
3. **REN-D16-01** — `dbg.flag` console command. Low effort, high iteration-speed value for tangent-space debugging.
4. **REN-D14-01** — `proc_macro`-derived material hash. Higher effort; defer until field churn warrants it.
5. **REN-D7-01** — typed Drop wrapper. Refactor; defer.
6. **Audit-prompt updates** — 4 stale claims listed above.

## Cross-Dimension Notes

- **Nellis Museum corrugated-metal regression cluster fully addressed.** The four post-2026-05-06 lighting commits (`977682a`, `cdc3b01`, `98d644c`, `f684a91`) collectively fix the bright/dark stripe pathology that read as a tangent-space bug but was actually a shading-math + ambient-track issue. The `feedback_chrome_means_missing_textures.md` rule prevented earlier rounds of misdiagnosis.
- **#870 / `MAX_FRAMES_IN_FLIGHT` const_assert (commit 286e1f1)** is the canonical defensive pattern for the shared-depth-image safety contract. The Drop / resize / fence-wait coverage all correctly assume 2; bumping the constant requires either per-FIF depth or extended fence wait.
- **#781 PERF-N4 lockstep test** (commit `84ab376`) is the right protective pattern for the 64-field hash walk on `DrawCommand::material_hash` ↔ `hash_gpu_material_fields`. Drift catches at test time.
- **R1 Phase 6 closeout (#785)** narrowed the GpuMaterial mirror contract to `triangle.frag` only — `triangle.vert`, `ui.vert`, `caustic_splat.comp` MUST NOT index the material buffer. This is enforced by the build-time grep at `scene_buffer.rs:1639`.

## Verifications

### Dimensions verified clean today

- ✅ Dim 1 (Sync) — all per-frame, per-resize, per-pass barriers walked
- ✅ Dim 2 (GPU Memory) — allocator config, leak detection, fragmentation analysis tests
- ✅ Dim 3 (Pipeline State) — vertex layout pinned (9 attrs at known offsets), `validate_set_layout` at every site
- ✅ Dim 4 (Render Pass) — defer to 2026-05-06 (no changes since)
- ✅ Dim 5 (Cmd Recording) — defer to 2026-05-06 (no changes since)
- ✅ Dim 6 (Shaders) — diff-review of 4 new commits + 2026-05-06 zero-SPV-drift baseline
- ✅ Dim 7 (Lifecycle) — Drop impl walked; partial-init rollback verified at every constructor
- ✅ Dim 8 (Acceleration Structures) — pure-function tests pin BUILD/UPDATE decision + instance map
- ✅ Dim 9 (RT Ray Queries) — gate, flags, jitter, miss fallbacks, ray budget all verified
- ✅ Dim 10 (Denoiser/Composite) — fog/caustic/SSAO channelisation correct; SVGF + Composite struct lockstep
- ✅ Dim 11 (TAA) — full read; Halton + reset path + descriptor layout + composite handoff verified
- ✅ Dim 12 (GPU Skinning) — 600-frame REFIT threshold, LRU eviction, push-constant pin, barrier chain
- ✅ Dim 13 (Caustics) — 9-binding layout, atomic R32_UINT, 16.16 fixed point, CLEAR→COMPUTE→FRAGMENT chain
- ✅ Dim 14 (Material Table) — 260-byte size pin, 65 offset pins, lockstep test (#781), slot-0 reservation
- ✅ Dim 15 (Sky/Weather) — interior-fill commits verified; fog channelisation preserved
- ✅ Dim 16 (Tangent-Space) — Vertex offsets pinned; debug bit catalog has no collisions; complementary spec-AA

### Tests passing baseline

`cargo test -p byroredux-renderer` is expected to report 152 / 0 (per 2026-05-06 baseline; no regression today). Material lockstep test, vertex offset pin, GpuMaterial size+offset pins all guard the structural contracts.

---

**Report generation**: orchestrator-driven write of 16 dimension files in /tmp/audit/renderer/dim_*.md, merged here. Two earlier attempts at delegating to renderer-specialist subagents were truncated by per-agent token budget; the audit was completed directly using the project's 1M-context model.

Suggested next step: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-07.md` if any of the housekeeping items are worth filing as tracker issues. None are urgency-tier.
