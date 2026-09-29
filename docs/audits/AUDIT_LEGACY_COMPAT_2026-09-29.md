**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-27.md` (HEAD `ee952b499`) · **Audited**: Dim 1 (one coordinate commit + new placement code), Dim 3 (three decode commits) · **Unchanged since baseline (skimmed)**: Dim 2. Its only in-range commit on the listed paths is WATAL-owned, so it was guard-spot-checked plus the new animation and skinning consumers.

# Legacy Compatibility Audit — 2026-09-29

## Method

This run is delta-scoped against the 2026-09-27 baseline, which has 42 commits in range.
- **Commit review:** I re-ran each dimension's `First step:` against HEAD and read every commit in range that touches a dimension's `Paths:`. I also read the new placement, animation and skinning code in `player_body.rs` and `npc_spawn/loot_appearance.rs`, which is not on the listed paths but falls in this charter.
- **Pattern-C checks:** I verified these with raw Python censuses of the shipped masters:
  - `Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm` (SE), `Fallout4.esm` and `Starfield.esm`, for DIAL sub-records and DIAL `DATA` bytes.
  - The same masters except `Starfield.esm`, for ACHR/ACRE header flag bits.
- **Dedup sources:**
  - `/tmp/audit/issues.json` (163 open).
  - Closed-issue searches for "DIAL DATA", "dial_type" and the rotation-mode issues.
  - The 17 sibling reports dated today.

No source, skill or issue was modified, and no engine or GPU process was launched.
The Gamebryo 2.3 drive was not needed: nothing in range touched a legacy class shape.

## Executive Summary

| Severity | Count (NEW + regression) |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 1 |
| **Total** | **2** |

**Baseline findings are now tracked:**
- LC-D2-01 → `#4938` (open, re-verified present).
- COORD-04 → `#4939` (open, unchanged).

**Verified fixed in range:** `#4126` and `#4764`, both by `8fd66a2b6`, which passes `--rotation-mode` through unclamped.

**Still open, unchanged:** `#4128` (KFM state machine).

**Close-eligible:** `#4127` (COORD-02). The coordinate doc has matched the code since 2026-09-27, and `8fd66a2b6` refreshed it again.

**Headline.** The in-range cross-game decode additions (DIAL `QNAM`, ACHR `Starts Dead`, `Initially Disabled`) are correct per the raw-ESM census.
The census exposed an older Pattern-C divergence on the same record: `DialRecord::dial_type` reads DIAL `DATA` byte 0 as the dialogue type on every game.
On Skyrim, FO4 and Starfield, byte 0 is the Topic Flags byte and the category is byte 1.
Today's gameplay audit routed this "decode half" to `/audit-esm`, but no report filed it.

---

## Dimension 1 — Coordinate + placement fidelity (Z-up → Y-up)

`8fd66a2b6` is the only coordinate-relevant commit on this dimension's paths. The other `boot/` hits (`766e1746e`, `ab31cfefe`, `0182fc5e8`, `424aad268`, `6d05c2bc0`, `a070baaad`, `efc059f3a`) only register systems and resources.

**Verified:**
- **`#4126` / `#4764` fixed correctly.**
  - `boot::cli::rotation_mode_arg` returns the parsed `u8` unclamped, and `boot/mod.rs:344-347` hands it straight to `set_refr_rotation_mode_diag`.
  - The `_ =>` arm of `euler_zup_to_quat_yup_mode` (`crates/core/src/math/coord.rs:198-206`) is the shipping ZYX formula. Mode 1 and any out-of-range value land there.
  - The test `rotation_mode_arg_tests::an_out_of_range_mode_reaches_the_library_fallback` pins that inputs 4, 9 and 255 resolve to the shipping formula and not to mode 3.
  - The stale "Defaults to 0" comment is gone, and `coordinate-system.md:176-183` names the helper.
  - Incidental, not filed: `cell_loader/euler.rs:34` initialises the atomic with a literal `1` rather than `REFR_ROTATION_MODE_SHIP`. It is harmless, because mode 1 and every unknown value hit the same arm.
- **Swap single source of truth.** The diff range adds no `(x, z, -y)`, `from_mat3`, `Mat3::from_cols` or `EulerRot`. There are still only two production `Quat::from_mat3` sites, and both are the documented exceptions: `nif/import/collision/mod.rs:731` and `physics/ragdoll.rs:727`.
- **Strip de-stitch.** There is one implementation, `strip::destrip`, called from `skin.rs:335`, `ni_tri_shape.rs:628` and `collision/shape.rs:668`.
- **Cell grid.** Every production `4096.0` hit is either a distance or a clamp:
  - `systems/cinematic.rs:349` is a tether distance.
  - `render/mod.rs:23` is a fog ray length.
  - The UV clamps in `env_translate.rs` and `material_translate.rs`, and the renderer constants, are the rest.

  `commands/world_info.rs:1235` is inside a test module and predates the baseline (`275b85a04`). `crates/bsa/src/uvd.rs:195` is unchanged; that is `#4939`.
