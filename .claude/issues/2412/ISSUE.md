# TD1-012: Repo-wide >200-LOC function sweep: parse_qust (cc 119/25, highest measured) and merge_external_material standouts

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2412
**Finding ID**: TD1-012 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `crates/plugin/src/esm/records/misc/quest.rs::parse_qust` (646 LOC), `byroredux/src/asset_provider/material.rs::merge_external_material` (678 LOC)
**Status**: NEW (consolidated finding)

## Description
Repo-wide >200-LOC function sweep (104 functions) outside the 9 named oversized files. Most are accepted shapes (ESM/NIF field-proportional parsers, Vulkan `new_inner` constructors, scene/world bootstrap sequences) and are intentionally not re-flagged. Two genuine outliers:

1. `parse_qust` — cognitive complexity **119/25**, the highest score measured anywhere in this audit, higher than `draw_frame`.
2. `merge_external_material` — complexity 35/25, the single NIFAL boundary function every per-game material audit finding routes through. Size/complexity concern only — correctness is `/audit-nifal`'s territory.

## Suggested Fix
File a follow-up for `parse_qust` specifically: split by QUST data group (stages/objectives/aliases/fragments), mirroring the `esm/records/actor/` per-data-group precedent (#2055). QUST is an actively-growing subsystem — the next quest feature is likely to land inside this same function otherwise. No action recommended on `merge_external_material` beyond awareness — it is a deliberate single NIFAL boundary and should not be split in a way that weakens that invariant.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If `merge_external_material` is touched, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: `parse_qust` split preserves all existing QUST parsing regression tests (stages/objectives/aliases/fragments)
