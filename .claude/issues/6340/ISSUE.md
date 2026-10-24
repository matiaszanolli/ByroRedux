# PERF-D1-09: jail_bed_system gathers every bed into a fresh Vec every frame while jailed

- **Issue**: #6340 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, performance, gameplay, game:oblivion, bug
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-24.md

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/crime.rs:321-335` (`bed_entities`), `:338-355` (`jail_bed_system`)
- **Description**: the system returns early unless `player_in_jail`, the right outer gate. Once jailed it calls `bed_entities` (an `iter` over every `Furniture` row with a `markers.iter().any(...)` and a `collect()` into a fresh `Vec`) on **every** frame, and only then asks whether an `ActivateEvent` from the player targets one of them. `ActivateEvent` is a transient marker empty on nearly every frame.
- **Evidence**: code read; no probe, derived at about 0.5–3 µs per frame for tens to a few hundred furniture rows.
- **Impact**: negligible, and limited to the time a sentence is being served. Filed because it is a per-frame `Vec` built ahead of a rare event in a system added this window.
- **Suggested Fix**: read the `ActivateEvent` storage first and return when no event has the player as activator; test `world.get::<Furniture>(target)` per matching event, or build the bed set only on that frame.


## Completeness Checks
- [ ] **SIBLING**: the same per-frame pattern is checked in the neighbouring package/AI systems (Follow, Escort, Travel, Guard already resolve once, #5724)
- [ ] **LOCK_ORDER**: if a query/resource lock scope changes, TypeId-sorted acquisition is preserved (the flee/find systems were already reworked once for an ABBA cycle, `ce22fcbcf`)
- [ ] **TESTS**: a regression test pins the fix (a resolve-count / scratch-capacity / settled-pass test in the style of `a_settled_pass_skips_the_walk_until_an_actor_appears`)

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md`. Costs are labelled measured/derived in the report; no GPU frame time is claimed._
