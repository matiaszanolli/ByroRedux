# #3445 — CONC-D3-2026-08-27b-03: studio_host::snapshot inverts the canonical order's Name → StringPool tail, closing a 2-cycle against resolve_entity_name and the debug evaluator

**Severity**: MEDIUM · **Dimension**: 3 (ECS Lock Ordering & Deadlock)
**Location**: `byroredux/src/studio_host.rs::snapshot`

## Fix

Verified the premise: `pool = world.try_resource::<StringPool>()` was
acquired once outside the per-entity `filter_map` closure and stayed
alive across every entity's `Transform` and `Name` reads, inverting the
canonical `… → Name → StringPool` tail `docs/engine/ecs.md` fixes for
this cluster — the exact reverse of `resolve_entity_name`
(`commands/shared.rs`) and the debug evaluator's own established order.

Applied the issue's "better" suggested option: reuse `resolve_entity_name`
directly instead of re-deriving the acquisition. Its own guards (`Name`
then the string pool) are fully dropped before it returns, so calling it
once per entity — before `Transform`/`Material` are touched for that same
entity — means no storage lock is ever held across a different storage's
acquisition at all, stronger than just reordering the two reads.

`resolve_entity_name` lives in `commands::shared`, a module private to
`commands` and not reachable from `studio_host` (a sibling module).
Widened `mod shared;` to `pub(crate) mod shared;` — the minimal visibility
change needed, matching this session's own established precedent for
"a helper needs to be reached from outside its original module" (#2530).

## SIBLING (issue's own checklist item)

- `CONC-D3-2026-08-27b-04` (`cinematic_animation_event_system`, the named
  LOW sibling) — already **CLOSED** as #3446.
- The rest of `studio_host.rs` — no other `StringPool` acquisition exists
  in the file after this fix (grep confirms).
- Broader scan for "any other `StringPool`-first walk" — two more sites
  found (`asset_provider/animation.rs`, `scene/nif_loader.rs`), both
  already correctly scoped: `nif_loader.rs`'s own comment documents
  "the pool read lock is short-lived" and its block-expression scope
  confirms no other storage is touched while it's held; `animation.rs`'s
  site is test-only and never acquires a second storage type inside its
  loop. Neither needed a fix. The two sites the issue's own evidence
  already names as correct (`commands/assets.rs`'s `skin.list`, the
  debug evaluator with its dedicated #2388 regression test) were
  re-confirmed unchanged.

## LOCK_ORDER (issue's own checklist item)

`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux
studio_host::` is clean. No `TypeId`-sorted paired-query API was
involved (the fix is sequential single-storage acquisitions, not a
`query_2_mut` pair), so nothing there to preserve.

## TESTS (issue's own checklist item — "extend the
`debug_evaluator_acquires_locks_in_canonical_order` pattern to
`studio_host.rs`")

The existing evaluator pattern scans a function body for direct
`world.query::<T>()`/`world.resource::<T>()` acquisitions and ranks them
against the canonical order — but the whole point of this fix is that
`snapshot` no longer directly acquires `Name`/`StringPool` at all, so
that literal pattern wouldn't detect anything meaningful here. Adapted it
into the shape that actually pins this fix:

- `snapshot_body_does_not_directly_acquire_string_pool_or_name` — a
  source scan asserting `snapshot`'s function body calls
  `resolve_entity_name` and contains neither `StringPool` nor a direct
  `Name` acquisition.
- `snapshot_still_resolves_entity_names_through_the_shared_helper` — the
  behavioral half: an entity with a real `Name` component still resolves
  to its (lowercased, `StringPool::intern`'s own case-fold convention)
  string through the refactored path.

Hit the self-matching trap while writing the structural test: the
function's own new explanatory comment initially spelled out
"StringPool" literally, which the source scan then matched against its
own describing prose. Reworded the comment to describe the resource in
general terms instead (documented inline why, matching this session's
established convention for the same hazard).

**Reintroduce-and-revert verification**: temporarily restored the exact
pre-fix shape (`pool` acquired via `try_resource` before the entity loop,
`Transform` read first, then `Name`+`pool.resolve` inline) — confirmed
the structural test failed
(`"snapshot must resolve names through the shared canonical-order helper"`).
Restored the fix and reran — all 4 tests in `studio_host::tests` pass
again.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -q -p byroredux --bin byroredux studio_host::`: 4 passing,
  0 failing (+2 new).
- `cargo test -q -p byroredux --bin byroredux`: 1897 passing, 0 failing
  (full binary crate, unaffected elsewhere).
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux --bin byroredux
  studio_host::`: 4 passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7187 passing, 0
  failing**.
