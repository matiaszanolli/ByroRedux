---
description: "Per-game audit of Fallout 3 compatibility — NIF v20.2.0.7, BSA v104, ESM via FNV-shared parser"
argument-hint: "--focus <dimensions>"
---

# Fallout 3 Compatibility Audit

FO3 rides the **FNV path** almost end-to-end (NIF v20.2.0.7 / BSVER 34, BSA v104, one ESM parser, one cell loader, one RT pipeline). This is NOT a re-run of `/audit-fnv`: it hunts the **divergences** — where FO3 content exercises a path FNV doesn't, where shared code carries an FNV-only assumption, and where a feature is verified on FNV but only *assumed* on FO3. Its data goes through shared mechanisms owned elsewhere (`.claude/commands/_audit-owners.md`); a defect in the mechanism is filed against its owner, but **tag its FO3 reach**: a shared-code regression hits all four classic games (the 2026-09-19 HIGH did).

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and `.claude/commands/_audit-severity.md` for shared protocol.

## Game Context

| Aspect | State |
|---|---|
| NIF / BSA | v20.2.0.7 BSVER 34 · BSA v104 · 7 mesh/texture archives (base + 5 DLC `- Main.bsa`) |
| Parse baselines (pull) | `crates/nif/tests/data/per_block_baselines/fallout_3.tsv` (17 172 NIFs, 6 mesh archives, 145 types, 0 unknown — the gate walks `open_all_mesh_archives`, #3041), `ROADMAP.md` FO3 row, `docs/feature-matrix.md`, `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` |
| Reference data | `/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/` |
| FO3 vs FNV | Both are `GameKind::Fallout3NV`. The **only** discriminator is `CharacterRulesProfile` (`FALLOUT3` vs `FALLOUT_NEW_VEGAS`, #4448). A divergence branch keyed on `GameKind` cannot tell them apart; pins: `obscript_dialect_follows_the_profile_not_the_game_kind` (`byroredux/src/cell_loader/references/attach.rs`), `fn586_condition_gate_is_profile_scoped_not_game_scoped` (`crates/plugin/src/consumables.rs`) |

**Authoring census** (measured — each line is a checklist premise that has already rotted once; re-measure before asserting the opposite):
- Inline shader stack only: **0 BGSM/BGEM** in the FO3 archives; **0** `bump_texture`; no bump-tiling field exists on any `BSShader*Property`.
- **0 `XATO` / `XTNM` / `XTXR`** in `Fallout3.esm` — no REFR texture overlays (#3511); `byroredux/src/cell_loader/refr_texture_overlay_tests.rs` fixtures are FO4-shaped and prove nothing about FO3.
- `Fallout3.esm`: 718 952 records (`HEDR.numrec` 808 699 = records + GRUPs + the TES4 header), 1 257 SCPT, 54 SCOL bases / 389 REFRs, 244 TXST records (243 parsed — Bethesda's own *NullTextureSet* is skipped), 51 LTEX all carrying `TNAM` (the 51 `landscape_texture_sets` entries).
- NIF: 1 374 `BSSegmentedTriShape` (#3101's "zero" measured one archive), 112 `bhkConvexListShape` chains, **0 `bhkMultiSphereShape`**, B-splines are 83% of base-archive `.kf` — never rule them out by era.
- Object LOD: **114** `<world>.levelN.high.x<X>.y<Y>.nif` quads that no code path consumes (54 `washmontop.level8` in `Fallout - Meshes.bsa`, 60 in `Anchorage - Main.bsa`), every one with a plain sibling. Open #4468 counts only the 60 DLC ones.
- **Known-open, do not re-file** (2026-09-19): #4398 (emitter orientation dropped), #4401 (paired clamp/parallax residuals of #4235), #4468 (`.high.` quads), #4469 (INFO `DATA` undecoded, routed `/audit-esm`), #4122 (SPT tail desync).

## Parameters (from $ARGUMENTS)

`--focus <dimensions>`: comma-separated numbers (e.g. `1,4`). Default: all 5.

## Phase 1: Setup

1. Parse `$ARGUMENTS`; `mkdir -p /tmp/audit/fo3`.
2. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
3. Confirm `Fallout 3 goty/Data/` exists; if not, name the dimensions that lose real-data validation.
4. **Run the real-data lane first** — it is the only guard for shared-code regressions on FO3 and no CI lane runs it: `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines -- --ignored fallout_3 real_archive_torch`. A red gate is the headline finding; dimension agents then explain it.
5. Reconcile every count against the tables above and the ROADMAP/feature-matrix rows. Measure across **all** archives (base + DLC), never the base archive alone.
6. Scope by delta: `git log --since=<last AUDIT_FO3 date> --format='%h %cs %s' -- <dimension Paths>`.
7. Toolchain: the `byroredux` bin crate needs rustc >= 1.94 — rustup cargo per `docs/contributing.md` § Toolchain note (#4466).

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF, Inline-Shader Material & Collision — FO3 Authoring
**Subagent**: `legacy-specialist`
Routing: block decode `/audit-nif`; single-boundary / no-fabrication / particle + collision translation `/audit-nifal`; Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` must be 0, FO3 authors no BGSM) `/audit-renderer`; solver + phantoms `/audit-physics`.
**Paths**: `crates/nif/src/blocks/shader/`, `crates/nif/src/blocks/particle.rs`, `crates/nif/src/shader_flags.rs`, `crates/nif/src/import/material/`, `crates/nif/src/import/collision/`, `crates/nif/src/import/walk/emitter.rs`, `byroredux/src/material_translate.rs`
**First step**: the Phase 1 lane, plus `git log --since=<date> -- crates/nif/src/import crates/nif/src/blocks/shader`
- FO3 shader properties use the **`BSShaderFlags` F1 + `BSShaderFlags2` F2 pair** (`fo3nv_f1` / `fo3nv_f2` in `shader_flags.rs`) — never the Skyrim/FO4 vocabularies (same numeric bits, different meaning). Both properties are field-for-field with nif.xml on every BSVER gate (14/24/26/34).
- Texture precedence: a `BSShaderTextureSet` outranks `NiTexturingProperty` for base + normal (#4235; census: 3 703 NoLighting base co-binds all name the identical path). Residuals: #4401 (open).
- `BSShaderNoLightingProperty` -> `MATERIAL_KIND_NO_LIGHTING` (102) fullbright with **no distance term** (`crates/renderer/shaders/triangle.frag`); decal bits honored on both properties; two-sided comes from `NiStencilProperty` (no FO3 flag bit); legacy emissive is `EmissiveSource::Material` only.
- `BSSegmentedTriShape` decodes the nif.xml 9-byte segment layout and feeds the plain `NiTriShape` geometry arm; segment (dismemberment) metadata is consumed-and-discarded by design — route to `/audit-nifal` + `/audit-character` when a consumer exists.
- **Emitter birth rate**: FO3's leg of `real_archive_torch_meshes_surface_particle_emitters` (`crates/nif/tests/parse_real_nifs.rs`, `--ignored`) must hold its per-game floor. #4261 broke it on every game from 2026-09-12 to 2026-09-19 (#4467) because the fix ran only the default lane — **any change under `walk/` or `blocks/particle.rs` needs this lane**. Emitter spawn orientation is #4398 (open).
- Collision: all FO3 chains classify `CollisionAuthoring::Classic`; `havok_motion_type` keeps BOX_INERTIA crates Dynamic (1 612 in FO3); every FO3-authored bhk shape kind has a `resolve_shape` arm (a new dispatch arm needs a resolve arm).
**Output**: `/tmp/audit/fo3/dim_1.md`

### Dimension 2: ESM Data Slice (Fallout3.esm)
**Subagent**: `general-purpose`
Routing: GRUP walk, `SubReader` accounting, dispatch, remap `/audit-esm`. This dimension owns FO3-only authoring and its counts.
**Paths**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/`, `crates/plugin/tests/parse_real_esm.rs`
**First step**: `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored parse_rate_fo3_esm` (one test at a time; whole-file `--ignored` runs can spike >20 GB).
- Baseline (#3756): `index.total()` >= `FO3_TOTAL_FLOOR` (44 000) is an *index-sum* over ~95 typed maps that double-counts by design — NOT a file record count. Also floor the cell tier `index.total()` cannot see: placed refs >= 573 000 (REFR 568 107 + ACHR 2 154 + ACRE 3 349 + PGRE 350) and exterior cells >= 41 900; 0 walker errors on the master and all 5 DLCs. Never cite "44 657" or "13 684".
- FO3 is a strict subset of FNV's `NPC_`/`DIAL`/`INFO` subforms (sole FO3-only subform `INFO.SNDD`, 1 record) and shares `CELL.XCLL` (`XCLL_SIZES_FALLOUT_ERA`, FO3 `{36,40}`); the SCOL gate `is_scol_era` must keep FO3 dispatching (54 bases / 389 REFRs). `SCHR` u16 tail: see `/audit-fnv` Dim 2.
- The one FO3 TXST consumer is `LTEX.TNAM -> TXST -> TX00` -> `EsmCellIndex.landscape_texture_sets` (terrain splat diffuse); the other ~192 parsed TXST have no live consumer on this title.
- Shared-parser gaps reachable on FO3 (file against `/audit-esm`, note FO3 reach): #4469 INFO `DATA` (22 327 / 22 327 INFOs).
**Output**: `/tmp/audit/fo3/dim_2.md`

### Dimension 3: Cell Loading — Interior + Exterior
**Subagent**: `general-purpose`
Routing: terrain / WTHR / water / LOD mechanism `/audit-exterior`; unload hygiene `/audit-safety` Dim 3.
**Paths**: `byroredux/src/cell_loader/` (`load`, `unload`, `exterior`, `references/`, `spawn`), `byroredux/src/scene/`, `crates/plugin/src/esm/cell/wrld.rs`, `docs/engine/exterior-readiness-plan.md`
**First step**: `git log --since=<date> --format='%h %cs %s' -- byroredux/src/cell_loader byroredux/src/scene crates/plugin/src/esm/cell`
- Interior `MegatonPlayerHouse`: **929 REFRs** parse-side (never the stale 1609); fixture cell *MegatonMoriartysSaloon* 458 REFRs. GPU entity/FPS numbers come only from a Vulkan run — record "not measured" otherwise.
- Exterior (Capital Wasteland): worldspace selection is data-driven (`select_worldspace_key`, deterministic tie-break; `--wrld` overrides #444's heuristic) — FO3 data has no `wastelandnv` EDID, so an FNV-first preference can never mis-pick; `wrld.rs` must hold no hardcoded worldspace name, origin or grid. Every `GameKind` use on the load path is asset-family classification, not a behavior branch. Gate: `docs/smoke-tests/m-exteriors.sh fo3` (`MegatonWorld`); last measured figures in `exterior-readiness-plan.md` (ROADMAP's FO3 row still says "GPU bench pending"). Default land texture is `DirtWasteland01.dds` (`DefaultLandTexture::for_game`).
- Object LOD is `ObjectLodScheme::FalloutLegacyBlocks` (`placement_lod_supported` is Oblivion-only). It is **not a flat ring** (#3502): 7 of FO3's 15 worldspaces bake level-8 object quads with no level-4 sibling (93 of 422 quads: `dcworld01/03/06/12/17`, `paradisefalls`, `washmontop`); `LodBandSelection::coarsen_to_available` must stay **object-only** (terrain keeps subdividing — `object_lod.rs` true / `terrain_lod.rs` false) and those worldspaces must show distant buildings inside 16 cells.
- `CachedNifImport` Arc cache: no re-parse and no leak across FO3 unload/load cycles.
**Output**: `/tmp/audit/fo3/dim_3.md`

### Dimension 4: BSA v104 & Real-Data Validation
**Subagent**: `general-purpose`
Routing: reader discipline `/audit-parsers`. FO3's BSA is byte-identical in format to FNV's — a divergence here is a v104 regression, not a format gap.
**Paths**: `crates/bsa/src/archive/`, `crates/facegen/`, `crates/spt/`, `crates/nif/examples/nif_stats.rs`
**First step**: `cargo test -p byroredux-bsa --release --test bsa_real -- --ignored fnv_meshes_bsa_v104` (the only committed v104 real-data guard is FNV's — **no FO3 BSA test exists**; FO3 archives are checked by sweep).
- Sweep every FO3 archive: `Fallout - Textures.bsa` extracts fully to valid DDS headers (12 261 / 12 261 at last measure), folder-hash census has 0 collisions over 1 810 folders, no game-conditional branch in the v104 path.
- Pick one deathclaw-class creature (skinned via `BSDismemberSkinInstance`, `crates/nif/src/import/mesh/skin.rs`), one UI `BSShaderNoLightingProperty` element (fullbright route), one FaceGen head (`crates/facegen/`; may not render fully — note the gap), one `.spt` (`parse_rate_fo3_spt`: placeholder-billboard fallback, not a hard error).
- Runtime telemetry diff: `/audit-runtime --game fo3`.
**Output**: `/tmp/audit/fo3/dim_4.md`

### Dimension 5: Animation, NPC Spawn, Gameplay Data & the Scripting Gap
**Subagent**: `legacy-specialist`
Routing: gameplay mechanism `/audit-gameplay`; MenuXml `/audit-ui`; scripting runtime `/audit-scripting`; clip registry `/audit-ecs`. The classic-era M41 guards (B-spline `FLT_MAX` pose fallback, clip-registry dedup, NPC hand meshes) are listed in `/audit-fnv` Dim 4 — verify them on FO3 data here.
**Paths**: `crates/nif/src/anim/`, `byroredux/src/npc_spawn.rs`, `byroredux/src/hud.rs`, `crates/menuxml/`, `crates/plugin/src/consumables.rs`, `docs/smoke-tests/fixtures/fo3.env`
**First step**: `git log --since=<date> --format='%h %cs %s' -- crates/nif/src/anim byroredux/src/npc_spawn byroredux/src/hud.rs crates/menuxml`
- FO3 kf-era spawn works because its `skeleton.nif` resolves (unlike FO4); Megaton dwellers with no hands = the hand-mesh regression; the three `mtforward.kf` body-class walk clips (`humanoid_walk_kf_path`) exist in FO3's archive.
- **HUD**: `HudGameProfile::fallout3` — menu art from `Fallout - Textures.bsa` (FNV: `Textures2`), 8 font slots (FNV: 9), 2 bars. Guards: `crates/menuxml/tests/fo3_corpus.rs` (`BYROREDUX_FO3_DATA`) and `docs/smoke-tests/m48-5-fo3-hud.sh`.
- **Consumables**: Stimpak restores health + the seven body-condition AVIFs on FO3 as on FNV; FO3 has no Hardcore — CTDA fn 586 must be rejected under the FO3 profile (`fn586_condition_gate_is_profile_scoped_not_game_scoped`). Real-master guard: `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health` in `byroredux/src/inventory.rs` (`--ignored`).
- Playable gates declared in `fo3.env`: `p0-door-interaction`, `p5-save-restart`, `p2-melee-core` (Moriarty's Saloon). An undeclared gate exits 2 — that is unmeasured, not covered.
- **Scripting gap (known, owned by `/audit-scripting`)**: 1 257 SCPT parse; there is no general executing runtime for FO3 script logic. M47.3 ticks compiled *quest* scripts game-agnostically (101 / 192 QUSTs carry script refs, 403 / 1 257 scripts have executable GameMode blocks; FO3's `SetStage`/`GetStage` ids match the Oblivion-derived table). The narrow `obscript_runtime.rs` load-order-idiom interpreter does not reach FO3: `obscript_dialect_for` gives `FALLOUT3` no compiled dialect.
**Output**: `/tmp/audit/fo3/dim_5.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo3/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FO3_<TODAY>.md`:
   - **Executive Summary** — compatibility level + delta vs FNV (what is shared, what diverges); state the real-data lane result first.
   - **Dimension Findings** — grouped by severity per dimension.
   - **FNV-Shared Surface** — record/block/shader paths FO3 inherits, plus any FO3-only gap inside them.
   - **FO3-Distinctive Gaps** — inline-shader-only material universe, Capital Wasteland worldspace, the SCPT runtime gap.
   - **Validation Status** — interior (Megaton 929 REFRs), exterior (population + image-health gates per `exterior-readiness-plan.md`; the R6a FPS bench is still pending per the ROADMAP row), creature/NPC.
   - **Cross-Audit Routing** — shared-mechanism findings handed to their owner.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO3_<TODAY>.md`
(label every finding `game:fo3` + `legacy-compat`, plus its own domain label.)
