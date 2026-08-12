# Renderer Audit — 2026-08-12

> **SCOPE — PARTIAL.** Only **Dimension 6** (NIFAL material canonical
> translation) and **Dimension 7** (Material table / R1 dedup) were run, as a
> focused pass of the `texture-roles-deep` audit suite. This is **NOT** a full
> 23-dimension renderer audit despite the filename. Dimensions 1–5 and 8–23
> were not examined in this pass. (An earlier sweep at 11:44 today covered the
> other dimensions in `/tmp/audit/renderer/` scratch files but never produced a
> merged report; those findings are not incorporated here except where they
> overlap Dims 6/7 — see "Prior-pass reconciliation".)

- **Date**: 2026-08-12
- **HEAD**: `8a404914`
- **Depth**: deep (data-flow traced, invariants validated)
- **Dedup baseline**: `/tmp/audit/issues.json` — 258 issues, open + closed
- **Scratch**: `/tmp/audit/renderer/dim_6.md`, `/tmp/audit/renderer/dim_7.md`
- **Focus of this preset**: the renderer as the **consumption end** of the
  post-2026-07-27 texture-role vocabulary — `GpuMaterial`'s Rust↔GLSL mirror,
  the twelve supplemental role indices, R1 dedup-hash coverage of the new
  fields, and the absence of render-time per-game classification.

---

## Headline result — `GpuMaterial` Rust ↔ GLSL field-for-field: **CLEAN**

The primary question this preset exists to answer. Compared programmatically
(field extraction + positional diff), not by eye, because a same-size
reordering is the failure mode a size pin cannot catch:

| Side | Source | Fields | Size |
|---|---|---|---|
| Rust | `crates/renderer/src/vulkan/material.rs` — `pub struct GpuMaterial` (`#[repr(C)]`) | 87 | 348 B |
| GLSL | `crates/renderer/shaders/include/bindings.glsl` — `struct GpuMaterial` | 87 | 348 B (std430) |

- **Order** identical across all 87 positions — zero transpositions.
- **Names** identical modulo snake_case ↔ camelCase, including the digit-group
  cases (`decal_map_0_index` ↔ `decalMap0Index`).
- **Types** identical under `f32`↔`float` / `u32`↔`uint` — 87/87.
- Every member is a 4-byte scalar and there is no `[f32;3]` anywhere, so std430
  member offsets equal the `#[repr(C)]` offsets and the array stride is 348 on
  both sides.
- The twelve supplemental role indices occupy 300…344 on both sides and are
  **individually** offset-pinned by
  `gpu_material_field_offsets_match_shader_contract`, with the total pinned by
  `gpu_material_size_is_348_bytes`.
- **Sentinel**: index `0` = "no texture", and `crates/renderer/shaders/triangle.frag`
  tests it explicitly at every site it reads a supplemental role — including the
  four-entry decal loop guarded by `handle != 0u`.

**R1 dedup-hash coverage of the new fields: CLEAN.**
`hash_gpu_material_fields` walks 87/87 struct fields in declaration order
(verified programmatically — empty symmetric difference), and
`DrawCommand::material_hash` mirrors the same sequence, covering the roles via
`for texture_index in self.supplemental_texture_indices`. A hash predating the
348 B growth would have collapsed materials differing only in a supplemental
role; it does not.

**`GpuInstance` five-site lockstep (checked opportunistically): CLEAN.** All of
`crates/renderer/shaders/include/bindings.glsl`,
`crates/renderer/shaders/triangle.vert`, `crates/renderer/shaders/ui.vert`,
`crates/renderer/shaders/water.vert` and
`crates/renderer/shaders/caustic_splat.comp` declare an identical 13-field
struct — same names, types and order.

---

## Summary

**3 findings** — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 2 LOW.

| ID | Sev | Dimension | Title | Status |
|---|---|---|---|---|
| REN-D6-01 | MEDIUM | NIFAL Material | Effect-shader glass-carrier promotion lost its texture-keyword arm, and the test pinning the FO4 direction was inverted in place rather than kept beside the new one | NEW |
| REN-D7-01 | LOW | Material Table | `ScratchTelemetry`'s R1 doc block mis-states both the value it holds and the console command that surfaces it | NEW |
| REN-D7-02 | LOW | Material Table / GPU-Struct Layout | Three of the twelve supplemental role lanes are produced, uploaded and hashed but sampled by no shader, with the deferral recorded only in a one-off audit report | NEW |

