**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D5-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: Render & Overlay Upload
- **Profile**: both, plus the ground-cover atlas, which shares the arena
- **Location**:
  - `crates/renderer/src/texture_registry/dynamic_rgba.rs:58-68` (`submitted`) and `:87-96` (`note_staging_skip`)
  - `crates/renderer/src/texture_registry/mod.rs:722-727`
  - `crates/renderer/src/vulkan/context/draw.rs:633`
- **Status**: NEW. This is a defect in the 792c56de3 fix for #4889.
- **Description**:
  - `submitted(slot)` sets `staging_skip_logged = false` unconditionally, on the premise that "a completed frame means
    staging works again".
  - `note_frame_submitted` calls `submitted` after every successful `queue_submit`. That includes the very frame whose
    upload was skipped: the skip returns `Ok`, so the frame records and submits as normal.
  - Under a failure that persists (BAR or host-visible exhaustion), frame N warns and sets the flag. Frame N submits,
    which clears the flag. Frame N+1 warns again.
- **Evidence**: control flow: `begin_frame_recording.rs:81-87` gets `Ok` from the degraded path, then
  `draw.rs:633 note_frame_submitted(frame)`, then `dynamic_rgba.rs:67 self.staging_skip_logged = false`. The pin
  `staging_failures_skip_the_overlay_frame_instead_of_failing_it` only counts call sites.
- **Impact**:
  - One `log::warn!` per frame (60 per second), plus one host-visible allocation attempt of about 8.3 MB (1080p) or
    33 MB (4K) per frame, during exactly the memory-pressure episode the flag was meant to keep quiet.
  - Cosmetic otherwise; the frame still renders.
- **Related**: #4889 (closed)
- **Suggested Fix**:
  - Re-arm only after a recording actually uploaded its dirty set, for example in `record_pending_rgba_uploads` once
    every dirty update reached `recorded_slot = Some(frame)`.
  - Add a `DynamicRgbaUploads` unit test for skip → submit → skip that expects one warning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml (FNV)
- **Location**:
  - `docs/smoke-tests/m48-5-fnv-hud.sh:46`
  - `scripts/check-playable-smoke-contracts.sh:98`
  - Env-rename residue: `docs/smoke-tests/m48-6-skyrim-hud.sh:25-26,34` and `docs/smoke-tests/m48-menu-load.sh:50-51,57`
- **Status**: Incomplete fix of #4724, which was closed on 2026-10-04. Its title includes "the HUD smokes report
  missing data as FAIL".
- **Description**:
  - `m48-5-fnv-hud.sh:46` still does `{ echo "FAIL: missing $DATA/$f"; exit 1; }`. The other four HUD smokes exit 77
    and print `smoke[<name>]: SKIP -- missing`.
  - The checker loop lists `m48-menu-load m48-4-oblivion-hud m48-5-fo3-hud m48-6-skyrim-hud m48-7-fo4-hud`, without
    FNV, so it cannot catch the omission.
  - The FNV smoke was added by 3536794c3 on 2026-09-25, after #4724 was filed, with the old pattern.
  - Residue from the same family (the 63c0aee3b env rename):
    - `m48-6-skyrim-hud.sh:25-26` documents "BYROREDUX_SKYRIMSE_DATA / BYROREDUX_SKYRIMSE_DATA".
    - `m48-6-skyrim-hud.sh:34` and `m48-menu-load.sh:57` nest
      `${BYROREDUX_SKYRIMSE_DATA:-${BYROREDUX_SKYRIMSE_DATA:-…}}`.
    - `m48-menu-load.sh:50-51` contrasts the variable with itself.
- **Impact**: a runner without data reports FAIL instead of SKIP for the FNV HUD gate, the exact outcome that kept
  the HUD smokes out of CI.
- **Related**: #4724 (closed), #5237 (open: the audit-runtime skill's gate matrix omits `m48-5-fnv-hud`)
- **Suggested Fix**: copy the 77 / `SKIP -- missing` block from `m48-5-fo3-hud.sh`, add `m48-5-fnv-hud` to the checker
  loop, and collapse the doubled variable expressions.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: MEDIUM. This uses the "translatable data dropped" row: authored blend state is never captured, and the glass renders
  opaque rather than disappearing. It is the same lineage and impact as #5196, which was MEDIUM.
