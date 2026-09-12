# Batch: #4158, #4159, #4160, #4161

## #4158 — NIF-D1-2026-09-11-03: skip_animation's block-size skip escapes as a whole-parse Err
**Severity**: LOW · **Location**: `crates/nif/src/lib.rs:459-469`

`skip_animation`'s block-size skip uses `?` to propagate a skip failure straight out of the
whole parse instead of engaging the loop's normal truncation-recovery path (`NiUnknown`
substitution + `recovered_blocks` counter).

Fix: Route the skip failure through the same truncation-recovery path used elsewhere in the
dispatch loop.

## #4159 — NIF-D1-2026-09-11-04: block_size-driven realignment can park the cursor past EOF silently
**Severity**: LOW · **Location**: `crates/nif/src/lib.rs:536,629`

`stream.set_position(start_pos + size as u64)` (two call sites) can park the cursor past EOF
silently, unlike `stream.skip()` which bounds-checks.

Fix: Bounds-check the target position against the stream length before `set_position`, return
`Err` on overrun.

## #4160 — NIF-D2-2026-09-11-04: bsver::OBLIVION has no production consumer
**Severity**: LOW · **Location**: `crates/nif/src/version.rs:401-404`

`bsver::OBLIVION`'s only consumer is `NifHeader::test_oblivion()`, `#[cfg(test)]`-gated. Same
dead-helper class removed twice already (#1511/#1840/#1897).

Fix: Either wire a real production consumer, or document it as test-fixture-only so a future
dead-code sweep doesn't re-litigate it.

## #4161 — NIF-D2-2026-09-11-05: NifVariant::detect's FO76 lower bound contradicts its own constant's doc comment
**Severity**: LOW · **Location**: `crates/nif/src/version.rs:676-698,487-490`

`FO76` constant's doc says the range starts at 152, but `detect`'s match arms branch on the
literal FO76 constant value (155) and a bare `170` (not `STARFIELD`=172).

Fix: Either correct the doc comment to match the code's actual lower bound, or introduce a named
constant for the 170 boundary literal.
