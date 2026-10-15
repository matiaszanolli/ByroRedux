# Tech-Debt Audit — 2026-10-15

**HEAD** f4ae42a6f (code state; f44dacbda adds only audit-skill edits) · **Prior report** `docs/audits/AUDIT_TECH_DEBT_2026-10-12.md` (HEAD 5d31fd637, ~330 commits / 718 files / +92k lines earlier) · **Depth** deep · **Dimensions** 1–9, one agent each (5+6 and 7+9 paired), all read-only.

Dim 3 and Dim 4 ran after the `/audit-sync` re-sync of the audit skills, so skill rot is measured against freshly synced files. The one-sentence story of this report: **the M47.2 Papyrus-native wave (~140 commits in three days) is the entire complexity delta**, and it also produced the report's single HIGH (an unbounded allocation in the new VM).

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 8 |
| LOW | 62 |
| **Total** | **71** (72 raw; `TD6-2026-10-15-03` folded into `TD8-2026-10-15-04`, same seven dead `populate_*_fragments` wrappers, per the Dim 8 > Dim 5/6 rule) |

By dimension: Dim 1 ×10 · Dim 2 ×23 · Dim 3 ×13 · Dim 4 ×5 · Dim 5 ×1 · Dim 6 ×2 · Dim 7 ×6 · Dim 8 ×6 · Dim 9 ×5.

- **HIGH (1):** `TD7-2026-10-15-01` — the Papyrus VM's `new T[n]`, `Array.Add(x, count)` and string `+` have no size cap; `MAX_STEPS` counts statements, not memory, so one statement from an installed mod's `.pex` can kill the process. Demote to MEDIUM only if mod scripts are treated as trusted; the fix is the same.
- **MEDIUM (8):** seven duplication findings, each with divergent bug-fix history (`TD2-01`..`-07`: the NIF shape cull gate in 8 arms, twin tangent synthesizers, the two NPC `Finalize` tails, `CachedNifImport` built twice, four WATR decoders, ten Papyrus dispatch tails, the NIF/core animation type twin), and `TD9-01` (about 218 of 322 `#[ignore]` tests run in no lane; at least 45 pin fixes for closed HIGH/CRITICAL issues).
- **The blocking clippy gate is red again** (`TD8-2026-10-15-01`: `byroredux-renderer` build script + 11 lints in `byroredux-scripting`, all from the last 48 h; this leaves the renderer lib and the `byroredux` bin unlinted). Filed LOW per the severity table, but it is the first thing to fix.
- **Audit-skill rot is nearly zero** after the sync (Dim 4: 0 STALE refs, 0 closed-issue hits across 218 refs, all 507 cited hashes resolve, baselines re-measure exactly). What remains is the validator's own advisory noise (#5607) and `.claude/agents/*.md`, which neither the validator nor the sync covers.
- **Closed since 10-12 (verified):** #5735, #5467, #5468, #5736, #5737, #5741. **Still live from 10-12:** #5738, #5739, #5740 (re-verified), #5746 (TD9), the `main.rs` space-run exemption (never filed, third report), the weak `#[cfg(test)]` cut (never filed, regression of #5469).

### Delta vs the 2026-10-12 baseline

| Metric | 10-12 | 10-15 | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 24 | 24 | same hits; 15 `XXXX` + 2 FO4 `HACK` FourCC false positives; `triangle.frag` MIT block intact (now L25–43) |
| `allow(dead_code)` | 44 | 41 | 12 are `cfg(test)` markers in `lock_tracker.rs`; every production site has a reason |
| `unimplemented!` / `todo!()` | 0 | 0 | the only `panic!("not …")` is a test arm |
| `#[ignore]` tests | 299 | 322 | +23, all data gates (22) or the VM timing census (1) |
| production files >2000 LOC | 4 | 6 | `fragment/vm.rs` (new), `translate/effects.rs` (1933→5295), `plugin/esm/cell/mod.rs`, `condition.rs` crossed; `components.rs` / `fragment/effects.rs` left via #5467 / #5468 |
| production fns >200 LOC | 190 | 199 | `apply_at_depth` 188→621 is the headline |
| test-heavy files >2000 total LOC | — | 84 | none escalates |

