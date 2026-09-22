# TOOL-D2-2026-09-22-01: ActorValues/ActorVitals and five other core gameplay components are never registered with the debug-server component registry

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4755

## Description
`ActorValues` is, per its own module doc, the production store shared by every gameplay reader — the backing for `GetActorValue`, health/SPECIAL/skills. Both it and `ActorVitals` are real `SparseSetStorage` components with the `inspect` derive already present (`crates/core/src/ecs/components/actor_values.rs:67-170`, confirmed `impl Component for ActorVitals` at line 90 and `impl Component for ActorValues` at line 169) — simply never added to `register_all`. Confirmed via `grep -c 'register_component::<' crates/debug-server/src/registration.rs` = 42, and `grep -n 'ActorValues\|ActorVitals\|EquippedWeapon\|CreatureAttack\|Perks\|FactionReputation\|Dead' crates/debug-server/src/registration.rs` = no matches. All seven components independently confirmed to `impl Component`:
- `ActorValues`, `ActorVitals` — `crates/core/src/ecs/components/actor_values.rs:169,90`
- `EquippedWeapon` — `crates/core/src/ecs/components/inventory.rs:160`
- `Dead` — `crates/core/src/ecs/components/actor_state.rs:18`
- `CreatureAttack` — `crates/core/src/ecs/components/creature_attack.rs:45`
- `Perks`, `FactionReputation` — `crates/core/src/character/components.rs:132,255`

This is the same gap pattern #4063 (closed) fixed for the six AI-procedure `*Behavior` components, recurring for components more central to routine gameplay debugging than any AI procedure. The only existing access path is write-only-by-formid (`setav`/`modav`) or one-value-at-a-time (`cond`) — there is no generic dump, unlike `Inventory`/`EquipmentSlots` (explicitly wired for the M41 smoke test).

## Evidence
```
$ grep -c 'register_component::<' crates/debug-server/src/registration.rs
42
$ grep -n 'ActorValues\|ActorVitals\|EquippedWeapon\|CreatureAttack' crates/debug-server/src/registration.rs
(no matches)
```

## Impact
"Why is this NPC's health/skill/SPECIAL wrong" has no generic `byro-dbg` path (`inspect <id>` / `entities <Component>` cannot see them). The existing completeness guard (`roster_tests::every_ai_procedure_behavior_component_is_registered`) only derives its roster from `impl Component for <X>Behavior` and structurally cannot catch this class of gap.

## Related
Existing: #4063 (closed) — same gap pattern, different component family (six `*Behavior` components). This is a new instance, not a regression of #4063.

## Suggested Fix
Register the seven listed components in `register_all` (mirrors the M41 `Inventory`/`EquipmentSlots` precedent in the same function). Widen `roster_tests`'s guard from "every `*Behavior` type" to "every `impl Component` type carrying the `inspect` derive", so future gaps in this class fail a test instead of landing silently.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D2-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: All seven listed components registered together, not just `ActorValues`/`ActorVitals` — the same PR that fixes one should fix the whole class
- [ ] **TESTS**: The widened `roster_tests` guard (every `impl Component` carrying the `inspect` derive is registered) is added so this class of gap fails CI going forward
