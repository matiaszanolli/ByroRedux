# PERF-D5-NEW-04: ReSTIR reservoir SSBOs (~130-530 MB screen-dependent) are absent from memory-budget.md and all VRAM telemetry

**Issue**: #1814
**Labels**: low,vulkan,memory,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-04)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-04)

## Location
`crates/renderer/src/vulkan/restir.rs:36-83`; `docs/engine/memory-budget.md` (no entry)

## Description
Session 49 added two device-local, screen-sized reservoir buffers (one per FIF slot, `width * height * RESERVOIR_STRIDE`) — ~127 MB at 1080p, ~236 MB at 1440p, ~531 MB at 4K — the largest single VRAM addition of the denoiser overhaul, but the authoritative per-pass VRAM ledger has no row for it and no telemetry attributes it.

## Evidence
`restir.rs:34` `RESERVOIR_STRIDE = 32`; buffers sized `width * height * RESERVOIR_STRIDE` per FIF slot; `docs/engine/memory-budget.md` has no ReSTIR/reservoir row (grep confirms zero hits).

## Impact
Budget-accounting drift only — no leak (create-once + recreate-on-resize with fenced destroy is correct). At 4K this is >13% of the ~4 GB engine budget going untracked, the same class of gap that has historically preceded budget regressions.

## Related
RT VRAM budget baseline (~4 GB target); #1583/#1590 (removed the per-pixel reservoir G-buffer attachment this SSBO pair replaced).

## Suggested Fix
Add a "ReSTIR reservoirs" row to memory-budget.md with the W×H×stride formula, and include both buffers in the renderer's memory-usage log line.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

