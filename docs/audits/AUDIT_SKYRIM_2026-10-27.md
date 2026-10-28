# Skyrim (SE + LE) Compatibility Audit — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-21.md` (HEAD `4a3e89d56`) · **Audited**: Dim 2 (shader dispatch + material slice; both 10-21 findings re-verified), Dim 3 (NPC equip + FaceGen, ~40 commits), Dim 4 (multi-master + TES5 cell load, incl. the new Skyrim FACT crime decode), Dim 5 (archives + corpus gates, real-data sweeps re-run) · **Unchanged since baseline (skimmed)**: Dim 1 (only the #6144 dispatch split and dead-code/doc commits; real-data corpus and SSE index guard re-run green)

Run context: part of the 2026-10-27 comprehensive suite. Analysis was solo, static, plus cargo tests. No engine launch and no sub-agents. All builds used the repo `target/`; no private target dir. Scratch notes are in `/tmp/audit/skyrim/dim_{1..5}.md`.

---

## Executive Summary

Skyrim parse and archive coverage is fully green on real data:
- **NIF corpus**: SE parses 33,468 of 33,468 NIFs across 8 archives (6 of them optional, present on this install). LE parses 22,466 of 22,466. All are clean.
- **Block baselines**: both per-block baselines match. SE has 856,103 blocks and 0 `NiUnknown`.
- **SSE skin indices**: the packed-index real-data guard checks 19,606 weighted lanes and finds 0 changed.
- **BSA sweeps**: SE 23 archives / 172,961 entries, LE 9 / 132,755, Meshes0 brute force 18,862 NIFs. All three report 0 errors.

The #6144 split of the 255-arm block dispatcher into nine family dispatchers holds:
- 301 distinct arm names, none claimed by two families.
- Every Skyrim-specific arm is present: `BSLODTriShape` → `NiLodTriShape`, `BSDynamicTriShape`, `BSSubIndexTriShape`, `BSTreeNode`, the packed-combined extras, and the lag-bone and lightning controllers.

Both 2026-10-21 findings are fixed and verified in code:
- **#6214**: the eye reflection centres now pass `zup_to_yup_pos` at import.
- **#6261**: the EyeEnvmap branch now samples mip 0.

Kind-16 eyes are now render-complete as far as static reading can show.

This run found no CRITICAL, HIGH or MEDIUM issues. It found five LOW ones:
- **SKY-D4-01**: the new FACT `CRVA` decode (4cfcee4b9) gates short forms at 14 bytes. The format cuts at 12 and 16, so 2 vanilla CRVAs are dropped and every 16-byte CRVA loses its steal multiplier. There is no vanilla runtime effect. This contradicts AUDIT_ESM_2026-10-27's "verified clean" note.
- **SKY-D3-01**: Skyrim's 13th and 12th template bits are missing from the enumeration. These are Use Keywords (0x1000) and Use Attack Data (0x0800). Today's #4137 doc commit says only Script and Def Pack List lack a consumer. Use Keywords has no consumer: `KeywordTable` reads the shell's KWDA, which loses keywords on 12 Skyrim.esm templated shells.
- **SKY-D5-01**: the LE BSA sweep passes silently without the env var. The LE NIF corpus finds the same install through its fallback.
- **SKY-D1-01 and SKY-D2-01** are doc-rot:
  - `decode_sse_skin_payload`'s no-remap contract sits on a type alias.
  - The import-side decal docs still call SLSF2 bit 21 `Cloud_LOD`.
  - About 10 `shader.rs:LINE` cites point into a file that has since been split, and the stale-line gate skips them on an ambiguous basename.

Items from this suite that affect Skyrim are cited here, not re-filed:
- **EXT-D6-2026-10-27-01 (CRITICAL)**: an out-of-bounds panic on terrain-LOD skirts at the first boundary LOD block of any exterior.
- **LC-2026-10-27-D2-01 (HIGH)**: at spawn the cell loader plays the first `NiControllerSequence` instead of Idle. This affects 1,033 of 1,137 SSE meshes whose first sequence is not Idle.
- **PAR-D1-01 (HIGH)**: hkx inlining weighs references by node count, not payload. Skyrim behavior graphs go through this path.
- **FNV-2026-10-27-02 (HIGH)**: the collider predicate covers Skyrim's `BSEffectShaderProperty` VFX only. Skyrim itself is fine.
- **SPT-2026-10-27-D4-01**: `spawn_nif_mesh` labels every upload with the Actor pool tier, so Skyrim loose and console loads are affected too.

