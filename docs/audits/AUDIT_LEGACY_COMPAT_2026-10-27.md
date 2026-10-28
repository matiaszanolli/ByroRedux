**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD `4a3e89d56`) · **Audited**: Dim 2 (the object-sequence spawn path rebuilt by `4d36470b5` #6253 and `0c73fce69` #6254, plus the `92173d61f` #4128 PlayGroup flag-0 slice), Dim 1 (an added-line scan of all 395 in-range commits) · **Unchanged since baseline (skimmed)**: Dim 1's listed paths (one doc-rename commit, `e52d41c8c`), Dim 3 (the Pattern A grep is clean; the new game branches are single named decisions or per-era-tested record splits)

# Legacy Compatibility Audit — 2026-10-27

## Method

This run is scoped to changes since the 2026-10-21 baseline: 395 commits, `4a3e89d56..fa6be4a6b`.

- **Legacy reference.** The Gamebryo 2.3 drive is not mounted. I used three substitutes:
  - `/mnt/data/src/reference/gamebryo-v32/Documentation/Reference/NiAnimation/NiControllerManager_overview.htm`. Sequences are added to the manager "for future playback", and each `NiControllerSequence` is "created upon activation (i.e. playback)".
  - The local GECK and CS wikis: `Door.wiki`, `PlayGroup.wiki` and `AnimGroups.wiki`.
  - OpenMW's `CharacterController`, where a non-actor starts in `CharState_Idle`.
- **Raw-data probe.** I wrote one read-only throwaway crate in the session scratchpad (`lcprobe`, with the repo `Cargo.lock` copied in). It built into the repo `target/` through `CARGO_TARGET_DIR=<repo>/target`, so no private target dir was used.
  - It runs `byroredux_nif::anim::import_kf` over every NIF in the vanilla meshes archives and prints each file's sequences in import order, with cycle type, duration and channel counts.
  - Archives covered: Oblivion, FO3, FNV and Skyrim SE `Skyrim - Meshes0/1.bsa`.
  - Outputs: `/tmp/audit/legacy-compat/probe_{obl,fo3,fnv,sse}_seq.txt` and `probe_fo3_seq_ch.txt`.
- **Tests run** (repo `target/`):
  - `cargo test -p byroredux-core --lib -- math::coord components::camera`: 47 passed.
  - `cargo test -p byroredux-nif --lib -- strip rotation import::coord anim`: 160 passed.
- **Dedup sources:**
  - `/tmp/audit/open_issues_snapshot.txt` (84 open).
  - `gh` closed-issue searches: "door open sequence spawn", "first sequence", "Open sequence", "idle sequence spawn", "doors spawn open", "Random Anim Start".
  - `docs/audits/` greps.
  - `/tmp/audit/SUITE_RULES.md` § 6, re-read just before finalizing (21 audits listed). Nothing overlaps.

No source, skill or issue was modified, and no engine was launched. Scratch notes are in `/tmp/audit/legacy-compat/dim_{1,2,3}.md`.

## Executive Summary

| Severity | NEW | Existing / cross-referenced |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 1 | 0 |

**Headline: placed objects play the wrong sequence at spawn.** The cell loader plays a model's **first** `NiControllerSequence` from spawn. The legacy engines play an object's **idle** at load. Gamebryo's manager activates nothing by itself, and Bethesda starts the idle ("Random Anim Start: … kicks off its idle animation at a random start time").

The first sequence in manager order is usually not an idle:

| Game | Meshes probed | First sequence not an idle | First is `Open` (Clamp) | First is `Forward` (Clamp) |
|---|---:|---:|---:|---:|
| FNV | 803 | 543 | 143 | 156 |
| FO3 | 411 | 270 | 148 | 64 |
| Oblivion | 423 | 314 | 86 | 113 |
| Skyrim SE | 1,137 | 1,033 | 245 | — |

The `Open` meshes are animated doors, vault gear doors, safes, lockers, trunks, fence gates and desks. The `Forward` meshes are activators, harvestable plants and slot machines.

So every such door or container swings open on cell load and stays open. Every `Forward`-first activator plays its activation motion once and holds the end pose.

This has been the behaviour since `fb74b409b` (#3602, 2026-08-30). #6253 and #6254 kept it unchanged while rebuilding the path around it. The 10-21 audit read this code for its envelope and sink gaps and missed which sequence it selects.

**Coordinates are still correct.**
- All five 10-21 findings are closed with their fixes in tree:
  - #6321 (`e9eab676e`)
  - #6322 (`8dc0a9cad`)
  - #6323 (`ca43fdf8e`)
  - #6253 (`4d36470b5`)
  - #6254 (`0c73fce69`)
- The new coordinate callers all go through `coord.rs`: ObScript position and angle, the `mesh.info` inverse, tree LOD, `ApplyHavokImpulse`, and the transition wrappers.
- The one remaining finding is a scattered-name axis swap that the #6321 ratchet cannot see (D1-01, LOW).

---

## Dimension 1 — Coordinate + placement fidelity

**Listed-path commits in range:** one, `e52d41c8c`, a docs symbol rename. The guards pass: core `math::coord` + `camera` (47) and nif `strip` / `rotation` / `import::coord` (160).

**Added-line scan** of the 395-commit diff for `(x, z, -y)`, `4096.0`, `from_mat3`, `EXTERIOR_CELL_UNITS`, `zup_to_yup`/`yup_to_zup`, `normalize_quat`, `from_euler`/`EulerRot` and `euler_zup_to_quat_yup`:
- **New conversions route through the canonical helpers:**
  - ObScript `GetPos` / `SetPos` / `Move` (`yup_to_zup_pos` → `zup_to_yup_pos`).
  - `GetAngle` / `SetAngle` (`quat_yup_to_euler_zup` / `euler_zup_to_quat_yup`).
  - `mesh.info` (`quat_yup_to_euler_zup`, the #6322 fix).
  - Tree LOD and `ApplyHavokImpulse` (`zup_to_yup_pos`, #6321).
  - `transition::{position_zup_to_yup, rotation_zup_to_yup_quat}`, which delegate to `zup_to_yup_pos` and the REFR dispatcher.
- **Other patterns:**
  - There is no new `Quat::from_mat3` and no new `normalize_quat` caller.
  - New world→grid floors use `EXTERIOR_CELL_UNITS`, and new `4096.0` literals are tests.
  - Two plugin examples (`shore_probe.rs`, `water_fixture_probe.rs`) declare `const CELL: f32 = 4096.0`. They are probes, not production, so I am not filing them.
  - `destrip` is still single-implementation.

### LC-2026-10-27-D1-01: A scattered-name Z-up→Y-up swap in the exterior apply job is invisible to the #6321 ratchet
- **Severity**: LOW
- **Dimension**: Coordinate + placement fidelity
- **Location**: `byroredux/src/cell_loader/exterior/apply_job.rs:237-244`; the guard is `byroredux/src/workspace_hygiene_tests.rs` (`no_production_vec3_hand_rolls_the_zup_to_yup_swap` / `hand_rolled_zup_swaps`)
- **Status**: NEW. The line dates from `10d320f07` (2026-04-12). The in-window split `1d383cc5a` (#6271) moved it, and it survived the #1044, #1617 and #6321 consolidations.
- **Description**: The bulk loader's initial-camera terrain point builds its Y-up vector by hand instead of calling `zup_to_yup_pos` / `cell_grid_to_world_yup`.
  - The ratchet's matcher only recognises a swap whose three arguments share one prefix: `x/z/y` fields or `[0]/[2]/[1]` indices.
  - `world_x, mid_height + 200.0, -world_y` has three different names, so it passes the scan.
  - A grep over production code for `Vec3::new(<…x…>, <…>, -<…y…>)` finds this as the only survivor.
- **Evidence**:
  ```rust
  let world_x = gx as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
  let world_y = gy as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
  // Z-up → Y-up: (x, height, -y), plus 200 units above ground.
  Vec3::new(world_x, mid_height + 200.0, -world_y)
  ```
- **Impact**: No wrong output today; the result equals `zup_to_yup_pos([world_x, world_y, mid_height + 200.0])`. The risk is the one #1044 and #6321 exist to remove: a future change to the canonical convention would not reach this site, and the guard would not flag it.
- **Related**: #6321 (closed), #1044, #1617.
- **Suggested Fix**: Write it as `Vec3::from(zup_to_yup_pos([world_x, world_y, mid_height + 200.0]))`. Optionally extend `hand_rolled_zup_swaps` to flag any `Vec3::new(a, b, -c)` whose first and third arguments end in `x` and `y`, with an allowlist for real Y-up literals.

---

## Dimension 2 — Legacy subsystem coverage

**In range:**
- `4d36470b5` (#6253): the standalone-controller clip and the first sequence now run on separate players, each with its own envelope.
- `0c73fce69` (#6254): sinks are attached for every named sequence.
- `92173d61f` (#4128): `PlayGroup` flag 0 holds the swap until the cycle boundary. The KFM remainder is documented as unbuilt because no shipped title carries `.kfm`.
- `088821edd` (#6301): satellite walkers go through `node_is_culled`.
- `52e48bd4c` (#6214): EyeEnvmap centres are converted Y-up at import.
- `051179b7b` / `8c5a718e3` (#6241 / #6235): root-motion yaw and layer weighting.

The #6253/#6254 rebuild is sound for what it targets: envelopes are kept per player, every sequence's sinks attach, and ambient controllers survive a `PlayGroup`. The `obscript_anim` lock-order cycle is ECS-2026-10-27-01 and is not re-filed here.

Unchanged and spot-checked:
- The material/property walker: refactors only, `fde8836dd` / `4ad1bbda7`.
- `crates/core/src/string`: no commits.

### LC-2026-10-27-D2-01: Spawn plays a model's first `NiControllerSequence` instead of its idle, so animated doors and containers load open and activators load activated
- **Severity**: HIGH. This is wrong visible content on every cell load, across four games and hundreds of vanilla meshes per game. Doors, lockers, safes and trunks render in their activated pose, and the default is the authored rest pose. Per `_audit-severity.md` (rendering correctness → at least HIGH; a layer-boundary mapping that removes or alters visible content escalates).
- **Dimension**: Legacy subsystem coverage — the animation model (`NiControllerManager` sequence activation)
- **Location**:
  - `byroredux/src/cell_loader/spawn.rs:953-975`: `first_sequence = handles.first().copied()`, then `spawn_placement_player` for it.
  - `byroredux/src/cell_loader/object_sequences.rs:3`: the module doc ("plays the first on spawn").
  - The order comes from `crates/nif/src/anim/entry.rs::import_kf`, which follows `NiControllerManager::sequence_refs`.
- **Status**: NEW. The code dates from `fb74b409b` (#3602, 2026-08-30) and was carried through `4d36470b5` (#6253) and `0c73fce69` (#6254). No open or closed issue covers it, and no audit names it; the 10-21 report D2-01/D2-02 covered envelope and sink loss, not selection.
- **Description**: A placed object's spawn starts whichever sequence the manager lists first. The legacy engines treat that list as a catalog:
  - **Gamebryo.** `NiControllerManager` holds sequences "for future playback"; a sequence runs only when activated (`NiControllerManager_overview.htm`).
  - **Bethesda.** An object's *idle* is what starts at load. The GECK Door/Activator form says "Random Anim Start: … this object kicks off its idle animation at a random start time". `Open` runs "when activated", and `Forward`/`Backward` are `PlayGroup` targets.
  - **OpenMW.** Non-actors start in `CharState_Idle`, which plays the `idle` group.
  
  In vanilla data the first sequence is usually an activation clip with Clamp cycle, so playing it at spawn leaves the object in its post-activation pose.
- **Evidence** (probe, `import_kf` over the vanilla meshes BSAs):
  - **First-sequence counts:**

    | | FNV | FO3 | Oblivion | Skyrim SE |
    |---|---:|---:|---:|---:|
    | Meshes with sequences | 803 | 411 | 423 | 1,137 |
    | First is neither `Idle` nor `SpecialIdle` | 543 | 270 | 314 | 1,033 |
    | First is `Open:Clamp` | 143 | 148 | 86 | 245 |
    | First is `Forward:Clamp` | 156 | 64 | 113 | — |
    | First is not an idle, but an idle appears later | 31 | 3 | 4 | — |

  - **Sample paths:**
    - FO3 `dungeons\vaultruined\doors\vdoorslidingvault106.nif`: `Open:Clamp:1.33` with 2 transform channels.
    - FO3 `dungeons\enclave\accessories\lockerenclave01.nif`: `Open:Clamp:0.33`, 1 transform channel.
    - FO3 `clutter\vendortrunk\vendortrunk01.nif`: `Open:Clamp:0.17`, 2 transform channels.
    - FNV `dungeons\vaultruined\doors\vgeardoorr106.nif`: `Open:Clamp:25.00`.
    - FNV `clutter\clfence\clfencegate01.nif`: `Open`.
    - FNV `trees\barrel_cactus\nv_barrelcactus.nif` and `landscape\plants\nvpinto_fruited.nif`: `Forward` (harvest).
    - Oblivion `architecture\bravil\bravilloaddoorlowerint02.nif`: `Open`.
  - **Code path:**
    - `spawn.rs:955-957` takes `handles.first()` with no name test.
    - `:964-974` attaches sinks and spawns an `AnimationPlayer` for it.
    - `named_sequences_for` keeps `import_kf` order.
    - No system plays `open` / `close` / `forward` by name. `NamedSequences::handle` is called only by `obscript_anim::play_group`.
- **Impact**:
  - **Doors and containers.** Every animated door, gate, locker, safe, trunk and desk in FO3, FNV and Oblivion animates open on cell load and holds open. This covers the vault sliding and gear doors and Oblivion's animated city and load doors. Their collision stays where it was authored, so the visual and the collider disagree.
  - **Activators.** `Forward`-first activators play their activation once and hold its end pose: traps, switches, harvestable plants (shown already harvested), slot machines, and effect meshes like `hchambergroundfog`.
  - **Idles.** Objects whose idle is not first lose their ambient loop.
  - **Skyrim SE.** Graph-driven props also run this player, because the spawn path does not gate on `behavior_graph_file`.
- **Related**: #3602, #6253, #6254, #4128 (all closed); LC-2026-10-21-D2-01/-02. This interacts with `PlayGroup` flag 0: `cycle_in_progress` treats a clamped-finished `Open` as done, so the first scripted `PlayGroup` is not held back by it.
- **Suggested Fix**:
  - Select the spawn sequence by name: `idle`, then `specialidle` (case-insensitive). Otherwise start no sequence player, and leave the bind (rest) pose plus the standalone controllers.
  - Leave the activation clips (`Open`/`Close`/`Forward`/…) to their consumers: door/container activation, `PlayGroup`, and a "default open" door state.
  - Pin it with a test over a door-shaped fixture whose first sequence is `Open`.

---

## Dimension 3 — Cross-game translation-pattern spot-check

Skimmed as unchanged. There are no findings.

- **Pattern A** (`bsver` literal grep, comment lines dropped): only the three test strings in `crates/nif/src/blocks/base.rs` (530, 550, 551).
- **New `GameKind` sites in `cell_loader`/`scene`/`streaming`:**
  - `load_order.rs::needs_exterior_processing`, one named decision (#6407).
  - Test fixtures.
- **Pattern C** (new per-game size or offset splits in `crates/plugin/src/esm/records`). Each carries per-era tests in `records/actor/tests.rs` and the items tests:
  - `NPC_` `ACBS` (FO4/FO76 vs Starfield lengths).
  - `RACE` `DAT2` (Starfield).
  - `ONA2` (Starfield).
  - The FO3/NV `ALCH` `ENIT` food flag.
- Decode-level divergences in range belong to `/audit-esm` (ESM-2026-10-27-01..04).

---

Publish with `/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md`, using labels `legacy-compat` + `animation` for D2-01 and `legacy-compat` + `tech-debt` for D1-01.
