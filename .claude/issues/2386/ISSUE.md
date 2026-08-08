# #2386 — ECS-D1-05: Recursive same-type read locking is explicitly whitelisted by the tracker and invisible to the ABBA graph

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2386


- **Severity**: LOW
- **Dimension**: 1 — Lock Ordering & Deadlock (defense-in-depth gap)
- **Location**: `crates/core/src/ecs/lock_tracker.rs:74` (`read_count += 1`), `:419-428` (`multiple_reads_same_type_ok`), `:83-84` (`if is_new` edge gate)
- **Status**: NEW

**Description**

`World::query::<T>()` issues an independent `RwLock::read()` per call, so two live `QueryRead<T>` guards on one thread is a genuine recursive read lock. `std::sync::RwLock` documents this as a deadlock hazard (a parked writer can block a second reader while the first is still held on some platforms). The tracker deliberately permits it — `track_read` counts instead of rejecting, and `multiple_reads_same_type_ok` pins that as intended behaviour. The ABBA graph cannot see it either: `record_and_check` is called only on the 0→held transition and `held_others` filters out the incoming type, so no `T → T` edge is ever recorded. This is the one deadlock class in the model that no guard covers.

**Evidence**:

```rust
// lock_tracker.rs:419-428 — the whitelist, as a passing test
#[test]
fn multiple_reads_same_type_ok() {
    let id = TypeId::of::<FakeA>();
    track_read(id, "FakeA"); track_read(id, "FakeA"); track_read(id, "FakeA");
```

**Impact**

Latent, not live. A scope-aware held-set scan of `byroredux/src` and every ECS-consuming crate found no production path that holds two same-type guards simultaneously (all candidates from the Dimension-1 sweep were statement-lifetime temporaries or test code), so this is reported as a gap, not a confirmed bug. It only bites once (a) some helper called under a `query::<T>()` guard also takes `query::<T>()`, and (b) another parallel-batch system writes `T` — and the declared-access analyzer would flag (b) as a `ReadWrite` conflict first, provided both systems declare (undeclared systems return `Unknown`, not `None`, so that backstop is advisory only).

**Related**: #313, ECS-D1-04, #2153 / #2269 (the "safety rests on undocumented discipline" family).

**Suggested Fix**: Either reject the second same-type read outright in `track_read` (a real behaviour change — check callers first), or, cheaper, document the hazard on `World::query` and add a debug-only `log::warn!` on `read_count` transitioning 1→2 so a recursive read is at least visible in a trace.

## Completeness Checks
- [ ] **SIBLING**: Re-run the held-set scan on the current tree before shipping any behaviour change, in case a new caller has since introduced a real recursive-read site
- [ ] **TESTS**: If a `log::warn!` or rejection is added, pin it with a test covering both the warn-and-continue and (if chosen) reject paths

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
