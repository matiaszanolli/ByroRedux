# TD3-2026-10-27-02: The geometry-pool tier rewrite left the reference docs describing the deleted two-tier scheme

Labels: low, renderer, documentation, doc-rot, tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD3-2026-10-27-02: The geometry-pool tier rewrite left the reference docs describing the deleted two-tier scheme

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD3-2026-10-27-02` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `docs/engine/memory-budget.md` ("Admission tiers" paragraph, ~:897-910); `crates/renderer/src/mesh.rs` (test doc at ~:2324 naming `PROP_POOL_RESERVE_PERCENT`); `docs/engine/terrain-displacement.md:36,139`.

## Description
22d6e1a80 (admission tiers) and bb97be77c (7M / 28M caps) landed on 10-27. memory-budget.md still describes the deleted scheme: "`GeometryAdmissionClass`, `PROP_POOL_RESERVE_PERCENT` = 5", props at 95%, and "terrain, water, terrain LOD **and actors** are essential ... may use the full cap". The code (`mesh.rs` ~:37-127, ~:997-1031) now has three tiers (Essential, Actor, Prop), `DEFAULT_GROUND_RESERVE_PERCENT` = 3 and `DEFAULT_ACTOR_RESERVE_PERCENT` = 2, actors capped below the ground reserve, a third latch `actor_geometry_admission_open`, and a reserve re-sized per exterior radius through `MeshRegistry::set_pool_reserve`. `PROP_POOL_RESERVE_PERCENT` no longer exists as a symbol: its only mention is the `mesh.rs:2324` test doc, which keeps the name in `_audit-validate.sh`'s symbol corpus and is why that gate does not flag memory-budget.md (a self-satisfying corpus). terrain-displacement.md still says "cap 4 M verts = 416 MB, 16 M indices" and "2.5 % of the vertex pool"; at the 7 M cap that is ~1.4%.

## Evidence
`grep -rn PROP_POOL_RESERVE_PERCENT crates docs` -> only `memory-budget.md:897` and `mesh.rs:2324` (a doc comment). `mesh.rs:41-42` defines `DEFAULT_GROUND_RESERVE_PERCENT = 3` / `DEFAULT_ACTOR_RESERVE_PERCENT = 2`. `terrain-displacement.md:36` and `:139` quote the 4 M cap.

## Impact
memory-budget.md is one of the authoritative docs audits are told to trust; it now documents a scheme that does not exist.

## Related
SPT-2026-10-27-D4-01 (code defect in the same tier work), TD8-2026-10-27-01.

## Suggested Fix
Rewrite the tier paragraph from `GeometryAdmissionClass`'s rustdoc, fix the `mesh.rs:2324` test doc to name the real reserve constants, and refresh the TSD-4 cap figures in terrain-displacement.md. Consider making `_audit-validate.sh` exclude doc-comment-only mentions from its symbol corpus.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites

