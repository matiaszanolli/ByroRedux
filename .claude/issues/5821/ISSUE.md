# TD2-2026-10-15-20: `MaterialProvider` records a failed path four times with the same half-evict + insert + push + warn block

Labels: low,import-pipeline,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-20`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/asset_provider/material/provider.rs:451-470`, `:486-499`, `:632-645`, `:672-686`
- **Status**: NEW
- **Age**: #951 / #1430 (half-eviction), #4672/#4664 (diagnostics)
- **Effort**: trivial
- **Description**: `half_evict(&mut self.failed_paths_order, self.failed_paths.len(), MAX_FAILED_PATHS, |old| { self.failed_paths.remove(old); }); if self.failed_paths.insert(key.clone()) { self.failed_paths_order.push_back(key); log::warn!(..) }`.
- **Evidence**: clone scan 13 windows `:451-486`; `grep -n 'failed_paths.insert' provider.rs` = 4.
- **Impact**: the eviction cap policy is edited four times; each previous half-eviction fix (#951, #1430) touched all of them.
- **Related**: #951, #1430.
- **Suggested Fix**: `fn record_failed(&mut self, key: String, what: impl FnOnce() -> String)` in `provider.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