- **New placement code (`player_body.rs`, `a070baaad` / `db8351587`).**
  - The body root's yaw is `from_rotation_y(input.yaw + PI)`.
  - The camera yaw convention is `yaw = atan2(-fwd.x, -fwd.z)` (`scene.rs:156`), so this rotation maps model +Z onto the camera forward.
  - That matches the engine-wide "imported actor forward = +Z" convention already used by `systems/locomotion.rs:235` and `systems/sandbox.rs:108`.
  - It introduces no new swap or Euler path.
- **Unit scale.** `spatial_units.rs` has no commits in range, and the diff adds no downstream per-game distance multiplier.

**No new findings.**

---

## Dimension 2 — Legacy subsystem coverage (skimmed; unchanged paths)

The only commit on the listed paths is `55dd8fa7e`. It changes `crates/core/src/ecs/components/water.rs` (the WaterMaterial scroll-vector doc and the wind-authored flag), which is WATAL and belongs to `/audit-exterior`.
These paths have zero commits in range: `crates/core/src/animation/`, `anim_convert.rs`, `import/material/`, `import/walk/`, `crates/core/src/string/` and `docs/legacy/api-deep-dive.md`.

**Guards spot-checked:**
- **Property → pipeline.** Each of these has a sink:
  - `NiZBufferProperty` (`legacy_properties.rs:177`).
  - `NiStencilProperty`: two-sided plus stencil state (`:1020`).
  - The `NiFlagProperty` subtypes Specular, Wireframe, Shade and Dither (`:1044-1080`).
  - `NiVertexColorProperty` (`:1083`).

  `NiFogProperty` is still the documented limitation.
- **New animation consumer.** `player_body::attach_player_locomotion_animation` (`db8351587`) reuses the NPC clip path: `load_idle_clip`, `idle_desync`, `humanoid_walk_kf_path` and `SkyrimWalkClip`. It adds no new timing-envelope handling. `#4128` is unchanged.
- **Bone-name → entity resolution for the mid-life gear import (`0182fc5e8`).**
  - `GearImportLoader` (`npc_spawn/loot_appearance.rs:555-610`) imports through `load_nif_bytes_with_skeleton`, then `attach_nif_skin_binding` (`scene/nif_loader.rs:1828-1900`).
  - That resolves each skin bone with `name_lookup::get_case_insensitive` against the retained `NpcSkeletonBones` map, then against the NIF's own nodes. This is the same semantics as the spawn path. Clean.
