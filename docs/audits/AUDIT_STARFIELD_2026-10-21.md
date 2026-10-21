**HEAD**: 4a3e89d56 · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-10-12.md (HEAD 5d31fd637, full) and docs/audits/AUDIT_STARFIELD_2026-10-19.md (HEAD 2139ae20e, area-scoped to streaming / NPC spawn) · **Audited**: Dims 2, 3, 4, 6 (delta-touched), plus the character audit's bleedout lead · **Unchanged since baseline (skimmed)**: Dims 1 and 5 (no logic commits on the Starfield paths; the guards and the corpus gate were run)

# Starfield Compatibility Audit — 2026-10-21

This report is one leg of `/audit-suite --preset comprehensive`. One auditor ran every dimension in sequence, with no sub-agents.

The window is `5d31fd637..4a3e89d56` (784 commits). The Starfield-relevant commits are:

- **Bring-up fixes for the last two reports:**
  - 17bd40f4f (#5757): LMSW / MTPT / XLMS material swaps.
  - 6cbae341c (#5758): regenerated `sf-esm` baselines.
  - 0a2955d33 (#6118): per-kind FormID spaces.
  - 78facba89 (#6120): 16-byte `ACBS`.
  - e8e00326d (#6122): RACE `ANAM` skeleton and the human face rig.
  - 1dfac76f6 (#6113): FaceGen keyed to the Use-Traits terminal.
  - bfea48f82 (#5533): `sf_smoke` pack-in bucket.
- **CDB:**
  - 1d7e2c6bd (refs #5502): texture-set slot and flat-colour inheritance.
  - e16a624e9: prefetch slot reserved before queueing.
  - 442901646 (#5525): one settings capture.
  - a0da96c0f (#5899 / #5900).
  - 30bdbcc3b (#5465).
  - fa6666053 (#5672).
  - 595110e3c (#5673).
- **Assets:** 48e422a71 (#6200): loose `Data\` tier.
- **Character:** 3162b21b8 (#5921): per-game character profile rows, including the bleedout death time.

Constraints honoured:
- No engine launch, no `--sf-smoke`, no smoke script, and no blanket `--ignored` plugin run. The one plugin `--ignored` test run was filtered to a single test.
- No source, skill or issue edits.
- Each real-data test ran alone under `systemd-run --scope -p MemoryMax=3G..5G`.
- The repo `target/` was used throughout; there is no private target dir.
- Probe scripts are read-only `mmap` walks run with `python3 -I`, kept in `/tmp/audit/starfield/probe/`.

## Guard runs and real-data gates (all green)

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| nif `--lib` `bs_geometry` / `shader::tests::starfield` / `starfield` / `skin` | 72 / 11 / 32 / 118 (1 ignored) |
| sfmaterial (lib + header_smoke + integration) | 51 + 6 + 1 (5 ignored) |
| plugin `--lib` `starfield` / `lgtm` / `xcll` / `spatial_units` / `pkin` / `dat2` / `txst` / `lmsw` / `acbs` / `small_master` | 28 (3 ign) / 5 / 25 (2 ign) / 10 / 12 (1 ign) / 7 / 18 / 11 / 8 (1 ign) / 3 (1 ign) |
| byroredux bin: `starfield_mat` 19, `asset_provider::material` 28, `material_translate` 78, `loose` 16, `cdb` 34, `light_anim` 31, `normalize_mesh_path` 8, `pkin` 17, `lmsw` 1, `starfield` 35 | all pass |
| **Dim 1 corpus** `parse_rate_starfield_all_meshes` (release, MemoryMax=5G, 8.2 s) | **13/13 archives, 120,543 NIFs, 100.00% clean, 0 truncated, 0 failed** |
| **sfmaterial `real_cdb.rs --ignored`** (release, MemoryMax=4G, 8.3 s) | 5/5, including the new `material_index_resolves_inherited_texture_slots` |
| `installed_starfield_small_masters_do_not_collide` (single ignored test, MemoryMax=5G) | pass (Constellation 21 / SFBGS004 852 own ids, no overlap) |
| `sf_smoke --tsv --recurse` on `Starfield.esm` (release, MemoryMax=5G) | **byte-identical to `.claude/audit-baselines/sf-esm/Starfield.tsv`** apart from the trailer comments. 92 FourCCs handled, **87.0%** of GRUP bytes (86.2% at the baseline). The unhandled head is SFTR 90.6 MB, GBFM 36.1, PNDT 25.8, STDT 12.0, BIOM 5.3 and PDCL 0.1. |

## Executive Summary

Starfield is a first-class `GameKind`:
- **NIF + BA2**: unchanged, and re-measured at 100.00% over 120,543 NIFs.
- **CDB Phase 2 is partial.** Colour, normal and emissive roles are translated, and so are the alpha, glass and blend flags. Texture-set slots now inherit down the `Parent` chain (1d7e2c6bd). Still parked:
  - authored metal/rough scalars;
  - slot-6 `_height`;
  - the five single-channel kinds;
  - emissive settings.
- **Walkable Cydonia**: not re-verified, because no engine launch is allowed.

All five findings from the last two Starfield reports are fixed and verified in code: #5757, #5758, #6118, #6120 and #6122. #6096 and #6099 are still open.

**Delta outcome: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW (all NEW).**

- **SF-2026-10-21-D4-01 (MEDIUM)**: essential Starfield actors that go down never get back up. #6120 made Starfield's `ACBS` Essential and Protected bits live: 626 essential and 20 protected `NPC_` records. But the Starfield character profile has no bleedout rule, so a downed essential actor stays down for good, through cell eviction and through saves.
  - The code says Starfield "has no bleedout recovery". Starfield's own data contradicts that.
  - `Starfield.exe` ships `NoBleedoutRecovery`, `OnRecoverFromBleedout`, `fEssentialHealthPercentReGain` and `fEssentialHostileDeathTimeMult`.
  - xEdit SF1 decodes a per-race `Bleedout` block with `Time Min` / `Time Max` / `Recover Chance` / `Health Drain Rate` fields. For example, `HumanRace` authors 8–12 s.
  - That block is not decoded. How the fields combine is unsourced, so this report gives no value.
- **SF-2026-10-21-D4-02 (MEDIUM)**: #5757's swaps reach a pack-in's outer reference, but not the pack-in's own contents.
  - 23.5% of template-cell references carry their own `XLMS` (111,026 of 473,299).
  - In Cydonia, 2,107 of the 12,372 instanced pack-in children are affected.
  - `expand_pkin_placements` keeps only the base and the transform, so each of these children renders its kit's default material.
- **SF-2026-10-21-D2-01 (LOW)**: #6200's loose `Data\` tier does not fit Starfield's asset layout.
  - It has no `geometries` root, so a loose `.mesh` override is invisible.
  - It never scans for a loose `materialsbeta.cdb`.
  - Even if one were registered, the index build re-opens its source with `Archive::open_shared`, which cannot open a directory.
  - The installed Starfield `Data` has no loose asset roots, so vanilla is unaffected.

## Dimension Summaries

| Dim | Verdict | Notes |
|---|---|---|
| 1 BA2 + corpus | unchanged | Only ffa85a0b4 (#5440, `nif_stats` directory mode now uses `is_nif_entry`) touches the paths. `ba2` 44/44. The corpus gate is 100.00% on all 13 archives. |
| 2 BSGeometry | 1 LOW | Only 48e422a71 (#6200) touches the paths. The BSGeometry import is untouched and its guards are green. The loose tier's missing `geometries` root is SF-2026-10-21-D2-01. |
| 3 CDB | clean (delta) | Five things verified (detail below). The loose-CDB gap is filed under D2-01. |
| 4 ESM + bring-up | 2 MEDIUM | #6118, #6120, #6122 and #5757 verified (detail below). The bleedout lead from the character audit is confirmed and filed as SF-2026-10-21-D4-01. Pack-in child `XLMS` is SF-2026-10-21-D4-02. |
| 5 NIF shader blocks | unchanged | b99db0f11 (#5737) only touches the FO4 `parse_shader_type_data_fo4` (the arms it now delegates are byte-identical to Skyrim's). The `BSVER >= 155` path is untouched; 11/11 guards pass; the #1510 truncation count stays 0. |
| 6 Material flow | clean (delta) | 2266f7778 (#4256): `source_shader_type` carries the importer's *normalised* type (`canonical_shader_type`), so FO76/SF `BSShaderType155` numbering does not leak. f81e6e722 (#5755/#5756): one canonical matcher, now shared by LMSW. `resolve_through_cdb` is still the single CDB lookup/fallback site. |

**Dim 3 detail.** These were checked:
- 1d7e2c6bd's `ancestry` walk is bounded at `MAX_PARENT_DEPTH + 1` and cycle-safe. Slots are nearest-wins, and a nearer `Enabled = false` blocks the inherited flat colour. Both are guarded.
- The e16a624e9 prefetch slot is installed synchronously, and the build gate sits inside `get_or_init`.
- Emissive and slot-6 parking are intact.
- #5661's `has_opacity == Some(true)` is intact.
- `real_cdb` passes 5/5.

**Dim 4 detail.**
- **#6118**: the per-kind ordinal remap is checked against xEdit SF1 TES4 flags (`0x100` Small, `0x400` Medium), and the installed small-master collision test passes.
- **#6120**: the 16-byte `ACBS` layout and the template-flag bits match SF1 and `wbDefinitionsCommon` (`wbTemplateFlags` uses the same bit order for Starfield).
- **#6122**: the human rig swap is guarded.
- **#5757**: the parser, the flattening and the per-shape consumer were reviewed.
  - Outer `XLMS` reaches pack-in synthetic children through `refr_overlay`.
  - `Starfield.esm` has 0 `ACHR` records carrying `XLMS` (FO4 has 0 too), so NPC spawn ignoring the overlay is moot for vanilla.
- **`sf_smoke`**: the baseline matches exactly.

Per-dimension notes: `/tmp/audit/starfield/dim_{1..6}.md` and `dim_bleedout.md`.

---

## Findings

### MEDIUM

### SF-2026-10-21-D4-01: Essential / protected Starfield actors that go down never get back up — the profile has no bleedout rule, while the data carries one
- **Severity**: MEDIUM
- **Dimension**: 4 (ESM + bring-up: NPC data through the shared mechanisms; the rule itself belongs to `/audit-character` + `/audit-gameplay`)
- **Location**:
  - `crates/core/src/character/profile.rs:461-463` (`STARFIELD.essential_death_time_secs: None`)
  - `crates/scripting/src/bleedout.rs:129-160` (`engine_defaults`, `install_bleedout_rules`)
  - `crates/scripting/src/bleedout.rs:186-215` (`spare_from_death`)
  - `crates/scripting/src/bleedout.rs:218-221` (`bleedout_system` early return)
  - `crates/plugin/src/esm/records/actor/race.rs` (Starfield RACE `DAT2` bleedout block not decoded)
- **Status**: NEW. This is the character audit's unfiled lead, now confirmed. Related open/closed issues:
  - #5858 (closed) changed the infinite timer to `None`.
  - #6120 (closed) made the flags live.
  - CHAR-2026-10-21-L-02 (this suite) covers the #5921 profile rows' missing capture lines.
- **Description**:
  - Since #6120 (78facba89), Starfield `NPC_` `ACBS` flags decode. `parse_npc` sets `essential` from bit 1 and `protected` from bit 11 (`npc.rs:584-586`), and `ActorProtection::from_record` stamps them at spawn.
  - The Starfield `CharacterRulesProfile` row has `essential_death_time_secs: None`, so `engine_defaults` returns `None`, and `install_bleedout_rules` *removes* `BleedoutRules`.
  - When an essential (or non-player-killed protected) actor's health reaches zero, `spare_from_death` inserts `Bleedout { remaining_secs: None }`.
  - `bleedout_system` returns immediately without `BleedoutRules`, so the actor stays in bleedout (counted unconscious by every #5017 consumer) until a script `Resurrect` (`systems/revive.rs`).
  - The state also survives cell eviction (`reference_state.rs`) and saves (`save_io/registry.rs:382`).

  The profile comment's rationale ("Starfield's executable carries no `fEssentialDeathTime`" → "the game has no bleedout recovery", `profile.rs:245-249` / `bleedout.rs:133-135`) is contradicted by the game's own data.
- **Evidence**:
  - **`Starfield.exe` strings**:
    - `fEssentialHealthPercentReGain`, `fEssentialHostileDeathTimeMult`, `fEssentialNPCMinimumHealth`, `fSpaceshipEssentialDeathTime`;
    - the Papyrus surface `SetNoBleedoutRecovery` / `GetNoBleedoutRecovery` / `NoBleedoutRecovery`, and the event `OnRecoverFromBleedout`.
    - There is no `fEssentialDeathTime` string; `Fallout4.exe` has one.
  - **xEdit `wbDefinitionsSF1.pas`**:
    - RACE `DAT2` +160..+184 is `Bleedout { Health %, Chance, Recover Chance, Time Min, Time Max, Health Drain Rate }` (lines 16582-16589).
    - `NPC_` `ONA2` flag 5 carries per-NPC `Bleedout Data` overrides (15358-15365).
    - `ACBS` bit 16 is "Doesn't bleed" and bit 18 is "Bleedout Override" (15016-15050).
  - **Probe (`/tmp/audit/starfield/probe/npc_race.py`, read-only over `Starfield.esm`)**:
    - 7,131 `NPC_` `ACBS`: 626 Essential, 20 Protected, 87 "Doesn't bleed".
    - 31 RACE `DAT2` ≥ 184 B. `HumanRace` = (0.25, 0.25, 0.25, **8.0, 12.0**, 0.01). 15 races (`ModelARace`, `MiniBotARace` among them) author all zeros.
- **Impact**: any Starfield essential actor (companions, quest givers) brought to zero health by combat, a hazard or a script `DamageValue` stays down permanently: no AI, no dialogue, no packages. The state is persisted, so the save is poisoned too. Before #6120 the flags were 0, so this path was unreachable; the flag fix exposed it.
- **Related**: #6120, #5858, #5921 (profile rows), CHAR-2026-10-21-L-02.
- **Suggested Fix**:
  - Do not invent a value (No Guessing policy).
  - Decode the SF RACE `DAT2` bleedout block and the `ONA2` override so the data is captured.
  - Source the recovery semantics (how `Time Min/Max`, `Recover Chance`, `Health Drain Rate` and `fEssentialHostileDeathTimeMult` combine) from a CK / Papyrus capture before wiring a Starfield `BleedoutRules`.
  - Until then, correct the profile and module docs, which claim Starfield has no recovery, and log once when an actor enters a recovery-less bleedout on a game whose rule is unsourced (as opposed to genuinely absent).

### SF-2026-10-21-D4-02: Pack-in template children drop their own `XLMS` layered material swaps; 2,107 of Cydonia's 12,372 instanced children render the un-swapped material
- **Severity**: MEDIUM
- **Dimension**: 4 (ESM + cell bring-up)
- **Location**:
  - `byroredux/src/cell_loader/refr.rs:633-690` (`expand_pkin_placements_with_depth`: template references are reduced to `(base_form_id, pos, rot, scale)` at line 671)
  - `byroredux/src/cell_loader/references/mod.rs:551-604` (one `refr_overlay`, built from the *outer* REFR, shared by every synthetic child)
- **Status**: NEW. This is a residual of #5757 (closed 2026-10-21) and #5489 (closed). The skill records that "per-child state beyond the base/transform (XPCS, XMSP, enable parents) is not carried". That was dormant for material swaps until #5757 made `XLMS` live, and Starfield's per-child swaps are `XLMS`, not `XMSP`.
- **Description**:
  - #5757 decodes `XLMS` and applies the flattened LMSW entries per shape through the REFR's texture overlay.
  - For a pack-in, `build_refr_texture_overlay` runs once on the outer REFR and that overlay is handed to every synthetic child. So the pack-in's own `XLMS` does reach its contents (164 Cydonia pack-in REFRs author one).
  - The template cell's references are re-placed from `template.references` with only `placed.base_form_id` and the composed transform, so each child's own `layered_material_swap_refs` is discarded.
  - In Starfield these template-cell references are where most kit recolouring is authored.
- **Evidence**:
  - **Game-wide** (`probe/pkin_xlms.py` over `Starfield.esm`): 11,281 PKIN template cells hold 473,299 REFRs, and **111,026** of them carry `XLMS` (of 689,041 `XLMS` REFRs total).
  - **Cydonia `citycydoniamainlevel` (0x2B3DA2)** (`probe/cydonia_pkin_xlms.py`): 370 pack-in REFRs expand (recursively, skipping initially-disabled) to **12,372** leaf children. That matches the engine's own #5533 count exactly. **2,107** of those children author an `XLMS` that is dropped.
- **Impact**:
  - 17% of Cydonia's pack-in content renders with its default kit material instead of the authored layered swap. Examples are the colour-coded industrial trim and painted-metal variants, the same class of visual #5757 fixed for top-level references.
  - The loss is silent: no log line, and `sf_smoke` counts these children as resolved.
- **Related**: #5757, #5489, #5533. The same carry gap applies to FO4 SCOL/PKIN `XMSP`, but FO4 pack-ins have no template cells.
- **Suggested Fix**:
  - Carry the template child's `layered_material_swap_refs` (and its `material_swap_ref`) alongside the transform in the expansion tuple.
  - Build each child's overlay as the outer overlay with the child's own entries merged in. Choose and document the order: outer-after-child (outer wins) or the reverse. The order is unsourced, so pin it as a decision rather than a fact.
  - Extend the `#[ignore]`d `starfield_cydonia_pack_ins_instance_their_template_cells` guard to count children carrying swaps.

### LOW

### SF-2026-10-21-D2-01: #6200's loose `Data\` tier misses Starfield's asset shapes — no `geometries` root, and a loose `materialsbeta.cdb` is never discovered (nor could its index build re-open a directory source)
- **Severity**: LOW
- **Dimension**: 2 (BSGeometry `.mesh` resolution) / 3 (CDB discovery)
- **Location**:
  - `byroredux/src/asset_provider/archive.rs:47` (`LOOSE_ASSET_ROOTS = ["textures", "meshes", "materials"]`)
  - `byroredux/src/asset_provider/archive.rs:524` (`normalize_mesh_path` keeps `geometries\` unprefixed)
  - `byroredux/src/asset_provider/material/provider.rs:73-77, 101-105` (the loose tier is pushed, never passed to `discover_starfield_cdbs`)
  - `byroredux/src/asset_provider/material/cdb.rs:162-172` (`build_cdb_material_index` → `Archive::open_shared(source)`)
- **Status**: NEW (refines a gap the provider's own comment acknowledges as "documented, LOW" since #2621, which #6200 was positioned to close and did not). Distinct from PAR-D6-01 / PAR-D6-02 (loose indexer symlink depth / case folding).
- **Description**: #6200 appends a loose tier that indexes only `textures\`, `meshes\` and `materials\` under `Data`. That misses two Starfield shapes.
  1. **No `geometries` root.** Starfield's external mesh data lives at `geometries\<X>.mesh` (#1292 keeps that root unprefixed). A loose `Data\geometries\…` override is never indexed, so a loose NIF override (which *is* indexed, under `meshes\`) that names new `.mesh` bodies misses every Stage-B slot. The shape is then dropped (`debug!` only).
  2. **No loose CDB discovery.** `discover_starfield_cdbs` runs only on the `--materials-ba2` and `--bsa` archive arms. A loose `Data\materials\materialsbeta.cdb` or `materials\creations\<plugin>\materialsbeta.cdb` is never registered. Even if it were, `build_cdb_material_index` re-opens the source by path through `Archive::open_shared`, which only opens BSA/BA2 files; for a directory it would fail and memoise `None`.

  The audit skill's Dim 3 text ("a loose CDB or `.mat` override joins the same scan") is therefore wrong for CDBs. Loose `.mat` files do resolve, through `extract` on the pushed tier.
- **Evidence**:
  - `LooseDir::index` filters on `LOOSE_ASSET_ROOTS`.
  - `build_material_provider` calls `discover_starfield_cdbs` only at provider.rs:41 and :85 (archive handles).
  - `Archive::open` sniffs file magic (archive.rs:171-200).
  - The installed Starfield `Data` holds only `video\`, so no vanilla number moves.
  - #6200's commit measured FO3, FNV, Skyrim SE and FO4 installs only.
- **Impact**: mod and user overrides only. A Starfield mesh replacer shipped loose loses its geometry, and a loose CDB-edited material set is ignored in favour of the archived CDB. Vanilla is unaffected.
- **Related**: #6200, #2621, #1292, PAR-D6-01, PAR-D6-02.
- **Suggested Fix**:
  - Add `geometries` to the loose roots, or derive the roots from the game.
  - Run `discover_starfield_cdbs` on the loose tier.
  - Have the CDB index build extract through the registered handle (or re-open via `Archive::open_loose` for a directory source) instead of `open_shared(path)`.
  - Correct the skill's Dim 3 sentence at the next `/audit-sync`.

---

## Checked and not filed

- **`ACHR` `XLMS`**: xEdit SF1 allows `XLMS` on `ACHR`, and NPC spawn never consults `refr_overlay`. But `Starfield.esm` has 0 `ACHR` with `XLMS` (9,530 `ACHR`), and `Fallout4.esm` has 0 too, so there is no vanilla impact. Not filed.
- **CDB texture-set inheritance precedence**: when a nearer set enables a `TextureReplacement` flat colour and an ancestor authors the texture file for the same slot, the inherited texture still wins (merge.rs binds the flat colour only when no colour texture exists). Whether the game's enabled replacement overrides an inherited file is unsourced, and the precedence predates 1d7e2c6bd (#5190). Not filed.
- **`ACBS` bit meanings after #6120**: Female (0), Essential (1), Protected (11), Opposite Gender Anims (19) and the template-flag bits all match xEdit SF1 / `wbTemplateFlags`. FormID spaces (#6118) match the SF1 TES4 flag list.

## CRC32 Flag Table

No change since the 2026-10-12 report: there are 0 commits on `crates/nif/src/shader_flags.rs`'s CRC arrays for BSVER ≥ 132 (7822695e4 added an FO3/FNV hair bit only).

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, Phases 0 and 1 are done, and 2–4 were invalidated by the 99.9%-parity measurement.

1. **CDB Phase 2 remainder (#3398).**
   - Shipped: the indexed reader, `XMCOLOR` offset order, `IsGlass`, AlphaBlend and emissive-settings capture, the loose `.mat` decoder, and settings plus texture-set `Parent` inheritance.
   - Left:
     - canonical roles for the five parked kinds (#4429);
     - a slot-6 height-blend consumer (#5381);
     - the `MaterialParamFloat` → roughness/metalness mapping;
     - a `UseSSS` colour source;
     - the inherited-glass coverage signal (rest of #5502);
     - the additive-family blend modes (#5509).
2. **Material swaps.** LMSW / XLMS shipped (#5757). The pack-in child swaps remain (SF-2026-10-21-D4-02).
3. **PDCL ahead of GBFM**, then exterior worldspace tiles, then space-cell / planet / GBFM / SFTR / PNDT records. `sf_smoke` is now 87.0% handled.
4. **Actor behaviour.** Bleedout recovery is unsourced (SF-2026-10-21-D4-01). The NPC stat model is #4453. Corpse ragdoll pose is out of scope (no Starfield rig is in `pose_skeleton`'s verified set).

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-21.md`. Label every finding `game:starfield` + `legacy-compat`, plus:
- D4-01: `character` / `gameplay`;
- D4-02: `esm-plugin` / `import-pipeline`;
- D2-01: `import-pipeline`.
