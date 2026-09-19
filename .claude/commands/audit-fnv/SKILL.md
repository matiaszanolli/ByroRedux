---
description: "Per-game audit of Fallout New Vegas compatibility — reference title: cell load, ESM/NIF data, ragdoll, ambient AI, consumables, HUD profile"
argument-hint: "--focus <dimensions>"
---

# Fallout New Vegas Compatibility Audit

FNV is the **reference title** — the most-validated end-to-end path and the reference realization for NIFAL material/physics and PHYSAL ragdoll. This audit owns **FNV's data through the shared mechanisms** (routing: `.claude/commands/_audit-owners.md`); a defect in the mechanism itself is filed against its owner audit, named under each dimension. A foundation that broke on FNV is the highest-severity finding this command can produce; the rest is regressions and unshipped polish.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and `.claude/commands/_audit-severity.md` for shared protocol.

## Game Context

| Aspect | State |
|---|---|
| NIF / BSA | v20.2.0.7, `bsver` 34 (`bsver::FO3_FNV`) · BSA v104 (`crates/bsa/src/archive/`) |
| Archive priority | Last-listed wins per pool; `Update.bsa` is listed last in all three FNV pools (`assets/debug_profiles.toml`). Guard: `byroredux/src/asset_provider/tests/archive_precedence.rs` |
| Reference data | `/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/` |
| Baselines (pull, never hardcode) | `ROADMAP.md` FNV row (parse rate + Prospector bench-of-record and its commit), `docs/feature-matrix.md`, `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`, `crates/nif/tests/data/per_block_baselines/fallout_nv.tsv` |

