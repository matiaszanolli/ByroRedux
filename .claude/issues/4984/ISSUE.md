# #4984 — ECS-2026-09-28-D1-03: three test bodies keep a query guard alive across a second acquisition and close cycles against production edges

Filed 2026-09-28 via `/audit-publish docs/audits/AUDIT_ECS_2026-09-28.md`. Snapshot as filed; GitHub is authoritative for live state.

**Source**: `docs/audits/AUDIT_ECS_2026-09-28.md` (HEAD `21319618c`)

- **Severity**: LOW (test hygiene). The same class as #3304 and #3312. It must still be fixed before
  the lane can go green.
- **Dimension**: 1 — Lock Ordering
- **Location**:
  - `byroredux/src/systems/follow.rs:491-499` (`follow_system_follows_the_playerref_sentinel`: `sq`
    (`FollowState`) is still alive when `tq` (`Transform`) is taken. Added by `a70b54f14`, #4694.)
  - `byroredux/src/inventory.rs:3349-3358` (`attach_to_player_stamps_the_character_seed`: the
    `values` `ComponentRef<ActorValues>` is alive across `world.get::<ActorVitals>`)
  - `byroredux/src/save_io/round_trip_tests.rs:482-488` (the `Traveled` query is alive across the
    `Seated` query)
- **Status**: NEW
- **Description**: Production sets the opposite orders. `follow_system_inner` takes `Transform →
  FollowState` (`follow.rs:158-161`), and `commit_actor_value_deaths` takes `ActorVitals →
  ActorValues` (`extensions/commands.rs:495-497`). A test that holds the reverse pair records the
  back edge. That red is not caused by production, but it reddens the lane on its own.
- **Evidence**:
  - `follow_system_follows_the_playerref_sentinel`: "attempted acquisition of `Transform` while
    holding `FollowState`" @ `follow.rs:499`
  - `attach_to_player_stamps_the_character_seed`: "`ActorVitals` while holding `ActorValues`" @
    `inventory.rs:3358`
- **Impact**: 2 panics outright. The round-trip test is a co-cause with D1-01.
- **Related**: #3304, #3312, D1-01.
- **Suggested Fix**: Scope or `drop` the first guard before the second acquisition in each test,
  or copy the value out (`.map(|v| *v)`) before asserting.

## Completeness Checks
- [ ] **LOCK_ORDER**: Each test drops/scopes its first guard (or copies the value out) before the second acquisition
- [ ] **SIBLING**: Grep other `#[test]` bodies for `let <q> = world.query::<A>()` followed by a second `world.query`/`world.get` while `<q>` is live
- [ ] **TESTS**: `follow_system_follows_the_playerref_sentinel` and `attach_to_player_stamps_the_character_seed` pass under `BYRO_LOCK_ORDER_CHECK=1`
