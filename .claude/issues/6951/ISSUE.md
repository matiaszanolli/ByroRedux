# #6951: TD1-2026-11-15-05: `crates/sfmaterial` ships about 520 lines of test fixtures in the production library (`pub mod test_support`, incl. a 291-line fixture function)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6951
- **Labels**: low,import-pipeline,game:starfield,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (cross-dim 9: test hygiene)
- **Age**: introduced by #3398; `synthetic_cdb_chunks_with` has been extended for #5277, #5283 and #5673.
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Location**: `crates/sfmaterial/src/index.rs:935-1458` (`pub mod test_support`, `#[doc(hidden)]`), `synthetic_cdb_chunks_with` at `:1066`
- **Status**: NEW (a crate no tech-debt report has named)
- **Description**: The module is intentionally public because `byroredux/src/asset_provider/tests/starfield_mat.rs:42,51` uses it cross-crate. It is not gated by a feature, so it compiles into every build of the crate. It is 35% of `index.rs`'s 1458 production LOC, and it contains the 291-line fixture function.
- **Evidence**: `crates/sfmaterial/Cargo.toml` has no `[features]`; the module has only `#[doc(hidden)]`. `prod_loc` reports 1458 for a file of 1934 total lines, and the test module starts at `:1459`.
- **Impact**: It inflates the production LOC figure for the crate and makes `index.rs` the second-largest file in a crate that has no other debt. A fixture-only change rebuilds the shipping library, and the byte-assembly helpers can be mistaken for production decoders.
- **Suggested Fix**: Add `[features] test-support = []`. Gate the module with `#[cfg(any(test, feature = "test-support"))]`. Enable the feature from `byroredux`'s `[dev-dependencies]` entry for `byroredux-sfmaterial`. Optionally move the module into `src/test_support.rs`. The `lib.rs:77` re-export gets the same gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
