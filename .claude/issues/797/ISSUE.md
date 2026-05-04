# SAFE-22 / #797 — MaterialTable::intern returns OOB material_id past MAX_MATERIALS

**Severity**: MEDIUM (reachability low today; severity per `_audit-severity.md` would be CRITICAL on actual reachability)
**Domain**: renderer / memory / safety
**Status**: NEW

## Locations
- `crates/renderer/src/vulkan/material.rs:326-334` (intern, no cap)
- `crates/renderer/src/vulkan/scene_buffer.rs:975-983` (upload claims unimplemented "default to 0")
- `crates/renderer/src/vulkan/scene_buffer.rs:63` (`MAX_MATERIALS = 4096`)

## One-line summary
`intern()` returns `material_id`s past `MAX_MATERIALS` once the table grows past the cap; the upload-side warn message at `scene_buffer.rs:978-979` claims those over-cap ids "silently default to material 0" but that defaulting logic doesn't exist anywhere. GPU shader reads `materials[id]` past the SSBO end → implementation-defined behaviour.

## Fix shape
One-line cap at `material.rs:330` — return 0 once `self.materials.len() >= MAX_MATERIALS`. Mirror the `Once`-gated pattern at `byroredux/src/render.rs:240-247` for the bone-palette overflow guard.

## Real-content reachability
Interior cells: 50–200 uniques. Exterior 3×3 grid: 300–600. Cap is 4096. Reachable today only on modded / synthetic / future Starfield-FO76 large content.

## Audit source
`docs/audits/AUDIT_SAFETY_2026-05-03.md` finding SAFE-22.
