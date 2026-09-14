# #4365 — TD4-004: Stale rows in `_audit-common.md`'s fenced Project Layout

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4365

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/_audit-common.md:29,63,74,80,83,85,89,99,105` · **Status**: NEW (`:82` is #4121; `:101` is #4114) · **Effort**: small · **Kind**: doc-rot
- **Finding**: *texture_registry.rs* (`:63`), *boot.rs* (`:74,80,85`) and *asset_provider/material.rs* (`:99,105`) no longer exist. The Scripting row omits `compatibility.rs`, `obscript.rs`, `obscript_runtime.rs`, `papyrus_provider/` and `combat.rs` (~3.4k LOC). The Systems row omits `combat_ai.rs` and `navmesh_path.rs`. The binary misc list omits `workspace_hygiene_tests.rs`.
- **Suggested Fix**: Rewrite the rows to their directory shapes and add the missing modules.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
