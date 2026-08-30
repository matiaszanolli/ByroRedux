# #3622 — REN-2026-08-30-D19-03: the two POM marchers now agree on the height *channel* but disagree on the height *mip*

**Labels**: `low,renderer,shaders,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3622 --json state`.

---

- **Severity**: LOW
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/renderer/shaders/include/material_sampling.glsl` (`sampleParallaxHeight`) vs `crates/renderer/shaders/include/ray_hit.glsl` (`resolveRayHitUV`)
- **Status**: NEW
- **Description**: `#3530` correctly made both marchers honour `heightInAlpha`, but
  they still fetch at different LODs. The secondary-ray marcher is explicit and
  uniform — `textureLod(textures[nonuniformEXT(parallaxIdx)], currentUV, 0.0)` at
  all three of its fetch sites (`ray_hit.glsl:337`, `:346`, `:353`). The raster
  marcher uses implicit-LOD `texture(...)` inside a loop with a data-dependent
  `break`, i.e. sampling with implicit derivatives under non-uniform control flow,
  which the GLSL/Vulkan contract leaves undefined.
- **Evidence**: `material_sampling.glsl` — `sampleParallaxHeight` is
  `texture(textures[nonuniformEXT(idx)], uv)` and is invoked at `:109` (pre-loop),
  `:117` (inside the loop, after the `break` at `:112`), and `:125` (post-loop).
  Its own sibling function `perturbNormal` and the primary base-colour fetch have no
  such divergence problem because they are not inside a march.
- **Impact**: On a distant or steeply-foreshortened surface the raster pass marches a
  mip-blurred height field while its reflection marches the sharp mip-0 one, so the
  reflected UV displacement disagrees with the direct one — the same class of
  raster/reflection divergence `#3530` set out to close, one axis over. The
  undefined-derivative aspect is pre-existing (it predates `#3530`) and is not
  observably broken on current drivers, so this is filed at LOW.
- **Suggested Fix**: Compute the LOD once before the loop from the entry UV
  (`textureQueryLod` or an explicit `log2` of the UV footprint) and switch
  `sampleParallaxHeight` to `textureLod`, matching `ray_hit.glsl`'s discipline while
  keeping mip-appropriate filtering. Per the "no speculative Vulkan/shader changes"
  rule, land it behind an A/B capture rather than blind.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D19-03

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
