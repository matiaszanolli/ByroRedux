# Skyrim SE Compatibility Audit — 2026-09-05

**Scope**: all 7 dimensions — BSTriShape packed geometry + SSE skinned
reconstruction, `BSLightingShaderProperty` shader-type dispatch, NPC equip +
FaceGen (`crates/facegen`), multi-master load order, BSA v105, specialty
blocks + distant LOD, NIFAL Skyrim slice. Plus the two live leads handed to
this run (CHARAL Skyrim ruleset reachability, `docs/smoke-tests/p2-melee-core.sh`
gate state).

**Status**: COMPLETE. Run sequentially in-process (no sub-agents).

**Method**: static analysis + offline corpus measurement against the installed
`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` (Skyrim
SE / Anniversary Edition, 34 files). No engine launch. Corpus figures were
produced by four throwaway probe examples under `crates/nif/examples/` and
`byroredux/examples/`, run and then deleted — every number below is
reproducible from the described method, not transcribed from a prior report.

---

## Executive Summary

Skyrim SE remains the engine's renderer control bench, and the *parsers* are
in the same excellent shape the 2026-08-30 pass measured: **32 709 / 32 709
NIF entries parse clean, 0 parse failures**, and `byroredux-nif`'s library
suite is 1 235 / 1 235 green.

Four of that report's ten findings have since been fixed (`30a7e10a` closed
the doc-rot set; **#3541 closed the headline** — the fabricated `[0,1,0]`
up-normal). Re-measured today: **0 of 96 123 imported Skyrim meshes still
carry an all-flat up-normal**, down from 19 657. That is the single most
valuable regression guard in this report and it is green.

**The one new defect is one layer further down the same normal path.** The
`MAT_FLAG_MODEL_SPACE_NORMALS` branch in `crates/renderer/shaders/triangle.frag`
consumes Bethesda's `_msn` maps in the wrong basis. Measured per-vertex
against three independent vanilla head meshes (3 473 samples), the sampled
model-space normal correlates with the mesh's own geometric normal at
**cos = +0.88 under a negated third channel** and at **cos = +0.14 as the
shader currently uses it** — i.e. essentially uncorrelated. 4 037 Skyrim
meshes set the flag in `Skyrim - Meshes0.bsa`, including **all 3 201 vanilla
FaceGen heads** and 431 armor pieces.

Both handed-over leads were checked and answered:

1. **CHARAL** — #3848's premise re-confirmed; one *additional* Skyrim-specific
   consequence found and filed separately (`with_gmst` / `game_setting_float`).
   The other candidates named in the hand-over (Magicka/Stamina regen,
   `melee_damage_charal_bonus`, CTDA derived rows) were traced and are **not**
   additional Skyrim consequences — reasoning in Dimension 4.
2. **`docs/smoke-tests/p2-melee-core.sh`** — its `skyrim_se` arm's frozen
   preflight is **GREEN**, verified by running the real
   `probe_combat_fixture` offline against `Skyrim.esm`. All four pinned
   assertions match. The stages past the preflight need a Vulkan device and
   were not run.

### Findings

| ID | Sev | Dim | Summary | Status |
|---|---|---|---|---|
| SK-2026-09-05-D2-01 | HIGH | 2 / 3 / 6 | The `MAT_FLAG_MODEL_SPACE_NORMALS` branch consumes `_msn` maps in the source basis, not the renderer's — the third channel needs negating. 4 037 Skyrim meshes, incl. every FaceGen head | NEW |
| SK-2026-09-05-D4-01 | MEDIUM | 4 (CHARAL) | `LevelingModel::with_gmst`'s only non-identity arm is Skyrim's `SkillXp`, so #3848 leaves `EsmIndex::game_setting_float` with zero reachable production consumers — and the real-data roster test pins the gap as expected behaviour | NEW |
| SK-2026-09-05-D5-01 | LOW | 5 | The runtime archive chain has no present-only optional tier: the six AE / Creation Club archives the NIF corpus gate deliberately sweeps are unreachable from every runtime launch | NEW |
| — | MEDIUM | 4 | Deleted (`0x20`) tombstone honoured for placements only; 9 DLC-deleted base records merge live | Existing: **#3543** |
| — | LOW | 2 | `parse_skyrim_shader_base` did not get #2603's gap-band predicates its two inline twins got | Existing: **#3845** |
| — | MEDIUM | 4 | `skyrim_ruleset` production-unreachable via `RulesetBuilder::None` | Existing: **#3848** |

