# #3895: TD9-2026-09-05-05: `cargo test -p byroredux-core` — the command CLAUDE.md documents for core tests — silently drops the two `inspect`-gated round-trip tests that only the workspace lane compiles

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3895 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: `/mnt/data/src/gamebyro-redux/crates/core/src/animation/player.rs` (`inspect_tests::reverse_direction_round_trips_through_json`) and `/mnt/data/src/gamebyro-redux/crates/core/src/animation/stack.rs` (`inspect_tests::stack_round_trips_reverse_and_blend_state`); documented command in `/mnt/data/src/gamebyro-redux/CLAUDE.md` Quick Reference
- **Status**: NEW
- **Description**: Both modules are `#[cfg(all(test, feature = "inspect"))]`, and `byroredux-core`'s `default` is `["parallel-scheduler"]` only. They compile in CI purely by feature unification: `byroredux-save` depends on `byroredux-core` with `features = ["save"]`, and `save = ["inspect"]`, so `cargo test --workspace` builds core once with `inspect` on. **CI coverage is therefore fine** — this is the answer to the "feature-gated tests never enabled in CI" triage bullet, and I verified it empirically rather than by reasoning about the resolver.

  The gap is the *documented developer command*. `CLAUDE.md` tells contributors to run `cargo test -p byroredux-core` for core tests; that invocation does not pull `byroredux-save` in, so `inspect` stays off and both #486 serialization guards vanish from the run with no diagnostic. A contributor iterating on `AnimationPlayer`/`AnimationStack` locally gets a green that omits precisely the two tests covering the field they are editing.
- **Evidence**:
  ```
  cargo test -p byroredux-core --lib -- --list                    → 742 tests, 0 matches for inspect_tests
  cargo test -p byroredux-core -p byroredux-save --lib -- --list  → animation::player::inspect_tests::reverse_direction_round_trips_through_json
                                                                    animation::stack::inspect_tests::stack_round_trips_reverse_and_blend_state
  ```
  `crates/core/Cargo.toml`: `default = ["parallel-scheduler"]`, `inspect = [...]`, `save = ["inspect"]`; `crates/save/Cargo.toml`: `byroredux-core = { workspace = true, features = ["save"] }`.
- **Impact**: Local false confidence on the animation-serialization path only; CI still catches it before merge. Bounded.
- **Related**: #486 (the issue both tests guard). **Cross-dimension note for the merge step**: the same CLAUDE.md line also claims "(162 tests)" where the real lib figure is **742** — that number is pure doc rot and belongs to **Dimension 3**, not here; it is flagged rather than double-filed, per the Cross-Dimension Dedup rule.
- **Suggested Fix**: Change the Quick Reference line to `cargo test -p byroredux-core --features inspect` (or `-p byroredux-core -p byroredux-save`) so the documented command matches what CI actually exercises.
- **Effort**: **Trivial**.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
