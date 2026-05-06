# SK-D5-NEW-05: Per-block stream realignments invisible to nif_stats parse-rate gate — silent data loss masked by clean==total

## Description

`nif_stats` reports `recovered: 0` (i.e. zero NIFs with partial unknown) even when 56+ per-block stream-realignment events fire across 18862 NIFs. The gate only inspects `clean` vs `total` at the **NIF** level, not the per-**block** level.

So `BSLODTriShape × 14` + `BSLagBoneController × 78` + `BSProceduralLightningController × 3` realignments collectively lose data on **95 blocks per pass**, but the gate stays green at `100.00%` for Meshes0. This is the same masking pattern the 2026-04-22 audit flagged as `SK-D5-06` — the issue re-applies once realignment count > 0.

## Location

`crates/nif/examples/nif_stats.rs:52` (`min_success_rate`) + the per-block consumption check in `crates/nif/src/lib.rs`

## Evidence

Meshes0 run prints `100.00% clean / truncated: 0 / recovered: 0` while the run log carries 67+ WARN lines for `consumed != block_size` realignments.

## Impact

Audit gate gives false confidence. Future regressions that turn dispatch-clean into per-block-truncated will not trip the gate. Verifying via `clean == total` is necessary but not sufficient.

## Suggested Fix

Pipe the per-block realignment counter into `Stats::recovered` (or a sibling `Stats::realigned` field) and bump the gate to fail when `realigned > 0` on a known-clean BSA. Roughly 10-line change in `nif_stats.rs`.

Caveat: must land alongside or after #836 (BSTriShape data_size warning) and #837 (BSLagBoneController by-design noise) — otherwise the gate goes immediately red on legitimate-but-noisy paths and masks actual regressions.

## Related

- SK-D5-NEW-02 / #836 (BSTriShape false-positive noise)
- SK-D5-NEW-03 / #837 (BSLagBoneController by-design noise)
- SK-D5-NEW-07 / #838 (BSLODTriShape — the real drift this gate would expose)
- 2026-04-22 SK-D5-06 (same pattern)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a synthetic NIF with a deliberately-misaligned block; assert `nif_stats` exits non-zero

## Source Audit

`docs/audits/AUDIT_SKYRIM_2026-05-05_DIM5.md` — SK-D5-NEW-05