**Totals: 0 CRITICAL, 1 HIGH, 1 MEDIUM, 1 LOW (new).** Three findings
deduplicated against issues filed today or earlier.

### Candidates measured and dropped

Six candidate findings were dropped after checking them against current code
or data rather than filing them:

1. **"#3541 left a tangent gap."** The fix deliberately keeps
   `normals_authored` false for a derived normal, so tangent synthesis is
   still skipped for those meshes. Measured exposure: of the **13 659**
   imported Skyrim meshes that reach the renderer with an empty tangent
   array and full geometry, **0** carry a tangent-space normal map. 13 656
   carry a *model-space* map (where the TBN is unused by construction) and 3
   carry no normal map at all. The gap is real in principle and unreachable
   in practice on vanilla Skyrim.
2. **"Skyrim terrain LOD is wrongly flagged model-space."** 9 584 `.btr`
   quads set `SLSF1_Model_Space_Normals` while naming a `_n.dds` (not
   `_msn.dds`) normal map. The SLSF1 flag is authoritative, not the filename,
   and the maps really are model-space: sampled means decode to
   ≈ `(0.03, +0.87, 0.10)`, i.e. the vertical is in the green channel. The
   engine's decode is correct.
3. **"Skyrim terrain-LOD normal maps author a leading `data\` the resolver
   drops."** They do (`data\textures\terrain\…`), but
   `normalize_texture_path` (`byroredux/src/asset_provider/archive.rs`)
   strips exactly that prefix, and `canonical_texture_key` composes it with
   `strip_build_prefix`. Not a defect.
4. **"#3637's last-wins archive inversion broke Skyrim."** #3896 corrected
   the `starfield` / `fnv` profiles' orderings and named neither `skyrim_se`
   nor a Skyrim symptom. Checked directly: `assets/debug_profiles.toml`'s
   `skyrim_se` profile lists `Meshes0, Meshes1` and `Textures0 … Textures8`
   — ascending, which is also Bethesda's own `sResourceArchiveList2` order,
   so last-wins is the correct precedence for it. #2584's open-set dedup then
   drops the explicit re-lists of archives already auto-loaded as numeric
   siblings. Correct as shipped.
5. **`apply_morphs`' Z-up/Y-up coordinate frame.** This is now reachable in
   production (`spawn_runtime_head` in `byroredux/src/npc_spawn/resumable.rs`
   calls `byroredux_facegen::apply_morphs` on already-Y-up
   `ImportedMesh::positions`), which is a change from the 2026-08-30 finding
   that called it unreachable. But the hook is gated on
   `NpcRecord::runtime_facegen`, which `GameKind::has_runtime_facegen_recipe`
   restricts to **Oblivion / FO3 / FNV**; Skyrim is on the mutually-exclusive
   `uses_prebaked_facegen` track. Out of scope here — handed to
   `/audit-oblivion` and `/audit-fnv` as a *now-live* path, not a dormant one.
6. **`pool_regen_tick_system` never runs.** `PoolRegenConfig` has no
   production constructor call anywhere (`oblivion_pool_regen_config` has
   zero callers outside `crates/core`), so the tick returns at its first
   guard for **every** game — it never reaches the `CharacterRuleset` guard
   #3848 is about. That makes it a cross-game CHARAL gap owned by
   `/audit-character`, not an additional Skyrim consequence, and it is why
   "Skyrim loses Magicka/Stamina regen" is **not** filed below.

### Deliberately not re-reported

* **Renderer ghosting** on Skyrim interiors (diagonal double-image) — open,
  needs RenderDoc. No source-level speculation offered.
