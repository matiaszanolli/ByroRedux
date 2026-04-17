# OBL-D5-H1: Truncated NIF scenes counted as parse success — hides ~9% real failure rate

**Issue**: #393 — https://github.com/matiaszanolli/ByroRedux/issues/393
**Labels**: bug, nif-parser, high

---

## Finding

`parse_nif` returns `Ok(NifScene { truncated: true, dropped_block_count: N })` when a mid-parse error forces early termination. Both `nif_stats` (example) and `parse_real_nifs.rs` (integration test) treat this as success for the `MIN_SUCCESS_RATE = 0.95` gate at `crates/nif/tests/parse_real_nifs.rs:21`.

## Evidence

On a full sweep of `Oblivion - Meshes.bsa` (before hitting the OOM at OBL-D5-C1):
- **678 of ~7,500 files** (≈ 9.04%) parse truncated.
- **67,987 blocks silently dropped.** Median 37 per failing file; max 3,945.
- **138 files truncate at the root NiNode (block 0)** → render as empty scenes.
- `nif_stats` summary reports 100% success; the real clean-parse rate is ≈ 90.96%.

Truncation root causes observed:
| Cause | Count | Notes |
|---|---|---|
| `NiTransformData: unknown KeyType: <garbage>` | 188 | Upstream drift (see OBL-D5-H3) |
| `NiNode: failed to fill whole buffer` | 138 | Index-0 total loss |
| `NiStringPalette: requested 4294967295-byte alloc` | 121 | Upstream drift |
| Unknown block types (no `block_sizes` skip) | 61 | OBL-D5-H2 |
| Geometry parser off-by-N | ~25 | — |
| Particle controller wire layout drift | ~10 | — |
| Havok layout gaps | ~4 | — |

## Impact

- CLAUDE.md + ROADMAP claim "Oblivion → 100% / 7963+ NIFs" is regressed.
- Cells referencing any of the 138 root-truncated meshes render invisible geometry.
- CI gate at 95% passes only because truncation = success.

## Fix

Two changes:
1. Change `nif_stats` and `crates/nif/tests/common/mod.rs:293` (the integration test helper) to treat `scene.truncated` as a **failure** for the rate metric, or at minimum report it as a secondary counter ("clean: X%, truncated: Y%, failed: Z%").
2. `ImportedScene` should carry `truncated: bool`; `import_nif_scene` at `crates/nif/src/import/mod.rs:266` should `log::warn!` when consuming a truncated scene so the cell loader has a signal.

Optional: decide whether to keep `MIN_SUCCESS_RATE = 0.95` as a true-clean-parse metric (will currently fail) or add a second metric `MIN_CLEAN_PLUS_RECOVERABLE = 0.98`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `parse_rate_skyrim`, `parse_rate_fnv`, etc. — same pattern?
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add assertion that `parse_nif` on a synthetic truncated input returns `Err` OR returns `Ok(scene)` where `scene.truncated == true` AND the test harness counts that as failure.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 5 H-1.