No CRITICAL or HIGH finding. In particular the two failure modes this preset
was aimed at — a silent `GpuMaterial` field reordering, and a dedup hash blind
to the supplemental roles — are both **absent**.

---

## Findings

### REN-D6-01: Effect-shader glass-carrier promotion lost its texture-keyword arm, and the test that pinned the FO4 direction was inverted in place rather than kept beside the new one
- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/helpers.rs` — `classify_glass_into_material`
  (the `effect_glass_carrier` binding); test
  `glass_keyword_does_not_override_effect_shader_semantics` in the same file.
  Introduced by `322f33a8` (2026-08-10).
- **Status**: NEW
- **Description**: `classify_glass_into_material` is the alpha-aware glass
  classifier `translate_material` invokes after `resolve_pbr` — the last
  decision taken at the NIFAL boundary. Its effect-carrier arm now reads
  `material_kind == MATERIAL_KIND_EFFECT_SHADER && bgem_glass`; before
  `322f33a8` it read `… && (keyword_match || bgem_glass)`. Since `bgem_glass`
  can only be true when an external `.bgem` resolved and passed
  `bgem_uses_glass_behavior`, the promotion is now structurally unreachable for
  any `BSEffectShaderProperty` material with no external material file, and for
  BGEM-backed materials it is reachable only through that heuristic — never
  through the explicit semantic name/texture.

  The change itself is deliberate and well-motivated (Skyrim's alchemy-bench
  `InnerHaze` effect layers share `plainglasstile01.dds` with the surrounding
  shells; promoting them to glass erased their emission — the function's doc
  comment now says exactly this). The defect is that the **only** regression
  guard covering the opposite direction was rewritten in place instead of kept
  alongside: the prior test *glass_keyword_promotes_effect_shader_carrier*
  (FO4 `NukaCola_Glass:3` / `nukacola_glass.dds`, whose doc comment stated that
  FO4 commonly authors ordinary glass on a `BSEffectShaderProperty` with no
  BGEM glass flag) was renamed, its fixture swapped to `InnerHaze01:8`, and its
  assertions flipped. Nothing pins the FO4 behaviour in either direction now.
- **Evidence**:
  ```rust
  // byroredux/src/helpers.rs — live
  let keyword_match = texture_path.is_some_and(is_glass_keyword_path)
      || mesh_name.is_some_and(is_glass_keyword_path);
  let effect_glass_carrier =
      material.material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER && bgem_glass;
  ```
  `git show 322f33a8 -- byroredux/src/helpers.rs` removes
  `&& (keyword_match || bgem_glass)`, renames the test, and deletes
  `assert_eq!(m.material_kind, GLASS)`,
  `assert_eq!(m.roughness, GLASS_SURFACE_BEHAVIOR.roughness)` and
  `assert_eq!(m.ior, GLASS_SURFACE_BEHAVIOR.ior)`. `keyword_match` is still
  computed and now feeds only the non-effect arms.
- **Impact**: A cross-game divergence decided by a game-shaped rather than
  source-shaped discriminator, at the one boundary with all-game blast radius
  and no per-draw fallback to mask it. FO4/FO76 effect-shader glass whose BGEM
  misses `bgem_uses_glass_behavior`'s heuristic bundle renders as an effect
  surface with no dielectric IOR path. Magnitude is unmeasured — it depends on
  real BGEM authoring and no game archives were read — but the guard that would
  catch a regression in either direction no longer exists.
- **Related**: #2626 (`bgem_uses_glass_behavior` treats the raw refraction bit
  as an unconditional glass signal — the same predicate from the other side);
  #2477.
- **Suggested Fix**: Do not restore the bare keyword arm. Re-add it gated on
  external-material provenance (`keyword_match && from_bgsm`) — the signal that
  separates Skyrim's inline `BSEffectShaderProperty` (no `.bgem` exists
  pre-FO4) from FO4+ BGEM carriers — and restore the deleted Nuka-Cola
  assertions as a **second** test beside the `InnerHaze` one so both directions
  are pinned.

---

### REN-D7-01: `ScratchTelemetry`'s R1 doc block mis-states both the value it holds and the console command that surfaces it
- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/core/src/ecs/resources/mod.rs` — the doc comments on
  `ScratchTelemetry::materials_unique` and `materials_interned`. The phantom
  command name recurs twice in `crates/renderer/src/vulkan/material.rs` (a test
  doc comment and one assertion message).