- **Dimension**: Shader-flags/Effects (CDB merge boundary) → Material
- **Tier Violated**: parked-not-leak (an authored translation that is wired but cannot fire)
- **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:324-327`: `if cdb_mat.is_glass == Some(true) { material.bgem_glass = true; }`;
  - `byroredux/src/material_translate.rs:844-856`: classifier call with coverage `source.has_alpha || source.alpha_test`;
  - `byroredux/src/helpers.rs`: `classify_glass_into_material_with_provenance`, `if !has_transparent_coverage || is_decal { return; }`.
    This runs before the `bgem_glass` check, and only `is_mirror_pane` precedes it;
  - `crates/nif/src/import/material/mod.rs:1398-1417`: `effective_alpha_blend`, whose only source is `NiAlphaProperty`;
  - `crates/sfmaterial/src/index.rs:416-443`: capture of `AlphaSettingsComponent` (threshold and `HasOpacity` only) and
    `EffectSettingsComponent` (`IsGlass` only).
- **Status**: NEW. This is an incomplete fix of #5196 (closed by `978d25c19`). The blend-field capture itself falls within open #3398.
- **Description**: The classifier only promotes to glass when the surface has transparent coverage. That gate is what keeps an
  opaque `PawnShopWindow` out of the glass path, and it applies to the authoritative `bgem_glass` input too.
  - FO4 faced the same problem and solved it at the merge boundary: the BGSM and BGEM arms forward the material file's
    `alpha_blend_mode` into `has_alpha` (`merge.rs:1266-1271`, `:1560-1564`). The comment there reads "FO4+ moved per-material
    blend state out of NiAlphaProperty into BGSM … every Institute / lab pane renders fully opaque".
  - Starfield moved blend state into the CDB. `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:176-182` lists
    `AlphaSettingsComponent.Blender` → `AlphaBlenderSettings.Mode` and `EffectSettingsComponent.BlendingMode` (`"AlphaBlend"`).
  - `apply_cdb_material` captures neither field. It sets `alpha_test` only from `AlphaTestThreshold`, so CDB glass reaches the
    classifier with `has_alpha = false`, and the early return fires before `bgem_glass` is consulted.
- **Evidence**:
  - Per-block baseline (`crates/nif/tests/data/block_coverage_baselines/starfield.tsv`): 884 `NiAlphaProperty` against 190,549
    `BSGeometry`.
  - Full-corpus import census (a scratch tool over `Starfield - Meshes01/02/Patch.ba2` with the `.mesh` resolver, output in
    `/tmp/audit/nifal/sf_glass_census.txt`):
    - 188,936 shapes, of which `has_alpha` is set on 477 and `alpha_test` on 122.
    - **3,449 glass-named shapes** (material path or node name contains `glass`; 3,447 have a `.mat` path), of which only 23 are
      `has_alpha` and 6 are `alpha_test`.
    - Sample: `thelodgeexterior01.nif`'s `materials\…\naglasscleanopaque01_blue01.mat` (×12 sub-shapes).
  - The census cannot say which of those materials carry `IsGlass = true`. The CDB join needs the >10 GB `cdb_join_probe`.
  - The #5196 regression test `is_glass_routes_to_the_classifier_signal_without_thin_shell` asserts only the `bgem_glass` bool,
    never `MATERIAL_KIND_GLASS` out of `translate_material`, so the inert end-to-end path stays green.
- **Impact**: Authored Starfield glass (windows, visors, bottles) still renders as opaque kind-0 dielectric with the CDB colour
  texture: the exact symptom #5196 was filed for ("Authored Starfield glass is mostly not glass"). Any other alpha-blended
  Starfield material (effect cards, foliage cut-outs that use blending rather than test) is opaque for the same reason.
- **Related**: #5196 (closed), #3398 (open), #1823/#1651 (BGSM blend forwarding precedent), #4283, REN-D6-2026-10-03-02.
- **Suggested Fix**: Capture `EffectSettingsComponent.BlendingMode` and `AlphaSettingsComponent.Blender.Mode` in `MaterialIndex`,
  and forward them in `apply_cdb_material` to `has_alpha` plus `src/dst_blend_mode`, as the BGSM arm does (each enum string gets
  an explicit arm and a documented default). Add a merge → `translate_material` test asserting that a CDB `IsGlass` + `AlphaBlend`
  material classifies as `MATERIAL_KIND_GLASS`. If the blend capture is deferred to #3398, reopen #5196 or record the dependency
  there, because the current test certifies a path that cannot fire.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: both
- **Location**:
  - `byroredux/src/hud.rs:462-478`
  - `byroredux/src/scaleform_hud.rs:445-459` and `:62`
  - `byroredux/src/scaleform_hud.rs:591-606` (`the_push_table_skips_reserved_engine_callbacks`)
- **Status**: NEW. The splice is the same class as open #5029.
- **Description**:
  - **Doc splice in `hud.rs`.** `menu_owned_overlay_skip` was inserted between `launch_hud`'s doc comment and
    `launch_hud`. The doc "Launch the HUD when `--hud` is present … registers the transparent overlay textures" now
    heads the helper's doc, and `launch_hud` has none. "textures" is also stale, since #4892 left one texture.
  - **Stale push doc.** The `push` doc still says FO4's lifecycle hooks are "skipped by the prefix guard".
  - **The skip at `:457-459` can never fire.** The `match` at `:460-472` acts only on
    `UpdateStats|updateStats|UpdateCompass|updateCompass`, and its `_ => continue` arm already skips every reserved
    name.
  - **The test asserts nothing.** Its pin pushes into a `UiManager` with no player, where invoke is a no-op, and then
    asserts `RESERVED_ENGINE_CALLBACKS.contains(&RESERVED_ENGINE_CALLBACKS[0])`. Deleting the skip, or the whole push
    body, leaves it green. The skill names this test as the guard for the exact-membership rule.
  - **Spliced enum line.** `scaleform_hud.rs:62` reads `pub(crate) enum ScaleformGame {    /// AVM1 …`, flagged by
    `rustfmt --check`.
- **Impact**: documentation and test quality only. Behaviour is correct today, but the guard would not catch a
  regression.
- **Related**: #4723, #4725 (closed), #5029 (open)
- **Suggested Fix**:
  - Move the launch doc back above `launch_hud` and update both docs.
  - Either drop the redundant skip, or make the test observe it (for example, by counting invoke attempts against a
    loaded player).
  - Fix the spliced enum line.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4, FO76, Starfield
- **Location**: `docs/engine/nifal.md:854-859`
- **Status**: NEW. This is an incomplete sibling of #4441 (closed by `c2b67d81e`, which touched only `material_translate.rs` and
  `material.rs`). The rustdoc half is REN-D6-2026-10-05-01.
