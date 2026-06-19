**Severity**: HIGH · **Dimension**: ESM Record Coverage (live path)
**Location**: `crates/plugin/src/esm/records/actor.rs:563-584` (`parse_npc`, the `b"ACBS"` arm)
**Status**: NEW — CONFIRMED this sweep (real-data byte-decode over vanilla `Oblivion.esm`)

## Description
`parse_npc` has a single `ACBS` arm gated `if sub.data.len() >= 24`, hardcoded to the FNV/FO3 field layout (`flags u32 @0`, level `i16 @8`, disposition `i16 @20`, template_flags `u16 @22`). The function already receives `game: GameKind` (passed at `actor.rs:457`, used elsewhere at `:467`), but the ACBS arm does **not** branch on it.

**Oblivion's ACBS subrecord is exactly 16 bytes** with a different layout — `flags u32 @0`, `baseSpell u16 @4`, `fatigue u16 @6`, `barterGold u16 @8`, **`level i16 @10`**, `calcMin u16 @12`, `calcMax u16 @14`. Because 16 < 24, the arm never fires for any Oblivion actor, so `record.level` keeps its constructor default of `1` (`actor.rs:486`) and `record.acbs_flags` keeps `0` (`actor.rs:488`). `parse_npc` is the live path for both NPC_ and CREA (`crates/plugin/src/esm/records/mod.rs:453, :468`), with `game` threaded through — so every Oblivion actor is affected.

## Evidence
- Structural byte-decode of vanilla `Oblivion.esm` (this sweep): **all 914** NPC_/CREA ACBS subrecords are size **16** (`{16: 914}`); none reach the `>= 24` gate. Decoding `i16 @10` yields a real level distribution (`1 ×308, -2 ×112, 0 ×82, 4 ×37, 6 ×35, 2 ×33, 12 ×32, 16 ×27, 8 ×23, …`), confirming the field is populated and varied — the parser flattens all of it to `1`.
- `crates/plugin/src/esm/records/actor.rs:563` — `b"ACBS" if sub.data.len() >= 24`.
- No Oblivion test exercises `parse_npc`: every `parse_npc` test call uses `GameKind::Fallout3NV`, `Skyrim`, or `Fallout4` (`actor.rs:1082`–`1556`) — the 16-byte path has zero coverage.

## Impact
Two silent runtime defects on **all** Oblivion actors:
1. **Leveled-list item tier** — `npc.level` feeds `actor_level` in leveled-list resolution (`crates/plugin/src/equip.rs:265, :356`; `byroredux/src/npc_spawn.rs:299, :498, :515, :983`). With `level` pinned to 1, every Oblivion NPC's leveled inventory/equipment resolves to the lowest tier (`filter(|e| e.level <= actor_level)`), so high-level Oblivion NPCs get low-level loadouts.
2. **Gender** — `acbs_flags` feeds `Gender::from_acbs_flags` (`byroredux/src/npc_spawn.rs:411, :1210`), which tests bit 0 (`equip.rs:56-62`). Oblivion ACBS flag bit 0 = Female (same convention as later titles). With `acbs_flags` pinned to 0, **every** Oblivion actor — including all female NPCs — resolves to `Male`, driving wrong body/equip selection.

Blast radius: every Oblivion NPC_ and CREA record (≥914 in vanilla `Oblivion.esm` alone). Silent — no warn, no test catches it.

## Related
Same un-gated-length trap class as the now-fixed Oblivion CTDA bug (#1548). #1560 (equip smoke test soft-warns on zero equip components) would not surface this because components still spawn — they just carry the wrong level/gender.

## Suggested Fix
Add a game-gated Oblivion arm before (or fused with) the existing one. When `matches!(game, GameKind::Oblivion) && sub.data.len() >= 16`: read `acbs_flags = u32 @0`, then `level = i16 @10` (skip 6 bytes: baseSpell+fatigue+barterGold). Leave `disposition_base` at its default (Oblivion ACBS has no disposition field) and `template_flags = 0` (Oblivion uses no TPLT template inheritance). Keep the existing `>= 24` arm for FNV/FO3/Skyrim+ unchanged. Add a regression test pinning a real Oblivion NPC's level > 1 and a female NPC's `Gender::Female`.

## Completeness Checks
- [ ] **SIBLING**: Same un-gated-length pattern checked in adjacent ACBS-class arms (TPLT/DATA) and in CREA-specific subrecords for Oblivion 16-byte variants
- [ ] **CANONICAL-BOUNDARY**: per-game ACBS layout stays gated on `GameKind` inside `parse_npc` (the parser→record boundary) — never re-derived at spawn/equip time
- [ ] **TESTS**: A regression test pins a real Oblivion NPC's `level > 1` and a female NPC's `Gender::Female` (the 16-byte path currently has zero coverage)
