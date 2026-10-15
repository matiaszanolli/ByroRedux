# OBL-D3-2026-10-14-01: the RACE body-section ICON decode and the spawn-time race-skin selection (second half of #5487) have no test

**Labels**: low,game:oblivion,legacy-compat,esm-plugin,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D3-2026-10-14-01`. Re-verified against current code before filing.

- **Severity**: LOW
- **Dimension**: 3 (NPC body assembly)
- **Location**: `crates/plugin/src/esm/records/actor/race.rs` (`parse_race`, the `b"ICON" if !in_head_section && !in_skeleton_section` arm and the `NAM0` / `NAM1` / `MNAM` / `FNAM` section state); `byroredux/src/npc_spawn/resumable/runtime.rs` (`prepare_runtime_state`, the `body_skin` closure)
- **Status**: NEW
- **Description**: #5487 shipped as two halves: the body-path table (pinned by `body_paths_kf_era_*` and the archive-presence test) and the race body skins (RACE `ICON` run → `RaceRecord::body_part_textures` → per-part base-texture swap + seam context). The second half decides every Oblivion NPC's skin tone and nothing exercises it. `body_part_textures` appears outside `race.rs` / `runtime.rs` only as `Vec::new()` in six fixtures. `body_skin` is an inline closure, so the `(index, gender-section)` pick cannot be unit-tested.
- **Evidence**: `git show 1b45763fa --stat` touches `race.rs` (+23) with no test file. Re-measured on `Oblivion.esm`: 124 entries, 13 races × 8 (idx 0–3 × M/F) plus Khajiit and Argonian × 10 (idx 4 Tail), all resolving. The code works today; only the guard is missing.
- **Impact**: a reorder of the head/body `INDX`/`ICON` state machine (it shares `icon_indx` with the head-section arm and relies on `NAM1` resetting it) would silently drop every race skin back to the Imperial default, returning the symptom #5487 fixed with every test green.
- **Related**: #5487 (closed), #5753 (tail MODL, open; would extend the same RACE arm).
- **Suggested Fix**: add a synthetic-RACE unit test asserting `body_part_textures` and untouched `head_part_textures`; factor `body_skin` into a free fn over `(&RaceRecord, icon_idx, want_gender_tag)` and pin the Male / Female / `None`-section picks; optionally a named `Oblivion.esm` assertion (15 races, 124 entries).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
