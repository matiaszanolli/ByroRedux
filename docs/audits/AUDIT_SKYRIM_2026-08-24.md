# Skyrim SE Compatibility Audit — 2026-08-24

**Command**: `/audit-skyrim` (standalone run, no `--focus` filter — full comprehensive sweep)
**Repo HEAD**: `048a8bd8`
**Delta audited**: 108 commits since `bb0b92f2` (the 2026-08-20 sweep's HEAD), 53 touching a
Skyrim-relevant path (`crates/nif`, `crates/bsa`, `crates/plugin`, `byroredux/src/npc_spawn.rs`,
`byroredux/src/render`, `byroredux/src/cell_loader`, `crates/renderer/shaders`,
`crates/core/src/character`, `byroredux/src/material_translate.rs`,
`byroredux/src/env_translate.rs`, `crates/facegen`)
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` (present —
`Skyrim.esm`, `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`, `MarketplaceTextures.bsa`,
`Skyrim - Meshes0/1.bsa`, `Skyrim - Textures0..8.bsa`, `Skyrim - Animations/Interface/Misc/Shaders/Sounds.bsa`)
**Dedup baseline**: `/tmp/audit/issues.json` (200 issues) + `docs/audits/AUDIT_SKYRIM_2026-08-20.md`

---

## Scope line

All **7 dimensions** executed by this agent directly, solo, no sub-agent fan-out (per the
task briefing — nested-agent runs on this audit family have gotten stuck or dropped
findings before). Unlike the 2026-08-20 pass, this run had **full tool access**: `cargo
check`/`test` on the affected crates, a live engine launch (`--bench-frames` /
`--bench-hold`) against the real Whiterun BanneredMare cell, and `byro-dbg` attach —
so this pass re-validates the control bench and the equip/render pipeline against real
game data and real code execution, not just static reading.

**Cross-reference note**: today's character audit landed
`crates/plugin/src/esm/records/actor_value_derive.rs::derive_skyrim_actor_values`
(Skyrim NPC Magicka/Stamina population) and independently byte-verified it against real
`Skyrim.esm`. This directly closes the prior D4-01 finding below (verified, not
re-derived — see Dedup).

**`cargo test --workspace` (bare) is broken** at HEAD by an unrelated `E0004` in
`crates/scripting/examples/fragment_coverage.rs:59` (non-exhaustive match, outside this
audit's scope). All testing below used per-crate `cargo test -p <crate>` /
`--bins` / `--test <name>`, which are unaffected.

---

## Executive Summary

This is the **cleanest pass in the `/audit-skyrim` series to date**: extensive live
verification (full per-crate test suites, a real Whiterun bench-frames + bench-hold run
with `byro-dbg` attach, a full Meshes0+Meshes1 parse sweep) turned up **zero new
findings**. The 108-commit delta since 2026-08-20 is dominated by a WATAL (water) refactor
(water params promoted to an SSBO, mesh-water translation unified, liquid-role
preservation) plus a new GPU morph-target deformation feature (#3231) — both are
higher-than-average regression risk by shape, and both check out clean under direct
testing.

Of the five findings the 2026-08-20 report opened, **four are now verified FIXED in code**
(the water.frag normal-blend regression, the `LVLF 0x02` over-expansion bug, the RACE
Magicka/Stamina parser gap, and the Skyrim underwater-fog clamp), and the fifth (the
CHARAL leveling-GMST unreachability) is unchanged and already tracked as **Existing:
#3221** (open). Two of the four fixes (#3217, and the morph-target index desync tracked
as #3233 — found pre-emptively by this audit, then disproved against the actual code) are
**fixed in the tree but their GitHub issues are still open** — a bookkeeping gap worth
closing, not a code gap.

**Totals — 0 NEW findings: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 0 LOW.**

---

## Live verification performed this pass

| Check | Result |
|---|---|
| `cargo check --workspace` | Clean, 0 warnings surfaced |
| `cargo test -p byroredux-nif -p byroredux-plugin -p byroredux-core` | 779 + fixture suites passed, 0 failed |
| `cargo test -p byroredux-plugin --test parse_real_esm -- --ignored` (real masters) | 17 passed, 1 unrelated FO4 failure (`parse_rate_fo4_esm` — out of scope for this game), **both Skyrim-tagged tests pass**: `skyrim_health_resolves_to_authored_avif_form_id`, `skyrim_default_water_promotes_underwater_tail` |
| `cargo run -p byroredux-nif --example nif_stats` over `Skyrim - Meshes0.bsa` | 18 862/18 862 clean (100.00%), 0 truncated, 0 recovered — matches ROADMAP baseline exactly |
| same, `Skyrim - Meshes1.bsa` | 0 `NiGeomMorpherController`/`NiMorphData` blocks present anywhere in the vanilla mesh corpus (relevant to the morph-target check below) |
| `cargo test -p byroredux-renderer --lib` | 743 passed, 0 failed, including all `shader_contract_tests` (`gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`, `gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_material_glsl_field_order_matches_rust_struct`, `gpu_light_glsl_copies_stay_in_lockstep`) |
| `cargo test -p byroredux --bins` (full binary crate) | **1515 passed, 0 failed**, 9 ignored (require other games' data) |
| `cargo test -p byroredux --test skinning_e2e -- --ignored sse` | 3/3 SSE skinned-reconstruction tests pass on real `Skyrim - Meshes0.bsa` data |
| Live `--esm Skyrim.esm --cell WhiterunBanneredMare --bench-frames 120` | Runs clean; `entities=5198 meshes=478 textures=319 draws=1234`, `wall_fps=79.5 / frame_p50=14.46ms`; `rt-integrity verdict=PASS` (`missing_skinned=0 missing_rigid=0 missing_ssbo=0`) |
| Live `--bench-hold` + `byro-dbg tex.missing` | `"No missing textures — all entities have resolved textures"` |
| Live `--bench-hold` + `byro-dbg entities Inventory` | 47 entities, including all **6 named Whiterun NPCs** (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda) — control-bench equip guard holds |
| Engine process shutdown | Clean teardown (`Vulkan context destroyed cleanly`); confirmed no orphaned process afterward |

The entity count (5198) is within 0.3% of the last stepped-camera refresh's 5183 (ROADMAP
`34074b93`, ~89.9 FPS); this run's 79.5 FPS is measured on shared desktop hardware, not an
isolated bench box, and per ROADMAP's own repeated caveats about that machine, an ~12%
FPS delta at flat entity count and 0 rt-integrity failures is **not** treated as a
regression finding here — consistent with the same caveat the 2026-07/08 bench-of-record
entries carry for this exact scene.

---

## Dimension roll-call

| # | Dimension | NEW findings | Verdict |
|---|-----------|-------------:|---------|
| 1 | BSTriShape packed geometry + SSE skinned reconstruction | **0** | Delta touches only Arc-sharing of `BSSubIndexTriShape` segmentation data (#2600, perf-only, no field/logic change) and the new morph-target extractor (see below, disproved). Live SSE skinning e2e tests (3/3) pass on real data. |
| 2 | `BSLightingShaderProperty` / `BSEffectShaderProperty` dispatch | **0** | `shader.rs` touched only by a documentation-reconciliation commit (`920f4db9`) and the water-unification refactor (mesh-water path only, doesn't touch Skyrim's lighting-shader arms). Coverage matrix unchanged from 2026-08-20. |
| 3 | NPC equip + FaceGen (M41) | **0** | `LVLF 0x02` fix (SKY-2026-08-20-D3-01) verified landed and correct; live bench confirms all 6 named Whiterun NPCs equip. No `crates/facegen` commits in the delta. |
| 4 | Multi-master load order + TES5 cell-load regression | **0** | RACE Magicka/Stamina fix (D4-01) verified landed, cross-referenced against today's character audit's independent byte-verification. Leveling-GMST unreachability (D4-02) unchanged, already tracked as `#3221`. |
| 5 | BSA v105 (LZ4) | **0** | Zero delta commits under `crates/bsa/`; re-ran the full Meshes0+Meshes1 sweep directly (protocol-blocked in the 2026-08-20 pass) — 100% clean, confirms the carried baseline by direct measurement rather than by construction. |
| 6 | Specialty blocks + real-data rendering | **0** | `water.frag` normal-blend regression (D6-01) verified fixed. New GPU morph-target feature (#3231) reviewed end-to-end; index-space handling verified correct with a dedicated regression test, and is unreachable on vanilla Skyrim content anyway (0 `NiGeomMorpherController` blocks in the mesh corpus). Live render trace (Whiterun) clean, `rt-integrity=PASS`. |
| 7 | NIFAL / WATAL canonical translation (Skyrim slice) | **0** | Underwater-fog clamp (D7-01) verified fixed — `HelgenWater` no longer degenerates. Mesh-water's previously-unsourced bit-16 gate is gone entirely (the water-unification refactor moved mesh water onto nif.xml's documented `WaterShaderPropertyFlags` bits 6/7); the WATR-side blend-normals gate now only discards nothing. |

---

## Dedup — prior report findings, re-verified at HEAD

### `AUDIT_SKYRIM_2026-08-20.md` findings

| Prior ID | Severity | State at HEAD | Evidence |
|---|---|---|---|
| SKY-2026-08-20-D6-01 (water.frag blend-normals discards layer B) | HIGH | **FIXED, verified** | `crates/renderer/shaders/water.frag:711-731` — the trailing `if (!blendAuthoredNormals) { nMix = nA; }` line the finding named is gone; the `else` arm of the third-layer branch now does `nMix = normalize(nA + nB)` unconditionally, restoring the two-layer blend on every record. |
| SKY-2026-08-20-D3-01 (`LVLF 0x02` treated as multi-pick, ~15× outfit over-population) | HIGH | **FIXED, verified — code matches the suggested fix exactly** | `crates/plugin/src/equip.rs:411`: `let multi_pick = lvli.flags & 0x04 != 0;`, with a comment citing `#3217` directly: *"`Calculate for each item` (bit 1 / 0x02) changes roll cardinality, not entry selection; treating it as Use All over-equipped 1,491 vanilla Skyrim NPCs (#3217)."* Landed in `bfdc3d3f`. **Issue #3217 is still OPEN on GitHub** — the commit message (`"Add tests for body piece masks and equip state handling"`) has no `Fix #3217` trailer. Recommend closing #3217 manually. |
| SKY-2026-08-20-D4-01 (RACE Magicka/Stamina never parsed) | MEDIUM | **FIXED, verified — cross-referenced, not re-derived** | `crates/plugin/src/esm/records/actor_value_derive.rs:169-177` (`derive_skyrim_actor_values`) now reads `race.starting_magicka`/`starting_stamina` plus the NPC's `magicka_offset`/`stamina_offset` and emits all three `(AVIF, value)` pairs. Landed in `4e1afcbe` (today's character-audit commit — see that report for the independent byte-verification against real `Skyrim.esm`). `skyrim_health_resolves_to_authored_avif_form_id` passes against real data. |
| SKY-2026-08-20-D7-01 (underwater-fog clamp erases negative near planes, `HelgenWater` degenerates to a 1-unit span) | MEDIUM | **FIXED, verified** | `crates/plugin/src/esm/records/misc/water.rs:895` (`apply_skyrim_dnam_tail`): `p.underwater_fog_near = near;` — the `.max(0.0)` clamp the finding named is gone; the sign is now preserved. `HelgenWater` (`near=-1000, far=-172`) now parses to `(near=-1000, far=-172)` — a genuine 828-unit span, not the prior degenerate `(0.0, 1.0)`. `skyrim_default_water_promotes_underwater_tail` passes against real data (`underwater_fog_far > underwater_fog_near`, `far >= 900.0` asserted directly in the test). |
| SKY-2026-08-20-D4-02 (Skyrim leveling-GMST overlay unreachable; `fXPPerSkillRank` fabricated) | LOW | **UNCHANGED — Existing: #3221 (open)** | `crates/core/src/character/profile.rs:106`: `CharacterRulesProfile::SKYRIM.ruleset` is still `RulesetBuilder::None`, so `build_ruleset` still short-circuits before `with_gmst` runs; `crates/core/src/character/leveling.rs:334`'s `LevelingModel::SKYRIM.with_gmst(...)` reference is still test-only. Already tracked as `#3221` (open) and `#3170` (CHARAL-owned duplicate, open) — not re-filed. |

