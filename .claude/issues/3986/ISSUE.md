# #3986 — REN-2026-09-06-D2-01: `rayHitHasCoverage` omits the decal-slot alpha composite the raster path applies, so alpha-tested decal meshes cast a different silhouette than they present

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl` —
  `rayHitHasCoverage` / `alphaComparePass`. Raster counterpart:
  `crates/renderer/shaders/triangle.frag`, the `materialDecals` overlay loop
  and the inline `mat.alphaTestFunc` block immediately after it. Data source:
  `crates/renderer/src/vulkan/material.rs` (`decal_map_0_index`, and
  `supplemental_texture_slot::DECAL_0`, imported as `slot` at the call site),
  populated from `byroredux/src/render/static_meshes.rs`
  (`supplemental_texture_indices[slot::DECAL_0] = texture_indices.decals[0]`).
  GLSL mirror: `mat.decalMap0Index` in `crates/renderer/shaders/include/bindings.glsl`.
- **Status**: **NEW.** No open issue matches (`decal`, `silhouette`,
  `coverage`, `alpha-test`, `rayHitHasCoverage`, `alphaComparePass` all return
  nothing across `/tmp/audit/renderer/open_titles.txt` and the JSON cache). No
  prior `docs/audits/` report names `rayHitHasCoverage` in this context.
  Sibling of the already-filed #3902, which covers the *shade* half of the same
  primary↔secondary divergence in `rayHitAlbedo`; this is the *coverage* half,
  in a different function.
- **Description**: `ray_hit.glsl`'s own `getHitVertexAlpha` docstring states the
  invariant: *"secondary rays must reconstruct the same barycentric value or
  alpha-tested leaves/grates cast a different silhouette from the visible
  surface."* The raster path builds its alpha-test input in this order — sample
  diffuse, apply the BC1 punch-through pin, multiply by
  `mat.materialAlpha * fragColor.a`, then run the four `mat.decalMap0..3Index`
  overlays as an alpha-over composite (`texColor.a = decalSample.a +
  texColor.a * (1.0 - decalSample.a)`), and only then evaluate the
  `alphaTestFunc` comparison. `rayHitHasCoverage` reproduces every step of that
  chain **except** the decal composite: it applies the BC1 pin, multiplies by
  `mat.materialAlpha * getHitVertexAlpha(...)`, and calls `alphaComparePass`
  directly.

  Because the composite is alpha-over, the raster alpha is always **≥** the
  un-composited alpha wherever a decal slot is bound. The RT silhouette is
  therefore a strict subset of the raster silhouette: shadow, reflection,
  refraction, GI and water rays punch through texels that the visible surface
  covers.

  Second, smaller divergence in the same pair: the seven-arm comparison table
  is hand-written twice with a **different EQUAL/NOTEQUAL epsilon** —
  `abs(a - aThresh) < 0.004` in `triangle.frag` versus
  `abs(alpha - threshold) < (1.0 / 255.0)` (0.0039216) in `alphaComparePass`.
  `alphaComparePass` already exists as the shared helper; the raster path does
  not call it. Two copies that have already drifted once will drift again.
- **Evidence**:
  - `triangle.frag`: `uint materialDecals[4] = uint[4](mat.decalMap0Index, …);
    for (int decalIndex = 0; decalIndex < 4; ++decalIndex) { … texColor.a =
    decalSample.a + texColor.a * (1.0 - decalSample.a); }` — unconditional, no
    material-kind gate — followed by `if (aThresh > 0.0) { … if (!pass) discard; }`.
  - `ray_hit.glsl::rayHitHasCoverage`: `alpha *= mat.materialAlpha *
    getHitVertexAlpha(instanceIdx, primitiveIdx, barycentrics); if
    (!alphaComparePass(alpha, mat.alphaThreshold, mat.alphaTestFunc)) return false;`
    — `mat.decalMap*Index` appears nowhere in the file.
  - The role is live, not dead: `decal_map_0_index` is written into
    `GpuMaterial` and hashed into the dedup key
    (`h.write_u32(mat.decal_map_0_index)` in `material.rs`), and fed from
    `texture_indices.decals[0]` in `static_meshes.rs`.
- **Impact**: Wrong RT silhouettes — holes in shadows, reflections and GI that
  the visible surface does not have — for any alpha-tested mesh whose
  `NiTexturingProperty` decal slots are populated with a partially-transparent
  overlay. That is Oblivion/FO3/FNV-era authoring, where the decal slots are
  the legacy multi-layer path. **The affected population is uncensused**: no
  archive was mounted this run, so the count of materials with
  (`alphaThreshold > 0` ∧ a bound decal slot ∧ `decalSample.a < 1`) is unknown.
  The mechanism is certain; the blast radius is not. Rated MEDIUM per the
  severity decision tree's "visual artifacts only" row rather than escalated,
  precisely because the population is unmeasured.
- **Related**: #3902 (the shade half of the same primary↔secondary divergence);
  #3911 (supplemental role↔slot correspondence is unpinned, which is how a
  decal slot could also be mis-routed); #1653 / #ae285062 (the BC1 pin that
  *is* mirrored correctly in both paths).
- **Suggested Fix**: Census first, then fix — count decal-slot materials with an
  active alpha test across the FO3/FNV/Oblivion archives before changing the
  hit path, since the composite costs up to four extra bindless fetches per RT
  hit. If the population is non-trivial, fold the four-slot alpha-over composite
  into `rayHitHasCoverage` behind an early-out on
  `mat.decalMap0Index == 0u && …`. Independently and cheaply: replace
  `triangle.frag`'s inline seven-arm block with a call to `alphaComparePass` so
  the table has one definition and the epsilon cannot diverge again.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
