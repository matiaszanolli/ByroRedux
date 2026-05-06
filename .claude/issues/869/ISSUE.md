## Source Audit
`docs/audits/AUDIT_OBLIVION_2026-05-06_DIM4.md` — Dimension 4 (Rendering Path)

## Severity / Dimension
LOW / 4 (Rendering Path)

## Location
- Capture: `crates/nif/src/import/material/walker.rs:733-744`
- Propagate: `crates/nif/src/import/mesh.rs:641-642, 933-934, 1139-1140`
- Pipeline forces FILL: `crates/renderer/src/vulkan/pipeline.rs:212, 222, 410, 573`
- Shaders have no `flat` qualifier: `crates/renderer/shaders/triangle.vert`, `triangle.frag`

## Description
Two `NiFlagProperty` subtypes are parsed by the importer, captured into `MaterialInfo`, and propagated through `ImportedMesh` — but the renderer never consults the resulting flags.

- **`NiWireframeProperty`** (flags=1 enables wireframe): walker sets `info.wireframe = true`. Every blend-pipeline arm in `pipeline.rs` hard-codes `polygon_mode(vk::PolygonMode::FILL)`. There is no `WireframePipeline` variant.
- **`NiShadeProperty`** (flags=0 requests flat shading): walker sets `info.flat_shading = true`. The shaders use smooth interpolation throughout — no GLSL `flat` qualifier on `frag_normal` or related varyings.

The walker comments at `walker.rs:734-735` and `:740-742` explicitly note "Renderer consumption is future work" for both, so this is tracked-but-unfiled rather than a regression.

## Evidence
```rust
// walker.rs:733-744 — capture
"NiWireframeProperty" if flag_prop.enabled() => {
    info.wireframe = true;  // captured but downstream FILL forces fill mode
}
"NiShadeProperty" if !flag_prop.enabled() => {
    info.flat_shading = true;  // captured but no `flat` qualifier in shaders
}
```

Verification grep confirms zero non-import-side consumers:
```
$ grep -rn "info\.wireframe\|info\.flat_shading\|mat\.wireframe\|mat\.flat_shading" crates/renderer/ byroredux/
(no matches)
```

## Impact
- **Wireframe**: zero impact on Oblivion vanilla. The walker comment notes wireframe is "Not present in Oblivion vanilla but used by FO3/FNV mods." A debug/mod-only feature whose absence does not affect the gameplay-content render.
- **Flat shading**: low impact on Oblivion vanilla. Walker comment notes it appears on "a handful of Oblivion architectural pieces" — meshes that should look faceted will instead render with smooth normals. Subtle visual mismatch, not a black-content failure.

## Suggested Fix
- **Wireframe**: ship a third pipeline variant `WireframeOpaque { two_sided }` in `pipeline.rs` with `polygon_mode(vk::PolygonMode::LINE)`, and one matching `Blended` variant. Pipeline-cache key already includes blend mode + two-sided; one more boolean fits naturally. Estimated 30-40 lines including the `MaterialKind` arm.
- **Flat shading**: less clean. Adding a `flat` qualifier to a fragment input forces vertex-shader `flat`-out on the same varying, which fights the per-vertex normal interpolation the rest of the pipeline depends on. Either (a) build a parallel `triangle_flat.vert/frag` pair and switch pipeline at draw time, or (b) compute face normals in fragment via `dFdx(worldPos) × dFdy(worldPos)`. Option (b) is simpler but adds 2 instructions to every fragment; gate behind a per-batch flag. Defer until a player-visible Oblivion mesh is identified that needs it.

## Related
None. No prior open issue covers either flag. Walker comments treat both as deferred work.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
