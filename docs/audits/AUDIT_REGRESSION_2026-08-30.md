# Regression Verification Audit — 2026-08-30

**Scope**: verify that previously-closed bug fixes still hold at HEAD (`64f64480`).
**Method**: `gh issue list --state closed --label bug --limit 60` for dynamic discovery,
plus explicit `--issues` verification of every lead handed to this run, plus the
unconditional Step 4 fragile-area checks. Every premise was re-verified against the
live tree before being written down; measurements were re-taken from real game data
rather than quoted from the reporting sibling.

**Environment discipline**: `CARGO_BUILD_JOBS=4`, per-package scoped test runs only,
no `--ignored` / `--include-ignored` anywhere (the ESM-parsing ignored tests are the
confirmed OOM culprit for this session), no engine launch.

---

## Headline

| | Count |
|---|---|
| Closed fixes verified | **41** |
| Regressed / not actually fixed on main (FAIL) | **2** |
| Closed but only partially fixed (PARTIAL) | **2** |
| Closed, fixed, but inert on real content | **1** |
| Fixed in code yet still OPEN in the tracker | **7** |
| Stale lead premises dropped after verification | **1** |
| Findings | 1 HIGH-severity guard failure + 1 HIGH content gap + 3 MEDIUM + 3 LOW |

**Severity roll-up**: CRITICAL 0 · HIGH 2 · MEDIUM 3 · LOW 3.

The dominant failure mode this cycle is **not** code regression. It is
**closure hygiene**: multi-part issues closed on one part, fixes that land but
cannot reach production data, fixes that live on an unmerged branch, and fixed
defects left open. Only one finding is a live code hazard (REG-01), and it is
red *in CI right now*.

---

## Findings

### REG-2026-08-30-01: the `BYRO_LOCK_ORDER_CHECK` CI gate is RED at HEAD — five ragdoll tests abort on a real ABBA cycle

- **Severity**: HIGH (`ECS deadlock potential` → HIGH minimum, `_audit-severity.md`)
- **Dimension**: Concurrency / regression guard
- **Location**: `byroredux/src/commands/view.rs:168-215` (`combat_approach_line_of_sight_reaches`) · guard at `crates/core/src/ecs/lock_tracker.rs:411` · CI job `.github/workflows/ci.yml:99-112`
- **Status**: Regression of the always-green lock-order gate; introduced by `5c8a1581` (Fix #3422, Fix #3424, and gate combat.approach on line of sight, #3423)
- **Description**: `combat_approach_line_of_sight_reaches` holds the `PhysicsWorld`
  resource read guard live across `world.query::<byroredux_physics::RapierHandles>()`
  and `world.get::<ActorColliderOwner>()`. That records a `PhysicsWorld → RapierHandles`
  edge in the process-wide acquisition graph. The ragdoll writeback path acquires the
  reverse order, closing the cycle `PhysicsWorld → RapierHandles → GlobalTransform →
  PhysicsWorld`.
- **Evidence** (reproduced this run, not quoted):

  ```
  $ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins
  test result: FAILED. 1642 passed; 5 failed; 17 ignored
    ragdoll::tests::activate_then_writeback_moves_bones
    ragdoll::tests::falling_ragdoll_expands_skinned_mesh_world_bound
    ragdoll::tests::writeback_inverts_body_local_offset_round_trip
    ragdoll::tests::writeback_rederives_non_body_descendant_from_simulated_parent
    ragdoll::tests::writeback_uses_seed_time_scale_not_live_scale_after_mutation

  panicked at crates/core/src/ecs/lock_tracker.rs:411:13:
  ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
  `byroredux_physics::world::PhysicsWorld` while holding
  `byroredux_core::ecs::components::global_transform::GlobalTransform` ... cycle:
  PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld
  ```

  Two-test minimal repro isolating the edge source:

  ```
  $ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins -- --test-threads=1 \
      combat_approach_line_of_sight ragdoll::tests::activate_then_writeback_moves_bones
  test commands::tests::combat_approach_line_of_sight_rejects_an_occluded_ring_candidate ... ok
  test ragdoll::tests::activate_then_writeback_moves_bones ... FAILED
  ```

  The same 15 ragdoll tests pass cleanly when the commands tests do not run first
  (`cargo test -p byroredux --bins ragdoll:: -- --test-threads=1` → 15 passed).
- **Impact**: the dynamic ABBA detector is the project's only cross-thread lock-order
  proof, and it is failing. **Every concurrency regression landed after `5c8a1581`
  is masked** — the job cannot go from green to red because it is already red. The
  underlying cycle is not a test artifact: `combat_approach_line_of_sight_reaches`
  is production code reachable from the `combat.approach` console command while
  `ragdoll_writeback` runs.
- **Related**: #3423 (the feature), #313 / #2675 (the detector), #3441 (a
  structurally identical `ActorValues ↔ CharacterRuleset` cycle, fixed in `b28acb0c`
  by releasing the storage guard before the resource acquire — the same shape of fix
  applies here).
