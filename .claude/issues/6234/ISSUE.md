# #6234: ECS-2026-10-21-03: the #6101 contention test turns the Miri gate red

**Labels**: medium,ecs,safety,test-gap,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 3 Query Borrow Safety & the ECS `unsafe` · **Location**: `crates/core/src/ecs/world_tests.rs:927-967`

- **Location**: `crates/core/src/ecs/world_tests.rs:927-967` (`try_resource_2_does_not_deadlock_against_resource_2_mut`); `.github/workflows/ci.yml:326-327` (the Miri step runs every `ecs::world::tests` test except `resource_visible_to_system_via_scheduler`)
- **Status**: NEW, introduced by `824c620a5` (Fix #6101). CI Miri job: green on `7822695e4`, red on `21b9743e0`, `1be33af66` and HEAD.
- **Description**: The test spins two reader threads and one writer for 2000 iterations each, then waits on `recv_timeout(Duration::from_secs(20))`. Miri runs about three orders of magnitude slower, so the 6000 lock round-trips do not finish in 20 s of wall time and the test panics with `a sorted pair acquisition must not deadlock: Timeout` (CI run 39633944988). The panicking main thread then leaves the workers running, and Miri reports a second error: `the main thread terminated without waiting for all remaining threads`.
- **Impact**: The `ecs-query-miri` job exists to catch aliasing UB in the four cached-pointer derefs in `query.rs` (#1367). While it is red, a genuine Stacked/Tree-Borrows violation in a new `ecs::world::tests` case would land unnoticed. The test itself is sound on native.
- **Related**: #6101, #1367.
- **Suggested Fix**: Mark the test `#[cfg_attr(miri, ignore)]`. It tests lock order, not pointer provenance, so this does not widen the cached-pointer coverage. Alternatively, scale the iteration count with `if cfg!(miri) { 10 } else { 2000 }`. Either way, record the exception next to the CI skip-list comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
