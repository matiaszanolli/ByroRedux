# ECS Audit — Dimension 3: World Integrity — 2026-05-23

**Scope:** Targeted single-dimension audit per `/audit-ecs 3`.

**Predecessors:**
- [AUDIT_ECS_2026-05-16.md](AUDIT_ECS_2026-05-16.md) (last full sweep — dim 3 clean,
  baselined `storage_write` downcast / `find_by_name` integer-equality / EntityId monotonicity).
- [AUDIT_ECS_2026-05-23_DIM5.md](AUDIT_ECS_2026-05-23_DIM5.md) (today's dim 5
  sibling — surfaced the M27 declared-access cluster #1236 / #1237 / #1238).

---

## Executive Summary

**Zero NEW findings.** Dim 3 is in steady-state. The four bullets
(TypeMap downcast safety / lazy storage init / EntityId monotonicity /
`find_by_name` integer equality) are all covered by the live 86-test
world suite — `cargo test -p byroredux-core --lib world` runs **86/86
green**.

The two core world/storage files (`world.rs`, `storage.rs`,
`sparse_set.rs`, `packed.rs`) have had **zero commits since the
2026-05-16 sweep**:

```
$ git log --since="2026-05-16" --oneline -- crates/core/src/ecs/world.rs \
    crates/core/src/ecs/storage.rs crates/core/src/ecs/sparse_set.rs \
    crates/core/src/ecs/packed.rs
(no output)
```

All the recent ECS activity (M27 Phase 1-3 = access/scheduler;
post-#1212/#1213/#1214 = cell-loader spawn-site plumbing) landed
*around* the world API surface, not in it. The world integrity
invariants this dimension checks are unchanged.

### Severity rollup

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |

Zero findings — no `/audit-publish` step needed.

---

## Verified-Clean Checks

### Bullet 1 — TypeMap downcast: `downcast_ref` / `downcast_mut` always correct (TypeId matches)

Three downcast sites in [`world.rs`](../../crates/core/src/ecs/world.rs):
| Site | Function | Failure handling |
|------|----------|------------------|
| `world.rs:224` | `remove::<T>` | `.downcast_mut::<T::Storage>()?` — propagates `None` |
| `world.rs:271` | `get_mut::<T>` | `.downcast_mut::<T::Storage>()?` — propagates `None` |
| `world.rs:743` | `storage_write::<T>` (internal) | `.expect("storage type mismatch (bug in World)")` |

All three are reachable only if a single `TypeId` somehow maps to a
storage of the wrong concrete type — Rust's `TypeId` per-type uniqueness
makes that structurally impossible. The two flavours are intentional:

- `remove` / `get_mut` use `?` because the outer storage lookup
  (`self.storages.get(...)?`) returns `None` when no entity ever had
  the component (lazy storage). The downcast `?` is a no-op defensive
  no-panic path on top.
- `storage_write` uses `expect(...)` because its caller path
  unconditionally creates the storage on the entry-or-insert branch.
  Any TypeId mismatch there would represent a `World` bug, hence the
  loud panic.

**Coverage**: `lazy_storage_init`, `query_returns_none_for_unregistered`,
`remove_nonexistent_does_not_create_storage`,
`get_mut_nonexistent_does_not_create_storage` exercise the No-Op /
None-propagating downcast path. The "bug in World" panic path is
correctly unreachable in production.

### Bullet 2 — Lazy storage init: first insert creates, queries before insert return None

[`world.rs:347-352`](../../crates/core/src/ecs/world.rs#L347-L352) (`query<T>`)
and [`world.rs:365-371`](../../crates/core/src/ecs/world.rs#L365-L371) (`query_mut<T>`)
both short-circuit at `self.storages.get(&type_id)?` — no storage,
return `None`. First `insert<T>` calls `storage_write<T>` at
[`world.rs:726-745`](../../crates/core/src/ecs/world.rs#L726-L745),
which `entry(type_id).or_insert_with(|| ...)` creates the storage on
demand. `register<T>` at [`world.rs:99-110`](../../crates/core/src/ecs/world.rs#L99-L110)
is the explicit pre-create path for setup code that wants queries
to succeed before any entity has the component.

**Coverage**: 5 tests pin this end-to-end —
- `lazy_storage_init` (raw `count<T>` / `has<T>` on virgin world)
- `query_returns_none_for_unregistered`
- `query_after_register`
- `remove_nonexistent_does_not_create_storage` (#39 regression)
- `get_mut_nonexistent_does_not_create_storage` (#39 regression)

### Bullet 3 — Entity ID monotonic (no reuse, no overflow handling)

`World::spawn` at [`world.rs:85-92`](../../crates/core/src/ecs/world.rs#L85-L92):

```rust
pub fn spawn(&mut self) -> EntityId {
    let id = self.next_entity;
    self.next_entity = self
        .next_entity
        .checked_add(1)
        .unwrap_or_else(|| panic!("World::spawn overflowed EntityId (u32::MAX reached)"));
    id
}
```

`next_entity` is `u32` (per `EntityId` type alias). `checked_add(1)`
panics rather than wraps, so the silent-aliasing failure mode from
#36 is structurally closed. `despawn` at
[`world.rs:121-136`](../../crates/core/src/ecs/world.rs#L121-L136)
walks storages but never decrements `next_entity` (#372 — reuse
without generational tagging would alias components). The class of
bug is closed.

**Coverage**:
- `despawn_does_not_reclaim_entity_ids` (#36 / #372 regression — spawn
  after despawn must advance, never reuse).
- `spawn_panics_on_entity_id_overflow` (#36 regression — jams
  `next_entity = EntityId::MAX` and verifies the next call panics with
  the "overflowed EntityId" message). Marked `#[should_panic]`.

Documented limitation: 4 G spawns in one process (~770 days at 60 Hz
of pure spawning) hit the panic. Effectively unreachable for game
sessions; long-running fuzzers or test harnesses are the only path
to it and they should be running with `--cfg debug_assertions` for
the early checked panic.

### Bullet 4 — `find_by_name` / `find_by_form_id` integer equality (no string compare in scan)

**`find_by_name`** at [`world.rs:307-318`](../../crates/core/src/ecs/world.rs#L307-L318):
```rust
let pool = self.try_resource::<StringPool>()?;
let sym = pool.get(name)?;            // ← string-compare ONCE, here
drop(pool);
let names = self.query::<Name>()?;
let result = names.iter().find(|(_, n)| n.0 == sym).map(|(id, _)| id);
//                                  ^^^^^^^^^^^^^^^^ ← integer eq on FixedString
```

`FixedString` is a type alias for `string_interner::DefaultSymbol`
at [`crates/core/src/string/mod.rs:18`](../../crates/core/src/string/mod.rs#L18),
which is a `u32`-slot index with derived `PartialEq` (integer equality).
The lookup pays one string-compare for the pool-side intern (O(name
length) hash + slot lookup), then the linear scan is N × u32-equality
— well under a microsecond even at 10k Name components.

**`find_by_form_id`** at [`world.rs:324-330`](../../crates/core/src/ecs/world.rs#L324-L330):
```rust
let q = self.query::<FormIdComponent>()?;
let result = q.iter().find(|(_, fid)| fid.0 == id).map(|(eid, _)| eid);
//                                ^^^^^^^^^^^^^^ ← FormId == FormId (integer)
```

`FormId` is also an integer wrapper. Linear scan over N
FormIdComponent entries.

**Coverage**: 4 + 4 tests cover the full hit/miss × resource-present/absent matrix —
- `find_by_name_hit`, `find_by_name_miss`, `find_by_name_no_pool`,
  `find_by_name_no_name_components`
- `find_by_form_id_hit`, `find_by_form_id_miss`,
  `find_by_form_id_no_components`, `form_id_pool_as_world_resource`

**Production consumers (non-test)**:
```
$ grep -rn "find_by_form_id\|find_by_name" --include="*.rs" \
    /mnt/data/src/gamebyro-redux/byroredux \
    /mnt/data/src/gamebyro-redux/crates | grep -v "fn find_by\|//\|tests"
crates/debug-server/src/evaluator.rs:317  (find_by_name — Papyrus inspect)
crates/debug-server/src/evaluator.rs:523  (find_by_name — Papyrus inspect)
crates/debug-server/src/evaluator.rs:527  (find_by_name — Papyrus inspect)
```

Only the debug-server Papyrus evaluator calls these — interactive,
not per-frame. No hot-path consumer today; the O(N) scan stays well
within the interactive-latency budget. If a future Papyrus
`ObjectReference.Disable()` consumer makes the call per-frame, a
secondary `HashMap<FormId, EntityId>` index would pay back the
maintenance cost — flag for that future audit, not for this one.

---

## Bonus checks (passed)

- **type_names side-table sync** (#466): both `register<T>` (L99-110)
  and `storage_write<T>` (L726-733) populate
  `self.type_names.entry(type_id).or_insert_with(std::any::type_name::<T>)`
  *alongside* the storage entry. No path creates a storage without
  also recording the type name — the fallback `<unknown>` at the
  `despawn` panic path ([`world.rs:131`](../../crates/core/src/ecs/world.rs#L131))
  is defense-only, unreachable in practice.
- **`get_mut::<T>` vs `get::<T>` asymmetry**: `get` takes `&self` and
  returns a `ComponentRef<'_, T>` (lock-guard wrapper); `get_mut` takes
  `&mut self` and returns `&mut T` directly, using
  `RwLock::get_mut()` to bypass the atomic ops on the lock. The
  asymmetry is intentional perf optimisation and the safety boundary
  is the `&mut self` exclusivity, not the lock.
- **No new commits to world.rs / storage.rs / sparse_set.rs / packed.rs
  since 2026-05-16** — confirmed via `git log`. The dim 3 surface
  hasn't shifted under us.

---

## Summary

| Severity | Count | Disposition |
|----------|-------|-------------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 0 | — |
| LOW      | 0 | — |

**Headline:** Dim 3 is in genuine steady-state. All four bullets are
covered by the existing 86-test world suite; both the core invariants
(TypeId-keyed downcasts, lazy init, monotonic EntityId, integer-eq
scans) and the surrounding plumbing (type_names side-table,
`get`/`get_mut` asymmetry) verified clean. No NEW findings, no
regressions, no /audit-publish step needed.

The forward-looking note (if `find_by_form_id` ever becomes per-frame,
add a `HashMap<FormId, EntityId>` secondary index) is a hypothetical;
no production caller exercises that path today. Re-check at the next
full sweep — or sooner if a Papyrus consumer starts spamming
`Game.GetForm().Disable()`.
