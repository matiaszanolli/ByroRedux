# ECS-D1-01: Cross-thread ABBA detector is documented as 'debug builds only' at six sites, omitting that it is off by default

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2547
**Finding ID**: ECS-D1-01

**Severity**: MEDIUM
**Dimension**: 1 — Lock Ordering & Deadlock (doc rot on an active safety property)
**Location**: `docs/engine/ecs.md:584`; `crates/core/src/ecs/world.rs:498-499`, `:568-569`, `:730-731`, `:854-855`; `crates/core/src/ecs/lock_tracker.rs:15`
**Status**: NEW (Regression of #1784 — CLOSED, fix applied to only half the issue; no other open issue tracks this residual gap)

## Description
Every user-facing description of the cross-thread ABBA graph says it runs in "debug builds only". It does not: it additionally requires `BYRO_LOCK_ORDER_CHECK=1` in the environment, and is inert without it. Closed issue #1784 asked for the headers to say "only the cross-thread ABBA graph is debug-only + `BYRO_LOCK_ORDER_CHECK`-gated". The always-on-in-release half of that fix landed; the env-var half did not, at any of the six sites. The gate is documented correctly only inside `lock_tracker.rs`'s `global_order` module doc (`:165`, `:186`), 150 lines below the module header that contradicts it.

## Evidence
Confirmed directly:
```
world.rs:497-499   /// (debug and release builds) and panics if a conflicting lock on
                   /// `A` or `B` is already held on the same thread. In debug builds
                   /// only, it additionally panics if the ordered lock graph detects
ecs.md:584         2. **Global lock-order graph** (debug builds only, #313) — records observed
lock_tracker.rs:15 //! 2. **Global lock-order graph** (debug builds only — see #313). Records
```
vs. `lock_tracker.rs:225-226`:
```rust
static ENABLED: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(std::env::var_os("BYRO_LOCK_ORDER_CHECK").is_some()));
```

## Impact
A maintainer adding a system, reading `query_2_mut`'s `# Panics` section or `docs/engine/ecs.md`'s lock-ordering policy, reasonably concludes that a plain `cargo test` or a debug engine run enforces cross-thread lock ordering. It does not. A genuine ABBA introduced today ships silently unless someone remembers the opt-in. This false premise has already produced mis-scoped work (#2137's whole investigation was "the detector is compiled in but inert"). Blast radius is the entire ECS lock-safety story — this is the only documentation of it.

## Related
#1784 (closed, partial fix), #2137, #313, #1410, and the sibling ECS-D1-04 finding (already tracked as #2387) — the same property is also untested.

## Suggested Fix
Append "and opt-in via `BYRO_LOCK_ORDER_CHECK=1`" to all six sites, and make `lock_tracker.rs`'s module header (`:15`) state the gate rather than deferring it 150 lines. Consider extending `.claude/commands/_audit-validate.sh`'s advisory pass to flag "debug builds only" near "#313".

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change); consider a source-scan advisory check that flags "debug builds only" without the env-var qualifier
