# TD3-2026-08-20-01: GpuCamera grew 336 to 352 B; five doc sites still say 336 and one names a test that no longer exists

**Issue**: #3201 — https://github.com/matiaszanolli/ByroRedux/issues/3201
**Severity**: MEDIUM
**Labels**: `medium,renderer,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD3-2026-08-20-01 (Dimension 3 — Stale Documentation & Comments).

**Severity**: MEDIUM · **Effort**: trivial (highest value per minute in that report)
**Status**: NEW — and a **direct regression of the 2026-08-16 report's own "Verified Clean" line** on this exact subject: *"`GpuCamera` 336 B … consistent across every live doc comment and pinned test."*

**Location**
- `docs/engine/shader-pipeline.md:193` (section heading), `:194-212` (field table — **missing a row**), `:380` (descriptor-set table)
- `docs/engine/memory-budget.md:37`
- `docs/engine/renderer.md:269`, `:576-577`
- `crates/renderer/src/vulkan/context/mod.rs:704` (historical claim, needs re-tensing only)

## Description

`8e7582ed` (2026-08-16) appended a `render_debug` `uvec4` to `GpuCamera`, taking it **336 → 352 B**. The struct doc and the pinned test were both updated (`gpu_camera_is_352_bytes`), and `.claude/commands/audit-renderer/SKILL.md:115` was updated — where it explicitly instructs the reader to *"confirm they hold and match `shader-pipeline.md`."* **`shader-pipeline.md` was not updated. Neither were three other docs.**

The tech-debt severity table's promotion trigger — *"Stale `GpuCamera`/`GpuInstance`/`GpuMaterial` size in a doc comment (lockstep-drift bait)"* — applies directly.

Two sites are worse than a wrong number:

1. **`docs/engine/renderer.md:576-577`** cites `gpu_camera_is_336_bytes` — a test name that exists **nowhere in the tree** — and glosses it as *"the live 336-byte `GpuCamera` layout."* A reader who follows the doc's own instruction to check the pin finds nothing, and the parenthetical asserts the wrong number is live.
2. **`shader-pipeline.md`'s field table *ends* at offset 320** (`render_origin`). There is no `| 336 | 16 | render_debug | … |` row. A reader building or auditing a `CameraUBO` mirror from that table produces a 336-byte struct and **silently truncates `render_debug`**.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -rn "fn gpu_camera_is" crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs
132:fn gpu_camera_is_352_bytes() {

$ grep -rn "gpu_camera_is_336_bytes" crates byroredux
(no output)

$ grep -rn "336" docs/engine | grep -i camera
docs/engine/shader-pipeline.md:193:### `GpuCamera` — 336 bytes, uniform buffer (Set 1, Binding 1)
docs/engine/shader-pipeline.md:380:| 1 | 1 | `UNIFORM_BUFFER` | `GpuCamera` (336 B) | triangle, water, cluster_cull, caustic_splat, volumetrics |
docs/engine/memory-budget.md:37:| Camera UBO | — | 1 | 336 B | 336 B | **672 B** |
docs/engine/renderer.md:269:6. Update the camera UBO (`GpuCamera`, 336 bytes) — view + prev-view-proj
docs/engine/renderer.md:576:> `gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_336_bytes`
docs/engine/renderer.md:577:> (the live 336-byte `GpuCamera` layout), and `GpuMaterial`-size tests pin
```

`shader-pipeline.md`'s last `GpuCamera` table row is `| 320 | 16 | render_origin | … |`. Meanwhile `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:290-296` is correct: *"GPU-side camera data (**352 bytes**, std140-compatible) … Layout pinned by `gpu_camera_is_352_bytes`."*

## Impact

`_audit-common.md`'s Key Reference Docs table names `docs/engine/shader-pipeline.md` as **the authority** for *"`GpuCamera`/`GpuInstance`/`GpuMaterial`/`GpuLight` exact byte layouts"* and tells every audit to *"prefer them over re-deriving facts from source."*

So an auditor or contributor who follows that instruction gets **a wrong size and a truncated field list for the single most widely re-declared GPU struct in the tree** — six shaders mirror `CameraUBO`. Per project memory *feedback_shader_struct_sync*, this hand-mirrored pattern is the documented #1 source of silent GPU desync.

Nothing is broken at runtime today — `reflect.rs:541`'s `camera_ubo_size_matches_gpu_camera_in_every_shader` derives `expected` from `size_of::<GpuCamera>()` and reflects all six declaring `.spv` blobs, and `8e7582ed` recompiled all six in the same commit. The damage is that **the reference material now actively teaches the wrong contract**.

This is also the *second* recurrence of the same shape: `GpuMaterial` 300 → 348 B is cited in `_audit-common.md:277-279` as the incident that justified the validate gate's symbol advisory in the first place. The advisory did not catch this one either — see the two sibling findings filed from this report about *why*.

## Suggested Fix

1. Change **336 → 352** at all five doc sites.
2. Add the missing `| 336 | 16 | render_debug | … |` row to `shader-pipeline.md`'s `GpuCamera` field table.
3. Rename the cited test at `renderer.md:576` to `gpu_camera_is_352_bytes`.
4. Re-tense `context/mod.rs:704` — *"doesn't touch GpuCamera's 336 B layout"* is a historical claim about #1023; make it "the then-336 B layout".
5. Then adopt the `docs/engine/*.md` glob extension filed alongside this, so the next growth is caught mechanically rather than four days and one audit sweep later.

## Related

- The two audit-tooling findings filed from this same report: the symbol advisory's case/negation blind spots, and its file glob excluding `docs/engine/` — running the *existing, unmodified* advisory logic over `docs/engine/*.md` surfaces `gpu_camera_is_336_bytes` immediately
- **#2753** — a separate, still-open `GpuCamera` doc defect (stale `triangle.vert` consumer credit, ambiguous position frame). Different sites; worth fixing in the same pass
- Precedent: `GpuMaterial` 300 → 348 B (`_audit-common.md:277-279`)
- `8e7582ed` (the growth) · `AUDIT_TECH_DEBT_2026-08-16.md` (the "Verified Clean" line this regresses)

## Completeness Checks
- [ ] **SIBLING**: All five doc sites updated, not just the heading — `grep -rn "336" docs/engine | grep -i camera` returns nothing
- [ ] **TABLE-COMPLETE**: `shader-pipeline.md`'s `GpuCamera` table runs to offset 336 and its rows sum to 352
- [ ] **TEST-NAME**: No doc cites `gpu_camera_is_336_bytes` (`grep -rn gpu_camera_is_336_bytes docs/engine`)
- [ ] **TESTS**: `gpu_camera_is_352_bytes` and `camera_ubo_size_matches_gpu_camera_in_every_shader` both still green