- **Suggested Fix**: collect the `RapierHandles` owner map (and the
  `ActorColliderOwner` lookup) **before** taking the `PhysicsWorld` guard, exactly as
  `condition.rs:470` now does for `ActorValues`/`CharacterRuleset`; or drop the
  `physics` guard into a narrower scope around `cast_ray` only.

---

### REG-2026-08-30-02: every vanilla `TREE` `.spt` MODL misses the archive, so #3528's `TREE.ICON` fix has zero production reach

- **Severity**: HIGH
- **Dimension**: Import pipeline / SpeedTree
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:449-452` · `byroredux/src/asset_provider/archive.rs:96-118` (`normalize_mesh_path`) · `byroredux/src/asset_provider/texture.rs:57-65` (`extract_mesh`)
- **Status**: NEW (blocks the closed #3528 from doing anything on shipped content)
- **Description**: #3528 correctly taught `resolve_tree_icon_path` to probe
  `trees\leaves\` / `trees\billboards\` for a bare `TREE.ICON`. But that resolver is
  only reached from `parse_and_import_spt`, which is only reached when
  `tex_provider.extract_mesh(&model_path)` returns bytes. It never does on vanilla
  content, because the model path is mangled twice:
  1. `synth_child.rs:449-452` prefixes `meshes\` unless the string already starts with it;
  2. `normalize_mesh_path` does the same again for any non-`meshes\`/`geometries\` path.

  Vanilla `TREE.MODL` for a SpeedTree is a **bare filename with a leading separator**
  (`\WhiteOak01.spt`), and the assets live at a **top-level** `trees\` folder, not under
  `meshes\`. The composed lookup is `meshes\\WhiteOak01.spt` — a guaranteed miss.
- **Evidence** (measured this run):

  ```
  # MODL bytes straight out of the master (grep -aoP + cat -v):
  FalloutNV.esm : MODL ^P^@ \WhiteOak01.spt          (size 0x10 = 15 chars + NUL)
  Oblivion.esm  : MODL ^M^@ \DTree01.spt             (size 0x0D = 12 chars + NUL)

  # .spt occurrences per master:
  FalloutNV.esm  3 · Fallout3.esm  9 · Oblivion.esm 142     (= 154 total)

  # Archive layout (crates/bsa examples/bsa_grep):
  $ cargo run --release -p byroredux-bsa --example bsa_grep -- \
      ".../Fallout New Vegas/Data/Fallout - Meshes.bsa" ".spt"
  trees\whiteoak01.spt
  trees\oasistreetop01.spt
  ... (10 total, every one under top-level `trees\`)

  $ ... "Oblivion - Meshes.bsa" ".spt"     → 50+ hits, all `trees\…`
  $ ... "Oblivion - Misc.bsa"   ".spt"     → 0
  ```

  The record-side fixtures never caught this because they author paths vanilla does
  not use: `crates/plugin/src/esm/records/tree.rs:243` uses
  `meshes\trees\treejoshua01.spt` and `:285` uses `trees\pine01.spt` — neither is the
  shipped `\Name.spt` shape. No `trees\` prefix rule exists anywhere in the resolution
  chain (`grep -rn 'trees\\\\' crates/plugin/src byroredux/src/cell_loader` → only the
  fixtures and `TREE_ICON_CANDIDATE_DIRS`).
- **Impact**: 154 of 154 vanilla SpeedTree bases fail to load on FNV, FO3 and Oblivion.
  Every `.spt` REFR falls to the `nif_not_found_sample` accumulator — no billboard, no
  placeholder, nothing. The whole Session-33 SpeedTree slice, #3076, #3528, #3529 and
  #3531 are collectively unreachable on shipped data. This is also why #3528's own
  corpus gate passes: `vanilla_tree_icons_all_resolve` exercises
  `resolve_tree_icon_path` directly against the texture archives and never goes through
  `extract_mesh`.
- **Related**: #3528 (CLOSED, correct as far as it goes), #3533 (open, adjacent
  SpeedTree wiring), #3191.
- **Suggested Fix**: add a `.spt`-aware arm at the single mesh-resolution boundary —
  strip a leading path separator and probe `trees\<name>.spt` before falling back to
  the `meshes\` normalisation. Keep it scoped to the `.spt` extension the way #3528
  scoped its ICON rule, and pin it with a corpus gate that goes through `extract_mesh`,
  not just the path helper.

---

### REG-2026-08-30-03: #3530's Oblivion `APPLY_HILIGHT2` parallax branch cannot fire on any shipped mesh

- **Severity**: MEDIUM
- **Dimension**: NIFAL material boundary / legacy-compat
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-285`
- **Status**: Landed-but-inert (fix present and unit-guarded; zero production reach)
- **Description**: the branch is
  `if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() { if let Some(normal) = info.normal_map { … } }`.
  On Oblivion the only producer of `info.normal_map` is
  `tex_prop.normal_texture` (`legacy_properties.rs:188-189`) — the other two producers
  are `BSShaderTextureSet` slot 1 (FO3+) and `BSEffectShaderProperty` (FO4+). Not one
  shipped Oblivion `NiTexturingProperty` authors a normal slot, so the inner `if let`
  never binds.
- **Evidence** (measured this run over `Oblivion - Meshes.bsa`, 8 032 NIFs, all parsed):

  ```
  NiTexturingProperty blocks            = 30 121
    with a normal_texture slot          =      0
  APPLY_HILIGHT2 (apply_mode == 4)      =  1 274   (in 659 files)
  APPLY_HILIGHT2 with a normal slot     =      0

  end-to-end via import_nif():
  meshes = 35 322 · textures.height Some = 0 · parallax_height_in_alpha = 0
  ```

  (The commit message cites 1 433 properties across 741 meshes; over
  `Oblivion - Meshes.bsa` alone this run measures 1 274 across 659. Either way the
  normal-slot count is zero, which is the number that decides the branch.)
- **Impact**: no correctness break — the fix is a strict no-op. But `Material`
  gained a saved field (`parallax_height_in_alpha`) and a save-format major bump
  (`crates/save/src/snapshot.rs:90-93`, FORMAT v10) plus a `PARALLAX_ALPHA_HEIGHT_BIT`
  shader-contract bit for behaviour nothing can reach. The issue's stated goal —
  Oblivion parallax — remains unachieved.
- **Related**: #3530 (CLOSED), #2317, #452.
- **Suggested Fix**: reopen #3530 or file a successor. The height source has to come
  from somewhere other than `NiTexturingProperty.normal_texture`; the commit's own
  claim that "the height source is the normal map's alpha channel" needs a *sourced*
  answer for where Oblivion's normal map actually comes from, since the slot is
  universally absent. Do not guess a synthesised `_n.dds` path.

---

### REG-2026-08-30-04: #3330 closed on its `bhkHinge` third; `bhkPrismatic` and breakable-wrapped edges still sever the Protectron ragdoll

- **Severity**: MEDIUM (`Translatable block silently dropped by NIFAL` → MEDIUM)
- **Dimension**: PHYSAL ragdoll articulation
- **Location**: `crates/nif/src/import/collision/ragdoll.rs:142-155` (breakable arm) and `:204-220` (`BhkConstraintData::Other` arm)
- **Status**: PARTIAL close of #3330
- **Description**: #3330's title and evidence name three drop classes across three FNV
  creature skeletons. The fix (`1ccf1abe`) decoded `bhkHingeConstraint` via
  `LimitedHingeCInfo::parse_hinge_fo3`, which resolves `sentryturret` and
  `minisentryturret`. `bhkPrismaticConstraint` still decodes to
  `BhkConstraintData::Other` and is dropped with a warn; `BhkBreakableConstraint`
  still falls through the downcast and is dropped with a warn. Both drops are on
  `creatures\protectron\skeleton.nif`, whose 4 connected components (`Bip01 Head`,
  `Bip01 Head Dome`, `Bip01 Spine Brain` each severed) are unchanged.
- **Evidence**: the code comment at `:204-212` states it outright — *"What remains
  reaching here on vanilla FNV is `creatures\protectron\skeleton.nif`'s two
  `bhkPrismaticConstraint` edges, which need a canonical prismatic joint kind that does
  not exist yet."* The hinge half is present and guarded
  (`crates/nif/src/blocks/collision/bhk_constraint_tests.rs:214-283`, FO3 + Oblivion
  layouts).
- **Impact**: a destroyed Protectron's head, head dome and spine-brain each become an
  independent free-falling multibody — exactly the visible break #3330 documented.
  Blast radius 1 creature skeleton (down from 3).
- **Related**: #1539, #1850, #3330.
- **Suggested Fix**: file the residual as its own issue rather than reopening — the two
  halves need different work (`ImportedJointKind::Prismatic` + a Rapier prismatic spec
  for one; retaining the wrapped CInfo geometry at parse time in
  `blocks/collision/constraints.rs` for the other, which is #1850's own note).

---

### REG-2026-08-30-05: four closed issues' fix commit sits on an unmerged branch; two of the four are genuinely unfixed on `main`

- **Severity**: MEDIUM
- **Dimension**: Process / tech-debt
- **Location**: `byroredux/src/npc_spawn.rs:1080` + `:1179` · `crates/core/src/character/profile.rs:47-52,178-191` · `crates/core/src/character/leveling.rs:93-109`
- **Status**: Regression of #2266 and #3170 (both CLOSED 2026-08-25, both unfixed on `main`)
- **Description**: commit `bbd501a1` ("Fix #2266 … Fix #3084 … Fix #3170 … Fix #3169")
  lives only on `fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst`.
  `git merge-base --is-ancestor bbd501a1 main` → **not an ancestor**. All four issues
  are CLOSED. Two of them were subsequently fixed on `main` by other commits; two were not.

  | Issue | On `main`? | Evidence |
  |---|---|---|
  | #3169 Skyrim `Illusion` → `AVMysticism` | **YES** — landed via `9e44a0dd` | `crates/core/src/character/skill.rs:149` `SkillDef::ungoverned("Mysticism")`, guard at `:386` |
  | #3084 Oblivion creature-asset corpus guard `#[ignore]`d | **YES** | `byroredux/src/npc_spawn/tests.rs:1543` `#[ignore = "needs Oblivion game data on disk; parses the whole master (~1.4 GB resident)"]` |
  | #2266 orphaned synchronous NPC-spawn wrappers deleted | **NO** | `spawn_npc_entity` (`npc_spawn.rs:1080`) and `spawn_prebaked_npc_entity` (`:1179`) both still present, both `#[allow(dead_code)]`, zero call sites tree-wide |
  | #3170 Skyrim `RulesetBuilder` arm wired | **NO** | `RulesetBuilder` (`profile.rs:47-52`) still has only `None`/`Fallout3`/`FalloutNewVegas`/`Fallout4`; `LevelingModel::with_gmst` (`leveling.rs:93-109`) still matches only `SkillXp`, which `build_ruleset` never constructs — `other => other` on every reachable model |
- **Impact**: #3170's premise ("#2942's GMST-sourcing seam has zero production reach")
  is fully intact at HEAD, so the seam is still dead while the tracker says otherwise.
  #2266's dead wrappers are still carried, silenced by `#[allow(dead_code)]`.
- **Related**: the standing "orphan branch" note in project memory.
- **Suggested Fix**: cherry-pick `bbd501a1`'s #2266 and #3170 hunks onto `main`, or
  reopen both. Worth a CI check that a `Fix #N` commit reachable only from a
  non-`main` ref does not silently close its issue.

---

### REG-2026-08-30-06: seven issues are fixed in code but still OPEN in the tracker

- **Severity**: LOW
- **Dimension**: Process / doc-rot
- **Status**: recommend closing all seven

| Issue | Premise at HEAD | Evidence |
|---|---|---|
| #3512 | DEAD — `CsgArchive::chunk_bytes` bounds the decoder | `crates/bsa/src/csg.rs:286` `inflate_bounded(ZlibDecoder::new(&comp[..]), CSG_CHUNK_SIZE, …)`, comment cites #3410 |
| #3513 | DEAD — ROADMAP FO3 row corrected | `ROADMAP.md` FO3 row: `100% (17 172 across 6 archives — base + 5 DLC; measured 2026-08-28)` |
| #3191 | DEAD — wind bend now pre-multiplied on a world axis | `byroredux/src/systems/billboard.rs:237-239` `Quat::from_axis_angle(axis, angle) * base`; the `along_weight`/`cross_weight` derivation is gone |
| #3149 | DEAD — the missing-destroy-trait state is now visible | `crates/ui/src/avm2_host.rs:45-47` `AdapterInjectedWithoutDestroyHook` + `has_destroy_hook()` accessor |
| #3151 | DEAD — the `InitCodeObj`/`ReleaseCodeObj` skip is removed | `crates/ui/src/avm2_host.rs:1643-1648` asserts `referenced_host_methods_in_tags` **returns** both names |
| #3155 | DEAD — the warn dedup key is per-menu | `byroredux/src/main.rs:236` `ui_reported_host_methods: HashSet<(String, String)>` |
| #3156 | DEAD — the set is capped | `crates/ui/src/navigator.rs:32` `MAX_IMPORT_ASSET_PATHS: usize = 512` + `extend_import_asset_paths` (`:99-114`) |

#3149/#3151/#3155/#3156 have now been reported as fixed-but-open for a third
consecutive audit cycle.

---

### REG-2026-08-30-07: #3488's companion guard is a hand-maintained list, not the scan its docstring claims

- **Severity**: LOW
- **Dimension**: Save/load, test-gap
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:97-132`
- **Status**: PARTIAL hardening gap on #3488 (the fix itself PASSES)
- **Description**: the test's docstring says *"this scans the tree for production
  `world.remove::<T>` sites … Adding one makes this fail and forces the maintainer to
  write the reconciler."* It does not scan. It iterates a two-entry `const RECONCILED`
  and then greps `inventory.rs` for one literal string. Adding a **new** production
  removal of a *different* delta column would not fail it.
- **Evidence — and a lead premise dropped**: a lead into this run claimed *"6 of 7
  production removal sites are unguarded."* **That is false at HEAD.** All six
  non-`EquippedWeapon` `world.remove::<T>` sites are inside `#[cfg(test)]` modules
  (verified by walking back to the nearest `#[cfg(test)]` above each):

  ```
  byroredux/src/systems/water.rs:957,958,971   (cfg(test) at 648)
  byroredux/src/systems/audio.rs:582           (cfg(test) at 342)
  byroredux/src/systems/bounds.rs:654          (cfg(test) at 344)
  crates/scripting/src/trigger.rs:790          (cfg(test) at 429)
  crates/physics/src/water.rs:1785-1787        (cfg(test) at 1026)
  crates/core/src/ecs/systems.rs:587           (cfg(test) at 256)
  ```

  So the test's *audit claim* is currently accurate; only its *enforcement mechanism*
  is weaker than advertised.
- **Impact**: no live defect. The guard will silently stop guarding the day someone
  adds a production removal.
- **Suggested Fix**: replace the literal grep with a source scan over the crate tree
  that skips `#[cfg(test)]` regions, or downgrade the docstring to match what the test
  actually does.

---

### REG-2026-08-30-08: both `manual_bench_draw_sort_*` benches panic on integer overflow in a debug build

- **Severity**: LOW
- **Dimension**: Test hygiene
- **Location**: `byroredux/src/render/draw_sort_key_tests.rs:505` and `:602`
- **Status**: NEW (pre-existing, surfaced incidentally)
- **Description**: both benches compute `c.mesh_handle = (i as u32 * 2654435761) & 0xFFFF;`.
  `2654435761 > u32::MAX / 2`, so the plain `*` overflows for `i >= 2` and panics under
  `debug_assertions`. Both are `#[ignore]` and documented `--release`, so this never
  reaches CI — but a maintainer running `-- --ignored` without `--release` gets a panic
  rather than a measurement.
- **Evidence**: verified statically. **Not** run: `--ignored` is forbidden in this
  session (the ignored ESM-parsing tests are the confirmed OOM culprit).
- **Suggested Fix**: `wrapping_mul(2654435761)`, matching the `wrapping_add` already
  used two lines below for `sort_depth`.

---

## Per-Issue Verification Log

### Leads verified

## #3330: undecoded `bhkHinge` / `bhkPrismatic` / breakable edges fragment three FNV creature ragdolls
- **Status**: PARTIAL
- **Closed**: 2026-08-29
- **Fix commit**: `1ccf1abe`
- **Fix site**: `crates/nif/src/blocks/collision/constraints.rs` (`LimitedHingeCInfo::parse_hinge_fo3`, `:214`); dispatch `blocks/mod.rs:1223`
- **Fix present**: Partially — hinge yes, prismatic no, breakable no
- **Guard test**: `bhk_constraint_tests.rs:214-283` (FO3 + Oblivion hinge layouts) — passes
- **Notes**: see REG-2026-08-30-04.

## #3528: every vanilla `TREE.ICON` is a bare filename
- **Status**: PARTIAL (fix correct, unreachable)
- **Closed**: 2026-08-30
- **Fix commit**: `19813460`
- **Fix site**: `byroredux/src/cell_loader/references/import.rs:300-350` (`TREE_ICON_CANDIDATE_DIRS`, `resolve_tree_icon_path`)
- **Fix present**: Yes
- **Guard test**: `import_tests.rs:124` (probe order, synthetic) + `:191 vanilla_tree_icons_all_resolve` (env-gated corpus) — both pass
- **Notes**: see REG-2026-08-30-02. The `.spt` never reaches `parse_and_import_spt`, so the resolver is dead on vanilla data.

## #3488: `EquippedWeapon` is removed at runtime with no reconciler
- **Status**: PASS
- **Closed**: 2026-08-30
- **Fix commit**: `fa511bbf`
- **Fix site**: `byroredux/src/inventory.rs:492` (`reconcile_player_equipped_weapon`), called from `byroredux/src/save_io.rs:1432`
- **Fix present**: Yes
- **Guard test**: `save_io/round_trip_tests.rs:98 delta_columns_removed_at_runtime_have_a_load_reconciler` — passes
- **Notes**: lead premise ("6 of 7 removal sites unguarded") DROPPED — all six are `#[cfg(test)]`. Residual hardening gap filed as REG-2026-08-30-07.

## #3530: Oblivion `APPLY_HILIGHT2` parallax
- **Status**: PARTIAL (present + guarded, zero production reach)
- **Closed**: 2026-08-30
- **Fix commit**: `19813460`
- **Fix site**: `crates/nif/src/import/material/legacy_properties.rs:272-285`
- **Fix present**: Yes
- **Guard test**: `crates/nif/src/import/tests/material_texture.rs:269-303`, `crates/nif/src/blocks/properties_tests.rs:306-364`, `shader_contract_tests.rs:2012-2075` — all pass (all synthetic)
- **Notes**: see REG-2026-08-30-03.

## #2266 / #3084 / #3169 / #3170 (orphan-branch set)
- **Status**: #3169 PASS · #3084 PASS · #2266 **FAIL** · #3170 **FAIL**
- **Closed**: all 2026-08-25
- **Fix commit**: `bbd501a1` — **not an ancestor of `main`**
- **Notes**: see REG-2026-08-30-05.

## #3512 / #3513 / #3191 / #3149 / #3151 / #3155 / #3156
- **Status**: fixed in code, OPEN in tracker
- **Notes**: see REG-2026-08-30-06.

### Recently-closed set (dynamic discovery, `--label bug --limit 60`)

| Issue | Title (short) | Status | Fix Present | Guard |
|-------|---------------|--------|-------------|-------|
| #3531 | zero-length 13005 SPT candidate | PASS | Yes — `crates/spt/src/parser.rs:172` | `parser.rs:525` |
| #3530 | Oblivion APPLY_HILIGHT2 parallax | PARTIAL (inert) | Yes | synthetic only |
| #3529 | NaN-transparent billboard clamp | PASS | Yes — `crates/spt/src/import/mod.rs:143 clamp_billboard_extent` | yes |
| #3528 | TREE.ICON bare filename | PARTIAL (unreachable) | Yes | corpus + synthetic |
| #3516 | `TexDesc.flags & 0xF` wrong nibble | PASS | Yes — `crates/nif/src/blocks/properties.rs:477` `(flags >> 12) & 0xF` | yes (`:189-195` census) |
| #3503 | GRUP depth cap reached 6 of 14 sites | PASS | Yes — all 14 now `bounded_group_content_end` | `grup_walker.rs:425-459` |
| #3488 | EquippedWeapon removal reconciler | PASS | Yes | yes (see REG-07) |
| #3443 | chunked geometry rebuild double alloc | PASS | Yes — `crates/renderer/src/mesh.rs:73,1275` | yes |
| #3472 | `settings_io::save_to_path` no fsync | PASS | Yes — `byroredux/src/settings_io.rs:137` | yes |
| #3471 | blend pass drops the weight filter | PASS | Yes — `crates/core/src/animation/stack.rs:347,437` | yes |
| #3470 | zero-advance text-key step | PASS | Yes — `crates/core/src/animation/stack.rs:38-44` | yes |
| #3469 | per-draw `vkGetBufferDeviceAddress` | PASS | Yes — `acceleration/types.rs:35-38` `vertex_address`, `skin_compute.rs:97` | yes |
| #3468 | `NiSequence` pre-10.1.0.104 Text Keys ref | PASS | Yes | `sequence_pre_10_1_0_106_tests.rs:67,128` |
| #3467 | 64 MiB rebuild chunk blocks | PASS | Yes — `resources/mod.rs:714`, `mesh.rs:404` | yes |
| #3466 | corpus gates walked a fraction of FO4/FO76/SF | PASS | Yes — `crates/nif/tests/parse_real_nifs.rs:210,298,352,374` | yes (env-gated) |
| #3441 | ActorValues ↔ CharacterRuleset lock cycle | PASS | Yes — `crates/scripting/src/condition.rs:470-474` | `:1486` |
| #3426 | Scaleform overlay before tone-map | PASS | Yes — `vulkan/presentation.rs:88,127,464,610` | yes |
| #3423 | melee swing lands on the wrong actor | PASS (fix works) | Yes — `commands/view.rs:168` | yes — **but see REG-01** |
| #3410 | BSA extraction inflated past the ceiling | PASS | Yes — `crates/bsa/src/safety.rs:114 inflate_bounded` | `safety.rs:296-346` |
| #3406 | MeshRegistry leaks vertex buffer | PASS | Yes — `crates/renderer/src/mesh.rs:422,531` | yes |
| #3402 | zero-index skinned meshes reach upload | PASS | Yes — `mesh.rs:728`, `nif_loader.rs:730` | `mesh.rs:3136-3161` |
| #3401 | ~12 embedded-FormID sites bypass remap | PASS | Yes — `records/common.rs:215` | `records/tests.rs:2142-2190` |
| #3400 | SCOL / PKIN child FormIDs bypass remap | PASS | Yes — `records/pkin.rs:94` | `records/tests.rs:2153` |
| #3399 | compressed-record size prefix trusted | PASS | Yes — `esm/reader.rs:22-37 MIN_RECORD_INFLATED_CEILING` | yes |
| #3391 | `canonical_mesh_path` panics on non-ASCII | PASS | Yes — `import/mesh/bs_geometry.rs:48-52` byte-slice | `:582-586` |

### Step 4 — unconditional fragile-area checks (all PASS)

- **Single material boundary**: `translate_material` (`byroredux/src/material_translate.rs:456`)
  is still the only `ImportedMesh → Material` site. `Material.metalness` / `.roughness`
  (`crates/core/src/ecs/components/material.rs:356,362`) and `PbrMaterial`'s pair
  (`:598-599`) are plain `f32` — no `Option<f32>` reintroduced. `classify_pbr_keyword`
  has exactly one non-test caller: `Material::resolve_pbr` (`:1167`). No render-time
  classifier.
- **Typed particle emitters**: `NiPSysEmitterCtlrData` (`blocks/mod.rs:1043`),
  `NiPSysGrowFadeModifier` (`:1067`), `NiPSysBoxEmitter` (`:1099`),
  `NiPSysSphereEmitter` (`:1101`), `NiPSysEmitterCtlr` (`:1135`) all dispatch typed.
  `extract_emitter_params` (`import/walk/mod.rs:786`) / `extract_emitter_rate` (`:916`)
  → `apply_emitter_params` (`byroredux/src/systems/particle.rs:29`) intact.
- **Collision shape coverage**: `BhkMultiSphereShape` (`import/collision/shape.rs:110`)
  and `BhkConvexListShape` (`:235`) still translate to a `CollisionShape`.
- **Disney BSDF / reservoir**: `crates/renderer/shaders/include/pbr.glsl` present,
  Burley attribution intact in `triangle.frag`. `resRadiance[]` stays **retired**
  (`include/lighting.glsl:120`, `triangle.frag:2756`); WRS recomputes via
  `shadowableLightRadiance` (`lighting.glsl:127`). No reservoir G-buffer attachment.
- **GPU struct size pins**: `cargo test -p byroredux-renderer gpu_` → **41 passed,
  0 failed**, including `gpu_camera_is_368_bytes` and
  `gpu_instance_is_160_bytes_std430_compatible`.

### Suite runs executed this audit

```
cargo test -p byroredux-nif                       1145 + 88 passed, 0 failed
cargo test -p byroredux-core                       705 +  2 passed, 0 failed
cargo test -p byroredux-plugin --lib               860 passed, 0 failed, 27 ignored
cargo test -p byroredux-plugin (all targets)         1 passed, 0 failed, 24 ignored
cargo test -p byroredux-renderer gpu_               41 passed, 0 failed
cargo test -p byroredux --bins                    1647 passed, 0 failed   (no env)
BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins
                                                  1642 passed, 5 FAILED   ← REG-01
```

No `--ignored` / `--include-ignored` run anywhere. Peak observed RSS stayed inside the
release `parse_nif` sweeps over `Oblivion - Meshes.bsa` (8 032 files, streamed one at a
time).

---

## Stale-premise accounting

- **1 lead premise dropped as stale**: #3488's "6 of 7 production removal sites are
  unguarded" — all six are `#[cfg(test)]` at HEAD (REG-2026-08-30-07 records the
  correction and the residual, weaker, real gap).
- **1 lead figure corrected on re-measurement**: #3530's APPLY_HILIGHT2 property count
  is 1 274 across 659 files over `Oblivion - Meshes.bsa`, not 1 430/741 (the commit
  message's 1 433/741 presumably spans more archives). The decisive number — normal
  slots present — is **0** either way, so the conclusion stands.
- **Every other lead was confirmed against current code**, including the two that
  required real-data measurement (`.spt` archive layout + MODL bytes; APPLY_HILIGHT2
  normal-slot census).
- **No skill-file drift found** in `audit-regression/SKILL.md` this run: the `grep -a`
  advice, the `byroredux-<crate>` package naming, and the Step 4 pin values
  (`GpuInstance` 160 B, `GpuCamera` 368 B) all match HEAD.

---

## Publish

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-30.md
```
