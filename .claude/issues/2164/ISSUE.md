Bundle issue for the 13 LOW findings from `docs/audits/AUDIT_RENDERER_2026-07-25.md`, following this repo's established convention for renderer-audit LOW tiers (see #926, #982). None individually rise to user-visible bugs; tracked together for opportunistic cleanup. All were re-verified against current code during publish (grep/read-confirmed); none are duplicates of any open issue.

Twelve of these are pure documentation/checklist/comment drift with zero runtime impact. **`L-5` is the one real (non-doc) code gap** — a genuine RAII-guard omission with a bounded but real leak path — verified independently: `crates/renderer/src/vulkan/scene_buffer/upload.rs::upload_terrain_tiles` creates its staging buffer, then runs three fallible (`?`-propagating) steps (`allocator.allocate`, `bind_buffer_memory`, `mapped_slice_mut`) before the unconditional `destroy_buffer`/`free` teardown at the bottom of the function — with no `StagingGuard` (the crate's own RAII type, `crates/renderer/src/vulkan/buffer.rs:302`) in between, unlike every other staging call site in the crate.

Two items (`L-8`, `L-9`) are flagged in the source report as **latent hazards** — no bug today, but a plausible future change would silently reintroduce a real bug current tests can't catch.

---

## L-1 — AS-eviction telemetry references a `mem.stats` command that doesn't exist
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`missing_skinned_blas`/`missing_rigid_blas`/`missing_ssbo_instance`, `build_tlas`)

The three cause-counters increment correctly and feed a genuinely useful rate-limited (`log::warn!`, once/sec) diagnostic. But no `mem.stats` console command exists (only `stats`, `mem.frag`, `ctx.scratch`, `sys.accesses`, `entities`, `systems`, `help` are registered), and none of those read the three counters. **Fix**: add the counters to a persistent resource surfaced by an existing command, or correct the doc/log text to describe the actual rate-limited-log mechanism.

## L-2 — `shrink_tlas_scratch_to_fit`'s documented call site ("cell-unload") contradicts both the code and `memory-budget.md`
**Location**: `crates/renderer/src/vulkan/context/draw.rs` (call site, end of `draw_frame`) vs. `.claude/commands/audit-renderer/SKILL.md`'s Dimension-1 checklist

Verified: `shrink_tlas_scratch_to_fit` is called only from `context/draw.rs:2466` (end of `draw_frame`), never from `cell_loader/unload.rs` (which calls the *different* `shrink_blas_scratch_to_fit` instead, confirmed at `cell_loader/unload.rs:141` and `context/resize.rs:59`). The checklist says it runs "at cell-unload (#1226)" — code matches `docs/engine/memory-budget.md`'s own corrected wording (#1911/REN-D1-01); only the SKILL.md checklist text is stale. **Fix**: update the SKILL.md Dimension-1 bullet to match `memory-budget.md`.

## L-3 — `GpuInstance` offset 92 is documented/named as padding but is live per-draw optical IOR data
**Location**: `docs/engine/shader-pipeline.md` (`GpuInstance` table, offset-92 row); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`_pad_id0`); `crates/renderer/src/vulkan/context/draw.rs` (`_pad_id0: draw_cmd.ior`); `crates/renderer/shaders/caustic_splat.comp` (reads it as `ior`); `include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert` (all still say `_padId0`)

Verified: `_pad_id0`/`_padId0` appears at all 5 mirror sites (`gpu_types.rs:105`, `draw.rs:1647`, `bindings.glsl:36`, `triangle.vert:48`, `ui.vert:24`, `water.vert:53`), byte-pinned at offset 92 (`gpu_instance_layout_tests.rs:80`), and is genuinely consumed as `ior` by `caustic_splat.comp` — real, load-bearing data at all sites but named as padding at 4 of 5. `shader-pipeline.md` lists offset 92 as "(padding)". **Fix**: rename `_pad_id0`/`_padId0` → `ior` at all 5 sites, and update `shader-pipeline.md`'s table row.

## L-4 — Audit checklist's "13 `DBG_*` bits" count is stale — the shared catalog now has 23
**Location**: `crates/renderer/src/shader_constants_data.rs` (`DBG_BITS` catalog) vs. `/audit-renderer` SKILL.md Dimension 3

