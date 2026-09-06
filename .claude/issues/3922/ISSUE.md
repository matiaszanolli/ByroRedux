# #3922: SK-2026-09-05-D2-01: the model-space-normal branch consumes `_msn` maps in the source basis, not the renderer's

Filed from `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D2-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `high,game:skyrim,legacy-compat,shaders,renderer,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3922 --json state`.

---

**Source**: `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D2-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
