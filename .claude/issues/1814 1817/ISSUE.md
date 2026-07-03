# #1814: PERF-D5-NEW-04: ReSTIR reservoir SSBOs (~130-530 MB screen-dependent) are absent from memory-budget.md and all VRAM telemetry

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-04)
**Location**: `crates/renderer/src/vulkan/restir.rs:36-83`; `docs/engine/memory-budget.md` (no entry)

## Description
Session 49 added two device-local, screen-sized reservoir buffers (one per FIF
slot, `width * height * RESERVOIR_STRIDE`) — ~127 MB at 1080p, ~236 MB at 1440p,
~531 MB at 4K — the largest single VRAM addition of the denoiser overhaul, but the
authoritative per-pass VRAM ledger has no row for it and no telemetry attributes
it.

## Impact
Budget-accounting drift only — no leak (create-once + recreate-on-resize with
fenced destroy is correct). At 4K this is >13% of the ~4 GB engine budget going
untracked, the same class of gap that has historically preceded budget
regressions.

## Suggested Fix
Add a "ReSTIR reservoirs" row to memory-budget.md with the W×H×stride formula,
and include both buffers in the renderer's memory-usage log line.

---

# #1817: SCR-D6-NEW-02: Trigger volume's occupant_inside not seeded from initial containment — spurious enter-fire when player loads already inside

**Severity**: MEDIUM
**Dimension**: Engine Attach & Trigger Wiring (runtime consequence)
**Location**: `byroredux/src/cell_loader/references.rs:1455-1461`
(`trigger_volume_from_primitive`) + `crates/scripting/src/trigger.rs:114-120`

## Description
`trigger_volume_from_primitive` hardcodes `occupant_inside: false` at spawn.
`trigger_detection_system` fires `OnTriggerEnterEvent` on the
`inside && !occupant_inside` edge. When the player begins a cell/save load
*already standing inside* a trigger volume, frame-1 detection sees `inside ==
true` against the seeded `false` and fires a spurious enter — i.e.
level-triggered-on-load rather than edge-triggered. Bethesda's `OnTriggerEnter`
semantics fire only on an actual outside→inside crossing.

## Impact
A quest gated on `OnTriggerEnter` can advance the instant the player loads a save
while inside the trigger box, even though they never crossed the boundary that
frame — silent game-logic corruption on load. Realistic for autosaves taken
inside a scripted trigger region.

## Suggested Fix
Seed `occupant_inside` from the volume's containment of the player's initial
world position at spawn (or run one silent "prime" pass of
`trigger_detection_system` that updates `occupant_inside` without emitting
markers before the first gameplay frame).
