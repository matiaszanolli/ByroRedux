# TD3-2026-09-29-01: CLAUDE.md's Workspace Structure names two functions deleted months ago and a type that never existed

**Labels**: low,tech-debt,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `CLAUDE.md:125` (`resources.rs  build_blas_for_mesh, register_ui_quad, …`)
  - `CLAUDE.md:128` (`mod.rs  … new()/destroy()/debug_assert_scratch_aligned()`)
  - `CLAUDE.md:66` (`archive.rs  GameArchive — wraps BSA … or BA2`)
  - `crates/facegen/src/eval.rs:116`
- **Evidence**:
  - `build_blas_for_mesh` was deleted by `999478ef4` (2026-08-15, #2914, "delete the dead single-shot
    BLAS path"). `resources.rs:323` now says "never-called single-shot `build_blas_for_mesh`".
  - `debug_assert_scratch_aligned` was deleted by `d6d0516f9` (2026-06-01).
  - `GameArchive` appears in no `.rs` file in the whole history; the type is
    `asset_provider::archive::Archive`.
  - `facegen/src/eval.rs:116` still says `out` "reaches the vertex SSBO and `build_blas_for_mesh`".
  - `_audit-validate.sh` covers skills and docs/engine only, not CLAUDE.md.
- **Impact**: CLAUDE.md is loaded into every agent session, and `_audit-common.md` names it as the
  authoritative tree.
- **Suggested Fix**:
  - Replace the three names with `build_blas_batched`, the round-up-at-use note, and `Archive`.
  - Fix the facegen comment.
  - Point the validator's symbol advisory at CLAUDE.md and AGENTS.md too.

**Validated at HEAD 9fcfdc3fc**: `CLAUDE.md:66/125/128` and `crates/facegen/src/eval.rs:116` still name `GameArchive` / `build_blas_for_mesh` / `debug_assert_scratch_aligned`; `git grep` finds no definition of any of the three in `*.rs`; the archive type is `pub(crate) struct Archive` (`byroredux/src/asset_provider/archive.rs:6`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
