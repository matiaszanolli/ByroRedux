# #6232: GAME-D4-2026-10-21-03: A lethal magic value effect has no death edge; an actor (the player included) is left alive at Health ≤ 0

**Labels**: medium,gameplay,combat,scripting,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Combat & Death Pipeline (Dim 4: every producer that can take Health to zero must reach the death path) · **Location**: `crates/scripting/src/magic_effects.rs:313-353` (`magic_effect_system`: the instant and per-second `apply_damage` branches); `:254-260` (`change_value`)

**Audit status**: NEW (from `d1b298921`). It is the class #5862 closed for script actor-value writes (`queue_health_death`, `fragment/effects/evaluate.rs:653-675`), which this new writer bypasses.

- **Trigger**: FO3 / FNV / Oblivion. A detrimental value-modifier effect on Health, instant or per second, reaches the actor through:
  - ObScript `Cast` / `CastImmediateOnSelf` (`obscript_effects.rs:525-535`);
  - an ability (`magic.rs:231`);
  - a challenge reward;
  - or, since GAME-D3-01, a drunk poison.
- **Description**: `magic_effect_system` writes `values.apply_damage(value.actor_value, -value.amount)` under its own `ActorValues` guard. It never calls `queue_health_death`, `spare_from_death` or `raise_kill_story_event`. The docs of #5862's fix state the rule: "Nothing else sweeps actors at Health <= 0". Only the next weapon hit (`combat_damage_system`) would kill such an actor.
- **Evidence**: `grep -n "queue_health_death\|PendingScriptKills\|Dead" crates/scripting/src/magic_effects.rs` finds only test lines.
- **Impact**: A poisoned or spell-damaged NPC stays standing, keeps its package, can be talked to and bartered with, and its OnDeath / KILL never fires. The player keeps acting at Health ≤ 0, because `player_can_act` tests `Dead`, not Health.
- **Related**: GAME-D3-01, GAME-D4-04, #5862.
- **Suggested Fix**: After each Health-reducing `change_value`, call the scripting crate's `queue_health_death(world, target)` (or a crate-visible wrapper) once the guard is dropped. The Late `scripted_kill_system` then spares essentials and raises KILL like every other producer. Add a test: an instant lethal Health effect queues one `PendingScriptKills` entry.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other `ActorValues` Health writer outside `combat_damage_system` reaches `queue_health_death`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
