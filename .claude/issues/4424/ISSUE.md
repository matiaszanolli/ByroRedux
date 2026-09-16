# #4424: FO4-D5-2026-09-16-01: FO4's `_s.dds` smooth-spec map is bound as both the gloss map and the specular-colour map, and as BC5 it zeroes the blue channel of every direct highlight

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4424
- **Labels**: high,nifal,nif,renderer,shaders,game:fo4,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: HIGH (wrong rendering on essentially all lit FO4 content; the decision tree says "affects rendering correctness → at least HIGH")
- **Dimension**: 5 (slot routing) and 2 (BGSM texture roles)
- **Location**:
  - `crates/nif/src/import/material/slot_role.rs:468-470`: `(TextureSlotLayout::Fallout4, 7) => Some(TextureRole::Specular)`
  - `byroredux/src/asset_provider/material/merge.rs:621-628`: BGSM `smooth_spec_texture` → `textures.smooth_spec`
  - `byroredux/src/render/static_meshes.rs:510` (`gloss_map_index = texture_indices.smooth_spec`) and `:932` (`supplemental_texture_indices[slot::SPECULAR] = texture_indices.specular`)
  - `crates/renderer/shaders/triangle.frag:491-496` (`specColor *= texture(specularMapIndex).rgb`) and `:1437-1453` (`roughness = mix(1.0, roughness, glossTexel.r)`)
  - `crates/renderer/shaders/include/lighting.glsl:326-327` (`specular * specStrength * specColor * NdotL`, on every path including `PBR_BSDF`)
- **Status**: NEW. Introduced by the #2998 fix. It contradicts #3234's premise: #3234 fixed the REFR overlay because binding `smooth_spec_texture` into `specular` put "a grayscale-ish smoothness mask" into the specular-colour channel. The NIF importer does exactly that on FO4.
- **Description**: On FO4, the BGSM field `smooth_spec_texture` and the NIF `BSShaderTextureSet` slot 7 name the **same file**, the `_s.dds` map. The two sources route it to different canonical roles:
  - The NIF importer sends slot 7 to `Specular`. #2998 made this unconditional on FO4.
  - `merge_external_material` sends the BGSM field to `SmoothSpec`.
  - Neither role is empty, so neither first-wins fill blocks the other.

  The same texture then drives two unrelated shader inputs:
  - `glossMapIndex`: `.r` modulates roughness.
  - `specularMapIndex`: `.rgb` multiplies the specular colour of every direct-light specular lobe.

  The file is BC5 (two-channel RG), and Vulkan samples BC5 as `(R, G, 0, 1)`. So `specColor.b` is forced to 0 on every such fragment, and `specColor.r`/`.g` are scaled by two different data channels.
- **Evidence** (measured on installed data):
  - `Fallout4 - Meshes.ba2`, 75,853 shapes whose shader names a resolvable `.bgsm`. 46,382 have both a slot-7 `specular` role and a BGSM-chain `smooth_spec`. In **45,974** the two paths are the same file after normalizing slashes and the `textures\` prefix. The other 393 are also `_s.dds` smooth-spec maps.
  - Sampled every 7th `_s.dds` across `Fallout4 - Textures1..9.ba2` (5,396 total): **794 of 795 are DXGI 83 (BC5_UNORM)**. One is BC1.
  - The two channels really do differ: over 600 files, mean |R−G| = 0.159 and corr(R,G) = 0.37. The texture is two data channels, not a grey mask repeated three times.
  - #2998's own numbers put ~757k slot-7 bindings on FO4 and ~50k of them on properties with no BGSM. Those shapes get only the specular-colour binding.
- **Impact**: Direct specular highlights on FO4 architecture, clutter, actors and weapons lose their blue component and pick up a per-texel red/green tint. They read warm or yellow, and the colour does not come from any authored value. The gloss path reads `.r` of the same texture. Which BC5 channel actually holds smoothness is not established here (see Open question), so roughness may also be driven by the wrong channel.
- **Open question (no guess made)**: the repo contradicts itself on the `_s.dds` layout. `crates/bgsm/src/bgsm.rs` says "smoothness in alpha, specular RGB". `merge.rs:621` says ".r encodes per-texel specular strength". The file itself has no alpha or blue channel. A correlation of each channel against the authored BGSM `smoothness`/`specular_mult` was inconclusive (|r| < 0.16). An authoritative FO4 source for the channel layout is needed before the gloss sampler is touched.
- **Related**: #2998 (introduced the routing), #3234 (opposite premise), #3085 (FO76 slot 6), NIFAL-D8-2026-09-16-06 (a doc note on the same `specular` role), #1476
- **Suggested Fix**: Decide what FO4's slot 7 means at the NIF boundary. It is the same resource BGSM calls `smooth_spec`, so route `(Fallout4, 7)` to the smooth-spec role rather than `Specular` (`TextureRole` currently has no `SmoothSpec` variant; one would need to be added). The two sources then agree and first-wins dedupes them. If a specular-colour reading is kept for non-BGSM content, do not multiply a two-channel texture into an RGB colour. Add a corpus-backed test asserting that an FO4 slot-7 `_s.dds` and the BGSM `smooth_spec_texture` resolve to one role.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SHADER-SYNC**: `triangle.frag` and `include/ray_hit.glsl` (`rayHitAlbedo`) change in lockstep; `.spv` recompiled with plain `-V`
- [ ] **TESTS**: A regression test pins this specific fix
