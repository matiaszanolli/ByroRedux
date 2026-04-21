# Issue #491

FO4-BGSM-2: corpus integration test — parse 6,899 vanilla BGSM/BGEM files at 95%+ threshold

---

## Parent
Split from #411. **Depends on #BGSM-1 (parser crate).**

## Scope

Integration test mirroring `crates/nif/tests/parse_real_nifs.rs` pattern — walks `Fallout4 - Materials.ba2` (BA2 v8, 6,616 BGSM + 283 BGEM files vanilla, plus DLC files) and asserts ≥95% parse rate.

### Deliverables

- `crates/bgsm/tests/parse_all.rs` with `#[ignore]` gate
- `BYROREDUX_FO4_DATA` env var fallback to `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`
- `MIN_SUCCESS_RATE_VANILLA: f64 = 0.95` initially; tighten to 1.0 once parser coverage is verified (per #487 pattern)
- Failure bucket summary: group errors by first line, print top-5 failing filenames per bucket
- Separate subtests for BGSM vs BGEM — different code paths, independent budgets

## Completeness Checks

- [ ] **TESTS**: single `cargo test -p byroredux-bgsm --test parse_all -- --ignored` runs cleanly against real FO4 Data
- [ ] **SIBLING**: pattern matches `parse_real_nifs.rs` (same env-var / skip-if-missing / bucket-summary shape)
- [ ] **DOCS**: test file documents the 95% commitment and the path to 100%

## Reference

- Audit: `docs/audits/AUDIT_FO4_2026-04-17.md` Dim 6
- Pattern reference: `crates/nif/tests/parse_real_nifs.rs` (post-#487 tightening)