- **`#4938` re-verified present.** `spawn_nif_lights` still passes a literal `0.0` falloff (`cell_loader/spawn.rs:1159-1172`). `b978bb5a1` (#4972) changed only the two raw flag words at this call site, not the falloff.

**No new findings.**

---

## Dimension 3 — Cross-game translation-pattern spot-check

**First-step greps (fresh at HEAD):**
- **Pattern A:** the only raw `bsver` literal thresholds are the three test asserts at `crates/nif/src/blocks/base.rs:530,550,551`. This matches the pinned baseline.
- **New `GameKind::` in the `cell_loader` / `scene` diff:** only test fixtures. There is one production per-game decision elsewhere in range, at `player_body.rs:406` (LC-D3-02).

**Decode changes in range, census-verified:**
- **DIAL `QNAM` (`766e1746e`, `dialogue.rs:191`).**
  - Oblivion, FO3 and FNV DIALs carry `QSTI` and no `QNAM`.
  - Skyrim carries `QNAM` on 15,037/15,037 DIALs and FO4 on 35,443/35,443, with no `QSTI` on either.
  - Accepting `QSTI | QNAM` on every game is therefore correct.
  - The doc misattribution ("QSTI … /FO4") is already filed today as **ESM-2026-09-29-D2-03**, so it is cross-referenced here and not re-filed.
- **Record-header flags (`f87490826`).**
  - `0x800` (Initially Disabled) is read on every placement type in every game.
  - `0x200` (Starts Dead) is read on ACHR only, for the `Tes5Plus` variant.
  - The commit flagged Oblivion as excluded with "no Oblivion source checked". The census settles that: no Oblivion ACHR (0 of 2,190) or ACRE (0 of 1,473) carries `0x200`; the only bits seen are `0x400`, `0x800` and `0x8000`.
  - FO3 and FNV likewise have no ACHR or ACRE with `0x200`. The exclusion drops nothing.
  - The per-era test `starts_dead_is_decoded_only_for_tes5_family_achr` covers both the 20-byte and the 24-byte header.
- **WATR (`55dd8fa7e`):** owned by `/audit-exterior`.

### LC-D3-01: DIAL `DATA` byte 0 is decoded as "dialogue type" on every game, but on Skyrim, FO4 and Starfield byte 0 is Topic Flags and the category is byte 1
- **Severity**: MEDIUM
- **Dimension**: 3 — Cross-game translation pattern (Pattern C: one wire structure, a per-era layout)
- **Location**:
  - `crates/plugin/src/esm/records/misc/dialogue.rs:199` (the decode).
  - `:21-28` (the field doc).
  - `:636-650` (the test `parse_dial_captures_dialogue_type_byte`).
- **Status**: NEW. It is not in any report dated today. `AUDIT_GAMEPLAY_2026-09-29.md:258` names it only as a routing note to `/audit-esm`, and `AUDIT_ESM_2026-09-29.md` does not carry it. It is not tracked in open or closed issues; `#1307` and `#1313` added the byte-0 read for Oblivion only.
- **Description**:
  - `parse_dial` stores `DATA[0]` into `DialRecord::dial_type`.
  - The field doc asserts that "FO3+ widen it (type byte + flags) but byte 0 is the type in every game, so the byte-0 read is cross-game safe".
  - That is true for the classic layouts (1 or 2 bytes). It is false for the 4-byte Skyrim, FO4 and Starfield layout, which is `flags u8, category u8, subtype u16`.
  - On those three games `dial_type` holds a flags bitfield, and the topic category (Topic / Favor / Scene / Combat / Favors / Detection / Service / Misc) is never captured.
  - The unit test pins the false premise with a synthetic 4-byte "FO3" `DATA` (`[5, 0x01, 0, 0]`). Real FO3 `DATA` is 2 bytes.
- **Evidence**: Raw census of DIAL `DATA`, this run:

  | Master | `DATA` length | byte 0 values | byte 1 values |
  |---|---|---|---|
  | Oblivion.esm | 1 (3,817) | 0..6 (type) | — |
  | Fallout3.esm | 2 (6,369), 1 (12) | 0..7 (type) | 0/1/2 (flags) |
  | FalloutNV.esm | 2 (18,205), 1 (10) | 0..7 (type) | 0/1/2 (flags) |
  | Skyrim.esm | 4 (15,037) | {0: 15,018, 1: 19} (flags) | 0..7 (category; 2 = 7,426, 0 = 6,535) |
  | Fallout4.esm | 4 (35,443) | {0, 1, 4, 6} (flags) | 0..7 (category; 2 = 31,330) |
  | Starfield.esm | 4 (68,154) | {0, 1, 2, 4, 6} (flags) | 0..7 (category; 2 = 63,367) |

  ```rust
  // dialogue.rs:197-199
  // DATA byte 0 = dialogue type, cross-game safe (Oblivion: 1 byte;
  // FO3+: wider, byte 0 still the type). #1307 / OBL-D3-...-03.
  b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0],
  ```
- **Impact**:
  - **Today:** nothing reads `dial_type` at runtime. The readers are `parse_real_esm.rs` printlns and `npc_dialogue` test fixtures that set it to 0.
  - **Next consumer:** **GAME-D2-2026-09-29-01** (MEDIUM) needs a category filter (Topic vs Scene / Combat / Misc), because Eltrys "owns" 117 MS01 DIALs, including 5 Scene and 4 Misc DIALs. A fix that filters on `dial_type` would read flags on 3 of 6 games. On Skyrim it would treat all 15,018 zero-flag DIALs as "Topic", which silently re-admits exactly the scene and bark topics it means to exclude.
  - **Shape:** the category is a translatable input dropped on three games, with a wrong value stored under a field documented as cross-game safe.
- **Related**:
  - GAME-D2-2026-09-29-01 (the consumer).
  - ESM-2026-09-29-D2-03 (sibling DIAL doc rot).
  - `#1307` and `#1313` (closed; they introduced the byte-0 read from Oblivion evidence only).
  - `#4469` (a different record: INFO `DATA` on FO3/FNV).
  - Depth is owned by `/audit-esm`.
- **Suggested Fix**:
  - Decode by layout generation. For `DATA` length ≤ 2, set type = byte 0 and flags = byte 1. For length 4, set flags = byte 0, category = byte 1 and subtype = `u16 @ 2`.
  - Expose a single category value whose enum mapping is per game.
  - Correct the field doc, and replace the synthetic 4-byte "FO3" test with one real-shaped case per era.

### LC-D3-02: The FO3/FNV "Child" race-flag translation is duplicated in `player_body.rs`
- **Severity**: LOW
- **Dimension**: 3 — Cross-game translation pattern (a per-game bitfield re-derived per consumer)
- **Location**: `byroredux/src/player_body.rs:406-407`, which copies `byroredux/src/npc_spawn/resumable.rs:557-560`
- **Status**: NEW. It landed in `db8351587`, and no report or issue mentions it.
- **Description**:
  - `RaceRecord::race_flags` is a raw per-game bitfield (`crates/plugin/src/esm/records/actor/mod.rs:655-662`). Bit 2 means Child on FO3/FNV and BeastRace on Oblivion.
  - The NPC spawn path translates it as `matches!(game, GameKind::Fallout3NV) && race_flags & 0x04 != 0` and records why the game gate is part of the translation.
  - The new player-body walk-clip resolution re-derives the same expression inline, with no shared helper. Its comment says it does this "exactly as `prepare_runtime_state` derives them".
  - The mid-life gear import's `ActorBodyClass` (`npc_spawn.rs:1128`) retains gender and race but not the translated child flag, so each future consumer will need a third copy.
- **Evidence**:
  ```rust
  // player_body.rs:406-407
  let is_child = matches!(game, GameKind::Fallout3NV)
      && race_flags.is_some_and(|flags| flags & 0x04 != 0);
  // npc_spawn/resumable.rs:557-560
  // FO3/FNV RACE DATA bit 2 is the authored Child flag. Oblivion reuses
  // that bit for BeastRace, so the game gate is part of the translation.
  let is_child = matches!(game, GameKind::Fallout3NV)
      && race.is_some_and(|race| race.race_flags & 0x04 != 0);
  ```
- **Impact**:
  - Both copies agree today.
  - The risk is drift, which is the per-game-branch sprawl the translation survey warns about. Suppose one site is widened (for example, a Skyrim child race gaining a KF walk path) or tightened (for example, FO4's `HumanChildRace` semantics under `#2455`) and the other is not. The player and NPCs of the same race would then pick different body or walk variants.
- **Related**: `#2455` (FO4 RACE flag semantics unverified); `docs/engine/per-game-translation-survey.md` §5.
- **Suggested Fix**:
  - Add one helper, for example `npc_spawn::is_child_race(game, race_flags)` or an accessor on the RACE side, and call it from both sites.
  - Optionally, also carry the translated flag on `ActorBodyClass` so runtime consumers read the canonical value.

---

## Scratch-file reconciliation

`/tmp/audit/legacy-compat/dim_1.md`, `dim_2.md` and `dim_3.md` were each checked against this report:
- `dim_1.md`: no findings. It records `#4939` as existing, `#4126`/`#4764` as verified fixed and `#4127` as close-eligible.
- `dim_2.md`: no findings. It records `#4938` and `#4128` as existing.
- `dim_3.md`: LC-D3-01 (MEDIUM) and LC-D3-02 (LOW).

All findings appear above, and nothing was dropped.

## Files Reviewed

- **Dimension 1 (coordinates and placement):**
  - `byroredux/src/boot/{cli,mod}.rs`
  - `byroredux/src/cell_loader/euler.rs`
  - `crates/core/src/math/coord.rs`
  - `docs/engine/coordinate-system.md`
  - `byroredux/src/player_body.rs`
  - `byroredux/src/systems/{locomotion,sandbox,cinematic}.rs`
  - `byroredux/src/scene.rs`
- **Dimension 2 (legacy subsystems):**
  - `byroredux/src/npc_spawn.rs`
  - `byroredux/src/npc_spawn/{resumable,loot_appearance}.rs`
  - `byroredux/src/scene/nif_loader.rs`
  - `byroredux/src/cell_loader/spawn.rs`
  - `crates/nif/src/import/material/legacy_properties.rs`
- **Dimension 3 (cross-game translation):**
  - `crates/plugin/src/esm/records/misc/dialogue.rs`
  - `crates/plugin/src/esm/cell/{mod,walkers}.rs`
  - `crates/plugin/src/esm/reader.rs`
  - `crates/plugin/src/esm/records/actor/mod.rs`
  - `crates/plugin/tests/parse_real_esm.rs`
- **Raw ESM censuses:** Oblivion, FO3, FNV, Skyrim SE, FO4 and Starfield masters.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-09-29.md
```
Labels:
- **LC-D3-01:** `medium` `bug` `legacy-compat` `esm-plugin` `dialogue`, plus `game:skyrim` `game:fo4` `game:starfield`.
- **LC-D3-02:** `low` `tech-debt` `legacy-compat` `character`.

Also consider closing `#4127`, since the doc matches the code.