- **Status**: NEW
- **Description**: Two independent inaccuracies in the one doc block a future
  R1 change is meant to be checked against.
  (a) `materials_unique` is documented as "(== `MaterialTable::len()`)", but
  `byroredux/src/main.rs` assigns `self.material_table.unique_user_count()`,
  which deliberately excludes the seeded neutral default at slot 0 (`len() - 1`)
  so the #780 dedup-ratio signal isn't skewed on no-user-material frames. The
  `unique_user_count` doc in `crates/renderer/src/vulkan/material.rs` states
  this correctly, so the two docs contradict each other.
  (b) Both fields are documented as displayed by the `mat.stats` console
  command. No such command exists — `byroredux/src/commands/world_info.rs`
  registers `"ctx.scratch"`, and that handler is what prints the
  `materials: N unique / M interned (R× dedup)` line and the overflow tail.
  This is the same shape as the skill's REN-LOW L-1 / L-6 notes about
  `mem.stats` / `mem`, recurring under a different phantom name.
- **Evidence**:
  ```
  crates/core/src/ecs/resources/mod.rs   "/// (== `MaterialTable::len()`). Pairs with `materials_interned` …"
  crates/core/src/ecs/resources/mod.rs   "/// the `mat.stats` console command. A drop here flags a regression"
  byroredux/src/main.rs                  tlm.materials_unique = self.material_table.unique_user_count();
  byroredux/src/commands/world_info.rs   "ctx.scratch"   ← the only registration
  ```
  `grep -rn '"mat.stats"'` over the tree returns zero registrations; all three
  textual hits are doc comments or an assertion message.
- **Impact**: Documentation only — but this block is the stated contract for
  the dedup-ratio telemetry that exists specifically to catch a silent R1
  regression (alignment hole, non-deterministic float in the producer) before
  VRAM pressure shows it. A reader who cannot find the command, or who derives
  the ratio against the wrong denominator, misreads that signal.
- **Related**: #2273 (the sibling stale field-count in the same subsystem's
  docs — see "Prior-pass reconciliation").
- **Suggested Fix**: Change the `materials_unique` doc to
  "== `MaterialTable::unique_user_count()` (`len()` minus the seeded neutral
  default at slot 0 — see #1032)", and replace all three `mat.stats` mentions
  with `ctx.scratch`.

---

