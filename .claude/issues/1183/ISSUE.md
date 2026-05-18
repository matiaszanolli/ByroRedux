# SF-D1-NEW-01: BSLightingShaderProperty Root Material field read but discarded

**Labels**: bug, nif-parser, medium

**Source**: [`docs/audits/AUDIT_STARFIELD_2026-05-18.md`](docs/audits/AUDIT_STARFIELD_2026-05-18.md)
**Dimension**: NIF BSVER 155-172+ shader blocks
**Severity**: MEDIUM (forward risk for Starfield content; no vanilla regression)

## Observation

`crates/nif/src/blocks/shader.rs:849-852`:

```rust
// Root Material (NiFixedString) — FO4+ only (BSVER >= 130).
if bsver >= crate::version::bsver::FALLOUT4 {
    let _root_material = stream.read_string()?;
}
```

The field is a `NiFixedString` per nif.xml (`vercond="#BS_GTE_130#"`), positioned between `Emissive Multiple` and `Texture Clamp Mode`. We read the value to keep stream alignment but drop it without surfacing it on `BSLightingShaderProperty`.

## Why bug

- For FO4 (BSVER 130) and Skyrim+ the inline Phong body is the authoritative material source — Root Material is a defensible sidecar reference, mostly redundant with `net.name`.
- For Starfield (BSVER 172+) the stopcond at `shader.rs:771-777` captures the common case (`net.name` IS the BGSM/BGEM/MAT path). The forward-risk case: `net.name` carries a non-material editor label AND Root Material carries the actual material path — that BGSM/BGEM/MAT reference is silently dropped.

The shape's prevalence in real Starfield content is unknown; needs empirical sampling. Likely not common in vanilla (the stopcond covers the dominant authoring pattern) but a regression vector for mod content or future SDK behavior.

## Trigger Conditions

Starfield NIF with BSVER ≥ 155 whose `BSLightingShaderProperty.NiObjectNET.name` is a non-material editor label (e.g., `"Material_Slot_01"`) AND whose Root Material field carries an actual `.bgsm` / `.bgem` / `.mat` path. Verifying frequency requires a sweep across vanilla `Starfield - Meshes*.ba2`.

## Fix

Promote `_root_material` to a real field on `BSLightingShaderProperty` (e.g., `root_material_path: Option<Arc<str>>`). In the importer (`crates/nif/src/import/material/walker.rs:122` + `:289`), when `material_path_from_name(shader.net.name.as_deref(), pool)` returns `None`, fall back to `shader.root_material_path` for the BGSM/BGEM/MAT capture.

Before shipping, sample a few hundred Starfield NIFs whose body parses (i.e. stopcond didn't fire) and confirm whether their Root Material is non-empty — sizes whether this is a vanilla concern or mod-only forward risk.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: `BSEffectShaderProperty` doesn't have a Root Material field per nif.xml (Source Texture is its own thing); verify the parser at `shader.rs:1382+` is consistent
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic FO4+ BSLightingShaderProperty fixture with non-empty Root Material round-trips into the new field; importer-level test confirming fallback fires only when `net.name` is not a material reference

## Related

- #749 — narrow `.bgsm`/`.bgem`/`.mat` suffix gate for the stopcond
- #1080 / FO4-D3-009 — FO4 (130) stopcond exemption (Root Material currently discarded for FO4 too, but FO4 has the inline body as canonical source)
- #762 / SF-D6-03 — Starfield `.mat` parser (downstream consumer of any captured material_path)
