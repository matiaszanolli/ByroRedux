# TD2-2026-10-25-09: `block_bound` (terrain LOD) and `world_bound` (BTR) are the same function

Labels: low, terrain-exterior, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6368
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1180-1198` (`block_bound(vertices, holes)`), `byroredux/src/cell_loader/terrain_lod_btr.rs:500-514` (`world_bound(vertices)`, 9384d4c2b 2026-06-19)
- **Effort**: trivial
- **Fix**: move one `vertex_bound(vertices, skip: impl Fn(usize)->bool) -> WorldBound` into `byroredux/src/cell_loader/lod_support.rs` (already the shared home both LOD modules import from); `terrain_lod` passes the hole mask, `terrain_lod_btr` passes `|_| false` (its test `:787` pins the empty case).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-09`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
