# TD3-003/004: feature-matrix.md marks shipped M35 terrain LOD + M41.x ragdoll as not-started

_Filed 2026-06-26 as #1756 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1756` for live state)._

**Severity**: LOW · **Dimension**: 3 — Stale Documentation
**Location**: `docs/feature-matrix.md:50`, `:178` (terrain LOD) and `:126` (ragdoll)
**Status**: NEW · **Audit**: TD3-003 + TD3-004 (consolidated — both are feature-matrix.md rows that lag shipped milestones)

## Description
Two matrix rows read `✗` for work that has shipped (the matrix is a status floor, not a record of what exists):

1. **M35 Terrain LOD** (`:50` "✗ Not started; .btr/.bto parsers unwritten", `:178` gap row) — but `byroredux/src/cell_loader/terrain_lod_btr.rs`, `object_lod.rs`, `terrain_lod.rs` ship. Commits `9384d4c2` (.btr distant terrain), `6ddcda30`/#1726 (`_far.nif` distant-object LOD), PR #1685. ROADMAP.md:321 documents live-verified Skyrim Tamriel (544 .btr / 30 synth, 0 errors). Only distance-based multi-band selection (8/16/32) + .btr normal maps remain.
2. **Physics Ragdoll** (`:126` "✗") — but `byroredux/src/ragdoll.rs` + the `ragdoll <id>` console command run a Bethesda ragdoll on Rapier (18-body Doc Mitchell verified, ROADMAP.md:135-139; PR #1529). Oblivion/FO3/FNV/Skyrim converged; FO4+ blocked on `BhkSystemBinary` only.

## Suggested Fix
- `:50` → `~ Partial | .btr (Skyrim+/FO4) + .bto + _far.nif (Oblivion/FO3/FNV) shipped; distance-based multi-band selection + .btr normal maps deferred`; `:178` → drop/rescope to "multi-band LOD selection".
- `:126` → `~ Classic constraint chain (Oblivion/FO3/FNV/Skyrim) on Rapier multibody; FO4+ blocked on BhkSystemBinary`. (Leave the :124-125 general dynamic-body `✗` rows — not ragdoll work.)

## Completeness Checks
- [ ] **SIBLING**: no other feature-matrix row lags a shipped milestone (M45/M47.2 already fixed under #1699/#1703 — verified held)
