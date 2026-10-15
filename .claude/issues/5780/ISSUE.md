# OBL-D1-2026-10-14-01: an unopenable Oblivion DLC mesh archive is still "absent" for two corpus gates

**Labels**: low,game:oblivion,legacy-compat,nif-parser,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D1-2026-10-14-01`. Re-verified against current code before filing.

- **Severity**: LOW (test gap)
- **Dimension**: 1 (NIF corpus integrity)
- **Location**: `crates/nif/tests/common/mod.rs` `open_optional_mesh_archives` (called from `block_coverage_baselines.rs::oblivion_block_count_parity` and `parse_real_nifs.rs::parse_rate_oblivion`)
- **Status**: NEW (adjacent to closed #5439 / #5232, which deliberately left this opener alone)
- **Description**: #5439 routed `open_mesh_archive`, `open_all_mesh_archives`, `open_ba2_by_name` and the drift corpus through `open_present_archive` (panic). `open_optional_mesh_archives` keeps its own `match` and only `eprintln!`s "present but failed to open", then drops the archive. For Skyrim SE the tier is account-varying (Creation Club), so tolerance is defensible; for Oblivion it is the eight static vanilla DLC archives, and `oblivion_block_count_parity` already keys `archive_parsed` per archive precisely so a present archive is always compared.
- **Evidence**: experiment with a real base BSA and a garbage `DLCFrostcrag.bsa`: `oblivion_block_count_parity` reported "1 archive(s) checked, all parsed >= baseline" and `parse_rate_oblivion` passed, while the drift corpus on the same directory panicked. The gates disagree on identical input.
- **Impact**: a BSA-reader regression specific to a DLC archive shape (1 580 NIFs, 16.4 %, the population #3712 added the gate for) passes both gates. A regression that also breaks the base archive is still caught, so the blast radius is small.
- **Related**: #5439, #5232, #3712, #3369.
- **Suggested Fix**: in `open_optional_mesh_archives` panic via `open_present_archive` (an absent file is still skipped; only a present-but-unopenable file fails). If the Skyrim SE tier must stay tolerant, make it per-game.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
