# REG-2026-08-03-02: cinematic.rs keyed-lerp refactor (#2260) introduces a redundant_closure clippy error

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2350
**Severity**: LOW
**Labels**: low, tech-debt, bug
**Source audit**: docs/audits/AUDIT_REGRESSION_2026-08-03.md (finding REG-2026-08-03-02)
**Location**: `crates/scripting/src/cinematic.rs:456`

## Description

Commit `bb98428a` ("Fix #2260: extract shared keyed-lerp helper for sample_scalar/sample_color") introduced a shared `sample_keyed<T, V>` generic helper taking `time_of`/`value_of` field-accessor closures. The terminal "last key" fallback line passes a closure that just forwards to `value_of`:

```rust
keys.last().map_or(default, |key| value_of(key))
```

Clippy's `redundant_closure` lint (part of the standard `-D warnings` set) flags this — the closure can be replaced with `value_of` itself.

## Evidence (re-verified directly against current code, 2026-08-03)

```
$ cargo clippy -p byroredux-scripting --lib -- -D warnings 2>&1 | tail -8
error: redundant closure
   --> crates/scripting/src/cinematic.rs:456:33
    |
456 |     keys.last().map_or(default, |key| value_of(key))
    |                                 ^^^^^^^^^^^^^^^^^^^ help: replace the closure with the function itself: `value_of`
error: could not compile `byroredux-scripting` (lib) due to 1 previous error
```

Line number matches the report exactly (`cinematic.rs:456`).

## Impact

Cosmetic — no behavior change, but a second, independent way `cargo clippy --workspace -- -D warnings` is red today, alongside #2349 (same commit-of-the-day batch #2258/#2259/#2260/#2261).

## Related

Sibling break of the same "clippy --workspace -D warnings must stay green" contract as #2349 (different lint/site, same day). Filed separately since domains differ (renderer/safety vs. scripting/tech-debt); no dedicated scripting label exists in this repo, so `tech-debt` + `bug` used per label reconciliation.

## Suggested Fix

`keys.last().map_or(default, value_of)` — one-line fix.

## Completeness Checks
- [ ] **UNSAFE**: N/A — no unsafe code in this finding
- [ ] **SIBLING**: Check if the same commit's `sample_keyed` refactor introduced other redundant closures at call sites
- [ ] **TESTS**: Add/restore CI enforcement so this can't silently regress a 3rd time

## Validation performed before filing

- Path-validation gate (`.claude/commands/_audit-validate.sh`): PASS
- Re-ran `cargo clippy -p byroredux-scripting --lib -- -D warnings` directly: reproduced the exact reported error at `cinematic.rs:456` — CONFIRMED
- Dedup: searched open + all-state issues (400-issue window) for "cinematic"/"closure"/"clippy" keywords — no open duplicate found (only #2269, an unrelated lock-ordering issue in the same file)
