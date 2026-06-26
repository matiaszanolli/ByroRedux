# TD3-001: renderer.md says GpuCamera is 304 B + cites a non-existent test (live: 336 B)

_Filed 2026-06-26 as #1754 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1754` for live state)._

**Severity**: MEDIUM (stale GPU-size in a doc — lockstep-drift bait) · **Dimension**: 3 — Stale Documentation
**Location**: `docs/engine/renderer.md:255`, `:512`, `:514`
**Status**: NEW · **Audit**: TD3-001

## Description
Three stale claims in the load-bearing renderer doc:
- `:255` — "Update the camera UBO (`GpuCamera`, 304 bytes)"
- `:512` — references a test named `gpu_camera_is_288_bytes`
- `:514` — "the live 304-byte `GpuCamera` layout"

## Evidence
The pinned test is `gpu_camera_is_336_bytes` (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:57`), asserting **336 B**. There is **no** `gpu_camera_is_288_bytes` test. `docs/engine/shader-pipeline.md:105,248` already correctly says 336 B, so renderer.md now self-contradicts its sibling doc. (GpuCamera grew 304→320 for DoF, then 320→336 for `render_origin`/markarth-precision.)

## Impact
This is the canonical renderer narrative — the place a contributor checks before touching the camera UBO / shader struct-sync. A reader pinning to 304 B (or grepping the dead test) gets the wrong byte budget. Classic lockstep-drift bait (`feedback_shader_struct_sync`).

## Suggested Fix
`:255` 304→336; `:512` `gpu_camera_is_288_bytes`→`gpu_camera_is_336_bytes`; `:514` "304-byte"→"336-byte" + drop the dead "288 for grep continuity" aside.

## Completeness Checks
- [ ] **SIBLING**: no other doc/comment quotes a stale GpuCamera/Instance/Material byte size
- [ ] **TESTS**: the cited test name resolves (`gpu_camera_is_336_bytes` exists)
