# #4044 — REN-2026-09-06-D6-04: the greyscale-palette LUT is still hand-copied at both particle spawn sites — the sibling half of the divergence #3589 closed one day earlier

**Labels**: low, nifal, renderer, shaders, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D6-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/systems/particle.rs`
  (`apply_emitter_overlays`), against the two blocks assigning
  `preset.greyscale_lut_index` in `byroredux/src/scene/nif_loader.rs` and
  `byroredux/src/cell_loader/spawn.rs`
- **Status**: NEW
- **Description**: `84c4a1df` (#3589) routed the BGEM effect-shader payload
  through `apply_emitter_overlays` because packing it with a hand-copied line
  at each spawn site was "the same divergence-risk class #1513 closed this
  helper to prevent". The **texture those flags index** — resolved by #3590 in
  the same delta — was left outside the boundary and is still hand-copied at
  both sites today, immediately after the boundary call. So `effect_shader_flags`
  (the palette *enable* bits) now goes through the single boundary while
  `greyscale_lut_index` (the palette *LUT* those bits select) does not, even
  though they are the two halves of one authored feature.
- **Evidence**: A scan of every `preset.<field> = …` assignment at both spawn
  sites finds exactly one remaining outside the boundary, and it is the same
  expression twice:

  ```rust
  preset.greyscale_lut_index = em            // cell_loader/spawn.rs
      .greyscale_lut_map
      .as_deref()
      .map(|path| resolve_texture(ctx, tex_provider, Some(path)))
      .unwrap_or(0);
  ```
  ```rust
  preset.greyscale_lut_index = emitter       // scene/nif_loader.rs
      .greyscale_lut_map
      .as_deref()
      .map(|path| resolve_texture(ctx, tex_provider, Some(path)))
      .unwrap_or(0);
  ```

  Each carries a "Mirrored in the sibling site" comment — the literal marker of
  the pattern. Meanwhile `apply_emitter_overlays`'s own rustdoc calls it "the
  **single overlay boundary** that folds **every** authored emitter override",
  and its field-by-field paragraph enumerates seven overlays without
  mentioning this one. The load-bearing semantic — `.unwrap_or(0)` so an
  emitter with no authored LUT keeps bindless slot 0 (the shader's "no LUT"
  sentinel) rather than `resolve_texture`'s neutral-fallback handle for an
  absent path — is stated in prose at both sites and enforced by neither.
- **Impact**: Latent, not live: the two copies are byte-identical today, so
  **nothing renders wrong** — which is why this is LOW and not MEDIUM. The
  cost is that the boundary's stated guarantee is false, and the next change
  to LUT resolution has two places to land instead of one. Divergence here
  drops the FO4 greyscale-to-palette remap on one load path only — the remap
  #3897/#3898 measured across 30,166 FO4 shader properties — which is the
  invisible-on-one-path failure this boundary exists to make impossible.
- **Related**: #1513 (the boundary), #2610 / #3589 (the sibling field, fixed
  `84c4a1df`), #3590 (the LUT resolution, landed hand-copied), #3897/#3898
  (the population it affects). Not covered by #3927/#3928/#3929, which are
  about the palette *shader semantics*, not the overlay boundary.
- **Suggested Fix**: Add an eleventh parameter `greyscale_lut: Option<u32>`
  to `apply_emitter_overlays` and move `preset.greyscale_lut_index =
  greyscale_lut.unwrap_or(0)` inside it, leaving only the
  `resolve_texture` call (which needs `&mut VulkanContext`) at the call
  sites — the same shape `84c4a1df` used for `effect_shader`. Extend
  `apply_emitter_overlays_applies_color_rate_size_and_force_fields` and
  `apply_emitter_overlays_none_inputs_keep_preset_defaults` with the new
  field, again exactly as #3589 did, so the `unwrap_or(0)` sentinel is pinned
  once instead of restated twice in prose.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