- **Description**: The spec says "For NIF-imported content … `Some(…)` is always present and `Material::resolve_pbr()` only clamps —
  its classifier arm (the `NaN` sentinel path) is a backstop for future non-pre-classified sources." Both halves are false today:
  - #2707's Starfield material-reference stubs leave both overrides `None` at NIF import;
  - the BGEM merge deliberately leaves them NaN.

  The paragraph also does not record #5197: a CDB **hit** stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` (`merge.rs:219-236`), so only CDB
  misses and BGEM reach the classifier. This is the deletion-inviting text #4284 and #4441 were filed against, left standing in
  the spec that auditors read first.
- **Evidence**: `grep -n backstop docs/engine/nifal.md` → `:858`. The commit body of `c2b67d81e` lists only the two rustdoc
  statements.
- **Impact**: A contributor working from the spec concludes the classifier arm is dead for current content.
- **Related**: #4441, #4284, #5197, REN-D6-2026-10-05-01, #5210 (the same doc pass).
- **Suggested Fix**: Rewrite step 3 to name the live NaN producers (BGEM, and Starfield stubs whose `.mat` misses the CDB or that
  run with no CDB) and the `NO_SIGNAL_NEUTRAL` CDB-hit outcome. Fold this into #5210 and REN-D6-2026-10-05-01.

**Publish note**: this is the spec half of the same stale claim. The rustdoc half (`material_translate.rs` boundary contract and `resolve_pbr` inline comment) is filed separately as REN-D6-2026-10-05-01 from AUDIT_RENDERER_2026-10-05. Fix both together with #5210's NIFAL doc pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-03) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: both
- **Location**:
  - `byroredux/src/commands/hud.rs:88` (`t.parse::<f32>().map(|v| v.clamp(0.0, 1.0))`)
  - `byroredux/src/commands/hud.rs:130-132` (`deg.rem_euclid(360.0)`)
  - `byroredux/src/hud.rs:829` (`fraction` re-clamps)
- **Status**: NEW
- **Description**:
  - `"nan".parse::<f32>()` succeeds, and `f32::clamp(NaN)` returns NaN, so `hud.values nan nan nan` pins NaN bars.
  - `hud.heading inf` (or `nan`) becomes NaN through `rem_euclid`.
  - The #4724 test comment says "the count and the 0-1 domain are enforced", but it only tests non-numeric junk.
- **Impact**:
  - MenuXml bar and compass geometry turns non-finite, so the raster's non-finite reject draws nothing and the bar
    vanishes.
  - Scaleform pushes `Number(NaN)`.
  - Signature hashing uses `to_bits` and `as i32`, so nothing hangs. The surface is the console/debug server only.
- **Related**: #4724 (closed)
- **Suggested Fix**: reject values that fail `is_finite()` in both parsers, and pin `nan` and `inf`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: none (tech-debt) · **Game Affected**: all (terrain/LOD)
- **Location**: `byroredux/src/material_translate.rs:1038-1067`
- **Status**: NEW (introduced by `235a90ba2`)
- **Description**: `translate_texture_only_material_with_authored_msn(texture_path, model_space_normals, texture_clamp_mode)` does
  nothing except call `translate_texture_only_material_with_authored_msn_and_clamp` with the same three arguments. The texture-only
  boundary now spans `translate_texture_only_material`, `_with_clamp`, `_with_authored_msn` and the private `_and_clamp`, plus
  `_inner`. Every public name must also be kept in `every_exterior_spawner_inserts_a_boundary_material`'s `boundary_fns` needle
  list (`:2516-2525`), which grew by one in the same commit.
