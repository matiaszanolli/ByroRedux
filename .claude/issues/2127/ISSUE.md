# SCR-D1-NEW2-01: metadata_matches_champollion spot-checks only 7 of 51 opcodes

**Issue**: #2127
**Labels**: low, tech-debt, bug
**Dimension**: PEX Reader & Opcode Decode
**Untrusted-Input**: No (test-coverage gap, not a live-data path)
**Location**: `crates/pex/src/opcode.rs:177-193` (test), table at `:73-125`
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 1)

## Description

The test individually asserts `arg_count()`/`has_varargs()` for only 7 of 51 `OPCODES` rows (`Nop`, `IAdd`, `CallMethod`, `CallParent`, `ArrayGetAllMatchingStructs`, `LockGuards`, `TryLockGuards`, plus one `.name()` check). `array_findstruct = 5` and 43 other rows (`struct_get`/`struct_set`, `propget`/`propset`, `jmpt`/`jmpf`, `cmp_lte`/`cmp_gte`, `unlock_guards`, …) have no direct unit-test pin, though the table itself was manually diffed against expected Champollion values and found correct, and the 26640/26641 real-corpus decompile rate is strong indirect corroboration.

## Impact

None today — the table is currently correct. A future edit to an untested row (reordering, a typo in an arg-count digit) would not be caught by `cargo test`; it would only surface as a silent instruction-stream desync on whichever real `.pex` files use that opcode.

## Suggested Fix

Extend the test (or add a sibling) to iterate the full 51-row table against a literal expected array, so any future edit is caught at compile-test time.
