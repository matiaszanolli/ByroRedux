# Legacy Compatibility Audit — 2026-07-16

**HEAD:** c3e09bb5 · **Type:** legacy-compat (canonical-translation boundary pass)

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux, framed by the three canonical translation layers
(NIFAL / EXAL / PHYSAL) plus coordinate-system correctness, the per-game
translation survey, and subsystem coverage vs. the legacy headers.

**Method:** Delta pass over the 2026-07-05 report (base a8d65d6c → HEAD
c3e09bb5, 85 commits). Re-verified all four single-producer boundaries
(material / env / coord / ragdoll) by grep at HEAD; they remain clean —
consistent with 07-05's finding of zero open defects there. Since that
report predates the entire M42 AI-package procedure-runtime arc (Sandbox
was already up; Wander/Travel/Follow/Escort/Guard/Patrol all landed in
this delta window, per `docs/engine/npc-spawn-ai-packages.md`), this pass
concentrated new-territory scrutiny there — a genuine legacy-compat
subsystem (Bethesda's PACK AI-package format) that no prior legacy-compat
audit has examined. That surfaced one verified NEW finding: the `PACK`
record's `PSDT` (schedule) sub-record is parsed with a single fixed byte
layout that a third-party reference implementation (wrye-bash's `brec`
module, cross-checked against the TES5Edit-derived `PACKDef.wiki`) shows
diverges structurally on Skyrim+. Deduplicated against the 28 currently
open issues (`/tmp/audit/issues.json`) and the per-layer leak inventories;
attempted to disprove the finding (including verifying the sibling `PKDT`
byte offset is *not* similarly broken) before including it.

**Headline:** The four canonical boundaries remain single-producer clean
at c3e09bb5 (no new `Material`/`SkyParamsRes`/`WeatherDataRes` construction
site, no duplicated `(x,z,-y)`/`4096.0` coordinate literal, `extract_ragdoll`
still switches only on `BhkConstraintData`, renderer still carries zero
`if game == …`). One **NEW MEDIUM** finding: `PACK` record parsing
(`crates/plugin/src/esm/records/misc/ai.rs::parse_pack`) has no `GameKind`
gate — unlike its sibling `SCOL`/`PKIN`/`MOVS`/`MSWP` arms in the very same
dispatch function (`crates/plugin/src/esm/records/mod.rs`) — and applies
the pre-Skyrim 8-byte `PSDT` layout unconditionally, silently misreading
the `duration_hours` field on Skyrim+/FO4/FO76/Starfield `PACK` records.
Currently dormant (no consumer reads packages for those games yet — see
below) but a live trap for the next M42 milestone. Six pre-existing open
issues re-verified unchanged.

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **Coordinate system** | `(x,z,-y)` swap + `EXTERIOR_CELL_UNITS` single-source | **Clean.** Re-grepped at c3e09bb5: every `zup_to_yup_pos`/`zup_to_yup_quat` hit outside `crates/core/src/math/coord.rs` is a *call* or a doc comment describing the convention, none re-derive the swap. The two other `4096.0` literals in the tree (`systems/locomotion.rs::LOCOMOTION_GROUND_RAY_MAX_DISTANCE`, a raycast distance cap, and `renderer/.../constants.rs::RENDER_ORIGIN_SNAP`) are distinct, already-documented constants, not cell-math duplicates. |
| **NIFAL — material** | `translate_material` sole populated-`Material` producer | **Clean.** Only non-test callers: `scene/nif_loader.rs:838`, `cell_loader/spawn.rs:917`. `cornell.rs`'s `matte`/`pbr`/`glass`/`emissive` helpers are the self-contained RT reference-scene harness (no on-disk game data), a documented exception, not a second NIF-import producer. |
| **EXAL — env resources** | `env_translate.rs` sole `SkyParamsRes`/`WeatherDataRes`/`CellLightingRes` producer | **Clean.** The only other construction sites (`render/lights.rs:300`, `systems/weather.rs:1210`, `commands_tests.rs:165`, `cell_loader/sky_params_cleanup_tests.rs:16`) are all test fixtures. `default_water_for_worldspace` remains the sole `GameKind`-branching exterior decision. |
| **PHYSAL — ragdoll** | one translate, one sink; extract game-agnostic | **Clean.** `crates/nif/src/import/collision/ragdoll.rs` has zero `game ==` branches; `build_ragdoll` (`crates/physics/src/ragdoll.rs:139`) is the sole non-test caller of the solver boundary. |
| **Renderer per-game branch** | render side carries no `if game == …` | **Clean.** Zero hits for `game ==`/`GameKind::`/`is_skyrim`/`is_fo4`/`is_starfield` across `crates/renderer/src` + `byroredux/src/render`. |
| **`3077dcb0` — engine-default interior lighting fallback** | new EXAL-adjacent code path stays inside the boundary doctrine | **Clean.** Ensures interior cells with no `XCLL` and no resolvable `LTMP` still get the engine-default `CellLightingRes` rather than an uninitialized/zeroed one; the fallback construction lives alongside the existing `translate_exterior_cell_lighting`-style pattern, not a new render-time branch. |

---

## Findings

### LC0716-01: `PACK` schedule (`PSDT`) parsed with a single fixed byte layout; diverges on Skyrim+/FO4/FO76/Starfield
- **Severity**: MEDIUM
- **Dimension**: Dimension 6 — per-game translation-survey gaps (Pattern-A/C: a hardcoded per-game byte layout applied with no `GameKind` branch, where sibling record types in the same dispatch function *do* have one)
- **Location**: `crates/plugin/src/esm/records/misc/ai.rs:538-550` (`parse_pack`'s `b"PSDT"` arm); `crates/plugin/src/esm/records/mod.rs:603-611` (the `b"PACK"` dispatch arm, contrast with the `is_scol_era`/`is_fo4_plus`-gated `SCOL`/`PKIN`/`MOVS`/`MSWP` arms at lines 246-294 in the same function)
- **Status**: NEW (searched `gh issue list --state all --search "PSDT OR PKDT OR PACK record"` → only #446, closed, unrelated to this format question; no legacy-compat report mentions `PACK`/`PKDT`/`PSDT`)
- **Description**: `parse_pack` decodes every game's `PACK.PSDT` sub-record with one fixed offset table (`month`@0, `day`@1, `date`@2, `time`@3, `duration: i32`@4..8), documented in the code's own comment as "FO3/FNV PSDT". This is the *only* layout used — `records/mod.rs`'s `b"PACK"` dispatch arm has no `GameKind` check, even though the same function already gates four sibling record types (`SCOL`/`PKIN`/`MOVS`/`MSWP`) on `is_scol_era`/`is_fo4_plus` for exactly this class of divergence. Cross-checked against a third-party reference implementation (wrye-bash's `brec.MelPackSchedule` / `MelPackScheduleOld`, the parser real modding tools use): the **old** (pre-Skyrim, i.e. Oblivion/FO3/FNV) `PSDT` struct is `['2b','B','b','i']` = 8 bytes, `duration` at offset 4 — matching Redux's fixed layout exactly. The **new** (Skyrim+) `PSDT` struct is `['2b','B','2b','3s','i']` = 12 bytes: `month`(1) + `day`(1) + `date`(1) + `hour`(1) + `minute`(1, **new field, not present pre-Skyrim**) + `unused`(3 bytes padding) + `duration`(4 bytes) — `duration` sits at **offset 8**, not offset 4. Redux's fixed offset-4 read on a Skyrim+ `PSDT` therefore reads the *`minute` byte plus 3 padding bytes* and reinterprets them as an `i32` `duration_hours`, and never touches the real duration bytes at offset 8. The sibling `PKDT` sub-record was also checked and is **not** affected the same way — wrye-bash's `MelPackPkdt` (Skyrim+, 12 bytes) confirms `package_ai_type` sits at the same offset 4 as the pre-Skyrim `package_ai_type`/`procedure_type` byte, so the procedure-type enum read (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol dispatch) is structurally compatible across eras — this finding is scoped to `PSDT` only, not the whole record.
- **Evidence**: `ai.rs:539-541` — `// FO3/FNV PSDT: month i8, dayOfWeek i8, date u8, time i8 (hour; -1/0xFF = any), duration i32 (hours).` (no era qualifier anywhere in the code path, and no `game` parameter reaches `parse_pack` at all — its signature is `parse_pack(form_id, subs, remap)`, no `GameKind`). `mod.rs:603-611` — the `b"PACK"` arm calls `parse_pack` unconditionally, unlike `b"SCOL" if is_scol_era`, `b"PKIN" if is_fo4_plus`, `b"MOVS" if is_fo4_plus` a few lines above in the same match.
- **Impact**: Currently **dormant** — verified via `rg -n "\.packages\b"` that `index.packages` (the `PackRecord` map this feeds) has exactly one production consumer, `npc_spawn::spawn_npc_entity` (`byroredux/src/npc_spawn.rs:671`), which per `GameKind::has_runtime_facegen_recipe()` runs only for **Oblivion + Fallout3NV**. The Skyrim+/FO4/FO76/Starfield spawn path (`spawn_prebaked_npc_entity`, `npc_spawn.rs:1813`) never calls any `active_package_is_*`/`active_package` function — confirmed via `awk` scan of the function body for `package`/`Behavior` tokens (zero hits). So no NPC's schedule gating is wrong *today*. The risk is forward-looking and concrete: `docs/engine/npc-spawn-ai-packages.md` explicitly frames the current seven procedures as a "bootstrap" with Skyrim+ package consumption as an obvious next step (the doc already notes Sandbox's sit-enter clip mechanism is "None for … Skyrim+/FO4+/FO76/Starfield … deferred" rather than "never"). The moment any future milestone wires package selection for those games, `PackSchedule::duration_hours` (and by extension `PackSchedule::active_at`, which every `active_package_is_*` selector calls) silently returns garbage for every Skyrim+/FO4/FO76/Starfield NPC with a scheduled package — no parse error, no panic, just wrong schedule windows (e.g. an NPC's sandbox slot silently active at the wrong hours, or never/always active depending on what garbage bytes land in `duration`).
- **Related**: `docs/engine/npc-spawn-ai-packages.md` §3-4 (`parse_pack`, `active_package`/`PackSchedule::active_at`); the `is_scol_era`/`is_fo4_plus` gating precedent at `crates/plugin/src/esm/records/mod.rs:193-294`; #446 (CLOSED — the original "PACK records skipped" bootstrap, which this is a follow-on format-fidelity gap on, not a regression of).
- **Suggested Fix**: Thread `GameKind` into `parse_pack` (it already has an implicit game context via `reader.get_form_id_remap()`'s caller, so the `game` value derived earlier in `parse_esm_with_load_order` just needs to reach this call site) and branch the `PSDT` decode on `game.uses_prebaked_facegen()` (or an equivalent "post-Skyrim package format" predicate) to select the 12-byte layout (`duration` at offset 8, plus surface the new `minute` field if a future consumer wants sub-hour schedule precision). Low urgency given the dormant blast radius, but cheap to fix now while the offset knowledge is fresh, and it removes a trap for whoever picks up Skyrim+ package consumption next.

---

## Still-open tracked gaps re-verified (Existing — do not re-file)

- **#1856** (FO3-D1-01) — FO3 `WaterShaderProperty.water_shader_flags` dead-ends at `MaterialInfo`. OPEN, NIFAL passthrough, unchanged.
- **#1849** (LC0702-05) — WRLD `NAM3`/`NAM4` LOD-water + `OFST` cell-offset table skipped, untracked-but-now-tracked. OPEN, correct, unchanged (EXAL §5.4).
- **#1827** (FO4-D4-02) — Starfield `BSGeometry` leaves per-vertex bone indices/weights empty. OPEN, informational (out of FO4 scope, in-scope for Starfield audit).
- **#1769** (D7-NEW-01) — VMAD attach dedup is case-sensitive; Papyrus names are case-insensitive. OPEN, unchanged.
- **#1743** (SCR-D7-03) — `--scripts-bsa` override order is first-listed-wins. OPEN, unchanged.
- **#1576** (SF-D4-03) — Model-less STAT/BNDS/ACTI/ARMO Starfield forms drop (geometry lives in a BFCB component block). OPEN, unchanged.

Confirmed **closed** since the 07-05 report (re-verified fixed, not re-filed): **#1889** (VWD marker), **#1852** (ragdoll writeback scale snapshot, `33bf6d8f`), **#1718** (FNV ragdoll bone-miss telemetry), **#1659** (BSDismemberSkinInstance now surfaced onto `ImportedSkin`, `120c4635` — still no render/gameplay consumer, which is correct per the no-fabrication rule; the parked-passthrough status moved from "discarded" to "surfaced, unconsumed"), **#1832** (TES grounding zero-mass fix), **#1731** (VWD flag parse).

## Documented limitations re-confirmed (NOT findings — do not re-file)

- **FO4/FO76/Starfield ragdolls** — blocked on `BhkNPCollisionObject → BhkSystemBinary` decoder (PHYSAL §5).
- **`BhkPCollisionObject` phantoms** — parked pending a `TriggerVolume` ECS path (PHYSAL §5).
- **NIFAL parked passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `bs_lod_cutoffs`, `lod_group`, `bs_sub_index`, `NiSwitchNode`/`NiTextureEffect` (content-absent).
- **`NiFogProperty`** — intentionally not dispatched (#1224); reads cell-scope `CellLighting`.
- **Emissive scale** — three `EmissiveSource` variants share ~1.0 scale; no normalization is correct (NIFAL §4).
- **Sun latitude** — no authored CLMT/WRLD latitude field exists; `SUN_SOUTH_TILT` is engine-defined (#1019 premise false; EXAL §9 Q1).
- **M42 AI-package v0 scope** — `docs/engine/npc-spawn-ai-packages.md` is exhaustively self-documented (spawn-time-only selection, no pathing/NAVM, no animation-clip swap, ~10 of 17 procedures unimplemented, `PTD2` unparsed, `NearReference` resolution ceiling ~12% on real FNV data). All of this is intentional, tracked scope — not re-filed here. LC0716-01 above is a genuinely new format-fidelity gap *within* the parser tier, distinct from these already-documented runtime-scope gaps.
- **Skyrim+ `PKDT` procedure-type byte** — verified this pass (see LC0716-01's evidence) to be offset- and (by field-naming convention) semantics-compatible with the pre-Skyrim layout; **not** a finding, despite living in the same sub-record family as the PSDT bug.

---

## Summary

- **Total findings**: 1
- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 1 (LC0716-01, NEW)
- **LOW**: 0

The compat surface stays in good shape at c3e09bb5. All four canonical
single-producer boundaries (material / env / coord / ragdoll) re-verify
clean across 85 commits of feature work, including the entire new M42
AI-package procedure-runtime arc (Wander/Travel/Follow/Escort/Guard/Patrol),
which is exceptionally well self-documented in its own cross-cutting doc
and introduces no boundary violations. The one new finding is a genuinely
fresh discovery in previously-unaudited territory (the `PACK` record's
per-era format, never before examined by a legacy-compat pass): the
`PSDT` schedule sub-record's fixed 8-byte offset table silently misreads
Skyrim+/FO4/FO76/Starfield's actual 12-byte layout. It is dormant today
(zero consumers touch non-Oblivion/FO3/FNV packages) but cheap to fix
before it becomes a live, hard-to-diagnose bug in a future package-runtime
milestone. Six pre-existing tracked issues re-verified unchanged; six more
confirmed closed since the last pass.

---

*Next step:* `/audit-publish docs/audits/AUDIT_LEGACY-COMPAT_2026-07-16.md`
(one NEW finding, LC0716-01, to file; the re-verified carryover issues need
no action).
