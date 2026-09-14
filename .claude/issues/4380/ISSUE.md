# #4380 — TD8-001: Debug-server whole-component `set_json` accessor is dead — stub closure, field and type alias have no invoker

**Labels**: low, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4380

- **Severity**: LOW · **Dimension**: 8
- **Location**: `crates/debug-server/src/registration.rs:29`, `crates/debug-protocol/src/registry.rs:14,37` · **Status**: NEW (Dim 6 hand-off, confirmed) · **Age**: `cc6aea877` (2026-04-13) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Every registered component gets a closure that always returns "whole-component replacement not yet supported". `DebugRequest` has no variant that calls it; only `SetField` → `set_field` is live. The field's doc describes behaviour that never existed.
- **Suggested Fix**: Delete the field, the `SetJsonFn` alias and the closure. Add them back with a request variant if the feature is ever wanted.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
