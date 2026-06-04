# Investigation — #1351 D6-04 FO4 PreCombined CSG reader

**Domain:** legacy-compat / performance · **Status: RESOLVED by M49 (no code change)**

## Premise check (2026-06-04): STALE
The issue (from AUDIT_FO4_2026-05-30) claims the CSG companion reader is absent
and precombined geometry never renders. M49 landed AFTER the audit
(Session 45, 2026-06-02) and resolves exactly this:
- `crates/bsa/src/csg.rs` exists — `CsgArchive` reads `Fallout4 - Geometry.csg`
  (zlib PSG keyed by filename hash + offset per `BSPackedGeomObject`).
- `byroredux/src/cell_loader/precombined.rs` opens it (`open_geometry_csg`),
  decodes via `build_precombine_meshes` / `decode_shared_geom_object`, and spawns
  precombined entities (module doc: "Current state (M49 — complete)").
- ROADMAP.md:272 — "~~M49~~ ... Closed (Session 45, 2026-06-02) ... Closes #1351
  / #1188 Stage A"; ROADMAP:583 "M49 ... closes #1351".
- Bench evidence: MedTekResearch01 now spawns 21414 entities (R6a-stale-14,
  `1c26bc25`) — "entirely from M49 CSG precombined geometry", i.e. pc_spawned > 0
  where it was 0 before.

The "no ROADMAP milestone" sub-claim is also stale: M49 is a full ROADMAP entry.

## SIBLING items — tracked, still open (NOT lost)
`_precomb.nif` Havok collision and `.uvd` occlusion volumes are documented as
"Deferred sub-items (M49 Stage B)" in both `precombined.rs` and ROADMAP:272.
They are separate future work, not part of this (Stage A) issue.

## Decision
No code change. Closed as resolved-by-M49 / #1188 Stage A. The audit predated the
landing — a stale-premise close, per audit-finding-hygiene.
