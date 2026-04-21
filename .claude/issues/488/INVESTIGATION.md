# Investigation — Issue #488 (FNV-5-F2)

## Domain
ESM — `crates/plugin/` test infrastructure.

## Current state

- **Existing inline test**: `crates/plugin/src/esm/records/mod.rs:496` `parse_real_fnv_esm_record_counts` is `#[ignore]`-gated and asserts per-category floors (items > 2500, NPCs > 3000, etc.).
- **Hardcoded path**: `/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm` — no env-var fallback.
- **No total assertion**: no `total >= 13_684` check on `EsmIndex::total()`. That helper already exists at `mod.rs:122-149` and rolls up every category including cells + statics.
- **No FO3 coverage** despite the dim_3 audit confirming FO3 parses to 18,007 structured records.

## Fix

Add `crates/plugin/tests/parse_real_esm.rs` as a proper integration test mirroring `crates/nif/tests/parse_real_nifs.rs`:

1. Env-var path resolution (`BYROREDUX_FNV_DATA` + fallback to Steam install), skip cleanly if missing.
2. `parse_rate_fnv_esm` — parse FalloutNV.esm, assert `total >= 13_684` (M24 Phase 1 baseline) + the per-category floors already established inline.
3. `parse_rate_fo3_esm` — same for Fallout3.esm, assert `total >= 18_000` (per the FO3 audit's 18,007 figure, slight margin).
4. Leave the inline test at `mod.rs:496` alone — it still works as a unit-level regression check and doesn't collide with the integration tests.

## Scope
1 new file: `crates/plugin/tests/parse_real_esm.rs`. No production code changes.
