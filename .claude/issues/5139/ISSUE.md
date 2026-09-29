# #5139: SPT-2026-09-29-D1-02: the #4122 boundary gate `walker_stops_on_true_tlv_boundary` silently drops files that fail to parse, and prints cumulative totals under per-game labels

**Labels**: low, bug, speedtree, test-gap

**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-09-29.md` (report ID `SPT-D1-02`)
**Severity**: LOW
**Dimension**: Walker Byte-Accounting

## Location
`crates/spt/tests/parse_real_spt.rs` (`walker_stops_on_true_tlv_boundary`)

## Description
The gate runs `let Ok(bytes) = archive.extract(path) else { continue; }; let Ok(scene) = parse_spt(&bytes) else { continue; };` before `totals.total_files += 1`, so a fatal `parse_spt` error removes the file from the denominator instead of failing the gate. The SKILL rates a new fatal path as HIGH (a dictionary edit that turns a mis-size into an array-cap, string-cap or underflow error).
- The `parse_rate_*` tests do count errors, but against a 95 % bar that tolerates about 5 fatal Oblivion files.
- The 26 Shivering Isles files are only in the boundary gate.
- The per-game `eprintln!` prints the running `totals` (`[FO3] 20 files`, `[OBL] 133 files`), although FO3 has 10 files and Oblivion 113.

## Evidence
This run's output: `[FNV] 10 … [FO3] 20 … [OBL] 133 … [SI] 159`.

## Impact
Test gap only; production is correct today. The next dictionary expansion past `TAG_MAX`, which #4122 unblocked, is exactly the change that could introduce a fatal path this gate would hide.

## Related
#4122, #3752 (the five fatal conditions).

## Suggested Fix
Count extract and parse failures and assert they are zero (or at least report them). Print per-archive deltas instead of running totals.

Validated at HEAD 9fcfdc3fc: the two `else { continue; }` arms precede `totals.total_files += 1`, and the `eprintln!` prints the shared `totals`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
