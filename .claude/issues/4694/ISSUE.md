# GAME-D5-2026-09-21-01: PlayerRef (0x14) never resolves through the shared FormID resolver — every package, condition and fragment targeting the player misroutes

**Issue**: #4694
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: HIGH
**Dimension**: 5 — NPC Spawn → AI Package Selection → Locomotion
**Location**: `crates/scripting/src/condition.rs` (`resolve_entity_by_global_form_id`); callers `byroredux/src/systems/follow.rs`, `systems/escort.rs`, `systems/travel.rs` (also Guard through `resolve_near_reference_target`); player identity `byroredux/src/scene.rs` + `crates/core/src/form_id.rs` (`PLAYER_FORM_ID_PAIR`)

## Description
`resolve_entity_by_global_form_id` matches an entity whose `FormIdComponent` resolves to `pair.local == form_id`. The player body's `FormIdComponent` is `PLAYER_FORM_ID_PAIR` with `local = 1`, not `0x14` — no cell places a real `0x14` ACHR. `scripting::package` special-cases `0x14 → player` locally; the ambient movers (Follow/Escort/Travel/Guard) and CTDA/fragment callers do not, since they call the shared resolver directly.

## Evidence
Census with the engine parser: FNV 75 PlayerRef-targeted packages on 29 NPC_, FO3 66 on 40 NPC_, Oblivion 12 on 4 NPC_. `GetDistance PlayerRef` reads `f32::MAX` (infinitely far) when unresolved.

## Impact
Companion/escort packages never follow or escort the player on FNV/FO3/Oblivion. Travel-to-player sends NPCs to arbitrary nearby points. Player-proximity CTDA always evaluates false.

## Related
#3099 (closed, different confusion); #1664 (GetDistance resolver); GAME-D2-2026-09-21-04 (same 0x14/0x7 confusion pattern in the theft rule — see AUDIT_GAMEPLAY_2026-09-21.md).

## Suggested Fix
Resolve PlayerRef inside `resolve_entity_by_global_form_id` itself (`0x14` → the player entity), then delete the two local special-cases in `package.rs`. Add a follow-the-player test through the ambient path.