Still open, not re-filed: #6096 (DLC facegeom from the global FormID; `prebaked_facegen_nif_path`, npc_spawn.rs:1844, is unchanged), #6434 (KeywordTable race taken off the shell), #5959 (BanneredMare bench), #4137.

| Severity | NEW | Existing / cited |
|---|---|---|
| CRITICAL | 0 | 1 cited (EXT-D6-01) |
| HIGH | 0 | 3 cited (LC-D2-01, PAR-D1-01, FNV-02 class) |
| MEDIUM | 0 | #6096, #5959 open |
| LOW | 5 | #6434, #4137 open |

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Reconstruction (skimmed; no behavioural change)

#### SKY-D1-01: `decode_sse_skin_payload`'s contract doc, including the #613 "no partition remap" invariant, is spliced onto the `SseSkinPayload` type alias
- **Severity**: LOW (doc-rot)
- **Dimension**: BSTriShape packed geometry
- **Location**: `crates/nif/src/import/mesh/skin.rs:634-645`
- **Status**: NEW
- **Description**: The doc block that opens "Resolve `shape.skin_ref` → `NiSkinPartition` → `SseSkinGlobalBuffer` …" ends with "The caller widens the packed indices without partition remapping." It then runs straight into "Decoded SSE skin payload: per-vertex `(weights, skin-global bone indices)`". All of it attaches to `pub type SseSkinPayload`. `pub fn decode_sse_skin_payload` itself has no doc. The type alias was inserted between the doc and the fn (32ec26bf4), and the no-remap line was added later on top (8c834e0be).
- **Evidence**: `git blame -L 634,646` shows the 014adc886 fn doc, then the 8c834e0be line, then the 32ec26bf4 alias doc and alias, then `pub fn decode_sse_skin_payload` at :645.
- **Impact**: The one load-bearing statement against re-introducing the #613/#2577 remap renders on a type alias. `core::source_scan::spliced_doc_blocks` (#6373) only flags docs that open with a sibling's name, so it cannot see this.
- **Related**: #6373, EXT-D6-2026-10-27-04 (same blind spot).
- **Suggested Fix**: Move the alias, with its own two-line doc, above the fn doc block.

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

Verified: #6214 and #6261 fixes (`triangle.frag:3218-3237` works in `fragWorldPosRel` and samples `textureLod(…, 0.0)`). The `.frag` and `.spv` were last touched in the same commit (e71451d08). The `parse_shader_type_data` arms are unchanged (`blocks/shader/lighting.rs:838`). `source_shader_type` is still copied before glass reassignment (`material_translate.rs:734`). 4ad1bbda7 drops the unread flag words from the modern decal/two-sided helpers, which preserves behaviour.

#### SKY-D2-01: Import-side decal docs still name Skyrim SLSF2 bit 21 `Cloud_LOD`, and ~10 comments cite `shader.rs:LINE` into the split `blocks/shader/` directory
- **Severity**: LOW (doc-rot)
- **Dimension**: Shader-type dispatch
- **Location**: `crates/nif/src/import/material/mod.rs:103-105,:121-125,:150-153`; `import/material/double_sided_tests.rs:272,:281`; `import/mesh/bs_tri_shape_shader_flag_tests.rs:278,:293`. Stale line cites: `material/mod.rs:138,:1250`, `dedicated_shader.rs:331,:454,:465`, `double_sided_tests.rs:330`, `lighting_shader_pbr_tests.rs:43`, `version.rs:522`, `blocks/shader_tests/fo76.rs:325`, `anim/channel.rs:523`
- **Status**: NEW. This is residual of closed #3545, which fixed only `shader_flags.rs`, and residual of closed #6167, whose list omitted `shader.rs`.
- **Description**: nif.xml `SkyrimShaderPropertyFlags2` defines bit 20 as `Cloud_LOD` and bit 21 as `Anisotropic_Lighting`. `shader_flags.rs:87-88,:149-152` now says that correctly. The import tier still says "flags2 bit 21 (`Cloud_LOD` on Skyrim, `Anisotropic_Lighting` on FO4 …)", so the crate contradicts itself. Separately, `crates/nif/src/blocks/shader.rs` no longer exists, but about ten comments cite line numbers in it. `crates/core/tests/stale_line_references.rs:104` skips those cites, because the basename `shader.rs` matches two live files (`blocks/controller/shader.rs`, `blocks/dispatch_tests/shader.rs`) and `Some([target])` only accepts a unique match. TD3-2026-10-27-01 covers zero-match basenames; this is the multi-match blind spot, and its sites are not in TD3's list.
- **Impact**: The code is correct; the modern helper no longer takes flags2. Readers of the decal helper docs are misled about Skyrim's bit map. The line cites point nowhere.
- **Related**: #3545, #6167, TD3-2026-10-27-01.
- **Suggested Fix**: Say "bit 21 = `Anisotropic_Lighting` on Skyrim and FO4, `Alpha_Decal` on FO3/FNV". Replace each `shader.rs:N` with the symbol plus its `blocks/shader/<file>.rs`. Make the gate fail on an ambiguous basename instead of skipping it.

