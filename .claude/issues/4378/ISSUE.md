# #4378 — TD7-005: Blade shaping literals are bare inline in the ground-cover blade shaders and absent from the uncited-values list

**Labels**: low, shaders, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4378

- **Severity**: LOW · **Dimension**: 7
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:215,243,245,254,340`, `crates/renderer/shaders/groundcover_blade.frag:75` · **Status**: NEW · **Age**: `637b652647` (09-06) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The stiffness factor `0.7` coincides with `GROUNDCOVER_WIND_MAX_BEND = 0.7` two lines later, which invites a wrong "unification".
- **Suggested Fix**: Name them in `shader_constants_data.rs` and list them as uncited in `docs/engine/exal-groundcover.md` §12.12.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
