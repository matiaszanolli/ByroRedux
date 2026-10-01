# TD1-2026-09-29-03: `cornell.rs` crossed to 2494 (+580) and hosts five unrelated harness scenes

**Labels**: low,renderer,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `byroredux/src/cornell.rs`
- **Age**: `b9e961eeb` (+392, titled as lighting docs, 09-23); `ff1b48d7c` (+189 shared-mesh oracle, 09-23)
- **Description**: the file holds five scenes, each on its own lines:
  - classic Cornell (`setup_cornell_scene` :1620)
  - the RT oracle ladder (`setup_cornell_oracle_scene` :715, a 363-line function)
  - glass dragon (:1229–1442)
  - combustion lab (:1443–1619)
  - godray lab (:94–287)

  It also holds the shared material/spawn builders and `MeshBuilder`.
- **Suggested Fix**: `cornell/{mod.rs (mode-flag parsing), oracle.rs, glass_dragon.rs, combustion_lab.rs,
  godray_lab.rs, builders.rs}`. Grep `include_str!("cornell.rs")` first.

**Validated at HEAD 9fcfdc3fc**: `prod_loc byroredux/src/cornell.rs` = 2494.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

---

## Solution

**Fixed**: cornell.rs (3096 lines) split into cornell/{oracle, glass_dragon,
combustion_lab, godray_lab, builders}.rs with the classic box, SDK studio
room and the family re-exports in the root — 1243/838/476/254/249/82. The
re-export table keeps every crate::cornell:: path scene.rs and
studio_host.rs use unchanged; no include_str! scanner reads cornell.rs.
Commit: `Fix #5090` (8f38df6df).

## Verification

- bin crate: 2576/2576 green; clippy clean on the cornell family
