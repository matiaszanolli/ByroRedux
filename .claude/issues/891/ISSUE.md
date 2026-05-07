# LC-D2-NEW-01: NiTextureEffect parsed but never imported

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/891
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07.md
**Severity**: LOW
**Dimension**: NIF Format Readiness — importer coverage gap

## Location
- Parser: crates/nif/src/blocks/texture.rs:567-684
- Dispatch: crates/nif/src/blocks/mod.rs:207
- No consumer in crates/nif/src/import/, byroredux/src/, or crates/renderer/src/.

## Game Affected
Oblivion exterior cells (sun gobo / projected shadow), Oblivion magic FX (projected env maps), FO3 / FNV interior light cookies, occasional Skyrim-LE projected decals.

## Root Cause
NiTextureEffect parser landed via #163 (CLOSED) — explicitly scoped to the wire layout only, not the importer plumbing or renderer pass. Six months later there is still no `import_nif_*_effect` analogue to `import_nif_lights`, and no ECS component represents projected textures.

## Suggested Fix (two-phase)

**Phase 1** (~30 LOC): Add `ImportedTextureEffect` to `crates/nif/src/import/mod.rs`, populated alongside `ImportedLight` via the same `NiDynamicEffect.affected_nodes` resolution pattern used by #335 / #461.

**Phase 2** (deferred): Renderer-side projector pass + `ProjectedTexture` ECS component.

## Verification

```bash
grep -rn NiTextureEffect crates/nif/src/import/ byroredux/src/ crates/renderer/src/
# expected: zero hits before fix; one or more import-site hits after Phase 1
```

## Related
- #163 (CLOSED): parser landing
- #335 / #461: NiDynamicEffect.affected_nodes resolution pattern
