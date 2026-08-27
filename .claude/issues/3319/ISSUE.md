# FNV-2026-08-26-D9-01

**Issue**: #3319
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/boot.rs:594-651` (the AI storage pre-registration block, `NavPath` absent) · `byroredux/src/components.rs:1753-1764` (declaration) · consumers at `byroredux/src/systems/travel.rs:197,333`, `wander.rs:263,379`, `patrol.rs:99,216`, `follow.rs:153,292`, `escort.rs:215,403`, `guard.rs:158,275`

**Premise verified**:
`World::query_mut` and `World::query` both bail on a missing storage —
`crates/core/src/ecs/world.rs:496` is `let lock = self.storages.get(&type_id)?;`
(same at `:470`). A storage only exists after `World::register::<T>()`
(`world.rs:124-135`) or an `insert` through `&mut World`
(`world.rs:194-202`, via `storage_write::<T>()`).

A repo-wide grep for `NavPath` (`grep -rn "NavPath" --include="*.rs" .`,
excluding `target/`) returns **zero** production registration or `&mut World`
insert. Every one of the eight `world.register::<NavPath>()` calls sits
inside a `#[cfg(test)] mod tests` block:
`guard.rs:547,607` · `wander.rs:635,694` · `patrol.rs:408` ·
`travel.rs:628,697,731` · `follow.rs:559,607` · `escort.rs:727,771`.
`boot.rs` contains no `NavPath` token at all, and the commit that landed
the wiring (`48304171`, "feat(ai): wire single-tile NAVM pathing into
travel_system (#2372)", 2026-08-23) touches no `boot.rs` hunk
(`git show 48304171 -- byroredux/src/boot.rs` is empty).

This is exactly the failure mode `boot.rs`'s own comments were written to
prevent — every sibling AI storage is pre-registered with the stated
rationale *"so `…_system`'s `query_mut::<XState>().insert(...)` … resolve
even before the first actor spawns"* (`boot.rs:594-597, 606-609, 614-618,
624-628, 630-637, 639-644, 646-651`). `NavPath` is the single omission in
that block.

**Evidence**:
```
$ grep -rn "register::<NavPath>" --include="*.rs" . | grep -v /target/
byroredux/src/systems/guard.rs:547:        world.register::<NavPath>();      # inside mod tests
byroredux/src/systems/guard.rs:607:        world.register::<NavPath>();      # inside mod tests
byroredux/src/systems/wander.rs:635 / :694                                    # inside mod tests
byroredux/src/systems/patrol.rs:408                                           # inside mod tests
byroredux/src/systems/travel.rs:628 / :697 / :731                             # inside mod tests
byroredux/src/systems/follow.rs:559 / :607                                    # inside mod tests
byroredux/src/systems/escort.rs:727 / :771                                    # inside mod tests
$ grep -c NavPath byroredux/src/boot.rs
0
```
`travel.rs:697` even carries the comment *"NavmeshTile/NavPath are
**registered** (unlike every other test in this module)"* — the tests
supply the registration the engine never does. `NavmeshTile` is unaffected:
`spawn_navmesh_tiles` (`components.rs:1723-1730`) inserts through
`&mut World`, which auto-creates the storage.

Consequence, per tick, in the live engine:
* `let nav_path_q = world.query::<NavPath>();` → `None` → `cached` is
  always `None` in `resolve_cached_waypoints` → the `match` always takes
  the `_ =>` recompute arm (`navmesh_path.rs:352-360`).
* `if let Some(mut nq) = world.query_mut::<NavPath>()` → `None` → the
  whole Pass-2 write block (insert **and** remove) is skipped in all six
  systems.

**Impact**: FNV-visible as CPU cost, not wrong movement (the freshly
computed path is correct — this is purely the cache being dead). The
design doc's §7 posture, quoted verbatim in `components.rs:1737` —
*"Computed **once per (entity, goal) pair, not every tick**"* — does not
hold in any shipped build. Cost per walking actor per frame:
`path_from_resident_tiles` `find_map`s every resident `NavmeshTile`
(`navmesh_path.rs:290-295`), and `find_containing_triangle`
(`navmesh_path.rs:107-128`) is an unindexed linear scan with no
bounding-box pre-test over that tile's whole triangle list. On the FNV
corpus that is **141 barycentric tests per resident tile per localize**;
a 7×7 exterior grid resident set is ~49 cells × ~1.5 NAVM ≈ 75 tiles ≈
**~10.6 k triangle tests + one A\* per actor per frame**, 60× a second,
instead of once per goal. It also silently defeats the "cache the
negative result" contract every one of the six systems documents in
prose, so an off-navmesh actor pays that full scan every single tick.

It further undermines the premises of two already-filed issues: **#3269**
(the double `VecDeque` clone) clones a queue that is now always freshly
allocated anyway, and **#3256** (no residency invalidation) describes a
staleness window that cannot occur because nothing is ever cached. Both
should be re-checked after this is fixed, not before.

No existing test can catch it: every system test registers `NavPath`
itself, and `save_io/registry_completeness_tests.rs:328` is a static
source-scanned allowlist of `impl Component` types, not a boot-registration
check.

**Fix sketch**: add `world.register::<crate::components::NavPath>();` to
`boot.rs`'s AI block (next to `AmbientPackageRuntime` at `:651`) with the
same rationale comment as its siblings; add a boot-level assertion or a
`register_ai_component_storages` test that pins the full seven-procedure +
`NavPath` + `NavmeshTile` set so the next Phase-N component can't repeat
this.

---

---


## MEDIUM (14)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
