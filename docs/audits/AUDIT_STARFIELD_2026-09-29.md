# Starfield Compatibility Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_STARFIELD_2026-09-22.md` (HEAD `ee6d3fb39`) ·
**Audited**: Dimensions 1–6. Every dimension had commits since the baseline; Dims 3, 4 and 6
carried the substantive change. · **Unchanged since baseline (skimmed)**: none. Dim 1 and Dim 2
were verified with guard runs and same-day sibling measurements, not re-derived.

**Method**: A single agent, with no sub-agents (per this suite run's constraints). Scoping was
delta-first against `ee6d3fb39`. The dominant in-window change for Starfield is **b9e961eeb**. Its
title is a lighting-docs commit, but it introduced the Starfield **metre → Bethesda-unit (×70)
translation** at both boundaries: `crates/nif/src/import/units.rs` (NIF import) and
`crates/plugin/src/esm/records/spatial_units.rs` (ESM parse). This audit walked both field by field.
It verified the two leads routed by `/audit-exterior` against raw `Starfield.esm` /
`SeventySix.esm` / `Fallout4.esm` bytes, using a read-only Python GRUP walker in the session
scratchpad, and xEdit dev-4.1.6 `wbDefinitions{Common,SF1}.pas` (cached by `/audit-esm` at
`/tmp/audit/esm/xedit/`).

Constraints honoured:
- The engine binary was not launched, and neither was `--sf-smoke`.
- No `--ignored` plugin tests were run.
- No source, skill or issue edits.
- Real-data corpus figures are cited from same-day sibling measurements (`/audit-nif`,
  `/audit-parsers`, `/audit-esm`), and each citation names its source.

Guard runs (all green):

| Suite | Result |
|---|---|
| bsa `ba2` | 42/42 |
| nif `bs_geometry starfield units` | 101/101 |
| nif `starfield` | 32/32 |
| sfmaterial `--lib` | 29/29 |
| plugin `spatial_units starfield txst dat2 xcll` | 54/54 (3 ignored) |
| byroredux `starfield_mat starfield_single_channel material_translate` | 88/88 |
| byroredux `light_anim spatial interior_spawn` | 41/41 |
| byroredux `normalize_mesh_path` | 8/8 |

**Result**: **0 CRITICAL, 1 HIGH, 1 MEDIUM, 3 LOW**. All five findings are NEW, and there are
zero regressions. The last cycle's only finding (SF-2026-09-22-D4-01) is fixed (#4770).
Eight Starfield-scoped issues closed in the window were verified fixed in code: #4269, #4279,
#4282, #4283, #4429, #4439, #4440 and #4770.

---

## Executive Summary

Starfield is a first-class `GameKind`. The NIF + BA2 path holds at **100% over 120,543 NIFs, 13/13
archives**, re-measured today by `/audit-nif`, and the BA2 real-data lane (Starfield full sweep) is
green per `/audit-parsers`. CDB materials remain **presence-only** (Phase 2, #3398). Walkable
Cydonia was not re-verified this cycle because no engine launch was permitted, so #3540's frame-0
fix still has no real-device confirmation.

The cycle's real event is the **units boundary**. Starfield authors scene data in metres. Before
b9e961eeb, placements, bounds, lights and fog reached the engine at 1/70 scale, with BSGeometry
vertices carrying only the `.mesh` decoder's 69.969 tooling factor. b9e961eeb fixed this once, at
the parse and import boundaries, as the NIFAL/EXAL rule requires. This audit confirms the NIF side
is complete and consistent (Dim 2):
- vertices take ×70/69.969;
- node, skin-bind, bounds, LOD, attach, furniture, light, particle and animation translations take
  ×70;
- the #1203 skeleton solver stays in metres on both of its inputs.

The ESM side has holes. `normalize` lifts CELL/REFR/XCLL/LGTM/LIGH/SCOL/WRLD/NAVM, but not:
- **WTHR** — already filed as EXT-D1-2026-09-29-01 (HIGH, fog at ~43 m);
- **WATR** — **SF-D4-01, HIGH**. Starfield's per-metre absorption coefficients are applied per BU,
  which makes water opaque within ~30 cm. Its 100 m normal falloff flattens surfaces 1.4 m from the
  camera, and its underwater fog saturates at ~1 m.

The same era-boundary blind spot shows up twice more:
- **SF-D4-02 (MEDIUM)**: Starfield's 72-byte WTHR FNAM never decodes its power / max-opacity /
  height-fog tail, because `parse_wthr` gates the tail on FO4 | FO76.
- **SF-D4-03 (LOW, dormant)**: `parse_lgtm` reads Starfield's 108-byte LGTM with Skyrim offsets.

The two remaining LOWs are guard quality:
- **SF-D3-01**: #4429's parked-kinds XOR guard is satisfied by incidental mentions.
- **SF-META-01**: this skill's own Dim 5 first-step filter runs zero tests.

---

## Dimension Summaries

| Dim | Verdict | Notes |
|---|---|---|
| 1 BA2 v2/v3 + corpus | clean | 42/42. #4662/#4670/#4671 guards added. Corpus 100% (120,543) same-day per `/audit-nif`. PAR-D2-2026-09-29-01 (debug-build DX10 panic) is `/audit-parsers`' and is cross-referenced. |
| 2 BSGeometry | clean | #4269 closed and verified (`MeshTrailer::Required` for inline bodies). Units + basis move verified consistent. #4268 still open. NIF-D4-2026-09-29-01 (doc rot) is `/audit-nif`'s. |
| 3 CDB | 1 LOW | #4657 nesting cap verified. #4283 provenance verified. #4429 closed, guard weak (D3-01). |
| 4 ESM + bring-up | 1 HIGH, 1 MEDIUM, 1 LOW | `normalize` coverage walk. WATR (D4-01), WTHR tail (D4-02), LGTM layout (D4-03). #4770 verified. |
| 5 NIF shader blocks | 1 LOW (meta) | #4439 and #4279 verified. CRC32 table unchanged. Skill first-step filter vacuous (META-01). |
| 6 Material flow | clean | #4283 split + #4855 dispatch guard verified at the boundary. #4441 still open and accurate. #4837 still open. |

Full per-dimension detail: `/tmp/audit/starfield/dim_{1..6}.md`.

---

## Findings (by severity)

### SF-2026-09-29-D4-01: Starfield WATR distance and inverse-distance fields are metric but reach WATAL unconverted — Beer–Lambert absorption runs 70× too strong, normals flatten 1.4 m from the camera, underwater fog saturates at ~1 m
- **Severity**: HIGH
- **Dimension**: ESM → cell bring-up (WATAL handoff)
- **Location**: `crates/plugin/src/esm/records/spatial_units.rs:65-125` (`normalize`, no
  `index.waters` arm); `crates/plugin/src/esm/records/misc/water.rs:1270-1392`
  (`decode_dnam_starfield`, shared with FO76 via `decode_dnam_fo76`); consumers
  `crates/renderer/shaders/water.frag:529-590` (`absorbWaterColumn`), `:865-872` (noise falloff),
  `:825-826` (noise UV), and `byroredux/src/systems/water.rs:566-620` (`compute_underwater_params`,
  `underwater_color_at_depth`)
- **Status**: NEW. The exterior audit left it as an unverified sibling of EXT-D1-2026-09-29-01;
  this audit verifies it with data. Distinct from #4837, which is the oceanness ÷20 lane.
- **Description**: b9e961eeb moved Starfield scene data into engine units (70 BU/m) at the parse
  boundary. The WATR record was left out. Starfield's WATR DNAM is authored in metres, and every
  distance-bearing lane reaches `WaterMaterial` as metres, where BU are expected:
  - **`absorption_coefficients` (DNAM 4/8/12)** are per-metre extinction coefficients
    (0.16558 / 0.09624 / 0.07627 on 14 of 15 vanilla WATRs; `WaterSulfuric` authors 0.3 / 0.075 / 0.01). They are inverse lengths, so the fix
    is ÷70, not ×70. `water.frag` applies `exp(-hitDist * coeff)` with `hitDist` in BU, and
    `underwater_color_at_depth` does the same with `depth` in BU.
  - **`noise_falloff` (DNAM 132)** is 100 on all 15 vanilla records, meaning 100 m. `water.frag`
    fades normals to flat by `1 - dist/noiseFalloff`, so every Starfield water surface loses its
    normal detail 100 BU (1.4 m) from the camera. The decoder comment says "the same 300-unit
    default across the three", but the real value is 100.
  - **`noise_uv_scale_{a,b,c}` (DNAM 120/124/128), not settled**: these are tile sizes of
    72.11 / 39 / 13. `normalize_noise_uv_scale` inverts them, and `uvWorld = vWorldPos.xz` is in
    BU, so as shipped the primary noise repeats every 72 BU (~1 m). Read as metres, the tile would
    be 5,048 BU. FO76 (BU, same decoder) authors 279 / 168 / 56, so BU and metric readings are
    3.9× and 18× off FO76 respectively. The data alone cannot decide this, so settle it with a
    capture before lifting (per *feedback_no_guessing*).
  - **`underwater_fog_near/far` (DNAM 40/44)** are −150 / 75 m. Clamped, that becomes 0 / 75, so
    the underwater fog saturates 75 BU (≈1 m) from the camera. FO76 authors −9,000 / 850 BU and FO4
    authors −6,000 / 1,100.
  - `depth_amount` (DNAM 0; 8 m vs FO4 471–1,087 BU) is captured and packed into `optical.x`, but
    no shader reads it today. It only needs the lift for consistency.
- **Evidence**: A read-only raw DNAM dump of all 15 `Starfield.esm` WATRs (e.g. `WaterClear`
  0x18: `8 | 0.16558 0.096239 0.076271 | … | -150 75 | … | 72.1141 39 13 | 100 100 100 | 1 0.08`)
  compared with `SeventySix.esm` (`-9000 850`, falloff `4096`). The absorption triplet is the strongest unit
  evidence: its red > green > blue order and per-metre magnitude match liquid-water extinction.
  Read per BU (1.43 cm), it would be ~11.6 m⁻¹ for red, which is opaque at hand depth. Transmission of the
  red channel through 70 BU (1 m) of water is `exp(-70·0.16558)` = **9.3e-6 as shipped**, against
  0.847 with the per-metre coefficient applied per metre. Through 20 BU (28 cm) it is 0.037 against
  0.954. There is no Starfield branch in `resolve_water_material` (`env_translate.rs:1053`) and no
  `BETHESDA_UNITS_PER_METER` use in the WATAL path.
- **Impact**: Every Starfield water body turns opaque deep tint within ~30 cm of depth, normals go
  flat beyond ~1.4 m, and the camera sees ~1 m of underwater visibility. The noise UV tile is
  unsettled (see above). This covers cell water (XCLW/XCWT), placed-water meshes
  (`apply_placed_water_type`), since `cell_loader/water.rs` has no game gate. It is latent only
  in the sense that no gate exercises a Starfield water scene (`m-exteriors.sh` skips Starfield).
- **Related**: EXT-D1-2026-09-29-01 (same root cause, WTHR); #4837 (open, Starfield oceanness
  ÷20); #4285 (closed, WATAL concentration normalisation); memory *watal_water_layer*.
- **Suggested Fix**: Add an `index.waters` arm to `spatial_units::normalize`, which is already
  `GameKind::Starfield`-gated, so FO76's shared decoder stays in BU. It should multiply
  `underwater_fog_near/far`, `noise_falloff` and `depth_amount` by 70, and divide
  `absorption_coefficients` by 70. Decide the noise UV tile sizes separately, with a capture. Pin the lift with a `spatial_units_tests` case carrying the vanilla `WaterClear` DNAM. Fix the
  "300-unit default" comment.


### SF-2026-09-29-D4-02: Starfield WTHR FNAM power / max-opacity / height-fog tail is never decoded — `parse_wthr` gates it on FO4 | FO76 only
- **Severity**: MEDIUM
- **Dimension**: ESM → cell bring-up (EXAL weather handoff)
- **Location**: `crates/plugin/src/esm/records/weather.rs:504-533` (FNAM arm of `parse_wthr`)
- **Status**: NEW. It appeared as a routed "secondary" inside EXT-D1-2026-09-29-01's Related
  field and was not filed on its own.
- **Description**: The FNAM arm reads the four fog distances for every non-Skyrim game. The
  power/max pair is gated on `matches!(game, Fallout4 | Fallout76) && len >= 32`, and the ten
  height-fog floats on the same match with `len >= 72`. xEdit's shared `wbWeatherFogDistance`
  (`wbDefinitionsCommon.pas:9795-9846`) gives power for `> gmTES4R`, max for `> gmFNV`, and the
  height block under `IsFO4Plus` from form version 119/120. SF1 uses the same struct
  (`wbDefinitionsSF1.pas:18870`), and every Starfield FNAM is 72 bytes. Starfield therefore keeps the
  defaults `fog_day/night_power = 1.0`, `fog_day/night_max = 1.0` and `fog_height = None`.
- **Evidence**: The raw FNAM of all 3 `Starfield.esm` WTHRs: `DefaultWeather` 0x15E
  `10 3000 10 3000 | 0.4 0.4 | 0.9 0.9 | 10 120 10 120 0.05 0.05 10 220 10 900`; the same for
  `NewAtlantisWeather50` 0x27CF9B (far 220/220); `SpaceWeather` 0x249FA6 `… 1 1 0 0 0 10000 0 10000 …`.
  `translate_weather` / `weather_data_from_record` (`env_translate.rs:1163-1192`, `:1474-1489`)
  consume `fog_day_max` in `FogMedium::from_legacy_ramp` and `fog_height` in
  `with_authored_height_range`, so the authored 0.9 opacity cap and the 120 m near-height band are
  replaced by defaults. SpaceWeather's authored `max = 0` (no fog in space) becomes 1.0.
- **Impact**: Starfield exterior fog ignores its authored opacity ceiling and vertical profile. The
  space weather gets a full-opacity fog cap it explicitly authored as zero. It is masked today by
  EXT-D1-01 (fog 70× too short), and becomes the visible defect once that lands. All 21 CLMT
  `WLST` entries point at the two 3000-far weathers.
- **Related**: EXT-D1-2026-09-29-01 (units; its fix must also lift the height mids/ranges once
  decoded); SF-2026-09-29-D4-01.
- **Suggested Fix**: Extend both `matches!` guards to `Fallout4 | Fallout76 | Starfield`, or better,
  gate on the byte length that xEdit's form-version rule implies. Then have
  `spatial_units::normalize` lift the four distances and the eight height mid/range values
  (not the density scales). Add a Starfield case beside `parse_fo4_fnam_retains_all_eighteen_floats`.

### SF-2026-09-29-D4-03: `parse_lgtm` reads a Starfield 108-byte LGTM DATA with Skyrim's tail offsets, so fog-far colour, fog max and light-fade come from the height-fog block, and `normalize` then scales two dimensionless scales by 70
- **Severity**: LOW
- **Dimension**: ESM → cell bring-up
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:1285-1310` (`parse_lgtm`, `>= 92`
  arm); `crates/plugin/src/esm/records/spatial_units.rs:106-112`;
  `byroredux/src/cell_loader/load.rs:1184-1186` (comment)
- **Status**: NEW
- **Description**: `parse_lgtm` has no game parameter. For DATA ≥ 92 bytes it skips 32 bytes
  from offset 40 and reads `fog_far_color`@72, `fog_max`@76, `light_fade_begin`@80 and
  `light_fade_end`@84, which is Skyrim's layout. xEdit SF1 `LGTM.DATA`
  (`wbDefinitionsSF1.pas:13758-13790`) is the SF XCLL shape: Fog Color Far@40, Fog Max@44, Light
  Fade Start/Stop@48/52, then Height Mid/Range@60/64, High colours@68/72, High Density
  Scale@76, Fog Near/Far Scale@80/84, …. The four Starfield fields therefore come from High Far
  colour, High Density Scale, Fog Near Scale and Fog Far Scale. `spatial_units::normalize` then
  multiplies the two scales (which it takes for light fades) by 70. The height-fog block is
  dropped. `lighting_from_template`'s comment "SF volumetric height-fog fields ride on inline XCLL
  rather than Skyrim-style LGTM templates" contradicts xEdit.
- **Evidence**: All 6 `Starfield.esm` LGTMs carry a 108-byte DATA. `ShipInteriorLT` 0x6658 has
  authored Fog Max@44 = 0 and fades@48/52 = 163,840/163,840. The parser yields `fog_max` = 1.0
  (@76), `light_fade_begin/end` = 1.0/1.0 (@80/84), lifted to 70/70 BU.
- **Impact**: Dormant on vanilla. `resolve_cell_lighting` consults an LGTM only when XCLL is absent
  or its inherit mask is set, and every Starfield interior CELL carries a 108-byte XCLL with no
  inherit mask (census above). A plugin CELL with LTMP and no XCLL would get fog max 1.0 and light
  fade 70 BU (lights culled beyond 1 m).
- **Related**: SF-2026-09-29-D4-01/-02 (same Starfield-layout-at-the-boundary class); #1579 (the
  XCLL analogue, fixed).
- **Suggested Fix**: Pass `GameKind` into `parse_lgtm` and decode the Starfield DATA with the same
  offset table as the SF XCLL arm (`walkers.rs:495-560`). Populate `starfield` height fog on the
  template, lift it in `normalize`, and correct the `load.rs` comment.

### SF-2026-09-29-D3-01: #4429's XOR guard counts any backticked mention anywhere in nifal.md as "parked", so a kind can leave the parked list with no struct role and the guard still passes
- **Severity**: LOW
- **Dimension**: CDB Material Database
- **Location**: `byroredux/src/asset_provider/tests/starfield_mat.rs:223-260`
  (`starfield_single_channel_kinds_are_parked_by_name_or_canonical`); `docs/engine/nifal.md:623-647`
- **Status**: NEW (hardening of the aa1b53c14 / #4429 guard)
- **Description**: The guard's doc says it fails on "neither", meaning a kind dropped from the
  parked table without a struct field. But `parked` is
  `NIFAL_SRC.contains("`{suffix}`")` over the whole of nifal.md. The "parked table" is one prose
  paragraph, and four of the five suffixes appear there more than once. The kind list is at line
  625, the TXST slot map at 627, and the `smooth_spec` sign-flip rationale at 632. Mention counts
  are `_rough` 3, `_metal` 2, `_ao` 2, `_opacity` 2 and `_transmissive` 1.
- **Evidence**: If Phase 2 edits line 625 to drop `_rough` from the kind list without adding
  `pub roughness: T,`, the guard stays green, because line 627 (`TX09 → `_rough``) and line 632
  (`routing `_rough` there`) still satisfy `contains`. The one-hit case (`_transmissive`) is the
  only kind the guard pins as described.
- **Impact**: Test coverage only; no runtime effect today (zero Starfield roles are produced).
  The guard exists to stop Phase 2 from quietly dropping 39% of Starfield's texture kinds, and
  for four of five kinds it cannot see a drop from the list.
- **Related**: #4429 (closed), #3398 (Phase 2), SF-2026-09-16-D3-01.
- **Suggested Fix**: Anchor the needle on the parked list itself. For example, put the five kinds
  in a delimited list (a `<!-- parked-starfield-kinds -->` fenced block or a table) and have the
  test scan only that span, the same way it already narrows `types.rs` to the
  `MaterialTextureSet` body.

### SF-2026-09-29-META-01: `/audit-starfield` Dim 5's "First step" filter `shader_tests::starfield` matches zero tests and reports green
- **Severity**: LOW
- **Dimension**: NIF Shader Blocks (audit infrastructure)
- **Location**: `.claude/commands/audit-starfield/SKILL.md` (Dimension 5, `**First step**:`)
- **Status**: NEW
- **Description**: `crates/nif/src/blocks/shader_tests/mod.rs` is mounted at
  `crates/nif/src/blocks/shader/mod.rs:188` via `#[path = "../shader_tests/mod.rs"] mod tests`.
  The test paths are therefore `blocks::shader::tests::starfield::*`, not `shader_tests::starfield`.
  The skill's documented first step, `cargo test -p byroredux-nif shader_tests::starfield`, filters
  to nothing.
- **Evidence**: `cargo test -p byroredux-nif --lib -- shader_tests::starfield` →
  `ok. 0 passed; 0 failed; 1371 filtered out`. `-- shader::tests::starfield` (or `-- starfield`)
  runs the 11 Dim 5 guards the skill names.
- **Impact**: An auditor following the skill literally gets a green zero-test run as the
  dimension's guard check. This is the vacuous-gate pattern `_audit-common.md` warns about. The
  2026-09-22 cycle avoided it only by using a broader filter.
- **Related**: `/audit-nif` (same module-path trap if any skill names `shader_tests::`).
- **Suggested Fix**: Change the first step to `cargo test -p byroredux-nif --lib -- shader::tests::starfield`,
  and consider a `_audit-validate.sh` check that each skill's `cargo test … <filter>` runs at least
  one test.

---

## CRC32 Flag Table

Unchanged. `crates/nif/src/shader_flags.rs` has no commits since 2026-09-16, and the only
shader-block commit in the window (e8ec0a608, #4439) is comment/fixture text plus a
`material_path.rs` test. The full 32-row derivation (flag → `bs_shader_crc32` → read-by-import →
vanilla occurrences) is in `AUDIT_STARFIELD_2026-09-11.md` / `-09-16.md`. One change in consumption:
the effect-shader `Own_Emit` additive promotion now reads the CRC `EMIT_ENABLED` flag as well as the
typed bit (`modern_effect_shader_bit`, #4279 closed). No vanilla Starfield block sets it, so the
change is invisible on vanilla.

---

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md`, Phases 0+1 are done, and Phases 2–4 were invalidated
by the 99.9%-parity measurement. The BGSM parser and the ESM parser have both shipped, and the NIF
truncation tail is cleared.

0. **New this cycle — finish the units boundary.** Lift WTHR (EXT-D1-2026-09-29-01) and WATR
   (SF-D4-01) in `spatial_units::normalize`, and decode the WTHR FNAM tail (SF-D4-02) so its
   height-fog mids/ranges can be lifted too. Also decode LGTM on the Starfield layout (SF-D4-03).
   These are cheap and single-file, and they gate every Starfield exterior and water scene.
1. **CDB Phase 2** (#3398, OPEN):
   - (a) canonical texture roles for `_rough`/`_metal`/`_ao`/`_opacity`/`_transmissive`. #4429
     closed as the parked-by-name contract; tighten its guard first (SF-D3-01).
   - (b) the `material_path → MaterialFields` index over the streaming visitor.
   - (c) the `XMCOLOR` field-order question.
   - (d) an authored glass signal.
   - #4277 (loose `.mat` JSON resolver) needs the same roles.
2. **PDCL ahead of GBFM**, by the baseline doc's own rule: PDCL 74.9% of unresolved Cydonia REFRs
   vs GBFM 0.081%. Unchanged.
3. **Exterior worldspace tiles.** Starfield is still a named policy skip in `m-exteriors.sh`
   (#4488 / #4507). Item 0 is a prerequisite for any exterior verdict.
4. **Space-cell / planet / GBFM records** (PNDT / STDT / BIOM, and SFTR, which now walks 1,505/1,505
   per `/audit-esm` but is unrouted).

---

## Coverage Notes

- **Not run this cycle**: `--sf-smoke` (engine binary), the `sf_smoke` example, the real-data
  `parse_rate_starfield_all_meshes`, and `real_cdb.rs`. Each was either forbidden or already
  measured today by a sibling:
  - `/audit-nif`: 100% / 120,543;
  - `/audit-esm`: Starfield GRUP coverage 96.14%;
  - `/audit-parsers`: Starfield BA2 full sweep, plus the CDB pin of 97 classes / 1,438,780 values.
- **Real-data probes run** (read-only Python GRUP walk; no production code executed):
  - Starfield WATR DNAM ×15, WTHR FNAM ×3, LGTM DATA ×6, REGN RPLD ×3;
  - CELL XCLL/LTMP census over 11,985 interior + 18,732 WRLD cells;
  - FO76 WATR DNAM and FO4 WATR DNAM, for unit comparison.
- **Known-open, re-cited (not re-filed)**:
  - #3398 (CDB Phase 2);
  - #4268 (`.mesh` bone indices unbounded);
  - #4277 (`.mat` JSON resolver);
  - #4441 (NIFAL doc still calls the live classifier arm a future backstop; text verified
    unchanged at `material_translate.rs:576-579`, `material.rs:1462-1465`);
  - #4837 (Starfield oceanness ÷20);
  - #1576 (model-less BFCB forms);
  - #4470 (Starfield HUD).
- **Sibling findings touching Starfield (cross-referenced, not re-filed)**:
  - EXT-D1-2026-09-29-01 (WTHR fog metres, HIGH);
  - EXT-D5-2026-09-29-01 (wind-FROM table; Starfield census gives no support);
  - ESM-D2-01 (TRDA emotion-remap warn flood on Shattered Space);
  - CHAR-2026-09-29-D1-02 (`vital_pools` `O2` vs AVIF `Oxygen`);
  - CHAR-2026-09-29-D5-01;
  - LC-D3-01 (DIAL DATA byte 0 = flags on SF);
  - NIF-D4-2026-09-29-01 (BSGeometry "decoded Y-up" doc rot);
  - PAR-D2-2026-09-29-01 (BA2 DX10 debug panic).
- **Cross-audit routing**:
  - `/audit-esm`: SF-D4-02 and SF-D4-03 land in its parser files.
  - `/audit-exterior`: SF-D4-01 is the WATAL half of EXT-D1-01's units defect. The noise UV tile
    unit needs a capture before any lift.
  - `/audit-fnv`, `/audit-fo3`: by xEdit, FO3/FNV WTHR FNAM carries day/night power at 16–23, but
    `parse_wthr` ignores bytes 16–23 on those games (`weather.rs:489-493` says "not cross-checked").
    This is a candidate for their owners. It is out of Starfield scope and not filed here.
- **Scratch**: `/tmp/audit/starfield/dim_{1..6}.md`. The report's five findings match the scratch
  files one-for-one: D3-01 ← dim_3, D4-01/02/03 ← dim_4, META-01 ← dim_5; dims 1, 2 and 6 are clean.

## Total Findings Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 3 |
| **Total NEW** | **5** |

| ID | Sev | Title | Status |
|---|---|---|---|
| SF-2026-09-29-D4-01 | HIGH | Starfield WATR distance / inverse-distance fields reach WATAL in metres (absorption 70× strong, normals flat at 1.4 m, underwater fog at ~1 m) | NEW |
| SF-2026-09-29-D4-02 | MEDIUM | Starfield WTHR FNAM power / max / height-fog tail never decoded (FO4 \| FO76 gate) | NEW |
| SF-2026-09-29-D4-03 | LOW | `parse_lgtm` reads Starfield's 108-B LGTM with Skyrim tail offsets; `normalize` lifts two scales (dormant) | NEW |
| SF-2026-09-29-D3-01 | LOW | #4429 XOR guard reads any backticked mention in nifal.md as "parked" | NEW |
| SF-2026-09-29-META-01 | LOW | `/audit-starfield` Dim 5 first-step filter `shader_tests::starfield` runs 0 tests | NEW |

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-29.md`. Label every finding
`game:starfield` + `legacy-compat`, and add:
- D4-01: `high` `bug` `water` `esm-plugin`;
- D4-02: `medium` `bug` `esm-plugin` `terrain-exterior`;
- D4-03: `low` `bug` `esm-plugin`;
- D3-01: `low` `test-gap` `import-pipeline`;
- META-01: `low` `tech-debt` (audit infrastructure; no dedicated label).
