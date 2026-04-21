# Investigation — Issue #487

## Domain
NIF — `crates/nif/tests/parse_real_nifs.rs` + `crates/nif/examples/nif_stats.rs`

## Current state

- `parse_real_nifs.rs:21` — `const MIN_SUCCESS_RATE: f64 = 0.95;`
- `nif_stats.rs:22` — same constant, 0.95
- ROADMAP.md claim: `"Full-archive parse rates: ALL 7 games at 100% (177,286 NIFs). Oblivion was 99.13%."`

The 95% threshold allows up to 5% (~744 NIFs on FNV alone) silent regression vs the stated 100% commitment.

## Fix

Raise both thresholds to 1.0. Today's state per ROADMAP is 100% across all 7 games — per the parse_real_nifs integration test there is no vanilla asset we tolerate failing. If a future regression drops a specific game below 100%, the per-game test fails with a clear signal pointing at that game.

Alternative (per-archive expected-failure lists) is out of scope — the simpler threshold raise achieves the same safety.

Stale comment at `parse_real_nifs.rs:74-77` saying Oblivion BSA v103 decompression isn't implemented — verify, update if stale.

## Scope
2 files. In scope.
