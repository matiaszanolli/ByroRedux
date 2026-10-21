# #6277: TD3-2026-10-21-03: stale doc from the 10-20/21 wave (deleted `push_bytes`, a backticked memory slug)

**Labels**: low,renderer,terrain-exterior,tech-debt,documentation,doc-rot

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 3 · **Location**: `crates/renderer/src/vulkan/groundcover_bench.rs:1623-1626` ("`push_bytes` hands the driver … the unsafe slice would start reading uninitialised padding"); `docs/engine/skyal.md:369` (`` `environment_temperature_grading` ``)

- **Severity**: LOW
- **Dimension**: 3
- **Location**: `crates/renderer/src/vulkan/groundcover_bench.rs:1623-1626` ("`push_bytes` hands the driver … the unsafe slice would start reading uninitialised padding"); `docs/engine/skyal.md:369` (`` `environment_temperature_grading` ``)
- **Status**: NEW
- **Age**: `02b828fff` (2026-10-21, #6033) deleted `push_bytes` and routed the push through `byte_view` with a `NoUninit` impl; `b6d6f8320` (2026-10-20) added the slug reference
- **Effort**: trivial
- **Description**: The test doc names a function that no longer exists and an unsafe slice that `NoUninit` now replaces. The padding invariant is now enforced by the `NoUninit` impl's SAFETY comment at `:174`. The skyal.md reference backticks a memory slug, which the Path-Reference Convention says must be italic, never backticked (`_audit-common.md`). The validator surfaces it as a missing symbol.
- **Suggested Fix**: Re-word the test doc to "`byte_view` hands the driver `size_of::<BenchPush>()` bytes; the `NoUninit` impl relies on no padding". Italicise the slug.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
