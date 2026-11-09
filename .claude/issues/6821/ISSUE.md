# NIFAL-D9-2026-11-08-01: `nifal.md` §3 names call sites and file homes that moved

GitHub: #6821 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,documentation,doc-rot

- **Severity**: LOW (doc rot)
- **Dimension**: Completeness / cross-cutting
- **Tier Violated**: harness-gap
- **Location**: `docs/engine/nifal.md` lines 963, 1019, 1031, 1098
- **Status**: NEW (verified by reading each line)
- **Description**: Line 963 still says `pack_imported_material_flags` lives in `cell_loader.rs`, but #6712 moved it to `material_flags.rs`. Lines 1019 and 1098 name `scene/nif_loader.rs` and `cell_loader/spawn/mesh_instance.rs` as the lowering and Phase-2 resolver call sites. Since #6690 the call sites are `scene/mesh_entity.rs` (`:149`, and `:351-367` for the resolvers). Line 1031 says the terrain Phase-2 caller is only `terrain_lod_btr.rs`, but `terrain_lod.rs:1347` also calls `resolve_msn_z_source`.
- **Suggested fix**: Update the four references. `.claude/commands/_audit-validate.sh` covers path tokens in skills, not these docs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
