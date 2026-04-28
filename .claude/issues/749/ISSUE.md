# #749: SF-D3-01: BGSM/BGEM stopcond fires on ANY non-empty Name (no suffix gate)

URL: https://github.com/matiaszanolli/ByroRedux/issues/749
Labels: bug, nif-parser, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-01)
**Severity**: HIGH
**Status**: NEW

## Description

Both `BSLightingShaderProperty::parse` (`shader.rs:774-780`) and `BSEffectShaderProperty::parse` (`shader.rs:1361-1368`) short-circuit the entire trailing body on `bsver >= 155 && !name.is_empty()`. Per nif.xml the stopcond is supposed to fire **only when Name is a material-file reference** (`.bgsm` / `.bgem` / `.mat`), not on any non-empty editor label.

On Starfield, blocks may carry a non-path editor name. If so, `material_reference_stub` is returned with all PBR scalars defaulted, and the trailing body bytes are skipped via the parent `block_size`. Stream alignment is preserved, but block contents are zeroed silently.

## Evidence

```rust
// shader.rs:774-780 (and identical at 1361-1368 for BSEffectShaderProperty)
if bsver >= 155 {
    if let Some(name) = net.name.as_deref() {
        if !name.is_empty() {
            return Ok(Self::material_reference_stub(net));
        }
    }
}
```

The helper `material_path_from_name` at `crates/nif/src/import/mesh.rs:750-761` already filters on `.bgsm` / `.bgem` suffix; the stopcond should match that filter.

## Impact

For Starfield blocks where Name is an editor label (not a material path), every PBR scalar (shader_flags_1/2, CRC arrays, texture set ref, alpha, refraction strength, glossiness, specular_color, etc.) silently defaults. Renderer pulls zero-valued material from `MaterialInfo`. Not currently a regression on FO76 (Bethesda always ships paths in Name there) but a Starfield-specific risk that compounds with the missing `.mat` parser (see SF-D3-03).

## Suggested Fix

Tighten the gate to suffix-aware:

```rust
fn is_material_ref(name: &str) -> bool {
    let trimmed = name.trim_end_matches('\0').trim();
    let lower = trimmed.to_ascii_lowercase();
    lower.ends_with(".bgsm") || lower.ends_with(".bgem") || lower.ends_with(".mat")
}

if bsver >= 155 {
    if let Some(name) = net.name.as_deref() {
        if is_material_ref(name) {
            return Ok(Self::material_reference_stub(net));
        }
    }
}
```

Strip trailing `\0` and whitespace before the suffix check (artists occasionally export with trailing nulls or spaces). Reuse the helper across the two parse sites + the `material_path_from_name` at `mesh.rs:756`.

## Completeness Checks

- [ ] **SIBLING**: Replace the existing `mesh.rs:756` lowercase-and-test pattern with the same helper to avoid drift.
- [ ] **TESTS**: Add tests for: (a) non-empty Name without suffix → stopcond does NOT fire, full body parses; (b) `.bgsm` / `.BGSM` / `.bgem` / `.mat` suffix variations → stopcond fires.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.
