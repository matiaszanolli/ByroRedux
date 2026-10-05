**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: Dim 1 (coordinate + placement: precombine frame fix, TRNS radians, unit lifts), Dim 3 (22 decode commits) · **Unchanged since baseline (skimmed)**: Dim 2. Its in-range commits on the listed paths are all owned by NIFAL, WATAL, EXAL or CHARAL, or are clippy rewrites, so the guards were spot-checked.

# Legacy Compatibility Audit — 2026-10-05

## Method

This run is delta-scoped against the 2026-09-29 baseline, which has 313 commits in range.
- **Commit review:** I re-ran each dimension's `First step:` at HEAD and read every in-range commit that touches a dimension's `Paths:`. That is 16 commits for Dim 1, 9 for Dim 2 and 22 for Dim 3. I also read new placement and decode code reached from those commits but outside the listed paths: `loading_screen.rs`, `cell_loader/precombined.rs`, `cell_loader/load.rs`, `import/transform.rs` (#4633) and `records/load_screen.rs`.
- **xEdit layouts:** I fetched `wbDefinitions{TES4,FO3,TES5,FO4,FO76,SF1}.pas` (dev-4.1.6) to scratch and checked these layouts against them: DIAL `DATA`, INFO `DATA`, the ACHR flag lists, LSCR, and the NPC_/CREA `DATA` health offset.
- **Raw-ESM censuses:** I walked the shipped masters with a Python script, reading sub-records only:

  | Master | Records counted |
  |---|---|
  | `Fallout4.esm` | TRNS, INFO |
  | `Skyrim.esm` (SE) | LSCR `RNAM`, INFO |
  | `Oblivion.esm` | NPC_ / CREA header flag vs `DATA` health; INFO |
  | `Fallout3.esm`, `FalloutNV.esm` | INFO |
  | `Starfield.esm` | ACHR flag bits and base EDIDs, TRNS, LSCR |
  | `SeventySix.esm` | LSCR, TRNS |

- **Tests run:**
  - `cargo test -p byroredux-core --lib math::coord`: 13 passed.
  - `cargo test -p byroredux-nif --lib strip`: 25 passed.
  - `cargo test -p byroredux-plugin --lib -- dialogue load_screen starts_dead lgtm`: 53 passed, 2 ignored.
  - Not run: the `byroredux` bin-crate REFR Euler guards (`cell_loader/euler_zup_to_quat_yup_tests.rs`). They need the 1.96 toolchain build of the bin crate, and nothing in range touched `euler.rs`.
- **Dedup sources:**
  - `/tmp/audit/issues.json` (97 open).
  - `gh` closed-issue searches for LC-D3-01/dial_type, `#4938`, INFO DATA / Goodbye, and FO76 LSCR.
  - Today's sibling reports, in particular `AUDIT_ESM_2026-10-05.md`. The recent `AUDIT_EXTERIOR_2026-10-02`, `AUDIT_FO3_2026-10-03` and `AUDIT_FO4_2026-10-03` reports were also checked.

No source, skill or issue was modified. No engine or GPU process was launched.

## Executive Summary

| Severity | Count (NEW + regression) |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 2 |
| **Total** | **2** |

**Baseline findings:**
- **LC-D3-01 → `#5045`: CLOSED by `7ab87c0fb`, verified correct.** DIAL category reads byte 0 on Oblivion and FO3/FNV, and byte 1 on Skyrim, FO4, FO76 and Starfield. The per-game enums match the TES5 / FO4 / FO76 / SF1 xEdit definitions, and there are per-era tests.
- **LC-D3-02 → `#5079`: still OPEN, unchanged.** The Child-flag translation is still duplicated, now at `player_body.rs:408` and `npc_spawn/resumable/runtime.rs:226` (the second moved in the `#5091` split).
- **Prior-baseline LC-D2-01 → `#4938`: CLOSED by `257e973d2`, verified.** `spawn_nif_lights` takes the canonical LIGH falloff lane (`spawn.rs:1126`, REFR caller `:728-735`). The loose-NIF caller passes `1.0` and documents why.
- **COORD-04 → `#4939`: CLOSED by `27002ed99`, verified.** `precombined.rs:104` const-asserts `UVD_CELL_UNITS == EXTERIOR_CELL_UNITS`.

**Still open, unchanged:** `#4128` (KFM state machine) and `#4127` (coordinate-doc XCLL claim). `#4127` has been close-eligible since 2026-09-27.

**Headline.** The coordinate and placement layer is clean.
- `#5228` exterior precombines: all three precombine spawn sites now pass a `Vec3::ZERO` root, and the only production `cell_grid_to_world_yup` caller left is the water-tile centre.
- `#5229` load-screen TRNS rotation: the radians reading is census-confirmed. FO4 TRNS peaks at 2π; Skyrim LSCR `RNAM` is i16 degrees.

**Cross-game decodes.** The new DIAL, Starts-Dead, Starts-Unconscious, EFID and Oblivion base-actor decodes are all correct per xEdit and the bytes.
- One independent catch, the INFO `DATA` layout, was already filed today as **ESM-2026-10-05-D2-01**.
- Two LOW translation-shape gaps are new: FO76 is left out of the LSCR family whose wire shape it shares, and the Starfield 108-byte lighting tail is decoded and lifted by hand-copied twins.

---

## Dimension 1 — Coordinate + placement fidelity (Z-up → Y-up)

**Commits reviewed:**
- **Coordinate-relevant:**
  - `f3e1bba62`: `#5228` precombine zero origin and `#5229` TRNS radians.
  - `27002ed99`: `#4939` uvd const-assert.
  - `e60911864`: LSCR model-cover stage pose.
  - The Starfield and FO76 unit lifts: `69ae5bffa`, `58e877137`, `3ab5d2122`, `8347fde67` and `40de6da5d`.
  - `6e3ca2716`: `#4633` gating of `compose_transforms` products on finiteness.
- **Not coordinate-relevant:** the remaining `boot/` and `camera.rs` hits (system registration, and a test-only constant swap).

**Verified:**
- **Frame composition (`#5228`).**
  - All precombine spawn sites pass `Vec3::ZERO`: `cell_loader/load.rs:525` and `:921` (interior and single-cell), and `exterior.rs:2114-2122` (streaming apply job).
  - The only production `cell_grid_to_world_yup` caller left is `exterior_water_tile_transform` (`exterior.rs:90`). Its centre math `(origin.x + half, origin.z - half)` matches the Z-up +Y → Y-up −Z flip.
- **`#4939` closed.**
  - The re-declaration in `crates/bsa/src/uvd.rs:198` is re-exported as `UVD_CELL_UNITS` (`bsa/src/lib.rs:46`).
  - It is const-asserted equal to the core SoT at `precombined.rs:104`.
- **Load-screen stage pose (`#5229`).**
  - **FO4:** `Fallout4.esm` has 949 TRNS. Their `DATA` is 36 B (649) or 28 B (300, zoom tail dropped). The maximum |rotation| is 6.2832 = 2π, and common values are 0.524 / 1.571 / 3.142, so the rotation is radians. `parse_trns` accepts 28 or 36 bytes (`load_screen.rs:194`).
  - **Skyrim:** `RNAM` is 6 B i16 on all 298 LSCRs, with values −165..100, so it is degrees. Only that arm converts (`loading_screen.rs:509-536`).
  - Both arms feed the REFR dispatcher `euler_zup_to_quat_yup_refr`, which is consistent with REFR placement. The translation goes through `coord::zup_to_yup_pos`.
- **Single implementation.**
  - The delta adds no inline `(x, z, -y)`. Every new conversion calls the SoT: `cell_loader/water.rs` → `zup_to_yup_pos`, and `loading_screen.rs` → `zup_to_yup_pos` / `euler_zup_to_quat_yup_refr`.
  - The production `Quat::from_mat3` sites are still only the two documented exceptions: `nif/import/collision/mod.rs:731` and `physics/ragdoll.rs:838` (moved from `:727`).
  - `strip::destrip` is the only de-stitch, with callers `ni_tri_shape.rs:628`, `skin.rs:335` and `collision/shape.rs:668`.
  - The `EulerRot::XYZ` in `studio_host.rs:247/391` predates the baseline. It is an engine-space Y-up edit round-trip, not a Bethesda Euler path.
- **Cell grid.** Every new `4096.0` in range is a test fixture or a renderer distance constant: the `frame_params` / `post_passes` tests and the `water.rs` test asserts.
- **Transform model.** `#4633` routes `compose_transforms` output through the existing `#4549` neutraliser `sanitize_transform_translation_and_scale` (`import/transform.rs:26-31`). This extends, not duplicates, the boundary sanitizer, and core `normalize_quat` gained no sanitizer caller.
- **`--rotation-mode`.** It is still unclamped. `euler.rs:34` still initialises with literal `1`; that is harmless and was not filed in the baseline either.
- **Unit scale.** Every new metric lift lands in `spatial_units::normalize`:
  - Starfield: WTHR fog + FNAM heights, WATR DNAM and LGTM height fog.
  - FO76: the absorption-only arm, which goes through a shared helper.

  The downstream `WORLD_UNITS_PER_METER` / `BETHESDA_UNITS_PER_METER` uses (`fog.rs`, `components.rs`) are game-agnostic BU→m physics conversions, not per-game multipliers.

**Incidental, not filed:** `precombined.rs::exterior_apply_advances_precombine_at_zero_origin` is an `include_str!` scan rather than a `source_scan::production_text` scan.
- It scans a *different* file (`exterior.rs`), so its own literals cannot satisfy it. That makes it coarse but not vacuous.
- It belongs to `/audit-tech-debt` if anyone wants it migrated.

**No new findings.**

---

## Dimension 2 — Legacy subsystem coverage (skimmed; in-range commits owned elsewhere)

**Commits on the listed paths:**
- **NIFAL material boundary:** `e80f7e854` (`#5012`), `c2b67d81e` (`#4441` BGEM doc) and `978d25c19` (`#5197` CDB translation; adds `PbrMaterial::NO_SIGNAL_NEUTRAL`).
- **WATAL:** `water.rs` and `37cc637e9`.
- **EXAL:** `groundcover.rs`.
- **CHARAL:** `actor_values.rs`.
- **Clippy:** `3f0852e08`, `4ad847a81` and `fd26378da`.

No semantic commit touched `crates/core/src/animation/`, `anim_convert.rs`, `import/walk/`, `crates/core/src/string/` or `docs/legacy/api-deep-dive.md`.

**Guards spot-checked:**
- **Animation (clippy).** `crates/nif/src/anim/controlled_block.rs` (`#5115`) rewrote the `NiBlend*Interpolator` if-else chain as an `Option::or_else` chain. It has the same four arms in the same order with the same `None` fall-through, so it is semantically identical. The bspline test edit keeps the NaN-incomparability assertion.
- **Property → pipeline.** `NiZBufferProperty` (`legacy_properties.rs:177-180`) and `NiStencilProperty` (`:1020-1028`) both still have sinks. `NiFogProperty` is still the documented limitation (`:144`).
- **`#4938` fix verified in place.** `nif_light_spawn_gate_tests.rs:687-707` pins the falloff pass-through.
- **`#4128`** is open, with no animation commits in range. The parked `ColorTarget::LightAmbient` (`#983`) is unchanged.

**No new findings.**

---

## Dimension 3 — Cross-game translation-pattern spot-check

**First-step greps (fresh at HEAD):**
- **Pattern A:** the only non-comment raw `bsver` literal thresholds are the three test asserts at `crates/nif/src/blocks/base.rs:530,550,551`. This is unchanged.
- **Per-game branches in `byroredux/src` with blame after 2026-09-29:**
  - `env_translate.rs:605-608` (`watr_angle_to_engine_xz`) and `:932` (`concentration_lane3_is_oceanness`). Both are table-shaped WATAL boundary decisions.
  - `object_lod.rs:797` (`tree_lod_supported`). This is EXAL, dead code until `#4913`.
  - `npc_spawn/resumable/runtime.rs`, which `37db35cca` moved from one file to another without adding branches.
  - Nothing new in `cell_loader/` or `scene/`.

**Decode commits verified (xEdit plus census):**
- **DIAL (`#5045` / `#5224`).**
  - The category table matches TES5 (1 Favor, 4 Favors), FO4 and SF1 (1 Command, 4 Favor) and FO76 (4 Detection, 5 Misc, 6–7 unknown).
  - FO3/FNV DIAL `Flags` byte 1 is {0 Rumors, 1 Top-level} (`SetOptionalFrom(1)`), which matches `data_flags` / `top_level()`.
- **INFO `DATA` (`#4469`, `cc9353043`) — cross-referenced, not re-filed.**
  - I independently found the same defect that **ESM-2026-10-05-D2-01** files. On FO3/FNV, `data_flags` = Next Speaker | Flags 1 << 8. Bit `0x80` is Speech Challenge, not Goodbye (`wbINFOAfterLoad` FO3:2206). Flags 2 is dropped.
  - Census confirms it: FO3 has 22,327 `DATA` (4 B ×21,693, 3 B ×634), with byte 1 ∈ {0, 1} (Next Speaker). FNV has 23,247, all 4 B.
  - Skyrim *does* author an 8-byte `DATA` on 924 of 31,465 INFOs, and FO4 authors none.
  - The unit test `parse_info_data_header_decodes_type_and_flags` pins the false `[0, 0x80, 0]` "FO3 goodbye" shape.
  - The field doc's "same byte-0-is-type convention `DialRecord::category` decodes" also went stale with `#5045`.
- **EFID (`#5075` / `#5076` / `#5084`).** `remap_efid` is the single Oblivion gate, and `EsmIndex::resolve_magic_effect` is the single code-vs-FormID lookup. There is no production `magic_effects.get` bypass.
- **Oblivion base-actor Starts Dead (`#5013`).**
  - NPC_: on all 2,482 records, header `0x80000` ⇔ `DATA` Health (u16 @21) == 0 (87/87 both ways).
  - CREA: 45 records carry the flag with 0 hp, and 3 carry it with hp > 0 (summons and wraiths). No CREA has 0 hp without the flag.
  - The flag-only rule therefore drops no authored corpse.
- **ACHR Starts Dead `0x200` on Starfield — correct, not filed.**
  - xEdit SF1's ACHR flag list *omits* bit 9, while TES5, FO4 and FO76 list it.
  - Yet 2,283 of 9,530 `Starfield.esm` ACHRs carry it, and 2,181 of those place `*Corpse*` bases: `REOverlayCorpse` ×955, `Biome_Prey_Corpse` ×250, `Biome_Critter_Corpse` ×120, `Loot_Corpse_*` and others. 1,870 of them also carry `XRGD`.
  - The variant-keyed decode in `walkers.rs:1217-1219` is therefore right on Starfield.
  - Note for maintainers: the `PlacedRef::starts_dead` doc (`cell/mod.rs:569-571`) names only Skyrim and FO4. A future "match xEdit SF1" change would resurrect 2,283 corpses, so the Starfield census belongs in that doc.
- **Starts Unconscious `0x2000`** is gated to FO4, FO76 and Starfield, matching the xEdit lists (TES5 has no bit 13). There are 193 Starfield refs.
- **TRNS on other games:** Starfield has 712 TRNS, all 36 B, with max |rotation| 6.27 (radians). FO76 has 4,064 (36 B ×3,933, 28 B ×131).
- **Starfield WTHR / WATR / LGTM lifts** all go through `spatial_units`. `/audit-exterior` (`#5171`, the unpinned LGTM lift) and `/audit-esm` own their depth.
- **Clippy sweep:** no negated-comparison rewrite that changes NaN semantics in plugin, nif or `cell_loader` production code.

### LC-D3-01: FO76 is left out of the LSCR "modern" family, so 356 FO76 model load screens and their 4,064 parsed TRNS transforms are dropped
- **Severity**: LOW
- **Dimension**: 3 — Cross-game translation pattern (Pattern B mis-dispatch: the wire format matches FO4, but the game enum excludes FO76)
- **Location**: `crates/plugin/src/esm/records/load_screen.rs:58-62` (the `legacy` / `modern` sets), `:75-76` and `:106-108` (the `TNAM` / `ZNAM` gate), `:93` (`ICON`), `:103`, `:129`; and the module doc at `:4-5`.
- **Status**: NEW. It landed with `e60911864` (LSCR model cover). No open or closed issue matches, and no report dated 2026-10-02..05 mentions FO76 LSCR.
- **Description**:
  - `parse_lscr` partitions games into `legacy` (Oblivion, FO3NV) and `modern` (Skyrim, FO4, Starfield). It gates `TNAM` / `ZNAM` on FO4 or Starfield.
  - `GameKind::Fallout76` is in neither set, so for FO76 every presentation sub-record falls through to `_ => {}`: `NNAM`, `TNAM`, `ONAM`, `ZNAM` and `MOD2`.
  - xEdit `wbDefinitionsFO76.pas` `wbRecord(LSCR)` is the FO4 shape: `NNAM` → STAT/SCOL, `TNAM` → TRNS, `ONAM` s16×2, `ZNAM` f32×2, `MOD2`. It adds `BNAM` 'Background Image' (string) and `LSST`.
  - The module doc lists TES4/FO3/FNV/TES5/FO4/SF1 as references and does not mention FO76, so the exclusion is an omission, not a decision.
  - The `TRNS` dispatch arm (`dispatch_misc_stub.rs:146`) *is* game-agnostic, so FO76's transforms are parsed into `load_screen_transforms` with nothing pointing at them.
- **Evidence**: Census of `SeventySix.esm`: 474 LSCR, of which 356 have `NNAM` + `TNAM` + `ONAM`, 439 have `BNAM` and 14 have `ZNAM`; and 4,064 TRNS (36 B ×3,933, 28 B ×131).
  ```rust
  // load_screen.rs:58-62
  let legacy = matches!(game, GameKind::Oblivion | GameKind::Fallout3NV);
  let modern = matches!(
      game,
      GameKind::Skyrim | GameKind::Fallout4 | GameKind::Starfield
  );
  ```
- **Impact**:
  - Every FO76 LSCR decodes with `model == 0`, so `load_screen_verdict` rejects all 474 and no FO76 load cover can ever be selected.
  - It is cosmetic, and FO76 is not a playable target (`game-compatibility.md` "Cell loading: not yet started"). That is why this is LOW.
  - The shape is the one the survey warns about: a per-game enum set that silently omits a game whose wire format already discriminates itself.
- **Related**: `e60911864`, `#5229`; `docs/engine/per-game-translation-survey.md` §5 Pattern B.
- **Suggested Fix**:
  - Add `GameKind::Fallout76` to `modern` and to the `TNAM` / `ZNAM` gate. Optionally decode `BNAM` into `icon` as FO76's image path.
  - Add `wbDefinitionsFO76.pas` to the module doc and an FO76 arm to the LSCR test.
  - If FO76 is meant to stay excluded, say so in the doc instead.

### LC-D3-02: The Starfield 108-byte lighting tail has two hand-copied decoders (XCLL and LGTM) and two hand-copied unit lifts
- **Severity**: LOW
- **Dimension**: 3 — Cross-game translation pattern (Pattern C shape: one per-game wire structure decoded at two sites)
- **Location**:
  - Decoders: `crates/plugin/src/esm/cell/walkers.rs:496-557` (the SF XCLL arm) and `crates/plugin/src/esm/records/misc/world.rs:1320-1364` (the SF LGTM arm, `#5002`).
  - Lifts: `crates/plugin/src/esm/records/spatial_units.rs:28-39` (`lighting()`, CellLighting) and `:138-152` (the `lighting_templates` loop).
- **Status**: NEW. `#5171` (open) covers only the *missing test* for the LGTM lift, and `AUDIT_ESM_2026-10-05` raises only the byte-28 label question. Neither covers the duplication.
- **Description**:
  - `8347fde67` fixed `#5002` by copying the SF XCLL offset table into `parse_lgtm`. Its commit message says "decode the SF tail with the same offset table as the SF XCLL arm".
  - The result is two independent 20-field decoders of bytes 28..108 (gravity scale, fog clip/power, far colour, fog max, light fades, the height-fog model, interior type) that build the same `StarfieldLighting`.
  - It also copied the four-line height-lift block into the `lighting_templates` loop, with the comment "lifts exactly like the XCLL one in `lighting()` above", rather than sharing `lighting()`.
  - The copies agree today. The XCLL arm uses `unwrap_or` / `f32_or_default` and the LGTM arm uses `.ok()`, which is equivalent under the `>= 108` guard.
- **Evidence**: The same offset comments appear at both sites:
  ```rust
  // walkers.rs:497-516                         // world.rs:1323-1341
  let gravity_scale = r.f32_or_default(); // 28  let gravity_scale = tail.f32_or_default(); // 28
  ...                                             ...
  let interior_type = r.u8_or_default(); // 104  let interior_type = tail.u8_or_default(); // 104
  ```
- **Impact**:
  - The risk is drift. Today's ESM report already disputes the byte-28 / 56-59 labels between xEdit's SF1 LGTM and XCLL definitions.
  - Any future correction (a label, a new SF DLC length, a lift change) must be made twice, or Starfield interiors (XCLL) and lighting templates (LGTM) will diverge. This is the "fix kept at only one site" failure that `#1044` collapsed for coordinates.
  - The lift twin is unpinned (`#5171`), so a drift there would be silent.
- **Related**: `#5002` (closed), `#5171` (open, the LGTM-lift test gap), `#1293` / `#1579` (the SF XCLL arm), ESM-2026-10-05 (the byte-28 label note).
- **Suggested Fix**:
  - Extract `decode_starfield_lighting_tail(&[u8]) -> (base fields, StarfieldLighting)` in `esm::cell` and call it from both arms.
  - Make the template lift reuse `lighting()`, either by giving `LgtmRecord` an embedded `CellLighting`-shaped core or by sharing a helper that lifts `StarfieldLighting` and the five base distances.
  - Pin both with the `#5171` test.

---

## Scratch-file reconciliation

`/tmp/audit/legacy-compat/dim_1.md`, `dim_2.md` and `dim_3.md` were each checked against this report:
- **`dim_1.md`:** no findings. It records `#4939` as closed and verified, the frame-composition and TRNS censuses, and the incidental `include_str!` note.
- **`dim_2.md`:** no findings. It records `#4938` as closed and verified, `#4128` as open, and the clippy semantic check.
- **`dim_3.md`:**
  - LC-D3-01 (LOW) and LC-D3-02 (LOW).
  - The INFO `DATA` cross-reference to ESM-2026-10-05-D2-01.
  - The Starfield Starts Dead note.
  - `#5045` closed and verified; `#5079` open.

All findings appear above, and nothing was dropped.

## Files Reviewed

- **Dimension 1 (coordinates and placement):**
  - `byroredux/src/cell_loader/{precombined,exterior,load,euler}.rs`
  - `byroredux/src/loading_screen.rs`
  - `byroredux/src/npc_spawn/resumable/runtime.rs`
  - `byroredux/src/studio_host.rs`
  - `crates/core/src/math/coord.rs`
  - `crates/nif/src/import/transform.rs`
  - `crates/bsa/src/{uvd,lib}.rs`
  - `crates/plugin/src/esm/records/{spatial_units,load_screen}.rs`
  - `byroredux/src/env_translate.rs` (the WATR lift interplay)
- **Dimension 2 (legacy subsystems):**
  - `crates/nif/src/anim/controlled_block.rs`
  - `crates/nif/src/import/material/{mod,legacy_properties}.rs`
  - `crates/core/src/ecs/components/material.rs`
  - `byroredux/src/cell_loader/spawn.rs`
  - `byroredux/src/scene/nif_loader.rs`
- **Dimension 3 (cross-game translation):**
  - `crates/plugin/src/esm/records/misc/{dialogue,world,magic}.rs`
  - `crates/plugin/src/esm/records/{dispatch_actor,dispatch_misc_stub,items,common,index,load_screen,spatial_units}.rs`
  - `crates/plugin/src/esm/records/items/consumable.rs`
  - `crates/plugin/src/esm/records/actor/mod.rs`
  - `crates/plugin/src/esm/cell/{walkers,helpers,mod}.rs`
  - `byroredux/src/cell_loader/{object_lod,lod_bands,terrain,placement_lod}.rs`
  - `byroredux/src/player_body.rs`
- **External reference:** xEdit `wbDefinitions{TES4,FO3,TES5,FO4,FO76,SF1}.pas` (dev-4.1.6).
- **Raw-ESM censuses:** `Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm` (SE), `Fallout4.esm`, `SeventySix.esm` and `Starfield.esm`.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md
```
Labels:
- **LC-D3-01:** `low` `bug` `legacy-compat` `esm-plugin` `game:fo76`.
- **LC-D3-02:** `low` `tech-debt` `legacy-compat` `esm-plugin` `game:starfield`.

Also consider closing `#4127`, since the coordinate doc matches the code.
