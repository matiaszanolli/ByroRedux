# SCR-D2-NEW-01: rebuild_expression's restart-to-zero copy-propagation scan is O(n^2) — DoS on a crafted .pex

**Labels**: medium, performance, bug

**Severity**: MEDIUM (algorithmic-complexity DoS, not a crash/OOB/OOM)
**Dimension**: Decompiler CFG & Lift
**Untrusted-Input**: Yes — reachable from raw `.pex` bytes via `translate_pex` on the live cell-loader VMAD-attach path.
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/pex/src/decompile/lift.rs:316-346` (`rebuild_expression`)

## Description
After every successful single-consumer fold, the scan index resets to `0` and re-scans the (now one-shorter) list from the start — a faithful port of Champollion's C++ iterator-invalidation workaround, but `O(n²)` in the number of statements whenever fold targets aren't clustered at the front. No file in `crates/pex` or `crates/scripting` bounds instruction count below the wire format's own `u16` field (max 65535) — `grep` confirms zero hits for any `MAX_INSTRUCTIONS`/size-cap/timeout.

Verified current: `rebuild_expression` still does `i = 0; // restart, like the C++ it = scope->begin()` after every successful fold; no `MAX_INSTRUCTIONS`/`MAX_BLOCK_INSTRUCTIONS` constant exists anywhere in `crates/pex` or `crates/scripting`.

## Evidence
Empirically benchmarked (standalone harness, not guessed): 500 fold pairs → 1.045ms; 21845 pairs (65535 instructions, the hard per-function ceiling) → 1.255s. Timing roughly quadruples on every doubling — textbook O(n²).

## Impact
`translate_pex` wraps decompilation in `catch_unwind` (#1816), which guards panics but not a slow-but-successful computation. On the live cell-loader path this runs synchronously; a single hostile REFR script (or several, since one `.pex` commonly carries multiple functions each independently able to approach the ceiling) can stall cell load for multiple seconds to tens of seconds without ever erroring. The same scan re-runs on larger merged scopes from `control_flow.rs`/`boolean.rs`, compounding the total cost.

## Suggested Fix
Resume the scan at `i.saturating_sub(1)` instead of `0` after a fold (the producer's fold target is always adjacent — `count_constant_id` only ever looks at `scope[i+1]` — so this preserves fold order and drops the pass to O(n)); or add a `MAX_BLOCK_INSTRUCTIONS` cap mirroring the existing `MAX_REBUILD_DEPTH` precedent. Prefer the O(n) fix — it also speeds up legitimate large scripts. Add a regression test asserting bounded time/iteration count on an adversarial fixture.

## Completeness Checks
- [ ] **TESTS**: A regression test asserting bounded time/iteration count on an adversarial fixture (wall-clock or call-count based — dhat allocation bounds cannot catch this class)