- **Impact**: Maintenance only. A further variant would grow the needle list again, and a missed needle fails the guard closed.
- **Suggested Fix**: Move the body into `translate_texture_only_material_with_authored_msn` and delete `_and_clamp`, with
  `_with_clamp` calling it with `false`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Skinning/Lights · **Tier Violated**: parked-not-leak (record-keeping) · **Game Affected**: Oblivion, FO3, FNV, Skyrim (pre-FO4 node `effects` lists)
- **Location**:
  - `docs/engine/nifal.md:268-286` (the only scoping note, ambient-only);
  - `crates/nif/src/blocks/node.rs:22,83` (`NiNode.effects` parsed for bsver < FO4);
  - `crates/nif/src/import/walk/texture_effect.rs:18-54` (the only walker of `effects`);
  - `byroredux/src/cell_loader/spawn.rs:1099-1101` and `:1146-1163` (the allowlist skip).
- **Status**: NEW. Related to #5189 (closed) and NIF-D4-2026-10-05-01/-02 (the doc wording and the count predicate; not this gap).
- **Description**: #5189 established, and NIF-D4-2026-10-05-01 refined, that Gamebyro scopes every `NiDynamicEffect` to its
  affected-node subtrees. Oblivion-era content writes the on-light list empty and registers scope on `NiNode.effects`; 47 root
  nodes list the artifact lights there.
  - The importer parses `NiNode.effects`, but walks it only for texture effects. `ImportedLight` therefore carries no scope, and
    every point/spot/directional NIF light spawns unscoped.
  - The fix for the one known case is a consumer-side, census-backed name skip (`__MAX_Default_Light`, 48 Oblivion carriers).
  - `nifal.md`'s Lights section is marked **converged**. It mentions affected-node scoping only as an `NiAmbientLight` parking
    note ("none with `affected_node_names`"). It records neither the general gap nor the spawn-gate allowlist.
- **Evidence**: `grep -rn affected_node byroredux/src crates/renderer/src` finds no consumer. The `effects` walkers are in
  `walk/texture_effect.rs` only.
- **Impact**: There is no known vanilla population beyond the allowlisted artifact. A scoped non-artifact light (modded, or an
  unsurveyed vanilla mesh) would light the whole scene. The next audit cannot tell parked from dropped.
- **Related**: #5189, #5123, #3557, NIF-D4-2026-10-05-01/-02, the baseline ledger row for `NiAmbientLight`.
- **Suggested Fix**: Add a Lights ledger row in `nifal.md` §2: "affected-node scope (on-light list and pre-FO4 `NiNode.effects`)
  is not translated for any kind; the exporter artifact is dropped by name at `spawn_nif_lights` (#5189)". Optionally census
  `effects`-listed non-artifact lights with `crates/nif/examples/ambient_light_census.rs`'s approach.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Shader-flags/Effects (CDB merge) · **Tier Violated**: parked-not-leak · **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:258-264` (emissive `fill`);
  - `crates/nif/src/import/types.rs:911-912` (`emissive_color: [0.0; 3]`, `emissive_mult: 0.0` defaults);
  - `crates/renderer/shaders/triangle.frag:1512-1517` and `:1689-1693` (the glow sample becomes `emissiveMask`, applied only when
    `emissiveMult > 0.01 && emissiveLum > 0.01`, as `emissiveColor * emissiveMult * emissiveMask`).
- **Status**: NEW (introduced by `18fce7e43`)
- **Description**: `apply_cdb_material` presents slot 7 as a landed canonical role ("Pure role translation … 7=emissive land in
  their canonical `MaterialTextureSet` roles"). The CDB's `EmissiveSettingsComponent` (`Enabled`, `EmissiveTint`,
  `LuminousEmittance`, per the spike table) is not captured. A material-reference stub carries no inline emissive data, so the
  bound texture is multiplied by zero. Unlike the five parked single-channel kinds, this role is neither parked nor effective.
- **Impact**: No Starfield surface glows from its CDB emissive map, and every such map still costs a texture upload and bindless
  slot. Diagnostics (`tex.loaded`, `mat.dump`) show an emissive texture bound, which reads as "emission translated".
- **Related**: #3398, #4429 (the parked-kinds precedent), #5210.
- **Suggested Fix**: Either capture `EmissiveSettingsComponent` (enable plus tint/emittance → `emissive_color`/`emissive_mult`
  with an `EmissiveSource`) or skip `SLOT_EMISSIVE` and list it in `nifal.md`'s parked table until the scalars land.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

