# #3562 — REN-2026-08-30-D19-01: `#3530` sets `PARALLAX_ALPHA_HEIGHT_BIT` without the `normal_has_alpha` gate its sibling mechanism uses — an alpha-less normal map yields a constant height of 1.0 and the marcher walks the FULL parallax slide

**Labels**: `high,renderer,shaders,game:oblivion,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3562 --json state`.

---

- **Severity**: HIGH
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-285` (`APPLY_HILIGHT2` route), `byroredux/src/render/static_meshes.rs:306-311` (bit transport), `crates/renderer/shaders/include/material_sampling.glsl` (`parallaxDisplaceUV`)
- **Status**: NEW
- **Description**: The `APPLY_HILIGHT2` route binds the **normal** map into the height
  slot and sets `parallax_height_in_alpha` on the sole conditions
  `tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none()` and
  `info.normal_map.is_some()`. Nothing checks whether that normal texture actually
  *has* an alpha channel. Its own sibling mechanism — `NORMAL_ALPHA_SPEC_BIT`, which
  the `#3530` comments cite as the pattern being reused "verbatim" — is gated on
  exactly that signal (`normal_alpha_spec_binding_applies(mat, normal_has_alpha, …)`,
  `material_translate.rs:795-813`; the value comes from
  `texture_registry.handle_has_alpha` → `dds::format_has_alpha`,
  `scene/nif_loader.rs:1100-1103`). The parallax half at
  `static_meshes.rs:306-311` reads `normal_has_alpha` into scope two dozen lines
  earlier (`:291-293`) and does not consult it.
- **Evidence**:
  - `dds::format_has_alpha` (`crates/renderer/src/vulkan/dds.rs:126-140`) returns
    `false` for every BC1/BC4/BC5 variant. DXT1 is decoded as
    `BC1_RGBA_SRGB_BLOCK` (`dds.rs:575`) — 1-bit punch-through, `A == 1.0` on every
    opaque 4-colour block; `ATI2`/BC5 maps to `BC5_UNORM_BLOCK` (`dds.rs:578`), for
    which the sampler returns `A = 1.0` by format.
  - Trace the constant through the raster marcher (`material_sampling.glsl`,
    `parallaxDisplaceUV`): with `sampledHeight == 1.0` the loop guard
    `if (currentDepth >= sampledHeight) break;` never fires, so it runs all `steps`
    iterations and exits with `currentUV = uv - planarSlide`, `currentDepth = 1.0`.
    The secant step then computes `afterDepth = 1.0 - 1.0 = 0.0`,
    `beforeDepth = 1.0 - (1.0 - layerDepth) = layerDepth`,
    `weight = 0 / (0 - layerDepth + 1e-6) ≈ 0`, so
    `mix(currentUV, prevUV, 0) == currentUV`. The returned UV is displaced by the
    **entire** `planarSlide` at every fragment.
  - `planarSlide = V_ts.xy / max(V_ts.z, 0.05) * heightScale` with the
    importer-installed `heightScale = 0.04` (`legacy_properties.rs:281-283`): at
    grazing incidence this reaches ≈0.8 UV units of slide, view-dependent per frame.
  - `sampleUV` is the single UV feeding every subsequent fetch — base, normal, detail,
    glow, gloss, dark, the eight terrain splat layers (`triangle.frag:231-241` and
    downstream), so the whole material slides, not just the height read.
  - The `#3530` route is not niche: its own comment records *"1,433 properties across
    741 distinct vanilla meshes carry it"* (`legacy_properties.rs:256-258`).
- **Impact**: On every Oblivion `APPLY_HILIGHT2` mesh whose normal map lacks a real
  alpha channel, the entire texture set swims with view angle at maximum parallax
  amplitude — the opposite of the intended "no-op when there is no height data".
  The mixed-block BC1 case is worse than either extreme: 3-colour blocks decode
  `A = 0` (instant break, no displacement) while 4-colour blocks decode `A = 1`
  (full displacement), so the surface tears along block boundaries. Both POM marchers
  inherit it identically, so reflections agree with the raster pass — on the wrong
  image.
- **Suggested Fix**: Gate the bit on the same signal `NORMAL_ALPHA_SPEC_BIT` uses.
  The cheapest correct placement is `static_meshes.rs:306-311`, where
  `normal_has_alpha` is already in scope:
  `if parallax_map_index != 0 && normal_has_alpha && mat.is_some_and(|m| m.parallax_height_in_alpha)`.
  Note the canonical-state purist reading argues for resolving it at the NIFAL
  boundary instead — but the DDS format is not known there, which is precisely why
  `normal_has_alpha` is a render-side `MaterialTextureHandles` field and not a
  `Material` field. Add a pin next to
  `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader`.
- **Cross-dimension corroboration**: Independently found a second time as *D2-01* by the SSBO/ray-query dimension, which rated it MEDIUM on reachability grounds and stated the caveat explicitly. Severity arbitrated **up** to HIGH here per the project rubric's *"severity is about IMPACT, not likelihood"* rule: the mechanism is certainly wrong and the failure is maximal-amplitude rather than graceful. The affected population is **uncensused** — Oblivion `_n.dds` are commonly DXT3/DXT5 (which do carry alpha); the BC1/BC5/single-channel subset within the 1,433 `APPLY_HILIGHT2` properties is the exposed set and no Oblivion texture archive was mounted in this session to measure it. Census first, then fix.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D19-01

## Population census (added at publish time)

A sibling audit measured the exposed population on **vanilla Oblivion** and found it
**empty**: 0 of 1,430 `APPLY_HILIGHT2` properties carry a normal/bump slot, and
`Material::parallax_height_in_alpha` is true on 0 of 35,322 meshes. The missing gate is
real as a code defect, but it is **currently unobservable on shipped content** — do not
spend time hunting a visual repro. Fix it as a correctness/robustness change, not as a
visual-bug chase.


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
