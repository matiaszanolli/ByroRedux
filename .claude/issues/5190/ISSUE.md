# #5190 — REN-D6-2026-10-03-01: CDB `TextureReplacement` is collapsed slot-agnostically into `diffuse_color`, so any slot's flat replacement (normal, roughness, AO, …) tints the albedo, even when a colour texture is also bound

**Labels**: high,nifal,renderer,import-pipeline,game:starfield,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: HIGH. The floor applies: a wrong `Material` comes out of the boundary on Starfield. Confidence in the per-slot semantics is code-structural plus external, and is stated below.
- **Dimension**: NIFAL Material
- **Location**: `crates/sfmaterial/src/index.rs` (`MaterialIndex::capture_instance` `"BSMaterial::TextureReplacement"` arm; `MaterialIndex::collect_slots`); `byroredux/src/asset_provider/material/merge.rs` (`apply_cdb_material`)
- **Status**: NEW. Related: #3398 (open umbrella). No issue mentions TextureReplacement or the CDB flat colour.
- **Description**:
  - The CDB `Components` table carries an `Index` per `(object, type)`. For `MRTextureFile` that `Index` is the texture slot. The code uses it that way (`self.textures…push((row.index, path))`), and census 1 of the `cdb_join_probe` example confirmed slot meaning by filename suffix.
  - A `TextureReplacement` lives on the same texture-set object with its own `Index`. The synthetic fixture models this as `(13, 13, 0)` beside `(13, 10, 0/1/3)`. But `capture_instance` drops `row.index` for this class and stores `flat_colors: HashMap<u32 /*object*/, (rgba, enabled)>` with `or_insert`. The first replacement on the texture set wins, whatever slot it replaces.
  - `apply_cdb_material` then writes it unconditionally: `material.diffuse_color = [r, g, b]`. It does this even when the same lookup also filled `textures.base_color` (`SLOT_COLOR`).
  - `triangle.frag` multiplies `albedo *= vec3(mat.diffuseR, mat.diffuseG, mat.diffuseB)` (the same product appears at the RT hit sites). The flat colour is therefore applied as a tint over the texture, not "INSTEAD of any texture" as the field doc and the comment in `apply_cdb_material` claim.
  - In Starfield's CE2 material model, each slot can carry its own replacement colour (fo76utils/NifSkope `CE2Material::TextureSet::textureReplacements[]` plus `textureReplacementMask`). That source is external and not checked into `/mnt/data/src/reference`. A flat-normal (≈0.5, 0.5, 1.0), roughness or AO replacement therefore becomes a blue, grey or white albedo tint.
- **Evidence**: `capture_instance` reads `"BSMaterial::TextureReplacement" => { … self.flat_colors.entry(row.object).or_insert((color, enabled)); }`, and `row.index` is unused in that arm. The guarding test does not catch it: `mat_path_merges_cdb_authored_textures_when_indexed` asserts that `base_color` is bound **and** that `diffuse_color == [0.25, 0.5, 0.75]`, which pins the texture × flat-colour product.
- **Impact**: For Starfield only, any material whose first-walked texture set carries a non-colour-slot replacement gets a fabricated albedo tint. The commit's own census counts 36,866 replacement instances. Their slot distribution is unmeasured.
- **Trigger / verification**: Extend census 4 of the `cdb_join_probe` example to key `TextureReplacement` by `Components.Index`. If any instances sit on slot ≠ 0, or the slot-0 replacements coexist with a slot-0 `MRTextureFile`, the finding is confirmed. This audit did not run the probe: available RAM was about 7 GB, and the memory note says to avoid big-RSS probes in-session.
- **Related**: REN-D6-2026-10-03-02, #3398, nifal.md parked-slot table (#4429).
- **Suggested Fix**: Capture `(row.index, color, enabled)` and keep the replacement per slot. Translate only the `SLOT_COLOR` replacement, and only when no colour texture landed in that slot. Park the other slots like the parked texture kinds, or route them to their role once the role exists. Rewrite the test to cover a non-zero-slot replacement.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the CDB merge (`apply_cdb_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
