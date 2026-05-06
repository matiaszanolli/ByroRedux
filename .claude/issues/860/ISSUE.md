## FNV-D3-NEW-01: CellLoadResult.weather / .climate are dead fields (always emit None)

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 MEDIUM

## Severity / Dimension
MEDIUM / Cell loading — data coherence

## Location
`byroredux/src/cell_loader.rs:62,67` (struct fields), `cell_loader.rs:379-388` (sole producer `load_cell_with_masters`)

## Description
`CellLoadResult` carries `weather: Option<WeatherRecord>` and `climate: Option<ClimateRecord>` fields, but the only producer site sets them to `None` unconditionally:

```rust
Ok(CellLoadResult {
    cell_name: cell.editor_id.clone(),
    entity_count: result.entity_count,
    mesh_count: result.mesh_count,
    center: result.center,
    lighting: resolved_lighting,
    weather: None,
    climate: None,
    cell_root,
})
```

`grep -rn "CellLoadResult\b"` returns three hits — struct definition, function signature, the one construction site. **No consumer ever reads `weather` or `climate` off this struct**, and `scene.rs:705-755` ignores both fields when destructuring in the `--esm <path> --cell <id>` arm.

## Evidence
The audit checklist asserted that `CellLoadResult` exposes `WeatherRecord` "for scene.rs consumption". Type signature carries it but the values never flow. Exterior weather actually flows through `apply_worldspace_weather()` (scene.rs:204) at session bootstrap, reading `wctx.default_weather` / `wctx.climate` directly off the `ExteriorWorldContext`.

`#[allow(dead_code)]` on the struct masks this from `cargo check` warnings.

## Impact
Misleading type signature. Any future caller of `load_cell_with_masters` who tries to act on `result.weather` will see `None` and silently render with the engine default (FOG / SUNLIGHT / AMBIENT constants in `scene.rs:412-422`). Interior cells **could** legitimately route a scripted weather override here (interior storm, rain inside) but the field is plumbing that isn't connected to anything.

## Suggested Fix
Either:
1. **Populate** these fields from `index.weathers` / `index.climates` keyed off the cell's worldspace (when it's a behaves-as-exterior interior).
2. **Drop** them from the struct entirely and update the audit-spec wording.

The current shape is misleading — pick one. Add a doctest on `CellLoadResult` documenting whichever contract you choose.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `OneCellLoadInfo` (exterior streaming output type) — has no weather/climate at all, which is correct. No sibling cleanup.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Doctest pinning the contract (either "always None pending feature work" or "populated from worldspace lookup")