### Bookkeeping gap surfaced by this pass

Two fixes are live in the tree with no GitHub issue closed against them:
- **#3217** (the `LVLF 0x02` finding) — fixed in `bfdc3d3f`, no `Fix #3217` trailer.
- **#3233** (`NIFAL-D7: morph-target index space desyncs between weight channel and
  vertex-delta array`) — this audit independently arrived at the same hypothesis while
  reviewing the new morph-target feature (see below), then found it already fixed:
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:1097-1120` (`flatten_morph_targets`)
  places each target's deltas at `original_index * vertex_count` rather than compacting
  filtered positions, exactly preserving the index space `AnimatedMorphWeights` (keyed by
  `FloatTarget::MorphWeight(idx)`, i.e. the same `NiMorphData` channel index) expects — with
  a dedicated regression test,
  `morph_gpu_buffer_preserves_filtered_source_index_holes`. This closes #3233's stated
  concern; **not re-filed as a new finding**. Recommend closing both issues.

---

## Disproved / rejected candidates (this pass)

Recorded so they are not re-investigated:

1. **Morph-target GPU weight buffer misaligns with a dropped/malformed target** (the
   working hypothesis behind this pass's deepest new-code review, before discovering it is
   `#3233`, already fixed). Disproved by direct code read: `flatten_morph_targets`
   (`byroredux/src/cell_loader/spawn/mesh_instance.rs:1100`) sizes the GPU delta buffer to
   `max(original_index) + 1` and writes each target at `original_index * vertex_count`,
   leaving an all-zero slot for any filtered/dropped index rather than compacting — and
   `update_morph_weights` (`byroredux/src/render/skinned.rs:280`) reads weights by the same
   index convention. Also moot on vanilla Skyrim regardless: `nif_stats` over both Skyrim
   mesh archives found **zero** `NiGeomMorpherController`/`NiMorphData` blocks in the
   entire corpus, so the feature has no reachable Skyrim content today.
