# #3423 — RT-2026-08-27-06: on the FNV arm, the P2 melee swing lands on a different actor than the fixture target and applies zero damage

Labels: high, combat, game:fnv, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-06 (live gate run at `969d81c8`). **Not bisected.**

- **Severity**: HIGH
- **Dimension**: playable-slice smoke gates (un-owned subsystem) → combat
- **Game**: `fnv` · **Cell**: `GSProspectorSaloonInterior`
- **Location**: gate `docs/smoke-tests/p2-melee-core.sh`; fixture `docs/smoke-tests/fixtures/fnv.env:85-100`; runtime `byroredux/src/combat.rs`, `byroredux/src/interaction.rs` (camera ray → `ActorColliderOwner`)

## Description

`p2-melee-core.sh fnv` clears preflight, reaches the held interactive state, resolves the frozen reference `0x104C6D` (`gstrudy`) to entity 1088, confirms Character mode and a clean combat state, passes the blocked-swing sub-check — and then fails:

```
smoke[p2-melee-core]: FAIL -- the swing did not land on the fixture's target (entity 1088); last_target=927
```

## Evidence

The retained `combat.status` artifact:

```
Combat status:
  cooldown=0.439 blocking=true attacks=1 hits=1 kills=0
  last_target=927 damage=0.0 health_before=220.0 health_after=220.0 killed=false
  outcome=health 220.0 -> 220.0
```

`inventory.entities` identifies entity 927 as `"gssettlercm"` — a different NPC. The fixture's target is `gstrudy` at 240.0 Health (`P2_TARGET_HEALTH="240.0"` in `fixtures/fnv.env`); the struck actor has 220.0. So two things are wrong at once: the camera ray after `combat.approach` resolves to the wrong actor, **and** the hit that did land applied `damage=0.0` and left Health unchanged.

## Impact

This is the melee vertical slice's only end-to-end contract, and it is failing on the arm that still reaches the engine. The zero-damage hit is independently concerning — `hits=1` with `damage=0.0` means the `HitEvent` → Health path ran and resolved to no damage, which the gate would have caught as a kill failure even had the ray been correct.

## Related

#3417 (the other arm of the same gate). #2976 (`HitEvent::blocked` wiring) is the most recent change to this path. #3411's armor-mesh growth changes what collider geometry the ray can hit, though `fnv` is *not* on the #3357 path — so that is a lead, not an attribution.

## Suggested Fix

Reproduce with `BYROREDUX_SMOKE_LOG=debug` and dump the camera ray's hit list; check whether `combat.approach` still lands the capsule where the fixture assumes, and whether `ActorColliderOwner` resolution is picking the nearest bone collider irrespective of owner. Separately, trace why a landed hit produced `damage=0.0` — `UNARMED_DAMAGE` is 8.0 (`byroredux/src/combat.rs:33`), so zero is not a configured value.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the Skyrim arm's ray/`ActorColliderOwner` resolution, and the other interaction-ray consumers in `interaction.rs`)
- [ ] **TESTS**: A regression test pins this specific fix
