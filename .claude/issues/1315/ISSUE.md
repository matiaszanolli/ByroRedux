# #1315 -- OBL-D4-NEW-04: pipeline.rs wireframe comment stale

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: INFO | **Dim 4** — Rendering Path for Oblivion Shaders
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D4-NEW-04)

**Location**: `crates/renderer/src/vulkan/pipeline.rs:246-255`

**Issue**: A comment in `pipeline.rs:246-255` claims that wireframe rendering (#869) is unimplemented / "deferred fix ships a `WireframeOpaque { two_sided }` pipeline variant". In fact wireframe is fully wired through `PipelineKey` (`context/draw.rs:1589-1599`) and the LINE pipeline is created and selected; `triangle.frag:1034` also handles the `flat_shading` path from `NiShadeProperty`. The comment is stale doc-rot that could trigger redundant work.

**Suggested fix**: update `pipeline.rs:246-255` to reflect that wireframe is live (wired through `PipelineKey`) and remove the "deferred fix ships ..." language.

## Completeness Checks
- [ ] **SIBLING**: check the audit-renderer Dim 3 checklist comment for the same stale framing
- [ ] **TESTS**: no behavior change
- [ ] **CANONICAL-BOUNDARY**: renderer-only; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
