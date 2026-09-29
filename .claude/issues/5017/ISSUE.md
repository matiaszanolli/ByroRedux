# #5017: FO4-D4-01: FO4's ACHR "Starts Unconscious" header flag (0x2000) is never decoded — 191 dormant robots and turrets placed enabled in vanilla spawn awake

**Labels**: bug, medium, legacy-compat, gameplay, ai, game:fo4, esm-plugin

**Source report**: `docs/audits/AUDIT_FO4_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: ESM architecture records + cell expansion. FO4 data through #4814's mechanism; the decode is `/audit-esm`'s and the consumer `/audit-gameplay`'s.

## Location
- `crates/plugin/src/esm/cell/walkers.rs` (`PlacedRef` construction): only `0x800` and `0x200` are read from `header.flags`.
- `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef`, beside `starts_dead`): no field.
- `byroredux/src/cell_loader/references/mod.rs` (actor-job completion, where `apply_starts_dead` runs): no unconscious sibling.
- `rg -i unconscious crates byroredux/src` is empty: the engine has no unconscious state.

## Description
xEdit's FO4 ACHR flag list is `9 Starts Dead, 10 Persistent, 11 Initially Disabled, 13 Starts Unconscious, 25 No AI Acquire, 29 Don't Havok Settle` (`wbDefinitionsFO4.pas:4483-4490`). FO76 (`:5544`) and SF1 (`:8479`) also carry bit 13; TES5's list does not. f87490826 decoded bits 11 and 9 from this header word and left bit 13 (`0x2000`) unread.

The bases on the flagged refs are machines, not humans: `LvlTurretBubble` ×9, `DN084_LvlTurretBubbleMassFusion` ×6, `LvlProtectron` ×6, `DN083_EncTurretTripodMounted` ×6, `LvlMrHandy_RandomHostile` ×5, `LvlProtectronPolice` ×5, `LvlSentryBot` ×4, `LvlAssaultron` ×3, `DN049_LvlMrHandy_Outlet` ×3, `RR101TourBot`, `DN035_Roboracers_*`. That is the powered-down machine a terminal, pod or quest script wakes.

**The runtime semantics are inferred, not sourced.** The flag name is from xEdit; "dormant until woken" is inferred from the flagged bases plus that label. Per the No Guessing policy, source the runtime meaning (e.g. Papyrus `Actor.SetUnconscious` / CK documentation) before implementing the consumer.

## Evidence
Byte census of the installed masters:

| Plugin | ACHR | `0x2000` | …not also `0x800` |
|---|---|---|---|
| Fallout4.esm | 7,615 | 79 | 77 |
| DLCRobot.esm | 152 | 10 | 10 |
| DLCCoast.esm | 1,124 | 6 | 6 |
| DLCNukaWorld.esm | 1,431 | 99 | 98 |

191 enabled (not Initially Disabled) dormant actors in total.

Validated at HEAD 9fcfdc3fc: `walkers.rs` reads only `FLAG_INITIALLY_DISABLED` and `FLAG_STARTS_DEAD` from `header.flags`; `rg -i unconscious` over `crates/` and `byroredux/src` returns nothing.

## Impact
191 dormant sentries spawn as live, AI-driven actors. Hostile variants (`*_Hostile`, `LvlMrHandy_RandomHostile`, raider/gunner-keyed turrets) can engage on sight through #4414's faction hostility, before the scripted wake-up that gates them in vanilla (Mass Fusion DN084, Nuka-World encounter layouts, protectron pods). Same class as #4814 (authored actor state ignored at spawn), narrower population. Reachability is limited today because FO4+ locomotion clips are absent (ROADMAP M42).

## Related
- #4814 (CLOSED), #4813, #4414.
- Sibling ACHR-state decode gaps from the same suite: #5005 (FO3/FNV `XRGD` dead marker), Oblivion base-record Starts Dead, Skyrim/FO4 `XRGD` corpse pose (see cross-references).

## Suggested Fix
- Decode bit 13 into `PlacedRef::starts_unconscious`, gated like `starts_dead` (ACHR, 24-byte family, FO4+).
- Source its runtime meaning (actor present, AI and combat suppressed until a `SetUnconscious(false)`-equivalent event), then consume it at the same actor-job completion hook as `apply_starts_dead`.
- Pin with a real-data count test (`Fallout4.esm` = 79).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO76 / Starfield carry the same bit 13)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic ACHR `0x2000` + real-master count)

## Cross-references: the starts-dead / starts-unconscious decode family (2026-09-29 suite)
#4814 only knows the Skyrim/FO4 ACHR header bit. Each game below has a distinct marker, so each is filed separately:
- #5005 — FO3/FNV: `XRGD` presence is the only corpse marker (rule **inferred from data, needs a source before fixing**).
- #5013 — Oblivion: base `NPC_`/`CREA` header flag `0x80000` + 0 Health (sourced: xEdit TES4, CS wiki).
- #5015 — Skyrim/FO4: the `XRGD` corpse pose is never decoded (marker is correct; pose is lost).
- #5017 — FO4: ACHR `0x2000` "Starts Unconscious" undecoded (dormant semantics **inferred, needs a source before fixing**).

