# Oblivion (TES4) Compatibility Audit — 2026-06-18

Working tree: `/mnt/data/src/gamebyro-redux`. Oblivion game data present at
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — real-data validation
was exercised (structural byte-decode over vanilla `Oblivion.esm` for the ACBS
finding).

Dedup baseline: `/tmp/audit/issues.json` (29 open issues) + all prior
`docs/audits/AUDIT_OBLIVION_*.md` reports scanned (latest 2026-06-14).

## Executive Summary

Oblivion compatibility continues to harden. **Every HIGH finding from the
2026-06-14 audit has since been fixed** and verified still-holding here:

- The Oblivion 24-byte CTDA drop (`DIM3-01` HIGH) — fixed by `c0ec86c4`
  (#1548); `parse_ctda` now accepts the 24-byte TES4 layout. Confirmed:
  `crates/plugin/src/esm/records/condition.rs:223` (`< 24` is now the floor).
- The `NiInterpController`-descendant whole-subtree truncation (`OBL-D1-NEW-01`
  HIGH) — fixed by `88ec52c5` (#1543/#1544/#1607).

This sweep surfaces **one NEW HIGH** in the live ESM actor path: the shared
`ACBS` parse arm is gated `>= 24` bytes and never branches on `GameKind`, but
**Oblivion's ACBS subrecord is exactly 16 bytes**, so the arm never fires for any
Oblivion NPC_/CREA. The consequence is silent and pervasive — every Oblivion
actor keeps `level = 1` and `acbs_flags = 0`, which corrupts both leveled-list
item selection and runtime gender. This is a single root cause with two
downstream symptoms.

| Aspect | State (live, this sweep) |
|--------|--------------------------|
| NIF v10.x stride-drift family | Regression guards hold (#1506/07/08/09 + #1543/44) |
| BSStreamHeader dual-band / `user_version` ≥ V10_0_1_8 | Hold (`header.rs:114,138`) |
| `NiTexturingProperty` u32 count, no bool gate | Holds (`blocks/texture.rs`) |
| `NiGeomMorpherController` `bsver > 9` gate | Holds (`controller/morph.rs:92`) |
| Oblivion 24-byte CTDA conditions | **Fixed** (#1548, `c0ec86c4`) — now a guard |
| Oblivion 16-byte ACBS (level/flags) | **BROKEN — new HIGH below** |

## Severity Counts

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH | 1 | OBL-D3-NEW-01 |
| MEDIUM | 0 | — |
| LOW | 0 | — |

## Dimension Findings

### Dimension 3 — ESM Record Coverage (live path)

#### OBL-D3-NEW-01: Oblivion 16-byte ACBS never parsed — every Oblivion NPC/creature stays `level = 1` and `acbs_flags = 0` (wrong leveled-list tier + all actors resolve Male)
- **Severity**: HIGH
- **Dimension**: ESM Record Coverage (live path)
- **Location**: `crates/plugin/src/esm/records/actor.rs:563-584` (`parse_npc`, the `b"ACBS"` arm)
- **Status**: NEW
- **Description**: `parse_npc` has a single `ACBS` arm gated `if sub.data.len() >= 24`,
  hardcoded to the FNV/FO3 field layout (`flags u32 @0`, level `i16 @8`,
  disposition `i16 @20`, template_flags `u16 @22`). The function already receives
  `game: GameKind` (passed at `actor.rs:457`, used elsewhere at `:467`), but the
  ACBS arm does **not** branch on it. **Oblivion's ACBS subrecord is exactly 16
  bytes** with a different layout — `flags u32 @0`, `baseSpell u16 @4`,
  `fatigue u16 @6`, `barterGold u16 @8`, **`level i16 @10`**, `calcMin u16 @12`,
  `calcMax u16 @14`. Because 16 < 24, the arm never fires for any Oblivion actor,
  so `record.level` keeps its constructor default of `1` (`actor.rs:486`) and
  `record.acbs_flags` keeps `0` (`actor.rs:488`). `parse_npc` is the live path
  for both NPC_ and CREA (`crates/plugin/src/esm/records/mod.rs:453, :468`), with
  `game` threaded through — so every Oblivion actor is affected.
- **Evidence**:
  - Structural byte-decode of vanilla `Oblivion.esm` (this sweep): **all 914**
    NPC_/CREA ACBS subrecords are size **16** (`{16: 914}`); none reach the
    `>= 24` gate. Decoding `i16 @10` yields a real level distribution
    (`1 ×308, -2 ×112, 0 ×82, 4 ×37, 6 ×35, 2 ×33, 12 ×32, 16 ×27, 8 ×23, …`),
    confirming the field is populated and varied — the parser flattens all of it
    to `1`.
  - `crates/plugin/src/esm/records/actor.rs:563` — `b"ACBS" if sub.data.len() >= 24`.
  - No Oblivion test exercises `parse_npc`: every `parse_npc` test call uses
    `GameKind::Fallout3NV`, `Skyrim`, or `Fallout4` (`actor.rs:1082`–`1556`) —
    the 16-byte path has zero coverage.
- **Impact**: Two silent runtime defects on **all** Oblivion actors:
  1. **Leveled-list item tier** — `npc.level` feeds `actor_level` in leveled-list
     resolution (`crates/plugin/src/equip.rs:265, :356`; `byroredux/src/npc_spawn.rs:299,
     :498, :515, :983`). With `level` pinned to 1, every Oblivion NPC's
     leveled inventory/equipment resolves to the lowest tier
     (`filter(|e| e.level <= actor_level)`), so high-level Oblivion NPCs get
     low-level loadouts.
  2. **Gender** — `acbs_flags` feeds `Gender::from_acbs_flags`
     (`byroredux/src/npc_spawn.rs:411, :1210`), which tests bit 0
     (`equip.rs:56-62`). Oblivion ACBS flag bit 0 = Female (same convention as
     later titles). With `acbs_flags` pinned to 0, **every** Oblivion actor —
     including all female NPCs — resolves to `Male`, driving wrong body/equip
     selection.

  Blast radius: every Oblivion NPC_ and CREA record (≥914 in vanilla
  `Oblivion.esm` alone). Silent — no warn, no test catches it.
- **Related**: Same un-gated-length trap class as the now-fixed Oblivion CTDA
  bug (#1548). #1560 (equip smoke test soft-warns on zero equip components) would
  not surface this because components still spawn — they just carry the wrong
  level/gender.
- **Suggested Fix**: Add a game-gated Oblivion arm before (or fused with) the
  existing one. When `matches!(game, GameKind::Oblivion) && sub.data.len() >= 16`:
  read `acbs_flags = u32 @0`, then `level = i16 @10` (skip 6 bytes:
  baseSpell+fatigue+barterGold). Leave `disposition_base` at its default (Oblivion
  ACBS has no disposition field) and `template_flags = 0` (Oblivion uses no TPLT
  template inheritance). Keep the existing `>= 24` arm for FNV/FO3/Skyrim+
  unchanged. Add a regression test pinning a real Oblivion NPC's level > 1 and a
  female NPC's `Gender::Female`.

## Blocker Chain — "Oblivion exterior cell renders"

Unchanged from 2026-06-14 (no new blockers found this sweep). Interiors render
end-to-end (Anvil Heinrich Oaken Halls). The real exterior chain remains: TES4
worldspace + LAND wiring → CELL exterior REFR placement → exterior bench. BSA
v103 decompression is **not** a blocker (#699, working since 2026-04-17).

The OBL-D3-NEW-01 ACBS bug does not block cell rendering — it corrupts actor
attributes once NPCs spawn into a (working) interior cell.

## Regression Guard List — verified still holding this sweep

- v10.x stride-drift family (#1506 / #1507 / #1508 / #1509) — guards in place.
- `NiInterpController` descendants routed through shared base on old Gamebryo
  (#1543 / #1544 / #1607, `88ec52c5`) — **fixed since last audit**, now a guard.
- Oblivion 24-byte CTDA accepted (#1548, `c0ec86c4`) + unexpected-length
  surfacing (#1550, `4b5f47b8`) — **fixed since last audit**, now guards.
  Confirmed `condition.rs:223` floor is `< 24`.
- `NiGeomMorpherController` `bsver > 9` gate (`controller/morph.rs:92`).
- BSStreamHeader dual-band (#170) + `user_version >= V10_0_1_8` threshold
  (`header.rs:114, :138`).
- `NiTexturingProperty` reads u32 count raw, no leading bool gate
  (`blocks/texture.rs`).
- BSA v103 extraction (#699) — folder-record size `if version ==
  BSA_V_SKYRIM_SE { 24 } else { 16 }` (`bsa/src/archive/open.rs`).
- Disney-BSDF gate stays 0 across the all-legacy Oblivion material universe
  (cross-ref Dim 4/5 of prior audits; no BGSM/.mat content authored by Oblivion).
- Skyrim 128/164-byte RACE DATA no longer mis-decoded as TES4 36-byte (#1629,
  `cb344fcd`) — adjacent guard, holds.

## Notes / Caveats

- Dimensions 1, 2, 4, 5, 6, 7 surfaced no NEW findings this sweep beyond the
  regression-guard confirmations above; the prior 06-14 NIF/CTDA HIGHs are all
  resolved, and the LOW doc-drift items (OBL-D2-DOC-01, OBL-D7-NEW-01/02/03)
  are unchanged stylistic notes not re-reported here.
- The ACBS finding is the one substantive correctness gap remaining in the
  Oblivion live ESM actor path.
