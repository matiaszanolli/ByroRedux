# #6229: GAME-D3-2026-10-21-01: An ingestible with no restoration plan is consumed through the caster runtime, which skips the effect conditions and the poison/addiction header, and drops unsupported effects without notice

**Labels**: medium,gameplay,inventory,bug

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Consumables & Timed Effects (Dim 3; the fail-closed policy) · **Location**: `byroredux/src/inventory.rs:517-527` (`install_catalog` builds `script_effects`), `:1484-1491`, `:1570-1573` (`consume_item` starts them); `crates/scripting/src/magic_effects.rs:80-105` (`cast_effect`, `consumable_cast_effects`)

**Audit status**: NEW (from `d1b298921`, 2026-10-18)

- **Trigger**: Any game whose `Aid` items carry `authored_effects` (Skyrim, FO3, FNV; Oblivion takes this path for every potion because `restoration_plan` returns `None` for it). The player uses an ALCH that `restoration_plan` refuses for any reason. The reasons include:
  - an effect-local CTDA other than `GetHasPerk` / NV `IsHardcore`;
  - one effect on an actor value outside the `restoration()` map;
  - a touch or target delivery;
  - a cleared `simple_consumption_header` (attached script, poison, addiction, withdrawal).
- **Description**: `restoration_plan` is all-or-nothing and fail-closed by design: an unsupported condition, effect, delivery, header or AV means `None`, "never a partial potion". `install_catalog` now adds a second table. For every item without a plan, it takes all of `consumable_cast_effects(index, id)` that `is_modelled()` (scripted, or a value modifier). `consume_item` treats a non-empty table as consumable and calls `magic_effects::start_effects` on it. `cast_effect` reads only `entry.effect`. It never looks at `ConsumableEffect.conditions`, `delivery` or `area`, and `consumable_cast_effects` never checks `simple_consumption_header`. As a result:
  - effect conditions that the plan path refused fail **open** here;
  - the sibling effects the runtime does not model are dropped (a partial potion);
  - poisons, whose detrimental value effects the header excluded, are "drunk" by the player.
- **Evidence**: `inventory.rs:522-524`:
  ```rust
  let effects: Vec<_> = byroredux_scripting::magic_effects::consumable_cast_effects(index, id)
      .into_iter()
      .filter(|effect| effect.is_modelled() && (!planned || effect.script != 0))
  ```
  `magic_effects.rs:102-105` maps `entry.effect` only. `restoration_plan` (`crates/plugin/src/consumables.rs:51-127`) is the only place that evaluates `conditions` / `condition_flags`, `delivery` and the header. The menu flag at `inventory.rs:1431-1433` now reports these items as consumable, while their catalog detail line still reads "Consumable (effects unavailable)" (`:553`).
- **Impact**: An item's conditional effects all apply at once, perk-gated or mode-gated variants included. Skyrim / Oblivion / Fallout poisons used from the inventory damage the player. That damage has no death edge (GAME-D4-03), so a strong enough poison leaves the player alive at Health ≤ 0. I did not count how many vanilla ALCH records each game routes here (no data read).
- **Related**: GAME-D4-03; the `restoration_plan` guards `unavailable_consumption_never_changes_health_or_count` and `poison_conditions_scripts_duration_and_malformed_payloads_are_unavailable` stay green because they test the plan path, not this one.
- **Suggested Fix**: Build `script_effects` only for items whose header is simple-consumption (or whose only header bit is the attached script). Refuse the whole item when any effect carries a CTDA the plan path cannot evaluate, or a non-self delivery. Keep the all-or-nothing rule: if any authored effect is not modelled, the item stays "effects unavailable". Fix the detail string to match. Add a guard test: a poison and a CTDA-gated effect are both `Unavailable`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other ALCH/ingredient paths that bypass `restoration_plan`, e.g. scripted `CastImmediateOnSelf` of an ingestible)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
