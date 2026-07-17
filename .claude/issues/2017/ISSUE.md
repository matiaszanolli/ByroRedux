# SAVE-D4-NEW-01: Quicksave ring cursor advances even when the pre-save validation gate aborts the write

**Labels**: medium, tech-debt, bug

**Severity**: MEDIUM
**Dimension**: Validation Gates
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`byroredux/src/save_io.rs:396-421` (`SaveCommand::execute`)

## Description
For a blank-slot quicksave, `state.ring.advance()` runs before `validate_world`/`validate_form_ids`. If validation fails, the function returns the abort message without ever writing — but the in-memory ring cursor has already permanently advanced. Nothing is corrupted by the failed attempt itself, but the round-robin invariant ("next quicksave lands one slot after the last *successful* one") is broken: each aborted quicksave burns a rotation with nothing written to back it.

Verified current: in `SaveCommand::execute`, the `"" => state.ring.advance()` branch executes before the `validate_world`/`validate_form_ids` calls and the `issues.is_empty()` abort check.

## Impact
Explicit-slot saves (`save 3`) are unaffected. Within one session, repeated quicksaves while the world is transiently validation-failing (e.g. mid-scripted-sequence) each burn a ring slot; once a real save succeeds it lands further around the ring than the "one after the last real save" model assumes — in the worst case (failed attempts ≥ ring size) the eventual write overwrites an older genuinely-good save early, with no warning. Self-limiting: `SaveRing::resume` (`#1706`) recomputes the cursor from on-disk mtimes at every process start, so the desync cannot persist across a restart.

## Related
Adjacent to but distinct from `#1706` (cursor persistence across restarts vs. this — cursor mutation racing ahead of the write it gates). No existing issue covers this specific ordering bug.

## Suggested Fix
Move `state.ring.advance()` after the validation gate — use a non-mutating peek for the abort-message path, and only call the mutating `advance()` once `issues.is_empty()` and the write is about to proceed.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
