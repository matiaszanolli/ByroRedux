# Issue #488

FNV-5-F2: No CI gate for the 13,684 FalloutNV.esm record-count baseline

---

## Severity: Low (CI gate gap)

**Location**: `crates/plugin/` (no `examples/`, no ignored integration test)

## Problem

CLAUDE.md + ROADMAP.md both quote `"13,684 structured records"` for `FalloutNV.esm` (M24 Phase 1 baseline), but no ignored integration test or example binary parses a real ESM and asserts the total. The value is documented but not verified on each change.

## Impact

Any record-parser regression that silently drops records (catch-all fallthrough, wrong type dispatch, sub-record boundary) would not fail CI.

## Fix

Add `crates/plugin/tests/parse_real_esm.rs` mirroring `crates/nif/tests/parse_real_nifs.rs`:

```rust
#[test]
#[ignore]
fn parse_real_fnv_esm_record_counts() {
    let Some(data_dir) = std::env::var("BYROREDUX_FNV_DATA").ok() else {
        return;
    };
    let path = Path::new(&data_dir).join("FalloutNV.esm");
    let index = parse_esm(&path).expect("parse FNV.esm");
    assert_eq!(index.items.len(), 2643);
    assert_eq!(index.containers.len(), 2478);
    assert_eq!(index.leveled_items.len(), 2738);
    // ... all 10 categories from M24 Phase 1
    assert!(index.total() >= 13_684);
}
```

~30 lines. Closes the validation gap without touching runtime behaviour.

## Completeness Checks

- [ ] **TESTS**: `BYROREDUX_FNV_DATA=... cargo test -p byroredux-plugin --test parse_real_esm -- --ignored` passes
- [ ] **SIBLING**: Add FO3 variant once FO3-3-01 (#439) lands — 18,007 records verified in FO3 audit
- [ ] **DOCS**: Reference ROADMAP M24 Phase 1 baseline numbers in the test comment

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-5-F2)
