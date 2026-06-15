# Oblivion (TES4) Compatibility Audit — 2026-06-14

Run as part of a `comprehensive` audit-suite sweep. Working dir
`/mnt/data/src/gamebyro-redux`. Oblivion game data present at
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — real-data
validation was exercised (NIF parse over all Oblivion BSAs, ESM parse over
vanilla `Oblivion.esm`).

Dedup baseline: `gh issue list` (300 issues) saved to `/tmp/audit/issues.json`;
prior Oblivion reports under `docs/audits/AUDIT_OBLIVION_*.md` scanned.

---

## Executive Summary

Oblivion compatibility is in strong shape and **better than the checked-in docs
claim**. The v10.x NetImmerse stride-drift family (#1506–#1509) that this audit
treats as a regression-guard set is fully fixed and holding; the residual
Oblivion-Meshes truncations dropped from the documented ~56 to **8** — six of
which are expected pre-Gamebryo `marker_*.nif` engine assets.

Current real-data state (live, 2026-06-15):

| Surface | Live state |
|---------|-----------|
| NIF parse — `Oblivion - Meshes.bsa` | **8024 / 8032 clean (99.90%)**, 0 failures, 8 truncated |
| NIF parse — all Oblivion + SI + DLC archives | **9604 / 9612 clean (99.92%)**, 0 failures |
| BSA v103 extraction | 100% (147,629 / 147,629 vanilla files; regression guard #699) |
| ESM parse — `Oblivion.esm` | header/GRUP/CELL/WRLD/LAND/REFR all resolve; 1855 cells, 33,549 exterior cells, 31,795 with LAND |
| Interior render | end-to-end (Anvil Heinrich Oaken Halls) |
| Exterior render | parse + load path **implemented and game-agnostic**; only an on-device render bench remains (not a code gap) |

**Top blockers in priority order:**

1. **OBL-D1-NEW-01 (HIGH)** — Three `NiInterpController`-descendant controllers
   in `shader.rs` plus the particle emitter/modifier controllers in
   `particle.rs` bypass the shared `parse_interp_controller_base` helper, so they
   miss the `Manager Controlled` bool on v10.1.0.104–108. This silently truncates
   whole subtrees on the only two non-marker Oblivion-Meshes files that still
   truncate (`scampswitch01.nif`, `arwelkydclusterfx01.nif`). Same root field as
   the resolved #1506; this is a sibling of that family the fix missed.
2. **DIM3-01 (HIGH)** — `parse_ctda` hard-rejects payloads `< 28` bytes and reads
   the FO3+ field map. Oblivion's CTDA is **24 bytes** with the function index as
   a u16 @8, so **every** Oblivion condition (60,115 of them) returns `None`.
   All Oblivion dialogue / quest-stage / AI-package / magic-effect conditions are
   silently dropped, so once the M47 logic consumes them every conditioned line
   is treated as unconditionally true.
3. **OBL-D1-NEW-02 (MEDIUM)** — `NiPSysEmitterCtlr.Visibility Interpolator` is
   gated on `>= V10_2_0_0` but nif.xml says `since=10.1.0.104`; compounds NEW-01
   on the same FX meshes. Folds into the same fix.

Everything else this sweep surfaced is documentation staleness (the ROADMAP
Oblivion row, the "exterior is blocked" framing, and the #688 truncation
narrative are all stale in the *improving* direction).

---

## Severity Counts

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH | 2 | OBL-D1-NEW-01, DIM3-01 |
| MEDIUM | 1 | OBL-D1-NEW-02 |
| LOW | 6 | OBL-D2-DOC-01, DIM3-02, OBL-D6-NEW-01, OBL-D7-NEW-01, OBL-D7-NEW-02, OBL-D7-NEW-03 |
| **TOTAL** | **9** | |

---

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x NetImmerse tail)

All 10 static-checklist items verified correct against current code; `cargo test
-p byroredux-nif --lib` = 811 passed / 0 failed. The two findings below come from
real-data tracing of the only two non-marker Oblivion truncations.

#### OBL-D1-NEW-01: NiInterpController descendants in shader.rs + particle.rs bypass `parse_interp_controller_base` → miss the `Manager Controlled` bool on v10.1.0.104–108 → silent whole-subtree truncation
- **Severity**: HIGH
- **Dimension**: 1 (NIF Version Handling)
- **Location**: `crates/nif/src/blocks/controller/shader.rs:181` (`NiMaterialColorController`), `crates/nif/src/blocks/controller/shader.rs:57` (`NiLightColorController`), `crates/nif/src/blocks/controller/shader.rs:214` (`NiTextureTransformController`); `crates/nif/src/blocks/particle.rs:932` (`parse_emitter_ctlr`), `crates/nif/src/blocks/particle.rs:922` (`parse_modifier_ctlr`)
- **Status**: NEW (sibling of the resolved #1506 family — same root field, different parser arms the #1506 fix did not cover)
- **Description**: The #1506 fix added the `NiInterpController.Manager Controlled` bool (nif.xml `since=10.1.0.104 until=10.1.0.108`) only to `NiSingleInterpController::parse` (`controller/mod.rs:254` via `parse_interp_controller_base`). Several `NiInterpController` descendants are decoded by hand-rolled functions that still call the plain `NiTimeControllerBase::parse` and therefore skip the bool in that version band:
  - `NiMaterialColorController` / `NiLightColorController` — nif.xml ancestry `NiPoint3InterpController → NiSingleInterpController → NiInterpController`.
  - `NiTextureTransformController` — `NiFloatInterpController → NiSingleInterpController → NiInterpController`.
  - `parse_emitter_ctlr` (`NiPSysEmitterCtlr`) / `parse_modifier_ctlr` (`NiPSysModifier*Ctlr`) — `NiPSysModifierCtlr → NiSingleInterpController → NiInterpController`.
  Missing the 1-byte bool under-reads the block by 1. Oblivion v10.1.0.x has **no per-block size table**, so the drift cascades and truncates the whole downstream subtree.
- **Evidence**: `nif_stats` over `Oblivion - Meshes.bsa` (2026-06-15) = 8024/8032 clean, 8 truncated. Six are pre-Gamebryo `marker_*.nif`; the other two are this bug:
  - `meshes\oblivion\architecture\citadel\interior\switch\scampswitch01.nif` (v10.1.0.106) drops **42 blocks**; `trace_block` shows drift first appearing at the two `NiMaterialColorController` blocks (#9/#10), then garbage `unknown KeyType: 16744447` at `NiTransformData`.
  - `meshes\dungeons\ayleidruins\interior\arwelkydclusterfx01.nif` (v10.1.0.106) drops **15 blocks**; `trace_block` shows `NiPSysEmitterCtlr` (block 17) consuming a bogus **4646 bytes**, then a ~1 GB allocation attempt at the next `NiTexturingProperty`.
  Inherit chains confirmed in `/mnt/data/src/reference/nifxml/nif.xml`: `NiInterpController.Manager Controlled` `since=10.1.0.104 until=10.1.0.108` (line 3615); `NiPoint3InterpController`/`NiFloatInterpController` both inherit `NiSingleInterpController` (lines 3613–3661). `has_interp_controller_manager_controlled()` (`version.rs:235`) is true for V10_1_0_104..V10_1_0_108 — the band these files sit in.
- **Impact**: Closes the last two non-marker Oblivion-Meshes truncations (→ 8030/8032, only the marker family remaining). Any old-Gamebryo (10.1.0.104–108) content with animated material/light color, animated UV transforms, or particle emitters silently loses the tail of its scene graph. Oblivion-only regression class.
- **Related**: #1506 (resolved — the `NiSingleInterpController::parse` half), OBL-D1-NEW-02 (compounds on the same files).
- **Suggested Fix**: Replace the bare `NiTimeControllerBase::parse` with the shared `parse_interp_controller_base` (or call `NiSingleInterpController::parse` and append the per-type tail) at all five sites. Promote `parse_interp_controller_base` to `pub(crate)` so `particle.rs` can use it. Add v10.1.0.106 regression fixtures for `NiMaterialColorController` + `NiPSysEmitterCtlr` mirroring the #1506 tests.

#### OBL-D1-NEW-02: NiPSysEmitterCtlr `Visibility Interpolator` ref gated on `>= V10_2_0_0` but nif.xml says `since=10.1.0.104`
- **Severity**: MEDIUM
- **Dimension**: 1 (NIF Version Handling)
- **Location**: `crates/nif/src/blocks/particle.rs:936-938` (and the identical gate in `parse_multi_target_emitter_ctlr`, `particle.rs:947-949`)
- **Status**: NEW
- **Description**: `parse_emitter_ctlr` reads the `Visibility Interpolator` ref only when `version >= V10_2_0_0`. nif.xml `NiPSysEmitterCtlr.Visibility Interpolator` is `type="Ref" since="10.1.0.104"` (nif.xml line 3674). For files in [10.1.0.104, 10.2.0.0) — including the v10.1.0.106 Oblivion FX content — the 4-byte ref is wrongly skipped. (The old `Data` ref `until=10.1.0.103` is correctly not read at ≥10.1.0.104.)
- **Evidence**: nif.xml line 3673-3674. `arwelkydclusterfx01.nif` is v10.1.0.106 → in band. Combined with NEW-01's missing bool, the emitter ctlr under-reads 1 + 4 = 5 bytes, consistent with the observed 4646-byte runaway consume.
- **Impact**: Compounds OBL-D1-NEW-01 on the same Oblivion FX meshes; no separate file population.
- **Related**: OBL-D1-NEW-01.
- **Suggested Fix**: Change the gate to `>= NifVersion::V10_1_0_104`. Also handle the pre-10.1.0.104 `Data` ref (`until=10.1.0.103`) arm and gate the `Interpolator` ref on `>= 10.1.0.104` the way `NiSingleInterpController::parse` already does. Fold into the OBL-D1-NEW-01 fix.

### Dimension 2 — BSA v103 Archive

Regression guard — confirmed holding. v103 opens and extracts 100% across all 17
vanilla Oblivion archives (live sweep: Meshes 20182/20182, Textures-Compressed
18040/18040, SI Meshes 3017/3017, Knights 4810/4810, Misc 115/115; zero
extract/NIF-magic/decompression failures). The "#699 v103 is broken" premise
stays dead. `cargo test -p byroredux-bsa archive` = 18 passed.

#### OBL-D2-DOC-01: Folder-record size doc comments omit v103 (say "v104" where they mean "v103/v104")
- **Severity**: LOW
- **Dimension**: 2 (BSA v103) — tech-debt / doc rot
- **Location**: `crates/bsa/src/archive/open.rs:92`, `crates/bsa/src/archive/open.rs:111`, `crates/bsa/src/archive/extract.rs:127`
- **Status**: NEW
- **Description**: Three comments say "v104" where the behavior (16-byte folder records / zlib) serves both v103 and v104. The live code is correct (`if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`, `open.rs:100`) and other comments in the same files already say "v103/v104" (`open.rs:4, :134`, `extract.rs:4`). Stale text only.
- **Evidence**: Static read; folder-record size constant verified at `open.rs:100`.
- **Impact**: None functional. Minor reader confusion / risk of perpetuating the long-dead "v104 = 24 B" misconception.
- **Suggested Fix**: Reword the three comments to "v103/v104".

### Dimension 3 — ESM Record Coverage (live path)

TES4 header / GRUP / 20-byte headers, the Oblivion record branches (actor
`is_oblivion` CLAS/RACE, MGEF-by-code map, CONT 4-byte guard, CLMT 3-entry WLST),
the CELL walker (1855 cells, 95.4% XCLL), and both ignored parity tests
(`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`,
in `crates/plugin/tests/parse_real_esm.rs`) all verified green against vanilla
`Oblivion.esm`. The exterior-REFR-placement ESM path is unblocked. One HIGH
coverage gap found.

#### DIM3-01: Oblivion 24-byte CTDA rejected — every Oblivion condition silently dropped
- **Severity**: HIGH
- **Dimension**: 3 (ESM Coverage) — import-pipeline
- **Location**: `crates/plugin/src/esm/records/condition.rs:222-229` (`parse_ctda`); consumers `crates/plugin/src/esm/records/misc/ai.rs:259,433` (QUST stages, INFO), `crates/plugin/src/esm/records/misc/magic.rs:435` (MGEF/SPEL)
- **Status**: NEW (distinct from #603 — CLOSED/LOW FO4 32-byte stride, which mis-stated FO3/FNV as 24-byte; FO3/FNV are 28-byte, Oblivion is 24-byte)
- **Description**: `parse_ctda` hard-rejects payloads `< 28` bytes and always reads the FO3+ field map (`function_index` u32 @8, `run_on` u32 @20, `reference_form_id` u32 @24). Oblivion's CTDA is **24 bytes** with a different layout: `type(1)+pad(3) | comparand(4) @4 | function u16 @8 + pad(2) | param1 @12 | param2 @16 | unused @20`. No `GameKind` is plumbed into `parse_ctda`, so there is no path that can accept the Oblivion shape — every Oblivion CTDA returns `None`. (Confirmed by reading the function: the `data.len() < 28` early-return and the `u32::from_le_bytes([data[8..12]])` function-index read.)
- **Evidence**:
  - Byte-decode of vanilla Oblivion.esm: **60,115** CTDA tags, size histogram `{24: 60115}` (100%). Decoded as the Oblivion layout, the u16@8 function-index histogram is the known TES4 catalog (72=GetIsID ×19595, 58=GetStage ×12458, 79=GetGlobalValue, …); bytes 10-11 are zero in 60114/60115 (confirms u16, not u32).
  - Live parse: `INFO_conditions=0`, `stage_conditions=0` across 19,278 INFOs / 390 quests; contrast FNV `INFO_conditions=59664`. (Verified with a temporary diagnostic test, since reverted; working tree clean.)
- **Impact**: Every Oblivion dialogue-response, quest-stage, AI-package, and magic-effect condition is lost at parse time. Empty `ConditionList` = "always fires" per Bethesda contract, so the downstream M47 logic will offer wrong dialogue branches, advance/skip quest stages incorrectly, and ignore AI-package guards. Blast radius = all Oblivion gameplay logic that gates on state. Silent — no warn, no test catches it.
- **Related**: #603 (CLOSED, FO4 stride; wrong FO3/FNV size premise), #1316 (condition evaluator stubs — downstream, unrelated to this parse-layout bug).
- **Suggested Fix**: Thread `GameKind` (or an explicit `ctda_len`-driven branch) into `parse_ctda`/`parse_condition_list` and add an Oblivion 24-byte arm: `function_index` as u16 @8, `param1 @12`, `param2 @16`, `run_on = Subject`, `reference_form_id = 0`, `extra_data_id = 0`. Keep the 28/32-byte arms unchanged. Add a regression test pinning a real Oblivion INFO's condition count > 0.

#### DIM3-02: `parse_ctda` has no game/length plumbing — silent length-gate is a recurring trap
- **Severity**: LOW
- **Dimension**: 3 (ESM Coverage) — import-pipeline / tech-debt
- **Location**: `crates/plugin/src/esm/records/condition.rs:222-279`
- **Status**: NEW
- **Description**: `parse_ctda(&SubRecord) -> Option<Condition>` is the single decode point for all games yet takes no game/version context; a "wrong layout" is signalled only via a silent `None`. This is the structural reason DIM3-01 hid — there is no warn or length-mismatch diagnostic. The XCLL decode already uses a `(game, len)` sanity-warn pattern that this path should mirror.
- **Evidence**: Static read of the function signature and the silent `data.len() < 28` early return.
- **Impact**: Defense-in-depth gap; future per-game CTDA layout drift will fail silently the same way.
- **Related**: DIM3-01.
- **Suggested Fix**: Route the length→layout decision through the same `(game, len)` sanity-warn pattern XCLL uses, so an unexpected length logs rather than silently dropping.

### Dimension 4 — Rendering Path for Oblivion Shaders

No findings. The legacy property pipeline maps correctly and the Disney BSDF gate
stays unreachable for Oblivion. Verified static checklist (see Regression Guard):
NiTexturingProperty slot routing (base/dark/normal-from-bump/detail/glow/gloss),
raw monitor-space `NiMaterialProperty` colors (no `srgb_to_linear` anywhere in
`crates/nif` or `byroredux`), full AlphaFunction blend-mode coverage, the #869
wireframe/flat-shading guards (captured on `MaterialInfo`, renderer-side
consumption deferred), vertex-color/material-color interaction, and the typed
particle-emitter import → `apply_emitter_params` runtime path (reachable for
box/cylinder/sphere emitters). `MAT_FLAG_PBR_BSDF` is only ever set behind a
`.bgsm`/`.bgem`/`.mat`/CDB gate, and `mesh.is_pbr` is hardcoded `false` in all
three NIF mesh extractors — so a legacy Oblivion `NiMaterialProperty`-only mesh
takes the Lambert branch (`triangle.frag:1013/1021`).

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion

No findings. An Oblivion `NiTexturingProperty`+`NiMaterialProperty` `MaterialInfo`
flows cleanly through the single canonical boundary `translate_material`
(`byroredux/src/material_translate.rs:157-160`) into the ECS `Material`.
Metalness/roughness carry the `f32::NAN` sentinel resolved exactly once by
`Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs:655-656`,
unconditional clamp); the render path reads `m.roughness`/`m.metalness` directly
(`byroredux/src/render/static_meshes.rs:314-315`) with no render-time keyword
scan. `emissive_source` is tagged `EmissiveSource::Material` on the
`NiMaterialProperty` arm (`crates/nif/src/import/material/walker.rs:604-605`;
test `nimaterial_tags_emissive_source_as_material`). The NaN-guard fixes
(#1434/#1411/#1409/#1382) and FxHash dedup (#1414) all confirmed closed. Tests:
`emissive_source` 5/5, `resolve_pbr` 5/5, `material_translate` 6/6.

### Dimension 6 — Real-Data Validation

No parse/import regressions. Live per-archive counts (2026-06-15):

| Archive | NIFs | clean | truncated | failures |
|---|---:|---:|---:|---:|
| Oblivion - Meshes.bsa | 8032 | 8024 (99.90%) | 8 | 0 |
| Oblivion - Misc.bsa | 0 (no .nif) | — | 0 | 0 |
| DLCShiveringIsles - Meshes.bsa | 1438 | 1438 (100%) | 0 | 0 |
| Knights.bsa | 75 | 75 (100%) | 0 | 0 |
| DLC{Battlehorn,Frostcrag,ThievesDen,VileLair,Orrery,HorseArmor} | 67 | 100% each | 0 | 0 |
| **TOTAL** | **9612** | **9604 (99.92%)** | **8** | **0** |

The per-block baseline test passes (81 types matched, zero `unknown grew` / zero
`parsed shrank`); the 8 truncated files are exactly the expected set (6 markers +
the 2 OBL-D1-NEW-01 files); 3 representative imports (alebottle clutter,
ictaloswallhouse01 architecture, rat creature) all import ≥1 mesh with full
material chains. One LOW tech-debt finding.

#### OBL-D6-NEW-01: Oblivion per-block baseline TSV is stale (parser has since improved; gate still green)
- **Severity**: LOW
- **Dimension**: 6 (Real-Data) — tech-debt
- **Location**: `crates/nif/tests/per_block_baselines.rs` (Oblivion TSV baseline)
- **Status**: NEW
- **Description**: The checked-in Oblivion baseline still lists 7 formerly-unknown types that now parse (NiPSys* modifiers, NiPSysData, NiStringExtraData) and is missing 7 new clean types (BSKeyframeController, NiCamera, NiPSysEmitter/Ctlr, NiPSysGrowFadeModifier, bhkConvexSweepShape, bhkMeshShape). The test stays green because the compare is asymmetric (fails only on `unknown` growth / `parsed` shrinkage), so the *improvements* are silently tolerated.
- **Evidence**: `nif_stats --tsv "Oblivion - Meshes.bsa"` diff vs the committed TSV shows only improvements.
- **Impact**: Baseline no longer reflects ground truth; a future improvement could regress one of these types back to `unknown` and the diff would be muddier to read.
- **Suggested Fix**: Regenerate with `BYROREDUX_REGEN_BASELINES=1`.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

The standing "Oblivion exterior is blocked on TES4 worldspace + LAND wiring"
framing is **empirically stale**: a live parse of vanilla `Oblivion.esm` shows
33,549 exterior cells across 70+ worldspaces, 31,795 carrying full LAND
(heights + normals + vertex colors + splat layers), 13,074 with placed refs. The
exterior loader is game-agnostic and un-gated (`scene.rs:216-250` `--grid`,
`exterior.rs:279-292` terrain spawn, `walkers.rs:954-1082` LAND decode). The only
remaining step is an on-device render bench (a Vulkan-device task out of
`cargo test` scope). Checklist items disproved with no finding: inline-string
fallback logs at `debug` not `warn` (`crates/nif/src/lib.rs:332`); `--bsa` reaches
Oblivion archives (`main.rs` → `asset_provider.rs:35`); anim name resolution is
present and game-agnostic (`anim_convert.rs`); particle emitters are routed
cell-loader-side (`cell_loader/spawn.rs`).

#### OBL-D7-NEW-01: ROADMAP / project-stats Oblivion clean-parse rate understated
- **Severity**: LOW
- **Dimension**: 7 (Doc Staleness) — tech-debt
- **Location**: `ROADMAP.md:197`, `ROADMAP.md:733`
- **Status**: NEW
- **Description**: ROADMAP states Oblivion "96.24% (7730/8032)"; live `nif_stats` over `Oblivion - Meshes.bsa` (2026-06-15) gives **8024/8032 = 99.90% clean, 0 failures, 8 truncated**. The #1506–#1509 family fixes landed since the last sweep. ROADMAP understates the rate and is internally inconsistent (line 73 already quotes a 99.99% recover rate). (Merges the duplicate OBL-D6-NEW-02 reported by Dimension 6.)
- **Evidence**: Live `nif_stats` run (Dimensions 1 + 6).
- **Impact**: Doc only; misleads anyone gauging Oblivion readiness.
- **Suggested Fix**: Refresh the ROADMAP Oblivion compat-matrix row and project-stats line to the live 99.90% (and 99.92% archive-aggregate). Per CLAUDE.md, ROADMAP is the authoritative source — fix it there.

#### OBL-D7-NEW-02: "Exterior blocked on TES4 worldspace + LAND wiring" framing stale across 4 doc sites
- **Severity**: LOW
- **Dimension**: 7 (Doc Staleness) — tech-debt
- **Location**: `ROADMAP.md:120`, `ROADMAP.md:197`, `ROADMAP.md:272`, `docs/feature-matrix.md:24-25` (+ the `✗` cells at `docs/feature-matrix.md:19-20`)
- **Status**: NEW
- **Description**: The docs frame the remaining Oblivion-exterior work as a TES4 worldspace + LAND wiring task. That wiring is implemented and game-agnostic (verified: 31,795 LAND-bearing exterior cells parse from `Oblivion.esm`); the true remaining step is an on-device render bench.
- **Evidence**: Blocker-chain trace (see below) — steps 1–5 implemented with file:line evidence.
- **Impact**: Mis-frames the remaining work; a contributor could spend effort re-implementing wiring that already exists.
- **Suggested Fix**: Update the four sites to read "parse + load ✓, exterior render bench pending".

#### OBL-D7-NEW-03: ROADMAP #688 narrative ("remaining ~149 NetImmerse-era files") stale vs live 8-truncated reality
- **Severity**: LOW
- **Dimension**: 7 (Doc Staleness) — tech-debt
- **Location**: `ROADMAP.md:197`, `ROADMAP.md:716`
- **Status**: NEW
- **Description**: The #688 closeout narrative still says ~149 NetImmerse-era Oblivion files truncate; live reality is **8** (6 markers + the 2 OBL-D1-NEW-01 files). The number is ~18× overstated after the #1506–#1509 fixes.
- **Evidence**: Live `nif_stats` + `recovery_trace`.
- **Impact**: Doc only.
- **Suggested Fix**: Refresh the #688 narrative number to 8 (and note OBL-D1-NEW-01 will take it to 6 once fixed).

---

## Blocker Chain — "Oblivion exterior cell renders"

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). Sequential
chain with verified current state:

1. **TES4 worldspace parse** — DONE. `crates/plugin/src/esm/cell/wrld.rs:15-183` (`parse_wrld_group`), dispatched from `crates/plugin/src/esm/records/mod.rs:76`. 70+ Oblivion worldspaces parsed.
2. **LAND heightmap parse** — DONE. `crates/plugin/src/esm/cell/walkers.rs:954-1082`; attached in `wrld.rs:213-215`. 31,795 / 33,549 exterior cells carry LAND.
3. **CELL exterior REFR placement** — DONE. `wrld.rs:204-216` collects refs; `byroredux/src/cell_loader/exterior.rs:371-385` spawns them. 13,074 exterior cells carry placed refs.
4. **Terrain mesh + splat spawn** — DONE (game-agnostic). `byroredux/src/cell_loader/terrain.rs:307`, invoked from `exterior.rs:279-292`.
5. **Exterior worldspace context + grid dispatch** — DONE. `byroredux/src/cell_loader/exterior.rs:72`, `byroredux/src/scene.rs:216-250` (`--grid`, no Oblivion exclusion; `"tamriel"` in the preferred list at `exterior.rs:114`).
6. **On-device exterior render bench** — PENDING (only remaining step). `cargo run --release -- --esm Oblivion.esm --grid 0,0 --radius 3 --bsa "Oblivion - Meshes.bsa" --textures-bsa "Oblivion - Textures - Compressed.bsa" --bench-frames 300 --bench-hold`. A Vulkan-device task, not a parser/loader gap.

DIM3-01 (24-byte CTDA) is a parallel gameplay-logic blocker that does not gate
the *render* milestone but does gate Oblivion AI/dialogue/quest correctness.

---

## Regression Guard List — verified still holding

- **#1506** NiQuatTransform TRS-valid bool[3] — `crates/nif/src/stream.rs:634-638`, gated `has_quat_transform_trs_valid()` (≤10.1.0.109). HOLDS. (Note: the *other half* of #1506, the `Manager Controlled` bool, holds for `NiSingleInterpController::parse` but is **missing** from the shader/particle controller arms — see OBL-D1-NEW-01.)
- **#1509** NiGeomMorpherController `bsver > 9` trailing-field gate — `crates/nif/src/blocks/controller/morph.rs:90-93`; tests `path_lookat_tests.rs:130/160/194`. HOLDS.
- **#170** BSStreamHeader dual-band guard — `crates/nif/src/header.rs:137-143`; test `bs_stream_header_not_read_for_off_spec_version` (`header.rs:582-603`). HOLDS.
- **user_version threshold** (≥ V10_0_1_8) — `crates/nif/src/header.rs:114-118`. HOLDS.
- **NiTexturingProperty u32 count raw** (no `Has Shader Textures` bool) — `crates/nif/src/blocks/properties.rs:211,337`; test `parse_ni_texturing_property_no_has_shader_textures_bool`. HOLDS.
- **v10.x sub-version constants + group_id band predicates** — `crates/nif/src/version.rs:71-132,235`. HOLDS.
- **Collision import** BhkMultiSphereShape → Ball/Compound + BhkConvexListShape → Compound — `crates/nif/src/import/collision.rs:500-526,618-633`. HOLDS.
- **BSA v103 extraction** (#699) — `crates/bsa/src/archive/open.rs:40,100`; 100% live extraction across 17 vanilla archives. HOLDS.
- **NiMaterialProperty raw monitor-space color** (no `srgb_to_linear`, 0e8efc6) — `crates/nif/src/import/material/walker.rs:591-608`; repo-wide grep for `srgb_to_linear` = 0 hits in `crates/nif` + `byroredux`. HOLDS.
- **Disney BSDF gate = 0 for Oblivion** (#1248-#1252) — `mesh.is_pbr` hardcoded `false` in `ni_tri_shape.rs:237` / `bs_tri_shape.rs:238` / `bs_geometry.rs:245`; `MAT_FLAG_PBR_BSDF` only set behind `.bgsm`/`.bgem`/`.mat`/CDB gate. HOLDS.
- **NIFAL NaN guards** (#1434/#1411/#1409/#1382) + FxHash dedup (#1414) — `Material::resolve_pbr` unconditional clamp (`crates/core/src/ecs/components/material.rs:655-656`). HOLDS.
- **#1239** Oblivion NiPSysEmitter version gating — emitter base layout routed by version gate; verified still in place (Dim 4). HOLDS.
- Test totals exercised this sweep: `byroredux-nif --lib` 811/0, `byroredux-bsa archive` 18/0, NIFAL material tests green, both ignored Oblivion ESM parity tests green un-ignored.

---

## Notes / Caveats

- All real-data numbers are from live runs on 2026-06-15 against the on-disk
  Oblivion install; nothing here was hardcoded from the skill or prior reports.
- Temporary diagnostic tests used to obtain the live ESM condition counts and the
  CTDA byte histogram were reverted; the working tree carries no source changes
  from this audit (only this report + the `/tmp/audit/oblivion/` scratch).

---

*Suggested next step:* `/audit-publish docs/audits/AUDIT_OBLIVION_2026-06-14.md`
