**Severity:** LOW · **Dimension:** Coverage (audit-tool accuracy) · **Game Affected:** All

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-07).

## Description
The `d5_coverage` example hardcodes a ~300-entry `DISPATCH_KEYS` snapshot of the dispatch table. Live `parse_block` carries ~308 keys and has gained arms since the snapshot, so the tool flags already-dispatched types (`NiAlphaController`, `BSKeyframeController`, `BSBoneLODExtraData`, `NiIntegersExtraData`, `NiFloatsExtraData`) as "UNCOVERED".

## Location
`crates/nif/examples/d5_coverage.rs:18-319`

## Evidence
Of 7 reported-uncovered types across all games, 4 are dispatched in live source (`crates/nif/src/blocks/mod.rs` lines 555, 568, 675, 775). Only 3 are genuine gaps. Example file confirmed present.

## Impact
Inflated uncovered counts could mask the real (smaller) gap or trigger wasted follow-up; coverage % figures are pessimistic but directionally correct.

## Suggested Fix
Either (a) derive `DISPATCH_KEYS` at runtime by probing `parse_block` per type and checking `block_type_name() != "NiUnknown"`, (b) generate the array from `mod.rs` via build script, or (c) add a unit test that diffs the array against the live match arms so it can't drift silently.

## Related
#601 (nif_stats top-20-only blindspot — same family of tooling-blindness issues).

## Completeness Checks
- [ ] **SIBLING**: Apply the same anti-drift mechanism to any other tool carrying a hardcoded dispatch-key snapshot (`nif_stats`?)
- [ ] **TESTS**: Option (c) — a `cargo test` diff of the snapshot against live match arms is itself the regression guard
