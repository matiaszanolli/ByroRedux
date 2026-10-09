# #5454: REG-2026-10-08-01: The #3865 consolidation missed a fourth-name `SubRecord` builder, and its guard cannot see it

**Labels**: low,esm-plugin,bug,test-gap,tech-debt
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5454

**Source**: `docs/audits/AUDIT_REGRESSION_2026-10-08.md` — `REG-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Incomplete-fix remainder of CLOSED #3865 (not a regression, not a duplicate). Re-verified at HEAD: `sub_of` at `crates/plugin/src/esm/records/misc/dialogue.rs:1856` (4 uses), private `fn zstring` at `test_support.rs:41`, guard needle list `["sub", "mk_sub", "make_sub", "edid", "modl"]`.

- **Severity**: LOW
- **Dimension**: Regression guards / test hygiene
- **Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:1856-1861` (`sub_of`); `crates/plugin/src/esm/records/test_support.rs:41` (private `zstring`), `:84-130` (the guard); `crates/plugin/src/esm/records/gras_tests.rs:11-15`; `crates/plugin/src/esm/records/soun.rs:126-130`
- **Status**: NEW. This is an incomplete fix of #3865 (closed 2026-09-12 by `cd5460e40`), not a regression: `sub_of` came in with `47d9e394d` (2026-08-30), before the fix.
- **Description**: #3865 folded the per-module `SubRecord` builders into `test_support::sub`. Its guard flags only definitions named `sub`, `mk_sub`, `make_sub`, `edid` or `modl`. `dialogue.rs`'s test module still defines `fn sub_of(code: &[u8; 4], data: &[u8]) -> SubRecord`, a verbatim copy of `sub` used at 4 sites, which is exactly what the issue set out to remove. Separately, `test_support::zstring` is private (`fn zstring`, not `pub(crate)`), so `gras_tests.rs` (`zstring`) and `soun.rs` (`zstring_sub`) each re-wrap the same null-terminated builder over `sub`.
- **Evidence**: `git grep -n "fn [a-z_0-9]*(.*-> SubRecord" -- crates/plugin` lists `sub_of` and the two `zstring` copies. The guard's needle list is `["sub", "mk_sub", "make_sub", "edid", "modl"]` (`test_support.rs:86`), so it passes.
- **Impact**: Cosmetic and maintenance only, which is the cost #3865 itself named. The guard reads as comprehensive but it is a name allowlist: any newly named generic copy passes it.
- **Related**: #3865.
- **Suggested Fix**: Replace `sub_of` with `test_support::sub`. Make `zstring` `pub(crate)` and drop the two local wrappers. Then make the guard match the body shape (`SubRecord { sub_type: *` inside a fn returning `SubRecord`) instead of a fixed name list.

## Completeness Checks
- [ ] **SIBLING**: Other generic `-> SubRecord` builders in `crates/plugin` (e.g. `equipment.rs` `mk`) checked against `test_support::sub`
- [ ] **TESTS**: A regression test pins this specific fix (the guard matches the body shape, not a name allowlist)