### REN-D7-02: Three of the twelve supplemental role lanes are produced, uploaded and hashed but sampled by no shader, with the deferral recorded only in a one-off audit report
- **Severity**: LOW
- **Dimension**: Material Table / GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::lighting_map_index` / `flow_map_index` / `wrinkle_map_index`
  and `supplemental_texture_slot::{LIGHTING, FLOW, WRINKLE}`); GLSL mirror in
  `crates/renderer/shaders/include/bindings.glsl`.
- **Status**: NEW (documentation/telemetry gap — the behaviour itself is a
  deliberate deferral; see the disprove attempt)
- **Description**: Of the twelve supplemental roles added in the 300 → 348 B
  growth, nine are read by `crates/renderer/shaders/triangle.frag`. Three are
  read by nothing: `lightingMapIndex`, `flowMapIndex` and `wrinkleMapIndex`
  appear in the struct declaration and nowhere else under
  `crates/renderer/shaders/`. They are fully live on the producer side:
  - `lighting` is populated from `BSEffectShaderProperty.lighting_texture` by
    `MaterialInfo::texture_set` (`crates/nif/src/import/material/mod.rs`);
  - `lighting`, `flow` and `wrinkle` are all populated by
    `merge_external_material` from `bgsm.lighting_texture` /
    `bgsm.flow_texture` / `bgsm.wrinkles_texture`, and `lighting` again from
    `bgem.lighting_texture` (`byroredux/src/asset_provider/material.rs`);
  - all three are resolved to bindless handles by `map_secondary_texture_handles`
    (`byroredux/src/asset_provider/texture.rs`), so the referenced DDS is loaded
    and uploaded for a map nothing samples;
  - all three are hashed into the dedup key by both `hash_gpu_material_fields`
    and `DrawCommand::material_hash`, so two otherwise-identical materials
    differing only in an unsampled lane occupy two `MaterialTable` slots and
    render byte-identically.

  Neither the Rust struct comment ("Supplemental semantic texture roles …
  source-format agnostic") nor the GLSL one ("Common supplemental semantic
  texture roles (offsets 300-344). Source-game slot numbering has already been
  translated away.") flags the three dead lanes.
- **Evidence**:
  ```
  $ grep -rl lightingMapIndex crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl flowMapIndex     crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl wrinkleMapIndex  crates/renderer/shaders/   → include/bindings.glsl        (only)
  $ grep -rl tintMapIndex     crates/renderer/shaders/   → include/bindings.glsl, triangle.frag
  ```
- **Disprove attempt (partly successful — narrowed, not dropped)**: the
  deferral IS deliberate and IS written down — but only in a prior audit
  report. `docs/audits/AUDIT_RENDERER_2026-07-28.md` states that these three
  "are imported, uploaded, hashed, and mirrored in GLSL but deliberately
  unsampled pending coordinate/actor-control semantics." So this is not a
  silent content drop, and the severity is LOW rather than MEDIUM. What remains
  is that a one-off report is not a code contract: the sibling FO4 audit
  (`docs/audits/AUDIT_FO4_2026-08-12.md`) tabulates all three as fully wired
  BGSM→`GpuMaterial` lanes with a blank remarks column — the deferral has
  already failed to propagate once, today.
- **Impact**: Per authored lane, one otherwise-unused DDS upload (VRAM +
  archive decompress at cell load) and one dedup-key lane that can split an
  otherwise-shared material. No visual corruption. The bookkeeping risk is the
  real one: the repo files issues for exactly this shape when the deferral
  comment is missing (#2642, "parsed with no `MaterialTextureSet` role **and no
  deferral comment**").
- **Related**: #2627 (BGSM `inner_layer_texture` — a populated role never wired
  by `merge_external_material`, the mirror-image gap); #2642; #2594;
  `docs/audits/AUDIT_RENDERER_2026-07-28.md`.
- **Suggested Fix**: Put the deferral in the code, not just the report — a note
  on the three Rust fields and the matching GLSL block naming the blocking work,
  mirroring how `Material::fresnel_power` records "captured, not yet shaded
  (#2284)". If the upload cost is unwanted meanwhile, gate the three `slot(…)`
  calls in `map_secondary_texture_handles` behind the same note rather than
  resolving handles no shader can reach.

---

## Prior-pass reconciliation

An earlier renderer sweep today (11:44, at `efc089ba`) left
`/tmp/audit/renderer/dim_6_7_17.md` covering Dimensions 6, 7 and 17 but never
merged a report. Its Dim-6/7 claims were re-verified against the live tree at
`8a404914` (the two intervening commits touch `boot.rs`, the ECS scheduler and
audio only — no material, renderer or shader file changed).

- **Confirmed and carried forward**: its Dim-6 finding on
  `classify_glass_into_material` (re-derived from live code plus
  `git show 322f33a8`) — reported above as REN-D6-01.
- **Corrected as a dedup miss**: its Dim-7 finding filed the stale "75 live
  scalar fields" phrase as **NEW**. That is a duplicate of **OPEN #2273**
  ("Stale field-count in `MaterialTable::intern`'s collision-policy comment"),
  whose body names the exact site and the exact 75 → 87 correction (verified in
  `.claude/issues/2273/ISSUE.md`). Worth appending to #2273 rather than filing
  new: the identical phrase occurs a second time in the same file, on the
  `hash_gpu_material_fields` doc comment, which #2273's location field does not
  cover.
- **Broadened**: the second half of that finding (the `materials_unique` doc)
  lives in a different crate and is genuinely uncovered — carried forward and
  extended with the phantom-command half as REN-D7-01 above.
- **Not covered here**: its Dimension 17 findings (Disney BSDF / PBR gating /
  soft shadows). Dimension 17 was outside this pass's `--focus`; those remain
  unmerged in the scratch file.

## Findings NOT re-filed (already tracked)

| Observation | Tracker |
|---|---|
| The Rust↔GLSL lockstep pins names, order and Rust offsets but never the GLSL scalar *type* | **#2688** OPEN — the gap that would let today's clean comparison rot silently |
| `gpu_instance_layout_tests.rs`'s `GpuMaterial` field-order test doc still quotes the pre-growth 300 B size | **#2415** OPEN |
| `gpu_instance_does_not_re_expand_with_per_material_fields` is a no-op test citing a stale byte size | **#2433** OPEN |
| Save-restore is a renderer-bound `Material` producer that runs neither `resolve_pbr()` nor finiteness validation | **#2687** OPEN |
| Terrain / terrain-LOD / object-LOD draw populations never reach `translate_material` | **#2444** OPEN |
| Per-draw normal-alpha-as-spec gloss binding / roughness write-back outside `translate_material` | **#2445**, **#2572**, **#2330** OPEN |
| Starfield `MaterialTextureSet` fill rate is 0 of 18 roles on vanilla content | **#2359** OPEN (sibling suite) |
| Importer `BSShaderTextureSet` slot→role errors (MultiLayerParallax slot 7 vs 6; FaceTint empty slots) | sibling NIFAL audit this suite — import-side; the renderer consumes whatever arrives and does not worsen it |
| `RefrTextureOverlay.inner` written by TXST/XTXR, read by nobody | sibling NIFAL audit this suite; the renderer-side analogue is REN-D7-02 |

---

## Guards verified intact

**Dimension 6 — NIFAL material boundary**
- **Single boundary**: `translate_material` has exactly two production callers
  — `byroredux/src/scene/nif_loader.rs` and `byroredux/src/cell_loader/spawn.rs`.
  The other four call sites are `#[cfg(test)]` in
  `byroredux/src/material_translate.rs`.
