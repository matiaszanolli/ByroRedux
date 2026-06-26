# REN-2026-06-26-I02: audit-renderer Dim-13 checklist points jitter assembly at render/camera.rs; Halton jitter lives in context/draw.rs

**Audit**: `docs/audits/AUDIT_RENDERER_2026-06-26.md` (Dimension 13 — TAA)
**Severity**: INFO (audit-skill checklist drift; no engine code defect)
**Status**: NEW, CONFIRMED against live `.claude/commands/audit-renderer/SKILL.md`

## Description
The Dimension-13 entry points and checklist in `.claude/commands/audit-renderer/SKILL.md` state the
Halton(2,3) TAA jitter is assembled in `byroredux/src/render/camera.rs`. That file contains zero
Halton/jitter computation — it only assembles the un-jittered `view_proj`. The per-frame Halton
jitter is computed in `crates/renderer/src/vulkan/context/draw.rs` (`halton` fn + the `(jx, jy)`
block in `draw_frame`, `idx = (frame_counter % 16) + 1`) and uploaded into `GpuCamera.jitter`. The
guard itself holds (jitter advances per frame, applied in NDC); only the file pointer is stale.

## Evidence
- `.claude/commands/audit-renderer/SKILL.md` line ~229 ("Entry points … jitter assembly in `byroredux/src/render/camera.rs`").
- `grep -c "halton\|Halton" byroredux/src/render/camera.rs` → 0.
- Live code: `crates/renderer/src/vulkan/context/draw.rs` — `fn halton(...)`, `halton(idx, 2)` / `halton(idx, 3)`.

## Impact
None functional. Misdirects a future TAA audit to the wrong file.

## Suggested Fix
Update the Dim-13 entry-point / checklist references to point jitter assembly at
`crates/renderer/src/vulkan/context/draw.rs` (`halton` + the `(jx,jy)` block). No engine code change.

## Completeness Checks
- [ ] **SIBLING**: confirm no other audit skill (e.g. /audit-performance) repeats the `camera.rs` jitter mis-pointer
- [ ] **DOC**: the corrected ref names the `halton` symbol so it survives future line drift
