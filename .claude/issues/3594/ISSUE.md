# #3594 — REN-2026-08-30-D8-04: two transcription defects in the code #3426 relocated — a mangled `const` assertion message and a warn-once that now misdiagnoses a second failure mode

**Labels**: `low,renderer,tech-debt,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3594 --json state`.

---

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/presentation.rs:645-651` (`_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`); `crates/renderer/src/vulkan/context/post_passes.rs:1095-1104` (the `overlay.is_none() && ui_instance_idx.is_some()` warn-once)
- **Status**: OPEN — introduced by commit `b28acb0c` (#3426)
- **Description**: Two independent nits, both artefacts of moving the block out of
  `geometry_pass.rs`:
  1. The assertion message lost its string line-continuation backslash in the
     move. The literal now reads
     `"UI overlay path covers VIEWPORT + SCISSOR only —                  extend it before growing UI_PIPELINE_DYNAMIC_STATES"`
     — 18 literal spaces mid-sentence. The `geometry_pass.rs` original had
     `only — \` + indentation, which the compiler folded away.
  2. The relocated warn-once fires on a strictly wider condition than the message
     describes. In `geometry_pass.rs` it was nested inside
     `if let Some(mesh) = self.mesh_registry.get(ui_quad)`, so "global-only" was
     the only reachable cause. The new site tests
     `overlay.is_none() && ui_instance_idx.is_some()`, which is also true when
     `self.mesh_registry.get(ui_quad)` returns `None` (handle not in the
     registry) — a different failure that would be reported as
     `"UI overlay quad has no per-mesh vertex/index buffer (global-only)"`. The
     surrounding comment already enumerates the three causes
     ("no UI texture this frame, no registered quad, or a quad with no per-mesh
     buffers"); only the log message was not widened. `ui_quad_handle == None` is
     genuinely unreachable here because `draw.rs:3242` gates `ui_instance_idx` on
     it, so the widening is by exactly one case.
- **Evidence**: `sed -n '645,651p' crates/renderer/src/vulkan/presentation.rs`;
  `git diff 969d81c8..HEAD -- crates/renderer/src/vulkan/context/geometry_pass.rs`
  shows the original `only — \` continuation; `draw.rs:3241-3252` shows the
  `ui_instance_idx` gate.
- **Impact**: Cosmetic in both cases — a compile-time panic string nobody has hit,
  and a once-per-process warning that would name the wrong of two adjacent causes.
  No runtime behaviour change.
- **Suggested Fix**: Restore the `\` continuation in the assertion message; make
  the warn text cover both causes (e.g. "UI overlay quad is unavailable (not in
  the mesh registry, or global-only with no per-mesh vertex/index buffer)"), or
  split the `mesh_registry.get` miss into its own arm.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D8-04

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
