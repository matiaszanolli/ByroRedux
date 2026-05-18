# FO4-D4-004: parse_rate_fo4_esm is #[ignore]-gated; five-map regression net misses default CI

**Labels**: bug, import-pipeline, medium

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: ESM Architecture Records
**Severity**: MEDIUM (coverage gap)

## Observation

`crates/plugin/tests/parse_real_esm.rs:962-963`:

```rust
#[test]
#[ignore]
fn parse_rate_fo4_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    ) else { ... return; };
```

Test only runs when `BYROREDUX_FO4_DATA` is set or the fallback Steam path exists. CI without the game data does not run the per-category floor asserts.

The #817 five-map regression net (`texture_sets` / `scols` / `packins` / `movables` / `material_swaps` floors in `EsmIndex::categories()`) only fires on a manual ignored-test run.

## Why bug

All FO4 parse correctness coverage outside this test comes from synthetic unit tests in `crates/plugin/src/esm/cell/tests/`. A future refactor that silently empties one of the five maps will not surface in default CI — only on opt-in. The parsers work today, but the regression net is conditional.

## Fix

Add a synthetic-bytes Fallout4-shape fixture (a handful of SCOL/PKIN/TXST/MSWP records assembled in-memory) that runs unconditionally and asserts:

```rust
assert!(index.cells.texture_sets.len() >= 1);
assert!(index.cells.scols.len() >= 1);
assert!(index.cells.packins.len() >= 1);
assert!(index.cells.material_swaps.len() >= 1);
// MOVS optional — vanilla ships zero (FO4-D4-002)
```

Keep the real-data run (`parse_rate_fo4_esm`) as the secondary gate for the live-count floor asserts.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm the same five-map fixture is also asserted by `parse_rate_fnv_esm` / `parse_rate_skyrim_esm` (or that the FO4-specific subset doesn't apply to other games)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: this issue IS the test addition; the synthetic fixture must run in default `cargo test -p byroredux-plugin`

## Related

- #817 — `EsmIndex::categories()` five-map exposure
- #819 — `parse_rate_fo4_esm` real-data harness
- FO4-D4-002 — MOVS-specific coverage gap
