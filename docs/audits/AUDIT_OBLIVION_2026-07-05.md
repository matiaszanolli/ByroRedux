# Oblivion (TES4) Compatibility Audit — 2026-07-05

**Engine**: ByroRedux · **Target**: The Elder Scrolls IV: Oblivion (retail v20.0.0.5 + the v10.x NetImmerse tail)
**Scope**: 7 dimensions — NIF version handling, BSA v103, ESM records, fixed-function render path, NIFAL material translation, real-data validation, exterior blocker chain.
**Real data**: `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` (Oblivion.esm 277 MB, `Oblivion - Meshes.bsa` 671 MB / 8032 NIFs, ShiveringIsles + DLC BSAs) — all validated against live game files.

---

## Executive Summary

Oblivion compatibility is **healthy and largely regression-guard territory**. Every previously-fixed hazard on the Oblivion-unique surface still holds; the audit surfaced **one genuine code defect** (a content-quality gap, not a blocker) plus a handful of test/doc-hygiene items.

Live compatibility level (measured this sweep, not quoted from the roadmap):

- **NIF parse** — `Oblivion - Meshes.bsa`: **8026 / 8032 clean (99.93%)**, 6 truncated, **0 failures**, 81 distinct block types (no new types), **0 unknown blocks archive-wide**. The parser is marginally *ahead* of the checked-in per-block baseline. The 6 residual truncations are the expected pre-Gamebryo NetImmerse v3.3–v4.2 markers (`marker_arrow/divine/map/radius/temple/travel`, #1611), not new drift.
- **Archive extract** — **100%** across all 14 vanilla+DLC v103 archives (50 760 / 50 760 files; 9 612 / 9 612 NIFs), 0 hash/offset/name-length warnings on 25 041 files in a debug sweep.
- **ESM parse** — TES4 live path correct: 20-byte header, HEDR-1.0 → `GameKind::Oblivion`, all Oblivion-specific decode branches verified against real `Oblivion.esm`; both ignored parity tests (`clas_oblivion_knight`, `race_oblivion_data`) pass un-ignored.
- **Render end-to-end** — interiors render (Anvil Heinrich Oaken Halls). Fixed-function property stack (`NiTexturingProperty`/`NiMaterialProperty`/`NiAlphaProperty`/…) and the NIFAL canonical boundary emit correct `Material`s; all guards intact.

**Top blockers, in priority order:**

1. **On-device exterior render bench** (validation gap, *not* a code blocker). TES4 worldspace + LAND wiring is implemented and game-agnostic — parse + load ✓; the only remaining step is running an exterior Tamriel grid on a device. Same shape FO3 was pre-cell-loader.
2. **Legacy Oblivion particle FX render invisible** (MEDIUM, content-quality — sits *behind* the render gate, blocks no geometry). See OBL-01.

There are **no CRITICAL or HIGH findings.** The "BSA v103 is broken" framing is dead (closed #699) and was not regenerated.

---

## Dimension Findings

### MEDIUM

#### OBL-01 — Legacy Oblivion particle stack parsed then silently dropped at the importer
*(Dim 4 OBL-D4-01 ≡ Dim 7 OBL-D7-01 — independently found from both the render and cell-loader sides; merged.)*

- **Severity**: MEDIUM *(escalates to HIGH if the legacy corpus includes core torch/fire FX — currently unmeasured)*
- **Location**: `crates/nif/src/import/walk/mod.rs:501-533` (surfacing site); dispatch at `crates/nif/src/blocks/mod.rs:379-411`; parsers at `crates/nif/src/blocks/legacy_particle.rs`
- **Status**: NEW — incomplete close of #401 (closed 2026-04-18); the M36 particle pipeline only covers the FO3+/Skyrim NiPSys path
- **Description**: The import walker only downcasts the **modern** `NiParticleSystem` (`walk/mod.rs:509`) to build an `ImportedParticleEmitter`. Oblivion's pre-NiPSys **legacy** stack — `NiParticleSystemController` / `NiAutoNormalParticles` / `NiRotatingParticles` (+ `NiParticleGrowFade`, `NiGravity`, `NiParticleColorModifier`, …) — parses cleanly into `legacy_particle::*` but is **never surfaced**: `grep` for these types across the entire import + runtime tree returns only doc-comments and tests, zero production downcasts. The block never enters `out.particle_emitters`, so neither scene-build loop (`scene/nif_loader.rs:487`, `cell_loader/spawn.rs:410`) sees it, no emitter entity spawns, and the host node renders empty.
- **Evidence**: The walker comment claims *"the target games all author the modern NiParticleSystem stack"* (`walk/mod.rs:504-505`) — but the parser module states the opposite: *"Oblivion is v20.0.0.5 and still serializes them"* (`legacy_particle.rs:1-13`). #1327 correctly removed the dead `NiPSysBlock` downcast arm, but the accompanying conclusion — that legacy types need no surfacing — is wrong.
- **Impact**: Every Oblivion torch flame, fire brazier, smoke column, dust mote, waterfall mist, spell projectile and enchant shimmer authored on the legacy stack imports as a geometry-less node. Cells have correct geometry + "dead" atmosphere. Affects interior **and** exterior. Does **not** block the primary "exterior cell renders" goal. Blast radius is Oblivion/Morrowind/early-FO3 only (Skyrim+/FO4 author pure NiPSys). #1239 confirms ≥219 Oblivion NIFs use the *modern* path, so modern is dominant — the legacy fraction is unmeasured, which is why this is MEDIUM not HIGH.
- **Suggested fix**: Add a legacy-particle surfacing arm in `walk_node_flat` for `NiParticleSystemController` + walk its `NiParticleModifier` chain (grow/fade → base_scale, color → curve, gravity → force, rate/speed/lifetime → `ImportedEmitterParams`), converging on the existing `apply_emitter_overlays` → `apply_emitter_params` runtime — no renderer change needed. Correct the misleading `walk/mod.rs:504-505` comment regardless. Minimum stop-gap: `log::debug!` on an unconsumed legacy particle block so the drop is observable. A BSA sweep counting `NiAutoNormalParticles`/`NiParticleSystemController` occurrences would settle the severity. Reopen #401 or file a follow-up.

### LOW

#### OBL-02 — Oblivion real-data test env-var split across two helpers *(Dim 3)*
- **Location**: `crates/plugin/src/esm/test_paths.rs:37` (`BYROREDUX_OBLIVION_DATA`) vs `crates/plugin/tests/parse_real_esm.rs:752,1180,1294` (`BYROREDUX_OBL_DATA`)
- **Status**: NEW. Two divergent env-var override surfaces select the Oblivion data dir; setting one name silently redirects only half the Oblivion suite (the other half runs against the hardcoded fallback or skips). Test hygiene only — no runtime effect.
- **Fix**: Rename `BYROREDUX_OBL_DATA` → `BYROREDUX_OBLIVION_DATA` in `parse_real_esm.rs` (3 sites) so both surfaces agree.

#### OBL-03 — No v103 fixture or Oblivion real-data test in the BSA crate *(Dim 2)*
- **Location**: `crates/bsa/src/archive/tests.rs` (synthetic writer is v105-only; ignored real-data tests target FNV v104 + Skyrim v105)
- **Status**: NEW. The v103-specific behaviour (0x100 "Xbox" bit ignored for embed-names; 16-byte folder records) is exercised by **no** test. A regression breaking Oblivion specifically — e.g. flipping the embed-names gate to `>= BSA_V_OBLIVION` — would stay green in CI while corrupting 100% of Oblivion extracts. Code is correct today (verified against real data); this is the coverage gap for exactly the regression this dimension guards.
- **Fix**: Add a synthetic v103 fixture (16-byte folder records, 0x100 set, one compressed + one uncompressed file) asserting `version()==103`, `embed_file_names==false`, byte-exact roundtrip. Optionally an `#[ignore]` real-data test keyed on `Oblivion - Meshes.bsa`.

#### OBL-04 — Oblivion per-block baseline TSV is stale (understated) *(Dim 6)*
- **Location**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv`
- **Status**: Existing — tracked by **#1841** ("5 of 7 per-block baseline TSVs are stale"). The checked-in baseline understates current parsed counts across ~22 block types and still records 2 partial-unknowns (`NiMaterialProperty`, `NiTexturingProperty`) the live parser now resolves cleanly. The gate still passes (only fires on parsed-shrink / unknown-grow), but the understated numbers leave a small blind spot.
- **Fix**: Regenerate via `BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif --test per_block_baselines -- --ignored per_block_baseline_oblivion`; fold into the #1841 refresh.

#### OBL-05 — `references/import.rs` claims cell-loader animation wiring is unshipped (contradicts #544) *(Dim 7)*
- **Location**: `byroredux/src/cell_loader/references/import.rs:117-135`
- **Status**: NEW (doc-rot). The comment says cell-load doesn't attach `Name`/subtree roots so animation name-lookup can't anchor, citing #261 as an open follow-up. False as of #544: `spawn.rs:883-884` attaches `Parent`+`add_child`, `:896-897` attaches `Name`, `:1224-1229` spawns a per-placement `AnimationPlayer`. #261 is CLOSED. Misleads future readers into re-investigating a fixed gap (this audit spent a pass disproving it).
- **Fix**: Rewrite the comment to state the wiring landed in #544; drop the "#261 follow-up" framing.

#### OBL-06 — feature-matrix.md omits Oblivion from the terrain-splatting game list *(Dim 7)*
- **Location**: `docs/feature-matrix.md:49`
- **Status**: NEW (doc). The terrain-splatting row lists every game except Oblivion, implying no splat support — but the pipeline is game-agnostic and the Oblivion-only `LTEX.ICON` direct-path resolver is implemented (`support.rs:294-300`). Under-states Oblivion capability (bench-pending, not capability gap).
- **Fix**: Add Oblivion with a "parse ✓ / bench pending" qualifier, or footnote the row as bench-confirmed-only.

#### OBL-07 — ROADMAP "single hard-fail / recover 99.99%" not reproduced on this archive *(Dim 6)*
- **Location**: `ROADMAP.md` Oblivion matrix row
- **Status**: NEW (doc nuance, not a regression). The live sweep over `Oblivion - Meshes.bsa` reports 0 failures / recover-100%; the 6 residuals are recoverable truncations, not hard `Err`s. Numbers are equal-or-better than documented — the row conflates a per-archive figure with a hard-fail this archive doesn't contain.
- **Fix**: Scope the "recover 99.99% / hard-fail" phrasing to the specific archive that contains #698's corrupt-by-design marker, or note `Oblivion - Meshes.bsa` is recover-100%.

#### OBL-08 — Audit-skill #1509 prose inverts doghead.nif semantics *(Dim 1)*
- **Location**: `.claude/commands/audit-oblivion` SKILL.md #1509 bullet
- **Status**: Existing — tracked by **#1870**. The checklist says "doghead.nif (bsver 9) must keep the field"; with the correct `bsver > 9` gate the bsver-9 file correctly *skips* the trailing field. Live code (`morph.rs:90-93`) is right; the doc is inverted. No code action.

---

## Blocker Chain — "exterior cell renders"

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). The exterior path was traced stage-by-stage through `byroredux/src/cell_loader/exterior.rs` + `main.rs` streaming — **every stage is implemented and game-agnostic**:

1. **Parse plugins → EsmIndex** — `build_exterior_world_context` (`exterior.rs:78`); worldspace select via override → grid-containment → `"tamriel"` default (`:149`). ✓
2. **LAND heightmap** — VHGT/VNML/VCLR + BTXT/ATXT/VTXT (`walkers.rs:1086`). ✓
3. **Terrain mesh + splat** — `spawn_terrain_mesh` (`exterior.rs:302`); Oblivion `LTEX.ICON` direct paths handled (`support.rs:294-300`). ✓
4. **Water plane** — Oblivion worldspace-default water (NAM2 → Tamriel sea level, `exterior.rs:218`, #1305). ✓
5. **REFR / ACHR / ACRE spawn** — base forms → models, 99.93% parse (Dim 6); Oblivion-only ACRE placed-creature routed (#396). ✓
6. **Distant LOD** — `_far.nif` placement scheme + real LOD textures, validated vs 9888/9889 vanilla `.lod` files (`placement_lod.rs`, #1726/#1745). ✓
7. **On-device exterior render bench** — **the only remaining step. Never run.** A validation gap, not a code blocker.

**The single sequential item left to reach "exterior cell renders" is an on-device Tamriel-grid render bench.** Behind that gate sits the OBL-01 particle-quality gap (does not block geometry). The BSA-v103 framing is dead — do not regenerate it.

---

## Regression Guard List — verified still holding this sweep

**NIF (Dim 1):** v10.x stride-drift family #1506/#1507/#1508/#1509 (all land on next-block boundary; `cargo test -p byroredux-nif --lib` 863/0) · `NiTexturingProperty` reads u32 count raw, no `Has Shader Textures: bool` gate · BSStreamHeader dual-band predicate #170 (off-spec file does NOT read header) · `user_version` threshold `V10_0_1_8` · per-file u16/u32 flag width (#1331) · inline-string pre-Gamebryo dispatch · canonical `havok_motion_type` full enum #1652 · MultiSphere/ConvexList collision resolve arms · `as_ni_node` subclass unwrap.

**BSA (Dim 2):** v103 open + zlib extract #699 (100% / 50 760 files) · folder-record 16 B for v103+v104, 24 B only v105 · 0x100 "Xbox" bit ignored for embed-names on v103 · folder/file/extension hashes correct on 25 041 real files.

**ESM (Dim 3):** 16-byte ACBS #1650 (before FNV arm) · CTDA 24-byte Oblivion #1548 · CLMT 8-byte WLST #540 · MGEF 4-char code map #969 · CONT 4-byte DATA guard · `flags_oblivion`/`is_oblivion` CLAS/RACE gating · DIAL/INFO TES4 decode · ACRE exterior placement #396.

**Render / NIFAL (Dim 4 + 5):** `NiMaterialProperty` colors raw monitor-space (0e8efc6, no sRGB linearization) · all 11 `NiAlphaProperty` AlphaFunction enum values mapped · #869 `NiWireframeProperty` → LINE pipeline + `NiShadeProperty.flat_shading` consumed in `triangle.frag` · #1239 Oblivion `NiPSysEmitter` version gate · modern NiPSys import → `apply_emitter_params` reaches ECS · PBR resolves exactly once via NaN sentinel (no per-draw `classify_pbr`) · `emissive_source == EmissiveSource::Material` for legacy meshes · **`MAT_FLAG_PBR_BSDF` stays 0 across the entire all-legacy Oblivion universe** (Disney lobe unreachable — flagged once, shared Dim 4/5).

**Real data (Dim 6):** 8026/8032 clean, 0 unknown blocks, 81 block types (no new), 6 expected NetImmerse truncations #1611; chandelier/book/ogre-head import traces produce correct mesh counts + real material chains.

**Exterior (Dim 7):** TES4 worldspace + LAND wiring game-agnostic · pre-Gamebryo inline-string fallback logs at `debug!` not `warn!` (no sweep spam) · `_far.nif` LOD #1726/#1745.

---

*Suggested next step:* `/audit-publish docs/audits/AUDIT_OBLIVION_2026-07-05.md` — 1 MEDIUM (OBL-01) + 5 NEW LOW findings are publishable; OBL-04/OBL-08 already have issues (#1841 / #1870).
