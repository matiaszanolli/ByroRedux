# FNV-D2-06: EsmIndex.total() and end-of-parse log line drift independently

## Finding: FNV-D2-06

- **Severity**: LOW
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Location**: [crates/plugin/src/esm/records/mod.rs:153-189](crates/plugin/src/esm/records/mod.rs#L153-L189) (`total()`) vs [records/mod.rs:441-472](crates/plugin/src/esm/records/mod.rs#L441-L472) (log line)

## Description

`EsmIndex.total()` and the end-of-parse log line are independent. Adding a new category (e.g. when fixing #519/AVIF or FNV-D2-01 ENCH) requires updating both sites; one is already missed: `total()` doesn't double-count `cells.statics` against the `activators`/`terminals` overlap (statics get the MODL pass first, then ACTI/TERM populate dedicated maps too).

The log message is correct; `total()` is not authoritative.

## Suggested Fix

Drive `total()` from a `categories()` iterator over a `(&'static str, fn(&Self) -> usize)` table:

```rust
impl EsmIndex {
    fn categories() -> &'static [(&'static str, fn(&Self) -> usize)] {
        &[
            ("items", |s| s.items.len()),
            ("containers", |s| s.containers.len()),
            ("LVLI", |s| s.leveled_items.len()),
            // ... one row per category
        ]
    }

    pub fn total(&self) -> usize {
        Self::categories().iter().map(|(_, f)| f(self)).sum()
    }
}
```

Then fold the end-of-parse log message through the same table — the two stay in sync, adding a category is a single edit.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check FNV.esm log output before/after — totals should match.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Test that `index.total() == sum-of-category-lens` for a parsed FNV.esm.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
