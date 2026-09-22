# #4770: SF-2026-09-22-D4-01: warn_unmodelled_txst_slot's regression guard depends on being the sole owner of a process-global, unresettable seen-set

**Severity**: LOW
**Dimension**: 4 (ESM + cell bring-up)
**Location**: `crates/plugin/src/esm/cell/support.rs:704-713` (`warn_unmodelled_txst_slot`); `crates/plugin/src/esm/cell/tests/txst.rs:654-697` (`starfield_pbr_slots_parse_without_dropping_the_record`)
**Labels**: low, test-gap, bug, game:starfield
**Source**: docs/audits/AUDIT_STARFIELD_2026-09-22.md (SF-2026-09-22-D4-01)

## Description

`warn_unmodelled_txst_slot`'s first-sighting gate (added by #4438) is a
`static OnceLock<Mutex<HashSet<[u8;4]>>>` scoped to the whole test/production
process, with no test-only reset. Its regression test asserts directly on
this global state, including an assertion that `TX17` is seen for the first
time ever (`assert!(warn_unmodelled_txst_slot(b"TX17"))`). That holds today
only because no other test in the crate touches `TX08`/`TX09`/`TX17`/`TX19`.

## Evidence

`crates/plugin/src/esm/cell/support.rs:704-713`:
```rust
pub(super) fn warn_unmodelled_txst_slot(fourcc: &[u8]) -> bool {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<[u8; 4]>>> =
        std::sync::OnceLock::new();
    let seen = SEEN.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut key = [0u8; 4];
    key.copy_from_slice(&fourcc[..4]);
    seen.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key)
}
```

`crates/plugin/src/esm/cell/tests/txst.rs:692-696`:
```rust
    assert!(!warn_unmodelled_txst_slot(b"TX08"));
    assert!(warn_unmodelled_txst_slot(b"TX17"));
    assert!(!warn_unmodelled_txst_slot(b"TX17"));
```

`grep -rn '"TX17"\|b"TX17"\|TX08\|TX09\|TX19' crates/plugin/src/esm/cell/tests/ crates/plugin/src/esm/records/`
finds no other reference to these four FourCCs anywhere else in the crate.
Re-verified at HEAD `c3f298a24`, unchanged since #4438 landed it.

## Impact

Latent. A future TXST test touching any of these four FourCCs (e.g. the
real-data `Starfield.esm` TXST census the module doc still lacks) will make
this test's outcome depend on `cargo test`'s thread-scheduling order — an
intermittent failure with no message pointing at the real cause.

## Related

#4438 (the fix this guards).

## Suggested Fix

Add a `#[cfg(test)]`-only reset function and call it at the top of the
test, or use FourCCs reserved exclusively for tests instead of real TXST
slot names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `byroredux/src/asset_provider/material/cdb.rs:240` and `byroredux/src/asset_provider/material/merge.rs:340` use the identical `OnceLock<Mutex<HashSet<String>>>` warn-once idiom, but neither has a test that calls the gate function directly and asserts on its return value, so they don't share this specific ordering hazard today.
- [ ] **TESTS**: A regression test pins this specific fix (the reset hook or test-reserved FourCCs, once added, needs its own coverage confirming isolation from process ordering).
