# #4385 — TD8-006: `NiNode` "backward compat" inherent accessors duplicate its trait impls one-for-one, with an April removal promise

**Labels**: low, nif-parser, nif, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4385

- **Severity**: LOW · **Dimension**: 8
- **Location**: `crates/nif/src/blocks/node.rs:23-51` (dup of `:70-98`) · **Status**: NEW · **Age**: `7e9274bd1` (2026-04-03) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Seven inherent methods mirror `HasObjectNET` / `HasAVObject`. Inherent methods shadow trait methods, so the traits never become the access path. `NiTriShape` and the light blocks already implement only the traits. A dangling comment has no field after it.
- **Suggested Fix**: Delete the inherent block and the comments; add the trait imports at whatever call sites the compiler flags.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
