# Legacy Compatibility Audit — 2026-08-16

**Base:** `85b77371` · **Type:** full `/audit-legacy-compat` sweep, all 7 dimensions ·
**Run as part of** the `comprehensive` audit-suite sweep.

## Scope

All seven dimensions of `/audit-legacy-compat` were run: coordinate-system
correctness (Z-up→Y-up), NIFAL cross-layer mapping shape, the material translation
boundary, PHYSAL's source axis, EXAL, per-game translation-survey patterns
(A/B/C), and subsystem coverage vs the legacy engine.

**Legacy-source caveat (read this before weighing any Dimension 7 finding):** the
Gamebryo 2.3 source tree at
`/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/`
was **not mounted** in this sandbox. Dimension 7 was therefore audited against
`docs/legacy/` plus the authoritative NIF spec at
`/mnt/data/src/reference/nifxml/nif.xml`, which takes precedence over the 2.3
source anyway. No dimension was skipped as a result, but where a claim would
depend on unread Gamebryo *runtime* semantics it is stated as unconfirmed rather
than asserted (see SUBSYS-2026-08-16-02).

**Evidence base:** unlike the previous sweep, four candidate findings were tested
against **real vanilla game data** — `Skyrim.esm`, `FalloutNV.esm`, `Fallout4.esm`
were scanned directly for CELL/REFR/WEAP/ARMO sub-record incidence. That measurement
is what promoted one finding to HIGH and what **disproved** another (see
[Disproved Candidates](#disproved-candidates)).

**Method:** every dimension traced its claimed single-boundary contract to all call
sites; every candidate finding was re-read against current source and then actively
attacked before being kept. Deduplicated against the 269 open issues cached at
`/tmp/audit/issues.json` and against `docs/audits/`. No GitHub issue state was
mutated; no source file was modified.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 3 |
| LOW | 0 |
| **Total** | **4** |

All four are NEW. The low count is not a light pass: **eleven of the previous
sweep's fourteen findings have been fixed** since `79bfc76e`, and every prior
boundary claim re-verified as holding. The three canonical-translation layers
(NIFAL / EXAL / PHYSAL) are structurally intact — zero per-game branches
downstream of any `translate()` boundary, zero unaccounted second-producer sites.

Every finding this sweep sits in the two dimensions that own *coverage* rather
than *shape*: per-game wire-format dispatch (D6) and subsystem parity with the
source engines (D7). Three of the four are newly load-bearing because the P2
gameplay slice landed on 2026-08-15/16 — data that was dormant last week now feeds
combat, inventory and door activation.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRITICAL | HIGH | MEDIUM | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 0 | **none — clean** |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 0 | 0 | **none — clean** |
| 3. Material translation boundary | 0 | 0 | 0 | 0 | **none — clean** |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none — clean** |
| 5. EXAL — exterior environment → renderer | 0 | 0 | 0 | 0 | **none — clean** (1 candidate disproved on real data) |
| 6. Per-game translation-survey patterns (A/B/C) | 0 | 1 | 0 | 0 | PAT-D6-2026-08-16-01 |
| 7. Subsystem coverage vs legacy | 0 | 0 | 3 | 0 | SUBSYS-2026-08-16-01..03 |

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Findings: 0.**

All five of the previous sweep's COORD findings are closed and were re-verified:

| Prior | Status now |
|---|---|
| COORD-1 (KF XYZ-Euler CCW convention) | Closed (#2434). `crates/nif/src/anim/keys.rs:125` now calls `byroredux_core::math::coord::euler_zup_to_quat_yup`; the private CCW formula is gone. |
| COORD-2 (XTEL rotation bypassed the dispatcher) | Closed. `byroredux/src/cell_loader/transition.rs:298` calls `super::euler_zup_to_quat_yup_refr`, guarded by a source-text assertion at `:487-509`. |
| COORD-3 (`RENDER_ORIGIN_SNAP` second `4096.0`) | Closed. `crates/renderer/src/vulkan/scene_buffer/constants.rs:366` now defines it as `byroredux_core::math::coord::EXTERIOR_CELL_UNITS`. |
| COORD-4 (four `C·R·Cᵀ` copies) | Mitigated. `crates/nif/src/import/tests/coord_cross_check.rs::all_four_zup_to_yup_rotation_paths_agree` cross-checks all four paths. |
| COORD-5 (`cell_rot_sweep` hand-copied the dispatcher) | Closed (#2468). `crates/plugin/examples/cell_rot_sweep.rs:19` imports `euler_zup_to_quat_yup_mode` from core. |

New scan results:

- `(x, z, -y)` position swap has exactly one production home,
  `crates/core/src/math/coord.rs:72`. No new duplicate.
- Every production `4096.0` is either `EXTERIOR_CELL_UNITS` itself or an
  unrelated ray/radius budget (`LOCOMOTION_GROUND_RAY_MAX_DISTANCE`,
  `MAX_DERIVED_RADIUS`, `FOG_HEIGHT_REFERENCE_RAY_MAX_DISTANCE`). The one
  remaining bare literal, in `crates/core/src/ecs/components/camera.rs`, is
  inside `#[cfg(test)]`.
- Every REFR-family Euler caller routes through
  `byroredux/src/cell_loader/euler.rs::euler_zup_to_quat_yup_refr`; no caller
  hardcodes a rotation mode.
- `byroredux/src/cell_loader/load.rs:145::xcll_direction_yup` folds the axis
  swap into a spherical conversion. Checked as a possible duplicate producer and
  cleared: single site, documented, test-pinned, and semantically distinct
  (azimuth/elevation, not an Euler triple).

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**Findings: 0.**

Per-category single-producer verification:

| Category | Boundary | Single producer? |
|---|---|---|
| material | `byroredux/src/material_translate.rs` (`translate_material` + `translate_texture_only_material`) | Yes — both live in the boundary module; all six production callers route through them |
| lights | `byroredux/src/systems/light_anim.rs:159::translate_light` | Yes — the boundary #2439 asked for now exists; all three ESM producers in `byroredux/src/cell_loader/references/synth_child.rs` call it |
| skinning | `byroredux/src/scene/nif_loader.rs:1119` (`SkinnedMesh::new_with_global`) | Yes — every other call site is `#[cfg(test)]` |
| geometry/transform | `crates/nif/src/import/mesh/` + `crates/nif/src/import/coord.rs` | Yes |
| particles | `byroredux/src/systems/particle.rs` | Yes |
| collision | `crates/nif/src/import/collision/shape.rs::resolve_shape` | Yes (authored) |
| animation | `byroredux/src/anim_convert.rs` + `byroredux/src/asset_provider/animation.rs` | Yes |
| nodes | no single boundary by design | N/A — documented triage |

**Downstream per-game-branch scan:** `grep -rn "GameKind\|NifVariant\|bsver::"` over
`crates/renderer`, `crates/core`, `crates/physics` returns two hits, both
doc-comment prose in `crates/core/src/ecs/components/water.rs`. **Zero code
branches** downstream of any boundary.

Not re-filed (recorded in `docs/engine/nifal.md` §2 as bounded known gaps): the
cell-loader skinning gap (#2440's deferral), the four raw-parked `ImportedNode`
fields, the Starfield particle-slice N/A.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Findings: 0.**

- `translate_material` remains the sole populated-`Material` producer. The only
  other struct literals are `byroredux/src/cornell.rs` (the self-contained
  `--cornell` RT harness) and `#[cfg(test)]` sites.
- `translate_texture_only_material`
  (`byroredux/src/material_translate.rs:280`, landed under #2444 for LAND /
  terrain LOD / object LOD) was examined specifically as a candidate fourth
  materialization site and cleared: it owns no scalar literals of its own,
  seeds the NaN sentinel and calls `Material::resolve_pbr`, so terrain
  classifies by the same rules as the architecture standing on it.
- `Material::grayscale_to_palette_scale` now exists and is copied at the
  boundary (#2443) — the prior MAT-D3-01 drop is closed.
- Both regression guards hold: the three `EmissiveSource` variants still share
  one scale (no normalization introduced), and `NiFogProperty` remains the
  documented deliberate skip.

Known-open, deliberately not re-filed: #2330 (second roughness write site),
#2687 (save-restore is a `Material` producer that skips `resolve_pbr`),
#2571 / #2572 (Oblivion raw-tier bypass).

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

**Findings: 0.**

Both of the previous sweep's findings are closed:

- **PHYS-01** (`extract_ragdoll` ignored the `is_t` gate): closed.
  `crates/nif/src/import/collision/ragdoll.rs:123` propagates `is_t` per body,
  pinned by `extract_ragdoll_propagates_is_t_per_body` at `:501`.
- **PHYS-02** (LimitedHinge perp axis parsed then discarded): closed (#2448).
  `crates/physics/src/ragdoll.rs:487` now builds the angle-limit frame from the
  authored `perp_a`/`perp_b`, falling back to a synthesized perpendicular only
  for degenerate (zero / parallel) input.

The source-axis contract holds: `extract_ragdoll` still switches on
`BhkConstraintData` only and never on game, and the per-game seam is still only
the constraint CInfo decode.

Solver-end items are owned by `/audit-physics` and were not duplicated here
(#2877, #2882, #2883, #2884 remain open there). The documented limitations
(FO4+/FO76/Starfield packed Havok, the cone+2-plane approximation, captured-but-
unused motors) were re-confirmed as limitations, not re-filed.

---

## Dimension 5: EXAL — per-game exterior environment → renderer

**Findings: 0.** One candidate was investigated on real data and **disproved**
(see [Disproved Candidates](#disproved-candidates)).

Six of the eight prior EXAL findings are closed: EXAL-01 (#2449
`translate_lod_water`), EXAL-02 (`resolve_worldspace_climate` + the shared
`inherit_up_chain`), EXAL-03 (#2451 per-cell XCCM), EXAL-04 (#2452 —
`byroredux/src/cell_loader/lod_support.rs:51::baked_lod_supported` is now the
single named predicate), EXAL-05 (`climate_tod_hours` now lives at
`byroredux/src/env_translate.rs:676`), EXAL-08 (#2454 OFST drop). EXAL-06/07
remain correctly scoped under the open epics #2371 / #2372.

Single-producer re-verified: every production `SkyParamsRes` / `WeatherDataRes` /
`CellLightingRes` struct literal lives in `byroredux/src/env_translate.rs`. The
only other production literals are the `--cornell` harness and
`byroredux/src/cell_loader/load.rs:55::engine_default_interior_lighting`, which
is the **interior** no-authoring default and therefore outside EXAL's exterior
scope. Everything else is `#[cfg(test)]`.

The regression guards hold: the sun model still derives from `tod_hours` +
`weather::SUN_SOUTH_TILT` with no fabricated latitude field, and no inline
hardcoded sky/lighting block has reappeared in the render loop.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

**Findings: 1 (HIGH).**

Patterns A and B re-verified clean: no new bare BSVER comparisons beyond the
three already-open TD7 items (#2423 / #2424 / #2425), and per-game decisions are
consistently expressed as named `GameKind` predicates (`baked_lod_supported`,
`placement_lod_supported`, `LodBandLadder::for_game`,
`canonical_light_animation_flags` / `canonical_light_shadow_flags`,
`translate_light`) rather than scattered `matches!`. The prior sweep's PAT-D6-01
(Skyrim+ RACE `DATA`) was filed and is closed.

### PAT-D6-2026-08-16-01: FO4 weapons decode to all-zero stats — `parse_weap` waits for a `DATA` sub-record Fallout 4 never emits

- **Severity**: HIGH
- **Dimension**: Per-game translation-survey gaps (Pattern C — variant-enum struct shapes for divergent records)
- **Location**: `crates/plugin/src/esm/records/items.rs:195` (the `DATA` arm grouping FO4 with FO3/FNV), `:217` (the `DNAM` arm gated to `Fallout3NV` only)
- **Status**: NEW
- **Description**: `parse_weap` handles FO4 by folding `GameKind::Fallout4` into the `GameKind::Fallout3NV` arm of the `b"DATA"` match, and gates the `b"DNAM"` arm on `matches!(game, GameKind::Fallout3NV)` alone. Fallout 4 emits **no `DATA` sub-record on WEAP at all** — its weapon stats live entirely in a 132-byte `DNAM`. Neither arm therefore ever executes for an FO4 weapon, so `common.value`, `common.weight`, `damage`, `clip_size` and `anim_type` all fall through to their zero initializers. The mis-bucketing is acknowledged in the parser's own comment ("FO4 groups here pending its own per-game arm, mis-bucketing tracked separately, AUDIT_FNV_2026-04-20 follow-up") but that follow-up never became an issue and never landed. The sibling `parse_armo` in the same file *does* give FO4 a correct arm, so this is an omission rather than a design position.
- **Evidence**: Direct scan of vanilla `Fallout4.esm` (`/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4.esm`), decompressing the compressed record bodies and enumerating sub-record types:
  ```
  Fallout4.esm   WEAP_total 252   DNAM_len_132 252   (DATA: absent on all 252)
  FalloutNV.esm  WEAP_total 261   DATA_len_15  261   (matches the FO3/FNV arm)
  Skyrim.esm     WEAP_total 2484  DATA_len_10  2484  (matches the Skyrim arm)
  Fallout4.esm   ARMO_total 688   DATA_len_12  688   (matches parse_armo's FO4 arm — the contrast)
  ```
  The `damage` field is live, not dormant: `byroredux/src/inventory.rs:224-235` selects the equipped weapon by highest `damage` and writes it into `EquippedWeapon`, and `byroredux/src/combat.rs:269-273::attack_damage` reads exactly that value.
- **Impact**: On Fallout 4 every weapon reads as damage 0, value 0, weight 0, clip 0. The P2 melee slice therefore lands hits that deal **zero damage** with any equipped FO4 weapon (the `UNARMED_DAMAGE` fallback only applies when no `EquippedWeapon` exists at all, so equipping a weapon is strictly worse than being unarmed), the inventory panel shows "Damage 0" for all 252 weapons, and any future economy/encumbrance consumer inherits the same zeros. Entirely silent — no warning, no test coverage, and the existing `parse_weap` tests only exercise `Fallout3NV`, `Skyrim` and `Oblivion`.
- **Related**: The Oblivion instance of this exact collapse (AUDIT_OBLIVION_2026-04-25 O3-N-01) was fixed with a dedicated arm; FO4 is the remaining half. Adjacent open FO4 parser issues: #2904, #2908, #2911.
- **Suggested Fix**: Add a `GameKind::Fallout4` arm that decodes the 132-byte `DNAM` (value / weight / damage / reach / speed / ammo capacity), and widen the `b"DNAM"` gate accordingly. Extend the `parse_weap` unit tests with an FO4 fixture, and add a real-data assertion in `crates/plugin/tests/parse_real_esm.rs` that no game's WEAP population decodes to a uniformly-zero `damage`.

---

## Dimension 7: Subsystem coverage vs legacy

**Findings: 3 (all MEDIUM).**

All four of the previous sweep's SUBSYS findings are closed or explicitly
resolved: SUBSYS-01 (#2456 — `is_non_orthonormal` + rate-limited discard warning
now measure the baked-scale corpus before committing to decomposition),
SUBSYS-02 (#2457 — the vertex-colour gate was narrowed off `has_material_data`),
SUBSYS-03 (bone-name lookup on both the skin and ragdoll paths now goes through
`crate::name_lookup::get_case_insensitive`), SUBSYS-04 (#2459 — `DISABLE_SORTING`
is now documented at `byroredux/src/render/mod.rs:340-346` as a deliberate
non-wire rather than an oversight). SUBSYS-05 remains open as #2221 and is not
re-filed.

### SUBSYS-2026-08-16-01: Authored weapon reach and speed have no canonical landing site — every melee weapon in every game has identical reach and swing cadence

- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `crates/plugin/src/esm/records/items.rs:183-184` (Oblivion `speed`/`reach` read into `_`-prefixed bindings and dropped), `:215-216` (Skyrim's 100-byte `DNAM` explicitly "not decoded yet"), `:98-119` (the `ItemKind::Weapon` variant — ten fields, none of them `reach` or `speed`); `crates/core/src/ecs/components/inventory.rs:142-146` (`EquippedWeapon` carries `damage` only); `byroredux/src/combat.rs:23,26`
- **Status**: NEW
- **Description**: `MELEE_REACH_BU = 180.0` and `MELEE_COOLDOWN_SECONDS = 0.45` are process-wide constants applied to every attack regardless of what the equipped weapon authors. The authored inputs exist on disk on every supported game and are visible to the parser, but none reaches a canonical type: Oblivion's `speed` and `reach` are read off `DATA` into `_speed` / `_reach` and discarded on the next line; FO3/FNV's reach sits at an undecoded offset inside `DNAM` (only `anim_type`, `min_spread` and one unnamed float are read); Skyrim's whole 100-byte `DNAM` is undecoded; and per PAT-D6-2026-08-16-01 above, FO4's `DNAM` is not read at all. There is consequently no field on `ItemKind::Weapon` or `EquippedWeapon` for a consumer to read even if the decode landed.
- **Evidence**: `let _speed = r.f32_or_default(); let _reach = r.f32_or_default();` at `items.rs:183-184` is the whole lifetime of Oblivion's authored values. `attack_damage` (`byroredux/src/combat.rs:269`) is the only place `EquippedWeapon` is consulted during an attack, and it reads `damage`; the reach used for the cast is the module constant, never a per-weapon value.
- **Impact**: A dagger and a warhammer reach the same distance and swing at the same rate on every game. This is a *uniform* wrongness rather than a crash, so it presents as "combat feels flat" rather than as a bug, and no test can catch it because there is no per-weapon value to assert against. It also blocks the natural next step for the P2 slice (weapon-differentiated melee) at the parser tier, not the gameplay tier — the fix is three layers down from where the symptom is felt.
- **Related**: PAT-D6-2026-08-16-01 (the FO4 half of the same decode gap); #2962 covers `crates/core/src/combat.rs` / `crates/core/src/stealth.rs`, which are different files from `byroredux/src/combat.rs` and a different concern (CHARAL damage formulas vs. per-weapon geometry)
- **Suggested Fix**: Add `reach: f32` and `speed: f32` to `ItemKind::Weapon` with a documented per-game decode (Oblivion `DATA`, FO3/FNV/FO4 `DNAM`, Skyrim `DNAM`), carry them onto `EquippedWeapon` alongside `damage`, and have `combat.rs` treat `MELEE_REACH_BU` / `MELEE_COOLDOWN_SECONDS` as the unarmed fallback rather than the universal rule. Decode one game first and pin it against a known weapon (Oblivion's `reach` is already measured at 1.3 for the record in the existing `items.rs` test fixture at `:944-951`).

### SUBSYS-2026-08-16-02: The `NiTimeController` timing envelope is parsed and discarded — every mesh-embedded animation is forced to Loop at rate 1.0 with no phase

- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy (animation model)
- **Location**: `crates/nif/src/blocks/controller/mod.rs:36-45` (`NiTimeControllerBase` parses `flags` / `frequency` / `phase` / `start_time` / `stop_time`); `crates/nif/src/anim/entry.rs:255-260` (the merged embedded clip hardcodes `cycle_type: CycleType::Loop, frequency: 1.0`)
- **Status**: NEW
- **Description**: `NiTimeControllerBase` faithfully decodes all five timing fields for every controller. **None of the five has a consumer anywhere in the workspace** — `grep` for `start_time` / `stop_time` outside the parser returns only the unrelated `NiBSplineInterpolator` and `legacy_particle` fields, and the controller `flags` word is never read after parse. `import_embedded_animations` then merges *every* controller found in the scene into one `AnimationClip` named `"embedded"` with `cycle_type` and `frequency` written as literals. The KF path does this correctly by contrast: `crates/nif/src/anim/sequence.rs:21-22` sources both from the owning `NiControllerSequence`. So the loss is specific to mesh-embedded (non-KF) animation — exactly the ambient content the function's own doc comment enumerates: UV scrolling on water, alpha fades on ghost meshes, visibility flicker on torch flames, material-colour pulses on lava.
- **Evidence**: `nif.xml` (`/mnt/data/src/reference/nifxml/nif.xml`, the authoritative spec) defines the fields on the `NiTimeController` niobject — `Frequency` (default 1.0), `Phase`, `Start Time`, `Stop Time` — and defines `TimeControllerFlags` as a bitfield whose bits 1-2 are `Cycle Type` with **`default="CYCLE_CLAMP"`**, bit 3 `Active` (default true) and bit 4 `Play Backwards`. Redux's hardcoded `CycleType::Loop` therefore contradicts the format's own documented default for that field. Live consumers of the merged clip: `byroredux/src/streaming.rs:1082`, `byroredux/src/cell_loader/references/import.rs:122`, `crates/nif/src/import/mod.rs:144` — i.e. every NIF loaded through the cell path.
- **Impact**: Four distinct authored behaviours are unreachable: (a) per-controller playback rate — a controller authored at `frequency 2.0` plays at half speed, one at `0.5` at double; (b) `phase` — identical controllers deliberately offset so several flames or fans in one mesh desynchronise instead play in perfect lockstep, which is visually conspicuous precisely because it looks mechanical; (c) `cycle_type` — a `CYCLE_CLAMP` controller (play once, hold) loops forever, and `Play Backwards` is ignored; (d) `Active` — a controller authored inactive still animates. All silent. **Corpus incidence is unmeasured**, and the Gamebryo 2.3 runtime semantics for the flag bits could not be read (see the scope caveat), so this is scored MEDIUM rather than higher and the fix should be gated on measurement.
- **Suggested Fix**: Measure first — add a `crates/nif/examples/` dumper reporting the distribution of `frequency` / `phase` / cycle-type bits across a vanilla archive, so the fix is sized against real content rather than against the spec default. Then, rather than widening the single merged clip, give the per-controller envelope a home: either emit one clip per controller carrying its own `cycle_type` / `frequency`, or add per-channel `phase`/`time_scale` to the channel structs so the merged clip can still represent divergent envelopes. Do not flip `CycleType::Loop` to `Clamp` on the strength of the nif.xml default alone.

### SUBSYS-2026-08-16-03: REFR `XLOC` is never parsed — every locked door and container in every game opens on activation

- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:691-870` (the REFR sub-record match — no `b"XLOC"` arm); `byroredux/src/components.rs:65-75` (`DoorTeleport` carries no lock state); `byroredux/src/interaction.rs:820-825` (`collect_candidates` makes every `DoorTeleport` unconditionally activatable)
- **Status**: NEW
- **Description**: The REFR walker decodes 20-plus `X*` sub-records (`XSCL`, `XESP`, `XTEL`, `XPRM`, `XLKR`, `XLRT`, `XRMR`, `XPOD`, `XRDS`, `XATO`, `XTNM`, `XTXR`, `XEMI`, `XMSP`, `XOWN`, `XRNK`, `XGLB`, …) but has no arm for `XLOC`, the lock-state block that carries lock level, key FormID and lock flags. `XLOC` appears in the tree only twice, both in doc-comment prose listing sub-records that "carry a cross-record FormID reference" (`crates/plugin/src/esm/sub_reader.rs:35`, `crates/plugin/src/esm/reader.rs:477`) — neither is a parse site. The activation path has correspondingly no lock concept: `activation_is_blocked` (`byroredux/src/interaction.rs:872`) consults only the `MG07LabyrinthianDoor` demo script's `disabled` / `activation_blocked` fields, and `queue_door_transition` is called for any `DoorTeleport` the player looks at.
- **Evidence**: Direct scan of the three vanilla masters for REFR records carrying an `XLOC` sub-record (compressed bodies decompressed):
  ```
  FalloutNV.esm   REFR_total 307,710    REFR_XLOC   426   (420 × 20 bytes, 6 × 12 bytes)
  Skyrim.esm      REFR_total 693,333    REFR_XLOC 1,277   (1,277 × 20 bytes)
  Fallout4.esm    REFR_total 1,244,528  REFR_XLOC 1,358   (1,358 × 16 bytes)
  ```
  The three distinct payload widths (12 / 16 / 20) confirm this needs a per-game arm, not a single layout.
- **Impact**: With the P0 door-interaction slice shipped, this is now live rather than latent: every authored-locked door in every supported game opens on activation, and every locked container would too once container interaction lands. Quest and progression gating built on locks — the primary use of the record — is bypassed wholesale, and there is no signal that it happened. The key/lockpick systems have nothing to gate on either, so this blocks that whole feature area at the parser tier.
- **Related**: Listed as a skipped REFR sub-record in AUDIT_SKYRIM_2026-04-16 (line 212) alongside `XLCM` / `XAPD` / `XPRD` / `XPWR` / `XEZN`, but never filed as an issue and never fixed; the `XESP` half of that same list has since been implemented.
- **Suggested Fix**: Add an `b"XLOC"` arm to the REFR walker with per-width dispatch (12 / 16 / 20 bytes measured above), landing lock level + key FormID + flags on a canonical `LockState` component. Gate `collect_candidates` / `queue_door_transition` on it, and log at `info` when an activation is refused for lock reasons so the behaviour is observable during bring-up rather than silently absent.

---

## Disproved Candidates

Recorded so a future sweep does not re-chase them.

### Exterior CELL lighting (XCLL / LTMP) is dropped — **disproved on real data**

`crates/plugin/src/esm/cell/wrld.rs:479` hardcodes `lighting: None` for exterior
cells, and the LTMP capture at `:310-312` carries a comment promising interior-style
semantics ("XCLL wins, LGTM fills in") that the exterior path does not implement —
`resolve_cell_lighting` (`byroredux/src/cell_loader/load.rs:665`) has exactly one
caller and it is the interior load. That looked like an EXAL coverage gap.

Scanning the three vanilla masters for CELL records carrying `XCLL` and `LTMP`
shows no content is actually lost:

```
Skyrim.esm     ext cells 16,978   ext XCLL 0   ext LTMP 16,978  (all 16,978 = NULL FormID 0x00000000)
FalloutNV.esm  ext cells 30,109   ext XCLL 0   ext LTMP 30,067
Fallout4.esm   ext cells 38,970   ext XCLL 0   ext LTMP 38,970
```

Exterior `XCLL` is never authored, and Skyrim's universal exterior `LTMP` is a
null reference — the Creation Kit writes the sub-record on every exterior cell but
points it at nothing. Exterior lighting legitimately comes from WTHR alone. The
only residual defect is the misleading comment at `wrld.rs:310-312`, which is not
worth a finding on its own. **Not filed.**

### Additional candidates checked and cleared

- `xcll_direction_yup` as a duplicate `(x, z, -y)` producer — single site, documented, test-pinned, semantically distinct from the Euler path.
- `translate_texture_only_material` as a fourth materialization site — owns no scalar literals; routes through `resolve_pbr`.
- `SkinnedMesh::new_with_global` multi-producer — every extra call site is `#[cfg(test)]`.
- `WeatherDataRes` / `CellLightingRes` producers in `byroredux/src/systems/weather.rs` — all inside `#[cfg(test)]` blocks (`:1238`, `:1392`).
- `parse_armo` FO4 bucketing — checked as the sibling of PAT-D6-2026-08-16-01 and found **correct**: FO4 ARMO `DATA` measured 688/688 at 12 bytes, exactly the `Fallout3NV | Fallout4` arm's layout, and `is_skyrim_or_later` correctly includes FO4 for the `BOD2`/`MODL`-as-armature handling.

---

## Deduplication

`gh`-cached open issues (269) at `/tmp/audit/issues.json` were keyword-scanned for
every finding: `weap|weapon|reach|melee|combat|damage|item|fo4|fallout ?4`,
`controller|frequency|phase|embedded|cycle`, `lock|locked|xloc|key`,
`gmst|game setting`, plus the Dimension 1-5 keyword sets
(`coordinate|euler|4096|rotation-mode`, `material|light|skin|terrain|lod|water|climate`,
`ragdoll|hinge|havok`). `docs/audits/` was scanned for prior write-ups of each.

| Finding | Nearest existing | Verdict |
|---|---|---|
| PAT-D6-2026-08-16-01 | Only a prose mention in AUDIT_FNV_2026-04-20 ("tracked separately") that never became an issue; the Oblivion sibling O3-N-01 is fixed | **NEW** |
| SUBSYS-2026-08-16-01 | #2962 covers `crates/core/src/combat.rs` / `stealth.rs` — different files, different concern (CHARAL formulas vs. per-weapon geometry) | **NEW** |
| SUBSYS-2026-08-16-02 | #2562 / #2563 are Oblivion controller *truncation* (missing `Data` refs), not the timing envelope | **NEW** |
| SUBSYS-2026-08-16-03 | Listed once in AUDIT_SKYRIM_2026-04-16; never filed | **NEW** |

Skipped as already OPEN: #2221 (non-transform animation channel sinks), #2330,
#2371, #2372, #2373, #2424, #2425, #2687, #2882-#2884, #2942, #2962.

## Verification

Read-only source review plus four read-only scans of vanilla game data
(`Skyrim.esm`, `FalloutNV.esm`, `Fallout4.esm`). No build or test command was run
as part of this audit; no source file, game file, or GitHub issue was modified.
Per-dimension working notes are at `/tmp/audit/legacy-compat/dim_1.md` …
`dim_7.md`.

## Summary

- **Findings:** 4 (all NEW) — 0 CRITICAL, 1 HIGH, 3 MEDIUM, 0 LOW.
- **Boundary health:** NIFAL / EXAL / PHYSAL all structurally intact. Zero
  per-game branches downstream of any `translate()` boundary; zero unaccounted
  second-producer sites for any canonical type.
- **Closed since the last sweep:** 11 of 14 prior findings (all 5 COORD, both
  PHYS, 6 of 8 EXAL, all 4 SUBSYS, MAT-D3-01/02/03, NIFAL-D2-01, PAT-D6-01).
- **Where the remaining gaps live:** not in the abstraction layers but in
  per-game record decode (D6) and in subsystems the layers do not yet cover (D7).
  Three of four findings became load-bearing only in the last 48 hours, when the
  P2 gameplay slice turned dormant parsed data into live gameplay inputs.
- **Highest-value fix:** PAT-D6-2026-08-16-01 — every Fallout 4 weapon currently
  deals zero damage in the shipping combat slice, and the parser change is a
  single new match arm.

Suggested next step:
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md
```
