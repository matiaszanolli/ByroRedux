# RF3-2026-11-05-02: `App`: move the 17 `groundcover_*` fields into a ground-cover state struct

Labels: low,tech-debt,bug,terrain-exterior

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/main.rs:539` (`App`, 94 fields)

## Why
17 fields hold ground-cover state, with 32 access sites in 3 files: `groundcover_model_records`, `groundcover_model_selection`, `groundcover_model_records_key`, `groundcover_cells`, `groundcover_chunks`, `groundcover_residency`, `groundcover_collect_scratch`, `groundcover_truncation_logged`, `groundcover_detail_atlas`, `groundcover_species`, `groundcover_species_table`, `groundcover_species_table_signature`, `groundcover_disturbers`, `groundcover_debug_points`, `groundcover_off`, `groundcover_bench`, `groundcover_bench_started`. `cover_template_draws` belongs with them.

## Change
Add one ground-cover sub-struct that owns these fields and `cover_template_draws`.

## Constraints
Run `grep -rn include_str` first: 37 `include_str!` sites read `main.rs` or `app_*.rs`, and a moved field can blind a scan. Move one group per commit.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `App` no longer declares them
- `cargo test -p byroredux --bin byroredux` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6694 --json state`)._
