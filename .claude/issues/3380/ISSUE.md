# PHYS-D3-2026-08-27-02: `release_victim_rapier_bodies`' tolerance of a duplicated victim list is incidental, undocumented and untested

- **Issue**: [#3380](https://github.com/matiaszanolli/ByroRedux/issues/3380)
- **Finding ID**: `PHYS-D3-2026-08-27-02`
- **Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,physics,test-gap,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3380 --json state`.

---

- **Severity**: LOW
- **Dimension**: ECS Sync — the 4(+1)-Phase Tick (release half)
- **Location**: `byroredux/src/cell_loader/unload.rs:482-544`;
  `byroredux/src/cell_loader/rapier_release_tests.rs:1-283`
- **Status**: NEW
- **Trigger Conditions**: any caller that hands the function a victim list
  containing the same `EntityId` more than once. PHYS-D3-2026-08-27-01 is one
  such caller today; the point of this finding is that nothing in the contract
  says whether that is allowed.
- **Description**: the function collects one `RapierHandles` (and one `Ragdoll`)
  per *occurrence* of a victim, then calls `pw.remove_body` / `pw.remove_ragdoll`
  once per collected entry:

  ```rust
  for &eid in victims {
      if let Some(h) = handles_q.get(eid) { to_remove.push(*h); }
  }
  ...
  for h in to_remove { pw.remove_body(h.body); }
  ```

  A repeated victim therefore produces a repeated `remove_body` on a handle that
  is already gone. This happens to be safe: rapier's `RigidBodySet::remove`
  matches on the handle's generation and returns `None` for a freed slot, and
  slots are not reused inside the loop because nothing inserts. But that is a
  property of the dependency, not of this code — and `PhysicsWorld::remove_body`
  (`crates/physics/src/world.rs:247-266`) documents only its cascade behaviour
  and its `#2863` wake, not idempotency.

  The two sibling release functions in the same file bracket the gap: the
  `ItemInstancePool` path is explicitly hardened against exactly this
  (*"defensive — duplicate-free is a logic bug elsewhere but we don't want to
  corrupt the arena over it"*, `crates/core/src/ecs/resources/mod.rs:1124-1136`)
  while `collect_victim_gpu_handles` is silently *not* hardened. The physics path
  sits between them with no stated position.

  `rapier_release_tests.rs` — the `#1520`/`#1531` regression guard the audit
  checklist points at — covers seven cases (bodies removed, colliders cascaded,
  non-victims spared, victims without handles, absent `PhysicsWorld`, ragdoll
  bodies/colliders/joints, and the both-components sweep). **None passes a
  duplicated victim.** So the property the code currently depends on is not
  pinned, and a future rapier upgrade or a switch to a non-generational handle
  scheme would break it silently.
- **Evidence**: `grep -n "fn " byroredux/src/cell_loader/rapier_release_tests.rs`
  lists `release_removes_victim_bodies_from_physics_world`,
  `release_cascades_colliders_with_body`, `release_leaves_non_victim_bodies_alive`,
  `release_tolerates_victims_without_handles`,
  `release_is_noop_when_physics_resource_absent`,
  `release_removes_ragdoll_bodies_colliders_and_joints`,
  `release_sweeps_both_ragdoll_and_rapier_handles` — every one is over a
  distinct-entity slice.
- **Impact**: none today beyond the wasted work described in
  PHYS-D3-2026-08-27-01. This is a hardening / test-coverage gap: the release
  contract is the single choke point that keeps `RigidBodySet` bounded across a
  session of cell crossings, and one of its preconditions is currently
  unstated and unverified.
- **Related**: PHYS-D3-2026-08-27-01; `#1520` (the original leak this function
  closed); `#1531` (its `Ragdoll` sibling).
- **Suggested Fix**: add
  *release_is_idempotent_over_a_duplicated_victim_list* to
  `rapier_release_tests.rs` — spawn one collider, sync, call
  `release_victim_rapier_bodies(&mut world, &[e, e])`, assert `body_count() == 0`
  and no panic — and state the precondition in the doc comment ("victims may
  repeat; removal is idempotent"). If the preferred posture is instead
  "victims must be distinct", assert that with a `debug_assert` and fix the
  producer.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PHYSICS_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PHYS-D3-2026-08-27-02`._