- **Third-producer sweep**: `Material {` literals across `byroredux/src` +
  `crates/core/src` resolve to `byroredux/src/cornell.rs` (`--cornell`
  harness), `byroredux/src/commands/scene.rs` (`mat.*` live edit),
  `byroredux/src/save_io.rs` (Existing #2687), test fixtures, and unrelated
  `WaterMaterial` / `ResolvedMaterial` types. No new leak.
- **`metalness` / `roughness` are plain resolved `f32`**: `resolve_pbr` has one
  production call site (`byroredux/src/material_translate.rs`); the rest are
  tests in `crates/core/src/ecs/components/material.rs`, including
  `resolve_pbr_is_idempotent`.
- **No per-frame re-classification**: `classify_pbr_keyword` is reached only
  from `Material::resolve_pbr` and the import-time classifier in
  `crates/nif/src/import/material/mod.rs`. Nothing under `byroredux/src/render/`
  calls it; `byroredux/src/render/static_meshes.rs` destructures `m.roughness` /
  `m.metalness` / `m.ior` directly, with an explicit "no per-draw keyword scan /
  classify_pbr fallback" comment. The only per-draw material logic left is the
  normal-alpha-as-spec gloss-slot binding behind the shared
  `normal_alpha_spec_applies` predicate (Existing #2445 / #2572 / #2330).
- **`EmissiveSource` resolved at translate** and copied to the canonical
  `Material`; the renderer reads the resolved `emissive_mult`.
- **`merge_external_material` signature still narrowed** to
  `&mut ImportedMaterial` — geometry, transforms, skinning and scene ownership
  remain unreachable from a sidecar merge.
- **`pack_imported_material_flags`** is the live flag packer
  (`byroredux/src/cell_loader.rs`), with per-bit tests for PBR_BSDF /
  TRANSLUCENCY / MODEL_SPACE_NORMALS / THIN_GLASS / EFFECT_PALETTE_COLOR. No
  residual *pack_material_flags* / *pack_bgsm_material_flags*.
- **Zero per-game branches in the shaders**: grepping `game ==` / `GAME_` /
  `isFNV` / `isSkyrim` / `isFO4` across `crates/renderer/shaders/*.frag` and
  `crates/renderer/shaders/include/*.glsl` returns nothing.
- **Particle slice**: `apply_emitter_params` (`byroredux/src/systems/particle.rs`)
  applies kinematics + lifetime + size and leaves colour to the curve overlay;
  `apply_emitter_params_overrides_kinematics_and_size_not_color` present.

**Dimension 7 — Material table / R1 dedup**
- All-scalar `#[repr(C)]`; `Hash`/`Eq` over the raw-bytes view, so the std430
  vec3-alignment desync hazard is structurally absent.
- Dedup-hash completeness 87/87 in declaration order on both walks, lockstep
  pinned by `material_hash_matches_gpu_material_field_hash`.
- Slot 0 is the seeded neutral default (`seed_neutral_default`, #807), re-seeded
  by `clear()`.
- Over-cap `intern_by_hash` routes to **id 0**, increments `overflow_count`, and
  warns exactly once through the `INTERN_OVERFLOW_WARNED` `Once` guard — no
  SSBO-index corruption.
- Debug builds construct the material even on a dedup hit and assert byte
  equality, so a producer-hash drift panics rather than silently mis-colouring.
- `upload_materials` (`crates/renderer/src/vulkan/scene_buffer/upload.rs`)
  `debug_assert!`s that `intern` already capped, then uploads
  `materials.len().min(MAX_MATERIALS)`; buffers sized for `MAX_MATERIALS`.
- Dedup-ratio telemetry (#780) wired end to end: `byroredux/src/main.rs` →
  `ScratchTelemetry` → `byroredux/src/commands/world_info.rs` (`ctx.scratch`),
  printing unique / interned / ratio plus the `OVERFLOW … → id 0` tail. (Its
  doc comment is wrong — REN-D7-01.)
- Import-side scalar guards present: the #1241 BGSM smoothness/subsurface suite
  and the #1243 `WaterShaderProperty` + #1244 `BSShaderPropertyBaseOnly`
  distinct-entry regression module, both in
  `crates/nif/src/import/material/mod.rs`.
- No per-instance field that should now live in `GpuMaterial`: `GpuInstance`
  retains only `texture_index` (UI-quad path, documented) and the per-draw `ior`
  (consumed by `crates/renderer/shaders/caustic_splat.comp`, documented).
- Particle colour-fade quantization (#1795): `COLOR_FADE_STEPS` and
  `quantize_fade` live in `byroredux/src/render/particles.rs`; `emit_particles`
  snaps only the colour LERP parameter and leaves the size LERP continuous.
  Four tests in `quantize_fade_tests`.

---

## Coverage and limits

**Files read.** Dim 6: `byroredux/src/material_translate.rs`,
`byroredux/src/helpers.rs`, `byroredux/src/scene/nif_loader.rs` and
`byroredux/src/cell_loader/spawn.rs` (call sites),
`byroredux/src/cell_loader.rs`, `byroredux/src/asset_provider/material.rs`,
`byroredux/src/asset_provider/texture.rs`, `crates/nif/src/import/types.rs`,
`crates/nif/src/import/material/mod.rs`,
`crates/core/src/ecs/components/material.rs`,
`byroredux/src/render/static_meshes.rs`, `byroredux/src/systems/particle.rs`.
Dim 7: `crates/renderer/src/vulkan/material.rs`,
`crates/renderer/shaders/include/bindings.glsl`,
`crates/renderer/shaders/triangle.frag`, the four standalone `GpuInstance`
mirrors, `crates/renderer/src/vulkan/context/mod.rs`,
`crates/renderer/src/vulkan/scene_buffer/upload.rs`,
`byroredux/src/render/particles.rs`, `byroredux/src/commands/world_info.rs`,
`byroredux/src/main.rs`, `crates/core/src/ecs/resources/mod.rs`.

**Not done.** Dimensions 1–5 and 8–23 were not run. No `cargo test` execution
(layout and lockstep pins were read, not run), no engine launch, no RenderDoc
capture, and no game data read — so the real-world prevalence behind REN-D6-01
(FO4 BGEM glass authoring) and REN-D7-02 (authored `lighting` / `flow` /
`wrinkle` maps) is unmeasured in both cases, and stated as such in the findings.

---

## Next step

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-08-12.md
```
