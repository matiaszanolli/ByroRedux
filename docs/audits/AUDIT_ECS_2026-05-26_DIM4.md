# ECS Audit — 2026-05-26 (Dimension 4 only: Resource Safety)

**Scope**: single-dimension run via `/audit-ecs 4` — focused audit of the `Resource` API surface in `crates/core/src/ecs/`. No prior Dim 4-only audit exists; most recent multi-dim ECS sweep was 2026-05-16. Other ECS dimensions were not run today.

## Executive Summary

**The Dim 4 contract is fully implemented.** All four core checklist items pass cleanly: `resource()` panics with `std::any::type_name::<R>()` baked into the message; `try_resource()` returns `Option<ResourceRead<'_, R>>` without panicking on miss; `ResourceRead`/`ResourceWrite` are zero-cost typed wrappers around `RwLockReadGuard<Box<dyn Any + Send + Sync>>` that use `downcast_ref`/`downcast_mut` (**no `unsafe`, no transmute, no lifetime cheating**); the entire resource API surface takes `&self`, so systems can mutate via `ResourceWrite` from inside `&World` system bodies. Poison handling is type-aware on every acquisition site. Lock ordering across `resource_2_mut` is TypeId-sorted (ABBA prevention). Same-type double-write asserted at entry.

**Zero NEW findings.** Two INFO callouts (scheduler diagnostic-only access declarations + reentrancy semantics) — both intentional design, not gaps.

| Severity | NEW | INFO |
|----------|-----|------|
| Critical | 0   | 0    |
| High     | 0   | 0    |
| Medium   | 0   | 0    |
| Low      | 0   | 2    |
| **Total**| **0** | **2** |