* **VWD full-model culling** (#1731 / #3307) — forward scope; the premise was
  re-measured 2026-08-29 and the "effectively unbuildable" framing is retired.
* **#1832** (mass-0 Dynamic-family Havok bodies reclassified Static) — settled.
* **#3905** (NIFAL) — the neutral-roughness / shader-escape gate asymmetry was
  filed today by `/audit-nifal`; the Skyrim `glossiness` sibling of it routes
  through the same `classify_pbr_keyword` inputs and is covered there.

---

# Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

Corpus: `Skyrim - Meshes0.bsa` (18 862 NIF entries, 71 149 imported meshes) +
`Skyrim - Meshes1.bsa` (13 847 entries, 24 974 meshes) = **32 709 entries /
96 123 imported meshes, 0 parse failures**. Entry selection through
`byroredux_nif::corpus::is_nif_entry` (`.nif` / `.bto` / `.btr`).

## Verified clean

| Guard | Measured 2026-09-05 |
|---|---|
| Parse rate | 32 709 / 32 709, **0 failures** |
| **#3541 normal synthesis** (`crates/nif/src/import/mesh/normal.rs`) | **0 of 96 123 imported meshes still carry an all-`[0,1,0]` normal array** — was 19 657 (20.4 %) on 2026-08-30 |
| Tangent coverage | 75 917 of 96 123 meshes carry tangents; of the 13 659 that do not *and* have full geometry, **0 need a TBN** (see dropped candidate 1) |
| SSE reconstruction (#559 / #1204 / #2817) | `try_reconstruct_sse_geometry` still gates `normals_authored` on `ReconstructedSseGeometry::normals_authored`, not on `sse_normals.is_some()`; the #2817 vacuous-guard fix is intact |
| Alpha cascade gate (#1201 / #1202) | `info.alpha_property_consumed` set in `crates/nif/src/import/material/mod.rs`, consulted in `crates/nif/src/import/material/dedicated_shader.rs` and `crates/nif/src/import/material/legacy_properties.rs`; `walker.rs` still holds only the stale comment |
| `byroredux-nif` library suite | **1 235 passed, 0 failed** |

`derive_normals_from_u32_indices` skips indices past `u16::MAX` rather than
truncating them. That is safe on this corpus by construction: `BsTriShape`
stores `triangles: Vec<[u16; 3]>`, so no BSTriShape body can address a vertex
above 65 535 in the first place.

---

# Dimension 2 — `BSLightingShaderProperty` / `BSEffectShaderProperty` Dispatch

## Shader-type coverage matrix

`parse_shader_type_data` (`crates/nif/src/blocks/shader.rs`) — the Skyrim
(BSVER < 130) arm. Verified arm-by-arm against nif.xml field counts:

| Numeric type | `ShaderTypeData` arm | Trailing fields read | Parse | Import | Render |
|---|---|---|---|---|---|
| 1 | `EnvironmentMap` | 1 × f32 (`env_map_scale`) | ✓ | ✓ | ✓ |
| 5 | `SkinTint` | 3 × f32 (Color3), `skin_tint_alpha: None` | ✓ | ✓ | ✓ |
| 6 | `HairTint` | 3 × f32 (Color3) | ✓ | ✓ | ✓ |
| 7 | `ParallaxOcc` | 2 × f32 (`max_passes`, `scale`) | ✓ | ✓ | ✓ (#3073 resolves once at the boundary) |
| 11 | `MultiLayerParallax` | 5 × f32 | ✓ | ✓ | ✓ |
| 14 | `SparkleSnow` | 4 × f32 | ✓ | ✓ | partial |
| 16 | `EyeEnvmap` | 1 + 3 + 3 × f32 | ✓ | ✓ | partial |
| 0, 2–4, 8–10, 12–13, 15, 17–20 | `None` | **0 bytes** — terminal `_ =>` arm | ✓ | n/a | n/a |

The `_ => Ok(ShaderTypeData::None)` fall-through consumes nothing, so no
Skyrim type can over-read. FO76's distinct numbering stays in
`parse_shader_type_data_fo76` (`Fo76SkinTint` on type 4, `HairTint` on
type 5) with no cross-contamination into the Skyrim arm.

**Existing: #3845** — `parse_skyrim_shader_base` is the shared Skyrim+ shader
head and did not get #2603's gap-band predicates that its two inline twins
did. Filed today; not re-reported.

## SK-2026-09-05-D2-01 (HIGH) — the model-space-normal branch consumes `_msn` maps in the source basis, not the renderer's

- **Severity**: HIGH
- **Dimension**: 2 (shader dispatch) / 3 (FaceGen) / 6 (distant LOD)
- **Location**: `crates/renderer/shaders/triangle.frag`, the
  `MAT_FLAG_MODEL_SPACE_NORMALS` branch inside the `normalMapIdx != 0u`
  block (search `MAT_FLAG_MODEL_SPACE_NORMALS`); flag producer
  `byroredux/src/cell_loader.rs` (`pack_imported_material_flags`), source
  signal `crates/nif/src/import/material/dedicated_shader.rs`
  (`model_space_normals`).
- **Status**: NEW. No open or closed issue covers the basis; **#2826**
  (CLOSED) fixed the *Z-reconstruction over-write* on three-channel maps and
  did not question which axis the sampled vector lives on.
- **Description**: the branch decodes the texel and rotates it straight into
  world space with the instance's model matrix:

  ```glsl
  vec3 mn = texture(textures[nonuniformEXT(normalMapIdx)], sampleUV).rgb;
  mn = mn * 2.0 - 1.0;
  if ((mat.materialFlags & MAT_FLAG_MSN_HAS_AUTHORED_Z) == 0u) {
      mn.z = sqrt(max(0.0, 1.0 - dot(mn.xy, mn.xy)));
  }
  mat3 model3 = mat3(inst.model);
  ...
  worldMn = model3 * mn;
  ```

  `inst.model` is a **renderer Y-up** matrix — the Z-up → Y-up conversion is
  baked per-vertex at import (`zup_point_to_yup`, `(x, y, z) → (x, z, -y)`),
  not carried by the instance transform. The sampled `mn`, however, is in the
  basis Bethesda's exporter wrote, and that basis is not the renderer's. The
  same file already knows this for the *other* authored-in-source-space
  resource it samples — the cubemap path a thousand lines later explicitly
  writes "Renderer world space is Y-up; Bethesda cubemaps are authored in the
  source Z-up basis" and inverts the import transform before selecting a face.
  The MSN branch has no equivalent step.
- **Evidence** (per-vertex correlation, nearest-neighbour sample of the mesh's
  own `_msn` map at each vertex UV, cosine against that vertex's imported
  Y-up normal; uncompressed `R8G8B8A8` sources only, so no BC decode is in
  the loop):

  | Mesh (`Skyrim - Meshes0.bsa`) | samples | identity **(shader today)** | `(x, y, −z)` | `(x, z, −y)` | `(−x, −y, −z)` |
  |---|---|---|---|---|---|
  | `meshes\actors\character\character assets\malehead.nif` | 898 | **+0.137** | **+0.885** | +0.282 | −0.137 |
  | `…\maleheadkhajiit.nif` | 1 356 | **+0.078** | **+0.864** | +0.228 | −0.078 |
  | `…\maleheadargonian.nif` | 1 219 | **+0.307** | **+0.691** | +0.281 | −0.307 |

  The winner is the same on all three and beats every alternative by a wide
  margin. It is also the transform a coherent authoring convention predicts:
  if the map stores Gamebryo axes as `(X, Z, Y)` — green = up — then reaching
  the renderer's `(X, Z, −Y)` needs exactly one sign flip on the third
  channel. The corroborating measurement is the terrain-LOD class: those maps
  average `(0.03, +0.87, 0.10)` decoded, i.e. up **is** in green, and their
  normals are so uniformly `+Y` that the Z sign cannot show up there — which
  is why this never surfaced from the LOD side.
- **Impact**: MSN-flagged meshes in the two Skyrim mesh archives, by class:

  | Archive | class | meshes |
  |---|---|---|
  | Meshes0 | `actors\character\facegendata` (per-NPC FaceGen heads) | **3 201** |
  | Meshes0 | `armor` | 431 |
  | Meshes0 | `other` 325 / `actors\character` 58 / `actors\` 6 / `architecture` 6 / `clutter` 6 / `dungeons` 3 / `effects` 1 | 405 |
  | Meshes1 | `terrain` (distant LOD `.btr`) | 9 584 |
  | Meshes1 | other | 35 |
  | | **total** | **13 656** |

  The 9 584 terrain quads are effectively immune (their normals are all
  near-`+Y`, the axis the flip does not touch). The **4 037 Meshes0 meshes
  are not**: every vanilla NPC face in the game, plus 431 armor pieces, is
  lit from a normal that is essentially uncorrelated with its own surface.
  This is the residue that survives #3541 — the geometry now has real
  normals, and the normal *map* then overwrites them with a mis-rotated one.
  It is also a plausible contributor to the long-running "faces look wrong /
  plastic" class of Skyrim symptom, though this audit does not claim to have
  closed that separately.

  The secondary consequence is on the `!MAT_FLAG_MSN_HAS_AUTHORED_Z` arm: the
  component it reconstructs is not a "height" but the renderer's facing axis,
  and it is reconstructed non-negative. Under the corrected basis the sign
  convention for that arm has to be re-derived, not merely inherited.
- **Related**: #2826 (the reconstruction half of the same branch), #1147
  (Phase 2b, which introduced the branch for FO4), #1592, the cubemap
  basis-inversion comment in the same shader.
- **Suggested Fix**: negate the third channel of the decoded model-space
  normal before the model-matrix rotation, mirroring what the cubemap path
  already does for source-basis data, and re-derive the reconstruction arm's
  sign at the same time. **Two caveats before landing.** First, the same
  branch serves FO4 / FO76 model-space content, and this audit measured only
  Skyrim — re-run the same per-vertex correlation on an FO4 `_msn` set before
  changing a shared branch. Second, per the project's standing rule this is a
  render-visible change whose failure mode `cargo test` cannot see: pair it
  with a RenderDoc or screenshot confirmation on a FaceGen head, not with a
  unit test alone. The measurement itself is offline and reproducible, so the
  *premise* does not depend on that confirmation — only the landing does.

## Disney/Burley lobe — regression guard

`MAT_FLAG_PBR_BSDF` (`crates/renderer/shaders/include/shader_constants.glsl`)
must stay unreachable on vanilla Skyrim, because BGSM is FO4+ and vanilla
Skyrim authors no PBR opt-in. The only producer path remains
`byroredux/src/asset_provider/material.rs`'s BGSM/BGEM merge, which cannot
fire without a `.bgsm` / `.bgem` material path — a role vanilla Skyrim NIFs
never populate. Structurally green; unchanged from the 2026-08-30 measurement.

---

# Dimension 3 — NPC Equip + FaceGen (M41)

## Verified clean

* **Pre-baked FaceGen path is the Skyrim one, and it is wired.**
  `GameKind::uses_prebaked_facegen` covers `Skyrim | Fallout4 | Fallout76 |
  Starfield` and is mutually exclusive with `has_runtime_facegen_recipe`
  (Oblivion / FO3NV). `byroredux/src/npc_spawn.rs` composes both halves —
  `meshes\actors\character\facegendata\facegeom\<plugin>\<formid:08x>.nif`
  and the matching `textures\…\facetint\<plugin>\<formid:08x>.dds` — and both
  are covered by tests in `byroredux/src/npc_spawn/tests.rs` and
  `byroredux/src/scene/nif_loader.rs`.
* **All 3 201 vanilla FaceGen head meshes import with real geometry** — they
  are the largest single MSN class above, which means they parse, import, and
  reach the material stage. Their remaining defect is SK-2026-09-05-D2-01,
  not a parse or import gap.
* The `RACE.WNAM` default-skin layering (#2093) and the post-loop
  occupancy filter (#2094) are both still present in
  `byroredux/src/npc_spawn.rs` (`race_skin_slots`, `hidden_biped_mask`,
  `authored_biped_mask`), and `humanoid_body_paths` still returns the empty
  slice for the Skyrim+ family.

## Gate status

The 6-named-NPC Whiterun equip guard lives **only** in
`docs/smoke-tests/m41-equip.sh` and needs a live engine + Vulkan device;
it was not run. There is no offline equivalent.

## `crates/facegen` — scope note

`crates/facegen`'s `.egm` parser now has a live production consumer
(`byroredux_facegen::EgmFile::parse` / `apply_morphs` in
`byroredux/src/npc_spawn/resumable.rs`). That consumer is gated on
`has_runtime_facegen_recipe()`, so it is an **Oblivion / FO3 / FNV** path.
Skyrim never reaches it. The crate is therefore, for this title, correctly
unused — the prior report's "zero consumers" observation is a cross-game
one and belongs with `/audit-fnv` / `/audit-oblivion`, which now also inherit
the live `apply_morphs` basis question (dropped candidate 5).

---

# Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

## Verified clean

| Guard | State |
|---|---|
| `.STRINGS` loader wired into the multi-plugin path (`db5bb149`) | `byroredux/src/cell_loader/load_order.rs` still builds `esm::StringsTableGuard::new(tables)` over the whole load order, not just the active plugin |
| ESL / light-master decode (#1554) | `crates/plugin/src/esm/reader.rs` still computes `0xFE00_0000 \| (((sub as u32) & 0x0FFF) << 12) \| (raw & 0x0000_0FFF)`, driven by `FileHeader::light_master` from TES4 flag `0x0200` |
| Deleted-REFR tombstone (#1660) | `RECORD_FLAG_DELETED = 0x0000_0020` still defined and still consulted in `crates/plugin/src/esm/cell/walkers.rs` |
| Frozen combat fixture (`Skyrim.esm`) | `probe_combat_fixture` run offline: `CELL BleakFallsBarrow01 form=000371DE`, `NPC ref=000383F7 base=000E9895`, `000236A5:DraugrGreatsword:damage=17`, `0002C672:DraugrWarAxe:damage=9` — **all four pinned lines present** |

**Existing: #3543** — the Deleted flag is honoured for placements only; 9
DLC-deleted **base** records still merge live. Unchanged, not re-filed.

## Lead 2 answered — `p2-melee-core.sh` `skyrim_se`

The arm that #3039 turned RED and #3417 re-derived is the fixture preflight,
and it is the only stage runnable without a GPU. Running the real
`probe_combat_fixture` against the installed `Skyrim.esm` reproduces every
line `docs/smoke-tests/fixtures/skyrim_se.env` pins, including the two
`P2_PROBE_WEAPON_LINES` leaves #3417 replaced the unreachable
`DraugrBattleAxe` pin with. **The preflight is green**; gates 1-5 (engine
launch, `combat.approach`, hit resolution, ragdoll body count, save/reload
continuity) were not exercised and remain unverified by this audit.

## Lead 1 answered — CHARAL, and what else it degrades

#3848's premise re-confirmed against current code:
`CharacterRulesProfile::SKYRIM` carries `ruleset: RulesetBuilder::None`
(`crates/core/src/character/profile.rs`), and `build_ruleset` returns `None`
at that arm before doing anything else. Not re-filed.

Three of the four consequences the hand-over asked about are **not**
additional Skyrim-specific findings, and saying so is part of the answer:

* **Magicka / Stamina regen.** `pool_regen_tick_system`
  (`crates/core/src/character/regen.rs`) is gated on `PoolRegenConfig`
  *before* `CharacterRuleset`, and `PoolRegenConfig` has no production
  constructor at all — `oblivion_pool_regen_config` is called from nowhere
  outside `crates/core`. The tick therefore returns at its first guard for
  every game, Skyrim included, and would still do so with the ruleset wired.
  Cross-game CHARAL gap; `/audit-character`'s.
* **`melee_damage_charal_bonus`.** `byroredux/src/combat.rs` bails without a
  `CharacterRuleset`, but `skyrim_ruleset` pushes no melee-damage row and
  `build_melee_damage_config` returns `None` for the TES family by design
  (#3093 — no `MeleeDamage` AVIF). Wiring the builder would change nothing here.
* **CTDA `GetActorValue` derived rows.** Skyrim's two derived rows are
  `DamageResist` (explicitly `.player_only()`) and `CarryWeight`. Only
  `CarryWeight` is actor-general, so the reachable consequence is one
  console/CTDA row resolving to the baked base instead of
  `250 + 0.5·BaseStamina` — real, but a direct restatement of #3848 rather
  than a separate defect.

The fourth is a genuinely separate consequence and is filed below.

## SK-2026-09-05-D4-01 (MEDIUM) — the GMST leveling overlay is structurally unreachable, and the real-data test pins that as expected

- **Severity**: MEDIUM
- **Dimension**: 4 (load order / CHARAL boundary)
- **Location**: `crates/core/src/character/leveling.rs`
  (`LevelingModel::with_gmst`), `crates/core/src/character/profile.rs`
  (`build_ruleset`), `crates/plugin/src/esm/records/index.rs`
  (`game_setting_float`), `byroredux/src/npc_spawn.rs`
  (`build_character_ruleset`), `crates/plugin/tests/parse_real_esm.rs`
  (`ROSTER_CASES`)
- **Status**: NEW — an additional consequence of #3848, not a restatement of it.
- **Description**: `with_gmst` has exactly one non-identity arm —
  `Self::SkillXp`, which is **Skyrim's** model (`LevelingModel::SKYRIM`);
  every other variant falls through `other => other`. It is called from one
  place, `build_ruleset`, and only *after* the `RulesetBuilder` match. Because
  Skyrim's arm is `None`, `build_ruleset` returns before `with_gmst` ever
  runs. Consequently:
  * `fXPLevelUpBase` / `fXPLevelUpMult` — the only two authored GMSTs any
    ruleset reads — are never read in production for any game;
  * `EsmIndex::game_setting_float` has **zero reachable production
    consumers**. Its sole caller is the `gmst` closure in
    `build_character_ruleset`, which is threaded all the way from the load
    path and then never invoked;
  * the `game_settings` map itself is populated on every load (2 039 entries
    on `Skyrim.esm` per the parser's own census) and read by nothing.

  The second half is what makes this worth its own issue rather than a note
  on #3848: `crates/plugin/tests/parse_real_esm.rs`'s `RosterCase` for Skyrim
  sets `derived_rows: None`, and the test body's `None` arm asserts
  `ruleset.is_none()`. The one real-data test that exercises this boundary
  **pins the broken state as the expected state**, so fixing #3848 requires
  editing a green test, and nothing today can go red if the GMST decode
  regresses.
- **Evidence**: the same test proves the data is present and resolvable — it
  asserts every entry of `SkillSet::SKYRIM` (18 skills) resolves to an
  authored AVIF in `Skyrim.esm` before it reaches the `derived_rows` check.
  So the resolver works, the rosters are correct, and only the builder arm is
  missing.
- **Impact**: no runtime symptom today beyond #3848's own. The cost is a
  silent test gap: a load-time path (`GMST` float decode → leveling curve
  overlay) that is fully plumbed, fully untested against real data, and will
  execute for the first time on the same commit that fixes #3848.
- **Related**: #3848, #3170 (the fix that never reached `main`), #3221.
- **Suggested Fix**: when #3848 adds the `RulesetBuilder::Skyrim` arm, flip
  this case's `derived_rows` from `None` to the measured count in the same
  commit, and add a direct assertion that `game_setting_float("fXPLevelUpMult")`
  resolves on a real master — otherwise the GMST overlay ships with no
  real-data coverage at all.

---

# Dimension 5 — BSA v105 (LZ4)

## Verified clean

* **Full-corpus extraction**: 32 709 NIF entries extracted from
  `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa` with **0 extraction
  failures** during this audit's own sweeps (both archives are v105 with LZ4
  block compression; every entry was decompressed and parsed).
* Texture extraction exercised across `Skyrim - Textures0/6/7.bsa` for the
  MSN probes, including both `BC3_SRGB_BLOCK` and `R8G8B8A8_SRGB` payloads —
  no failures.
* **Numeric sibling auto-load** (`byroredux_bsa::numeric_sibling_paths`)
  behaves correctly for Skyrim's zero-based series: a trailing `0` with no
  digit before it yields `…1 … …9`, so `Skyrim - Textures0.bsa` drags in
  Textures1-8 and `Skyrim - Meshes0.bsa` drags in Meshes1. The explicit
  re-lists in the `skyrim_se` profile are then skipped by #2584's
  `opened_paths` set instead of being opened twice.
* **Last-wins ordering** (#3637) is right for Skyrim — see dropped
  candidate 4.

## SK-2026-09-05-D5-01 (LOW) — the runtime archive chain has no present-only optional tier, so AE / Creation Club archives are unreachable

- **Severity**: LOW
- **Dimension**: 5
- **Location**: `assets/debug_profiles.toml` (`[profiles.skyrim_se]`),
  `byroredux/src/game_profiles.rs`, `byroredux/src/asset_provider/archive.rs`
  (`open_with_numeric_siblings`)
- **Status**: NEW
- **Description**: the installed Data directory is a stock Anniversary
  Edition install and carries six archives the `skyrim_se` profile lists
  nowhere and no auto-load rule can reach: `_ResourcePack.bsa` (916 MB),
  `ccBGSSSE001-Fish.bsa`, `ccBGSSSE025-AdvDSGS.bsa`,
  `ccBGSSSE037-Curios.bsa`, `ccQDRSSE001-SurvivalMode.bsa`, and
  `MarketplaceTextures.bsa`. None is a numeric sibling of a listed archive,
  so `numeric_sibling_paths` cannot find them, and the profile has no
  optional/present-only archive field.

  The NIF corpus gate already models this correctly and separately:
  `Game::optional_mesh_archives` in `crates/nif/tests/common/mod.rs` sweeps
  exactly this present-only tier (that is what took the #3369 headline from
  32 709 to 33 424 files). The concept exists **only** in the test harness;
  the runtime has no counterpart.
- **Impact**: bounded today — the profile also loads only `Skyrim.esm`, so
  nothing in a default launch references CC assets. It becomes a real 404
  surface the moment a user passes `--esm ccBGSSSE001-Fish.esm` or
  `--master _ResourcePack.esl`, which is the natural way to reach AE content
  and the exact case the corpus gate was extended to cover. The asymmetry
  also means the corpus gate is measuring content the engine cannot render.
- **Related**: #3369 (the corpus split that introduced `optional_mesh_archives`),
  #3896 / #3637 (archive precedence), #2584 (sibling dedup).
- **Suggested Fix**: give `GameProfile` a present-only optional archive list
  (skip silently when absent, appended last to match #3637's precedence) and
  seed `skyrim_se` from the same names the test harness already enumerates,
  so the two tiers cannot drift.

---

# Dimension 6 — Specialty Blocks + Real-Data Rendering

## Block-dispatch guards — all intact

| Guard | Site |
|---|---|
| `"BSLODTriShape" => NiLodTriShape::parse` (#838 — must NOT route through BSTriShape) | `crates/nif/src/blocks/mod.rs` |
| `"BSMeshLODTriShape" => BsTriShape::parse_lod` | same |
| `"BSSubIndexTriShape"` its own arm | same |
| `"BSLagBoneController" => BsLagBoneController::parse` (#837) | same |
| `BsProceduralLightningController::parse` (#837) | same |

No realignment or `block_size` recovery WARN was observed on any of the
32 709 entries swept.

## Distant LOD

`.btr` terrain LOD (`byroredux/src/cell_loader/terrain_lod_btr.rs`) and `.bto`
object LOD (`byroredux/src/cell_loader/object_lod.rs`) both parse and import
cleanly — 13 847 entries in `Skyrim - Meshes1.bsa`, 0 failures, 9 619 of them
MSN-flagged terrain/architecture quads. The band ladder
(`LodBandLadder::for_object_game`) is unchanged since the 2026-08-30
verification against the game's own `Ultra.ini`.

Terrain LOD is the one MSN class SK-2026-09-05-D2-01 does **not** visibly
damage, for the reason given in that finding — recorded here so a fix is not
validated on the LOD view and declared green.

---

# Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

## Verified clean

* **Single boundary**: `translate_material` in
  `byroredux/src/material_translate.rs` remains the only
  `ImportedMesh → Material` path. `Material::classify_pbr` is still deleted;
  `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`) is
  the resolve-once site and delegates to `classify_pbr_keyword`.
* **Ordering**: `material.resolve_pbr()` runs immediately before
  `crate::helpers::classify_glass_into_material`, so forced-glass roughness
  still wins over the keyword default.
* **`EmissiveSource` discriminator (#1280)**: production writes exist in
  `crates/nif/src/import/material/dedicated_shader.rs` (`Lighting` for
  `BSLightingShaderProperty`, `Effect` for `BSEffectShaderProperty`) and
  `crates/nif/src/import/material/legacy_properties.rs` (`Material` for
  `NiMaterialProperty`), each pinned by
  `crates/nif/src/import/material/emissive_source_tests.rs`. Skyrim's
  `emissive_multiple` routes through `Lighting`, not `Effect` — confirmed.
* **`specular_authored` (#2573)** now flows `MaterialInfo` → `ImportedMaterial`
  → `Material` and is read by `resolve_pbr`'s backstop instead of being
  hardcoded `false`, preserving the #1873 chrome-flyer reasoning.

No Dimension 7 findings.

---

## Cell-Load Regression Status

* TES5 cells parse through the unified `crates/plugin/src/esm/cell/` walker;
  compressed records still decompress (the `BleakFallsBarrow01` probe walks a
  full interior and enumerates every ACHR/NPC_/WEAP leaf).
* `Skyrim.esm` continues to resolve the frozen `p2` triple
  (CELL / reference-base pair / weapon family) byte-for-byte.
* Whiterun BanneredMare control-bench entity count and FPS were **not**
  re-measured — that requires an engine launch, which this run was instructed
  not to perform. Cite ROADMAP's Bench-of-record; nothing in this audit's
  static findings would move the entity count.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-09-05.md
```

Label every finding `game:skyrim` + `legacy-compat`, plus:
SK-2026-09-05-D2-01 → `high` `bug` `shaders` `renderer` `nifal`;
SK-2026-09-05-D4-01 → `medium` `bug` `character` `test-gap`;
SK-2026-09-05-D5-01 → `low` `enhancement` `import-pipeline`.
