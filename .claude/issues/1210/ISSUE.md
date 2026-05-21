# #1210 — REN-DIM17-01: water-side caustics unimplemented + untracked

**Source**: docs/audits/AUDIT_RENDERER_2026-05-19_DIM17.md (Dim 17, LOW)
**Severity**: low / Labels: low, renderer, enhancement
**State**: OPEN (filed 2026-05-20)
**Type**: M38-Phase-2 tracking issue

## Cause

`caustic_splat.comp:211-215` documents the architectural split — "the water-side caustic is the water shader's responsibility (M38)". `water.frag` has zero caustic implementation. `is_caustic_source` (draw.rs:52) intentionally excludes water. After #1069 / #1070 / #1071 / #1129 closed, no open issue tracks the gap.

## Fix (tracking-only)

Keep the gap visible to future audits. Implementation is M38-Phase-2:
- Per-fragment ray query to sun
- Refract via water surface normal (IOR 1.33)
- Project + `imageAtomicAdd` into existing R32_UINT caustic accumulator
- Composite already consumes; just need water surface to splat

Constraints (REN-D13-NEW-04): single eta, single bounce.

Secondary: add Dim-17 checklist item in `.claude/commands/audit-renderer.md` ("water-side caustic implementation status: deferred / wired") so future audits surface this without re-reading both shaders.

## Risk

ZERO — tracking-only.

## Estimated impact

Visual fidelity on exterior water on sunny TODs. Bethesda content uses water surfaces extensively. Post-#1199 the TOD palette survives cell unloads, making the bench meaningful again.
