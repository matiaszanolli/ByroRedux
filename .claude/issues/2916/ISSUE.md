# REN-D2-01: Glass refraction multiplies the hit texture in twice — avgAlbedo stopped being the tint at #1628

- **Issue**: [#2916](https://github.com/matiaszanolli/ByroRedux/issues/2916)
- **Finding ID**: `REN-D2-01`
- **Labels**: `medium,renderer,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2916 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: SSBO/Indexing
- **Location**: `crates/renderer/shaders/triangle.frag` — the IOR refraction terminus
  (`tInst` / `tAlbedo` / `tColor` inside the `refractionResolved` branch); field source:
  `gi_albedo` in `crates/renderer/src/vulkan/context/draw.rs`; correct sibling:
  `rayHitAlbedo` in `crates/renderer/shaders/include/ray_hit.glsl`
- **Status**: NEW
- **Description**: The refraction terminus is the only secondary-ray hit site that derives its
  surface colour from `GpuInstance.avgAlbedo*` instead of the shared `rayHitAlbedo(mat,
  baseRgb)` helper. It samples the hit's diffuse texture (`textureLod(textures[tInst
  .textureIndex], tUV, refrMip)`) and then multiplies by `tInst.avgAlbedoR/G/B`. Since #1628
  (`93add433`, 2026-06-15) `avg_albedo_*` is no longer the material tint: `draw.rs` uploads
  `draw_cmd.avg_albedo[i] * handle_avg_rgb(texture_handle)[i]` — the material `diffuse_color`
  **times the diffuse texture's mean texel colour**. The refraction site (`f1b6e1e9`,
  2026-06-05) predates that change by ten days and was never revisited, so the texture now
  enters the product twice: once as the sampled texel, once as its own frame-wide mean.
- **Evidence**:
  - `draw.rs`: `let gi_albedo = match self.texture_registry.handle_avg_rgb(
    draw_cmd.texture_handle) { Some(mean) => [draw_cmd.avg_albedo[0] * mean[0], …], None =>
    draw_cmd.avg_albedo }`, then `avg_albedo_r: gi_albedo[0], …`.
  - `triangle.frag`: `vec3 tColor = tAlbedo * vec3(tInst.avgAlbedoR, tInst.avgAlbedoG,
    tInst.avgAlbedoB);`, guarded by a comment that still asserts "multiply by the hit's
    canonical avgAlbedo (the material diffuse_color) … For textured content avgAlbedo is the
    white tint, so detail is preserved" — both clauses were true before #1628 and are false
    now.
  - Every other terminus uses `rayHitAlbedo(mat, baseRgb) = max(baseRgb * vec3(mat.diffuseR,
    mat.diffuseG, mat.diffuseB), vec3(0.0))`: `traceReflection` (`hitColor`), the GI path loop
    (`hitAlbedo`), `traceWaterRay`, and `traceShadowTransmittance`'s glass tint.
  - `bindings.glsl`'s own field comment — "offset 96 — kept for `caustic_splat.comp` (set 0
    reads, not migrated)" — no longer describes the readership; `triangle.frag` reads it too.
- **Impact**: Every surface seen *through* refractive glass renders darker than the same
  surface seen directly or in a mirror, by that surface's own mean texel luminance (typically
  0.2–0.5 for Bethesda diffuse maps, i.e. roughly 2–5×). Untextured / vertex-coloured content
  (Cornell walls, the `--cornell` harness) is unaffected because `handle_avg_rgb` returns
  `None` for fallback handles, which is exactly why the Cornell probe cannot surface it.
  Blast radius: all games, every `MATERIAL_KIND_GLASS` draw that resolves a textured terminus.
  Visual-only — no index goes out of range.
- **Related**: #1628 (introduced the semantic change), #789 / `f1b6e1e9` (introduced the
  read), #804 (removed the `GpuMaterial` copy that would otherwise have been the natural
  source), #1098 / #1230 (the still-open-in-spirit "migrate `avg_albedo` off `GpuInstance`"
  thread).
- **Suggested Fix**: Replace `tAlbedo * vec3(tInst.avgAlbedo*)` with `rayHitAlbedo(tMat,
  tAlbedo)` so the terminus uses the same texture × `mat.diffuse*` rule as every sibling path,
  and correct the stale comment block. If the texel-mean folding is wanted for refraction as
  well, it must replace — not multiply — the texture sample. Also tighten `bindings.glsl`'s
  `avgAlbedoR` comment to name its real readers.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