**Authoring census** (byte-scan of the shipped data, 2026-09-19 — these prevent false findings):
- 20 archives / 182 177 entries: **0 `_far.nif`, 0 `distantlod\`**. FNV object LOD is `ObjectLodScheme::FalloutLegacyBlocks`; `placement_lod_supported` is Oblivion-only (`placement_lod_supported_is_oblivion_only`). FNV also ships 25 `<world>.level4.high.x<X>.y<Y>.nif` quads (6 in `Fallout - Meshes.bsa`, 19 in `LonesomeRoad - Main.bsa`) that no code path consumes — the same fidelity-only gap as open #4468 (filed for FO3 only; not a coverage hole).
- FalloutNV.esm: **98 SCOL bases / 1 084 REFRs**; 0 MOVS / PKIN / MSWP; 219 `XATO` (all *Activation Prompt* strings, #3511 — not texture overlays) and 0 `XTNM` / `XTXR`.
- 989 `BSSegmentedTriShape` blocks (`fallout_nv.tsv`).

## Parameters (from $ARGUMENTS)

`--focus <dimensions>`: comma-separated numbers (e.g. `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`; `mkdir -p /tmp/audit/fnv`.
2. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
3. Confirm `Fallout New Vegas/Data/` exists (required — FNV is the baseline).
4. Read the FNV rows above; every "regression" is judged against them, not against numbers in this skill.
5. Scope by delta: `git log --since=<last AUDIT_FNV date> --format='%h %cs %s' -- <dimension Paths>`; skim dimensions whose Paths did not change.
6. Toolchain: the `byroredux` bin crate needs rustc >= 1.94 — use the rustup cargo per `docs/contributing.md` § Toolchain note (#4466) or bin-crate tests give no feedback.

## Phase 2: Launch Dimension Agents (parallel)

Ordered by FNV risk: cell load first (highest blast radius), gameplay-data slices last.

### Dimension 1: Cell Loading & Streaming
**Subagent**: `general-purpose`
**Paths**: `byroredux/src/cell_loader/` (general files; `terrain*`, `water.rs`, `lod*`, `object_lod.rs`, `placement_lod.rs` are `/audit-exterior`), `byroredux/src/scene/`, `byroredux/src/streaming.rs`, `docs/engine/pipeline-overview.md`, `docs/engine/exterior-grid-streaming.md`
**First step**: `git log --since=<date> --format='%h %cs %s' -- byroredux/src/cell_loader byroredux/src/scene byroredux/src/streaming.rs`
- Interior `GSProspectorSaloonInterior`: entity/draw counts vs the ROADMAP row; XCLL lighting resolves (`fog_far_color` optional field; an authored LIGH falloff of 0.0 must resolve to the pre-Skyrim quadratic default, `falloff_exponent_sentinel_resolves_per_layout_generation` in `byroredux/src/systems/light_anim.rs`); `NiAlphaProperty` decal routing.
- Exterior WastelandNV radius 3 and the FNV-specific data: default land texture `DirtWasteland01.dds` via `DefaultLandTexture::for_game` (guard `default_land_textures_exist_in_vanilla_archives`, `--ignored`); WTHR/CLMT/terrain/water translation mechanism -> `/audit-exterior`. Repeatable gate: `docs/smoke-tests/m-exteriors.sh fnv static|boundary|soak|cycle|water`.
- Cache and unload hygiene (mechanism: `/audit-safety` Dim 3, `/audit-performance`): `NifImportRegistry` Arc cache (`byroredux/src/cell_loader/nif_import_registry_tests.rs`); unload frees BLAS / Rapier / skin / inventory state (`rapier_release_tests.rs`, `inventory_release_tests.rs`, `unload_skin_cleanup_tests.rs`, `unload_greyscale_lut_tests.rs`, all under `byroredux/src/cell_loader/`). FNV check: after a Prospector -> Goodsprings -> Prospector round trip `stats` returns to baseline.
- Persistent reference state across a cell round trip (door/loot/pickup state, `byroredux/src/cell_loader/reference_state.rs` — mechanism `/audit-gameplay`, `/audit-save`): `docs/smoke-tests/p0-door-interaction.sh fnv`, `p5-save-restart.sh fnv`.
**Output**: `/tmp/audit/fnv/dim_1.md`

### Dimension 2: ESM Data Slice (FalloutNV.esm)
**Subagent**: `general-purpose`
Routing: GRUP walk, `SubReader` byte accounting, schema dispatch, FormID remap -> `/audit-esm`. This dimension owns the semantics that only show on FNV's masters.
**Paths**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/`, `crates/plugin/src/equip.rs`, `crates/plugin/tests/parse_real_esm.rs`
**First step**: `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored parse_rate_fnv_esm` (one test at a time; whole-file `--ignored` runs can spike >20 GB).
- `index.total()` >= `FNV_TOTAL_FLOOR` and the ROADMAP/feature-matrix record counts; do not transcribe counts into this skill.
- Gameplay-record spot checks: Varmint Rifle stats, NCR faction relations, VATS AVIF entries; the FNV actor-value roster and Health resolution (`fnv_actor_value_roster_and_health_resolve_on_shipped_master`); PLDT decode (`parse_rate_fnv_pack_pldt_location`).
- **SCOL is FNV-era** (#1538): `is_scol_era` in `crates/plugin/src/esm/records/parse.rs` must keep dispatching `parse_scol_group` for `Fallout3NV`; re-narrowing it to FO4-only silently drops 1 084 placements. MOVS/PKIN/MSWP are FO4+-only and must not take FNV dispatch. An `unreachable_patterns` warning in `esm/cell/walkers.rs` is a smell.
- **SCPT `SCHR` is a 20-byte header with a `u16` flags tail on Oblivion/FO3/FNV** (#1654): `ScriptRecord.flags` is read via `u16_or_default` in `records/script.rs`; a `u32` read fails every real script and pins flags to 0. Guard: `parse_scpt_extracts_schr_scda_sctx_and_vars` + the real-data script counts.
- LVLI flattening: `expand_leveled_form_id` / `expand_leveled_loot` (`equip.rs`) — NPC outfits and container loot must resolve to base items, not empty. Guard: `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master` + `equip.rs` `expand_leveled_*` tests.
**Output**: `/tmp/audit/fnv/dim_2.md`

### Dimension 3: NIF / NIFAL Slice — the Reference Realization
**Subagent**: `legacy-specialist`
Routing: block decode -> `/audit-nif`; the single-boundary / no-fabrication / no-render-time-fallback rules and collision/particle translation -> `/audit-nifal`; the Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` must be 0 — FNV authors no BGSM/BGEM) -> `/audit-renderer`.
**Paths**: `crates/nif/src/blocks/`, `crates/nif/src/import/`, `byroredux/src/material_translate.rs`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/tests/per_block_baselines.rs`
**First step**: `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines -- --ignored fallout_nv real_archive_torch` (real-data lane; plain `cargo test` never runs it).
- Parse rate and per-block histogram hold vs `fallout_nv.tsv` / ROADMAP; a histogram shift = a mis-dispatched block.
- FNV legacy emissive is `EmissiveSource::Material` (`emissive_source_tests.rs`); Skyrim/FO4 variants must not bleed in.
- FNV must place on the post-Oblivion side of every `bsver` gate (`bsver::FO3_FNV` = 34); `examine_collision_kind` classifies FNV chains `CollisionAuthoring::Classic`; `havok_motion_type` maps BOX_INERTIA (4) to Dynamic (`havok_motion_type_maps_full_enum`).
- Emitter birth rate reaches the ECS from the *authored* controller chain, per emitter instance: the per-game rate floor `real_archive_torch_meshes_surface_particle_emitters` (`--ignored`) must be green for FNV. It was red on main 2026-09-12..19 (#4467) because the fix session ran only the default lane — any change under `crates/nif/src/import/walk/` needs this lane run.
- `MAX_NIF_NODE_DEPTH` (128) never trips on a legit FNV scene (`crates/nif/src/import/walk/tests.rs`).
**Output**: `/tmp/audit/fnv/dim_3.md`

### Dimension 4: Animation, Skinning & PHYSAL Ragdoll (FNV reference slice)
**Subagent**: `legacy-specialist`
Routing: the solver end (collider translation, fixed-step, `build_ragdoll`, controller) -> `/audit-physics`; clip registry / lock shape -> `/audit-ecs`. This dimension owns the **source axis**: does FNV's authored bhk chain reach the canonical spec intact.
**Paths**: `crates/nif/src/anim/`, `crates/nif/src/import/collision/`, `byroredux/src/ragdoll.rs`, `byroredux/src/anim_convert.rs`, `byroredux/src/npc_spawn.rs`, `docs/engine/physal.md`
**First step**: `cargo test -p byroredux-nif --release --test ragdoll_import -- --ignored fnv_` and `docs/smoke-tests/m41-ragdoll.sh`.
- Classic-era guards, verified on FNV data (FO3 re-verifies on its own): B-spline pose fallback gated on the `FLT_MAX` sentinel (`crates/nif/src/anim/bspline.rs`; B-splines are reachable on FNV/FO3); clip registry dedup by lowercased path (`get_or_insert_by_path_dedupes_repeated_calls`); NPC hand meshes load beside `upperbody.nif` on kf-era NPCs (`byroredux/src/npc_spawn.rs`) — bodies with no hands are the regression.
- Ragdoll: `template_from_imported` / `extract_ragdoll` warn on dropped bodies/constraints rather than silently shrinking; FNV's dominant constraint is a `bhkMalleableConstraint` wrapping a Ragdoll (`docs/engine/physal.md` § FO3/FNV) and must yield a jointed body. Real-data guard: `fnv_protectron_skeleton_is_one_connected_component` (`crates/nif/tests/ragdoll_import.rs`). `bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint` / the ball-socket chain are decoded (#4212) but `extract_ragdoll` warns and drops them (no canonical joint kind) — a sole-link drop detaches a limb; no FNV occupancy census exists (#3792 left it open), so measure before assuming there is no FNV example. Writeback must not corrupt the skinned palette (`byroredux/src/ragdoll_installed_tests.rs`).
- No per-game branch in `ragdoll.rs` or the solver bridge — the only per-game seam is the constraint CInfo decode.
**Output**: `/tmp/audit/fnv/dim_4.md`

### Dimension 5: Ambient AI, Consumables & HUD — FNV Data Through Gameplay Mechanisms
**Subagent**: `general-purpose`
Routing: procedure runtimes, package selection, seat reservation, CTDA fail-open, consumable/restoration/loot mechanics, HUD driver -> `/audit-gameplay`; MenuXml eval/layout/raster -> `/audit-ui`, parse side -> `/audit-parsers`; character formulas -> `/audit-character`. Check here only what FNV authors.
**Paths**: `byroredux/src/npc_spawn.rs`, `byroredux/src/boot/schedule/post_update.rs`, `byroredux/src/hud.rs`, `byroredux/src/inventory.rs`, `crates/menuxml/src/profile.rs`, `docs/engine/npc-spawn-ai-packages.md`, `docs/engine/playable-vertical-slice.md`
**First step**: `git log --since=<date> --format='%h %cs %s' -- byroredux/src/npc_spawn byroredux/src/hud.rs byroredux/src/inventory.rs crates/menuxml`
- **Ambient locomotion is default-on** (M42.10): sandbox seat, Wander/Travel/Follow/Escort/Guard/Patrol and walk playback register unconditionally behind the single `BYRO_NO_AI_LOCOMOTION=1` kill-switch; the seven older `BYRO_*` opt-in variables are not read. Guard: `ambient_locomotion_default_on_tests` in `byroredux/src/boot/schedule/mod.rs`. A finding that "NPCs walk without an opt-in" is a false premise.
- Walk clips are FNV assets: `humanoid_walk_kf_path` resolves `locomotion\{male,female,child}\mtforward.kf` under `meshes\characters\_male\` per `(gender, is_child)`, child gated on the RACE child flag; `walk_speed_for` derives per-actor speed from the clip's authored stride, sane range [30, 250] u/s, fallback `LOCOMOTION_WALK_SPEED`. Confirm on real FNV NPCs that female/child actors get their own clip and a measurable stride.
- **Consumables / Hardcore** (mechanism: `/audit-gameplay` Dim 3): FNV's eligible plans per the spec (`docs/engine/playable-vertical-slice.md` § P3) are Stimpak (`00015169`), Blood Pack, Bitter Drink. `IsHardcore` (CTDA 586) selects the saved `HardcoreMode` flag (`crates/core/src/ecs/resources/hardcore.rs`); hunger/thirst/sleep/ammo weight/companion death are documented as unbuilt (that doc, 2026-09-16). Only Stimpak has a real-master test: `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health` (`--ignored`, needs FO3 + FNV masters).
- **HUD profile** (`HudGameProfile::fallout_nv`): assembled style (meters grafted from `menus\prefabs\meter.xml`, compass from `template_compass_window`), 2 bars = Health `0x2C9` / ActionPoints `0x2D0` (`crates/core/src/character/fallout.rs`), 9 font slots (FO3 has 8) from `Fallout - Textures2.bsa`, menu XML from `Fallout - Misc.bsa`. The profile is chosen from the `--esm` file name containing `falloutnv`. Guard for the font table: `profiles_pin_corpus_facts`. **Coverage gap**: the corpus test (`crates/menuxml/tests/fo3_corpus.rs`) and smoke (`m48-5-fo3-hud.sh`) are FO3-only — verify FNV by hand: `--game fnv --hud`, then `hud.status` / `hud.values 0.3 0.9`.
**Output**: `/tmp/audit/fnv/dim_5.md`

### Dimension 6: Real-Data Validation, Bench-of-Record & Smoke Gates
**Subagent**: `general-purpose`
Routing: renderer correctness of RT lighting / denoise / sky on FNV scenes -> `/audit-renderer`, `/audit-exterior`; runtime telemetry diff -> `/audit-runtime --game fnv`.
**Paths**: `assets/debug_profiles.toml`, `byroredux/src/game_profiles.rs`, `scripts/fsr-bench-matrix.sh`, `docs/smoke-tests/`, `.claude/audit-baselines/runtime/`
**First step**: `git log --since=<date> --format='%h %cs %s' -- scripts/fsr-bench-matrix.sh docs/smoke-tests assets/debug_profiles.toml`
- **Use `--game fnv`** for anything not the bench-of-record (#3346): it expands to absolute `--esm` / `--bsa` / `--textures-bsa` paths, so CWD cannot break it. A vanilla `Data/` has no bare `Meshes.bsa` (`Fallout - Meshes.bsa`; `--bsa` opens the literal path) and a bare-name run from the wrong CWD loads ~36 entities with a spurious FPS.
- **Bench-of-record**: `scripts/fsr-bench-matrix.sh 3 300` (encodes archives, CWD, upscaler sweep and the #3347 sanity gates), or single-config `cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --upscaler taa --bench-frames 300 --bench-hold` then `byro-dbg` (port 9876) `stats`. **`--upscaler taa` is mandatory for comparison**: the flag defaults to `fsr3`, and comparing FSR-Quality FPS to the ROADMAP TAA-native headline reads as a fake ~75% win (#2560). Compare entity/draw/FPS/fence to the ROADMAP FNV row at its recorded commit; the pre-collider baseline was never recovered, so read ROADMAP Known Issues before flagging fence/FPS.
- Chrome/posterized surfaces: run `tex.missing` first (`Update.bsa` + `Fallout - Textures2.bsa` split the texture set). The committed v104 real-data guard is `fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic` (`crates/bsa/tests/bsa_real.rs`, `--ignored`); reader discipline is `/audit-parsers`.
- Playable-slice gates on FNV (`docs/smoke-tests/README.md`): `p0-door-interaction.sh fnv`, `p1-character-traversal.sh fnv`, `p5-save-restart.sh fnv`, `w1-water-traversal.sh fnv` (real capsule Lake Mead shore -> swim -> dive -> surface -> shore -> cell boundary), `m-exteriors.sh fnv`. Missing data is exit 77, never a pass.
**Output**: `/tmp/audit/fnv/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fnv/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FNV_<TODAY>.md`:
   - **Executive Summary** — FNV is the baseline; any regression against the ROADMAP-recorded numbers is at least HIGH (CRITICAL if it breaks a shipped foundation).
   - **Dimension Findings** — grouped by severity per dimension.
   - **Baseline Comparison Table** — ROADMAP number vs observed for entity count, draw count, FPS, fence, parse rate, record count (cite the ROADMAP commit compared against).
   - **Regression Guard List** — previously-fixed issues verified still correct.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FNV_<TODAY>.md`
(label every finding `game:fnv` + `legacy-compat`, plus its own domain label.)
