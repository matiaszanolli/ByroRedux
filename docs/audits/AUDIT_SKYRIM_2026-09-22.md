# Skyrim (SE + LE) Compatibility Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-09-11.md` (HEAD `f74f8f68a`) · **Audited**: Dim 1 (BSTriShape packed geometry + SSE reconstruction), Dim 2 (shader-type dispatch + Skyrim material slice), Dim 3 (NPC equip + FaceGen), Dim 4 (multi-master load order + TES5 cell-load regression), Dim 5 (archives + corpus gates) · **Unchanged since baseline (skimmed)**: none — every dimension had commits since 09-11 and was analysed in full; none required re-doing scratch work (recovered intact from a prior killed run and re-verified against current HEAD, see Methodology)

**Scope note**: the skill's dimension count dropped from 7 (09-11 report) to 5 this cycle — the former Dimension 6 ("Specialty Blocks + Real-Data Rendering") folded into Dimension 1's checklist, and the former Dimension 7 (NIFAL canonical material) is now `/audit-nifal`'s territory, with this skill's Dimension 2 only covering "Skyrim's data through" that shared mechanism. Findings that were filed under the old D6/D7 labels are tracked here where they still fall inside a current dimension's checklist (D7's glass-classifier pair maps onto current Dim 2).

## Methodology

A previous run of this audit was killed by a session restart before it could write a report. It left five complete dimension analyses at `/tmp/audit/skyrim/dim_1.md`–`dim_5.md` (each already containing its own "commits since baseline" git-log pass, checklist verification, real test runs — including several `--ignored` real-Skyrim-SE/LE-data tests — and an explicit findings section, several stating "None"/"None new"). All five were re-verified this session against current HEAD (`ee6d3fb39`) rather than redone:

