# #1198 — PERF-DIM7-07: bump MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME 16 → 227

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, LOW — "Quick wins")
**Severity**: low
**Labels**: bug, low, renderer, M29
**State**: OPEN (filed 2026-05-19)

## Cause

crates/renderer/src/vulkan/scene_buffer/constants.rs:44 caps pending bind_inverses uploads at 16 per frame. Cells with > 16 simultaneously first-sighting NPCs (FO4 MedTek 23; FO3 Megaton REFR spill) take 2 frames to populate `bind_inverses_persistent`. Un-uploaded entities render in bind pose (palette = identity × identity per #1191) for one frame.

## Fix

Bump constant from 16 to 227 = `MAX_TOTAL_BONES / MAX_BONES_PER_MESH`. HOST_VISIBLE staging cost moves 144 KB → ~2 MB, trivial on 6 GB VRAM target.

## Risk

LOW — verify no test pins the literal 16.

## Estimated impact

0 FPS. Eliminates one-frame bind-pose glitch on heavy first-sight cell loads.
