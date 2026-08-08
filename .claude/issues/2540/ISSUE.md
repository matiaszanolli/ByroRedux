# SCR-D5-NEW10-02: Widened SetObjective{Displayed,Completed,Failed} i32 field has no regression test pinning the new range

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2540
**Finding ID**: SCR-D5-NEW10-02

**Severity**: LOW (test-coverage gap; the widen itself is verified correct)
**Dimension**: Recognizer-Chain Soundness
**Untrusted-Input**: No
**Location**: `crates/scripting/src/translate/effects.rs:529,541,552` (`i32::try_from(int_arg(args, 0)?).ok()?`); field type widen in the `Effect` enum at `:76,83,90`
**Status**: NEW (widen itself confirmed correct, coverage gap is new)

## Description
The `u16`→`i32` widen for the objective-index field is a **genuine bug fix, not a loosened range check** — confirmed against `crates/plugin/src/esm/records/misc/quest.rs:77-81`'s `QuestObjective::index` doc comment ("signed 32-bit on FO3/FNV, u16 on Skyrim+/FO4", `i32` as the documented common representation). No test in `effects.rs` exercises a value outside the old `u16` range (0..=65535) — neither a negative index (legal per FO3/FNV) nor an `i32`-overflowing literal (which must still decline via `.ok()?`).

## Evidence
Confirmed directly at `effects.rs:527,540,551` — all three `prim_set_objective_{displayed,completed,failed}` functions use `i32::try_from(int_arg(args, 0)?).ok()?` identically.

## Impact
None today (the guard reads correctly by inspection, matching the pattern #2286's fix also used). Fold into the existing #2289 tracking (test-coverage gaps on this file's newer primitives) rather than a new issue.

## Related
#2289 (existing tracking issue for test-coverage gaps on this file's newer primitives).

## Suggested Fix
Add one test per `SetObjective*` primitive asserting a negative index lowers correctly and one asserting an `i32`-overflowing literal declines. Fold into #2289.

## Completeness Checks
- [ ] **TESTS**: One test per `SetObjective*` primitive for negative-index and overflow-decline cases