### Dimension 3 — NPC Equip + FaceGen

Verified: #6228 Use-Traits sex/VTCK reads are pinned by `resolve_inherited_call_sites_are_enumerated_and_pinned`. The 1d319a942 `HeadPartRoot` rename leaves the prebaked path unchanged. #6210 (PSDT minutes) is in. The BODT and WNAM real-data censuses are green. The window's Oblivion and FO3/FNV body work (#5753 tails, #6090 ICON skins, #6320 seam blend) is gated off Skyrim.

#### SKY-D3-01: Skyrim's Use Attack Data (0x0800) and Use Keywords (0x1000) template bits are absent from the enumerated set, and Use Keywords has no consumer: `KeywordTable` reads the shell's own KWDA
- **Severity**: LOW
- **Dimension**: NPC equip / template chain (#4086)
- **Location**: `crates/plugin/src/esm/records/actor/npc.rs:382-406`; `crates/plugin/src/equip.rs:364-406` (eae8fd740, today); `crates/scripting/src/keywords.rs:40-49`
- **Status**: NEW. Related to open #4137 (it names 11 bits) and open #6434 (the race facet of the same reader).
- **Description**: Skyrim's ACBS template-flags u16 at offset 18 carries 13 bits. UESP *Skyrim Mod:Mod File Format/NPC* lists `0x800 Use Attack Data` and `0x1000 Use keywords`, and so does the CK wiki *Template Data* page. The doc and today's equip.rs rewrite list 0x0001–0x0400 and say "Only `0x0200` Script and `0x0400` Def Pack List are parsed and stored with no consumer yet". Neither 0x0800 nor 0x1000 has a constant or a mention. `KeywordTable::from_index` chains `npc.keywords` (the placed shell's KWDA) with the race's keywords, and never walks TPLT under 0x1000.
- **Evidence**: A byte-walk census over `Skyrim.esm` (5,118 `NPC_`):
  - 768 set Use Keywords, and 716 of those author no KWDA.
  - Templated shells that lose keywords their terminal authors: 2 through an `NPC_` chain (`E3demoGiant01`/`E3demoGiant02` → `EncGiant01`), and 10 through an LVLN template whose leveled members author KWDA (`LvlHorse`, `LvlHorseSaddled`, `LvlHorseSaddledPlayer`, `LvlFrostbiteSpiderNoGiantAmbush`, …).
  - 3,596 set Use Attack Data. ATKD is not parsed at all, so that bit is a doc gap only.
- **Impact**: `HasKeyword`-family conditions on those templated actors evaluate against an empty or wrong set. The blast radius is small in vanilla. The doc understates the gap on the day it was written.
- **Related**: #4137, #6434, #5498 (FO4 TPTA already reserves the Keywords slot; FO4 shares the gap).
- **Suggested Fix**: Add `TEMPLATE_FLAG_USE_KEYWORDS = 0x1000` and a `ResolvedNpc` keywords terminal, and have `KeywordTable` read it. Fold #6434's race fix in at the same time. Enumerate 0x0800 in the doc as "not parsed".

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load

Verified: `load_order.rs` changed only by doc moves (#6373). `parse_real_skyrim_esm` and `ms01_eltrys_authored_placement` (DIAL QNAM ownership) are green.

#### SKY-D4-01: The FACT `CRVA` short-form gate is 14 bytes, but the format's optional cut is at 12 and 16. Two vanilla CRVAs are dropped and every 16-byte CRVA's steal multiplier is discarded
- **Severity**: LOW
- **Dimension**: TES5 record coverage (Skyrim data through the FACT parser)
- **Location**: `crates/plugin/src/esm/records/actor/faction.rs:5-11` (`CRVA_MIN_LEN = 14`, `CRVA_TAIL_LEN = 20`), `:334-355`; test `actor/tests.rs:2693-2694` (`crva[..14]`)
- **Status**: NEW (4cfcee4b9, 2026-10-24). This contradicts AUDIT_ESM_2026-10-27 § Verified clean, "`CRVA` matches TES5/FO4 `SetOptionalFrom(7)`, a 14-byte minimum".
- **Description**: The layout is:
  - `u8 arrest`, `u8 attack-on-sight`, then `u16` murder, assault, trespass, pickpocket and unknown. That is **12** bytes.
  - `f32 steal mult` brings it to **16**.
  - `u16 escape` and `u16 werewolf` bring it to **20**.

  UESP *Skyrim Mod:Mod File Format/FACT* gives `struct[12/16/20]`. `SetOptionalFrom(7)` makes element 7 (steal mult) the first optional element, which is 12 bytes in, not 14: elements 0–1 are u8. The parser's own doc ("end after the `unknown` word (14 bytes)") is off by two. Records at 12 bytes fail the `>= 14` gate, so `values` is `None`. Records at 16 bytes take `tail = len >= 20` false, so `steal_multiplier = 0.0` whatever is authored.
- **Evidence**: CRVA size census:

  | Plugin | 20 B | 16 B | 12 B |
  |---|---|---|---|
  | `Skyrim.esm` | 969 | 112 | 2 |
  | `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm` | all | 0 | 0 |

  The 12-byte records are `MS06CultistFaction` and `MS06PotemaFaction`.
- **Impact**: No vanilla runtime effect. All 112 16-byte multipliers are authored 0.0. None of the 114 short-form factions sets TRACK_CRIME (0x40). The only consumer (`crime_faction.rs:114`) reads fines, not arrest or attack-on-sight. A mod that authors an older-version 16-byte CRVA loses its steal multiplier.
- **Related**: AUDIT_TECH_DEBT_2026-10-25 (named the constants only), #6427.
- **Suggested Fix**: Gate at 12 bytes. Read `steal_multiplier` at ≥ 16 and escape/werewolf at ≥ 20. Pin the test at 12 and 16.

### Dimension 5 — Archives + Corpus Gates

Verified: no `crates/bsa/src/archive` change since the baseline. The #5663 stamp (dev/ino/ctime), the #6308 loose fold and the #6243 depth cap of 32 move no Skyrim baseline; SE `Data\` has no loose meshes or textures.

#### SKY-D5-01: `skyrimle_bsa_corpus_full_extract_zero_errors` passes silently when `BYROREDUX_SKYRIMLE_DATA` is unset
- **Severity**: LOW (test-gap)
- **Dimension**: Archives + corpus gates
- **Location**: `crates/bsa/tests/bsa_real.rs:99-103` (`skyrimle_data_dir`), `:322-326`
- **Status**: NEW (the #6184 shape; the file sits under #6184's allowlist)
- **Description**: Every other game in `bsa_real.rs` resolves through `data_dir(env, fallback)` (:55-75). That helper falls back to the dev-machine path and calls `require_game_data` on the strict lane. `skyrimle_data_dir` is env-only. It has no Wine-prefix fallback, although `_audit-common.md` and `crates/nif/tests/common/mod.rs` (`Game::SkyrimLE.default_path()`) both use one, and it never fails the strict lane. With the var unset, the test prints an `eprintln` and returns `ok`.
- **Evidence**: This run, `cargo test --release -p byroredux-bsa --test bsa_real -- --ignored --exact skyrimle_bsa_corpus_full_extract_zero_errors` printed `[Skyrim LE] set BYROREDUX_SKYRIMLE_DATA to enable corpus sweep` and reported `ok`. The LE NIF corpus in the same shell ran 22,466 NIFs through its fallback. With the var set, the sweep extracts 9 archives / 132,755 entries with 0 errors.
- **Impact**: The LE BSA v104 extraction gate reads green without having run. The skill's "LE extracts every entry" guard is only as good as the operator's environment.
- **Related**: #6184, #5903, #3850.
- **Suggested Fix**: Route it through `data_dir("BYROREDUX_SKYRIMLE_DATA", <Wine-prefix Data path>)`.

---

## Shader-Type Coverage Matrix

Skyrim `BSLightingShaderType`, BSVER 83–129. Block counts (SE / LE) come from the 2026-10-12 census and were not re-measured. The Skyrim parse path is byte-unchanged (per-block baselines match). Only row 16 changed since 2026-10-21.

| # | nif.xml name | ShaderTypeData | Parse | Import | Render | SE / LE blocks |
|---|---|---|---|---|---|---|
| 0 | Default | None | yes, 0 B | n/a | default lit | 45,458 / 36,963 |
| 1 | Environment Map | EnvironmentMap | yes, 4 B | `env_map_scale` + latch | forced env reflection; alchemy glass → glass (#4392) | 6,726 / 3,176 |
| 2 | Glow Shader | None | yes, 0 B | glow slot by flag | `glowMapIndex` | 1,395 / 729 |
| 3 | Parallax | None | yes, 0 B | height → `parallax_map` | POM | 11 / 14 |
| 4 | Face Tint | None | yes, 0 B | tint slots, `detail_neutral` | `TINT_ALPHA_WEIGHT_BIT` | 3,158 / 2,554 |
| 5 | Skin Tint | SkinTint | yes, 12 B | `skin_tint_color` | albedo tint | 1,631 / 1,423 |
| 6 | Hair Tint | HairTint | yes, 12 B | `hair_tint_color` | multiply | 10,817 / 8,748 |
| 7 | Parallax Occ | ParallaxOcc | yes, 8 B | passes / scale | POM | 0 / 12 |
| 8 | Multitexture Landscape | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 9 | LOD Landscape | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 10 | Snow | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 11 | MultiLayer Parallax | MultiLayerParallax | yes, 20 B | all 4 fields | inner layer + strength; `refraction_scale` offsets the inner UV (#5961) | 662 / 502 |
| 12 | Tree Anim | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 13 | LOD Objects | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 14 | Sparkle Snow | SparkleSnow | yes, 16 B | `sparkle_parameters` | glint | 19 / 19 |
| 15 | LOD Objects HD | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 16 | Eye Envmap | EyeEnvmap | yes, 28 B | 3 fields; centres Y-up at import (#6214) | iris-centred cube, render-origin-relative (#6213), mip 0 (#6261) | 3,251 / 2,628 |
| 17 | Cloud | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 18 | LOD Landscape Noise | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 19 | Multitexture Landscape LOD Blend | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 20 | FO4 Dismemberment | None | yes, 0 B | n/a | n/a | 0 / 0 |

`None` types read no trailing data (#4252 guard passes in the nif lib run, 1,446 / 1,446).

## Cell-Load Regression Status

- **TES5 cells through the unified walker**: checked statically only, with no engine launch. The walker delta since the baseline is FO3/FNV/FO4-specific (#6089, #6088, #6407) plus the WRLD helper refactor (61df661d8) and doc moves. `parse_real_skyrim_esm` (finds `SolitudeWinkingSkeever`, 92-byte XCLL) is green. The new Skyrim FACT crime decode is SKY-D4-01.
- **Spawn-time animation**: LC-2026-10-27-D2-01 (cited) means Skyrim doors, containers and activators whose first sequence is not Idle load in that first pose.
- **Exteriors**: EXT-D6-2026-10-27-01 (cited, CRITICAL) panics at the first boundary terrain-LOD block. Skyrim exterior loads are exposed until it is fixed.
- **Whiterun BanneredMare control bench**: not measured (suite rule: no engine launch). #5959 is still open. The DLC repro (`Forelhost01`) and `m41-equip.sh skyrim` were not run.

## Tests run (repo `target/`)

| Command | Result |
|---|---|
| `cargo test --release -p byroredux-nif --test parse_real_nifs -- --ignored skyrim` | SE 33,468/33,468 clean (8 archives, 6 optional); LE 22,466/22,466 clean |
| `… --test per_block_baselines -- --ignored skyrim` | SE + LE OK |
| `… --test block_coverage_baselines -- --ignored unknown_ceiling_skyrim_se` | 856,103 blocks, 0 `NiUnknown` |
| `… --lib -- --ignored packed_sse_indices_match_partition_palette_expansion_on_real_data` | 19,606 lanes, 0 changed |
| `cargo test --release -p byroredux-nif --lib` | 1,446 passed, 1 ignored |
| `cargo test --release -p byroredux-plugin` (4 Skyrim real-data tests, one at a time, `--exact`) | BODT masks, `parse_real_skyrim_esm`, WNAM census, `ms01_eltrys_authored_placement`: all ok |
| `cargo test --release -p byroredux-bsa --test bsa_real -- --ignored --exact …` | SE census 23 / 172,961, 0 errors; Meshes0 brute force 18,862, 0 errors; LE 9 / 132,755, 0 errors, but only with the env var set (SKY-D5-01) |
| `cargo test -p byroredux --bin byroredux -- prebaked facegen npc_spawn ragdoll_pose skyrim eye_pack source_shader_type loose_override archive_siblings` | 267 passed, 24 ignored |

Censuses (Python byte walk over the SE masters; scripts in the session scratchpad): NPC_ template bits (SKY-D3-01) and FACT CRVA sizes (SKY-D4-01).

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-27.md` (label each finding `game:skyrim` + `legacy-compat`, plus `doc-rot` / `test-gap` / `esm-plugin` / `nif` as fits).
