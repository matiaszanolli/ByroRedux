# #3680 — PERF-D1-2026-08-30-04: the lock tracker materialises its `held_others` snapshot *before* the detector's own enabled check, so every ECS lock acquisition in a debug build heap-allocates while the code documents that path as "one relaxed load"

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D1-2026-08-30-04`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,ecs,concurrency,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3680

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/lock_tracker.rs:115-122` (`track_read`) and `:160-167` (`track_write`); the early-outs they feed are at `:344-348`
- **Status**: NEW. **Not** a regression of #823 — that baseline is "the `held_others` collection is `#[cfg(debug_assertions)]`-gated", and it still is (re-verified; `AUDIT_ECS_2026-08-30.md:775` records the same). This finding is about the *debug*-build path #823 deliberately left on, and about the cost claim written beside the enabled flag.
- **Description**: `ENABLED`'s doc comment states the design intent plainly: "*Cached in an atomic so the per-acquire fast-path is one relaxed load*" (`lock_tracker.rs:270-272`). It is not. Both callers build the snapshot unconditionally inside the `cfg(debug_assertions)` block and only then call `record_and_check`, which checks `held_others.is_empty()` first and `ENABLED.load(...)` second. So in any debug build with `BYRO_LOCK_ORDER_CHECK` unset — the default, and the mode `CLAUDE.md`'s Quick Reference documents as the way to launch the engine (`cargo run`) and run the suite (`cargo test`) — every `world.query` / `query_mut` / `resource` / `try_resource` / `World::get` taken while at least one other lock is held iterates the thread-local `HashMap` and collects a `Vec<(TypeId, &'static str)>` that is then discarded at the `ENABLED` load.
- **Evidence**:
```rust
// crates/core/src/ecs/lock_tracker.rs:115-122  (identical block at :160-167)
#[cfg(debug_assertions)]
{
    let held_others = locks
        .borrow()
        .iter()
        .map(|(id, state)| (*id, state.type_name))
        .collect::<Vec<_>>();                       // allocates before any enabled check
    global_order::record_and_check(type_id, type_name, &held_others);
}

// :344-348 — the checks that make the work pointless, both AFTER the collect
if held_others.is_empty() { return; }
if !ENABLED.load(Ordering::Relaxed) { return; }
```
  The nesting depth is what makes it compound: `collect_static_mesh_draws` holds ~24 read queries concurrently (`byroredux/src/render/static_meshes.rs:100-166`), so acquisitions 2..24 each allocate a `Vec` of length 1..23; and `animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` across the whole body (`byroredux/src/systems/animation.rs:515-600`) while re-acquiring `Transform` / `AnimationTextKeyEvents` / the animated-channel sinks **per animated entity** (`:696`, `:757`, `:766-793`).
- **Impact**: Debug-build only — release is genuinely zero-cost, and #823's stated contract is not violated. But debug is the everyday development and test configuration, so this inflates `cargo test` wall time and makes any debug-build profile of the ECS hot path unrepresentative. Allocation count scales as (locks already held) × (acquisitions per frame), which for the animation path is per-entity: the checked-in actor baselines are `skin_pool_live` 206 / 248 / 83 (`.claude/audit-baselines/runtime/*.tsv`). **No `dhat` or allocation guard covers this site.** I did not measure the wall-clock cost and it is unknown.
- **Related**: #823 (ECS-PERF-01, the `cfg(debug_assertions)` gate — still intact); #2675 (the reachability generalisation inside `record_and_check`); #2384; `AUDIT_ECS_2026-08-30.md` ECS-D3-01, which reports a *correctness* gap in the same block (the `recursive_read` early return skipping `record_and_check`) — a fix for that and a fix for this touch the same lines and should land together.
- **Suggested Fix**: Expose a `global_order::is_enabled()` (or reuse `set_enabled_for_tests`' flag) and hoist both the emptiness and the enabled test above the `collect`: `if locks.borrow().len() > 0 && global_order::is_enabled() { … }`. Behaviour is identical — `record_and_check` already returns on both conditions — and the fast path then really is one relaxed load, as documented.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