## Checklist Status

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | `resource()` panics with type name | **PASS** | [world.rs:540-545](crates/core/src/ecs/world.rs#L540-L545) + `:565-570` use `std::any::type_name::<R>()`; test at [world_tests.rs:436-441](crates/core/src/ecs/world_tests.rs#L436-L441) asserts `#[should_panic(expected = "Resource `")]`. Poisoned-lock path at [world.rs:42-51](crates/core/src/ecs/world.rs#L42-L51). |
| 2 | `try_resource()` returns `None` (no panic) | **PASS** | [world.rs:658-667](crates/core/src/ecs/world.rs#L658-L667) returns `Option<ResourceRead<'_, R>>`; mirror at `:674-683` for write. `try_resource_2_mut` at `:704-721` does existence check BEFORE acquiring either lock. |
| 3 | `ResourceRead`/`ResourceWrite` Deref impls type-safe | **PASS** | [resource.rs:47-54](crates/core/src/ecs/resource.rs#L47-L54) (Read), `:88-95` (Write Deref), `:97-103` (Write DerefMut). All use `downcast_ref` / `downcast_mut`; no `unsafe`, no transmute. `PhantomData<R>` makes the wrapper invariant in `R`. Drop impls at `:41-45`, `:82-86` call `lock_tracker::untrack_*`. |
| 4 | Resources usable from systems (`&self`) | **PASS** | `resource()` (line 538), `resource_mut()` (line 563), `resource_2_mut()` (line 592), `try_resource()` (line 658), `try_resource_mut()` (line 674), `try_resource_2_mut()` (line 704) all take `&self`. End-to-end test `resource_visible_to_system_via_scheduler` at [world_tests.rs:482-503](crates/core/src/ecs/world_tests.rs#L482-L503). |

### Extended items (INFO — no findings)

- **Lock ordering across multiple resource acquisitions**: PASS. `resource_2_mut` at [world.rs:617-648](crates/core/src/ecs/world.rs#L617-L648) acquires in `TypeId::of::<A>() < TypeId::of::<B>()` order, identical to `query_2_mut`. The two branches set up `lock_tracker::TrackedWrite` scopes in the same order they acquire the real RwLock. Cites #313 for ABBA-graph rationale.
- **Poisoned-lock handling**: PASS. All 8 RwLock acquisition sites in `world.rs` (lines 549, 574, 623, 626, 638, 641, 664, 680) route poison via `.unwrap_or_else(|_| resource_lock_poisoned::<X>())`, panicking with the type name and a hint pointing at the previous system. `remove_resource` (line 525) does the same. Matches the side-table pattern from #466 E-03.
- **Resource shadowing**: `insert_resource` returns `Option<R>` (the previous value). Test at [world_tests.rs:469-480](crates/core/src/ecs/world_tests.rs#L469-L480). Replace semantics, not panic.
- **Resource removal with guard held**: structurally prevented. `remove_resource` takes `&mut self`; guards are bound by `&self`. Borrow checker rejects at compile time — no runtime check needed.
- **Reentrancy on `resource::<T>()` from the same thread**: `std::sync::RwLock` is used (not `parking_lot`). Reentrancy is not portably safe, but `lock_tracker` panics on same-thread reentry BEFORE the real lock acquisition. **Multiple reads on the same type from the same thread are explicitly allowed** ([lock_tracker.rs:58-95](crates/core/src/ecs/lock_tracker.rs#L58-L95)); read-then-write panics with a clear message ([lock_tracker.rs:117](crates/core/src/ecs/lock_tracker.rs#L117)); write-then-anything panics ([lock_tracker.rs:109](crates/core/src/ecs/lock_tracker.rs#L109)).
- **Scheduler integration (M27)**: INFO — see cross-cutting notes.

## Findings

None — Dim 4 surface is mature.

## Cross-cutting notes

- **Scheduler `Access` declarations are diagnostic, not enforcing.** [scheduler.rs:499-540](crates/core/src/ecs/scheduler.rs#L499-L540) (`access_report()`) reports conflicts but `Scheduler::run` at [scheduler.rs:407-472](crates/core/src/ecs/scheduler.rs#L407-L472) iterates `parallel` with `rayon::par_iter_mut` regardless of declared conflicts. Header comment at [scheduler.rs:6](crates/core/src/ecs/scheduler.rs#L6) is explicit: "the per-storage `RwLock` design naturally serialises conflicting accesses." Correctness is preserved by `lock_tracker`'s global ABBA graph + per-resource RwLock; declared `Access` is purely diagnostic. **This is intentional design, not a Dim-4 gap.** If a Dim 5 audit wants enforcement, the partition would live in `Scheduler::run`'s rayon dispatch (split `data.parallel` into compatible cohorts).
- **Dim 7 (Lifecycle)**: `remove_resource` returns `Option<R>` (owned). Borrow checker prevents drop-while-held. No gap.
- **lock_tracker integration**: the Dim 4 surface relies entirely on `lock_tracker::TrackedRead/Write` + `defuse()` for the no-leak Drop contract. Pattern is consistent with `query.rs`'s usage; both paths share the same lifetime/scope invariant — `defuse()` after successful real-lock acquisition.
- **Test budget**: `cargo test -p byroredux-core` currently exercises 10 resource tests in [world_tests.rs:396-510](crates/core/src/ecs/world_tests.rs#L396-L510). Coverage is good for Dim 4.

## Methodology

1. Walked the 4 entry points (`resource.rs`, `resources.rs`, `world.rs`, `access.rs`).
2. Cross-checked each checklist item against named test in `world_tests.rs`.
3. Verified all 8 RwLock acquisition sites route poison through the typed panic helper.
4. Spot-checked `scheduler.rs` for M27 access-declaration consumer behavior (diagnostic-only, intentional).
5. Dedup baseline at `/tmp/audit/ecs/issues.json` (state=all, ~157KB) — no existing issue covers any new Dim 4 concern (which is expected since there are no NEW findings).

---

**Recommendation**: no `/audit-publish` needed — zero NEW findings. Filed as a delta-baseline doc only; next ECS audit can reference this as the "Dim 4 verified clean on 2026-05-26" anchor.
