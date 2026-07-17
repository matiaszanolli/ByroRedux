# FNV-D4-03: Dead real-data spot-check — Varmint Rifle test keyed on a FormID that doesn't exist in FalloutNV.esm

- **Severity**: LOW
- **Labels**: low, tech-debt, bug
- **Location**: `crates/plugin/src/esm/records/tests.rs:432-441`

## Description
`if let Some(varmint) = index.items.get(&0x000086A8)` never matches (real FormID is `0x0007EA24`); the `if let` has no `else`, so the assertion inside silently never runs. The parser itself is confirmed correct by manual decode of the real record — this is a dead spot-check, not a parser bug, but it means the intended real-data validation coverage for this item is a no-op today and would silently stay a no-op through future regressions.

## Evidence
`tests.rs:434`: `if let Some(varmint) = index.items.get(&0x000086A8) { ... assert_eq!(...) }` with no `else` branch — if the key never matches, the whole block silently no-ops and the test passes regardless of whether the parser is correct.

## Impact
False sense of real-data test coverage; a future regression in this parsing path would not be caught by this test.

## Suggested Fix
Fix the key to `0x0007EA24`, replace `if let` with a hard assert (`.expect(...)` or `assert!(index.items.contains_key(...))`) so future regressions fail loudly instead of silently skipping.

## Completeness Checks
- [ ] **SIBLING**: Check other real-data spot-check tests in the same file for the same silent-`if-let`-no-else pattern
- [ ] **TESTS**: This finding IS the test-coverage fix — verify the corrected assertion actually executes and passes against real `FalloutNV.esm` data