## Baseline Snapshot (Phase 1, measured at f4ae42a6f)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 24
allow(dead_code):      41
unimplemented!/todo!(): 0
#[ignore] tests:        322
files >2000 production LOC: 6
test files >2000 total LOC (separate bucket): 84
```

`prod_loc` self-test: ok. Production >2000 LOC: `crates/scripting/src/fragment/vm.rs` 6213, `crates/scripting/src/translate/effects.rs` 5295, `crates/renderer/src/shader_constants_data.rs` 2900 (data table), `byroredux/src/save_io.rs` 2136, `crates/plugin/src/esm/cell/mod.rs` 2108, `crates/scripting/src/condition.rs` 2005. Watch-list within 5%: `crates/nif/src/import/types.rs` 1997, `crates/audio/src/lib.rs` 1977, `crates/scripting/examples/mq101_conformance.rs` 1974 (#5586), `crates/debug-ui/src/panels.rs` 1952, `crates/plugin/src/esm/records/misc/world.rs` 1912.

## Top 10 Quick Wins (trivial / small)

1. **`TD7-01` (HIGH, small)** — add a named `MAX_ARRAY_ELEMENTS` (engine limit 128; the static path already caps at 512) and a string-length cap in `vm.rs`; one test per path.
2. **`TD8-01` (small)** — clear the 12 lints that turn the blocking clippy gate red (`manual_is_multiple_of` at `shader_constants_data.rs:683` and 11 in `byroredux-scripting`); it unblocks linting of the renderer lib and the bin.
3. **`TD3-03/-06/-07/-08` (trivial ×4)** — stale comments: `vm.rs:50` "Not modelled: structs", the LCTN note on `DISPATCH_HANDLED_FOURCCS`, the m42 smoke "seats the settler" text in `CLAUDE.md` + smoke README + script banner (user-owned file: needs the user's edit), `ci.yml` "733 unsafe blocks" → 704.
4. **`TD8-03` (trivial)** — remove the four unused dev-deps and run `cargo machete --with-metadata` (the CI job cannot see `[dev-dependencies]`; `cxx-build` needs an ignore).
5. **`TD4-05` (trivial)** — close #5742 and #5606: fixed in `f44dacbda`, which has no `Fix #N`, so they auto-stayed open.
6. **`TD2-02` (MEDIUM, small)** — collapse the two tangent synthesizers (`tangent.rs`): three prior fixes each caught one copy up to the other.
7. **`TD2-01` (MEDIUM, small)** — one shape cull-gate helper for the eight walker arms; #332 / #3640 / #4561 each hand-applied the fix.
8. **`TD6-01` (small)** — `scripting.md` claims `RecurringUpdate` is "shipped (recognizer path)" (the #5608 fix added this); nothing constructs it and six systems (five exclusive) run every frame for nothing. Fix the docs or wire it.
9. **`TD1-09` (trivial)** — `Effect::kind()` replaces the 95-arm `effect_kind` pasted into two example binaries.
10. **`TD9-03` (trivial)** — three scripting real-data test files skip green when game data is missing; make them honour `BYROREDUX_REQUIRE_GAME_DATA` (sibling of open #5784). Also `TD2-23` (re-point the 18 weak `#[cfg(test)]` cuts at `source_scan.rs`, regression of #5469).

## Top 5 Medium Investments

1. **The M47.2 seam (`TD1-01` + `-02` + `-03` + `-04` + `-08`, with `TD2-06`).** One root cause, so one coordinated investment instead of four splits: give each subsystem an `install` entry point (resources, systems, saved registry rows) so the six central hub lists stop growing with every native; add `World::init_resource::<R: Default>()` (the insert-if-absent idiom occurs 160 times); split `vm.rs` and `translate/effects.rs` along the same Papyrus-API-family axis so a native's lowering, applier and VM read/check halves pair by file name (precedent: #5467 / #5468); extract `apply_at_depth`'s 45 flushes into `flush_*` methods first. Hazards: `include_str!`/`SOURCES` source-scan tests (`fragment.rs` `SOURCES`, `condition.rs`, `save_io.rs`, `boot/schedule/mod.rs`) must follow the splits.
2. **Test-lane honesty (`TD9-01` + `TD9-02` + `TD9-04` + `TD9-05`).** About 218 ignored tests run in no automated lane and 45+ pin closed HIGH/CRITICAL fixes; the `combustion_lab_60f` golden differs from the 09-21 baseline in 98.9% of pixels after 103 shader commits and CI never runs it (`rt-correctness` is manual-dispatch only). Decide which gates need a scheduled lane and regenerate the golden.
3. **Spawn / import duplication with fix-history (`TD2-03` + `-04` + `-07`).** Share the `Finalize` tail between runtime and prebaked NPC spawn (the Draugr marker reached prebaked two days late, #4700), build `CachedNifImport` once, and collapse the `nif::anim` / `core::animation` twin types.
4. **Catalog as a table (`TD1-05` + `TD1-06`).** `condition.rs` states a 49-variant catalog in six parallel exhaustive matches; `plugin/esm/cell/mod.rs` is ~85% record data types. Row table + split by record family, `pub use` hubs keep callers unchanged.
5. **WATR decoder head (`TD2-05`).** Four decoders restate the same head in three read styles; #3105, #3107, #3146 and #4931 were each per-decoder fixes.

## Deferred

- The M47.2 splits (investment 1) are best sequenced after the Papyrus-native wave settles; the file is changing ~25 commits a day, so a split now fights merges. `TD7-02` and the `TD1-04` flush extraction can land first because they do not move files.
- `CellData::sky_region` (TES5/FO4 `XCCM` as a REGN) has no consumer yet (forward scope of #5463), as do `AttachPoints` / `ChildAttachConnections` until the equip system reads them (`TD6-02`).
- `loading_screen.rs:10` / `:47` MOD2 TODOs are 13 days old with no issue; they become reportable on 2026-11-01.
- Dim 3 `TD3-13` (toolchain note: the agent saw rustc 1.96.0 on PATH, the doc says the distro default is 1.93.1) needs the user to confirm on their own shell.
- Findings that touch `CLAUDE.md` (`TD3-07`, `-10`, `-13`) are the user's file: report only.

## HIGH Findings

#### TD7-2026-10-15-01: The Papyrus VM's `new T[n]`, `Array.Add(x, count)` and string concatenation have no size cap, so one statement can exhaust memory
- **Severity**: HIGH (promotion rule "a magic/missing bound would silently over/underflow under documented use" applied to a missing bound; see Severity note)
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/scripting/src/fragment/vm.rs:2039-2042` (`Expr::New`), `:2314-2320` (`"add"` array method), `:4339-4340` and `:4395` (string `+` / `StrCat`)
- **Status**: NEW
- **Age**: `Expr::New` `e766d3147` (2026-10-13, "the Papyrus VM runs quest fragments the lowering declines"); array methods `20cbfd7b7` (2026-10-13)
- **Effort**: small
- **Description**: the VM's only budgets are `MAX_STEPS = 100_000` statements per run (`:87`, enforced in `tick()` `:1038-1044`) and `MAX_DEPTH = 16` (`:89`); both count statements, not memory. Three single-statement allocations are unbounded:
  1. `new Int[n]` evaluates `n` as an `f32`, casts with `as usize` (saturating, so `1e10` stays 10 billion) and runs `vec![Value::default_for(ty); size]`. There is no cap, and `Value` is a 32-byte-class enum (`String` variant).
  2. `arr.Add(x, count)` loops `for _ in 0..int(1, 1).max(0)` pushing `wanted.clone()` each time: `count` is an `i32`, so up to 2^31-1 pushes in one step.
  3. `s = s + s` (`binary()` / `arithmetic()` build `format!("{}{}", ..)`) doubles the string per statement; about 35 statements reach terabytes, far under `MAX_STEPS`.
- **Evidence**:
  ```rust
  Expr::New { ty, size } => {
      let size = self.eval(frame, &size.node)?.number().unwrap_or(0.0).max(0.0) as usize;
      Value::Array(Rc::new(RefCell::new(vec![Value::default_for(&ty.node); size])))
  }
  ...
  "add" => { for _ in 0..int(1, 1).max(0) { items.push(wanted.clone()); } Value::None }
  ```
  The sibling static-lowering path DOES cap: `crates/scripting/src/papyrus_provider/lower_call.rs:629-635` declines `new T[n]` unless `n` is an `IntLit` `<= MAX_SCRIPT_ARRAY_ELEMENTS` (`crates/sdk/src/script_function.rs:23`, 512), and the engine's own limit is 128 (`falloutck-uesp-wiki/(main)/Arrays (Papyrus).wiki`: "up to 128 elements. Attempting to create an array larger then that will fail"; same for `Add`/`Insert`). The ahead-of-time check only recurses into the size expression (`vm.rs:5108-5111`). `catch_unwind` exists only around translation/population (`translate/mod.rs:249`, `fragment/populate.rs:312,548,707`), not around a VM run, so an allocation failure aborts the process and a `capacity overflow` panic unwinds through a scheduler system.
- **Impact**: a .pex from any installed mod (or a corrupted one) kills the engine process with a one-liner; no vanilla script does this, but the VM is the first component that executes untrusted script bodies at all (the `MAX_STEPS` limit exists precisely because of that), and the same window shipped sandbox caps for sibling attack surface (#5718 hostcall fuel, #5719 CWS inflate ceiling).
- **Severity note**: documented-use values are all <= 128, so the HIGH rests on "silent process death from untrusted input", not on a legitimate script overflowing. If the maintainers treat mod scripts as trusted, demote to MEDIUM; the fix is the same.
- **Related**: #5718, #5719, `MAX_SCRIPT_ARRAY_ELEMENTS`, `/audit-scripting`.
- **Suggested Fix**: add a named `MAX_ARRAY_ELEMENTS` (engine 128, or reuse the SDK's 512) in `vm.rs`; make `Expr::New`, `Add` (`len + count`), `Insert` and array literals `stop("array size")` past it, and cap string results at a named length in `binary()`. One test per path beside `vm/tests.rs:4900` (`Counts = new Int[0]`).


## MEDIUM Findings

#### TD2-2026-10-15-01: Both NIF import walkers re-spell the shape cull gate eight times; every fix to it had to be hand-applied per arm and one arm family (particles) was missed
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/import/walk/mod.rs` hierarchical walker `:525-547` (NiTriShape), `:578-600` (BsTriShape), `:631-642` (NiLodTriShape), `:671-681` (BSGeometry); flat walker `:925-947`, `:969-991`, `:1009-1020`, `:1044-1054`; node-level copies at `:355-361` / `:801-806` (switch) and the NiNode arms
- **Status**: NEW
- **Age**: gate introduced `a050c5d38` (2026-04-17); mask changed `b06692d0e` (#332, 2026-04-20); live-NiVisController clause `452cccd0b` (#3640, 2026-09-02); particle arms brought into line `9281eebd7` (#4561, 2026-09-23)
- **Effort**: small
- **Description**: every geometry arm opens with the same 6 lines: `if shape.av.flags & 0x01 != 0 && !has_live_visibility_controller(scene, shape.av.net.controller_ref) { return; }` then `if is_editor_marker(shape.av.net.name.as_deref()) { return; }`, and each carries its own copy of the 12-line APP_CULLED / #332 / #3640 comment. Eight shape copies plus four node copies. The particle path already has the right shape (`particle_system_is_culled`, `:236`) but the shape arms never adopted a sibling.
- **Evidence**: the divergence is in the git history, not hypothetical. #332 changed the cull mask from `0x21` to `0x01` and had to touch each arm; #3640 added the `has_live_visibility_controller` clause to all eight; #4561's own commit message says "Every node and geometry arm in both import walkers gates on APP_CULLED ... but neither NiParticleSystem arm read the block's own flags or name" - an emitter authored hidden spawned from frame zero for ~5 months because the particle arms were outside the copy set.
- **Impact**: the next cull rule (a new APP_CULLED-like flag, a Starfield `BSGeometry` visibility bit) is a 12-site edit and a miss silently draws hidden content on one walker only (hierarchical = cell placement, flat = collision/legacy path).
- **Related**: #332, #3640, #4561, #5485 (revisit guard already lives once at `:282`).
- **Suggested Fix**: add `fn av_is_culled(scene: &NifScene, av: &NiAVObject) -> bool` beside `particle_system_is_culled` (flags bit + `has_live_visibility_controller` + `is_editor_marker`), have `particle_system_is_culled` call it first, and replace the 12 inline gates with `if av_is_culled(scene, &shape.av) { return; }`. Keep the 12-line rationale once, on the helper.

#### TD2-2026-10-15-02: `synthesize_tangents` and `synthesize_tangents_yup` are the same Lengyel accumulation plus Gram-Schmidt finalize twice; three fix rounds each landed on one copy first
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/import/mesh/tangent.rs:170-358` (Z-up) vs `:396-575` (Y-up); callers `bs_tri_shape.rs:217,242`, `bs_geometry.rs:338`, `ni_tri_shape.rs:74,101`
- **Status**: NEW (10-11 / 08-30 only touched this file for the coord-swap call sites)
- **Age**: `df60da808` (#1516) -> `b09cec5c3` (#2632, 2026-08-08, Y-up only) -> `8075133c5` (2026-08-18, "Orthogonalize degenerate Z-up tangents", Z-up catch-up) -> `19742e805` (#3176/#3177, 2026-09-02, "tangent producer parity")
- **Effort**: small
- **Description**: the 40-line per-triangle accumulation (`x1..z2`, `s1..t2`, `det`, `sdir`, `tdir`, the six `tan_u/tan_v` adds) and the 60-line per-vertex finalize (normalize N, degenerate-permutation fallback with `fallback_orthogonal_seed`, T/B Gram-Schmidt, `bitangent_sign`) are line-for-line the same; the Z-up copy differs only by calling `zup_to_yup_pos` on the inputs of the finalize. The comments in the Y-up copy already say "mirroring `synthesize_tangents_yup`'s established guard" and "matching the Y-up producer's #2632 guard" - the duplication is acknowledged in prose and maintained by hand.
- **Evidence**: three commits above, each titled as a catch-up of one producer to the other: #2632 normalised N in Y-up only; the Z-up copy got it 10 days later (#3177 is literally "Z-up sibling"); #3176's zero-tangent seed was added to both in the same commit only because the audit named both.
- **Impact**: tangent basis feeds every normal-mapped surface; a lagging producer shows as the "chrome / posterized" class (see #786's history in the same file).
- **Related**: #786, #1516, #2632, #3176, #3177.
- **Suggested Fix**: make `synthesize_tangents` a thin wrapper: convert `vertices`/`normals_zup` to Y-up with `zup_to_yup_pos` and call `synthesize_tangents_yup` (the finalize already runs in Y-up space in both). Extract the per-vertex Gram-Schmidt into `fn orthonormal_basis(n, t, b) -> ([f32;3],[f32;3])` if the degenerate-permutation choice must stay space-specific (the doc comment calls that divergence "documented"; drop it, any orthogonal-to-N direction is valid).

#### TD2-2026-10-15-03: The runtime and pre-baked NPC spawn arms each carry a ~100-line `Finalize` tail; #5312 shared the Armor/Skeleton units but not this, and #4700 was the Draugr marker missing from one copy
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:354-449` (`finalize_prebaked`) vs `runtime.rs:797-936` (`RuntimePhase::Finalize`); also the armor-phase arm `prebaked.rs:318-348` vs `runtime.rs:765-795`
- **Status**: NEW (partial: #5312 "share the Armor and Skeleton spawn units" closed the unit bodies, not the phase glue or finalize)
- **Age**: `ec3a18d2f` (2026-09-21, Draugr marker on runtime path only) -> `3978b5184` (#4700, 2026-09-23, prebaked catch-up); `774560dce` walk speed; `37db35cca` (#5091, 2026-10-01) split the two arms into files and froze the copy
- **Effort**: small
- **Description**: both tails insert `AnimationTarget`, `NpcSkeletonBones(skel_map.clone())`, the `DraugrCombatAnim` marker, the `WalkAnimation` + `WalkSpeed` pair (identical 20 lines: registry duration, `last_pos` from `Transform`, `captured: None`, `transition_secs: 0.0`), the `ResolvedNpc::resolve` + `apply_ai_package_behavior` + `arm_combat_disposition` triple, `tag_descendants_as_actor`, and `loot_appearance::install` with `mem::take(&mut state.appearance)`. The armor-phase arm (call `spawn_armor_unit`, the 8-argument call, the identical `log::debug!` "armor ... not in archives", the `next = index + 1` transition) is a second 30-line twin; the prebaked comment even says "(see the runtime Finalize twin)".
- **Evidence**: the Draugr combat marker landed in `ec3a18d2f` on the runtime arm only; `3978b5184` (#4700) states the prebaked path "stayed silent on Skyrim, the only game with Draugr" until two days later, and its inserted comment reads "beside `AnimationTarget` as on the runtime path".
- **Impact**: every new per-actor component (the M42 behaviors, M47.2 behavior-graph state, the next AI marker) is a two-arm edit in spawn glue; the prebaked arm is the Skyrim+/FO4 path, so a miss shows on exactly the newest games.
- **Related**: #5312, #5091, #4700, #5535 (touched both in a lock-order fix).
- **Suggested Fix**: in `resumable/mod.rs` add `fn attach_skeleton_runtime(world, root, skeleton, skel_map, draugr: bool)`, `fn attach_walk_animation(world, root, handle)` and `fn finish_actor(world, root, npc, index, player_body, appearance, skel_map) -> UnitOutcome`; both finalize arms keep only their path-specific head (inventory/equipment slots, idle KF, walk-clip selection) and call the three helpers. A shared `advance_armor_phase(state, index, ...) -> Option<next_index>` removes the armor twin.

#### TD2-2026-10-15-04: `CachedNifImport` is assembled twice with a mirrored 60-line scene-satellite extraction; the streaming copy lagged on two fields (#3072/#3074) and M47.2 has just pasted a third block into both
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/partial.rs:60-200` (streaming partial, build at `:158`) vs `byroredux/src/cell_loader/references/import.rs:45-270` (`parse_and_import_nif`, build at `:193`); `import.rs:520` is the deliberate SpeedTree shell and is not part of this
- **Status**: NEW
- **Age**: `8209a03f9` (#3072/#3074, 2026-09-02: furniture markers and flame-attach offset hard-coded `None` on the partial path citing a blocker that was false); M47.2 `graph_nodes`/`behavior_graph_file` block (2026-10-13/14) pasted verbatim into both
- **Effort**: small
- **Description**: both functions run `collision_authoring`, `find_attach_light_offset`, `find_projectile_node_offset`, `extract_furniture_markers` -> `furniture_component`, `extract_behavior_graph_file` -> `graph_nodes::build` -> `import_kf`, `extract_phantom_bounds`, the post-merge `merge_external_materials` snapshot and the embedded-clip registration, then spell the same 25-field struct literal. The partial path's comments cite the sync path ("computed the same way the synchronous `parse_and_import_nif` path does", "mirroring the synchronous path (`references/import.rs`)") three times.
- **Evidence**: `8209a03f9` message: the partial path "used to hardcode `None`, citing the same (false) blocker" for `attach_light_offset` and `furniture`; the identical 5-line `behavior_graph_file`/`graph_nodes`/`graph_sequences` block now sits at `partial.rs:150-156` and `import.rs:182-190` (clone scan: 9 identical windows).
- **Impact**: cell-streamed REFRs (every exterior cell) take the partial path; a satellite added to only the sync path (loose NIF, interior) is silently absent from streamed cells, which is precisely the #3072 failure.
- **Related**: #3072, #3074, #1594 (the one deliberate asymmetry: `attach_points`/`child_attach_connections` are `None` on partial), #4290.
- **Suggested Fix**: add `fn scene_satellites(scene: &NifScene, meshes: &[ImportedMesh], collisions: &[ImportedCollision]) -> SceneSatellites { collision_authoring, attach_light_offset, projectile_offset, furniture, behavior_graph_file, graph_nodes, graph_sequences, phantom_bounds }` in `nif_import_registry.rs` next to `CachedNifImport`, and `CachedNifImport::from_scene(...)` taking it; the partial path overrides only `attach_points: None` with the #1594 comment.

#### TD2-2026-10-15-05: Four WATR `DATA`/`DNAM` decoders restate the same wind/wave/specular/fog/colour head in three read styles; every correction (#3105, #3107, #3144, #3146, #4931) was a per-decoder fix
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/records/misc/water.rs`: `decode_data_short` `:500-568`, `decode_data_oblivion` `:570-680`, `decode_data_fo3nv` `:681-740`, `decode_dnam_pre_fo4` `:864-933` (the same head again at `decode_dnam_fo4` `:1118`)
- **Status**: NEW
- **Age**: head repeated since the WATR decoders landed; #3146 `845c1aa30` (2026-09-09) deleted one unreachable copy of it; #4931 `55dd8fa7e` (2026-09-28) diverged the authored flag per arm
- **Effort**: medium
- **Description**: wind speed at 0, wind direction (degrees -> radians, #3144) at 4, wave amplitude/frequency at 8/12, `sun_specular_power.clamp(1.0, 2048.0)`, reflectivity/fresnel `clamp(0,1)`, `fog_near.max(0)`, `fog_far.max(near+1)` and three colours appear in each, once through `SubReader::f32_finite`, once through `read_f32_at` + `for (dst, offset) in [...]`, once through `u8_to_linear(data[40])`. Only the fog/colour offsets (+4 on Oblivion, +0 elsewhere) and the authored-flag policy differ, and that is exactly what a layout table would state.
- **Evidence**: the doc comments are a list of one-copy-fixed bugs: `decode_data_short`'s comment says its tail reads "used to sit here too, structurally unreachable ... assigning offsets that contradicted the long decoder for the same fields (#3146)"; `decode_data_fo3nv`'s #3107 note says the majority 196/184-byte `DNAM` form "ran `decode_dnam_pre_fo4` alone ... left at canonical defaults on 79% of FO3 and 90% of FNV water types"; #3105 swapped the Oblivion rain/displacement starting sizes (5x wrong, "both live to the GPU").
- **Impact**: WATAL (the all-games water goal) consumes these values straight into `GpuWaterParams`; a missed copy is a wrong-looking water type on one game, found only by a per-record bytes census.
- **Related**: #3105, #3107, #3144, #3146, #4931, #5596 (the `read_f32_at` helper).
- **Suggested Fix**: `struct HeadLayout { fog_near: usize, colours: usize, wind_direction_authored: fn(f32)->bool }` and one `fn decode_legacy_head(p: &mut WaterParams, data: &[u8], layout: &HeadLayout)` in `water.rs`; the four decoders call it and keep only their tails (`apply_fo3nv_tail`, Oblivion simulator block, Skyrim tail). Add a test that decodes the same synthetic bytes through each layout and pins the shared fields.

#### TD2-2026-10-15-06: Ten Papyrus-call dispatch tails repeat `Effect::Interpret` -> `apply_fragment_guard_free(from_ref)` -> player lookup -> `push_quest_stage_advances`; the #5763 frame-budget gate reached only the ones touched that day
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/hits.rs:246-281`, `distance.rs:272-295`, `package.rs:256-274`, `perk.rs:68-86`, `terminal.rs:67-85`, `events.rs:257-264`, `updates.rs:415-451`, `reference.rs:472-504` (plus the console twins at `reference.rs:377,410`); budget helper `budget.rs:146,164`
- **Status**: NEW
- **Age**: M47.2 dispatchers 2026-10-12..13; the budget `195e3b4ef` (#5763, 2026-10-13)
- **Effort**: small
- **Description**: `package_fragment_system`, `perk_fragment_system`, `terminal_fragment_system` are the same 25-line scaffold (take the pending resource -> clone the fragment table -> look up the player -> loop building an `Effect::Interpret { script, function, args, resume: None }` -> `apply_fragment_guard_free(world, std::slice::from_ref(&effect), owner, vmad)` -> `let Some(player) = player else return` -> `push_quest_stage_advances`); `hits.rs`, `distance.rs` and `updates.rs` carry the same 12-line epilogue (`advances.is_empty()` -> `try_resource::<PapyrusPlayerEntity>()` -> push). Clone scan: 7 identical windows across `hits`/`distance`/`updates`.
- **Evidence**: the budget is a hand-copied `if super::budget::exhausted(world) { match super::budget::defer(...) { Ok(()) => continue, Err(mut e) => effects.remove(0) } }` at `updates.rs:427` and `reference.rs:480` (plus `events.rs:263` which always defers, and the two in `systems.rs`). `budget.rs:1-40` states "Every interpreter system ... is an exclusive Update system" and lists four; `hits.rs`, `distance.rs`, `package.rs`, `perk.rs` and `terminal.rs` still call `apply_fragment_guard_free` unconditionally, so an `OnHit` / `OnDistanceLessThan` / package / perk / terminal burst runs to completion in the frame it is raised, which is exactly the cost #5763 set out to remove.
- **Impact**: the per-frame interpreter bound is not a property of the system, it is a property of each dispatcher that remembered to copy the gate; the next dispatcher (the M47.2 list is still growing, 6 commits in 2 days) defaults to unbudgeted.
- **Related**: #5763, #5592 (the same consolidation, for the extension host side), `fragment/effects.rs:758` (budget gate inside a continuation).
- **Suggested Fix**: `fragment/dispatch.rs` with `fn run_papyrus_call(world, owner: QuestFormId, vmad: Option<&ScriptInstanceData>, script, function, args) -> Vec<QuestStageAdvanced>` (budget check + `defer` + `apply_fragment_guard_free`) and `fn flush_advances(world, advances)` (player lookup + `push_quest_stage_advances`); the ten sites become one call each and the budget policy has one home.

#### TD2-2026-10-15-07: `crates/nif/src/anim/types.rs` is a field-for-field twin of `core::animation::types`, bridged by a 200-line copy that has dropped fields before
- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/anim/types.rs:47-224` (`TransformChannel`, `TranslationKey`, `RotationKey`, `ScaleKey`, `AnimFloatKey`, `AnimColorKey`, `AnimBoolKey`, `FloatChannel`, `ColorChannel`, `BoolChannel`, `TextureFlipChannel`, `AnimationClip`) vs `crates/core/src/animation/types.rs:50-235`; bridge `byroredux/src/anim_convert.rs:556-760` (`convert_nif_clip`)
- **Status**: NEW (#3862 unified only `FloatTarget`/`ColorTarget`; its own comment says "the canonical vocabulary for both sides of the NIF boundary")
- **Age**: `8b72dbdcb` (#3097, 2026-08-18) added `phase` NIF-side; `d1bcf6e2b` (#3345, 2026-08-28) added it core-side; `f680df2e0` (#4167, 2026-09-12) added completeness guards for the twin
- **Effort**: medium
- **Description**: `byroredux-nif` already depends on `byroredux-core`, yet it declares its own `[f32; 3]`/`[f32; 4]` keyed structs and `convert_nif_clip` maps each key vector element-by-element into the glam ones. The eight key/channel structs carry no information the core ones lack (the two real translations - `TexType` slot -> `FlipTextureRole`, `String` text keys -> `FixedString` - are the only reason the clip struct cannot be shared as-is).
- **Evidence**: the bridge's own comment records the failure mode: "#3345 - `NiControllerSequence.phase` reached the NIF-side clip under #3097 but stopped here: this literal copied every sibling field and not this one, and `byroredux_core`'s `AnimationClip` had nowhere to put it"; #3258 and #3432 then added `sanitized_clip_frequency/duration/weight` at the same seam because the raw fields flowed through unvalidated.
- **Impact**: any clip field added NIF-side (the Starfield/FO4 animation work in flight) is dropped until someone notices, and #4167's completeness guards pin the symptom rather than remove the second declaration.
- **Related**: #3862, #3345, #3258, #3432, #4167, NIFAL (this is a legitimate Imported -> Canonical boundary for the *clip*, not for the key structs).
- **Suggested Fix**: have `nif::anim` emit `byroredux_core::animation::{TranslationKey, RotationKey, ScaleKey, AnimFloatKey, AnimColorKey, AnimBoolKey, FloatChannel, ColorChannel, BoolChannel, TransformChannel}` directly (delete the nif twins, the `convert_nif_clip` key loops shrink to the interning and texture-flip arms), and keep only `AnimationClip`/`TextureFlipChannel` as the Imported tier.

#### TD9-2026-10-15-01: About 218 of the 322 ignored tests run in no automated lane, and at least 45 of them pin fixes for closed HIGH/CRITICAL issues
- **Severity**: MEDIUM (severity table: `#[ignore]`d test that guards a fix from a closed CRITICAL/HIGH issue)
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:139-146,177-231,244-246` (what the nightly covers) vs the ignored tests in `crates/plugin` (92), `byroredux` bin (78, excluding `tests/{golden_frames,cornell_rt_oracle,upscaler_quality}.rs`), `crates/scripting` (16), `crates/nif` (13 outside the three corpus binaries), `crates/audio` (9), `crates/spt` (4), `crates/ui` (3), `crates/pex`, `crates/renderer` (1 each)
- **Status**: NEW (related open: #5784; #4492 / #4619 closed the same shape for the exterior harnesses and the NIF corpus)
- **Age**: structural; the +23 of this window are all of this class
- **Effort**: medium
- **Description**: the nightly lane runs `byroredux-nif` `parse_real_nifs` / `per_block_baselines` / `block_coverage_baselines`, the `bsa`, `bgsm`, `sfmaterial`, `hkx`, `facegen`, `menuxml` suites, and one named `byroredux-scripting` test; `rt-correctness.yml` (manual dispatch) runs cornell, golden and the FSR preset gate. Everything else ignored is reachable only by a hand-run `--ignored` or by an audit skill's instructions. The workflow comment at `:232-233` says the `plugin` suites stay out deliberately (memory: one `parse_real_esm.rs` run spikes past 20 GB), and no scoped alternative was added. Tests in the uncovered set that cite a closed HIGH/CRITICAL issue in their doc comment or body (token scan of the 60 lines after each `#[ignore]` against `gh issue list --label critical|high --state all`): 45, among them `fnv_corpses_key_on_base_health_not_xrgd`, `fo4_starts_unconscious_and_xrgd_poses_match_the_census`, `skyrim_xrgd_poses_match_the_census_and_nothing_is_unconscious`, `fo3_fnv_script_killed_corpses_match_the_measured_census` (#5005), `race_skeletons_match_authored_paths_on_real_skyrim_data` (#5487), `starfield_cydonia_pack_ins_instance_their_template_cells` (#5489), `exterior_precombine_instances_are_world_absolute` (#5228), `distant_water_mesh_is_per_cell_on_real_fnv` (#5243), `fnv_actor_value_roster_and_health_resolve_on_shipped_master` and `fo3_fnv_named_npc_health_matches_the_authored_curve` (#5238), the three `*_ruleset_resolves_against_the_real_master` (#2986), `parse_rate_oblivion_esm` (#533, CRITICAL) and `cross_game_translation_completeness` (#1277/#1292, named in `audit-nifal` and `audit-renderer`).
- **Evidence**: `/tmp/audit/tech-debt/d9_ignguard.py` (lane map: `covered` prefixes from the workflow files; 103 covered, 219 not, one of the 219 being the scripting test the nightly does run). Caveat: an issue number in a test's doc is a pointer to the fix, not proof the test is its only guard; the 45 is an upper bound on "pins a closed HIGH fix".
- **Impact**: the last two weeks fixed #5005, #5228, #5238, #5243, #5487, #5489 and added their real-data pins; each of those regressions would be found only when an audit happens to run the right `--ignored` filter by hand on a machine with the game installed.
- **Related**: #5784, #4492, #4619, #3850 (`BYROREDUX_REQUIRE_GAME_DATA`), `plugin_ignored_tests_oom` (memory).
- **Suggested Fix**: add a scoped step to `real-data-gates.yml` that runs the plugin/bin/scripting/audio real-data tests per game in separate processes (`--test-threads=1`, `-- --ignored <filter>` per title, `BYROREDUX_REQUIRE_GAME_DATA=1`, with the existing "refuse a green no-op" count check); that removes the OOM objection because each process loads one master. Start with the 45 closed-HIGH pins.


## LOW Findings

#### TD1-2026-10-15-01: `crates/scripting/src/fragment/vm.rs` is 6213 production LOC in one file, created three days ago, with a 1259-line `object_read` and a 773-line `check_call`
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/vm.rs` (6215 lines, tests already external in `fragment/vm/tests.rs`, 5631 lines)
- **Status**: NEW
- **Age**: file added e766d3147 (2026-10-13, "the Papyrus VM runs quest fragments the lowering declines"); 77 commits touch it in three days (`852c61078`, `20acec55d`, `141dbbb90`, `a0045b0cf`, `8b3b75d4d`, `5320d4ac9`, …)
- **Effort**: large (decompose first)
- **Description**: three responsibilities sit in contiguous runs, none of them cut by a module boundary:
  1. values + marshalling (L93–350, L694–760, L4320–4403): `Value`, `Stop`, `Frame`, `store`/`restore`, `cast`, `arithmetic`, `binary`;
  2. script-model lookup (L497–950): `Callable`, `callable_in`, `script_chain`, `resolve_in`, `auto_state`, member typing;
  3. the interpreter `impl Vm<'_>` (L1028–4168, **3140 lines, ~95 methods**): registrations (`quest_update`, `event_registration`, `distance_registration`, `hit_registration`, L1038–1275), frame/exec/eval/assign (L1323–2260), collection/array/layer natives (L1717–2350), **`object_read` (L2483–3741, 1259 lines, 190 match arms)** and `call_statement` (L3938, 227 lines);
  4. the static pre-pass `Checker` (L4404–6095, ~1700 lines) whose **`check_call` is 773 lines / 109 arms** and `check_native` another 81.
  `object_read` is two blocks: a `static_class` match for `Game.*` / `Utility.*` / `Weather.*` (L2513–2660) and an instance-method `match (&owner, name)` (L2761–3740, ~100 arms) that interleaves unrelated Papyrus API families (animation events, crime gold, bribe/favor, destruction, linked-ref chains, map markers, owner changes, form lists). Each arm is 5–40 lines and calls into a subsystem module that already exists (`crime_gold.rs`, `bribe.rs`, `destruction.rs`, `linked_refs.rs`, `animation_graph.rs`, `map_markers.rs`, `ownership.rs`).
- **Evidence**: `prod_loc` 6213; `fnscan.py` rows `object_read 1259 / depth 6 / 190 arms`, `check_call 773 / 109 arms`, `call_statement 227`. `awk` over L2761–3750 lists the arm heads (`(Value::Form(actor), "getrace")`, `"setbribed"`, `"damageobject"`, `"getlinkedrefchain"`, …).
- **Impact**: the highest-churn file in the repo and every new native edits it in two places that must agree — the runtime read arm in `object_read` and the "can the VM run this" arm in `check_call`/`check_native`. A native with a read arm but no check arm leaves a fragment installed as `Effect::Interpret` that stops mid-run; the reverse leaves a fragment declined that could run. Nothing ties the two halves together today.
- **Related**: #5468 / #5467 (the split-by-family precedent landed 10-14: `fragment/effects/{inventory,lock,…}.rs`); finding 02 and 03 (same wave).
- **Suggested Fix**: split along the one axis both halves share — the Papyrus API family — not along runtime-vs-checker. `fragment/vm/{value.rs, script_model.rs, interp.rs (run_fragment/exec/eval/assign/frames), registrations.rs, checker.rs (Checker core: kind/static_type/check_block/check_stmt)}` plus `fragment/vm/natives/{game_utility.rs, alias.rs, form_list.rs, actor.rs, reference.rs, animation.rs, ownership_crime.rs, destruction.rs, linked_refs.rs, map_stats.rs}` where each family file holds both `read_<family>(&mut Vm, ..)` and `check_<family>(&mut Checker, ..)`, chained from `object_read` / `check_call` as `or_else`-style dispatch (order matters only for the first-match arms; pin it with the existing `vm/tests.rs`). Hazard: `fragment.rs` `SOURCES` (`concat!(include_str!(…))`, L109–137) and its `sources_completeness` test must list every new file (#3854 pattern), and `byroredux/src/boot/schedule/mod.rs:626` reads `fragment/effects/*` the same way.

#### TD1-2026-10-15-02: `crates/scripting/src/translate/effects.rs` tripled to 5295 production LOC (1933 → 5295 in three days)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/translate/effects.rs` (10117 lines total, tests start at L5296)
- **Status**: NEW (the file was a watch-list row at 1933 on 10-12; no issue)
- **Age**: +3362 prod lines across ~82 commits since 5d31fd637, e.g. `f597efa83`, `f2d559a79`, `5dc24a272`, `e2a234c6e`, `a0045b0cf`
- **Effort**: large
- **Description**: the file is five layers in sequence:
  1. the effect IR: `Effect` enum (L1032–1577, 545 lines, ~100 variants), `FragmentGuard` (L118–267), `ValueExpr`, `ActorRef`/`ObjectRef`/`QuestRef`;
  2. guard + value classification (L331–996): `classify_value`, `classify_condition`, `guard_reading` (**234 lines**), `guard_from`;
  3. statement lowering (L1772–2800): `lower_fragment*`, `lower_statements`, `lower_if`, quest-script inlining (`inline_quest_script_call`, 151 lines), variable writes, `prune_unread_locals`;
  4. the primitive table and its **~95 `prim_*` classifiers (L2916–4980, ~2060 lines)** keyed off `EFFECT_PRIMITIVES` (L2963); and the VM-facing bridge (`VmResume`/`VmFrame`/`VmStep`/`VmStored`/`NativeBindings`, `classify_native_*`, L2812–2915);
  5. receiver / argument resolvers (L4980–5295).
  Layer 4 is a flat list of independent `fn prim_x(e: &Expr, scope: &Scope) -> Option<Effect>` grouped only by comment: quest/stage/objective, inventory/caps, faction/perk/AV, map/message, scene/activation/lock, player controls, combat/AI, animation/vehicle.
- **Evidence**: `prod_loc` 5295; `grep -n '^fn prim_' | wc -l` ≈ 95; line spans above from `grep -nE '^(pub )?(fn|enum|struct|impl)'`.
- **Impact**: a new native means one `prim_*` + one `Effect` variant + one `EFFECT_PRIMITIVES` row in a 5000-line file, then a matching applier in `fragment/effects/<family>.rs` and a VM arm (finding 01). The applier side was already split by family (#5468); the lowering side was not, so the same native lives in two differently-organised trees.
- **Related**: finding 01, 03; #5468 (appliers by family); the `every_effect_primitive_bounds_its_argument_count` test (L9679) scans this file's own text.
- **Suggested Fix**: `translate/effects/{model.rs (Effect, FragmentGuard, ValueExpr, refs), guards.rs, lower.rs, inline.rs, vm_bridge.rs, receivers.rs}` and `translate/effects/prims/{quest,inventory,actor_state,object,scene_lock,player_control,combat_ai,animation_vehicle}.rs`, one file per family named exactly like the matching `fragment/effects/` applier so a native's two halves are easy to pair. `EFFECT_PRIMITIVES` stays one table in `prims/mod.rs` (it is the ordering contract). Hazard: the `include_str!("effects.rs")` scan at L9679 splits on `"\nfn prim_"` and must read `concat!` of all `prims/*.rs` instead; `translate/mod.rs` and the `scripting/examples/` harnesses import from `translate::effects::*`, so keep a `pub use` re-export hub there.

#### TD1-2026-10-15-03: Each M47.2 native touches six hub files, and all six grew in three days — `register_late_systems` 615 → 808, `build_save_registry` 259 → 368, `apply_at_depth` 188 → 621
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `byroredux/src/boot/schedule/late.rs:15` (`register_late_systems`, 808 lines) and `update.rs:17` (`register_update_systems`, 763, was 661); `byroredux/src/save_io.rs:394` (`build_save_registry`, 368, was 259); `crates/scripting/src/lib.rs:310` (`register`, 241, was 72); `crates/scripting/src/fragment/effects/deferred.rs:380` (`apply_at_depth`, 621, was 188); `crates/scripting/src/scene/quest_alias.rs:793` (`refresh_scene_actor_bindings`, 484, was 332); `byroredux/src/asset_provider/script.rs:915` (`populate_scene_runtime`, 420, was 278)
- **Status**: NEW (`register_*_systems` is the regrowth of closed #3739/#3855; `build_save_registry` rides on #5585)
- **Age**: all since 2026-10-12; stat of four sampled commits (`a0045b0cf` DamageObject, `8b3b75d4d` SetActorOwner, `5320d4ac9` SetBribed, `f597efa83` SetLocRefType): every one touches `late.rs`, `save_io.rs`, `save/src/snapshot.rs`, `save_io/serde_default_guard_tests.rs`, `scripting/src/lib.rs`, `fragment/effects.rs` (+`vm.rs` or `translate/effects.rs`) and `docs/engine/scripting.md`
- **Effort**: large (a seam, not a move)
- **Description**: the M47.2 subsystems are good modules (`crime_gold.rs`, `ownership.rs`, `loc_ref_types.rs`, `destruction.rs`, …), but each one is wired in by hand-editing six central lists: a `pub use` + a `try_resource().is_none() → insert_resource(T::default())` block in `scripting/lib.rs::register` (37 such blocks), a system row with an `Access` chain in `late.rs`, a registry row in `build_save_registry` + a snapshot-format touch + a serde-default guard row, a queue field in `DeferredFragmentEffects` with its flush block in `apply_at_depth` (45 `if !self.x.is_empty()` blocks, L396–935), and a `fragment/effects.rs` arm. The wiring grows linearly with natives and none of it is owned by the subsystem.
- **Evidence**: `fnscan.py` base-vs-HEAD table (values in Location); `grep -c 'is_none()' crates/scripting/src/lib.rs` region = 37; `git show --stat` of the four commits above.
- **Impact**: the same hot files collide on every native; `apply_at_depth` and the two `register_*` functions are already 600–800 lines each; `save_io.rs` crossed the line because of the registry table (#5585).
- **Related**: #5585 (save_io split), #3739 / #3855 (closed, the five-file `register_*` split), finding 01/02, `TD1-2026-10-15-04`.
- **Suggested Fix**: give each subsystem an `install` entry point (`pub fn install(w: &mut World)` for resources, `pub fn register_systems(s: &mut Scheduler)` for its rows, `pub fn register_saved(r: &mut SaveRegistry)`), and have the hubs call a flat list of `<subsystem>::install(..)` lines. For `apply_at_depth`, the smaller first step is below. Do this incrementally, one subsystem per commit, with the existing `registry_completeness_tests` as the safety net. Dim 2 pointer: the 37-block `if try_resource::<T>().is_none() { insert_resource(T::default()) }` idiom occurs **160 times** workspace-wide (`scripting/lib.rs` 41, `scene/world_setup.rs` 12, `story_manager.rs` 8, `dialogue.rs` 7) and `World` has no `init_resource::<R: Default>()`; adding it removes ~110 lines from `register` alone.

#### TD1-2026-10-15-04: `DeferredFragmentEffects::apply_at_depth` is 621 lines of 45 inline queue flushes; `apply_effect` (562, 67 arms) and `evaluate_guard` (305, 40 arms) are the same shape
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects/deferred.rs:380-1000` (`apply_at_depth`); `crates/scripting/src/fragment/effects.rs:138` (`apply_effect`); `crates/scripting/src/fragment/effects/evaluate.rs` (`evaluate_guard`)
- **Status**: NEW (the pre-split `apply_at_depth` was 188 lines, tracked under #5468 which closed on the file split, not the function)
- **Age**: 188 → 621 since 10-12 (M47.2 natives: `a0045b0cf`, `5320d4ac9`, `8b3b75d4d`, `e30e8bc2e`, …)
- **Effort**: small (apply_at_depth), medium (the other two)
- **Description**: `apply_at_depth` is a straight list of drains, `for x in take(&mut self.messages) {…}`, `if !self.keyword_changes.is_empty() {…}`, … ×45, each 5–40 lines that call a subsystem function. The only non-flat part is the provider-step recursion at L940–1000 that must stay last. Because the order is semantic (the #3946 comment above it), extracting each block into `fn flush_<queue>(&mut self, world)` and calling them in the same order is a mechanical, behaviour-preserving cut. `apply_effect` is a 67-arm `match` that already delegates its newest families to `effects/{ai_combat,inventory,lock,…}.rs`; the remaining arms should follow so the function is a dispatcher.
- **Evidence**: `awk` over `deferred.rs` L380–1010 for `^        (for|if|let|…)` yields the 45 flush heads listed in order; `fnscan.py`: `apply_at_depth 621 depth 8`, `apply_effect 562`, `evaluate_guard 305`.
- **Impact**: depth-8 nesting in the function every fragment goes through; a reorder bug here moves a gameplay effect across a lock boundary.
- **Related**: #3946 (the barrier-cap comment), #5468, finding 03.
- **Suggested Fix**: `impl DeferredFragmentEffects { fn flush_messages, flush_deletions, … }` in `deferred.rs` (or one per family file next to its applier), `apply_at_depth` becomes a 60-line ordered call list; keep the `fragment.rs::SOURCES` concat valid by not changing file names.

#### TD1-2026-10-15-05: `crates/scripting/src/condition.rs` crossed 2000 production LOC (1235 → 2005 in three days); a 49-variant catalog is spelled in six parallel exhaustive matches
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/condition.rs` (3691 lines total, tests from L1995)
- **Status**: NEW (the 10-12 report did not list it; only `PerkList` #5610 and #5616 name the file)
- **Age**: +770 lines since 5d31fd637 — `6f6a9eb3d` (location runtime), `5bf9ac87e` (HasKeyword), `87e43246c` (GetRelationshipRank), `a6d1f2e44` (cleared locations), `e490f6fd9`, `5ea89f3be`, `44e4d948d`, `f67d82b3c`, …
- **Effort**: medium
- **Description**: four layers: (a) the catalog `ConditionFunction` (L105–788, ~685 lines): the enum, `from_index` (L336, 49 arms), `first_param_is_form_id`, `name`, `reads_run_on_entity`, `is_deterministic`, `reads_context`, `is_identity_test`, `run_on_identity`; (b) `ConditionContext` + `EventDataSlots` + `resolve` (L790–1160, ~370); (c) evaluators (L1166–1880, ~715): `evaluate_contextual` (174 lines, 42 arms), `actor_value_view`, **`evaluate_function` (405 lines, 37 arms)**; (d) `subject_requirement` (L1887–1993). Layer (a) states every function six times — `from_index`, `name`, and four boolean-classification matches (the doc on `reads_run_on_entity` says they are deliberately exhaustive so a new variant must be classified) — so adding one function edits six `match` blocks plus `CATALOG`.
- **Evidence**: `grep -c` of variants in the enum = 49; `fnscan.py`: `evaluate_function 405`, `evaluate_contextual 174`.
- **Impact**: the catalog grows with every Condition function a quest or INFO uses; six matches that must stay in step are a cheap place to drift, and the file is the evaluator every dialogue / quest / package path calls.
- **Related**: #5616 (closed, CTDA length constants), #5610 (`PerkList`), finding 03.
- **Suggested Fix**: `condition/{catalog.rs, context.rs, eval.rs, requirement.rs}` plus, in `catalog.rs`, one `const ROWS: &[Row { func, index, name, first_param_form_id, reads_run_on_entity, deterministic, reads_context, identity_test }]` with the existing exhaustive-match guarantees recovered by a `const` assertion or a test that walks `CATALOG` against `ROWS`; `evaluate_function` is then split by evaluator family (quest/global, actor, location, alias, event data). Hazard: three in-file `include_str!("condition.rs")` pins (L2488, L2841, L3605) and `byroredux/src/boot/schedule/mod.rs:653` (`CONDITION_SRC`) must read the new files.

#### TD1-2026-10-15-06: `crates/plugin/src/esm/cell/mod.rs` crossed 2000 production LOC (1935 → 2108); it is ~85% record data types
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/plugin/src/esm/cell/mod.rs:1-2108`
- **Status**: NEW (watch-listed since 10-05 at 1866)
- **Age**: +173 since 10-12: `3b28dca71` (#5463 XCCM), `e32304ec9` (#5489), `18b3d87a7` (XRGD pose, #5015), `745d64830` (activate parents), `a0045b0cf` (destruction stages)
- **Effort**: medium
- **Description**: after the earlier split (`helpers`, `support`, `walkers`, `wrld`) `mod.rs` is the type hub: lighting (`CellLighting`, `StarfieldLighting`, `decode_starfield_lighting_tail`, L31–197), landscape (L198–254), `CellData` (L255–451, 33 fields), the placed-reference family (`PlacedRef` 230 lines + `RagdollPoseBone`, `LockData`, `TextureSlotSwap`, `TeleportDest`, `PrimitiveBounds`, `LinkedRef`, `ActivateParent`, `PortalLink`, `EnableParent`, L452–927, ~475 lines), base-object data (`LightData`, `DecalData`, `AddonData`, `StaticObject`, `DestructionData/Stage`, `TextureSet`, L928–1323, ~400), worldspace (L1324–1440), and the index with merge logic (`EsmCellIndex` impl 290 lines, `merge_cell_override`, `merge_navmeshes`, `CellLocator`, `CellRef`, L1441–2070, ~630). Functions are all small (largest is `merge_from`, 129 lines).
- **Evidence**: `prod_loc` 2108; item list from `grep -nE '^(pub(\([a-z]+\))? )?(fn|struct|enum|impl|…)'`.
- **Impact**: low per edit but every M47.2 data addition to a placed ref or base object lands here (five commits in three days).
- **Related**: #5309 (closed, walker dedup), `crates/plugin/src/esm/cell/tests/` mirrors the topics already.
- **Suggested Fix**: split by record family, `mod.rs` keeps `pub use` re-exports so no caller changes: `lighting.rs`, `landscape.rs`, `cell_data.rs`, `placed_ref.rs` (the 475-line placed-reference family), `base_objects.rs` (statics/lights/decals/destruction/texture sets), `worldspace.rs`, `index.rs` (`EsmCellIndex` + merge + locator). Hazard: `cell/tests/txst.rs:616` (`include_str!("../mod.rs")`) and the module's own `plugin_loading_doc_pin_tests` (L2111–2216, four `include_str!` hits) read this file's text.

#### TD1-2026-10-15-07: Existing #5585 — `byroredux/src/save_io.rs` grew again (2011 → 2136, +125); `build_save_registry` 259 → 368
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `byroredux/src/save_io.rs`; `build_save_registry` L394 (368 lines), `execute_pending_save_loads` L1864 (307), `SaveCommand::execute` L1029 (201), `reload_exterior_session` L1661 (127)
- **Status**: Existing: #5585 (open). The issue body (axis `save_io/{mod, registry, player, commands, live_load}.rs`, with `execute_pending_save_loads` + `reload_*_session` + `PRE_RELOAD_RESOURCES` kept together) re-verified against HEAD: the five runs are still contiguous at the same offsets plus the growth below.
- **Age**: growth is the M47.2 registry rows: `a0045b0cf`, `8b3b75d4d`, `5320d4ac9`, `f597efa83`, … (one registry row + `ReloadOutcome`/delta touch each)
- **Effort**: medium (as filed)
- **Description / Suggested Fix**: unchanged. Two additions: (1) the issue's stated trigger "P5 hardening settles" is overtaken: the file is now the M47.2 hot spot, so do the `registry.rs` cut first (it is where the growth is, and finding 03's `register_saved` seam lands there); (2) the include_str hazard list is current — `byroredux/src/systems/character.rs:1778` also reads `save_io.rs`, not in the issue.

#### TD1-2026-10-15-08: `crates/scripting/src/lib.rs` now has two jobs — 64 `pub use` lines (27 on 10-12) and a 241-line `register` (72 on 10-12)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/lib.rs:1-550`, `register` at L310
- **Status**: NEW (the 09-05 report dismissed it as "a facade with one non-re-export item"; that premise no longer holds)
- **Age**: M47.2 commits since 10-12 (see finding 03)
- **Effort**: small
- **Description**: rule "`mod.rs` / `lib.rs` with >20 `pub use` (two jobs)". It was 72 mods + 27 re-exports + a 72-line `register`; now `register` is 241 lines, 37 of them the same three-line "insert default if absent" block.
- **Evidence**: `grep -cE '^\s*pub use' lib.rs` → 64 (27 at 5d31fd637); `fnscan.py`: `register 241 / was 72`.
- **Impact**: the crate's public surface and its world setup edit the same file; every native touches both.
- **Related**: finding 03.
- **Suggested Fix**: move `register` to `crates/scripting/src/registration.rs` (`lib.rs` re-exports it), add `World::init_resource::<R: Default>()` in `crates/core/src/ecs/world.rs` and replace the blocks. The other `pub use` hubs are unchanged and fine (`core/ecs/components/mod.rs` 44 lines of re-exports in 114 lines; `plugin/esm/records/mod.rs` 24).

#### TD1-2026-10-15-09: `effect_kind` is a 95-arm variant-name table pasted into two example binaries, edited by hand on each new `Effect` variant
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms >50; Dim 2 overlap, hand to the Dim 2 agent if it already has it)
- **Location**: `crates/scripting/examples/mq101_conformance.rs:117-215` and `crates/scripting/examples/fragment_coverage.rs:187-285`
- **Status**: NEW
- **Age**: 56 commits touch the two examples in three days (M47.2)
- **Effort**: trivial
- **Description**: the two functions are the same 99-line `match` over every `Effect` variant returning its name (only the arm order of `Disable`/`Enable` differs). `Effect` already derives `Debug` and has an `impl Effect` (L1597, `is_placeholder`).
- **Evidence**: `diff` of the two ranges differs only in the parameter name and two arms' position.
- **Impact**: a new variant that misses one copy mis-reports in that harness; the harnesses are the M47 coverage gates.
- **Related**: finding 02 (owns the enum), `mq101_conformance.rs::run` = #5586.
- **Suggested Fix**: `impl Effect { pub fn kind(&self) -> &'static str }` (or `strum::IntoStaticStr` if already a dependency) next to `is_placeholder`; delete both copies.

#### TD1-2026-10-15-10: `crates/renderer/src/shader_constants_data.rs` is 2900 LOC, 51% comments, and spells each of ~440 constants twice (data table — do not split)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/renderer/src/shader_constants_data.rs` (consts L1–2255, generator machinery `ShaderValue`/`HeaderLine`/`render_shader_constants_header` L2256–2380, `SHADER_DEFINES` table L2381–2900)
- **Status**: NEW (carried as "data table" in the baseline; first time it has a finding)
- **Age**: +131 lines since 10-12 from the grass commits (`b731b01f5`, `39c9e39f0`, `af8323881`, `2c86473ff`, `3f1c5e962`)
- **Effort**: small
- **Description**: the skill's rule for data tables is a static table or generation, not a split; the file is already that, and it is `include!`d by both `shader_constants.rs` and `build.rs`, which would make a file split fragile. What it still carries: (1) each constant is a `pub const X` (≈440) and again a `HeaderLine::Define("X", ShaderValue::…(X))` row in `SHADER_DEFINES` (~520 lines), so a new constant is two edits in two distant places and a missed row means a Rust value with no GLSL `#define` (the lockstep class `feedback_shader_struct_sync` warns about); (2) the generator types and renderer (L2256–2380) live in the data file only because `build.rs` can `include!` one file.
- **Evidence**: `grep -cE '^\s*(pub )?const ' → 440`; comment lines 1485 of 2900; `SHADER_DEFINES` rows at L2381.
- **Impact**: this is the #3815 / #3882 provenance gate's subject; a missed row is silent unless the gate's tests catch it.
- **Related**: #3815, #3882, #3880 (closed).
- **Suggested Fix**: a `shader_const!{ NAME: u32 = …, "section comment" }` macro (or a `build.rs` pass) that emits both the `const` and its `SHADER_DEFINES` row from one line; move the generator types to a second `include!`d file. Do not split by shader domain.

#### TD2-2026-10-15-08: Fifteen clamp-to-edge point/linear samplers are hand-built, each an 8-line `SamplerCreateInfo` chain plus `try_or_cleanup!` and `.context(...)`
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/renderer/src/vulkan/ssao.rs:165,179`, `presentation.rs:265`, `caustic.rs:427`, `sky_cube.rs:289`, `svgf.rs:536`, `taa.rs:269,283`, `composite.rs:481,502`, `bloom.rs:306`, `volumetrics/init.rs:281,302,322`, `exposure_meter.rs:81`, `cloud_noise.rs:124` (repeat); `context/helpers.rs:611` and `texture_registry/mod.rs:1396` are legitimately different
- **Status**: NEW
- **Age**: the sibling of #5593 (c6d36b4e7, 2026-10-14) which consolidated layout bindings, UBO creation and cleanup but left samplers
- **Effort**: small
- **Description**: clones of `vk::SamplerCreateInfo::default().mag_filter(F).min_filter(F).address_mode_u/v/w(CLAMP_TO_EDGE)` with `F` in {NEAREST, LINEAR} and an optional `.mipmap_mode(NEAREST|LINEAR)` / `.max_lod(..)`; `taa.rs:269` and `:283` are two copies in the same function differing in `F` only. There is no `descriptors::` sampler builder to mirror `layout_binding`/`write_*`.
- **Evidence**: `grep -rn 'create_sampler(' crates/renderer/src` -> 20 sites in 16 files; the 15 listed differ only in filter and mip mode.
- **Impact**: a sampler policy change (anisotropy off for point samplers, a debug-name, a new `unnormalized_coordinates` rule) is a 15-site edit; low blast radius, high boilerplate.
- **Related**: #5593 (same family and guard `passes_build_layouts_writes_and_teardown_through_the_shared_helpers`).
- **Suggested Fix**: `pub(crate) unsafe fn clamp_sampler(device, filter: vk::Filter, mip: vk::SamplerMipmapMode) -> Result<vk::Sampler>` in `descriptors.rs`; extend the #5593 shape guard to flag a new bare `create_sampler(` outside it.

#### TD2-2026-10-15-09: Eight extension dispatch systems repeat the 25-line host-acquire -> lock -> dispatch -> apply -> drain -> emit -> fault-warn tail (the ECS-adapter half of what #5592 consolidated in `dispatch.rs`)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/extensions/systems.rs:247-289` (custom events), `:546-612` (activation), `:615-663` (cell load), `:665-742` (equipment), `:744-796` (input), `:798-835` (session), `:837-935` (hit), `:937-962` (update); host acquisition alone appears 15 times (`ExtensionHostSlot>()` at `:56..:939`)
- **Status**: NEW (#5592 closed the `ExtensionHost::dispatch_*` side, `91a28a4c9`, 2026-10-14)
- **Age**: `55cbc0d6c` (#3843 split, 2026-09-11)
- **Effort**: small
- **Description**: each system ends with `let mut host = host.lock().expect("ExtensionHost mutex poisoned by a host panic"); let stats = host.dispatch_X(...); apply_pending_world_commands(world, &mut host); let diagnostics = host.take_diagnostics(); drop(host); emit_diagnostics(diagnostics); if stats.faults > 0 { log::warn!("extension X dispatch: events={} deliveries={} commands={} faults={}", ...) }`, and the same 10-line `slot.host()` preamble.
- **Evidence**: clone scan `:326-348` vs `:754-793`; eight textual copies of the `events={} deliveries={} commands={} faults={}` format string.
- **Impact**: the next dispatch family (or any change to the drop-before-emit ordering, which is lock-order-sensitive after #5535) is eight edits.
- **Related**: #5592, #5535.
- **Suggested Fix**: `fn with_extension_host(world: &World, label: &str, f: impl FnOnce(&mut ExtensionHost) -> DispatchStats)` in `systems.rs` owning the preamble and the whole tail; each system keeps only its event capture and the one `host.dispatch_*` call.

#### TD2-2026-10-15-10: `populate.rs` repeats one lower-or-interpret loop three times and four `lower_*_fragment` entry points repeat the `Scope` build and `prune_unread_locals` / `locals_never_suspend` pipeline
- **Severity**: LOW (promotes to MEDIUM the first time one copy is fixed alone; M47.2 added the interpret fallback to quest, then scene + info a day later)
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/populate.rs:378-456` (quest), `:636-684` (scene), `:740-799` (info); `OwnedFragmentProviders` -> `FragmentProviderScope` conversion at `:267-285` and `:500-523`; `crates/scripting/src/translate/effects.rs:1804-1818`, `:1849-1866`, `:1871-1887`, `:2181-2206`
- **Status**: NEW
- **Age**: `e766d3147` (2026-10-13, quest fallback) -> `2362a2a2a` (2026-10-13, scene + info copy)
- **Effort**: small
- **Description**: each `*_internal` computes `quest_property_names_with_library` + `argless_function_names`, loops bindings, calls its `lower_X_fragment`, `.or_else(interpret_fallback(...))`, `attribute_provider_calls`, `is_empty` skip - 20 identical lines, differing only in the lowering call, the VM-script sink (`frags.insert_vm_script` vs `providers.vm_scripts`) and the insert. The four `lower_*` entry points differ by one `Scope` flag (`vmad_is_quest: true` on quest, a `speaker_locals` seed on info); `lower_scene_fragment` equals `lower_quest_fragment` minus that flag.
- **Evidence**: clone scan 24 windows within `populate.rs`; `effects.rs:1811-1817` vs `:1859-1865` identical.
- **Impact**: `#5407` already had to thread `own_argless_function_names` through all four; the next scope input is four signatures plus three loops.
- **Related**: #5407, #5763.
- **Suggested Fix**: `fn lower_with_scope(body, scope: Scope, providers) -> Option<Vec<Effect>>` in `translate/effects.rs` and `fn lower_or_interpret(script, fragment_name, providers, lower: impl FnOnce(&FragmentCtx)->Option<Vec<Effect>>, sink: &mut dyn FnMut(Arc<Script>)) -> Option<Vec<Effect>>` in `populate.rs`; `impl From<OwnedFragmentProviders<'_>> for FragmentProviderScope<'_>`.

#### TD2-2026-10-15-11: `StartScene`/`StopScene` copy the `StartQuest`/`StopQuest` lifecycle body, and the #5468 split put the two copies in sibling files without sharing them
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/effects/quest_scoped.rs:40-80` vs `effects/scene.rs:55-90`
- **Status**: NEW
- **Age**: `f5127c1cc` (2026-09-10) in `effects.rs`; moved into separate files by `adbd468e2` (#5468, 2026-10-14)
- **Effort**: trivial
- **Description**: `start_quest(quest, start_up_stage)` + `scene_actor_bindings_dirty = true` on `Some`, and `is_running(quest).then(|| shut_down_stage.map(|stage| { set_stage; QuestStageAdvanced { .., stage_set: true } })).flatten()` + `stages.stop(quest)` + dirty flag, are identical.
- **Evidence**: clone scan 5 identical windows `quest_scoped.rs:56-60` / `scene.rs:67-71`.
- **Impact**: a lifecycle rule (the #5406 "refused start" precedent in `story_manager.rs:792`, the Run-Once reset) has to be edited in both and in `quest_stages.rs`, `papyrus_demo/quest_advance.rs:473`.
- **Related**: #5406, #2539.
- **Suggested Fix**: `QuestStageState::start_with(quest, start_up_stage) -> Option<QuestStageAdvanced>` and `stop_with_shutdown(quest, shut_down_stage) -> (bool, Option<QuestStageAdvanced>)` in `quest_stages.rs`; both effect arms call them.

#### TD2-2026-10-15-12: 62 production sites read the player entity as `try_resource::<PapyrusPlayerEntity>().map(|p| p.0)` with no accessor
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src` (21 files: `relationships.rs:60`, `keywords.rs:96`, `package.rs:667`, `dialogue.rs:607`, `bleedout.rs:197`, `combat.rs:58`, `trigger.rs:164`, `fragment/{hits,distance,updates,package,perk,terminal,systems}.rs`, `fragment/effects/{resolve,scene}.rs`, ...); `byroredux/src/extensions/systems.rs`, `systems/{follow,cinematic,npc_dialogue,trap_hits,scripted_sound,archetype_idle}.rs`
- **Status**: NEW
- **Age**: spread across M30..M47.2; `crates/scripting/src/papyrus_demo/mod.rs:72` defines the type
- **Effort**: small
- **Description**: the 4-line `world.try_resource::<..>().map(|player| player.0)` (with the long `crate::papyrus_demo::` path) is re-spelled everywhere; a second player resource (`byroredux::systems::character::PlayerEntity(pub Option<EntityId>)`) is kept in sync by `extension_player_entity_sync_system`, so call sites cannot tell which to read.
- **Evidence**: `grep -rn 'PapyrusPlayerEntity>()' crates byroredux --include='*.rs' | grep -v test` = 62 `try_resource` reads.
- **Impact**: moving the player resource (the planned P5 save/load work already special-cases it) is a 62-site edit.
- **Related**: #4694 (PlayerRef 0x14 resolution), TD2-06 on the 10-11 report.
- **Suggested Fix**: `impl PapyrusPlayerEntity { pub fn get(world: &World) -> Option<EntityId> }` and migrate the production sites; fold into TD2-2026-10-15-06's `flush_advances`.

#### TD2-2026-10-15-13: `CellLightingRes` has no default/builder, so five production sites and eleven test fixtures spell all 25 fields
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: production `byroredux/src/cell_loader/load.rs:146` (`engine_default_interior_lighting`), `env_translate/fallback.rs:55`, `env_translate/sky_weather.rs:84`, `cornell.rs:612`, `cornell/oracle.rs:352`; `components/environment.rs:70` (`from_cell_lighting`); struct `components/environment.rs:27`; tests `systems/weather.rs:2224,2528,2957,3167`, `render/lights.rs:371,396,866`, `systems/audio.rs:1402`, `render/sky.rs:265`, `cell_loader/load.rs:1349`
- **Status**: NEW
- **Age**: fields added over `4cd98e4e2` (#4481/#4485), `db0ec5467` (#4926) and the fog/Skyrim XCLL work; the file moved in `6ef8976f2` (#5467)
- **Effort**: small
- **Description**: each literal ends in the same ten `None`s (`fog_clip`, `fog_power`, `fog_far_color`, `fog_max`, `light_fade_begin/end`, `directional_ambient`, `specular_color/alpha`, `fresnel_power`, `inheritance_flags`) and `fog_medium: FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None)` appears verbatim in three production files.
- **Evidence**: clone scan: 9 identical windows `load.rs:155` / `fallback.rs:64`, 12 `cornell.rs:618` / `load.rs:152`.
- **Impact**: the next lighting field is 16 edits; a field forgotten in a fixture compiles but silently means "None".
- **Related**: #861 (the `from_cell_lighting` helper doc says exterior and the engine-default build the struct directly).
- **Suggested Fix**: `impl CellLightingRes { pub(crate) fn bare(ambient, directional_color, directional_dir, is_interior, fog_color, fog_near, fog_far) -> Self }` (ten `None`s and `fog_medium` computed once) and have the 5 production + 11 test sites use `..CellLightingRes::bare(..)`.

#### TD2-2026-10-15-14: The interior and exterior CELL walkers still repeat the 22-field `CellSubrecordFields` destructure and the 28-field `CellData` literal after #5309
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:430-481` vs `crates/plugin/src/esm/cell/wrld.rs:430-498`
- **Status**: NEW (residue of closed #5309 `78c2b2d61`, which removed the 20 duplicated arms but not the consumption)
- **Age**: `78c2b2d61` (2026-10-05)
- **Effort**: trivial
- **Description**: both walkers destructure the identical `CellSubrecordFields { editor_id, display_name, .. }` then rebuild a `CellData` field by field; they differ in `is_interior`, `show_sky`, `grid`, `lighting`. `precombined_mesh_hashes` / `absorbed_ref_bakes` / `regional_color_override` were each added to both by hand.
- **Evidence**: clone scan 20 windows `walkers.rs:195-478` / `wrld.rs:383-476`.
- **Impact**: a new CELL sub-record (an XCLL-like field, the next `XCRI`-style hash) is three edits (struct, absorb arm, two literals).
- **Related**: #5309, #5484.
- **Suggested Fix**: `impl CellSubrecordFields { fn into_cell(self, form_id: u32, is_interior: bool, show_sky: Option<bool>, grid: Option<(i32,i32)>, lighting: Option<CellLighting>) -> CellData }` in `cell/helpers.rs`.

#### TD2-2026-10-15-15: `ExtensionComponentStore::apply_batch` repeats the schema/field/type lookup and row-budget staging in its `Set` and `IncrementI64` arms
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/sdk/src/component.rs:490-614`
- **Status**: NEW (the sandbox quota path; crate not named by any prior report)
- **Age**: `84e899dbe` / `316fb2028` (2026-08-31); untouched since 2026-09-01
- **Effort**: trivial
- **Description**: 35 identical lines in both arms: `schemas.get(&schema_key).ok_or(UnknownSchema)`, `fields.iter().find(..).ok_or(UnknownField)`, the `TypeMismatch` build, then `row_key`, `new_rows.insert(...)` + `existing_rows + new_rows.len() > max_rows_per_principal` -> `RowBudgetExceeded`, and the `staged.entry(..).or_insert_with(..)` clone. A third command variant (the JContainers work adds collection ops) would copy it again, and the row-budget arm is a security quota.
- **Evidence**: clone scan 8 windows `:496-562` / `:542-588`.
- **Impact**: a divergence here is a quota bypass in sandboxed-extension state, so the cost of a one-sided fix is higher than the line count suggests.
- **Related**: `docs/engine/sandboxed-linked-mods.md`.
- **Suggested Fix**: `fn field_declaration(&self, principal, schema, field) -> Result<&ComponentFieldDeclaration>` and `fn stage_row<'a>(&'a self, staged, new_rows, existing_rows, principal, schema, entity) -> Result<&'a mut Row>` on the store; both arms keep only their type check and value write.

#### TD2-2026-10-15-16: Three console commands parse FormIDs with an inline hex-only chain while `parse_console_u32` is decimal-first, so the same token means different things across commands
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/commands/gameplay.rs:57-59` (`inv.add`), `:97` (`inv.equip`), `:434-436` (`levelup perk`) vs `byroredux/src/commands/shared.rs:81-87` (`parse_console_u32`, used by 12 `quest.rs` sites, `cndf.rs`, `condition.rs`, `anim_archetype.rs`, `actor_value.rs`, `script_var.rs`)
- **Status**: NEW
- **Age**: `parse_console_u32` is the M45/M47 helper; the gameplay commands predate it
- **Effort**: trivial
- **Description**: `u32::from_str_radix(raw.trim_start_matches("0x"), 16)` (a bare `00014` is hex; `0X` is not stripped) vs `parse_console_u32` (a bare `14` is decimal; `0x`/`0X` is hex). `inv.add 10` adds form 0x10, `quest.force 10` addresses form 10 decimal.
- **Evidence**: `grep -n 'from_str_radix' byroredux/src/commands/*.rs`.
- **Impact**: console FormID entry is inconsistent; a debugging session mixing `quest.force` and `inv.add` mis-addresses forms. Not a correctness bug in shipped gameplay.
- **Related**: `crates/scripting/src/papyrus` token hex (`token.rs:88`) is a third spelling.
- **Suggested Fix**: add `parse_console_form_id(tok) -> Option<u32>` (hex with optional 0x/0X) in `shared.rs`, document that FormID arguments are hex, and route the three sites and `script_var.rs:167`'s `strip_prefix("0x").and(...)` through it.

#### TD2-2026-10-15-17: The new tree-LOD parser re-grew the unchecked LE reader that #5596 centralised three days earlier
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:111-123` (`u32_at`, `f32_at`); the sibling `placement_lod.rs:109-116` already wraps `byroredux_core::bytes::le_u32_at`
- **Status**: NEW (regression of the #5596 class; #5596 CLOSED `cce3d59b5` 2026-10-11)
- **Age**: `a9384f9c0` (#4913, 2026-10-14)
- **Effort**: trivial
- **Description**: `bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0],b[1],b[2],b[3]]))` - the exact shape `core::bytes` replaced, including the unchecked `at + 4` its module doc cites ("offset near `usize::MAX` is a miss rather than an overflow"). `from_le_bytes` appears nowhere else in `byroredux/src` after #5596.
- **Evidence**: `grep -rln from_le_bytes byroredux/src` = `tree_lod.rs` only.
- **Impact**: `at` comes from file-declared counts, so a crafted `.btt` can panic in debug; same class #5596 closed.
- **Related**: #5596.
- **Suggested Fix**: replace both bodies with `byroredux_core::bytes::le_u32_at(bytes, at).ok_or(TreeLodError::Truncated { needed: at.saturating_add(4), have: bytes.len() })`.

#### TD2-2026-10-15-18: 22 `crates/plugin/examples` each hand-roll an unbounded recursive GRUP walk over `EsmReader`, four with the same `OwnedSub` callback shape
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/examples/{perk_fragment_census,dump_qust_vmad_fragments,probe_objective_fragments,qust_alias_script_census}.rs` (identical `type OwnedSub` + `walk(reader, end, want, f)`), and `autocalc_census`, `ctda_function_census`, `ctda_variable_census`, `corpse_marker_census`, `dump_npc_subs`, `dump_pack_subs`, `dump_record_subs`, `dump_scen_subs`, `dump_voli_subs`, `dump_wthr_subs`, `dump_wths_refl`, `esm_dim8_coverage`, `health_gmst_dump`, `probe_npc_perks`, `probe_weap_vats`, `qust_alias_rawdump`, `water_current_census`, `watr_wind_census`; production home `crates/plugin/src/esm/records/grup_walker.rs:96` (`extract_records`, `pub(super)`)
- **Status**: NEW (67 examples; 15 added 2026-10-12..14)
- **Age**: copies since 2026-08; 10 new on 10-12..14
- **Effort**: medium
- **Description**: `while reader.position() < end && reader.remaining() > 0 { if reader.is_group() { read_group_header; group_content_end; recurse } else { read_record_header; match type { want => read_sub_records, _ => skip_record } } }`, 22 files with `group_content_end`, 23 with `read_group_header`. The production walker is the bounded, depth-checked version (`bounded_group_content_end`) but is not reachable from examples; the example copies are unbounded recursion, so a corrupt real ESM can stack-overflow a census run.
- **Evidence**: `grep -l group_content_end crates/plugin/examples/*.rs | wc -l` = 22.
- **Impact**: each new ESM probe (the M47.2 census cadence is 3-5 a day) clones the walker again; none gets the production depth bound.
- **Related**: `/audit-esm`, #5485 (the NIF-side revisit guard).
- **Suggested Fix**: expose `EsmReader::for_each_record(&mut self, want: Option<&[u8;4]>, f: impl FnMut(&RecordHeader, Vec<SubRecord>))` (depth-bounded, built on `bounded_group_content_end`) in `reader.rs`, and migrate the examples; `dump_*_subs.rs` (6 files, 745 lines) collapse into `dump_record_subs`.

#### TD2-2026-10-15-19: Four streaming-ring reconcilers each re-spell `desired_set` -> `retain` + unload-non-empty -> budgeted spawn; the Skyrim tree-LOD ring (2026-10-14) is the newest copy
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/object_lod.rs:261-290`, `placement_lod.rs:354-375`, `terrain_lod.rs:543-560`, `tree_lod.rs:405-420`; shared module `lod_support.rs:47` (`LodReconcileInput`), `:227` (`LodWorkBudget`), `:308`
- **Status**: NEW
- **Age**: `a9384f9c0` (2026-10-14) added the fourth
- **Effort**: small
- **Description**: `lod_support` already owns the input, budget and ordering; the "drop what left the ring, count unloads, skip empty sentinels" step is still inline in each (`blocks.retain(|coord, blk| if desired_set.contains(coord) { true } else { if !blk.entities.is_empty() { unload_*_block(world, ctx, blk); unloaded += 1; } false })`).
- **Evidence**: clone scan 7 identical windows `object_lod.rs:261` / `placement_lod.rs:354`.
- **Impact**: band-switch hysteresis and the empty-sentinel rule are per-ring policy today; a leak fix (the #863/#4986 class for LOD block GPU release) must be applied four times.
- **Related**: `docs/engine/exal.md`.
- **Suggested Fix**: `lod_support::retain_resident<K, B>(blocks: &mut HashMap<K,B>, desired: &HashSet<K>, mut unload: impl FnMut(&mut B)) -> usize`, one body returning the unload count.

#### TD2-2026-10-15-20: `MaterialProvider` records a failed path four times with the same half-evict + insert + push + warn block
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/asset_provider/material/provider.rs:451-470`, `:486-499`, `:632-645`, `:672-686`
- **Status**: NEW
- **Age**: #951 / #1430 (half-eviction), #4672/#4664 (diagnostics)
- **Effort**: trivial
- **Description**: `half_evict(&mut self.failed_paths_order, self.failed_paths.len(), MAX_FAILED_PATHS, |old| { self.failed_paths.remove(old); }); if self.failed_paths.insert(key.clone()) { self.failed_paths_order.push_back(key); log::warn!(..) }`.
- **Evidence**: clone scan 13 windows `:451-486`; `grep -n 'failed_paths.insert' provider.rs` = 4.
- **Impact**: the eviction cap policy is edited four times; each previous half-eviction fix (#951, #1430) touched all of them.
- **Related**: #951, #1430.
- **Suggested Fix**: `fn record_failed(&mut self, key: String, what: impl FnOnce() -> String)` in `provider.rs`.

#### TD2-2026-10-15-21: The metrics snapshot exists in four hand-synced copies and three hand conversions
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/core/src/ecs/metrics.rs:28` (`MetricsSnapshot`), `crates/debug-protocol/src/lib.rs:244` (`DebugResponse::Metrics`), `tools/byro-dbg/src/tui.rs:180` (`MetricsView`, built at `:232`), `crates/debug-ui/src/panels.rs:385` (`MetricsSnapshotView`, built at `byroredux/src/main.rs:1463`); conversion at `crates/debug-server/src/evaluator.rs:174-197`
- **Status**: NEW
- **Age**: `0925f7926` (2026-09-26) added fields across 9 files
- **Effort**: small
- **Description**: ~20 fields each. The debug-ui copy's doc claims "the debug-ui crate doesn't depend on core's resource types directly", but `crates/debug-ui/Cargo.toml:7` depends on `byroredux-core`, so `MetricsSnapshotView` and the 20-line `main.rs` conversion can simply go. The protocol crate genuinely cannot depend on core, so the wire variant stays, but `MetricsView` in byro-dbg is a third flatten of it; one pinned test covers only core <-> wire.
- **Evidence**: `git log -S driver_vram_total_mb --name-only` -> 9 files in one commit.
- **Impact**: each new telemetry field is 9 files and the unpinned copies can lag silently.
- **Related**: #2513.
- **Suggested Fix**: debug-ui reads `byroredux_core::ecs::MetricsSnapshot` directly (delete the view and the `main.rs` mapping); in `debug-protocol` make the variant `Metrics(MetricsPayload)` with a `#[derive(Serialize, Deserialize)]` struct that byro-dbg stores as-is (delete `MetricsView`).

#### TD2-2026-10-15-22: `GpuUploadCtx { device, allocator, queue, command_pool }` is spelled out 22 times with no constructor
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/{scene.rs (3), cornell/builders.rs (3), cell_loader/water.rs (3), hud.rs (2), cell_loader/spawn/mesh_instance.rs (2), app_frame.rs (2), scaleform_hud.rs, loading_screen.rs, scene/nif_loader.rs, cornell/oracle.rs, cell_loader/terrain.rs}`; `crates/renderer/src/vulkan/{buffer.rs (5), context/resources.rs (3), context/init.rs (2), context/resize.rs (2), taa.rs, svgf.rs, texture.rs}`; `crates/renderer/src/mesh/geometry_ssbo.rs (3)`; struct `crates/renderer/src/vulkan/mod.rs:83`
- **Status**: NEW (borrow-checker caveat below; one of the weaker findings)
- **Age**: the struct was introduced to stay under `too_many_arguments`
- **Effort**: small
- **Description**: the 7-line literal with `ctx.allocator.as_ref().unwrap()` recurs, and `hud.rs:578` / `scaleform_hud.rs:286` additionally repeat `register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])`.
- **Evidence**: `grep -rn 'command_pool: ctx.transfer_pool\|command_pool: self.transfer_pool'` = 22 outside tests.
- **Impact**: a fifth handle (a timeline semaphore for the upload queue) is a 22-site edit. A `&self` constructor would conflict with the `&mut ctx.texture_registry` borrow at most call sites, which is likely why none exists.
- **Related**: #5593.
- **Suggested Fix**: `impl VulkanContext { fn upload_parts(&mut self) -> (GpuUploadCtx<'_>, &mut TextureRegistry, &mut MeshRegistry) }` (split borrow in one place), or a `macro_rules! upload_ctx!(ctx)` that expands to the literal.

#### TD2-2026-10-15-23: The weak `#[cfg(test)]` source cut survives at 18 sites in 12 files; guard unchanged; second report without an issue (TD2-2026-10-12-06, carried)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/debug-server/src/registration.rs:470,518,563,648`, `crates/physics/src/water.rs:1404`, `crates/renderer/src/vulkan/presentation.rs:917`, `svgf.rs:1716,2054`, `crates/nif/src/import/collision/mod.rs:789,833`, `crates/nif/src/import/walk/lights.rs:265,318`, `byroredux/src/material_translate.rs:1522`, `boot/world.rs:553`, `systems/cinematic.rs:2091`, `systems/combat_ai.rs:689`, `cell_loader/terrain_lod_btr.rs:616`, `cell_loader/load.rs:1065`; guard `no_source_scan_cuts_with_the_weak_cfg_test_needle` at `byroredux/src/workspace_hygiene_tests.rs:709`
- **Status**: Regression of #5469 (CLOSED); `git grep` unchanged since 10-12 (line numbers moved; four of the files were edited since)
- **Age**: `334458b3f` (#5469) landed the helper and guard; survivors predate it
- **Effort**: small
- **Description**: the guard's needles are `find(`, `rfind(`, `split(` and `split_once("\n<attr>")`; the survivors' spelling `.split_once("#[cfg(test)]")` (no leading newline) matches none, so the guard is green. The four `nif/import/{collision/mod,walk/lights}.rs` sites also fall back to the whole file via `.unwrap_or(src)`, the vacuity #4842 removed elsewhere. `crate::source_scan` is reachable from all of these (only `pex`/`facegen` lack the dependency and are allowlisted).
- **Evidence**: `grep -rnF '"#[cfg(test)]"' crates byroredux tools --include='*.rs'` minus the allowlist = 18 `.split_once(` lines.
- **Impact**: each is a guard that a test module in its own file can satisfy.
- **Related**: #5469, #5100, #4842, #4604.
- **Suggested Fix**: migrate the 18 sites to `production_text` / `production_text_or_whole` / `strip_test_modules` and add `split_once("{attr}")` (bare) to the guard needle set. If this was declined on purpose, record that in the issue so the 10-11 and 10-12 carry stops.

#### TD3-2026-10-15-01: The grass LOD rewrite (af8323881) left the stochastic / blue-noise / clump-card / FSR-reactive story live in 14 doc and comment sites
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/renderer/src/vulkan/groundcover/construct.rs:775-779`; `crates/renderer/shaders/groundcover_blade.vert:11, 84-93, 106-109`; `crates/renderer/src/vulkan/groundcover/frame.rs:122-127`; `crates/renderer/src/vulkan/groundcover.rs:159-162, 754-761`; `crates/renderer/shaders/include/groundcover_scene.glsl:137-140`; `byroredux/src/render/groundcover.rs:824`; `docs/engine/exal-groundcover.md:16, 378-381, 414-440, 638, 1795-1799`; `docs/engine/fsr3-upscaler-integration-plan.md:520-526`
- **Status**: NEW
- **Age**: `af8323881` (2026-10-14) replaced "every blade through every stream, blue-noise loser discarded per fragment" with one geomorphing 3/2/1-segment tier per 512-unit chunk (`include/groundcover_lod.glsl`). `17619c0bf` (same day) wrote the correct paragraph at `exal-groundcover.md:1854-1869` but did not retire the old ones.
- **Effort**: small
- **Description**: the code now writes `outFsrReactive = 0.0` / `outFsrTransparency = 0.0` for blades and has no mid/card tier; the test string at `construct.rs:797` says "there is no stochastic handoff left to flag", but the doc comment two lines above it (`:777-778`) still says only "either stochastic projected-size handoff raises a bounded reactive contribution". `exal-groundcover.md` contradicts itself: §6's table (3 tiers incl. "clump cards"), the "Current geometric LOD implementation" subsection (blue-noise cross-fade, bounded FSR reactive band, "Tier 2 groups a 4x4 set into a six-vertex card") and §12.14 ("since `fd0cd577c` the ribbon writes `0.9 * max(midTransition, cardTransition)` ... must not regress to a blanket 1.0") all describe the removed design, while `:1864` says "no stochastic cross-fade, no FSR reactive exception and no clump-card tier". `fsr3-upscaler-integration-plan.md:520-526` repeats the removed reactive contract and names the test that now asserts the opposite. Code comments (`blade.vert:84-93`, `frame.rs:122-127`, `groundcover.rs:754-761`) still describe the 2-bit tier word as carrying "the clump-card tier" and a "blue-noise LOD cross-fade". `GpuGroundCoverSpecies.card_atlas` / `cardAtlas` is documented as the Tier-2 card atlas but no ground-cover shader reads it any more (the sampling was deleted in `af8323881`'s frag diff; only the host copy and a unit test remain).
- **Evidence**: `grep -rn "cardAtlas\|card_atlas" crates byroredux --include='*.rs' --include='*.glsl'` -> 5 hits, none a shader read; `grep -n "stochastic\|midTransition\|cardTransition" docs/engine/exal-groundcover.md docs/engine/fsr3-upscaler-integration-plan.md`.
- **Impact**: a reader following §12.14 / the FSR plan could "restore" a reactive mask the shader deliberately dropped; the FSR plan names a test as the pin for a contract that test now inverts.
- **Related**: af8323881, 17619c0bf, fd0cd577c; the dead `card_atlas` field is a Dim 8 candidate.
- **Suggested Fix**: mark `exal-groundcover.md` §6 table + "Current geometric LOD implementation" + §12.14 reactive paragraph as superseded (or rewrite to the segment-tier model), rewrite the `construct.rs:775` comment and the tier-word comments to "tier index (3 segment levels)", delete or re-document `card_atlas`.

#### TD3-2026-10-15-02: Papyrus animation-graph module docs and scripting.md still say actor graphs do not run, blends/conditions/layered blenders/modifiers are not evaluated
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/scripting/src/fragment/anim_events.rs:20-22`; `crates/scripting/src/animation_graph.rs:25-27`; `byroredux/src/asset_provider/actor_graph.rs:14-15`; `docs/engine/scripting.md:1475, 1491-1495, 1522-1524`
- **Status**: NEW (the VM lead; confirmed and widened)
- **Age**: `3bb6806b5` (FO4 actor graphs run, 2026-10-14), `49529db7c` (whole generator tree, layered blends, modifiers, 2026-10-14), `64fea511f` (unregistered event on unload), `scripted_sound_system` (SoundPlay.*)
- **Effort**: small
- **Description**: `anim_events.rs` lists as "Not modelled" three things that now are: `OnAnimationEventUnregistered` (`unregister_unloaded`, ~20 lines below, sends it), "actors' graphs (none runs)" (FO4 actors run an `ObjectAnimationGraph`, same component the system drains), and `SoundPlay.*` events (scripting.md:1519 says they play). `animation_graph.rs:26` says "Not run: layered (non-parametric, bone-masked) blenders ...; modifiers", yet 49529db7c's message is "layered blenders with bone-weight masks ... the modifiers that act on the graph itself run" and `animation_modifiers.rs` exists. `actor_graph.rs:14` says "Not evaluated: blend weights and variables, transition blends and conditions, and the graph's modifiers" while the sibling module doc (lines 17-24) says weights, blends and conditions run. `scripting.md` carries the same pre-`49529db7c` text ("Transitions are instantaneous (no blends)", "blend weights ... are not kept", "only the motion leaf plays at a time", "`SetAnimationVariable*` have no graph variable to act on and do nothing") and, at :1523, "Not modelled: actors' graphs (none runs)", eighty lines above its own :1592 "The graph itself now runs on the actor".
- **Evidence**: `sed -n 20,22p crates/scripting/src/fragment/anim_events.rs`; `grep -n "none runs\|no blends\|motion leaf" docs/engine/scripting.md`; `git show 49529db7c --stat`.
- **Impact**: `/audit-scripting` is told scripting.md is authoritative; it would flag live behaviour as missing or miss that the docs under-state the scope of `object_graph_step_system`.
- **Related**: #5788 (same class, different module), 6f48ffbcd.
- **Suggested Fix**: rewrite the three "Not modelled/Not run/Not evaluated" sentences to what each commit left (e.g. `SoundPlayAt.<name>.<node>` and pose-modifiers: look-at, twist, foot IK, ragdoll, jiggle, per `animation_modifiers.rs:31`), and delete the superseded scripting.md sentences.

#### TD3-2026-10-15-03: `fragment/vm.rs` module doc ends "Not modelled: structs." although `Value::Struct` and FO4 struct support exist
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/scripting/src/fragment/vm.rs:50`
- **Status**: NEW
- **Age**: `20cbfd7b7` (2026-10-13, "Fallout 4 structs and array methods on the Papyrus VM"); the line predates it (`4cfdc525b`).
- **Effort**: trivial
- **Description**: `Value::Struct(Rc<RefCell<Vec<(String, Value)>>>)` is at `:107`, `PropertyValue::Struct` converts at `:214`, and `scripting.md:696-707` documents struct and array-method support (including save serialisation).
- **Evidence**: `sed -n 100,110p` vs `sed -n 50p crates/scripting/src/fragment/vm.rs`.
- **Impact**: the module header is the first thing a reader of a 6 215-line file sees.
- **Related**: #5788 is the open sibling for `obscript_quests.rs`'s "phase 1" header (`:1`, `:14`, `:22`, `:207`) — do not re-file; widen it to the `0x1059` / `0x101f` rows.
- **Suggested Fix**: delete the line (or replace it with what is genuinely unmodelled, e.g. "a struct or array held across `Utility.Wait` — the saved continuation stores scalars only", `scripting.md:926`).

#### TD3-2026-10-15-04: Skyrim NPC/player skill population (7c740eca0) is not reflected in `feature-matrix.md`, `charal.md` or the `actor_value_derive.rs` header
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:268, 296-299`; `docs/engine/charal.md:117`; `crates/plugin/src/esm/records/actor_value_derive.rs:1-14`
- **Status**: NEW
- **Age**: `7c740eca0` (2026-10-14)
- **Effort**: trivial
- **Description**: `feature-matrix.md:296-299` says Skyrim population derives Health/Magicka/Stamina "— no other actor values or skills"; `derive_skyrim_actor_values` now appends `derive_skyrim_skills` (18 skills, `SKYRIM_SKILLS`). Row `:268` lists "Health+Magicka+Stamina" only. `actor_value_derive.rs:11` describes Skyrim as "seeds Health from the race's starting Health plus ... ACBS Health offset" (Magicka/Stamina were already missing, skills now too). `charal.md:117` still gives the derive site as "(today: FNV/FO3)" though Skyrim and FO4-stored paths live there. `charal-skyrim-ruleset.md` itself does not call skill derivation deferred (lead 6 checked: only the Armor-Rating NPC `0.015` constant is named "not modelled", which is still true).
- **Evidence**: `git show 7c740eca0 --stat`; `grep -n "no other actor values" docs/feature-matrix.md`.
- **Impact**: `feature-matrix.md` is the status floor; it contradicts `scripting.md:1395` ("Skyrim's skills are now populated").
- **Related**: #5000-series CHARAL population work; `bribe.info`.
- **Suggested Fix**: one sentence each in the three places: Skyrim = pools + 18 skills (UESP CLAS formula, stored NPC keeps DNAM).

#### TD3-2026-10-15-05: `feature-matrix.md` omits the Papyrus VM, marks Story Manager payload and per-material footsteps as not started
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:158, 185, 207, 343`
- **Status**: NEW
- **Age**: Papyrus VM `0c6fe4d34`..`195e3b4ef` (2026-10-12/13); Story Manager Phases 1-4 (2026-10-07/08, ROADMAP:286); footsteps `886edcded` (2026-10-10, ROADMAP:366 "implemented in full")
- **Effort**: small
- **Description**: (a) Row 185 "Full Papyrus transpiler (M47.2) ... full transpiler deferred" and row 343 "Arbitrary script execution on real content (`.pex` recognizer slice shipped Session 51)" never mention the interpreter that now runs what the lowering declines (`fragment/vm.rs`, per-frame budget #5763, reference/alias/update/event scripts); the word "VM" does not appear in the file. ROADMAP:75 and :195 say it exists. (b) Row 207 "Story Manager event payload / search ✗" is the file's only Story Manager line; Phases 0-4 shipped (SMBN/SMEN/SMQN decode, dispatcher, R1/R2/L1/L2 payload slots, policy bits, KILL/CLOC/AHEL producers, `sm1-story-manager.sh`). (c) Row 158 "Per-material footsteps (FOOT records) ✗" — the selection mechanism (`select_footstep_sound`, `PhysicsWorld::surface_material_under`) shipped; the table is empty pending IPDS decode, so "~ mechanism shipped, no authored data" is the accurate cell.
- **Evidence**: `grep -n "Story Manager\|VM\b" docs/feature-matrix.md`; `sed -n 281,330p byroredux/src/systems/audio.rs`.
- **Impact**: the doc the audits treat as a floor under-states shipped work in the highest-churn subsystem.
- **Related**: #5366, #5367, TD3-2026-10-11-04 (#5600, same class), #5763.
- **Suggested Fix**: add a "Papyrus VM (bounded, decline-fallback)" row and an SM row, change row 207 to the remaining gap (SCPT/LEVL/SKIL producers), re-mark 158 as `~`.

#### TD3-2026-10-15-06: `DISPATCH_HANDLED_FOURCCS` doc comment says LCTN "has never had a top-level dispatch arm" and the list below it contains LCTN
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/plugin/src/esm/records/parse.rs:35-39` (comment), `:57` (list entry), `:431` (arm), `crates/plugin/src/esm/records/dispatch_misc_stub.rs:144`
- **Status**: NEW (regression of the #4437 drift note)
- **Age**: note `a3d8efb61` (2026-09-19) was true then; `6f6a9eb3d` (2026-10-12, "LCTN parse") made it false.
- **Effort**: trivial
- **Description**: the arm at `dispatch_misc_stub.rs:144` runs `extract_records` for LCTN; the list is guarded by `dispatch_handled_fourccs_matches_the_live_dispatch_arms`, so the list is right and the comment is wrong. Left as is, the comment tells a maintainer to remove a correct entry.
- **Evidence**: `sed -n 35,39p crates/plugin/src/esm/records/parse.rs`; `sed -n 144p crates/plugin/src/esm/records/dispatch_misc_stub.rs`.
- **Impact**: could cause the "regression class inverted" the comment itself warns about.
- **Related**: #4437, #4278, #5758 (open sf-esm baseline staleness, same family).
- **Suggested Fix**: replace the parenthetical with "LCTN gained its arm in 6f6a9eb3d".

#### TD3-2026-10-15-07: The m42-eat-sleep smoke is described as seating the settler at the bed in CLAUDE.md, the smoke README and the script's own section header; it now asserts selection + idle
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md:104-109`; `docs/smoke-tests/README.md:57`; `docs/smoke-tests/m42-eat-sleep.sh:97-98`
- **Status**: NEW
- **Age**: `5f205f0ca` (2026-10-13)
- **Effort**: trivial
- **Description**: the CLAUDE.md sentence "the walk-then-seat runtime must seat HIM at the bed (the seat leg is scoped to the found entity)" and the README row "window selection, walk-then-seat" predate #5391/#5500; the script now fails if the settler IS `Seated` (`:123-125`) and says the seat runtime is unit-gated (`:14-15`, `:118`). The script's own banner at `:97-98` ("... then the walk-then-seat runtime seats her at the bed") contradicts the comment block 12 lines below it.
- **Evidence**: `sed -n 97,127p docs/smoke-tests/m42-eat-sleep.sh`; `git show 5f205f0ca`.
- **Impact**: someone reading CLAUDE.md believes a live seat leg gates Eat/Sleep; it does not.
- **Related**: #5391 (closed), #5500, TD3-2026-10-15-12.
- **Suggested Fix**: CLAUDE.md: "... must hand him `SleepBehavior`, and — because his In-Cell package targets another cell (#5391) — he must idle, not walk or seat; the seat runtime is unit-gated". Same edit in the README row and the `:97` banner.

#### TD3-2026-10-15-08: ci.yml says the renderer has "733 `unsafe` blocks"; it has 704
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `.github/workflows/ci.yml:211`
- **Status**: NEW
- **Age**: `530c9e7aa` (2026-09-30)
- **Effort**: trivial
- **Description**: `grep -rE "unsafe *\{" crates/renderer/src --include='*.rs' | wc -l` = 704 (no commented-out hits). The helper consolidation (#5593 etc.) shrank it.
- **Evidence**: command above; `sed -n 211p .github/workflows/ci.yml`.
- **Impact**: none functional; a hand-typed count in a CI comment will drift again.
- **Related**: #5121, #4595.
- **Suggested Fix**: drop the number ("the renderer's `unsafe` blocks").

#### TD3-2026-10-15-09: ROADMAP "Growth discipline — Tripwire today" is off by 2.5x on two rows, names a deleted file, and omits four files over the 3 500-line line
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `ROADMAP.md:443-450`
- **Status**: NEW
- **Age**: measured 2026-10-10; `c140cb48e` (#5584) deleted `byroredux/src/env_translate.rs` (now `env_translate/{mod,climate,fallback,lod,sky_weather,water}.rs`); the M47.2 native-call arms grew `translate/effects.rs`.
- **Effort**: trivial
- **Description**: at HEAD (inline tests included, dedicated test files excluded, the doc's own rule): `crates/scripting/src/translate/effects.rs` 10 117 (doc: 4 022), `crates/physics/src/world/mod.rs` 5 209 (doc: 4 149), `byroredux/src/inventory.rs` 4 202 (4 075). Missing: `crates/scripting/src/fragment/vm.rs` 6 215, `crates/physics/src/water.rs` 3 963, `crates/scripting/src/condition.rs` 3 691, `crates/plugin/src/esm/records/misc/world.rs` 3 508. `env_translate.rs` no longer exists.
- **Evidence**: `find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -not -path '*/tests/*' -not -name '*_tests.rs' -not -name 'tests.rs' -exec wc -l {} + | awk '$1>3500'`.
- **Impact**: ROADMAP:443 is the only place the 3 500 "investigate before adding" threshold is operationalised, and its list is the list `/audit-tech-debt` Dim 1 is expected to reconcile. `ROADMAP.md` is at 798 lines against the 800 cap (#5111), so the fix must be net-zero lines.
- **Related**: #5111, #5584, Dim 1 report (effects.rs / vm.rs).
- **Suggested Fix**: rewrite the list to the nine files above with 2026-10-15 numbers (or remove the numbers and keep the command).

#### TD3-2026-10-15-10: CLAUDE.md "Workspace Structure" still omits 16 of 29 crates and 3 of 4 tools, and describes `components.rs` as the marker-component home (regression of #5602, CLAUDE.md half)
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md` Workspace Structure block (crates list; `src/components.rs` row at line ~ 9 of the block)
- **Status**: Regression of #5602 (CLOSED; fix commit `65e6562fc` touched only ROADMAP.md although the title names CLAUDE.md)
- **Age**: crates added since the block was written; `6ef8976f2` (2026-10-14) split `components.rs` into `components/{render,terrain,environment,indices,input,audio,tuning,ai_anim,navmesh}.rs`
- **Effort**: small
- **Description**: the tree lists core, plugin, renderer, bsa, platform, nif, ui, scripting, menuxml, papyrus, cxx-bridge, debug-protocol, debug-server (13) plus `tools/byro-dbg`. Absent: audio, bgsm, boot-request, debug-ui, facegen, fsr3-sys, game-detect, hkx, mod-runtime, pex, physics, save, sdk, settings-io, sfmaterial, spt, and tools byro-detect, byro-launcher, texture-upscale. `src/components.rs  Marker components (Spinning, AlphaBlend, TwoSided, Decal) + app resources` is now a ~30-LOC hub. Other named docs (README, ROADMAP Crate Map) are complete.
- **Evidence**: `ls crates tools`; `sed -n '/^## Workspace Structure/,/^## Architecture/p' CLAUDE.md`.
- **Impact**: CLAUDE.md is loaded into every session; agents locate physics / save / audio code from it.
- **Related**: #5602, #5105, #4351.
- **Suggested Fix**: add a one-line-per-crate list for the 19 missing members (names + one clause), or replace the crate list with a pointer to ROADMAP's Crate Map; update the `components.rs` row.

#### TD3-2026-10-15-11: The #5470 space-run gate still exempts `byroredux/src/main.rs`, hiding four collapsed-continuation messages (carried, third report, never filed)
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments (text hygiene)
- **Location**: `byroredux/src/main.rs:338, 344, 415, 423`; exemption `byroredux/src/workspace_hygiene_tests.rs:397`
- **Status**: Regression of #5470 (CLOSED); carries TD3-2026-10-12-02 / 10-11 TD3-01
- **Age**: `f8ea9d46f` (2026-08-31)
- **Effort**: trivial
- **Description**: unchanged since the 10-12 report: `grep -nE '"[^" ][^"]*[^ "] {6,}[^ "][^"]*"' byroredux/src/main.rs | grep -v '\\n'` returns the same four lines (line numbers shifted by +6).
- **Evidence**: command above.
- **Impact**: cosmetic; the exemption is a blind spot.
- **Related**: #5470, #5481.
- **Suggested Fix**: re-wrap with `\` continuations; delete the `main.rs` exemption.

#### TD3-2026-10-15-12: Remaining M42 procedure-count contradictions after #5471 (carried, narrowed; `feature-matrix.md` itself is now consistent)
- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/engine/npc-spawn-ai-packages.md:9` (vs `:522`); `ROADMAP.md:61`, `:95`; `docs/engine/game-loop.md:155`
- **Status**: Regression of #5471 (CLOSED); carries TD3-2026-10-12-03
- **Age**: Eat/Sleep `00f580e09` (2026-10-08), Dialogue `14cff35ae` (2026-10-07); `graph_locomotion_system` / `console_walk_system` registered at `boot/schedule/post_update.rs:139-140`
- **Effort**: trivial
- **Description**: `npc-spawn-ai-packages.md:9` "exactly seven procedures execute" vs `:522` "Ten procedures of ~17 execute"; `ROADMAP.md:61` "Seven AI package procedures run by default" vs `feature-matrix.md:89` (10); `ROADMAP.md:95` "10 of ~17 package procedures need subsystems that do not exist (... dialogue)" (should be 7 missing, dialogue exists); `game-loop.md:155` PostUpdate row lists sandbox/wander/travel/follow/escort/guard/patrol only (no `make_eat_sleep_system`, `graph_locomotion_system`, `console_walk_system`).
- **Evidence**: `grep -n "exactly seven\|Seven AI package\|10 of ~17" docs/engine/npc-spawn-ai-packages.md ROADMAP.md`.
- **Impact**: `/audit-gameplay` treats `npc-spawn-ai-packages.md` as authoritative and is told `eat_sleep_system` should not exist.
- **Related**: #5471, TD3-2026-10-15-07.
- **Suggested Fix**: "exactly seven" -> "ten", ROADMAP:61 "Seven" -> "Ten", ROADMAP:95 -> "7 of ~17 (item use, package combat, magic)", add the three systems to the game-loop row.

#### TD3-2026-10-15-13: CLAUDE.md / contributing.md toolchain note ("default distro rustc here is 1.93.1 ... `rustup which` workaround required") no longer matches this machine
- **Severity**: LOW (environment-dependent; needs the user's confirmation)
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md:29-40`; `docs/contributing.md:123-145`; `rust-toolchain.toml:9-11` comment
- **Status**: NEW
- **Age**: `8529a3e6e` (2026-10-01); `rust-toolchain.toml` pin #5308
- **Effort**: trivial
- **Description**: in this session `which cargo` is `~/.cargo/bin/cargo`, `cargo --version` = 1.96.0 (the repo pin, active in the repo dir; `rustup toolchain list` shows `1.96.0 (active)`), and the distro `/usr/bin/rustc` is 1.97.1, not 1.93.1 — both satisfy the 1.96 MSRV. The "distro cargo shadows the rustup shims; `cargo +1.96.0` does NOT work" premise does not hold on this PATH, so bare `cargo test -p byroredux --bin byroredux` may already work. I did not run it (read-only audit).
- **Evidence**: `rustup toolchain list`, `cargo --version`, `/usr/bin/rustc --version`.
- **Impact**: agents follow the longer `rustup which` ritual (still correct) believing the short form fails; low harm.
- **Related**: #4466, #5308.
- **Suggested Fix**: user to confirm on their interactive shell; if bare `cargo` works, reword to "the repo pins 1.96.0 via rust-toolchain.toml; if your PATH puts a distro cargo first, use the `rustup which` form".

#### TD4-2026-10-15-01: Three deleted/historical names are still backticked in skills (the real part of the advisory list)
- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.claude/commands/audit-oblivion/SKILL.md:101` (`inherit_all_up_chain`), `:103` (`env_translate.rs`), `.claude/commands/audit-nifal/SKILL.md:90` (`SourceSoftAdditive`)
- **Status**: NEW as an issue — carry-over of prior-report TD4-2026-10-12-02 (never published as its own issue; the validator half is Existing #5607)
- **Age**: `inherit_all_up_chain` deleted by `65cadde2f` (#5516, 2026-10-12); `env_translate.rs` split by `c140cb48e` (#5584, 2026-10-12); the sync touched both lines and kept the backticks
- **Effort**: trivial
- **Description**: `_audit-common.md` Path-Reference Convention says deleted things go in italics. `inherit_all_up_chain` ("deleted the dead ... copy") and `env_translate.rs` ("split `env_translate.rs` into ...") are history; `SourceSoftAdditive` is a CDB `BlendingMode` string value, not a symbol. The validator prints all three as advisories on every run, which is exactly the signal that lets a future renamed guard hide.
- **Evidence**: `/tmp/audit/tech-debt/d4_validate.txt` lines 8 and 82-94; `grep -rn inherit_all_up_chain crates byroredux` empty; `git ls-files | grep env_translate` shows only the `env_translate/` dir files.
- **Impact**: advisory noise masks real renames; convention drift.
- **Related**: #5607, TD4-2026-10-12-02.
- **Suggested Fix**: italicise the three names (or drop backticks); re-run `_audit-validate.sh`.

#### TD4-2026-10-15-02: The validator advisory is mostly extractor truncation and non-Rust tokens; the sync-era leads (`triangle_early.frag`, `ImportedMaterial.sh`) are noise, `env_translate.rs` is real
- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: noise sites: `audit-performance/SKILL.md:55,128,129`, `audit-renderer/SKILL.md:38,111`, `docs/engine/shader-pipeline.md:259,403` (`triangle_early.frag.spv`); `audit-skyrim/SKILL.md:60` (`ImportedMaterial.shader_type`); `docs/engine/shader-pipeline.md:661` (same truncation class); `audit-physics/SKILL.md:15` (`ragdoll_installed_tests.rs`), `:72` (`ragdoll_installed` test filter); shell/EDID tokens: `FIXTURE_DATA_ENV` (`audit-runtime:108`), `FIXTURE_GATES` (`audit-fo3:102`), `GSHouseInterior02` (`audit-fnv:103`), `WIGreeting` (`audit-runtime:20`), `CRHoldExpansion` (`CLAUDE.md`)
- **Status**: Existing: #5607 (extends it with the measured split; corrects TD4-2026-10-12-02's claim about `triangle_early.frag`)
- **Age**: `triangle_early.frag.spv` truncation documented as noise since 2026-10-05; EDID entries new with the 10-12 to 10-15 fix wave
- **Effort**: small
- **Description**: Of 7 skill-symbol advisories and 6 skill-basename advisories, only the three in TD4-01 are rot. `triangle_early.frag.spv` is a real tracked file (`crates/renderer/shaders/triangle_early.frag.spv`); the extractor's extension list stops at `.frag`, so the check reports `triangle_early.frag`. The 10-12 report's remark that it "would also read better in italics" is wrong: the skill text is correct. `ImportedMaterial.sh` is `ImportedMaterial.shader_type` cut at `.sh` (the extractor's documented truncation, `_audit-validate.sh:310-323`). `ragdoll_installed` is a deliberate non-symbol test filter. `WIGreeting`, `GSHouseInterior02`, `CRHoldExpansion` are Skyrim/FNV editor IDs. `docs/engine` advisories now run to 345 symbols (lines 96-447 of the output), so the signal-to-noise problem #5607 describes has grown.
- **Evidence**: `git ls-files | grep triangle_early` -> the `.spv`; validator output lines 8-14, 82-94 (`/tmp/audit/tech-debt/d4_validate.txt`).
- **Impact**: same as #5607: a real rename is invisible in 400 lines of scroll.
- **Related**: #5607, TD4-01, TD4-2026-10-12-02.
- **Suggested Fix**: in `_audit-validate.sh` add `.spv` to the extension list (and treat a trailing `.name` after a known extension as a truncation, not a missing file), skip `*.sh`/`*.env` variable names by also grepping `docs/smoke-tests/`, and italicise the EDIDs; then make the skills-only advisory fail the script as #5607 proposes.

#### TD4-2026-10-15-03: `.claude/agents/*.md` is outside both the validator and the sync, and two of the three agents carry stale architecture facts
- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.claude/agents/renderer-specialist.md:14,31,32-34`; `.claude/agents/ecs-specialist.md:29`
- **Status**: NEW
- **Age**: last touched 2026-05-12 (`b38f3ac74`), five months and hundreds of commits ago
- **Effort**: small
- **Description**: These agents are loaded into renderer/ECS audit sub-agents. Verified stale: (1) `src/vulkan/context.rs` is now `context/` (a dir; the validator misses it because it checks only `command_files` + `docs/engine`); (2) "gpu-allocator 0.27" but `Cargo.toml:87` pins 0.28; (3) "Push constants: 128 bytes (mat4 viewProj + mat4 model)": the main pipeline in `pipeline.rs` declares no push-constant range (per-draw data is the instance SSBO); the remaining ranges are small per-pass ones (`water.rs`, `svgf.rs`, `caustic.rs`, `presentation.rs`, `groundcover_models.rs`); (4) "`GpuBuffer` (vertex/index, HOST_VISIBLE for now)"; (5) ecs-specialist: "`systems.rs` itself is a 27-line module index" (it is 81 lines), `footstep_system` "lives in `animation.rs`" (it lives in `systems/audio.rs`), and the `systems/{...}.rs` list names 9 files of the 36 in the directory. `audit-sync`'s `meta` group owns `_audit-*` and `.agents/skills` but not `.claude/agents/`.
- **Evidence**: `grep -n "push_constant_ranges" crates/renderer/src/vulkan/pipeline.rs` (none); `wc -l byroredux/src/systems.rs` = 81; `ls byroredux/src/systems | wc -l` = 36 files; `grep -rln "fn footstep_system" byroredux/src` = `systems/audio.rs`.
- **Impact**: specialists asked to review sync or push constants start from a false premise; same failure class as skill premises, with no gate.
- **Related**: `audit-sync/SKILL.md:43-45` (meta group scope).
- **Suggested Fix**: rewrite the two agent files' layout/invariant sections to point at `CLAUDE.md` + `docs/engine/renderer.md` instead of hand-typed trees; add `.claude/agents/*.md` to `command_files` in `_audit-validate.sh` and to the `meta` group's remit.

#### TD4-2026-10-15-04: The `audit-scripting` Codex shim's description omits the M47.2 Papyrus VM and the `/audit-papyrus` split
- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.agents/skills/source-command-audit-scripting-skill/SKILL.md:3` (description) vs `.claude/commands/audit-scripting/SKILL.md:2`
- **Status**: NEW
- **Age**: shim last touched 2026-10-12 (`5d31fd637`) or earlier; the canonical description gained the VM clause and the compiler-frontends handoff since
- **Effort**: trivial
- **Description**: The shim body is a correct thin pointer and its "seven dimensions" is right, but its `description:` (the text Codex matches triggers on) lacks the M47.2 Papyrus VM (budget, reference/alias scripts, update/event registrations) and "compiler frontends are /audit-papyrus", and says "SKSE/JContainers provider-compat" where the canonical says extender-compat provider layer. A request for a "Papyrus VM audit" does not trigger it. A word-diff of all 34 shim/canonical descriptions shows every other shim within 0-5 minor words; this is the only material divergence. `audit-sync` tells the `meta` group to check shim descriptions; this one slipped.
- **Evidence**: shim vs canonical description diff (16 canonical words absent from the shim).
- **Impact**: wrong routing for non-Claude agents; low.
- **Related**: `audit-sync/SKILL.md:43-45`.
- **Suggested Fix**: copy the canonical description clause into the shim's `description:` and its "Use when" list.

#### TD4-2026-10-15-05: Fix trackers #5742 and #5606 are fixed in the tree but still OPEN
- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: GitHub #5742 (TD4-2026-10-12-01), #5606 (TD4-2026-10-11-02); fix is in `f44dacbda`
- **Status**: Existing: #5742, #5606
- **Age**: fixed 2026-10-15 (`f44dacbda` "Refactor audit sync and tech debt scripts..."); neither the subject nor body carries `Fix #N`
- **Effort**: trivial
- **Description**: `gh issue view 5742` -> OPEN, yet the four callouts it lists are rewritten (see "already fixed" above). #5606's twelve-skill list re-swept clean: no closed issue is presented as open in any skill. Per the fix-to-issue link convention (#5083), these will rot as "open" trackers and be re-verified by later audits.
- **Evidence**: `gh issue view 5742 --json state` -> OPEN; sweep output: 0 true positives out of 218 candidate refs.
- **Impact**: audit time re-verifying fixed trackers (the #5083 pattern).
- **Related**: #5083, #5606, #5742.
- **Suggested Fix**: close #5742 and #5606 with a comment citing `f44dacbda`; #5607 stays open (validator half, TD4-02 above). Not closed here (read-only audit).

#### TD5-2026-10-15-01: `scene.rs` carries a past-tense "TODO" that re-matches the marker grep on every audit, naming a closed issue
- **Severity**: LOW
- **Dimension**: 5 Stale Markers
- **Location**: `byroredux/src/scene.rs:1290`
- **Age**: `e6192cc573`, 2026-05-14 (5 months)
- **Effort**: trivial
- **Status**: NEW (flagged as noise in the 10-12 report's Dim 5 line, never actioned)
- **Description**: The comment "Closes the #242 consumer-side TODO (#1055)" documents a *finished* piece of work (the geometry-SSBO StagingPool reuse), but contains the literal token `TODO` and a closed issue number. Every run of the Dim 5 recipe (and the Phase-1 marker baseline) re-surfaces it and an auditor must re-triage it as "past tense".
- **Evidence**: `// rebuilds. Closes the #242 consumer-side TODO (#1055).` - #242 CLOSED, #1055 is the fix.
- **Impact**: Pure grep noise, 1 of the 24 baseline hits; no behaviour.
- **Related**: `AUDIT_TECH_DEBT_2026-10-12.md` Dim 5 summary.
- **Suggested Fix**: Reword to "Closes #242's consumer side (#1055)." so the Dim 5 baseline drops to 23 real-or-false-positive hits and the grep means something.

#### TD6-2026-10-15-01: `RecurringUpdate` / `recurring_update_tick_system` are documented "shipped (recognizer path)" and "the half that has producers", but the only producer is a demo system whose script component nothing spawns
- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `docs/engine/scripting.md:116-118`, `:2500`, `:2507`; `crates/scripting/src/recurring_update.rs`; `crates/scripting/src/papyrus_demo/dlc2_ttr4a.rs:186-215`; `byroredux/src/boot/schedule/update.rs:131-136,609-616,661-666`
- **Age**: false claim added by `bef345396` (the #5608 fix) 2026-10-13; the underlying orphaning dates to the demo's `8d054a9db` 2026-05-16
- **Effort**: small
- **Status**: NEW (sibling of closed #5608; #5608's fix wrote it)
- **Description**: #5608 corrected `ScriptTimer` but did so by contrasting it with `RecurringUpdate`: "Only the `RecurringUpdate` half has producers today" (`scripting.md:116`), "`RecurringUpdate` is **shipped** (the recognizer path)" (`:2500`), "`RecurringUpdate { ... }` shipped (recognizer path)" (`:2507`). There is no recognizer path: `translate/recognizers/` holds exactly `quest_stage_gate`, `rumble`, `two_state_activator`, and none constructs a `RecurringUpdate`. The sole non-test construction in the workspace is `RecurringUpdate::every(5.0)` inside `dlc2_ttr4a_on_init_system`, which iterates `Dlc2Ttr4aPlayerScript` entities; the only constructor of that component, `dlc2_ttr4a_player_script()`, has zero non-test callers. The M47.2 VM's `RegisterForUpdate` rides `fragment/updates.rs` (`ScriptUpdates`/`UpdateRegistration`) instead, as `scripting.md:1018` itself says. The save-registry ledger already knows: `registry_completeness_tests.rs:730` "`RecurringUpdate` - forward-latent - its only writer is unreachable in production today".
- **Evidence**: `grep -rn "RecurringUpdate::\|RecurringUpdate {" crates byroredux --include='*.rs'` minus tests/examples -> only `recurring_update.rs` itself and `dlc2_ttr4a.rs:210`; `grep -rnw "Dlc2Ttr4aPlayerScript"` -> `dlc2_ttr4a.rs` + tests; `registry_completeness_tests.rs:663` ("no live production spawn site exists outside tests/examples"). Same R5 demo family, same ledger: `MG07LabyrinthianDoor` (:675 "no live production spawn site ... despite its systems being scheduler-wired"), `KeystoneInventory` (:674), `UiMessageCommand` (:759), `ActorStats` (:649).
- **Impact**: (1) a reader or auditor trusting `scripting.md` believes the recognizer path can fire `OnUpdate`; it cannot - `papyrus_provider/execute.rs:155` queries `OnUpdateEvent` and `cleanup.rs:101` drains it, both waiting on a producer that does not exist. (2) Six systems run every frame in `Stage::Update` with nothing to do: `recurring_update_tick_system`, `dlc2_ttr4a_on_init_dispatch`, `dlc2_ttr4a_on_update_dispatch`, `mg07_on_load_dispatch`, `mg07_on_activate_dispatch`, `mg07_tick_system` (the dispatch ones are *exclusive* systems, i.e. scheduler serialization points), plus `MG07LabyrinthianDoor` branches in `byroredux/src/interaction.rs:975,1363,1407`. (3) `dlc2_ttr4a.rs` module docs say "The transpiler emits an equivalent factory for every per-script component it generates" - no such emission exists. `scripting.md` (R5 demo section, ~:2250) does call the demos "living evidence and a test surface", so the *demo* rows are honest; the `RecurringUpdate` rows are not.
- **Related**: #5608 (CLOSED docs-only), #5609 / #5610 (still OPEN, same "no production consumer" family), `docs/engine/scripting.md:1018` ("the older `RecurringUpdate` ... serves only the hand-written recognizer path" - also worth rewording, since no recognizer emits it).
- **Suggested Fix**: Edit the three `scripting.md` rows to "`RecurringUpdate` ... dormant: only the `dlc2_ttr4a` demo writes it, and nothing spawns that demo; M47.2 `RegisterForUpdate` rides `UpdateRegistration`". Then either drop the five demo exclusive systems + `recurring_update_tick_system` from the schedule behind a `cfg(test)`/example-only registration (keeping the modules as test surface), or keep them and say so in the schedule comment.

#### TD6-2026-10-15-02: `AttachPoints` / `ChildAttachConnections` are stamped on every FO4 placement root but have no reader; their docs say the equip system reads them
- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/core/src/ecs/components/attach_points.rs:1-30,95-140`; stamp site `byroredux/src/cell_loader/spawn.rs:551-570`; import `byroredux/src/cell_loader/references/import.rs:169-170`, `attach.rs`; docs `docs/engine/ecs.md:538`
- **Age**: `01b088119` 2026-05-12 (#985; 5 months)
- **Effort**: small (doc) / large (the equip consumer, FO4 OMOD weapon mods)
- **Status**: NEW (drivers #985 and #1594 are CLOSED - they closed the *import->ECS* half only)
- **Description**: The component module doc says "The equip system composes the world transform as `parent_world * attach_point.local_transform * accessory.local`", "The equip system queries this component on the parent", and `ChildAttachConnections` "The equip system reads this when an accessory is being mounted ... or the accessory can't mount". No such system exists. `AttachPoints::find` (the "equip hot path" lookup) has no caller outside its own tests, and nothing in `byroredux/src/` reads either component; the save ledger records them as "write-once NIF-import data, no runtime mutator" (`registry_completeness_tests.rs:587,595`). FO4 OMOD-driven weapon-mod assembly is not implemented, so the components are inert on every FO4 weapon placement.
- **Evidence**: `grep -rnw "AttachPoints\|ChildAttachConnections" byroredux crates --include='*.rs'` minus tests -> builders (`references/attach.rs`), cache fields (`nif_import_registry.rs:317,322`), the stamp (`spawn.rs:561-566`) and `world.insert`; zero `get`/`query`/`find` sites. `ROADMAP.md`/`feature-matrix.md` make no weapon-mod claim (so no roadmap lie), but `ecs.md:538` lists them in the component catalog without the "unconsumed" caveat.
- **Impact**: per-modular-weapon placement carries two inert SparseSet inserts (+ a `FixedString` intern pass at import) for a consumer 5 months overdue; the doc misdirects an FO4 weapon-mod implementer into thinking half the equip path exists. Whether the NIF-side `attach_points`/`child_attach_connections` cache fields are worth their per-import cost is the sibling Dim 8 question.
- **Related**: #985, #1594 (both CLOSED), `registry_completeness_tests.rs:587`.
- **Suggested Fix**: Add a "No consumer yet - FO4 OMOD weapon-mod assembly (unimplemented) will read this" paragraph to `attach_points.rs` and an annotation in `ecs.md:538`, matching the honest style of `audio/src/lib.rs:14-18` (#5612) and `scripting/src/timer.rs:7-17` (#5608). Re-file the consumer as a tracked ROADMAP item rather than leaving it implied by a closed issue.

#### TD7-2026-10-15-02: Three different bare literals bound "follow the keyword's linked-ref chain" in the same VM, and `GetNthLinkedRef` silently truncates at 64
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/scripting/src/fragment/vm.rs:2889` (`seen.len() > 256`, Enable/DisableLinkChain), `:3241` (`map_or(100, ..)`, GetLinkedRefChain default, with a caller-supplied `limit` that is otherwise unbounded), `:3721` (`.min(64)`, GetNthLinkedRef)
- **Status**: NEW
- **Age**: `12fb0c549`, `7caefeeb1`, `d7b7f5f78` (2026-10-13)
- **Effort**: trivial
- **Description**: the `100` is the CK-documented `iMaxExpectedLinkedRefs` default (the comment at `:3232-3234` says so); the `256` and `64` have no source and no name. A script calling `GetNthLinkedRef(70)` gets the 64th link, not the 70th or `None`; `EnableLinkChain` stops at 256 links while `GetLinkedRefChain` stops at 100 over the same graph.
- **Evidence**: `for _ in 0..(hops.max(0.0) as usize).min(64)`; `if !seen.insert(link) || seen.len() > 256 { break; }`.
- **Impact**: wrong ref for deep chains (no vanilla chain is that long), and three unrelated tunables for one concept.
- **Related**: TD7-2026-10-15-01 (same file, same missing-ceiling class).
- **Suggested Fix**: one `MAX_LINK_CHAIN_HOPS` const (CK default 100 as the documented value), clamp the caller's `limit` to it, and make `GetNthLinkedRef` return `None` past the cap instead of the capped hop.

#### TD7-2026-10-15-03: The three ground-cover passes hand-sum their descriptor-pool sizes while eight sibling passes derive them from the layout
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (also Dim 2: a consolidation #5593 did not reach)
- **Location**: `crates/renderer/src/vulkan/groundcover/construct.rs:395-415` (`17 * frames`, `max_sets(3 * frames)`; layouts at `:325-363`), `crates/renderer/src/vulkan/groundcover_models.rs:272-285,421-434` (layout is `(0..12)`, pool is `11 * frames` + `frames` TLAS), `crates/renderer/src/vulkan/groundcover_bench.rs:690-712` (`4 * frames + 2 * frames`, `3 * frames`, `3 * frames`, `max_sets(2 * frames)`)
- **Status**: NEW
- **Age**: `0f9982177` (2026-10-01), `aabd99a057` (2026-09-24), `40b5c5b6a` (2026-09-06)
- **Effort**: small
- **Description**: `DescriptorPoolBuilder::from_layout_bindings(&bindings, sets)` (`vulkan/descriptors.rs:643`) makes "layout is the single source of truth" for pool sizing; ssao, exposure_meter, sky_cube, presentation, caustic, skin_compute, taa and compute use it. The ground-cover passes still carry a comment-arithmetic count ("7 scatter + 7 draw + 3 interaction storage bindings per frame-in-flight, plus the scatter's TLAS"). Counts are correct today (checked: scatter 7 storage + TLAS, interaction 3, draw 7; models 12 bindings, binding 3 the TLAS), so this is lockstep bait, not a live bug: adding a binding to a layout without touching the pool exhausts it at `allocate_descriptor_sets`. `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` (`descriptors.rs:931`) polices layout/write/macro spellings but not pool sizing.
- **Evidence**: `.descriptor_count(17 * frames)` beside `set_layout_contracts()` which already returns the three binding lists.
- **Impact**: a future ground-cover binding addition fails at runtime only on a device/driver that enforces pool limits (validation layers catch it, release builds may not).
- **Related**: #5593 (closed 2026-10-14, helper consolidation), #5113 class.
- **Suggested Fix**: build each pool with `DescriptorPoolBuilder::from_layout_bindings(&contract.bindings, ...)` (chain `add_layout_bindings` for the three scatter/interaction/draw layouts); extend the shared-helper test to flag `descriptor_count(<n> * frames)`.

#### TD7-2026-10-15-04: `PROJ` `DATA` is decoded by absolute offsets paired with hand-typed length gates
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (ESM sub-record sizes)
- **Location**: `crates/plugin/src/esm/records/misc/effects.rs:182-211` (`len() >= 12`, `>= 40` + `[36..]`, `>= 56` + `[52..]`, `>= 80` + `[72..]` / `[76..]`)
- **Status**: NEW
- **Age**: `9e6c44cea` (2026-10-14, "Weapon.Fire launches a projectile")
- **Effort**: trivial
- **Description**: every other record in `crates/plugin/src/esm/records/` walks a `SubReader` sequentially; this one is the only production site (4 hits repo-wide) that re-slices `SubReader::new(&sub.data[N..])` and guards each with `len() >= N+4`. The offset and the gate must be edited together; the comment at `:188-189` documents "16..36 are lights, tracer chance..." instead of skipping them with the reader's own `skip_or_eof(16)`.
- **Evidence**: `if sub.data.len() >= 40 { let mut r = SubReader::new(&sub.data[36..]); ... }`.
- **Impact**: a game-variant tail change (the code already splits Skyrim+ vs FO3/FNV at offset 72) needs two coordinated literal edits; a mismatch silently returns a default instead of an error.
- **Related**: `crates/plugin/src/esm/sub_reader.rs:111` (`skip_or_eof`).
- **Suggested Fix**: one `SubReader` pass with `skip_or_eof` between the named fields (flags, gravity, speed, range, [skip lights/tracer/proximity/timer], explosion, [skip], impact force, [skip], collision radius, lifetime), each `*_or_default()` covering a short tail.

#### TD7-2026-10-15-05: `hkx` clip and skeleton decoders gate hostile-input counts on five bare `4096`s and `degree > 8` while `behavior.rs` names the same class
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/hkx/src/animation.rs:332` (bones), `:414` (transforms), `:415` (floats), `:419` (blocks), `:981` (spline control points; `degree > 8` on the same line); named counterparts `crates/hkx/src/behavior.rs:25-29,60` (`MAX_EVENTS`, `MAX_STATES`, `MAX_BONES = 1024`, ...) and the named `MAX_TRANSFORM_SAMPLES` / `MAX_SAMPLES` / `MAX_TRACKS` in the same file
- **Status**: NEW
- **Age**: `02c24e4f1` (2026-08-01)
- **Effort**: trivial
- **Description**: the same file already names the census-derived sample ceilings (#5316, #5006: "set from the census at 8x the measured maximum") but leaves the plausibility ceilings for four different quantities as one anonymous `4096`. They disagree with the behavior-graph reader: a skeleton may declare up to 4096 bones in `animation.rs` while a bone-weights array in `behavior.rs:1063` is rejected above `MAX_BONES = 1024`, so a skeleton the clip reader accepts can be unusable by the graph reader with only a generic error. The values carry no provenance (neither a vanilla maximum nor a format limit).
- **Evidence**: `|| bone_count > 4096`, `|| transform_count > 4096`, `|| float_count > 4096`, `|| num_blocks > 4096`, `control_count > 4096`.
- **Impact**: low; the gates are correct as hostile-input backstops. The cost is that a future Starfield/FO76 rig over a limit gets an unexplained `InvalidData`, and the census tests (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) cannot cite the constants they measure against.
- **Related**: #5316, #5006, #3011.
- **Suggested Fix**: `MAX_SKELETON_BONES`, `MAX_TRANSFORM_TRACKS`, `MAX_FLOAT_TRACKS`, `MAX_SPLINE_BLOCKS`, `MAX_SPLINE_CONTROL_POINTS` with the measured vanilla maximum in each doc comment; reconcile `MAX_BONES` in `behavior.rs` with the skeleton ceiling.

#### TD7-2026-10-15-06: The material "four decal slots" count is an independent literal in six places and two shader path-tracing ceilings sit outside the budget table
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (shader provenance)
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl:441,516`, `crates/renderer/shaders/triangle.frag:381` (`decalIndex < 4`, beside `materialDecals[4]` built from `decalMap0Index..decalMap3Index`), `crates/nif/src/import/types.rs:396` (`decals: [T; 4]`), `crates/nif/src/blocks/properties.rs:388` (`const MAX_DECAL_SLOTS: u32 = 4`); shader-local `const int MAX_GLASS_INTERFACES = 4` (`shaders/include/shadow_transport.glsl:99`) and `const int MAX_DIFFUSE_BOUNCES = 2` (`shaders/triangle.frag:4347`)
- **Status**: NEW
- **Age**: decals `31ebdd7ca` / `1d94eb246` / `1c8b39644` (older than 2026-08); `MAX_GLASS_INTERFACES` `1fb79038a`; `MAX_DIFFUSE_BOUNCES` `6c56e3115`
- **Effort**: small
- **Description**: `shader_constants_data.rs` holds `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_DIRECT_SHADOW_SAMPLES` and `GLASS_RAY_BUDGET` for the same reason (#3879: a hand-typed loop bound clamps a CPU-side tier increase to a silent no-op). The decal-slot count has four named struct fields on the GPU side and no constant, and the two path-tracing ceilings above are named but shader-local. No misbehaviour found: the decal count is pinned by the four-field `GpuMaterial` layout, and no CPU-side value is clamped by the two ceilings. One doc discrepancy to settle when it is next touched: the glass telemetry comment at `shader_constants_data.rs:1134-1138` says the interface allowance grows "from 2 to 8" per tier, while the shadow-transport loop is fixed at 4.
- **Evidence**: `for (int decalIndex = 0; decalIndex < 4; ++decalIndex)` in three shader files; `grep -n 'DECAL' crates/renderer/src/shader_constants_data.rs` finds only `RENDER_LAYER_DECAL`.
- **Impact**: none today; the same lockstep class #3879/#3880 closed, one level down.
- **Related**: #3879, #3880, `feedback_shader_struct_sync`.
- **Suggested Fix**: `MATERIAL_DECAL_SLOTS` (Rust `MaterialTextureSet::decals.len()`-derived) emitted through the generated header and used by the three GLSL loops; move the two path ceilings into the table or add a comment tying them to the budget tiers.

#### TD8-2026-10-15-01: The blocking clippy gate is red again (12 lints in 2 crates) and leaves `byroredux-renderer` (lib) and the `byroredux` bin unlinted; the bin carries 11 hidden lints
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: renderer build script: `crates/renderer/src/shader_constants_data.rs:683` (`GROUNDCOVER_MODEL_MAX_RECORDS % 32 == 0`, `manual_is_multiple_of`). `byroredux-scripting` lib (11): `fragment/package.rs:87` (`multiple_bound_locations`, `E: Copy` + `where E: Clone`), `animation_graph.rs:721` (`nonminimal_bool`), `animation_modifiers.rs:125` (`only_used_in_recursion`, `events`), `animation_modifiers.rs:174` (`collapsible_match`), `encounter_level.rs:24` (`doc_lazy_continuation`), `fragment/reference.rs:657` (`nonminimal_bool`), `fragment/systems.rs:13` (`type_complexity`), `fragment/vm.rs:2811` + `:2821` (`unnecessary_map_or`), `fragment/vm.rs:5270` (`collapsible_if`), `obscript_quests.rs:200` (`drop_non_drop`, `drop(host)`). Hidden by the red gate, in the `byroredux` bin (11, seen only with `-D` removed): `app_frame.rs:955`, `cell_loader/graph_nodes.rs:224`, `cell_loader/spawn.rs:1468` (three `too_many_arguments` 8/7), `cell_loader/graph_nodes.rs:228` (`only_used_in_recursion`, `_needed`), `asset_provider/object_graph.rs:140` + `asset_provider/script.rs:192` (`type_complexity`), `cell_loader/terrain.rs:1124` (`redundant_locals`), `components/projectile_muzzle.rs:9-11` (3x `doc_lazy_continuation`), `systems/graph_locomotion.rs:145` (`map_clone`)
- **Status**: NEW (the 10-12 lints, #5621/#5744, are all cleared; this is a fresh wave — same class as closed #5621, not a re-open of the same sites)
- **Age**: every site is from the last 48 h (blame): scripting lints from `1a12cef0e`, `6e6b1da5d`, `ef0bc42fb`, `9c8e0161a`, `d5c59fcb3`, `f67d82b3c` (10-13) and `49529db7c`, `e2a234c6e` (10-14); renderer lint `c14dd2b39` (10-14); bin lints `aabd99a05`, `6db3805a5`, `caa81f212`, `f6613fd6a`, `017968468`, `592f31b87`, `f6a64fe20`, `746ba8dfd` (10-13/14)
- **Effort**: small
- **Description**: `cargo clippy --workspace --keep-going -- -D warnings` fails `byroredux-renderer` (build script — the renderer lib is then skipped) and `byroredux-scripting` (lib, 11 errors). `byroredux` depends on both, so the highest-churn crate (the bin) is never linted by CI. The renderer's separate `undocumented_unsafe_blocks` re-lint step (`-p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`) is unaffected, so the #4595 guard is still armed. 12 of 12 gate errors and 11 of 11 hidden bin lints were introduced after the 10-12 audit; the previous wave was cleared in the same 72 h window, so this is a re-accrual pattern, not a stuck backlog.
- **Evidence**: `/tmp/audit/tech-debt/d8_clippy_gate.txt` (gate), `d8_clippy_nod.txt` (no `-D`); reverse-dependency check via `cargo metadata`: only `byroredux-renderer`, `byroredux-scripting`, `byroredux` are in the unlinted closure.
- **Impact**: CI `Test + Check + Clippy` is red on every push until fixed; the bin stays lint-blind meanwhile.
- **Related**: #5621 (closed, same class), #5622 (open), TD8-2026-10-15-02, TD8-2026-10-12-01.
- **Suggested Fix**: apply the rustc suggestions (4 are machine-applicable: the two `map_or` -> `is_none_or`, the `collapsible_if`, `is_multiple_of`); `drop(host)` -> let the borrow end by scope; `populate_fragments` fold `E: Copy` into the `where` clause; give `graph_nodes.rs:224` / `spawn.rs:1468` / `app_frame.rs:955` a parameter struct or a reasoned `#[allow]`.

#### TD8-2026-10-15-02: `--all-targets` (non-blocking CI step) carries 26 test/example-target lints; the 10-12 deny-level HKX error is gone
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy, test targets)
- **Location**: `crates/hkx/examples/{annotation_census_ba2,behavior_census,behavior_census_ba2,class_census_ba2,graph_dump,sound_events,clip_info}.rs` (10, mostly `needless_borrow` at `path`), `crates/hkx/src/behavior.rs:1740` (`vec_init_then_push`); `crates/scripting/examples/alias_script_coverage.rs:95,104,108` and `crates/plugin/examples/qust_alias_script_census.rs:88` (`unnecessary_sort_by`); `crates/scripting/src/fragment/vm/tests.rs:905,906,5018`, `crates/scripting/src/condition.rs:2056` (`clone_on_copy`), `crates/scripting/src/animation_graph.rs:1838` (`needless_borrow`); `crates/renderer/src/vulkan/groundcover.rs:1471-1473` (3x `assertions_on_constants`), `crates/ui/src/prepare.rs:695` (`assertions_on_constants`), `crates/plugin/src/esm/records/soun.rs:185` (`needless_borrows_for_generic_args`), `byroredux/src/systems/graph_locomotion.rs:344,359` (`manual_range_contains`)
- **Status**: NEW bucket (10-12 had 11 + 1 deny; `TD8-2026-10-12-02` / #5744 closed and verified: `KNOWN_VANILLA_REJECTS` assert no longer trips `absurd_extreme_comparisons`)
- **Age**: 3 days or less for all sampled (hkx example census set `3bb6806b5`, groundcover constants `af8323881`, ui `d601b45ab`)
- **Effort**: small
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) finishes; 49 diagnostics total, 23 of them duplicates of the production lints in TD8-01, the remaining 26 only exist in test/example targets. `assertions_on_constants` x4 (renderer + ui) are tests asserting compile-time constants — they should be `const _: () = assert!(…)`.
- **Evidence**: `/tmp/audit/tech-debt/d8_clippy_all.txt`, `all_lints.txt`.
- **Impact**: CI step is `continue-on-error`, so it is informational; no deny-level error at HEAD.
- **Related**: #5621 (closed), TD8-2026-10-15-01.
- **Suggested Fix**: `cargo clippy --fix --workspace --all-targets` handles most; convert the four constant assertions to `const` asserts.

#### TD8-2026-10-15-03: The CI `cargo machete` gate cannot see `[dev-dependencies]`; four unused dev-deps sit behind it
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (unused-dependency gate)
- **Location**: `crates/papyrus/Cargo.toml:11` (`env_logger`), `crates/bgsm/Cargo.toml:12` (`env_logger`), `crates/sfmaterial/Cargo.toml:13` (`env_logger`), `crates/nif/Cargo.toml:19-23` (`byroredux-facegen`), `.github/workflows/ci.yml:80` (`cargo machete`)
- **Status**: NEW
- **Age**: `byroredux-facegen` dev-dep from `4b6108636` (#2599) — its only use, the `facegen_half_to_f32_copy_matches_canonical_bit_for_bit` pin, was removed with the facegen half decode (#4653; `crates/nif/src/import/mesh/decode_half_float_tests.rs:50-53` is now a comment), but the Cargo.toml comment still says "This dep exists solely so `decode_half_float_tests` can pin the two impls". `env_logger` dev-deps date from `edb0525e9` (#490, bgsm) and `c99d78035` (#762, sfmaterial); papyrus likewise has no use.
- **Effort**: trivial
- **Description**: plain `cargo machete` (what CI runs) reports "didn't find any unused dependencies". `cargo machete --with-metadata` reports `env_logger` x3 and `byroredux-facegen` (and a false positive on `cxx-build`, used by `crates/cxx-bridge/build.rs`). Verified with a scratch workspace: plain machete flags an unused `[dependencies]` entry written as `{ workspace = true }` but does NOT flag the same entry under `[dev-dependencies]`; `--with-metadata` flags both. `grep -rn env_logger crates/{papyrus,bgsm,sfmaterial}` finds no use outside Cargo.toml; `grep -rn byroredux_facegen crates/nif` finds nothing. Re-running `cargo machete` (plain) is exactly the blind spot the job comment (`ci.yml:71-74`) claims not to have ("does not produce false negatives for the plain declared-but-never-referenced case").
- **Evidence**: `cargo machete --with-metadata` output; the greps above.
- **Impact**: dev-deps compile into every test build (facegen into the nif tests) and the gate cannot catch the next one. The workspace.dependencies table itself is clean (every entry has a member).
- **Related**: #3890 (introduced the gate), closed #2426-#2431 sweep.
- **Suggested Fix**: delete the four lines (and the stale comment block in nif/Cargo.toml); switch CI to `cargo machete --with-metadata` and add `[package.metadata.cargo-machete] ignored = ["cxx-build"]` to `crates/cxx-bridge/Cargo.toml` (note `--with-metadata` may touch Cargo.lock — pair it with `--locked` or a `git diff --exit-code Cargo.lock`).

#### TD8-2026-10-15-04: #5745's zero-reference list grew by four fns and three re-exported dead wrappers; #5762's premise is stale
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft
- **Location**: new zero-reference `pub` fns: `crates/debug-ui/src/lib.rs:303` (`message_box_menu_visible`) and `:313` (`barter_menu_visible`) — the sibling `inventory_menu_visible`/`dialogue_menu_visible`/`game_menu_visible` are used, these two are not; `crates/scripting/src/dialogue.rs:154` (`QuestGates::conditions_of`, superseded by `shared_conditions_of` at `:160`, which is what `dialogue.rs:427` calls); `crates/scripting/src/fragment/alias_scripts.rs:257` (`started_quests`). Dead wrappers kept alive only by a re-export line (a name-count scan that does not strip `use` lines misses them): `crates/scripting/src/fragment/populate.rs:458` (`populate_scene_fragments_from_pex`, re-exported `scripting/src/lib.rs:134`; every caller uses `_detailed` / `populate_owned_scene_fragments_from_pex_detailed_with_providers`), `crates/scripting/src/fragment/package.rs:36` (`PackageFragmentSet` alias, re-exported `lib.rs:152`), `crates/plugin/src/esm/records/misc/water.rs:1571` (`watr_to_params`, `pub fn watr_to_params(record) -> WaterParams { record.params }`, re-exported `misc.rs:101`, zero callers since M38 — `2ee1c68df`, 2026-05-12)
- **Status**: Existing: #5745 (open; 15 items unchanged since 10-12 — re-verified, all still count 1 across `git grep -w '*.rs'`) — extension, not a new issue. Also #5762 (open): `ActorValues::set_permanent` is no longer dead — `crates/scripting/src/condition.rs:1419` calls `avs.set_permanent(avif, 0.0)` since `44e4d948d` (M47.2 GetBaseValue, 2026-10-13); #5762 / the `set_permanent` entry in #5745 should be closed or re-scoped.
- **Age**: 10-12 to 10-13 for the four fns; `watr_to_params` 2026-05-12
- **Effort**: trivial
- **Description**: the 10-12 scan (identifier count == 1) is unchanged, 4 more items have joined it, and a second class exists that the count misses: `pub use`d wrappers whose only other occurrence is the re-export. Re-scanning with `use` statements stripped adds the three wrappers above (plus vocabulary consts `GRAS_FLAG_VERTEX_LIGHTING`, `SCENE_*`, `SLOT_OPACITY/TRANSMISSIVE`, `DEFAULT_COVER_AFFINITY`, deliberately not flagged: decoded-field vocabulary / documented aliases). `watr_to_params` is the clear case: "adapter from a parsed `WatrRecord` onto a `WaterParams` view" that returns a field.
- **Evidence**: `git grep -nw <name> -- '*.rs'` for each; scan scripts in `/tmp/audit/tech-debt/d8_scan*.py` (outputs `d8_scan4_out.txt`).
- **Impact**: dead surface; the two superseded wrappers (`conditions_of`, `populate_scene_fragments_from_pex`) suggest the "plain vs shared/detailed" pair split leaves the weaker one behind each time.
- **Related**: #5745, #5762, TD8-2026-10-12-03.
- **Suggested Fix**: append these 7 to #5745's delete list (drop the three re-export lines with them); leave the debug-ui accessors only if a menu-state test is planned.

#### TD8-2026-10-15-05: The grass-LOD rewrite orphaned the frame-serial plumbing: `GC_FRAME_SERIAL` has no reader, but host packing, a push-constant bit-field and a test still maintain it
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (shader + host)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:106-111` (`#define GC_FRAME_SERIAL`), `crates/renderer/src/vulkan/groundcover/frame.rs:122-137` (packs `serial << 2 | tier`), `crates/renderer/src/vulkan/groundcover.rs:359-360` (`GROUNDCOVER_FRAME_SERIAL_BITS/_MASK`), `:770-830` (the test that pins the define's text and the f32 round-trip); also `crates/renderer/shaders/include/groundcover_density.glsl:89` (`byroGcWorley`, a wrapper over the used `byroGcWorleyCell`)
- **Status**: NEW
- **Age**: `af83238810` (2026-10-14, "Grass LOD: per-chunk 3/2/1-segment tiers") — its diff deletes `vFrameSerial = GC_FRAME_SERIAL;` and the `blueNoiseRankAt` use; `byroGcWorley` from `39c9e39f0` (10-14)
- **Effort**: small
- **Description**: a whole-tree identifier count over the shader directory finds exactly two symbols with a single occurrence: `GC_FRAME_SERIAL` (defined, never read) and `byroGcWorley`. The first matters: the host still masks `floor(time*60)` to 22 bits, multiplies by the tier stride, and a test asserts the shader `#define` text byte-for-byte (`groundcover.rs:785-792`), while the frame.rs comment still says the serial "makes the blue-noise transition repeat under a replayed simulation clock" — a consumer that no longer exists. #4498's f32-exactness fix stays correct for the tier bits, but the serial half is dead weight in the push-constant word.
- **Evidence**: `git show af8323881 -- crates/renderer/shaders | grep FRAME_SERIAL` (the `-    vFrameSerial = GC_FRAME_SERIAL;` hunk); `git grep -nw GC_FRAME_SERIAL` -> define + the Rust test string only.
- **Impact**: low runtime cost; the risk is a future reader trusting the comment and a test that keeps a dead shader contract alive. Removing the define does not change SPIR-V (unused macro), but anything editing `groundcover_blade.vert` must re-run `glslangValidator` per *feedback_spv_rebuild_staleness* and the shader-struct lockstep rules — do not touch the tier packing.
- **Related**: #4498 (closed, serial exactness), M-grass LOD work.
- **Suggested Fix**: drop the define, `GROUNDCOVER_FRAME_SERIAL_*`, the serial term in `frame.rs` and its test assertions (keep tier encoding + its f32-exact round-trip); delete `byroGcWorley` (or call it from `byroGcWorleyCell`'s only consumer if intended).

#### TD8-2026-10-15-06: `import_nif`'s doc says it is "used by cell loader"; the flat-import family has zero production callers
- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft
- **Location**: `crates/nif/src/import/mod.rs:409-416` (`import_nif`, doc "Backward-compatible flat import (used by cell loader …)"), `:420` (`import_nif_with_resolver`), `:584` (`import_nif_with_collision`, no resolver), `import_nif_impl` `:428`
- **Status**: NEW
- **Age**: `import_nif` predates the scene-graph importer; verified callers are `byroredux/examples/{dump_nif,msn_basis_probe}.rs`, `crates/nif/tests/translation_completeness.rs`, `byroredux/src/fog.rs` (test module, `:2450+`) and `import/tests/units.rs`, `cell_loader/finish_partial_tests.rs`
- **Effort**: trivial
- **Description**: production code (cell loader, streaming) calls `import_nif_scene*` and `import_nif_with_collision_and_resolver` (`cell_loader/references/import.rs:88`, `streaming/pre_parse.rs:341`); `import_nif`, `import_nif_with_resolver` and `import_nif_with_collision` are reached only from examples, corpus tests and `#[cfg(test)]` code. The doc claim "used by cell loader where hierarchy is unnecessary" is false, and `import_nif_texture_effects` (`:551`) is documented as a deliberately dead extractor (`docs/engine/nifal.md:839`) so it is correctly NOT flagged.
- **Evidence**: `git grep -nw import_nif -- '*.rs'` filtered to non-test, non-example paths -> nothing.
- **Impact**: misleads a reader into believing the flat path is a live production route (it feeds none of the NIFAL-translated data the scene importer does).
- **Related**: TD8-2026-10-12-03 (same "public API reachable only by tests" class).
- **Suggested Fix**: fix the doc to "harness/example entry point; production uses `import_nif_scene*`"; fold `import_nif_with_collision` into the `_and_resolver` form with `None`.

#### TD9-2026-10-15-02: The committed `combustion_lab_60f` golden last failed by a wide margin on this machine and was never regenerated; the golden CI job is manual-dispatch only
- **Severity**: LOW (promote to MEDIUM if a run on the RT runner confirms red at HEAD)
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/tests/golden/combustion_lab_60f.png` (committed 2026-09-21, `7cdc0c7ff`), `byroredux/tests/golden/cube_demo_60f.png` (2026-09-15, `b23c92e2f`), `byroredux/tests/golden_frames.rs:102-107` (`PIXEL_TOLERANCE = 8`, `MAX_DIFF_PCT = 1.0`, `MAX_CHANNEL_DELTA = 32`), `.github/workflows/rt-correctness.yml:3-14,68-94`
- **Status**: NEW (#5625 closed by adding the `golden` job; #4227 closed by a regen)
- **Age**: baselines 2026-09-21 / 2026-09-15; 103 commits touched `crates/renderer/shaders` since 2026-09-21
- **Effort**: small
- **Description**: `byroredux/tests/golden/combustion_lab_60f.png.actual.png` (git-ignored, mtime 2026-10-09 17:12) is the artefact `save_actual_next_to_baseline` writes only when `compare_or_fail` rejects a frame. Compared to the committed baseline (both 1280x720): 98.9 % of pixels differ by more than 8/255 and the largest channel delta is 200/255, against thresholds of 1 % and 32. No regeneration followed (the PNG is unchanged since 09-21; 48 renderer commits landed after 10-09). Whether the failure was an intended visual change awaiting a regen, a different GPU, or a regression is not recoverable from the tree. The staleness gate `committed_baseline_matches_the_current_invocation` only checks that the `.capture` manifest names the current CLI flags, not that the pixels are current. The `golden` job exists (#5625) but `rt-correctness.yml` triggers on `workflow_dispatch` only and needs the self-hosted `byroredux-rt` runner, so nothing runs it on a push or a schedule.
- **Evidence**: `python3 -I` with PIL/numpy over the two PNGs: `pct>8: 98.89 max 200`. `ls -la byroredux/tests/golden`.
- **Impact**: the only pixel-level render regression guard is either red and ignored or green by luck; `audit-runtime` lists both goldens as a gate.
- **Related**: #5625, #4227, #3849, `audit-runtime` Dim (gate matrix).
- **Suggested Fix**: on the RT runner run `cargo test --release -p byroredux --test golden_frames -- --ignored`; regenerate with `BYROREDUX_REGEN_GOLDEN=1` after a human look at `.actual.png` if the change is intended, else bisect the 103 shader commits. Add a `schedule:` trigger to the `golden` job (nightly, like `real-data-gates.yml`).

#### TD9-2026-10-15-03: Three scripting real-data test files skip green when game data is missing, ignoring `BYROREDUX_REQUIRE_GAME_DATA`
- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/scripting/tests/perk_entry_points_real_data.rs:45-48,103-106,129-132` (`SKIP: FalloutNV.esm not found`), `crates/scripting/tests/location_aliases_real_data.rs:38-41,100-103` (`skipping: game data unavailable`), `crates/scripting/tests/pex_recognize_e2e.rs:40-42,85-87,127-129,152-154` (`SKIP: ... no game data?`)
- **Status**: NEW (sibling of open #5784, which is the same shape in `npc_spawn/tests.rs`)
- **Age**: 6 of the 9 `#[ignore]` tests in these files landed in this window (`perk_entry_points_real_data` x3, `location_aliases_real_data` x2, `pex_recognize_e2e` x1)
- **Effort**: trivial
- **Description**: #3850 made an absent install fail instead of skip when `BYROREDUX_REQUIRE_GAME_DATA` is set (helpers in `crates/plugin/src/esm/test_paths.rs`, `crates/scripting/src/obscript_quests.rs`, `byroredux/src/ragdoll_pose_tests.rs:254`, `cell_loader/tree_lod_tests.rs:263`). These three files consult only `test_paths::*_ENV`/`*_DEFAULT` (or an archive lookup) and `return` on `None`, so an `--ignored` run in a strict lane reports `ok` without having asserted anything.
- **Evidence**: `let Some(index) = fnv_index() else { eprintln!("SKIP: ..."); return; };`
- **Impact**: moot while the tests have no lane (TD9-01), and a false green the moment one is added.
- **Related**: #5784, #3850, TD9-2026-10-15-01.
- **Suggested Fix**: one shared `require_or_skip(path, what)` in `crates/scripting/tests/common` (panic when `BYROREDUX_REQUIRE_GAME_DATA` is set) and call it from the three helpers.

#### TD9-2026-10-15-04: 44 new examples (2 680 LOC, +35 % in three days) accreted under crates whose `cargo test` links every one; the hygiene guard only sees the `tmp_` prefix and the "throwaway" marker
- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/hkx/examples/` (17 new, 13-164 LOC each), `crates/plugin/examples/` (16 new, 20-210 LOC), `crates/scripting/examples/` (6 new + `common/archives.rs`), `crates/nif/examples/skeleton_tree_dump.rs`, `crates/pex/examples/pex_dump_fn.rs`; guard `byroredux/src/workspace_hygiene_tests.rs:241-318`
- **Status**: NEW (same class as closed #3746: 73 `_tmp_*` examples, 6 978 LOC, linked twice per CI run)
- **Age**: 2026-10-13 to 10-14 (`git diff --diff-filter=A 5d31fd637 HEAD -- '*/examples/*.rs'`)
- **Effort**: small
- **Description**: 168 tracked examples, 22 888 LOC. None of the new ones is disposable by the guard's definition (each has a module doc and a real purpose: censuses, graph dumps, per-record probes), and none carries a `#[test]` or an assertion (44 of 44 score 0 on `grep -c '#\[test\]\|assert'`), so no coverage is lost by consolidating. Examples are compiled by `cargo test --workspace` (and linked against their crate's full dependency set), which was the driver for #3746. Many are 13-35 line variations on "open an archive, print one thing" (`hkx/examples/{class_census,start_clip_ba2,clip_info,skeleton_bones,skeletons_dump,dump_objects}.rs`, `plugin/examples/{base_model,cell_refs_by_base,sound_names,script_cells}.rs`).
- **Evidence**: `git diff --name-only --diff-filter=A 5d31fd637 HEAD -- '*/examples/*.rs' | xargs wc -l` = 2 680.
- **Impact**: CI wall time and link memory, not correctness; plus the next "which census do I run" question.
- **Related**: #3746, `no_tmp_scratch_examples_are_committed`.
- **Suggested Fix**: fold the one-liners per crate into a single `examples/inspect.rs` with subcommands (the hkx archive plumbing already repeats in `*_ba2.rs`); keep the multi-page censuses. Extend the guard with an example-count/LOC ceiling per crate if the pattern recurs.

#### TD9-2026-10-15-05: The ignored VM timing census has 99 lines of `eprintln!` and no assertion, and two more environment-dependent bench tests share the shape
- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/scripting/src/fragment/vm/tests.rs:4358-4457` (`vm_cost_idle_burst_polling_runaway`, `#[ignore = "timing census; run with --release --nocapture"]`); `byroredux/src/render/draw_sort_key_tests.rs` (2 x `manual timing bench, environment-dependent`)
- **Status**: NEW
- **Age**: `vm_cost_idle_burst_polling_runaway` 2026-10-13
- **Effort**: trivial
- **Description**: the only assertion-free `#[test]` in the tree that is not a helper-wrapper false positive (a whole-repo scan of 10 507 tests for bodies lacking any assert/panic/expect/`?` found 24; 23 are false positives through helper functions such as `approx(..)` or `.expect(..)` source scans). It exists to print µs-per-call tables for the VM systems, i.e. a benchmark in test clothing, and it is the only place the VM's per-frame cost is measured. Nothing fails if a system regresses 100x.
- **Evidence**: `eprintln!("\nA. idle frame (no events), µs per call of each VM system");` ... no `assert`.
- **Impact**: none today; the PERF gate the table suggests does not exist.
- **Related**: `crates/scripting/src/fragment/vm/tests.rs:4601` (`budgeted(&mut world, 100_000)`).
- **Suggested Fix**: either move it to a `crates/scripting/examples/vm_cost.rs` (an example, not an `#[ignore]` test) or add one generous ceiling assertion (e.g. idle-frame cost at 20 000 refs under a named budget).


## Per-dimension notes (no finding)

- **Dim 1:** `#5586` (`mq101_conformance.rs::run`, 1454 lines) unchanged; `build_pipelines_and_finish` stays dismissed (every phase-3 value feeds the terminal struct literal). `shader_constants_data.rs` is a data table: do not split, generate (`TD1-10`).
- **Dim 3:** `ecs.md` stage list, `memory-budget.md` ground-cover figures, `shader-pipeline.md` binding tables, `game-compatibility.md` vs the ROADMAP matrix and the `docs/engine/index.md` index were sampled and are current.
- **Dim 4:** validator 0 STALE; `groups.txt` covers all 34 `audit-*` dirs exactly once; ~100 `/audit-x Dim N` cross-references valid; the 15 CRITICAL/HIGH findings from 2026-07-13..16 all trace to closed issues. Reports before 2026-06-07 were not re-verified.
- **Dim 5/6:** zero `unimplemented!`/`todo!()` in production; every console command and scheduler system checked is registered; `ScriptTimer`/`TimerExpired` has no producer or consumer but #5608 closed docs-only and the docs now say so. Production-unreachable components are the honestly documented "forward-latent" rows in `registry_completeness_tests.rs` (#5609, #5610 open).
- **Dim 7:** NIF version literals are guarded by `version_literal_tests.rs`; every shader `#define` outside the generated header is a guard, a push-constant alias or a named/cited constant.
- **Dim 8:** no `#[deprecated]` items, `// removed:` breadcrumbs or compatibility re-exports; every workspace dependency has a consuming member; every feature has a cfg consumer; `#[allow(clippy::…)]` without a reason comment is 32 (28 tracked in #5622).
- **Dim 9:** the `workspace_hygiene_tests` guards are not `#[ignore]`d; the core `--no-default-features` lane only compiles, which is proportionate.