2. **`BSWaterShaderProperty`'s mesh-water bit-16 gate is still an unsourced/undefined
   flag** (open concern carried from 2026-08-20's dedup list, item 4). Disproved as
   currently moot — `fa515b9c` ("unify mesh water translation") replaced the mesh-water
   optical-gate code with nif.xml's documented `WaterShaderPropertyFlags` vocabulary
   (`byroredux/src/material_translate.rs:139-148`: explicit `REFLECTIONS = 1 << 6` /
   `REFRACTIONS = 1 << 7` constants), and no `blend_normals` field is set from the mesh-water
   path at all any more — that field is WATR-record-only
   (`byroredux/src/env_translate.rs:701`). The old bit-16 code this concern was about no
   longer exists.
3. **The WATAL SSBO-growth refactor (`c329a91c`) desyncs `GpuWaterParams`' Rust/GLSL
   layout.** Disproved — `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep` (part of
   the 743-test renderer suite, all passing) is the dedicated regression guard and is green
   at HEAD.
4. **The Whiterun control-bench entity count or NPC-equip guard regressed under the WATAL
   delta.** Disproved by direct live measurement this pass (see table above) — entity count
   flat, all 6 named NPCs present with `Inventory`, `tex.missing = 0`, `rt-integrity = PASS`.

---

## Shader-Type Coverage Matrix (Skyrim `BSLightingShaderType`)

Unchanged from 2026-08-20 — `crates/nif/src/blocks/shader.rs`'s Skyrim dispatch arms have
zero delta commits touching their logic (the one commit that touched the file,
`920f4db9`, is a documentation reconciliation only). The arms `{1, 5, 6, 7, 11, 14, 16}`
continue to match nif.xml's `cond="Shader Type == N"` set for `#NI_BS_LTE_FO4#`; all other
numeric types fall through to `ShaderTypeData::None`. `BSEffectShaderProperty`'s
`env_map_min_lod` remains an unconsumed dead-end — **Existing: #2582**, still open, not
re-filed.

---

## Cell-Load Regression Status

| Guard | Result |
|---|---|
| `skyrim_health_resolves_to_authored_avif_form_id` (real `Skyrim.esm`) | **PASS**, live-run this pass |
| `skyrim_default_water_promotes_underwater_tail` (real `Skyrim.esm`) | **PASS**, live-run this pass — the `far >= 900.0` assertion directly exercises the D7-01 fix |
| `.STRINGS` / ESL / tombstone / repeatable `--master` guards (#1553/#1554/#1660/#561) | Zero delta commits touching `cell_loader/load_order.rs`'s remap core since 2026-08-20; not re-derived |
| Meshes0 sweep (`nif_stats`) | **18 862/18 862 clean, 0 truncated, 0 recovered** — directly re-measured this pass (not carried) |
| Meshes1 sweep (`nif_stats`) | Also clean; 0 morph-controller blocks present (see disproved-candidates §1) |
| Whiterun BanneredMare control-bench | **Live-measured this pass**: 5198 entities / 478 meshes / 1234 draws / 79.5 wall FPS / `rt-integrity=PASS`, all 6 named NPCs equipped, 0 missing textures — see live-verification table above for the shared-hardware FPS caveat |
| Full binary-crate test suite (`byroredux --bins`) | **1515 passed, 0 failed** |
| Full renderer-crate test suite (incl. all shader-contract lockstep guards) | **743 passed, 0 failed** |

---

## Evidence artifacts

No throwaway `_tmp_*` cargo examples were added this pass — all verification used the
existing `nif_stats` example, the existing `#[ignore]`-gated real-data test suites, and a
direct engine bench-frames/bench-hold run with `byro-dbg` attach (log at
`/tmp/audit/skyrim/bench_hold.log`, not durable — scratch only). The engine process was
launched, queried, and cleanly shut down within this session (no parallel/orphaned
instance left running, confirmed by `pgrep` before and after).

---

TALLY: CRITICAL=0 HIGH=0 MEDIUM=0 LOW=0
