# #3927: FO4-2026-09-05b-D5-01: `grayscale_to_palette_scale` is a palette-row selector, not a blend weight — the shader treats it as a `mix()` weight and hardcodes the row at `v = 0.5`

Filed from `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D5-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `high,game:fo4,legacy-compat,shaders,renderer,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3927 --json state`.

---

**Source**: `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D5-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: HIGH
- **Dimension**: 5 (FO4 shader flags & BGSM PBR routing) → 7 (NIFAL canonical translation) → renderer
- **Location**:
  `crates/renderer/shaders/triangle.frag` — the lit palette branch guarded by
  `MAT_FLAG_EFFECT_PALETTE_COLOR && mat.greyscaleLutIndex != 0u` (the `#1353 /
  FO4-D8-07` block), and the `MATERIAL_KIND_EFFECT_SHADER` palette block above
  it (the `#890 Stage 2c` block) ·
  `crates/core/src/ecs/components/material.rs` — `Material::grayscale_to_palette_scale` ·
  `byroredux/src/material_translate.rs` — the copy at `translate_material` ·
  `crates/renderer/src/vulkan/material.rs` — `GpuMaterial::grayscale_to_palette_scale`
- **Status**: **NEW.** No open issue matches (`palette` / `grayscale` /
  `greyscale` / `grad` / `lut` over 125 open issues returns only `#3481` and
  `#3308`, neither related). Not covered by `AUDIT_FO4_2026-09-05.md`, which
  reported the *enable bit* never reaching the GPU (`#3897`/`#3898`, both now
  fixed) and stopped there. **Corrects the premise of `#2443`.**
- **Description**:
  Both palette branches read the authored scalar as a lerp weight:

  ```glsl
  float paletteScale = clamp(mat.grayscaleToPaletteScale, 0.0, 1.0);
  texColor.rgb = mix(texColor.rgb, lut.rgb, paletteScale);      // effect path
  ```
  ```glsl
  vec3 paletteColor = texture(textures[nonuniformEXT(mat.greyscaleLutIndex)],
                              vec2(gsIndex, 0.5)).rgb;           // lit path, V pinned
  texColor.rgb = mix(texColor.rgb, paletteColor,
                     clamp(mat.grayscaleToPaletteScale, 0.0, 1.0));
  ```

  The corpus says it is a **texture coordinate**, specifically the V row
  selector into a 2D palette atlas whose U axis is the greyscale ramp:

  * `bricks01grad01.dds` is 32 wide (the ramp) × 128 tall, with **31 of its 32
    block-rows distinct** — 32 authored brick palettes stacked vertically.
  * Exactly **9 vanilla BGSMs** reference it, at **9 distinct scales**
    (0.00, 0.13, 0.20, 0.35, 0.40, 0.50, 0.60, 0.70, 1.00). Nine brick colour
    variants, one atlas, one differing parameter.
  * `hittechmetalpanel_01lgrad.dds`: 48 materials / 12 distinct scales, 31 of 32
    rows distinct. `paintedwoodgrad01.dds`: 20 materials / 11 distinct scales.
  * On the NIF side the same scalar takes **47** distinct values across the 5 441
    palette-enabled LSPs in `Meshes.ba2` and **104** across the 24 725 in
    `MeshesExtra.ba2`. `haircurly1_1bit.bgsm` carries `0.98823535` = **252/255**,
    a byte-quantised index, not a hand-dialled artistic weight.

  A blend-weight reading cannot account for any of this, and it is
  self-contradictory at the endpoints: **48 lit properties and two vanilla BGSMs
  (`combatarmor_leg.bgsm`, `synth_ecorche_dryremap.bgsm`) author `scale = 0.0`
  while also setting the enable bit and supplying a LUT** — i.e. "remap this
  material, with zero remap". Under the row reading, `0.0` is simply palette row 0.
- **Evidence**: measurement sections 4 and 5 above, all taken this run.
  Chain confirming the value reaches the shader unmodified:
  `bgsm.grayscale_to_palette_scale` → `ImportedMaterial.grayscale_to_palette_scale`
  (`byroredux/src/asset_provider/material.rs`) → `Material` (`material_translate.rs`,
  pinned by `translate_material_copies_grayscale_to_palette_scale`) →
  `GpuMaterial` (`byroredux/src/render/static_meshes.rs`) → `mat.grayscaleToPaletteScale`.
  `crates/renderer/src/vulkan/material_tests.rs` pins the field at byte offset 420.
- **Impact**:
  Live **as of `79194306`, committed roughly an hour before this audit** — before
  that commit the lit branch was dead code and this was latent.
  * **30 166** vanilla FO4 `BSLightingShaderProperty` blocks now enter the lit
    palette branch (5 441 in `Meshes.ba2`, 24 725 in `MeshesExtra.ba2`), plus
    **477** BGSM-authored materials.
  * Every material sharing an atlas renders the **same** colour: all 9 brick
    variants, all 48 hi-tech panels, all 20 painted-wood variants collapse onto
    one row. 31 of 32 authored brick palettes are unreachable dead data.
  * `scale = 0.0` materials (combat armour legs, synth ecorché, one power-armour
    and one brick variant) render the **raw greyscale source** — the remap they
    asked for is switched off entirely.
  * Content families affected: architecture brick/panel/wood, vehicle paint,
    power armour and `PA_palette_d`, combat armour, robots (`_lgrad` bot colour
    sets), creature variants, hair.
  * `_PaletteAlpha` is inert on vanilla FO4 (0 authorings measured), so only the
    colour half is implicated.
  * **Not a cross-game regression.** Skyrim SE vanilla authors the bits nowhere:
    `Skyrim - Meshes0.bsa` 0 / 67 105 LSPs, `Skyrim - Meshes1.bsa` 0 / 17 999.
    `#3897`'s ungated lit-path capture is therefore FO4-scoped in practice.
- **Related**: `#3897` / `#3898` (`79194306`, the commit that made this
  reachable) · `#2443` (MAT-D3-01 — captured the scalar on the strength of a
  "soften the ramp" reading this measurement contradicts) · `#1353` /
  *FO4-D8-07* (the lit branch) · `#890` Stage 2c (the effect branch and its
  `v = 0.5` rationale, which was reasoned about **Skyrim FX atlases**, not FO4
  architecture atlases) · `#2997` (slot 3 → `GreyscaleLut`) ·
  `AUDIT_FO4_2026-09-05.md` `FO4-2026-09-05-D5-01`.
- **Suggested Fix**: sample the lit branch at
  `vec2(gsIndex, clamp(mat.grayscaleToPaletteScale, 0.0, 1.0))` and drop the
  `mix()` (the remap is a replace, as the branch's own `#1353` comment already
  says), keeping the `greyscaleLutIndex != 0u` guard. **Do not touch the
  `MATERIAL_KIND_EFFECT_SHADER` branch in the same change** — its `v = 0.5` has
  a separate documented rationale for Skyrim's semantically-1D 64×64 FX atlases,
  and `paintedwoodgrad01.dds` shows genuinely-1D FO4 atlases exist too (for which
  any V is equivalent, so the fix is a no-op there rather than a risk). Land it
  behind the existing FO4 bench cell and confirm on `bricks01grad01`-backed
  geometry that the nine variants separate; this is a visible-output change, so
  it wants a screenshot diff, not only a unit test.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
