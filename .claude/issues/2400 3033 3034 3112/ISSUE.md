# Issues 2400, 3033, 3034, 3112

## #2400 — CONC-D3-2026-08-07-02 (LOW, concurrency+animation)
- Location: `byroredux/src/systems/animation.rs:386-833` (guards at `:386`, `:454`)
- `animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` read guards across
  every component acquisition in the system (~15 types deep), undocumented as a lock-order
  constraint — unlike the `NameIndex`-before-`Name` rule already documented a few lines away.
  Registered in a parallel scheduler lane; no live deadlock today (alone in its lane), but a
  future co-scheduled system taking these in the opposite order would ABBA.
- Fix: doc-only / defense-in-depth — add the same hold-stack comment style as the existing
  `NameIndex`-before-`Name` block, naming `AnimationClipRegistry`+`NameIndex` as outermost locks.
- Domain: animation → binary (byroredux) + byroredux-core context.

## #3033 — ECS-2026-08-16-05 (LOW, gameplay/ecs)
- Location: `byroredux/src/combat.rs:80-99`
- `combat_input_system` consumes the attack edge and arms `MELEE_COOLDOWN_SECONDS` **before**
  checking `PlayerMode::Character`. In fly-cam mode, `attacks_started` climbs and cooldown arms
  with no swing possible. `record_miss` also not called on the mode bail, leaving `CombatState.last`
  stale.
- Fix: move the `PlayerMode::Character` gate before the `CombatState` mutation; decide + comment
  whether mode-bail should `record_miss`.
- Domain: gameplay → binary (byroredux).

## #3034 — ECS-2026-08-16-06 (LOW, animation)
- Location: `crates/core/src/animation/text_events.rs:29-68`, `crates/core/src/animation/player.rs:97-105`
- `visit_text_key_events` silently drops every text key when one frame advances a full clip
  period — the scan window wraps onto itself and yields nothing instead of everything.
- Fix: handle full-period-advance explicitly — emit each key once.
- Domain: animation → byroredux-core.

## #3112 — ECS-2026-08-20-03 (MEDIUM, gameplay/ecs) — real gameplay defect
- Location: `byroredux/src/inventory.rs:20-24, 146-150, 422-424`
- #3032 gave weapons an equip slot using bit 31 of `EquipmentSlots::occupants`, claiming it's
  outside "the lower 32-bit contract" — self-contradictory, since MAX_BIPED_SLOTS=32 and bit 31
  is a real authorable BSDismemberBodyPartType (slot 61/FX01) on Skyrim+/FO4 BOD2 masks.
  Equipping an armor with that bit set silently unequips the player's weapon (and vice versa).
- Fix: move weapon occupancy out of the biped-slot array — separate `Option<InventoryIndex>`
  field on `EquipmentSlots`, or widen `occupants` so no authored mask can collide.
- Invariant to test: no `describe_kind` output for `ItemKind::Armor` can collide with the weapon slot.
- Domain: gameplay → binary (byroredux).

## Plan
Independent fixes across 2 locations in byroredux binary + 1 in core:
1. #2400: doc-only comment in animation.rs — no behavior change.
2. #3033: reorder gate in combat.rs, decide record_miss-on-bail.
3. #3034: fix wrap-around window logic in text_events.rs.
4. #3112: real fix — give weapon its own field outside the biped occupancy array in inventory.rs.

Test targets: `byroredux` (binary) for #2400/#3033/#3112, `byroredux-core` for #3034; full
workspace suite as final gate.
