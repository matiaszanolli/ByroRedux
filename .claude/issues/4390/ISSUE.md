# #4390 — TD9-002: The five `recon`-feature unit tests in `crates/spt` run in no CI lane

**Labels**: low, speedtree, tech-debt, bug, test-gap
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4390

- **Severity**: LOW · **Dimension**: 9
- **Location**: `crates/spt/src/recon/mod.rs:199-272`, `crates/spt/src/lib.rs:53`, `.github/workflows/ci.yml:151-152` · **Status**: NEW (sibling of closed #3894) · **Age**: `8b77cb7c2` (05-09) · **Effort**: trivial · **Kind**: test-gap
- **Finding**: #3894 added `cargo check --features recon --examples`, which never builds `cfg(test)`; `cargo test --workspace` leaves `recon` off. These are value-asserting tests.
- **Suggested Fix**: Add `cargo test -p byroredux-spt --features recon --lib`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
