# NIFAL-D7-2026-09-21b-01: #4551's route pin counts its own string literals, so it cannot detect a dropped route

**Issue**: #4635
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: LOW
**Dimension**: Animation
**Tier Violated**: harness-gap (the #4411 source-scan class)
**Game Affected**: Skyrim
**Location**: `byroredux/src/cell_loader/load.rs:1314-1329` (`every_animation_route_installs_the_combat_family_too`).

## Description
The test source-scans `load.rs` via `include_str!` and counts occurrences of the two call-site strings, but each needle also matches the test's own `.matches("...")` literal argument, so it self-counts once. Confirmed at HEAD: each needle appears 3× (two production sites + the test literal). Deleting an entire route (both call sites) drops both counts from 3 to 2, and the assertions (`walk == combat`, `walk >= 2`) still pass — the exact regression this test exists to catch is invisible to it.

## Suggested Fix
Strip the `#[cfg(test)]` module before counting (as `material_translate.rs`'s meta-guard does via `split_once("#[cfg(test)]")`), then floor at the real production count (2). Alternatively, fold the three installers into one shared helper.

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D7-2026-09-21b-01)
