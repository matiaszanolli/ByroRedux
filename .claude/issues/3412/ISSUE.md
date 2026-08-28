# SKY-2026-08-27b-D6-01: `.bto` object LOD binds one worldspace atlas to every sub-mesh, but 34% of vanilla bindings name the mountain-slab texture instead

- **Severity**: MEDIUM
- **Dimension**: 6 (specialty blocks + real-data rendering)
- **Location**: `byroredux/src/cell_loader/object_lod.rs:294` (single atlas resolve), `:342` and `:356` (bound to every sub-mesh), premise stated at `:249`
- **Confidence**: CONFIRMED (measured across every shipped `.bto`)

## Description

`spawn_object_lod_quad` resolves exactly one texture per worldspace — `object_lod_atlas_path(scheme, worldspace_key)` — and binds it to every sub-mesh the `.bto` imports, discarding whatever each sub-mesh's own `BSShaderTextureSet` named. The module doc states the assumption as fact: *"All sub-meshes share the worldspace object atlas"* (`:249`). Vanilla Skyrim data contradicts it.

## Evidence

Reading slot 0 of every `BSLightingShaderProperty` in all 1,078 `.bto` files in `Skyrim - Meshes1.bsa`, grouped by worldspace:

```
22 distinct texture bindings across 1078 .bto
   1131  tamriel               -> data\textures\terrain\tamriel\objects\tamriel.objects.dds
    612  tamriel               -> data\textures\landscape\mountains\mountainslab02.dds
    114  dlc2solstheimworld    -> …\dlc2solstheimworld.objects.dds
     82  dlc2apocryphaworld    -> …\dlc2apocryphaworld.objects.dds
     61  dlc01falmervalley     -> …\dlc01falmervalley.objects.dds
     58  dlc01soulcairn        -> …\dlc01soulcairn.objects.dds
     57  dlc2solstheimworld    -> data\textures\landscape\mountains\mountainslab02.dds
     40  sovngarde             -> …\sovngarde.objects.dds
     39  sovngarde             -> data\textures\landscape\mountains\mountainslab02.dds
     …
```

**809 of 2,357** slot-0 bindings (34.3%) name `mountainslab02.dds`, and every one of the 12 worldspaces with baked object LOD has some. The atlas the code binds does exist and resolves (`textures\terrain\tamriel\objects\tamriel.objects.dds` → `Skyrim - Textures7.bsa`, verified by direct `BsaArchive::contains`), so this is not a missing-asset problem — the correct per-sub-mesh path is right there in the imported material and is simply not read.

## Impact

Roughly a third of Skyrim's distant-object LOD geometry — in Tamriel, that is the mountain silhouette, the most visually prominent distant feature in the game — samples the object *atlas* with UVs authored for a tiling mountain slab. The result is coherent-looking garbage rather than an obvious failure, which is why the parse-rate and texture-resolution gates stay green. The `_n` normal siblings are also present for every atlas and are separately unbound, which `object_lod.rs:508-509` already records as known forward scope.

## Suggested Fix

Prefer each sub-mesh's own imported `material.textures.base_color` when it is populated (running it through the existing `strip_build_prefix` for the `data\` prefix these paths carry), and keep the per-worldspace atlas as the fallback for sub-meshes that name nothing. That reduces to one `resolve_texture` per distinct path per quad, which is the same order of work as today.

## Related

#2444 (MAT-D3-02 — object LOD carries no real `Material`; CLOSED, and its `translate_texture_only_material` call at `:356` is the site that would carry the fix), #2586 / #2587 (earlier `.bto` corpus/alignment work). Distinct from `SKY-2026-08-27-D6-01` (`.btr` height scale, #3358, CLOSED) — that was terrain, this is objects, and it is a texture binding, not a transform.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
