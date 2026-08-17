# SCR-D8-04: crates/hkx's only integration test passes vacuously without game data

**Issue**: #3014
**Severity**: MEDIUM
**Dimension**: 8 — Havok packfile reader
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 8 — Havok packfile reader).

**Location**: `crates/hkx/src/animation.rs`:887-899

## Description

`crates/hkx`'s only integration test **passes vacuously without game data**, leaving the crate with three real tests and **no negative-input coverage** at all.

## Evidence

```rust
#[test]
fn skyrim_cart_player_idle_decodes_when_assets_are_available() {
    let data_dir = std::env::var_os("BYROREDUX_SKYRIM_DATA") … ;
    let archive_path = data_dir.join("Skyrim - Animations.bsa");
    if !archive_path.is_file() {
        return;          // <- silent pass
    }
```

Re-verified 2026-08-17. This is a plain `return`, not `#[ignore]`, so the test reports **green** on any machine without the archive — including CI.

## Impact

The crate is a parser for **untrusted binary input** on `_audit-common.md`'s un-owned-subsystem list. Its only integration coverage is conditional on data most runs do not have, and there are no malformed-input tests at all — which is why #3011 (unbounded `num_frames`) and #3013 (unvalidated bone bindings) both survived.

This is the green-by-construction shape the tech-debt sweep's Dimension 9 exists to find.

## Suggested Fix

Mark the data-dependent test `#[ignore]` so a skipped run is distinguishable from a passing one (matching the house pattern used by `crates/pex/tests/r5_fidelity.rs`), and add checked-in malformed-input fixtures covering the bounds this crate does not currently enforce.

## Related

- #3011, #3013, #3018 — the parser gaps this missing coverage allowed
- #3003 (RT-2026-08-16-04 — the same skip-reads-as-pass shape in the smoke gates)

## Completeness Checks
- [ ] **SKIP≠PASS**: The data-dependent test is `#[ignore]`d, not silently returning
- [ ] **NEGATIVE-COVERAGE**: Checked-in malformed fixtures exist, not only happy-path real data
- [ ] **SIBLING**: Other crates checked for the same silent-`return` test shape
- [ ] **TESTS**: The new fixtures fail before the #3011/#3013 fixes and pass after

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3014 --json state` when live state is needed.*
