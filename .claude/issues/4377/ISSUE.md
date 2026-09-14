# #4377 — TD7-003: "blades per chunk = workgroup × candidates per thread" is a prose invariant over three hand-edited constants

**Labels**: low, shaders, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4377

- **Severity**: LOW · **Dimension**: 7
- **Location**: `crates/renderer/src/shader_constants_data.rs:259,275,286-289` · **Status**: NEW · **Age**: `673b21458` (09-13, edited in lockstep by hand) · **Effort**: trivial · **Kind**: tech-debt
- **Suggested Fix**: `pub const GROUNDCOVER_MAX_BLADES_PER_CHUNK: u32 = GROUNDCOVER_SCATTER_WORKGROUP * GROUNDCOVER_CANDIDATES_PER_THREAD;`

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