- Confirmed `git log` shows no commits after each dimension's last-cited commit that touch its owned paths (the five most recent repo commits, 10:31–11:07, are ECS/concurrency access-declaration and hierarchy-walk-guard fixes in `byroredux/src/boot/schedule/`, `anim_convert.rs`, `cell_loader/water.rs`, `ragdoll.rs` — owned by `/audit-concurrency`/`/audit-ecs`, not in any Dim 1–5 path list, and pre-date the dim files' completion timestamps).
- Spot-checked `/tmp/audit/issues.json` (4,569 issues, max #4681, fetched fresh at the *original* run's start) against `gh`-equivalent state for every issue number cited in the dim files (#4249–#4252, #4255, #4256, #4392, #4421, #4086, #4628) — all states matched exactly (CLOSED/OPEN as claimed).
- Re-read `byroredux/src/helpers.rs:85-135` directly to confirm the glass-classifier fix (dim_2's central claim) is present in the code exactly as described, including the #4392 environment-map carve-out.
- Confirmed `grep -rn "remap_bs_tri_shape_bone_indices\|remap_one" crates/ byroredux/` returns zero hits (dim_1's central claim: the old palette-remap bug shape cannot be silently reintroduced).
- Confirmed all four sibling-audit reports the dim files cite exist and say what's claimed: `AUDIT_GAMEPLAY_2026-09-21.md` (GAME-D4-2026-09-21-02), `AUDIT_AUDIO_2026-09-22.md` (AUD-2026-09-21-D5-01), `AUDIT_NIF_2026-09-21.md` (NIF-D3-2026-09-21-02), and the task-provided facts (`AUDIT_NIFAL_2026-09-21b.md` NIFAL-D1-2026-09-21b-01, `AUDIT_EXTERIOR_2026-09-21.md` EXT-D5-2026-09-21-01, `AUDIT_CHARACTER_2026-09-21.md` player-pool confirmation).

No claim required correction. All five scratch files are reused verbatim below.

---

## Executive Summary

Skyrim SE remains ByroRedux's renderer **control bench** (Whiterun BanneredMare — both loose-mesh and full-cell rendering work), so this cycle is regression coverage against the 09-11 baseline plus the Skyrim-specific risk surface the skill names. **Result: clean.** Every one of the 09-11 baseline's 9 findings that falls inside this cycle's 5-dimension scope is now either fixed-and-verified or unregressed-and-already-tracked; **zero new or regressed findings** were produced this session.

- **Dimension 1** (BSTriShape + SSE reconstruction): the one prior LOW (particle-data trailing-read gate) is fixed and verified (`#4249`, closed 09-12). The skill's explicitly-flagged regression trap — reintroducing a partition-palette remap on the already-global packed bone-index buffer — is confirmed structurally impossible today (the old remap functions are fully deleted, not just unused). A real-data regression test against live Skyrim SE Draugr/body/hands/FaceGen geometry passed fresh this session.
- **Dimension 2** (shader-type dispatch + material slice): all three prior LOWs fixed (`#4250`–`#4252`, closed 09-12). The prior HIGH — the NIFAL glass classifier silently overwriting authored Skyrim shader types (ice MultiLayerParallax, mirror EnvironmentMap, glowing gems) — is fixed in two stages (`#4255` then the completing `#4392`, both closed 09-12/09-14) and re-verified directly against current `byroredux/src/helpers.rs`, including a `#4392`-discovered complication (EnvironmentMap kind 1 needed a carve-out for Skyrim's alchemy-lab glass) that the fix already handles. The structural root cause (`SKY-D7-2026-09-11-02`, MEDIUM) — `ImportedMaterial.shader_type` never crossing the NIFAL boundary — remains open and unregressed as **#4256**.
- **Dimension 3** (NPC equip + FaceGen): 0 findings, unchanged from baseline. Two items the skill cites as already-fixed facts (#4086 template-chain resolution, #4421 FaceGen tint scoping) reconfirmed in code with passing tests, one against real Skyrim SE data (Bleak Falls Barrow Draugr). The one live defect touching this dimension's territory — the P2 Draugr combat tail (attack/hit/death takes, sounds) never firing on Skyrim because its marker is only inserted on the Oblivion/FO3NV runtime-FaceGen spawn path — is already fully reported by `AUDIT_GAMEPLAY_2026-09-21.md` (GAME-D4-2026-09-21-02, MEDIUM) and `AUDIT_AUDIO_2026-09-22.md` (AUD-2026-09-21-D5-01), cited here rather than duplicated.
- **Dimension 4** (multi-master load order + TES5 cell-load): 0 findings. The cycle's largest change in scope, Skyrim-LE auto-detection (`66fff31bf`, new `GameProfileEntry::releases`/`for_data_dir` + a TES4-record-version HEDR disambiguator that separates Skyrim LE's 0.94/40 from FO3 GOTY's identical-looking header), is new well-tested infrastructure verified against the real on-disk LE install, not a regression. The control-bench guard (Whiterun entity count/FPS) remains INCOMPLETE — no Vulkan device in this sandbox — with no code-path reason found to expect drift.
- **Dimension 5** (archives + corpus gates): 0 findings. `fb8173fe0` extended `crates/hkx` to accept Skyrim LE's 32-bit Havok packfile layout alongside SE's 64-bit one; verified both decode to the same rig/clips against real installed SE and LE data. The one red gate in scope, `per_block_baseline_skyrim_se`, is confirmed (both by this dimension and independently by `AUDIT_NIF_2026-09-21.md`, NIF-D3-2026-09-21-02) to be game-data drift from the 2026-09-02 archive rewrite, not a parser regression — already tracked as **#4628** (OPEN, LOW), not re-reported.

**Cross-cutting facts affecting Skyrim, owned by sibling audits and cited per this run's dedup instructions (not re-reported):**
- `AUDIT_NIFAL_2026-09-21b.md` (NIFAL-D1-2026-09-21b-01, **HIGH**) — Skyrim's `.btr` distant-terrain normal maps are authored model-space (`ImportedMaterial.model_space_normals = true` is in hand at import) but the `.btr` spawner lowers them texture-only and binds through the tangent-space shader path, discarding the authored bit. Affects every Skyrim exterior's distant terrain.
- `AUDIT_EXTERIOR_2026-09-21.md` (EXT-D5-2026-09-21-01, **HIGH**) — Skyrim/FO4 WATR per-layer noise-wind angles are decoded ~90° rotated from the same record's own `NAM0` current frame; `#4544` attenuated the visible symptom without fixing the root decode.
- `AUDIT_CHARACTER_2026-09-21.md` — Skyrim player Health/Magicka/Stamina pools (100/100/100, NordRace 50 + ACBS +50 each) confirmed correct.
- `AUDIT_AUDIO_2026-09-22.md` (AUD-2026-09-21-D5-01) — corrects `AUDIT_GAMEPLAY_2026-09-21.md`'s claim that the player swing sound survives the Draugr combat-marker gap: the swing push happens before the same early-return that skips the drain, so on Skyrim **no** P2 combat sound fires at all, not even the swing.

**Total NEW findings this session: 0.** (0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW.) **Matched-existing (re-verified, still open, not re-reported): 2** — #4256 (MEDIUM) and #4628 (LOW). **Prior findings closed since baseline: 6** (#4249–#4252, #4255+#4392 as one HIGH).

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Result: 0 new findings.** All five checklist items (packed-bone-index global-buffer discipline, SSE reconstruction axis/tangent convention, `BSLODTriShape`≠`BSMeshLODTriShape` dispatch, alpha-property cascade, specialty-block parsers) verified correct against current code and passing tests.

- Commits since baseline touching this dimension: `ab8779a52` (perf-only tangent-buffer presizing, correctly gated), `8c834e0be` (SSE geometry/skin-index refactor — the fix for checklist item 1), `e3131f5ef` (trivial accessor-trait swap), `797e82124` (closes all 4 prior D1/D2 LOWs).
- Tests run: `cargo test -p byroredux-nif --lib -j4 -- bs_tri_shape sse_recon sse_skin_geometry tangent_convention alpha_flag bs_lod_tri_shape` → 103 passed; `mesh::sse_skin_index_space_tests -- --ignored` (real Skyrim SE `Meshes0/1.bsa`, Draugr male/female, body/hands, one FaceGen head) → 1 passed, >1000 weighted lanes checked, 0 changed; `-- bs_lod_tri_shape tri_shape_skin_vertex dispatch_tests` → 123 passed.

#### SKY-2026-09-11-D1-01 (was: SKY-D1-2026-09-11-01) — particle-data trailing-read BSVER gate
- **Severity**: LOW
- **Dimension**: 1 — BSTriShape packed geometry
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:659`
- **Status**: Existing: #4249 — **CLOSED, fix verified in code**
- **Description**: Was gated on `bsver < FALLOUT4` (a broad range) where `nif.xml` gates the field on BSVER exactly 100 (`#BS_SSE#`). Now reads `if stream.bsver() == crate::version::bsver::SKYRIM_SE`, matching the spec exactly.
- **Evidence**: direct code read, line cited above; issue #4249 confirmed CLOSED in `/tmp/audit/issues.json` and re-verified against current source.
- **Impact**: was limited to modded/backported/synthetic geometry with a mis-detected BSVER header; no vanilla content reached it (Skyrim LE ships classic `NiTriShape`, not `BSTriShape`).
- **Related**: none open.
- **Suggested Fix**: none — already applied.

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice (NIFAL)

**Result: 0 new findings; 1 matched-existing (MEDIUM, unregressed).** All checklist items verified: full numeric shader-type dispatch table (59 wire-level tests), FO76 `BSShaderType155` non-cross-contamination, unauthored-fields-stay-unauthored (#4393), PBR lobe unreachable for vanilla Skyrim (no code change since baseline), `EmissiveSource::Lighting` routing for `emissive_multiple`, detail/tint darkening fixes (#4422/#4423) present, and the shared `legacy_properties.rs` clamp-mode/parallax re-latch (#4401, new since baseline, Skyrim-reachable) tested clean.

- Commits since baseline (Skyrim-relevant): `4a8875b39` (#4255, first glass-classifier fix attempt), `d776d37e1` (#4392, completing fix), `c9447bd94` (#4393), `730dcc935` (#4401), plus several FO3/FO4/Starfield/Oblivion-only commits confirmed not Skyrim-reachable.
- Tests run: `-- shader_type_2 skyrim` → 59 passed; `-- shader_type_data_tests emissive_source_tests lighting_shader_pbr_tests` → 35 passed; `byroredux --bin byroredux -- helpers::glass_classification_tests` → 27 passed; `-- legacy_property_precedence_tests` → 12 passed.

#### SKY-2026-09-11-D2-01/02/03 — env_map_scale defaults, consumed-latch, test-coverage gap
- **Severity**: LOW (×3)
- **Dimension**: 2 — Shader-type dispatch
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs`, `shader_data.rs`
- **Status**: Existing: #4250, #4251, #4252 — **all CLOSED, fixes verified**
- **Description**: (01) Skyrim's effect-shader import wrote a placeholder `env_map_scale = 0.0` that disagreed with `MaterialInfo`'s own `1.0` default — fixed by `#4393`: `skyrim_effect_shader_placeholder_env_map_scale_does_not_reach_material_info` now passes. (02) The `#2328` consumed-latch was only honoured by legacy writers, not the two Skyrim+ dedicated writers — fixed. (03) Only `shader_type = 0` of thirteen no-trailing-data types had a wire-level byte-position pin — closed by `#4252`'s `parse_bs_lighting_remaining_no_trailing_shader_types_consume_nothing_extra`.
- **Evidence**: tests cited above, all passing; issues confirmed CLOSED.
- **Impact**: was latent (affected meshes short-circuit before the field is read) / test-gap only; no live defect at any point.
- **Related**: none open.
- **Suggested Fix**: none — already applied.

#### SKY-D7-2026-09-11-01 — glass classifier overwriting authored Skyrim shader types
- **Severity**: HIGH
- **Dimension**: 2 — Shader-type dispatch / material slice
- **Location**: `byroredux/src/helpers.rs:85-135` (`classify_glass_into_material`), called from `byroredux/src/material_translate.rs:649`
- **Status**: Existing: #4255 (first fix) + #4392 (completing fix) — **both CLOSED, fix verified directly against current code**
- **Description**: `classify_glass_into_material` protected only the engine-synthesized `material_kind >= 100` range; every authored low-range Skyrim shader type (`0..=20`) fell through to a bare keyword+coverage+metalness heuristic that could silently promote it to `MATERIAL_KIND_GLASS` with no way back — confirmed reachable on `MultiLayerParallax` ice surfaces, glowing soul gems, and (via `is_mirror_pane`) authored `EnvironmentMap` mirrors. The fix introduces `lit_carrier_authored_dispatch = (2..=20).contains(&material.material_kind) && !from_bgsm` and narrows the keyword list instead of the guard for the one population (ordinary Skyrim alchemy-lab glass, authored as `EnvironmentMap` kind 1) that legitimately needs the keyword path — see the code comment's 22,047-NIF SE census (kind 0: 29, kind 1: 80, kind 2: 1, kind 11: 7).
- **Evidence**: `byroredux/src/helpers.rs:85-135` read directly this session; matches the dim_2 description field-for-field, including the `#4392` EnvironmentMap carve-out. `helpers::glass_classification_tests` (27 tests, incl. `multi_layer_parallax_ice_surface_survives_glass_keyword`, `gem_keyword_on_non_default_shader_type_is_not_reclassified`, `mirror_pane_heuristic_does_not_clobber_authored_environment_map`, `environment_map_alchemy_glass_classifies_as_glass`, `environment_map_ice_is_not_glass_kind`) all pass.
- **Impact**: was per-title, whole-exterior blast radius (every ice surface, mirror, and glowing gem on Skyrim); now closed.
- **Related**: SKY-D7-2026-09-11-02 (#4256) below — same root cause, still open.
- **Suggested Fix**: none — already applied.

#### SKY-D7-2026-09-11-02 — `ImportedMaterial.shader_type` never crosses the NIFAL boundary
- **Severity**: MEDIUM
- **Dimension**: 2 — Shader-type dispatch / material slice
- **Location**: `crates/nif/src/import/types.rs:815` (`ImportedMaterial.shader_type: u32`), `byroredux/src/material_translate.rs:486-647`
- **Status**: Existing: #4256 — **OPEN, verified still accurate, unregressed**
- **Description**: Only the per-variant *payload* (`shader_type_fields`) crosses the NIFAL boundary into `Material`; the discriminator that identifies which shader-type variant it belongs to does not (`grep -n "source.shader_type\b" byroredux/src/material_translate.rs` → zero hits). This is the structural root cause the HIGH above worked around (via the `from_bgsm` provenance signal + a narrowed keyword list) rather than resolved — no canonical field retains the original authored shader-type provenance once `material_kind` is reassigned.
- **Evidence**: `crates/nif/src/import/types.rs:815` and the grep above, re-run this session with the same zero-hit result as dim_2 recorded.
- **Impact**: forces at least one downstream consumer (`TextureSlotContext` at cell-spawn, per the 09-11 report) to read back into the raw `ImportedMaterial` tier directly — a NIFAL single-boundary violation in the making, currently masked by the HIGH's workaround.
- **Related**: SKY-D7-2026-09-11-01 (#4255/#4392, closed).
- **Suggested Fix**: add an explicit canonical `source_shader_type` field on `Material`, distinct from the engine-dispatch `material_kind`, and let `classify_glass_into_material` key off it directly instead of the `from_bgsm` proxy.

### Dimension 3 — NPC Equip + FaceGen (M41)

**Result: 0 findings**, unchanged from baseline. All five checklist items (armor chain / race-skin-first + occupancy filter, template chain #4086, FaceGen pre-baked + tint scoping #4421, `BSDismemberSkinInstance` partition data, LVLI single/multi-pick) reconfirmed against current code with passing tests, including a real-data test against live Skyrim SE Bleak Falls Barrow Draugr ARMA/ARMO chain (`real_skyrim_bleak_falls_draugr_expands_to_one_weapon_leaf`, 1 passed).

- Tests run: `equip_template_tests equip::` → 56 passed; real-data ignored test → 1 passed; `scene::nif_loader::tests` → 6 passed; `npc_spawn::` → 84 passed, 9 ignored.
- No findings filed. One cross-reference, not re-reported per this dimension's "cite rather than re-derive" instruction: the P2 Draugr combat tail (attack/hit/death takes, sounds) never fires in production because its marker is inserted only on the Oblivion/FO3NV runtime-FaceGen spawn path, never on the prebaked-FaceGen path every Skyrim actor uses — fully reported as `AUDIT_GAMEPLAY_2026-09-21.md` GAME-D4-2026-09-21-02 (MEDIUM) and corrected/extended by `AUDIT_AUDIO_2026-09-22.md` AUD-2026-09-21-D5-01 (no P2 sound fires at all, not even the swing).

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

**Result: 0 findings**, unchanged from baseline. The cycle's largest change in scope is new infrastructure, not a regression:

- `66fff31bf` (09-13) adds `GameProfileEntry::releases`/`GameRelease`/`for_data_dir` and a TES4-record-version-gated `GameKind::from_header` disambiguator that separates Skyrim LE's HEDR 0.94/master-version-40 (which is bit-identical in float value to FO3 GOTY's HEDR) from FO3 GOTY, using `(40..100).contains(&record_version)`. This is exactly the "LE is selected by the files on disk, not a flag" mechanism the skill's Game Context table documents; it did not exist at the 09-11 baseline. Verified against the real on-disk LE install (`/home/matias/Games/skyrim-original/.../Data/`, un-numbered `Skyrim - *.bsa` set matching `assets/debug_profiles.toml`'s `[[profiles.skyrim_se.releases]]` block verbatim) and consumed at all three real launch/validate sites (`byroredux/src/boot/cli.rs:389`, `studio_host.rs:556`, `crates/debug-server/src/evaluator.rs:148`), not just a health check.
- `efe15ceaf` (09-18) game-gates the XCLL ≥92-byte ambient-cube arm to Skyrim/FO4/FO76-era games; Skyrim's own `XCLL_SIZES_SKYRIM = &[28, 92]` decode is confirmed unchanged.
- Tests run: `byroredux-game-detect --lib` → 45 passed; `game_kind_from_header esm::reader::tests` → 42 passed; `xcll xezn` → 22 passed (2 ignored, non-Skyrim); `esm::cell` → 172 passed (12 ignored); `parse_real_skyrim_esm --ignored` (real `Skyrim.esm`, 590 cells, 18,244 statics, 37 worldspaces) → 1 passed; `cell_loader::load_order` → 14 passed; real-data load-order test → 1 passed.
- **Control-bench guard: still INCOMPLETE, not FAILED.** No Vulkan device available in this sandbox to physically re-run `--bench-frames 300 --bench-hold` against WhiterunBanneredMare. No commit since baseline touches STAT/REFR/WEAP/ARMO/LIGH resolution or LAND heightmap scale for that cell; both dimension-relevant new code paths (XCLL game-gate, LE release detection) preserve Skyrim's existing behavior by construction and are independently unit-tested against real data above. This gap is owned by `/audit-runtime`'s baseline harness, not resolvable from a source-only audit.

### Dimension 5 — Archives + Corpus Gates

**Result: 0 findings**, unchanged from baseline. `fb8173fe0` (09-14) extends `crates/hkx` to accept both Skyrim LE's 32-bit and SE's 64-bit Havok packfile pointer layouts, adding a Skyrim-LE `Game` variant and the `skyrim_le.tsv` per-block baseline the skill's checklist cites. Verified against real installed SE and LE data: `animation::tests::skyrim_cart_player_idle_decodes_when_assets_are_available` and `skyrim_le_packfiles_decode_to_the_same_rig_and_clips_as_se` (both `--ignored`, real data) → 2 passed, confirming 32-bit LE and 64-bit SE packfiles decode identically.

- `parse_real_nifs -- --ignored skyrim` → `parse_rate_skyrim_se` and `parse_rate_skyrim_le` both 100.00% clean (SE: 33,468 NIFs across 8 archives incl. 4 present-only Creation Club + Animations; LE: 22,466 NIFs), 0 truncated, 0 failed.
- `siblings_skyrim_zero_start_offers_1_through_9` → 1 passed.
- `byroredux-bsa --lib` → 95 passed, 0 failed.

#### SKY-2026-09-22-D5-01 (per_block_baseline_skyrim_se drift)
- **Severity**: LOW
- **Dimension**: 5 — Archives + corpus gates
- **Location**: `crates/nif/tests/data/per_block_baselines/skyrim_se.tsv`
- **Status**: Existing: #4628 — **OPEN, verified premise still holds, not re-reported as new**
- **Description**: `per_block_baseline_skyrim_se` FAILS this session (`PARSED shrank BSDynamicTriShape 21140 -> 21054`), while `per_block_baseline_skyrim_le` passes (146 types matched). Both this dimension's own run and `AUDIT_NIF_2026-09-21.md` (NIF-D3-2026-09-21-02) independently confirm this is not a parser defect: a parser-independent header recount shows an exact 1:1 swap with `BSTriShape` (+86), same combined total (856,103), consistent with the Skyrim SE archives on disk being rewritten 2026-09-02 (five days after the 2026-08-27 baseline regen that produced the checked-in `.tsv`) — exactly the archive-rewrite caveat this audit run's brief flags in advance.
- **Evidence**: fresh test run this session (see above); cross-corroborated by an independent audit leg (`AUDIT_NIF_2026-09-21.md`) using a different method (header recount vs. parser run).
- **Impact**: cosmetic CI-gate red; the corpus still parses 100% clean (`parse_rate_skyrim_se`), only the per-block-type distribution shifted with the on-disk data.
- **Related**: NIF-D3-2026-09-21-02 (same root cause, `/audit-nif`'s territory).
- **Suggested Fix**: regenerate the `skyrim_se.tsv` baseline against the current installed archive set (already the suggestion on file for #4628).

---

## Shader-Type Coverage Matrix

`ShaderTypeData` variants (`crates/nif/src/blocks/shader.rs`) × parse / import / render completeness for Skyrim (BSVER 83/100). Unchanged from the 09-11 baseline except that the "at risk of being overwritten" caveats on `EnvironmentMap` and `MultiLayerParallax` are now resolved (SKY-D7-2026-09-11-01 fixed):

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh`) | Render (`triangle.frag` / RT) |
|---|---|---|---|---|
| `None` | 0,2,3,4,8,9,10,12,13,15,17,18,19,20 | Complete (zero-byte, verified no over-read) | N/A | Default-lit |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed; no longer at risk of glass-classifier overwrite (fixed #4392) |
| `SkinTint` | 5 | Complete | Complete | Consumed |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete (payload fields survive) | Consumed; ice-surface dispatch preserved through glass classifier (fixed #4255) |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| `Fo76SkinTint` | (FO76 numeric 4, distinct table) | N/A for Skyrim (separate `parse_shader_type_data_fo76`, structurally unreachable from Skyrim BSVER) | — | — |

Residual gap: `ImportedMaterial.shader_type` (the discriminator) still does not cross the NIFAL boundary (#4256, MEDIUM, open) — the payload does, but downstream consumers that need the raw authored type (e.g. `TextureSlotContext` at cell-spawn) read back into the pre-boundary tier directly.

Cross-cutting: distant-LOD (`.btr`) normal maps are a separate texture-role bug, not a `ShaderTypeData` dispatch gap — see NIFAL-D1-2026-09-21b-01 above (owned by `/audit-nifal`/`/audit-exterior`, not re-tabulated here).

---

## Cell-Load Regression Status

- TES5 cells parse through the unified `esm/cell/` walker; compressed GRUPs decompress correctly. `parse_real_skyrim_esm` passes fresh this session against real `Skyrim.esm`: 590 cells, 18,244 statics, 37 worldspaces; `SolitudeWinkingSkeever` resolves with 981 refs and populated extended lighting (92-byte XCLL on all 590/590 cells) — identical shape to the 09-11 baseline, confirming no collapse.
- Multi-master remap, ESL/light-master decode, and both tiers of deleted-REFR tombstone handling all re-verified against real data and the full `esm::cell`/`load_order` suites (172+14 passed this session across two crates, 0 failed).
- Skyrim LE is now auto-detected from on-disk archive names (`66fff31bf`, new since baseline) via a TES4-record-version HEDR disambiguator, replacing what the 09-11 report implied was manual configuration — verified against the real installed LE prefix.
- **Whiterun BanneredMare control-bench guard: still INCOMPLETE, not FAILED** — no Vulkan/display device available to any dimension this session (same limitation as 09-11). No code-path reason found to expect the ROADMAP-recorded entity-count/FPS figures to have moved: no commit since the last bench-of-record refresh touches STAT/REFR/WEAP/ARMO/LIGH resolution or LAND heightmap scale for that cell. A real `--bench-frames 300 --bench-hold` run is the only way to close this out; out of scope for a source-only sandboxed audit.

---

## Total Findings: 0 new/regression, 2 matched-existing, 6 fixed-since-baseline

| Severity | New | Regression | Matched-existing (re-verified, still open) | Fixed since baseline |
|---|---|---|---|---|
| CRITICAL | 0 | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 | 1 (SKY-D7-2026-09-11-01, #4255+#4392) |
| MEDIUM | 0 | 0 | 1 (#4256) | 0 |
| LOW | 0 | 0 | 1 (#4628) | 5 (#4249, #4250, #4251, #4252, and SKY-2026-09-11-D6-02/03 — the latter two owned by the now-folded D6, not re-verified this cycle since outside current dimension scope) |
| **Total** | **0** | **0** | **2** | **6** |

**By dimension:**

| Dimension | New | Matched-existing | Fixed since baseline |
|---|---|---|---|
| 1 — BSTriShape packed geometry | 0 | 0 | 1 (#4249) |
| 2 — Shader-type dispatch | 0 | 1 (#4256, MEDIUM) | 4 (#4250, #4251, #4252, #4255+#4392) |
| 3 — NPC equip + FaceGen | 0 | 0 | 0 |
| 4 — Multi-master load order | 0 | 0 | 0 |
| 5 — Archives + corpus gates | 0 | 1 (#4628, LOW) | 0 |
| **Total** | **0** | **2** | **5 issue numbers (6 findings)** |

No CRITICAL or HIGH findings this session (the one prior HIGH is now closed and verified fixed).

---

## Suggested Next Step

No new findings to publish from this leg — nothing to run `/audit-publish` against. The two matched-existing items (#4256 MEDIUM, #4628 LOW) remain open under their existing issue numbers; no action needed here. Sibling-audit HIGH findings that affect Skyrim (NIFAL-D1-2026-09-21b-01, EXT-D5-2026-09-21-01) are tracked and published under their owning audits.
