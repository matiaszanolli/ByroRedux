# REG-2026-08-20-D6-01: #2987 removed the Skyrim engine-enum key space but ActorValues doc still declares it

**Issue**: #3216 — https://github.com/matiaszanolli/ByroRedux/issues/3216
**Severity**: LOW
**Labels**: `low,ecs,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D6-01 (Dimension 6 — Carry-forward of the 2026-08-16 report's own findings).

**Severity**: LOW (doc-only)
**Status**: Residual of `REG-2026-08-16-D1-02` — that finding's functional half is resolved; this is the narrowed remainder. No prior issue was filed for it.
**Location**: `crates/core/src/ecs/components/actor_values.rs:15-18` (the false contract); `byroredux/src/commands_tests.rs:563` (a fixture still using the retired key).

## Description

**#2987** (`ESM-2026-08-16-D7-02`, HIGH, closed 2026-08-17) established that the premise behind the second key space was false — *"Vanilla `Skyrim.esm` contains 149 `AVIF` records and one of them is `AVHealth`, FormID `0x000003E8`"* — and removed `SKYRIM_HEALTH_ACTOR_VALUE`.

`health_actor_value_key` now returns the real remapped AVIF FormID for Skyrim, so `ActorValues` is back to **a single key space**, and `crates/scripting/src/condition.rs`'s `GetActorValue` arm (whose doc comment asserts exactly that) is correct again.

**The module doc that *created* the confusion was not updated.** `actor_values.rs:15-18` still reads:

> *"Built-in TES5 actor values use Skyrim's engine enum index (for example Health is 24), because vanilla does not author `AVIF` records for them."*

Both clauses are now false, and the sentence sits at the top of the file that defines the canonical component's **key contract**.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -rn "engine enum" --include='*.rs' crates/ byroredux/
crates/core/src/ecs/components/actor_values.rs:16://! space**. Built-in TES5 actor values use Skyrim's engine enum index (for

$ grep -rn "SKYRIM_HEALTH_ACTOR_VALUE" --include='*.rs' .
(no output)  ← removed by #2987

$ grep -n "health_actor_value_key" crates/plugin/tests/parse_real_esm.rs
191:    assert_eq!(index.health_actor_value_key(), Some(0x0000_03E8));
```

**One** hit for "engine enum" — the doc comment. No production code produces enum-index keys any more.

Meanwhile `byroredux/src/commands_tests.rs:563` still constructs the retired enum index as a fixture:

```rust
world.insert(target, byroredux_core::ecs::components::ActorVitals { health: 24 });
```

## Impact

Doc-only, hence LOW. Recorded because **this exact sentence produced a MEDIUM finding in the previous sweep** (`REG-2026-08-16-D1-02`), and leaving it in place will produce the same false finding again next sweep: an auditor reading the contract doc concludes the two-key-space hazard is live when it is not.

**A stale invariant in a doc comment misleads as effectively as a stale assertion in a test** — and this one has now cost two audit cycles.

## Suggested Fix

1. Rewrite `actor_values.rs:13-19` to state the **restored single-space rule**, citing #2987 for why the engine-enum workaround was withdrawn.
2. Describe `ActorVitals` as the per-game **Health key carrier**, not as a bridge between two key spaces.
3. Change the `commands_tests.rs:563` fixture to a plausible AVIF FormID so no reader mistakes `24` for a live convention.

## Related

- **#2987** — removed the second key space
- **#2986** — the `AV`-prefix root cause
- **#1663** — the original single-space contract
- `AUDIT_REGRESSION_2026-08-16.md` § `REG-D1-02` — the finding this closes out

## Completeness Checks
- [ ] **SIBLING**: Both the module doc *and* the `commands_tests.rs` fixture updated — the fixture is the second place a reader learns `24`
- [ ] **NO-STALE-INVARIANT**: `grep -rn "engine enum" --include='*.rs' crates/ byroredux/` returns nothing
- [ ] **TESTS**: `parse_real_esm.rs:191`'s `0x3E8` pin still green after the fixture change