The catalog (single source of truth, hash `8eaade44`) now lists 23 entries spanning to `0x400000` — 10 Session-49-era ReSTIR/SVGF/FSR additions post-date the "13" figure. The catalog mechanism itself (value-pinning + no-redeclare guard) is correctly fixed post-#1860 and test-covered. **Fix**: update the checklist bullet to "currently 23, `0x1`…`0x400000`".

## L-5 (real code gap) — `SceneBuffers::upload_terrain_tiles` staging buffer isn't RAII-guarded
**Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs`, fn `upload_terrain_tiles`

See summary above — independently re-verified during publish. Bounded impact (cell-transition-only frequency, requires an allocator OOM/bind failure to trigger) but a real, mechanical leak. **Fix**: construct a `StagingGuard` immediately after the successful `allocate` + `bind`, and call `guard.destroy()` at the end, matching every other staging call site in the crate.

## L-6 — Material dedup telemetry doc/log comments reference a nonexistent `mem` command
**Location**: `crates/renderer/src/vulkan/material.rs` (`overflow_count`/`collision_count` doc comments, `INTERN_OVERFLOW_WARNED` log text), `byroredux/src/main.rs` (debug-assert message)

The real operator-facing surface is the **`ctx.scratch`** console command (`commands/world_info.rs`), which does correctly print `materials: N unique / M interned (X× dedup)` plus an `OVERFLOW n → id 0` suffix. Only `mem.frag` and `ctx.scratch` are registered; there is no bare `mem` command. **Fix**: update the doc comments/log text to say `ctx.scratch`, not `mem`.

## L-7 — TAA checklist describes a retired mechanism (#1497's progressive alpha floor), not the current one
**Location**: `crates/renderer/src/vulkan/taa.rs` (`upload_params`), `crates/renderer/shaders/taa.comp`

A later refactor (`e5d02f83`) deleted the `static_frames`-driven per-pixel alpha floor and hardcoded `let alpha = 0.1;`, replacing the mechanism with a per-pixel octahedral-normal surface-consistency disocclusion test (`dot(currNormal, prevNormal) < 0.85`). The original #1497 hazard cannot recur; the replacement is test-pinned (`taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`). **Fix**: update the Dimension 13 checklist bullet to describe the current flat-α + normal-validated-disocclusion design.

## L-8 (latent hazard) — stale comment invites a future RT hit-position bug via `GpuInstance.model`
**Location**: `crates/renderer/src/vulkan/context/draw.rs`, frustum-culled-instance comment above the `GpuInstance` push in `draw_frame`

Since the render-origin rebase work, `GpuInstance.model` is render-origin-**relative** while the TLAS it's paired with is absolute. The comment above the push still promises RT hit shaders "the right material / **transform** (#516)". Today the only RT reader of `.model` (`raytrace.glsl::getHitTriNormal`) is translation-invariant, so nothing is wrong yet — but the comment invites a future RT hit-position reconstruction that would land `renderOrigin` (up to ~176k units on MarkarthWorld) away from the true hit. **Fix**: amend the comment to state rotation/scale is valid for RT but translation is origin-relative.

## L-9 (latent hazard) — no regression-guard test for the `fragWorldPosRel` render-origin convention
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (absent sibling); `crates/renderer/shaders/triangle.vert`/`triangle.frag`

Verified: `grep -rln fragWorldPosRel --include=*.rs .` returns zero hits repo-wide, while the varying itself is used at 6 sites in `triangle.vert`/`triangle.frag`. Unlike the sibling #1486 convention (which has a static source-check test, `triangle_vert_skinned_branch_rebases_render_origin`), the #1496 split (relative varying, `main()`-top reconstruction to absolute `fragWorldPos`, four derivative consumers) is enforced only by shader comments. A refactor that renames the varying or switches a derivative consumer to the absolute local compiles clean and passes all 428 renderer tests. **Fix**: add a static source-check test mirroring #1486's, asserting `triangle.vert` contains `fragWorldPosRel = worldPos.xyz`, `triangle.frag` contains `fragWorldPosRel + renderOrigin.xyz`, and the four known derivative call sites spell `fragWorldPosRel`.

## L-10 — `GpuCamera.render_origin.w` documentation is inconsistent — some sites say "unused", but it carries the FSR-reset-flag payload
**Location**: `crates/renderer/src/vulkan/context/draw.rs` (`render_origin:` field comment), `crates/renderer/shaders/water.vert`, `crates/renderer/shaders/cluster_cull.comp`, `docs/engine/shader-pipeline.md` ("Coordinate Spaces & Precision")

`draw.rs` uploads the FSR-reset-pending flag into `render_origin.w`, and `triangle.frag` reads it in the FSR-reset debug view — `gpu_types.rs` and `triangle.vert` document this correctly, but `draw.rs`'s own comment and `water.vert`/`cluster_cull.comp` still say "w unused"; `shader-pipeline.md`'s spec section documents only `xyz`. Same class of trap as the already-closed #1928/REN-D10-01 (which covered `VolumetricsParams.render_origin.w`'s `is_exterior` payload) — this is a distinct field/file, not a regression of that fix. **Fix**: fix the `draw.rs` comment and the two shader comments to describe the FSR-reset payload; add `w` to the doc's coordinate-spaces section.

## L-11 — Light-animation checklist cites a test name that was consolidated/renamed
**Location**: `crates/core/src/ecs/components/light.rs` (test at `light_anim.rs:236`, formerly `fallout4_shadow_spotlight_is_not_slow_pulse`)

The checklist cites `fallout4_shadow_spotlight_is_not_slow_pulse`. The current suite consolidated this into a broader, multi-game test, `shadow_spotlight_bit_never_leaks_into_animation_on_any_game`. Coverage is provably stronger (any-game, not FO4-only) under a different name. **Fix**: update the checklist's cited test name.

## L-12 — `VERTEX_STRIDE_FLOATS` is 26 (104 B), not the 25/100 B still cited in the checklist and in this project's own `CLAUDE.md`
**Location**: `crates/renderer/src/shader_constants_data.rs` (`VERTEX_STRIDE_FLOATS = 26`, verified); `crates/renderer/src/vertex.rs` (`Vertex`, 104 B after the `[f32;4]` tangent lane, #783/M-NORMALS); project `CLAUDE.md`'s Quick Reference (`vertex.rs` line still says "9 attribute descriptions, 100 B (19 f32 + 4 u32 + 8 u8)")

Verified: `shader_constants_data.rs` confirms `VERTEX_STRIDE_FLOATS = 26`; `CLAUDE.md` still says "100 B". The stride constant and every consumer are cross-pinned by tests plus a byte-identical `.spv` artifact check — only the human-facing docs (audit checklist + `CLAUDE.md`, which is checked into the repo and read every session) are stale. **Fix**: update `CLAUDE.md`'s `vertex.rs` line to "104 B (19 f32 + 4 u32 + tangent `[f32;4]` + 8 u8)" and correct the SKILL.md checklist's "25/100 B" figure. Worth prioritizing since `CLAUDE.md` is read every session, not just an audit-tooling artifact.

## L-13 — Checklist has the #681 skin-buffer usage-flags fix direction backwards
**Location**: `crates/renderer/src/vulkan/skin_compute.rs` (`SkinComputePipeline::create_slot`)

The checklist implies the skinned-output buffer is missing a needed `VERTEX_BUFFER` usage flag. In fact commit `b99ae91e` ("Fix #681 (MEM-2-6): drop unused `VERTEX_BUFFER` from skin_compute output") deliberately *removed* it — M29.3 raster still inline-skins in `triangle.vert`, nothing binds the slot buffer as a VBO. The current flags are correct as-is; only the checklist's framing of #681 is inverted. **Fix**: correct the checklist to state the flag was deliberately *removed*, and note it should only be re-added alongside a Phase-3 raster bind path.

---

## Suggested handling

These are opportunistic cleanup items. `L-5` (real leak-risk) and `L-9`/`L-8` (latent hazards, cheap insurance) are worth pulling forward per the source report's Prioritized Fix Order; the remaining checklist/doc-only items can ride along whenever the relevant files are next touched. Don't carve out a dedicated LOW-bundle PR — it dilutes review attention.

Filed by `/audit-publish` from `docs/audits/AUDIT_RENDERER_2026-07-25.md`.

## Completeness Checks
- [ ] **TESTS**: `L-5` needs a regression test pinning the `StagingGuard` fix; `L-9` needs the new static source-check test described above
- [ ] **SIBLING**: `L-5`'s fix pattern should be spot-checked against the crate's other staging call sites to confirm none share the same gap
