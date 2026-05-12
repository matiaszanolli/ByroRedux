# Issue #976

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/976
**Title**: NIF-D4-NEW-02: BSLightingShaderProperty drops Starfield .mat material refs — asymmetric with BSEffectShader branch
**Labels**: bug, nif-parser, import-pipeline, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 4)
**Severity**: MEDIUM
**Dimension**: Import Pipeline
**Game Affected**: Starfield primarily; FO76 marginally
**Location**: `crates/nif/src/import/material/walker.rs:116-121`

## Description

The Skyrim+ `BSLightingShaderProperty` branch inlines an ad-hoc suffix test:

```rust
if let Some(name) = shader.net.name.as_deref() {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
        info.material_path = intern_texture_path(pool, name);
    }
}
```

The shared helper `is_material_reference` (`crates/nif/src/blocks/shader.rs:24`) — explicitly authored to be the single stopcond source — also accepts `.mat` (Starfield JSON materials) and trims trailing `\0` / whitespace per #749. The sibling `BSEffectShaderProperty` branch (four lines below in walker.rs) correctly routes through `mesh::material_path_from_name` which delegates to `is_material_reference`.

The asymmetry means a Starfield `BSLightingShaderProperty` whose `name` carries a `.mat` reference imports with `material_path = None`. Any trailing-whitespace BGSM reference also misses.

## Impact

`mesh.info <entity>` debug command shows `material_path = None` for Starfield meshes that explicitly reference a `.mat` JSON. The BgsmProvider lookup chain (sits upstream of open #762 SF-D6-03) never even receives the path string, so the Starfield material system can't kick in.

## Suggested Fix

One-line delegate to the existing helper:

```rust
if let Some(idx) = shader_property_ref.index() {
    if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx) {
        info.material_path = mesh::material_path_from_name(shader.net.name.as_deref(), pool);
        // ... rest of texture_set handling unchanged
    }
}
```

Mirrors the BSEffectShaderProperty branch four lines below. The helper already handles all three suffixes (`.bgsm`, `.bgem`, `.mat`) and trailing-whitespace trimming.

## Completeness Checks

- [ ] **SIBLING**: Any other place in the importer that pattern-matches `.bgsm` / `.bgem` inline? Grep for `ends_with(\".bgsm\")` / `ends_with(\".bgem\")` across `crates/nif/src/import/`
- [ ] **TESTS**: Add a fixture test with a Starfield BSLightingShaderProperty whose `name` ends in `.mat`; assert `material_path = Some(\"...\")`
- [ ] **SHARED_HELPER**: After the fix, the inline suffix check at walker.rs:116-121 should be deletable — confirm no other call site depends on its specific behaviour
- [ ] **#762 LINK**: Reference this issue from #762 so the SF-D6-03 fix has the upstream prerequisite tracked

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D4-NEW-02.

Related: #749 (is_material_reference helper), #762 (SF-D6-03 .mat parser + provider integration — downstream of this).

