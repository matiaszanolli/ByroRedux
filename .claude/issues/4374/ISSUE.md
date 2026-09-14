# #4374 — TD6-007: `DebugRequest::ListLoadedAssets` always errors "not yet implemented", no client sends it, and the debug-CLI doc presents it as working

**Labels**: low, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4374

- **Severity**: LOW · **Dimension**: 6
- **Location**: `crates/debug-server/src/evaluator.rs:133-140`, `crates/debug-protocol/src/lib.rs:118-134`, `docs/engine/debug-cli.md:126,731-734` · **Status**: NEW · **Effort**: trivial (delete) / small (implement) · **Kind**: tech-debt
- **Suggested Fix**: Delete the variant and `AssetKind` (`tex.loaded` / `mesh.*` cover the use case) unless the TUI plans a consumer.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
